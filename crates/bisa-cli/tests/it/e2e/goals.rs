//! A goal in each of its three modes, and what a person does to one,
//! through the binary: an **auto** goal designed, adopted and run with
//! nobody asked; a **manual** goal that wakes nobody and runs the workflow
//! its person points it at — a second run queued behind the first, stopped,
//! started again, closed, put away and taken back out. The **guided** goal is
//! `from_capture_to_done`'s, and a crash in a step `crash_in_a_step`'s.

use super::sealed::{Sealed, AGENT_HARNESS};
use serde_json::{json, Value};

/// A designer that proposes a workflow of one step, and a worker that
/// yields it.
fn designer_and_worker() -> Value {
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

/// A question to a person, and nothing else: a run that waits.
fn asks_first() -> Value {
    json!({
        "name": "Ask first",
        "inputs": [{ "name": "about", "label": "About", "kind": "text", "required": true }],
        "steps": [{
            "id": "ask",
            "name": "Which one?",
            "kind": "human",
            "prompt": "Which one, for {inputs.about}?",
            "options": [
                { "id": "this", "label": "This one" },
                { "id": "that", "label": "That one" },
            ],
        }],
    })
}

/// What the goal's row of the inbox asks, by subject.
fn asked(ws: &Sealed, goal: &str) -> Vec<String> {
    ws.json(&["inbox"])["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row["key"] == goal)
        .flat_map(|row| row["needs_action"].as_array().cloned().unwrap_or_default())
        .filter_map(|ask| ask["subject"].as_str().map(str::to_string))
        .collect()
}

/// The goal's runs, oldest first: `(id, status)`.
fn runs_of(ws: &Sealed, goal: &str) -> Vec<(String, String)> {
    let mut runs: Vec<(String, String)> = ws.json(&["runs", goal])["runs"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|run| {
            (
                run["id"].as_str().unwrap_or_default().to_string(),
                run["status"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    runs.sort();
    runs
}

#[test]
fn an_auto_goal_is_designed_adopted_and_run_with_nobody_asked() {
    let mut ws = Sealed::with_script(&designer_and_worker());
    ws.start();

    // Auto is what a goal is when nobody says otherwise.
    let captured = ws.json(&["new", "what tea is"]);
    assert_eq!(captured["mode"], "auto", "{captured}");
    let goal = captured["goal"].as_str().expect("the goal").to_string();

    let status = ws.until("the goal to be done by itself", || {
        let status = ws.json(&["status", &goal]);
        assert_eq!(
            asked(&ws, &goal),
            Vec::<String>::new(),
            "an auto goal asks nobody on its way"
        );
        (status["status"] == "done").then_some(status)
    });
    assert_eq!(status["run"]["outcome"], "done");
    assert_eq!(
        status["run"]["steps"]["write"]["output"],
        json!({ "line": "tea is leaves and patience" })
    );
    // The design is the goal's own, and the run says it ran unattended.
    let design = status["run"]["workflow"]["id"]
        .as_str()
        .expect("the design");
    let shown = ws.json(&["workflow", "show", design]);
    assert_eq!(
        shown["workflow"]["origin"],
        json!({ "origin": "goal", "goal": goal }),
        "{shown}"
    );
    let told = ws
        .recorded("prompt")
        .into_iter()
        .find(|p| p["scope"] == "work_item")
        .expect("the worker's prompt");
    assert!(
        told["text"]
            .as_str()
            .is_some_and(|t| t.contains("This goal runs unattended")),
        "the worker is told nobody is watching: {told}"
    );
    // What was decided was decided by the platform, and is written down.
    let said = ws.json(&["log", &goal])["events"].to_string();
    assert!(said.contains("\"type\":\"guidance\""), "{said}");
    assert_eq!(ws.recorded("started").len(), 2, "a designer and a worker");
    ws.stop();
}

#[test]
fn a_manual_goal_wakes_nobody_and_is_run_stopped_started_again_and_closed_by_its_person() {
    let mut ws = Sealed::with_script(&designer_and_worker());
    let file = ws.file("ask.json", &asks_first().to_string());
    let workflow = ws.json(&["workflow", "new", "--from", &file.to_string_lossy()])["workflow"]
        ["id"]
        .as_str()
        .expect("its id")
        .to_string();
    ws.start();

    // Captured, and nothing moves: no workflow, no designer.
    let captured = ws.json(&["new", "pick a shelf", "--mode", "manual"]);
    let goal = captured["goal"].as_str().expect("the goal").to_string();
    assert_eq!(captured["mode"], "manual");
    assert_eq!(ws.json(&["status", &goal])["status"], "draft");
    let nothing = ws.bisa(&["run", &goal]);
    assert!(
        nothing.status.success(),
        "a goal with no workflow is said, not failed: {}",
        String::from_utf8_lossy(&nothing.stderr)
    );
    assert_eq!(runs_of(&ws, &goal), vec![]);

    // Its person points it at a workflow; a missing input is refused by name.
    ws.ok(&["workflow", "use", &goal, &workflow]);
    let refused = ws.bisa(&["run", &goal]);
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("about"),
        "the refusal names the input: {}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(runs_of(&ws, &goal), vec![], "and no run was made");

    // A run, which asks; a second, which waits its turn behind it.
    let first = ws.json(&["run", &goal, "--input", "about=the hall"]);
    assert_eq!(first["status"], "waiting", "{first}");
    let first = first["run"]["id"].as_str().expect("the run").to_string();
    assert_eq!(asked(&ws, &goal), vec![format!("step:{first}/ask")]);
    ws.ok(&["run", &goal, "--new", "--input", "about=the study"]);
    let both = runs_of(&ws, &goal);
    assert_eq!(both.len(), 2, "{both:?}");
    assert_eq!(both[0], (first.clone(), "waiting".to_string()));
    assert_eq!(both[1].1, "queued");
    let second = both[1].0.clone();

    // Stopped: the live run is cancelled, the queued one withdrawn, and the
    // goal is open — a draft again, asking nothing.
    let stopped = ws.json(&["stop", &goal, "--rationale", "not today"]);
    assert_eq!(stopped["stopped"], first.as_str(), "{stopped}");
    assert_eq!(stopped["withdrawn"], json!([second]), "{stopped}");
    assert_eq!(ws.json(&["status", &goal])["status"], "draft");
    assert_eq!(asked(&ws, &goal), Vec::<String>::new());
    let after: Vec<String> = runs_of(&ws, &goal).into_iter().map(|(_, s)| s).collect();
    assert_eq!(after, vec!["cancelled", "cancelled"]);

    // Started again: a new run of what the goal last ran, with what that
    // run was given — the run that started, never one that only waited.
    ws.ok(&["restart", &goal]);
    let third = ws.until("the new run to ask", || {
        let status = ws.json(&["status", &goal]);
        (status["status"] == "waiting").then_some(status)
    });
    let third_id = third["run"]["id"].as_str().expect("the run").to_string();
    assert!(third_id != first && third_id != second);
    assert_eq!(third["run"]["inputs"]["about"], "the hall");
    let answered = ws.json(&["step", "answer", &goal, "ask", "-o", "this"]);
    assert_eq!(answered["status"], "done", "{answered}");

    // Closed for good, with the person's word on why; put away, and taken
    // back out — still closed.
    ws.ok(&["close", &goal, "--rationale", "the shelf is up"]);
    let closed = ws.json(&["status", &goal]);
    assert_eq!(closed["status"], "closed", "{closed}");
    let why = &closed["goal"]["closed"];
    assert_eq!(why["reason"], "abandoned", "{closed}");
    assert_eq!(
        why["rationale"], "the shelf is up",
        "its person's word is kept"
    );
    // Another, closed in favour of the one that takes its place: it says which.
    let other = ws.json(&["new", "pick a bigger shelf", "--mode", "manual"])["goal"]
        .as_str()
        .expect("the goal")
        .to_string();
    let heir = ws.json(&["new", "build the shelf", "--mode", "manual"])["goal"]
        .as_str()
        .expect("the goal")
        .to_string();
    ws.ok(&["close", &other, "--superseded-by", &heir]);
    let replaced = ws.json(&["status", &other])["goal"]["closed"].clone();
    assert_eq!(replaced["reason"], "superseded", "{replaced}");
    assert_eq!(replaced["by"], heir.as_str(), "{replaced}");
    let refused = ws.bisa(&["run", &goal, "--input", "about=again"]);
    assert!(!refused.status.success(), "a closed goal runs nothing");
    let listed = |ws: &Sealed| -> bool {
        ws.json(&["search"])["matches"]
            .as_array()
            .is_some_and(|all| all.iter().any(|g| g["goal"] == goal.as_str()))
    };
    assert!(listed(&ws), "a closed goal is still in the list");
    ws.ok(&["archive", "goal", &goal]);
    assert!(!listed(&ws), "a goal put away is out of it");
    ws.ok(&["archive", "goal", &goal, "--undo"]);
    assert!(listed(&ws), "and back in it when it is taken out");
    assert_eq!(ws.json(&["status", &goal])["status"], "closed");

    // Nobody was woken in all of that: a manual goal has no designer, and
    // its workflow no agent step.
    assert_eq!(ws.recorded("started"), Vec::<Value>::new());
    ws.stop();
}

/// A worker that holds its first turn — at work, saying nothing more.
fn holding_worker() -> serde_json::Value {
    serde_json::json!({
        "turns": [
            { "scope": "work_item", "say": ["Working on it."], "hold": true },
        ],
    })
}

fn one_agent_step() -> serde_json::Value {
    serde_json::json!({
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

/// Whether a process with this pid is still there, by the one call every
/// shell has — the test's own stand-in for a `ps` of the agent.
fn process_exists(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// A goal stopped while its worker holds the turn: the stop answers once
/// the worker's process is gone — and says it ended one session, terminated
/// nothing and left nothing live — and the roster lists no live row.
#[test]
fn a_goal_stopped_while_its_worker_holds_the_turn_ends_the_worker() {
    let mut ws = Sealed::with_script(&holding_worker());
    let definition = ws.file("build.json", &one_agent_step().to_string());
    let workflow = ws.json(&["workflow", "new", "--from", &definition.to_string_lossy()])
        ["workflow"]["id"]
        .as_str()
        .expect("the workflow")
        .to_string();
    ws.start();
    let goal = ws.json(&["new", "a thing to build", "--workflow", &workflow])["goal"]
        .as_str()
        .expect("the goal")
        .to_string();
    // The worker is in the middle of its turn.
    ws.until("the worker to be at work", || {
        (!ws.recorded("holding").is_empty()).then_some(())
    });
    let pid = ws.recorded("holding")[0]["pid"]
        .as_u64()
        .and_then(|p| u32::try_from(p).ok())
        .expect("the agent says its pid");
    assert!(
        process_exists(pid),
        "the worker's process is there while it holds"
    );

    let stopped = ws.json(&["stop", &goal]);

    assert_eq!(stopped["ended"]["sessions"], 1, "{stopped}");
    assert_eq!(
        stopped["ended"]["terminated"], 0,
        "a harness that stops when told is never terminated: {stopped}"
    );
    assert_eq!(stopped["ended"]["still_live"], 0, "{stopped}");
    assert!(
        !process_exists(pid),
        "the worker's process is gone when the stop answers"
    );
    assert_eq!(ws.json(&["status", &goal])["status"], "draft");
    let runs: Vec<String> = runs_of(&ws, &goal).into_iter().map(|(_, s)| s).collect();
    assert_eq!(runs, vec!["cancelled"]);
    ws.stop();
}
