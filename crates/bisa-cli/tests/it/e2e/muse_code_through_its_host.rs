//! Muse Code, through the binary: a harness that speaks the Muse Session
//! Protocol under an id of its own.
//!
//! No real CLI runs. The scripted agent is placed as `muse`, so the adapter
//! finds it by its own probe (`--version`, answered with a version), starts
//! it as a host with its own words — `serve`, trusted in the checkout, and
//! nothing that names a model, a level or a tool allowed unasked — and
//! drives it over the one protocol door. What the journey asserts is what
//! the host kept: the command line, the model list read before any session,
//! the session started in the checkout under `promptUnmatched` and handed
//! the platform's server, the turn carrying the effort held to the levels
//! the model's row lists — never how the code got there. Its terminal is a
//! plain terminal, and the platform writes nothing under `~/.config/muse`.

use super::a_harness_in_a_terminal::{a_port, a_project};
use super::sealed::Sealed;
use serde_json::{json, Value};

/// The binary's name, and the harness's id.
const PROGRAM: &str = "muse";
const HARNESS: &str = "muse-code";

/// The words the adapter starts the host with.
const HOST_WORDS: [&str; 2] = ["serve", "--trust-workspace"];

/// A host whose sign-in lists two models: `small`, the default, taking two
/// levels; `large` taking three. A worker on it yields.
fn lists_two_models() -> Value {
    json!({
        "models": ["small", "large"],
        "model_efforts": {
            "small": ["low", "medium"],
            "large": ["low", "high", "max"],
        },
        "usage": {
            "tier": "power",
            "window": { "usedPercent": 21, "windowDurationMins": 300, "resetsAtMs": 1788782400000u64 },
            "weekly": { "usedPercent": 4, "resetsAtMs": 1789128000000u64 },
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

/// A workspace with Muse Code installed, the two core agents on it, and
/// no daemon yet.
fn with_muse_installed() -> Sealed {
    let ws = Sealed::bare();
    ws.install_agent_as(PROGRAM, &lists_two_models());
    for agent in ["general-agent", "workflow-agent"] {
        ws.ok(&[
            "agent",
            "edit",
            agent,
            "--harness",
            HARNESS,
            "--model",
            "large",
        ]);
    }
    ws
}

/// An agent of the workspace on Muse Code, on the models given, best
/// first, asked to work at `effort`.
fn an_agent(ws: &Sealed, name: &str, models: &[&str], effort: &str) {
    let mut args = vec![
        "agent",
        "add",
        "--name",
        name,
        "--harness",
        HARNESS,
        "--effort",
        effort,
        "--prompt",
        "You write what you are asked to.",
    ];
    for model in models.iter().copied() {
        args.extend_from_slice(&["--model", model]);
    }
    ws.ok(&args);
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

/// Run the workflow to its end.
fn run_to_done(ws: &Sealed, workflow: &str) {
    let started = ws.json(&["workflow", "run", workflow]);
    let run = started["run"].as_str().expect("the run");
    assert_eq!(
        started["status"],
        "done",
        "{}",
        ws.json(&["status", run])["run"]["steps"]
    );
}

#[test]
fn muse_code_is_found_started_trusted_in_the_checkout_and_turned_at_a_fitted_level() {
    let mut ws = with_muse_installed();

    // --- found: by the adapter's own probe, with a version -----------------------
    let listed = ws.json(&["harness", "list"]);
    let rows = listed["harnesses"].as_array().expect("the rows");
    let row = rows
        .iter()
        .find(|row| row["id"] == HARNESS)
        .unwrap_or_else(|| panic!("no row for {HARNESS}: {listed}"));
    assert_eq!(row["probe"]["available"], true, "{row}");
    assert!(
        row["probe"]["version"]
            .as_str()
            .is_some_and(|v| v.starts_with("scripted-agent ")),
        "the version the binary answered, never a path: {row}"
    );
    assert_eq!(row["tier"], "builtin", "{row}");
    assert_eq!(row["label"], "Muse Code", "{row}");
    assert_eq!(
        row["launch"]["program"], PROGRAM,
        "the bare CLI in a terminal: {row}"
    );
    assert_eq!(
        row["launch"]["resume_args"],
        json!(["resume", "--last"]),
        "{row}"
    );
    assert_eq!(row["tool_guard"], true, "{row}");
    assert_eq!(row["install"]["verify"], "muse --version", "{row}");

    // --- its models: what the host answers `model/list`, the default first,
    //     each with the levels its row lists --------------------------------------
    let models = ws.json(&["agent", "models", HARNESS]);
    let listed = models["models"].as_array().expect("the models");
    assert_eq!(
        listed
            .iter()
            .filter_map(|m| m["id"].as_str())
            .collect::<Vec<_>>(),
        ["small", "large"],
        "{models}"
    );
    assert_eq!(listed[0]["efforts"], json!(["low", "medium"]), "{models}");
    assert_eq!(
        listed[1]["efforts"],
        json!(["low", "high", "max"]),
        "{models}"
    );
    assert_eq!(listed[1]["label"], "Model large", "{models}");
    let asked = ws.recorded_by(PROGRAM, "started");
    assert!(
        asked.iter().all(|fact| fact["args"] == json!(["serve"])),
        "a host asked a question opens no session, and is given no trust: {asked:?}"
    );

    // --- a run ----------------------------------------------------------------------
    an_agent(&ws, "Muse", &["large"], "xhigh");
    let workflow = one_step_for(&ws, "muse");
    ws.start();
    run_to_done(&ws, &workflow);

    // Started with the adapter's own words: the host, trusted in the
    // checkout, and nothing that names a model, a level, or a tool allowed
    // unasked.
    let started = ws.recorded_by(PROGRAM, "started");
    let hosts: Vec<&Value> = started
        .iter()
        .filter(|fact| fact["args"] != json!(["serve"]))
        .collect();
    assert_eq!(hosts.len(), 1, "one host for the one session: {started:?}");
    assert_eq!(hosts[0]["args"], json!(HOST_WORDS), "{}", hosts[0]);

    // The model list read first; then one session, in the checkout, under
    // the mode that still asks, handed the platform's own server.
    assert!(!ws.recorded_by(PROGRAM, "models").is_empty());
    let sessions = ws.recorded_by(PROGRAM, "session_start");
    assert_eq!(sessions.len(), 1, "one session a run: {sessions:?}");
    let session = &sessions[0];
    assert_eq!(session["model"], "large", "{session}");
    assert_eq!(session["approval_mode"], "promptUnmatched", "{session}");
    let cwd = session["cwd"].as_str().expect("the workspace root");
    assert!(
        std::path::Path::new(cwd).is_absolute() && std::path::Path::new(cwd).is_dir(),
        "an absolute folder that exists: {cwd}"
    );
    assert_eq!(session["scope"], "work_item", "{session}");
    assert!(
        session["servers"]
            .as_array()
            .is_some_and(|servers| servers.iter().any(|server| server["name"] == "bisa")),
        "{session}"
    );
    assert!(
        ws.recorded_by(PROGRAM, "start_refused").is_empty()
            && ws.recorded_by(PROGRAM, "config").is_empty(),
        "the model rode the start; nothing was set after"
    );

    // One turn, at the effort held to the levels `large` lists: `xhigh`
    // asked comes to `high`. Fitted to `small`'s levels it would have been
    // `medium`.
    let turns = ws.recorded_by(PROGRAM, "turn");
    assert_eq!(turns.len(), 1, "{turns:?}");
    assert_eq!(turns[0]["effort"], "high", "{}", turns[0]);
    assert_eq!(turns[0]["if_busy"], "queue", "{}", turns[0]);
    assert_eq!(turns[0]["parts"], 1, "the text alone: {}", turns[0]);
    assert_eq!(ws.recorded_by(PROGRAM, "prompt").len(), 1);
    // The worker's tool reached the platform's server, and the turn ended
    // with its words.
    let tools = ws.recorded_by(PROGRAM, "tool");
    assert!(
        tools
            .iter()
            .any(|t| t["name"] == "yield_result" && t["failed"] == false),
        "{tools:?}"
    );
    ws.stop();

    // The platform wrote nothing of the harness's own.
    assert!(
        !ws.home().join(".config").join("muse").exists()
            && !ws.home().join(".local").join("share").join("muse").exists(),
        "nothing under ~/.config/muse or ~/.local/share/muse"
    );
}

#[test]
fn a_model_the_host_does_not_list_is_passed_over_for_the_next_of_the_plan() {
    let mut ws = with_muse_installed();
    an_agent(&ws, "Muse", &["huge", "large"], "low");
    let workflow = one_step_for(&ws, "muse");
    ws.start();
    run_to_done(&ws, &workflow);

    // A host was started for each model tried; only the one on `large`
    // started a session — the host's list said `huge` is not there, before
    // any session file was written — and only that session was turned.
    let hosts = ws
        .recorded_by(PROGRAM, "started")
        .into_iter()
        .filter(|fact| fact["args"] == json!(HOST_WORDS))
        .count();
    assert!(hosts >= 2, "a host for each model tried: {hosts}");
    assert!(ws.recorded_by(PROGRAM, "models").len() >= 2);
    let sessions = ws.recorded_by(PROGRAM, "session_start");
    assert_eq!(sessions.len(), 1, "{sessions:?}");
    assert_eq!(sessions[0]["model"], "large", "{}", sessions[0]);
    assert!(
        ws.recorded_by(PROGRAM, "start_refused").is_empty(),
        "nothing was ever asked to start on a model the host does not list"
    );
    let turns = ws.recorded_by(PROGRAM, "turn");
    assert_eq!(turns.len(), 1, "{turns:?}");
    assert_eq!(turns[0]["effort"], "low", "{}", turns[0]);
    ws.stop();
}

#[test]
fn a_muse_code_tab_is_a_plain_terminal_and_no_session() {
    let mut ws = Sealed::bare();
    let _node = ws.start_listening(a_port());
    let checkout = a_project(&ws, "shelf");
    let (_, before) = ws.call("GET", "/node", &[], None);

    // The CLI takes no hook for one launch, and the platform writes nothing
    // under ~/.config/muse or into the project: the tab opens, and nothing
    // is handed it.
    let (status, opened) = ws.call(
        "POST",
        "/sessions/terminal",
        &[],
        Some(&json!({"scope": "workstream", "id": checkout, "harness": HARNESS})),
    );
    assert_eq!(status, 200, "{opened}");
    assert!(opened["session"].is_null(), "no roster row: {opened}");
    assert!(
        opened["args"].as_array().is_none_or(|args| args.is_empty()),
        "no word is added to its command: {opened}"
    );
    let (_, after) = ws.call("GET", "/node", &[], None);
    assert_eq!(after["live_sessions"], before["live_sessions"], "{after}");
    assert!(
        !ws.data().join("run").join("interactive").exists()
            || std::fs::read_dir(ws.data().join("run").join("interactive"))
                .is_ok_and(|mut entries| entries.next().is_none()),
        "nothing was written for it"
    );
    assert!(!ws.home().join(".config").join("muse").exists());
    ws.stop();
}
