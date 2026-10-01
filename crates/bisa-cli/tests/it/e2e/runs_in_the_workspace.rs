//! A workflow of the library run with no goal behind it, through the binary:
//! two runs of it going at once, each answered by its own id; one stopped
//! and started again by itself; the daemon gone and back while a run waits,
//! and while one works; a workflow that reads its goal refused by name.

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

/// A worker still at work the first time it is prompted, that yields the
/// time after.
fn slow_worker() -> Value {
    json!({
        "turns": [
            { "scope": "work_item", "times": 1, "say": ["Working on it."], "hold": true },
            {
                "scope": "work_item",
                "tools": [{
                    "name": "yield_result",
                    "arguments": { "output": { "line": "written after the restart" } },
                }],
                "say": ["Written."],
            },
        ],
    })
}

/// An agent writes a line about what the run was given; a person keeps it.
fn draft_and_confirm() -> Value {
    json!({
        "name": "Draft and confirm",
        "description": "a line about a subject, kept by a person",
        "inputs": [{ "name": "subject", "label": "Subject", "kind": "text", "required": true }],
        "steps": [
            {
                "id": "draft",
                "name": "Draft",
                "kind": "agent",
                "instructions": "One line about {inputs.subject}.",
                "harness": [AGENT_HARNESS],
                "then": ["confirm"],
            },
            {
                "id": "confirm",
                "name": "Keep it?",
                "kind": "human",
                "prompt": "Keep the line about {inputs.subject}?",
                "options": [
                    { "id": "keep", "label": "Keep it" },
                    { "id": "drop", "label": "Drop it" },
                ],
            },
        ],
    })
}

/// A workflow recorded in the library from a definition; its id.
fn recorded(ws: &Sealed, name: &str, definition: &Value) -> String {
    let file = ws.file(name, &definition.to_string());
    let made = ws.json(&["workflow", "new", "--from", &file.to_string_lossy()]);
    made["workflow"]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("the workflow's id: {made}"))
        .to_string()
}

/// A run of the workspace begun by the verb, which follows it until a
/// person is needed; its id.
fn begun(ws: &Sealed, workflow: &str, subject: &str) -> String {
    let started = ws.json(&[
        "workflow",
        "run",
        workflow,
        "--input",
        &format!("subject={subject}"),
    ]);
    assert_eq!(started["scope"], "workspace", "{started}");
    assert_eq!(
        started["goal"],
        Value::Null,
        "no goal is behind it: {started}"
    );
    assert_eq!(
        started["status"], "waiting",
        "its person is asked: {started}"
    );
    started["run"]
        .as_str()
        .unwrap_or_else(|| panic!("the run's id: {started}"))
        .to_string()
}

