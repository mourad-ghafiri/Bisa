//! GitHub Copilot CLI, Grok Build and Gemini CLI, through the binary: three
//! harnesses that speak the Agent Client Protocol under ids of their own.
//!
//! No real CLI runs. The scripted agent is placed under each program's name,
//! so each adapter finds it by its own probe (`--version`, answered with a
//! version), starts it with its own words, and drives it through the one ACP
//! door. What a journey asserts is what the agent kept: the command line it
//! was started with, the MCP server it was handed, and what its session was
//! set to — the model first, then the effort, fitted to the levels *that
//! model* offers, where the harness has a level at all — never how the code
//! got there. Gemini CLI's agent speaks the draft before config options
//! (`model_api: set_model`): its model is set with `session/set_model`, and
//! nothing is sent for an effort it has no control for.
//!
//! The terminal half plays the hooks Copilot CLI fires against the plugin the
//! platform handed it — the PascalCase events and the payload its hooks
//! reference gives them (`hook_event_name`, `cwd`, `tool_name` as Claude
//! names the tool, `tool_input`;
//! https://docs.github.com/en/copilot/reference/hooks-reference, read
//! 2026-09-30) — each command line run as a harness runs it. Grok Build's and
//! Gemini CLI's terminals are handed nothing, and are no session.

use super::a_harness_in_a_terminal::{a_port, a_project, a_shell, ended, waiting_in_a_terminal};
use super::sealed::Sealed;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Output;

/// One harness of the journey.
struct Harness {
    /// The program it is found as — its id, the binary's name.
    program: &'static str,
    /// The agent of the journey that runs on it.
    agent: &'static str,
    /// The words its adapter starts it with.
    words: &'static [&'static str],
    /// What a session on `large` at `max` is set to — the model, then the
    /// effort held to the levels `large` offers; nothing for the effort
    /// where the harness has no level.
    set: &'static [(&'static str, &'static str)],
}

const HARNESSES: [Harness; 3] = [
    Harness {
        program: "copilot",
        agent: "pilot",
        words: &["--acp", "--stdio", "--no-ask-user"],
        set: &[("model", "large"), ("effort", "high")],
    },
    Harness {
        program: "grok",
        agent: "builder",
        words: &["agent", "--no-leader", "stdio"],
        set: &[("model", "large"), ("effort", "high")],
    },
    Harness {
        program: "gemini",
        agent: "twin",
        words: &["--acp"],
        set: &[("model", "large")],
    },
];

/// A harness whose sessions choose their model: they open on `small`, which
/// takes two levels; `large` takes others. A worker on it yields. Gemini
/// CLI's says its models the draft way and offers no level at all.
fn chooses_its_model(program: &str) -> Value {
    let mut script = json!({
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
    });
    if program == "gemini" {
        script["model_api"] = json!("set_model");
        script.as_object_mut().unwrap().remove("model_efforts");
    }
    script
}

/// A workspace with every harness installed, the two core agents on the
/// first, and no daemon yet.
fn with_all_installed() -> Sealed {
    let ws = Sealed::bare();
    for Harness { program, .. } in HARNESSES {
        ws.install_agent_as(program, &chooses_its_model(program));
    }
    for agent in ["general-agent", "workflow-agent"] {
        ws.ok(&[
            "agent",
            "edit",
            agent,
            "--harness",
            "copilot",
            "--model",
            "large",
        ]);
    }
    ws
}

