//! A goal's runs through the binary, at their edges: a run asked of a goal
//! that has no workflow and designs one; a parent stopped with the goal it
//! spawned; a run followed to its question with no terminal to decide at;
//! a decision asked of a question with no answer, offline.

use super::sealed::{Sealed, AGENT_HARNESS};
use serde_json::{json, Value};

/// A worker that writes its line and yields it.
fn worker() -> Value {
    json!({
        "turns": [{
            "scope": "work_item",
            "tools": [{
                "name": "yield_result",
                "arguments": { "output": { "line": "{{prompt}}" } },
            }],
            "say": ["Written."],
        }],
    })
}

/// A workflow held until a person releases it.
fn held() -> Value {
    json!({
        "name": "Held",
        "description": "waits for a person",
        "steps": [{ "id": "hold", "name": "Hold", "kind": "wait", "until": { "until": "release" } }]
    })
}

/// A parent that spawns a held child and waits for it.
fn spawning(child: &str) -> Value {
    json!({
        "name": "Spawns",
        "description": "a child held open",
        "steps": [{
            "id": "child", "name": "Child", "kind": "spawn",
            "statement_template": "the child's part", "workflow": child, "wait": true
        }]
    })
}

/// An agent writes a line about the subject; a person approves it.
fn draft_and_approve() -> Value {
    json!({
        "name": "Draft and approve",
        "description": "a line about a subject, approved by a person",
        "inputs": [{ "name": "subject", "label": "Subject", "kind": "text", "required": true }],
        "steps": [
            {
                "id": "draft", "name": "Draft", "kind": "agent",
                "instructions": "One line about {inputs.subject}.",
                "harness": [AGENT_HARNESS], "then": ["ship"],
            },
            { "id": "ship", "name": "Ship it?", "kind": "approval", "prompt": "Ship the line about {inputs.subject}?" },
        ],
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

fn recorded(ws: &Sealed, name: &str, definition: &Value) -> String {
    let file = ws.file(name, &definition.to_string());
    let made = ws.json(&["workflow", "new", "--from", &file.to_string_lossy()]);
    made["workflow"]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("the workflow's id: {made}"))
        .to_string()
}

fn goal_of(made: &Value) -> String {
    made["goal"]
        .as_str()
        .unwrap_or_else(|| panic!("the goal: {made}"))
        .to_string()
}

/// Everything a plain (not `--json`) verb said, on either stream.
fn said_by(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn a_goals_runs_queue_stop_and_are_followed_through_the_binary() {
    let mut ws = Sealed::with_script(&worker());
    let held = recorded(&ws, "held.json", &held());
    let spawner = recorded(&ws, "spawner.json", &spawning(&held));
    let drafting = recorded(&ws, "draft.json", &draft_and_approve());
    let asking = recorded(&ws, "asks.json", &asks());
    ws.start();

    let designed = goal_of(&ws.json(&["new", "needs a design", "--mode", "guided"]));

    // A parent stopped takes the goal it spawned with it, and says so.
    let parent = goal_of(&ws.json(&["new", "spawns a child", "--workflow", &spawner]));
    ws.until("the child to be spawned", || {
        let goals = ws.json(&["search"])["matches"].as_array()?.len();
        (goals >= 3).then_some(())
    });
    let stopped = ws.ok(&["stop", &parent]);
    assert!(
        said_by(&stopped).contains("1 goal(s) spawned by it were stopped with it"),
        "{}",
        said_by(&stopped)
    );

    // A run followed from the verb, with no terminal to decide at: the
    // worker writes, the approval opens, and the verb says where to decide.
    let shipping = goal_of(&ws.json(&["new", "a line to ship", "--mode", "manual"]));
    ws.ok(&["workflow", "use", &shipping, &drafting]);
    let followed = ws.ok(&["run", &shipping, "--watch", "--input", "subject=tea"]);
    let said = said_by(&followed);
    assert!(said.contains("started run"), "{said}");
    assert!(said.contains("Ship it? (approval)"), "{said}");
    assert!(said.contains(&shipping), "{said}");
    assert_eq!(ws.json(&["status", &shipping])["status"], "waiting");

    // A goal whose run asks a question.
    let asked = goal_of(&ws.json(&["new", "a question", "--workflow", &asking]));
    ws.until("the question", || {
        (ws.json(&["status", &asked])["status"] == "waiting").then_some(())
    });

    // Offline: a decision on a question that needs an answer, given none,
    // is refused in words and the question stands; and a goal with no
    // workflow, in a mode where the Workflow Agent designs one, has nothing
    // to run — the verb says so and starts nothing.
    ws.stop();
    let refused = ws.refused(&["approve", &asked]);
    assert!(refused.contains("this carried nothing"), "{refused}");
    assert_eq!(ws.json(&["status", &asked])["status"], "waiting");
    let nothing = ws.ok(&["run", &designed]);
    assert!(
        said_by(&nothing).contains("the Workflow Agent designs one"),
        "{}",
        said_by(&nothing)
    );
    assert_eq!(
        ws.json(&["run", &designed])["started"],
        json!(false),
        "and as JSON, nothing started"
    );
}