/// What the workflow's row of the inbox asks, by subject.
fn asked(ws: &Sealed, workflow: &str) -> Vec<String> {
    let inbox = ws.json(&["inbox"]);
    let Some(row) = inbox["rows"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["key"] == workflow))
    else {
        return Vec::new();
    };
    assert_eq!(row["kind"], "workflow", "{row}");
    let mut subjects: Vec<String> = row["needs_action"]
        .as_array()
        .map(|asks| {
            asks.iter()
                .filter_map(|ask| ask["subject"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    subjects.sort();
    subjects
}

fn ask_of(run: &str) -> String {
    format!("step:{run}/confirm")
}

/// The run as its snapshot on disk says it: the signed event's content.
fn on_disk(ws: &Sealed, run: &str) -> Value {
    let path = ws
        .data()
        .join("workflows")
        .join("runs")
        .join(run)
        .join("state")
        .join(format!("33413-{run}.json"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("no snapshot at {}: {e}", path.display()));
    let event: Value = serde_json::from_str(&text).expect("a signed event");
    serde_json::from_str(event["content"].as_str().expect("its content")).expect("the run")
}

#[test]
fn a_workflow_runs_with_no_goal_several_at_once_each_by_itself() {
    let mut ws = Sealed::with_script(&worker());
    let workflow = recorded(&ws, "draft.json", &draft_and_confirm());
    ws.start();

    // Two runs of one workflow, both going: neither waits on the other.
    let tea = begun(&ws, &workflow, "tea");
    let coffee = begun(&ws, &workflow, "coffee");
    assert_ne!(tea, coffee);
    let listed = ws.json(&["workflow", "runs", &workflow]);
    let rows = listed["runs"].as_array().expect("the runs");
    assert_eq!(rows.len(), 2, "{listed}");
    for row in rows {
        assert_eq!(
            row["status"], "waiting",
            "started at once, never queued: {row}"
        );
    }
    assert_eq!(
        ws.json(&["search"])["matches"],
        json!([]),
        "and no goal was captured for them"
    );

    // Each worker was told what its own run was given, stood in its own
    // run's scratch folder, and was handed the platform's server for its
    // own work item.
    let prompts: Vec<String> = ws
        .recorded("prompt")
        .iter()
        .filter_map(|p| p["text"].as_str().map(str::to_string))
        .collect();
    for subject in ["tea", "coffee"] {
        assert!(
            prompts
                .iter()
                .any(|p| p.starts_with(&format!("One line about {subject}."))),
            "{subject}: {prompts:?}"
        );
    }
    let sessions = ws.recorded("session_new");
    assert_eq!(sessions.len(), 2, "a worker a run");
    for run in [&tea, &coffee] {
        let scratch = std::fs::canonicalize(
            ws.data()
                .join("workflows")
                .join("runs")
                .join(run)
                .join("scratch"),
        )
        .expect("the run's scratch folder");
        let session = sessions
            .iter()
            .find(|s| {
                s["cwd"]
                    .as_str()
                    .and_then(|cwd| std::fs::canonicalize(cwd).ok())
                    .as_ref()
                    == Some(&scratch)
            })
            .unwrap_or_else(|| panic!("no worker stood in {}: {sessions:?}", scratch.display()));
        assert_eq!(session["scope"], "work_item");
        let status = ws.json(&["status", run]);
        let item = status["run"]["steps"]["draft"]["work_item"]
            .as_str()
            .expect("the step's work item");
        let args = session["servers"][0]["args"].to_string();
        assert!(args.contains(item), "{args} names {item}");
    }

    // Both ask, on the workflow's row of the inbox: no goal holds them.
    let mut both = vec![ask_of(&tea), ask_of(&coffee)];
    both.sort();
    assert_eq!(asked(&ws, &workflow), both);

    // One is answered by its id, and is done; the other still asks.
    let answered = ws.json(&["step", "answer", &tea, "confirm", "-o", "keep"]);
    assert_eq!(answered["status"], "done", "{answered}");
    let done = ws.json(&["status", &tea]);
    assert_eq!(done["run"]["outcome"], "done");
    assert_eq!(
        done["run"]["steps"]["confirm"]["answer"]["selected"],
        json!(["keep"])
    );
    assert_eq!(asked(&ws, &workflow), vec![ask_of(&coffee)]);

    // The other is stopped by itself…
    let stopped = ws.json(&["workflow", "stop", &workflow, "--run", &coffee]);
    assert_eq!(stopped["runs"], json!([coffee]), "{stopped}");
    let after = ws.json(&["status", &coffee]);
    assert_eq!(after["run"]["cancelled"]["cause"], "stopped", "{after}");
    assert_eq!(
        asked(&ws, &workflow),
        Vec::<String>::new(),
        "a stopped run asks nothing"
    );
    assert_eq!(
        ws.json(&["status", &tea])["run"]["outcome"],
        "done",
        "and the run beside it is as it was"
    );

    // …and started again by itself: a new run, with what the old one was given.
    let restarted = ws.json(&["workflow", "restart", &workflow, "--run", &coffee]);
    let again = restarted["run"].as_str().expect("the new run").to_string();
    assert_ne!(again, coffee);
    assert_eq!(restarted["status"], "waiting", "{restarted}");
    let new = ws.json(&["status", &again]);
    assert_eq!(new["run"]["inputs"]["subject"], "coffee");
    assert_eq!(asked(&ws, &workflow), vec![ask_of(&again)]);

    // The daemon goes while the run waits on its person. The files say
    // where it stands, with no daemon to ask…
    ws.stop();
    let kept = on_disk(&ws, &again);
    assert_eq!(kept["scope"]["scope"], "workspace");
    assert_eq!(kept["steps"]["confirm"]["state"]["state"], "waiting");
    assert_eq!(ws.json(&["status", &again])["status"], "waiting");

    // …and when it is back the question is asked again, and answered.
    ws.start();
    let back = ws.until("the question to be asked again", || {
        let subjects = asked(&ws, &workflow);
        (!subjects.is_empty()).then_some(subjects)
    });
    assert_eq!(back, vec![ask_of(&again)]);
    let answered = ws.json(&["step", "answer", &again, "confirm", "-o", "drop"]);
    assert_eq!(answered["status"], "done", "{answered}");
    assert_eq!(on_disk(&ws, &again)["outcome"], "done");

    // Three runs were made in all, and every worker has ended.
    let listed = ws.json(&["workflow", "runs", &workflow]);
    assert_eq!(listed["runs"].as_array().map(Vec::len), Some(3), "{listed}");
    ws.until("every worker to end", || {
        (ws.recorded("ended").len() == ws.recorded("started").len()).then_some(())
    });
    ws.stop();
}

#[test]
fn a_run_of_the_workspace_cut_off_in_a_step_is_picked_up_when_the_daemon_starts_again() {
    let mut ws = Sealed::with_script(&slow_worker());
    let workflow = recorded(&ws, "draft.json", &draft_and_confirm());
    ws.start();

    // The verb follows its run; the worker is in the middle of its turn.
    let mut following = ws.begin(
        "following",
        &[
            "--json",
            "workflow",
            "run",
            &workflow,
            "--input",
            "subject=tea",
        ],
    );
    ws.until("the worker to be at work", || {
        (!ws.recorded("holding").is_empty()).then_some(())
    });
    let listed = ws.json(&["workflow", "runs", &workflow]);
    let run = listed["runs"][0]["id"]
        .as_str()
        .expect("the run")
        .to_string();
    let before = ws.json(&["status", &run]);
    assert_eq!(before["run"]["steps"]["draft"]["state"]["state"], "running");
    let item = before["run"]["steps"]["draft"]["work_item"]
        .as_str()
        .expect("the step's work item")
        .to_string();

    // Cut off. The worker goes with its daemon, and the verb that followed
    // ends by itself, saying it lost the node — never hanging on.
    ws.crash();
    ws.until("the worker to end with its daemon", || {
        (ws.recorded("ended").len() == ws.recorded("started").len()).then_some(())
    });
    let ended = ws.until("the following verb to end", || following.ended());
    assert!(!ended.success(), "a verb that lost its node says so");

    // Back over the same files: the same run, on the item it had.
    ws.start();
    let after = ws.until("the run to wait on its person after the restart", || {
        let status = ws.json(&["status", &run]);
        (status["status"] == "waiting").then_some(status)
    });
    let step = &after["run"]["steps"]["draft"];
    assert_eq!(step["state"]["state"], "done");
    assert_eq!(
        step["output"],
        json!({ "line": "written after the restart" })
    );
    assert_eq!(step["work_item"], item.as_str(), "the item it already had");
    assert_eq!(
        ws.json(&["workflow", "runs", &workflow])["runs"]
            .as_array()
            .map(Vec::len),
        Some(1),
        "the same run, not another"
    );
    // The run's own journal says what the restart found.
    let said = ws.json(&["log", &run])["events"].to_string();
    assert!(
        said.contains("a restart interrupted 1 running step"),
        "the interruption is written down on the run: {said}"
    );
    let answered = ws.json(&["step", "answer", &run, "confirm", "-o", "keep"]);
    assert_eq!(answered["status"], "done", "{answered}");
    ws.stop();
}

#[test]
fn a_workflow_that_reads_its_goal_is_refused_in_the_workspace_by_name() {
    let mut ws = Sealed::with_script(&worker());
    let reads_its_goal = json!({
        "name": "About the goal",
        "steps": [{
            "id": "ask",
            "name": "Ask",
            "kind": "human",
            "prompt": "Is this what you meant: {goal.statement}?",
        }],
    });
    let workflow = recorded(&ws, "goal.json", &reads_its_goal);
    ws.start();

    let refused = ws.bisa(&["--json", "workflow", "run", &workflow]);
    assert!(
        !refused.status.success(),
        "a run in the workspace has no goal"
    );
    let why = String::from_utf8_lossy(&refused.stderr).to_string();
    assert!(
        why.contains("step `ask` reads {goal.statement}") && why.contains("run it on a goal"),
        "the refusal names the step and what to do: {why}"
    );
    assert_eq!(
        ws.json(&["workflow", "runs", &workflow])["runs"],
        json!([]),
        "and no run was made"
    );

    // On a goal it runs.
    let captured = ws.json(&["new", "a shelf for the hall", "--workflow", &workflow]);
    let goal = captured["goal"].as_str().expect("the goal").to_string();
    let status = ws.until("the goal's run to ask", || {
        let status = ws.json(&["status", &goal]);
        (status["status"] == "waiting").then_some(status)
    });
    assert_eq!(
        status["run"]["scope"],
        json!({ "scope": "goal", "goal": goal })
    );
    ws.stop();
}