/// An agent of the workspace on `harness`, on the models given, best first,
/// asked to work at `effort`.
fn an_agent(ws: &Sealed, name: &str, harness: &str, models: &[&str], effort: &str) {
    let mut args = vec![
        "agent",
        "add",
        "--name",
        name,
        "--harness",
        harness,
        "--effort",
        effort,
        "--prompt",
        "You write what you are asked to.",
    ];
    for model in models.iter().copied() {
        args.extend_from_slice(&["--model", model]);
    }
    ws.ok(&args);
    let id = name.to_lowercase();
    let listed = ws.json(&["agent", "list"]);
    assert!(
        listed["agents"]
            .as_array()
            .is_some_and(|all| all.iter().any(|a| a["id"] == id.as_str())),
        "{listed}"
    );
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

/// What the sessions of the agent placed as `program` were set to, in the
/// order they were set: `(session, option, value)`.
fn set_on(ws: &Sealed, program: &str) -> Vec<(String, String, String)> {
    let word = |v: &Value| v.as_str().unwrap_or_default().to_string();
    ws.recorded_by(program, "config")
        .iter()
        .map(|set| {
            (
                word(&set["session"]),
                word(&set["config_id"]),
                word(&set["value"]),
            )
        })
        .collect()
}

#[test]
fn each_harness_is_found_started_and_set_to_its_model_and_level_the_protocols_way() {
    let mut ws = with_all_installed();

    // --- found: by the adapter's own probe, with a version -----------------------
    let listed = ws.json(&["harness", "list"]);
    let rows = listed["harnesses"].as_array().expect("the rows");
    for Harness { program, .. } in HARNESSES {
        let row = rows
            .iter()
            .find(|row| row["id"] == program)
            .unwrap_or_else(|| panic!("no row for {program}: {listed}"));
        assert_eq!(row["probe"]["available"], true, "{row}");
        assert!(
            row["probe"]["version"]
                .as_str()
                .is_some_and(|v| v.starts_with("scripted-agent ")),
            "the version the binary answered, never a path: {row}"
        );
        assert_eq!(row["tier"], "builtin", "{row}");
        assert_eq!(row["launch"]["program"], program, "{row}");
    }

    // --- its models: Grok Build's as its CLI prints them, the default first;
    //     Copilot CLI's as its reference lists them; Gemini CLI's as its page
    //     names them, `auto` first, with no level beside any ---------------------
    let grok = ws.json(&["agent", "models", "grok"]);
    let ids: Vec<&str> = grok["models"]
        .as_array()
        .expect("the models")
        .iter()
        .filter_map(|m| m["id"].as_str())
        .collect();
    assert_eq!(ids, ["small", "large"], "{grok}");
    let copilot = ws.json(&["agent", "models", "copilot"]);
    assert_eq!(copilot["models"][0]["id"], "claude-sonnet-4.6", "{copilot}");
    assert!(
        copilot["models"]
            .as_array()
            .is_some_and(|all| all.len() == 11),
        "{copilot}"
    );
    let gemini = ws.json(&["agent", "models", "gemini"]);
    let listed = gemini["models"].as_array().expect("the models");
    assert_eq!(
        listed
            .iter()
            .filter_map(|m| m["id"].as_str())
            .collect::<Vec<_>>(),
        [
            "auto",
            "gemini-3-pro-preview",
            "gemini-3-flash-preview",
            "gemini-2.5-pro",
            "gemini-2.5-flash"
        ],
        "{gemini}"
    );
    assert!(
        listed
            .iter()
            .all(|m| m["efforts"].as_array().is_none_or(|e| e.is_empty())),
        "no effort control: {gemini}"
    );

    // --- a run on each ------------------------------------------------------------
    let mut workflows = Vec::new();
    for Harness { program, agent, .. } in HARNESSES {
        let name = format!("{}{}", agent[..1].to_uppercase(), &agent[1..]);
        an_agent(&ws, &name, program, &["large"], "max");
        workflows.push(one_step_for(&ws, agent));
    }
    ws.start();
    for (
        Harness {
            program,
            words,
            set: expected,
            ..
        },
        workflow,
    ) in HARNESSES.iter().zip(&workflows)
    {
        run_to_done(&ws, workflow);

        // Started with the adapter's own words: the protocol, and nothing
        // that names a model, a level, or a tool allowed unasked.
        let started = ws.recorded_by(program, "started");
        assert!(!started.is_empty(), "{program} was never started");
        for fact in &started {
            assert_eq!(fact["args"], json!(words), "{program}: {fact}");
        }

        // Handed the platform's own MCP server, as every session is.
        let sessions = ws.recorded_by(program, "session_new");
        assert_eq!(sessions.len(), 1, "{program}: one session a run");
        assert!(
            sessions[0]["servers"]
                .as_array()
                .is_some_and(|servers| servers.iter().any(|server| server["name"] == "bisa")),
            "{program}: {}",
            sessions[0]
        );

        // Set the protocol's way: the model, then the effort — `max` asked,
        // held to the levels `large` offers. Fitted to the list the session
        // opened with, `small`'s, it would have been `medium`. A harness with
        // no level is set its model alone, the draft's way.
        let session = sessions[0]["session"].as_str().unwrap_or_default();
        let set: Vec<(String, String)> = set_on(&ws, program)
            .into_iter()
            .filter(|(of, _, _)| of == session)
            .map(|(_, option, value)| (option, value))
            .collect();
        let expected: Vec<(String, String)> = expected
            .iter()
            .map(|(option, value)| (option.to_string(), value.to_string()))
            .collect();
        assert_eq!(set, expected, "{program}");
        // And it was prompted once it was set, never before.
        let prompts = ws.recorded_by(program, "prompt");
        assert_eq!(prompts.len(), 1, "{program}: {prompts:?}");
    }
    ws.stop();
}

#[test]
fn a_model_a_session_does_not_offer_is_passed_over_for_the_next_of_the_plan() {
    let mut ws = with_all_installed();
    // A plan whose first model the harness does not offer — on a harness
    // whose sessions say their models as a config option, and on one that
    // says them the draft way.
    let walked = [("copilot", "Pilot", "pilot"), ("gemini", "Twin", "twin")];
    let workflows: Vec<String> = walked
        .iter()
        .map(|(program, name, agent)| {
            an_agent(&ws, name, program, &["huge", "large"], "low");
            one_step_for(&ws, agent)
        })
        .collect();
    ws.start();
    for ((program, _, _), workflow) in walked.iter().zip(&workflows) {
        run_to_done(&ws, workflow);

        // The session asked for `huge` ended before it was prompted; the one
        // that did the work was set to `large`. Nothing was ever set to a
        // model the harness does not offer, and no session ran on one nobody
        // asked for.
        let sessions = ws.recorded_by(program, "session_new");
        assert!(
            sessions.len() > 1,
            "{program}: a session for each model tried: {sessions:?}"
        );
        let models: Vec<String> = set_on(&ws, program)
            .into_iter()
            .filter(|(_, option, _)| option == "model")
            .map(|(_, _, value)| value)
            .collect();
        assert_eq!(
            models,
            ["large"],
            "{program}: the only model a session was set to"
        );
        let prompts = ws.recorded_by(program, "prompt");
        assert_eq!(
            prompts.len(),
            1,
            "{program}: one session was prompted: {prompts:?}"
        );
        let worked = prompts[0]["session"].as_str().unwrap_or_default();
        assert!(
            set_on(&ws, program)
                .iter()
                .any(|(session, option, value)| session == worked
                    && option == "model"
                    && value == "large"),
            "{program}: the session that was prompted is the one on `large`"
        );
    }
    // The draft's set is its own method; the option's is the protocol's.
    let how: Vec<Value> = ws
        .recorded_by("gemini", "config")
        .iter()
        .map(|set| set["method"].clone())
        .collect();
    assert!(
        !how.is_empty() && how.iter().all(|m| m == "session/set_model"),
        "{how:?}"
    );
    ws.stop();
}

// --- in a terminal ---------------------------------------------------------------

/// A tab with Copilot CLI in it: what its host was answered, and the plugin
/// the harness was pointed at.
struct CopilotTab {
    session: String,
    /// What every hook inherits: the environment the node answered, and
    /// where the node is.
    given: Vec<(String, String)>,
    /// The folder `--plugin-dir` names.
    plugin: PathBuf,
    hooks: Value,
}

impl CopilotTab {
    fn open(ws: &Sealed, node: &str, checkout: &str) -> Self {
        let (status, opened) = ws.call(
            "POST",
            "/sessions/terminal",
            &[],
            Some(&json!({"scope": "workstream", "id": checkout, "harness": "copilot"})),
        );
        assert_eq!(status, 200, "the harness is registered: {opened}");
        let session = opened["session"]
            .as_str()
            .expect("a harness that reports is a session")
            .to_string();
        let mut given: Vec<(String, String)> = opened["env"]
            .as_object()
            .expect("the environment of the process")
            .iter()
            .map(|(name, value)| (name.clone(), value.as_str().unwrap_or_default().to_string()))
            .collect();
        given.push(("BISA_NODE_URL".to_string(), node.to_string()));

        // The recipe: one word, naming one folder of the session's own.
        let args: Vec<&str> = opened["args"]
            .as_array()
            .expect("the arguments")
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(args.len(), 1, "{args:?}");
        let plugin = PathBuf::from(
            args[0]
                .strip_prefix("--plugin-dir=")
                .unwrap_or_else(|| panic!("the plugin's folder, in one word: {args:?}")),
        );
        assert_eq!(
            plugin,
            ws.data().join("run").join("interactive").join(&session),
            "the session's own folder, never a project and never ~/.copilot"
        );
        let read = |name: &str| -> Value {
            let text = std::fs::read_to_string(plugin.join(name))
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name} as JSON: {e}"))
        };
        let manifest = read("plugin.json");
        assert_eq!(manifest["hooks"], "hooks.json", "{manifest}");
        let hooks = read("hooks.json");
        assert_eq!(hooks["version"], 1, "{hooks}");
        Self {
            session,
            given,
            plugin,
            hooks: hooks["hooks"].clone(),
        }
    }

    /// The command lines Copilot CLI runs when `event` fires, in order.
    fn lines(&self, event: &str) -> Vec<String> {
        self.hooks[event]
            .as_array()
            .unwrap_or_else(|| panic!("the plugin hooks {event}"))
            .iter()
            .map(|hook| {
                assert_eq!(hook["type"], "command", "{hook}");
                hook["command"].as_str().expect("a command").to_string()
            })
            .collect()
    }

    /// Copilot CLI fires `event`: every hook of it runs, in order, with the
    /// payload its PascalCase name delivers. What each said.
    fn fires(&self, ws: &Sealed, event: &str, carrying: Value) -> Vec<Output> {
        let mut payload = json!({
            "hook_event_name": event,
            "session_id": "copilots-own-id",
            "timestamp": "2026-09-30T10:00:00Z",
            "cwd": ws.data(),
        });
        for (key, value) in carrying.as_object().expect("what the hook carries") {
            payload[key] = value.clone();
        }
        let input = payload.to_string();
        self.lines(event)
            .iter()
            .map(|line| {
                let out = ws.hook(line, &self.given, &input);
                // Copilot CLI reads a `PreToolUse` hook that fails as a
                // refusal: a hook of the platform's never fails.
                assert!(
                    out.status.success(),
                    "a hook never fails its harness ({event}): {}",
                    String::from_utf8_lossy(&out.stderr)
                );
                out
            })
            .collect()
    }

    /// [`Self::fires`], for an event whose hooks have nothing to say.
    fn reports(&self, ws: &Sealed, event: &str, carrying: Value) {
        for out in self.fires(ws, event, carrying) {
            assert_eq!(
                (
                    String::from_utf8_lossy(&out.stdout).trim(),
                    String::from_utf8_lossy(&out.stderr).trim()
                ),
                ("", ""),
                "{event}"
            );
        }
    }

    /// Copilot CLI is about to run a tool: the reporter, then the guard.
    /// What the guard printed — the verdict Copilot reads — or nothing.
    fn asks_before(&self, ws: &Sealed, tool: &str, input: Value) -> Option<Value> {
        let said = self.fires(
            ws,
            "PreToolUse",
            json!({"tool_name": tool, "tool_input": input}),
        );
        assert_eq!(said.len(), 2, "the reporter, then the guard");
        assert!(
            said[0].stdout.is_empty(),
            "the reporter prints nothing Copilot would read as a verdict"
        );
        let verdict = String::from_utf8_lossy(&said[1].stdout).trim().to_string();
        (!verdict.is_empty()).then(|| serde_json::from_str(&verdict).expect("a verdict, as JSON"))
    }

    /// The row's state, in its one word, and the whole of it.
    fn reads(&self, ws: &Sealed, word: &str) -> Value {
        let (status, row) = ws.call("GET", &format!("/sessions/{}", self.session), &[], None);
        assert_eq!(status, 200, "{row}");
        assert_eq!(row["state"]["state"], word, "{row}");
        row
    }

    /// One of the host's doors, under the session's secret.
    fn host(&self, ws: &Sealed, door: &str, body: Option<Value>) -> (u16, Value) {
        let secret = self
            .given
            .iter()
            .find(|(name, _)| name == "BISA_SESSION_SECRET")
            .map(|(_, value)| value.as_str())
            .expect("the session's secret, in the environment");
        let bearer = format!("Bearer {secret}");
        ws.call_with(
            "POST",
            &format!("/sessions/{}/{door}", self.session),
            &[("authorization", &bearer)],
            body.as_ref(),
        )
    }
}

#[test]
fn a_copilot_tab_reports_through_its_plugin_and_is_judged_in_copilots_own_shape() {
    let mut ws = Sealed::bare();
    let node = ws.start_listening(a_port());
    let checkout = a_project(&ws, "shelf");
    // A rule of the journey's own, on a tool nobody has.
    let rules = json!([
        {"id": "journey_refuses", "label": "refused by the journey", "action": "deny",
         "matcher": {"kind": "command", "regex": "^fake-refused\\b"}},
    ]);
    ws.ok(&[
        "settings",
        "set",
        "workspace",
        "security.guard.rules",
        &rules.to_string(),
    ]);

    // --- opened: a plugin of two files, the owner's alone -------------------------
    let tab = CopilotTab::open(&ws, &node, &checkout);
    let row = tab.reads(&ws, "starting");
    assert_eq!(row["kind"], "terminal");
    assert_eq!(row["harness"], "copilot");
    #[cfg(unix)]
    for name in ["plugin.json", "hooks.json"] {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(tab.plugin.join(name))
            .unwrap_or_else(|e| panic!("{name}: {e}"))
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "{name}: the owner's alone");
    }
    // Every event is subscribed under the name that delivers the payload the
    // reporter and the guard read; the guard is on `PreToolUse` alone.
    let subscribed = tab.hooks.as_object().expect("the hooks, by event");
    for (event, entries) in subscribed {
        assert!(
            event.starts_with(|c: char| c.is_ascii_uppercase()),
            "{event}"
        );
        let lines = tab.lines(event);
        assert!(
            lines[0].ends_with("'session' 'report' '--harness' 'copilot'"),
            "{event}: {}",
            lines[0]
        );
        assert_eq!(
            lines.len(),
            if event == "PreToolUse" { 2 } else { 1 },
            "{event}: {entries}"
        );
    }
    assert!(
        tab.lines("PreToolUse")[1].ends_with("'session' 'guard' '--harness' 'copilot'"),
        "{:?}",
        tab.lines("PreToolUse")
    );

    // --- the process is up, and Copilot speaks, hook by hook ----------------------
    let shell = a_shell();
    let (status, answer) = tab.host(
        &ws,
        "report",
        Some(json!({"events": [
            {"tier": "lifecycle", "event": {"type": "process_started", "pid": shell.id()}}
        ]})),
    );
    assert_eq!(status, 200, "{answer}");
    tab.reports(&ws, "SessionStart", json!({"source": "startup"}));
    tab.reads(&ws, "idle");
    tab.reports(&ws, "UserPromptSubmit", json!({"prompt": "tidy the shelf"}));
    tab.reads(&ws, "thinking");

    // A tool the guard has no opinion on: it says nothing, and Copilot's own
    // prompt stands.
    assert_eq!(
        tab.asks_before(&ws, "Bash", json!({"command": "fake-tool --level"})),
        None
    );
    let row = tab.reads(&ws, "running");
    assert_eq!(row["state"]["tool"], "Bash", "{row}");

    // --- it waits on its person, in its own words: a row of the Inbox -------------
    assert_eq!(waiting_in_a_terminal(&ws), Vec::<Value>::new());
    tab.reports(
        &ws,
        "Notification",
        json!({
            "notification_type": "permission_prompt",
            "title": "Permission needed",
            "message": "Copilot wants to run: fake-tool --level",
        }),
    );
    tab.reads(&ws, "waiting");
    let waiting = waiting_in_a_terminal(&ws);
    assert_eq!(waiting.len(), 1, "{waiting:?}");
    assert_eq!(waiting[0]["key"], tab.session.as_str());
    // Answered at the prompt: the tool ran, and the wait is over.
    tab.reports(&ws, "PostToolUse", json!({"tool_name": "Bash"}));
    tab.reads(&ws, "thinking");
    assert_eq!(waiting_in_a_terminal(&ws), Vec::<Value>::new());
    // A background agent going idle is not this turn's end.
    tab.reports(
        &ws,
        "Notification",
        json!({"notification_type": "agent_idle", "message": "an agent is idle"}),
    );
    tab.reads(&ws, "thinking");

    // --- a call the journey's rule refuses: the verdict, flat ---------------------
    let refused = tab
        .asks_before(&ws, "Bash", json!({"command": "fake-refused now"}))
        .expect("a verdict");
    assert_eq!(refused["permissionDecision"], "deny", "{refused}");
    assert!(
        refused["permissionDecisionReason"]
            .as_str()
            .is_some_and(|reason| reason.contains("refused by the journey")),
        "{refused}"
    );
    assert!(
        refused.get("hookSpecificOutput").is_none(),
        "Claude Code's nesting is Claude Code's: {refused}"
    );
    tab.reports(&ws, "Stop", json!({"stop_reason": "end_turn"}));
    tab.reads(&ws, "idle");

    // --- the process ends, the tab closes, the plugin goes with it ----------------
    let code = ended(shell);
    let (status, answer) = tab.host(&ws, "exit", Some(json!({"code": code, "signal": null})));
    assert_eq!(status, 200, "{answer}");
    tab.reads(&ws, "done");
    let (status, answer) = tab.host(&ws, "close", None);
    assert_eq!(status, 200, "{answer}");
    assert!(!tab.plugin.exists(), "the session's files went with it");
    // A guard with nobody to ask gives no verdict, and still ends well:
    // Copilot's own prompt stands.
    assert_eq!(
        tab.asks_before(&ws, "Bash", json!({"command": "fake-refused now"})),
        None
    );
    ws.stop();
}

#[test]
fn a_grok_build_or_gemini_cli_tab_is_a_plain_terminal_and_no_session() {
    let mut ws = Sealed::bare();
    let _node = ws.start_listening(a_port());
    let checkout = a_project(&ws, "shelf");
    let (_, before) = ws.call("GET", "/node", &[], None);

    // Neither TUI takes a hook for one launch, and the platform writes
    // nothing under ~/.grok or ~/.gemini or into the project: each opens, and
    // nothing is handed it.
    for harness in ["grok", "gemini"] {
        let (status, opened) = ws.call(
            "POST",
            "/sessions/terminal",
            &[],
            Some(&json!({"scope": "workstream", "id": checkout, "harness": harness})),
        );
        assert_eq!(status, 200, "{harness} opens all the same: {opened}");
        assert!(
            opened["session"].is_null(),
            "{harness}: no roster row: {opened}"
        );
        assert!(
            opened["args"].as_array().is_none_or(|args| args.is_empty()),
            "{harness}: no word is added to its command: {opened}"
        );
    }
    let (_, after) = ws.call("GET", "/node", &[], None);
    assert_eq!(after["live_sessions"], before["live_sessions"], "{after}");
    assert!(
        !ws.data().join("run").join("interactive").exists()
            || std::fs::read_dir(ws.data().join("run").join("interactive"))
                .is_ok_and(|mut entries| entries.next().is_none()),
        "nothing was written for either"
    );

    // A harness with no terminal form at all is still refused by name.
    let (status, _) = ws.call(
        "POST",
        "/sessions/terminal",
        &[],
        Some(&json!({"scope": "workstream", "id": checkout, "harness": "acp:goose"})),
    );
    assert_eq!(status, 400, "a protocol target opens in no terminal");
    ws.stop();
}
