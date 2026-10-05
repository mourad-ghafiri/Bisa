//! A harness a person opened in a terminal, from its tab's opening to its
//! close, as the two who speak for it do: the **tab's host**, which
//! registers it under the workspace's token and then says how its process
//! started, ended and closed under the session's own secret; and the
//! **harness's hooks**, which are command lines the node wrote, run here
//! exactly as a harness runs them — through `sh`, the payload on their
//! input, the session and the node's address in their environment.
//!
//! No harness runs: the journey plays the hooks Claude Code fires against
//! the recipe the platform handed it — the events in the order its hooks
//! reference gives for a tool call (`PreToolUse`, `PermissionRequest` when a
//! decision is needed, `PostToolUse`), each payload the fields the reference
//! names and the platform reads (https://code.claude.com/docs/en/hooks, read
//! 2026-09-30). A harness runs an event's hooks at once; the journey runs
//! them one after the other, which is one of the orders the node may see.
//! The process behind the tab is a shell of the journey's own that waits on
//! its input and ends when the journey lets it.

use super::sealed::Sealed;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};

const HARNESS: &str = "claude-code";
const SECRET: &str = "BISA_SESSION_SECRET";

/// A loopback port nothing listens on right now: the node is given it, and
/// given it again after a restart, as a desktop gives its node one address.
pub(super) fn a_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .expect("a free port")
        .port()
}

/// A project that is a repository: its id, which is its own tree's
/// workstream's.
pub(super) fn a_project(ws: &Sealed, slug: &str) -> String {
    let made = ws.json(&["project", "new", slug]);
    made["project"]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("the project: {made}"))
        .to_string()
}

/// The shell behind a tab: it waits on its input, and ends well once the
/// journey closes it.
pub(super) fn a_shell() -> Child {
    Command::new("/bin/sh")
        .args(["-c", "read line; exit 0"])
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("a shell of the journey's own")
}

/// One tab with a harness in it: what its host was answered, and the hooks
/// the harness was handed.
struct Tab {
    session: String,
    /// What every hook inherits: the environment the node answered — the
    /// session and its secret among it — and where the node is.
    given: Vec<(String, String)>,
    /// The file the harness was pointed at, and the hooks in it by event.
    settings: PathBuf,
    hooks: Value,
}

impl Tab {
    /// The host registers the harness, in a checkout, before it runs.
    fn open(ws: &Sealed, node: &str, checkout: &str) -> Self {
        let (status, opened) = ws.call(
            "POST",
            "/sessions/terminal",
            &[],
            Some(&json!({"scope": "workstream", "id": checkout, "harness": HARNESS})),
        );
        assert_eq!(status, 200, "the harness is registered");
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
        assert_eq!(
            given
                .iter()
                .find(|(name, _)| name == "BISA_SESSION")
                .map(|(_, value)| value.as_str()),
            Some(session.as_str())
        );
        assert!(
            opened.get("secret").is_none() && opened.get("token").is_none(),
            "the secret travels once, in the environment"
        );
        given.push(("BISA_NODE_URL".to_string(), node.to_string()));

        // The recipe: one flag naming one file of the session's own.
        let args: Vec<&str> = opened["args"]
            .as_array()
            .expect("the arguments")
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(args.first().copied(), Some("--settings"), "{args:?}");
        let settings = PathBuf::from(args[1]);
        assert!(
            settings.starts_with(ws.data().join("run").join("interactive").join(&session)),
            "under the workspace's run folder, never in a project: {}",
            settings.display()
        );
        let text = std::fs::read_to_string(&settings).expect("the settings file");
        let hooks =
            serde_json::from_str::<Value>(&text).expect("the settings, as JSON")["hooks"].clone();
        Self {
            session,
            given,
            settings,
            hooks,
        }
    }

    fn secret(&self) -> &str {
        self.given
            .iter()
            .find(|(name, _)| name == SECRET)
            .map(|(_, value)| value.as_str())
            .expect("the session's secret, in the environment")
    }

    /// The command lines the harness runs when `event` fires, in order.
    fn lines(&self, event: &str) -> Vec<String> {
        self.hooks[event][0]["hooks"]
            .as_array()
            .unwrap_or_else(|| panic!("the recipe hooks {event}"))
            .iter()
            .map(|hook| {
                assert_eq!(hook["type"], "command", "{hook}");
                hook["command"].as_str().expect("a command").to_string()
            })
            .collect()
    }

    /// The harness fires `event`: every hook of it runs, in order, with the
    /// payload a Claude Code hook carries. What each said.
    fn fires(&self, ws: &Sealed, event: &str, carrying: Value) -> Vec<Output> {
        let mut payload = json!({
            "hook_event_name": event,
            "session_id": "the-harness-own-id",
            "cwd": ws.data(),
            "permission_mode": "default",
            "transcript_path": ws.data().join("transcript.jsonl"),
        });
        for (key, value) in carrying.as_object().expect("what the hook carries") {
            payload[key] = value.clone();
        }
        let input = payload.to_string();
        self.lines(event)
            .iter()
            .map(|line| {
                let out = ws.hook(line, &self.given, &input);
                assert!(
                    out.status.success(),
                    "a hook never fails its harness ({event}): {}",
                    String::from_utf8_lossy(&out.stderr)
                );
                out
            })
            .collect()
    }

    /// [`Self::fires`], for an event whose hooks have nothing to say: each
    /// was taken by the node, in silence.
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

    /// The harness is about to run a tool: the reporter, then the guard.
    /// What the guard printed — the verdict the harness reads — or nothing.
    /// `id` is the call's `tool_use_id`, the one its `PermissionRequest` and
    /// its `PostToolUse` carry too.
    fn asks_before(&self, ws: &Sealed, tool: &str, id: &str, input: Value) -> Option<Value> {
        let said = self.fires(
            ws,
            "PreToolUse",
            json!({"tool_name": tool, "tool_input": input, "tool_use_id": id}),
        );
        assert_eq!(said.len(), 2, "the reporter, then the guard");
        assert!(
            said[0].stdout.is_empty(),
            "the reporter prints nothing a harness would read as a verdict"
        );
        let verdict = String::from_utf8_lossy(&said[1].stdout).trim().to_string();
        (!verdict.is_empty()).then(|| serde_json::from_str(&verdict).expect("a verdict, as JSON"))
    }

    /// One of the host's doors, under the session's secret.
    fn host(&self, ws: &Sealed, door: &str, body: Option<Value>) -> (u16, Value) {
        let bearer = format!("Bearer {}", self.secret());
        ws.call_with(
            "POST",
            &format!("/sessions/{}/{door}", self.session),
            &[("authorization", &bearer)],
            body.as_ref(),
        )
    }

    /// The host says the process is up, as it does once the shell spawned.
    fn started(&self, ws: &Sealed, shell: &Child) {
        let (status, answer) = self.host(
            ws,
            "report",
            Some(json!({"events": [
                {"tier": "lifecycle", "event": {"type": "process_started", "pid": shell.id()}}
            ]})),
        );
        assert_eq!(status, 200, "{answer}");
    }

    fn row(&self, ws: &Sealed) -> (u16, Value) {
        ws.call("GET", &format!("/sessions/{}", self.session), &[], None)
    }

    /// The row's state, in its one word, and the whole of it.
    fn state(&self, ws: &Sealed) -> (String, Value) {
        let (status, row) = self.row(ws);
        assert_eq!(status, 200, "{row}");
        (
            row["state"]["state"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            row,
        )
    }

    fn reads(&self, ws: &Sealed, word: &str) -> Value {
        let (state, row) = self.state(ws);
        assert_eq!(state, word, "{row}");
        row
    }
}

/// The rows of the Inbox that are a harness waiting in a terminal.
pub(super) fn waiting_in_a_terminal(ws: &Sealed) -> Vec<Value> {
    ws.json(&["inbox"])["rows"]
        .as_array()
        .expect("the rows")
        .iter()
        .filter(|row| row["kind"] == "session")
        .cloned()
        .collect()
}

/// Let the shell go and say how it ended.
pub(super) fn ended(mut shell: Child) -> Option<i32> {
    drop(shell.stdin.take());
    shell.wait().expect("the shell ends").code()
}

#[test]
fn a_harness_in_a_terminal_is_a_row_from_its_start_to_its_tabs_close() {
    let mut ws = Sealed::bare();
    let port = a_port();
    let node = ws.start_listening(port);
    assert_eq!(node, format!("http://127.0.0.1:{port}"));
    let checkout = a_project(&ws, "shelf");
    // Two rules of the journey's own, on a tool nobody has.
    let rules = json!([
        {"id": "journey_refuses", "label": "refused by the journey", "action": "deny",
         "matcher": {"kind": "command", "regex": "^fake-refused\\b"}},
        {"id": "journey_asks", "label": "asked by the journey", "action": "ask",
         "matcher": {"kind": "command", "regex": "^fake-asked\\b"}},
    ]);
    ws.ok(&[
        "settings",
        "set",
        "workspace",
        "security.guard.rules",
        &rules.to_string(),
    ]);

    // --- opened: a row before the harness has said a word -----------------------
    let tab = Tab::open(&ws, &node, &checkout);
    let row = tab.reads(&ws, "starting");
    assert_eq!(row["kind"], "terminal");
    assert_eq!(row["harness"], HARNESS);
    assert_eq!(row["workstream"], checkout.as_str());
    assert_eq!(row["project"], checkout.as_str());
    assert!(
        row.get("model").is_none() && row.get("effort").is_none(),
        "a person's own harness: the row claims neither: {row}"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&tab.settings)
            .expect("the settings file")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "the owner's alone");
    }
    // Every hook is the platform's own binary in its reporter personality —
    // and no hook holds the secret: it is in the environment and nowhere else.
    let recipe = std::fs::read_to_string(&tab.settings).expect("the settings file");
    assert!(!recipe.contains(tab.secret()), "no secret in a file");
    for event in ["SessionStart", "UserPromptSubmit", "Stop", "SubagentStart"] {
        let lines = tab.lines(event);
        assert_eq!(lines.len(), 1, "{event}: {lines:?}");
        assert!(
            lines[0].ends_with("'session' 'report' '--harness' 'claude-code'"),
            "{event}: {}",
            lines[0]
        );
    }
    let before_a_tool = tab.lines("PreToolUse");
    assert!(
        before_a_tool[1].ends_with("'session' 'guard' '--harness' 'claude-code'"),
        "{before_a_tool:?}"
    );

    // --- the process is up --------------------------------------------------------
    let shell = a_shell();
    tab.started(&ws, &shell);
    assert_eq!(tab.reads(&ws, "starting")["pid"], shell.id());

    // --- the harness speaks, hook by hook ---------------------------------------
    tab.reports(
        &ws,
        "SessionStart",
        json!({"source": "startup", "model": "claude-opus-5-5"}),
    );
    assert_eq!(tab.reads(&ws, "idle")["model"], "claude-opus-5-5");
    tab.reports(&ws, "UserPromptSubmit", json!({}));
    tab.reads(&ws, "thinking");

    // A tool the guard has no opinion on: it says nothing, and the harness's
    // own prompt stands.
    assert_eq!(
        tab.asks_before(
            &ws,
            "Bash",
            "toolu_1",
            json!({"command": "fake-tool --level"})
        ),
        None
    );
    let row = tab.reads(&ws, "running");
    assert_eq!(row["state"]["tool"], "Bash");
    assert_eq!(
        row["state"]["args"], r#"{"command":"fake-tool --level"}"#,
        "what the tool was given, as the harness gave it"
    );
    tab.reports(
        &ws,
        "PostToolUse",
        json!({"tool_name": "Bash", "tool_use_id": "toolu_1"}),
    );
    tab.reads(&ws, "thinking");

    // One the journey's rule asks about: the verdict hands the call to the
    // person at the keyboard, and the harness stops at its own prompt.
    let asked = tab
        .asks_before(&ws, "Bash", "toolu_2", json!({"command": "fake-asked now"}))
        .expect("a verdict");
    assert_eq!(
        asked["hookSpecificOutput"]["permissionDecision"], "ask",
        "{asked}"
    );

    // --- it waits on its person: a row of the Inbox, answered in the tab --------
    assert_eq!(waiting_in_a_terminal(&ws), Vec::<Value>::new());
    tab.reports(
        &ws,
        "PermissionRequest",
        json!({
            "tool_name": "Bash",
            "tool_input": {"command": "fake-asked now"},
            "tool_use_id": "toolu_2",
        }),
    );
    let row = tab.reads(&ws, "waiting");
    assert_eq!(row["state"]["on"]["on"], "permission", "{row}");
    assert_eq!(row["state"]["on"]["tool"], "Bash", "{row}");
    let waiting = waiting_in_a_terminal(&ws);
    assert_eq!(waiting.len(), 1, "{waiting:?}");
    assert_eq!(waiting[0]["key"], tab.session.as_str());
    assert_eq!(waiting[0]["waiting"]["words"], "permission: Bash");
    assert_eq!(waiting[0]["waiting"]["workstream"], checkout.as_str());
    assert_eq!(
        waiting[0]["needs_action"],
        json!([]),
        "answered in the terminal, never here"
    );
    // Answered at the prompt: the tool ran, and the wait is over.
    tab.reports(
        &ws,
        "PostToolUse",
        json!({"tool_name": "Bash", "tool_use_id": "toolu_2"}),
    );
    tab.reads(&ws, "thinking");
    assert_eq!(waiting_in_a_terminal(&ws), Vec::<Value>::new());

    // --- the person answers in the tab, and no hook says so --------------------
    // The hooks say when a dialog shows and when a tool ran — never how the
    // person answered. The terminal that shows the dialog does, through the
    // host's own door: the wait is over at once, and the row goes back to the
    // call it already announced; the harness's next word corrects it.
    assert_eq!(
        tab.asks_before(
            &ws,
            "Bash",
            "toolu_4",
            json!({"command": "fake-asked again"})
        )
        .expect("a verdict")["hookSpecificOutput"]["permissionDecision"],
        "ask"
    );
    tab.reports(
        &ws,
        "PermissionRequest",
        json!({"tool_name": "Bash", "tool_input": {"command": "fake-asked again"}, "tool_use_id": "toolu_4"}),
    );
    tab.reads(&ws, "waiting");
    assert_eq!(waiting_in_a_terminal(&ws).len(), 1);
    let (status, _) = tab.host(&ws, "answered", Some(json!({})));
    assert_eq!(status, 200);
    let row = tab.reads(&ws, "running");
    assert_eq!(row["state"]["tool"], "Bash", "the call it announced: {row}");
    assert_eq!(
        waiting_in_a_terminal(&ws),
        Vec::<Value>::new(),
        "answered, so no longer the Inbox's"
    );
    // Claude Code refused it after all: the call is closed, nothing waits.
    tab.reports(
        &ws,
        "PermissionDenied",
        json!({"tool_name": "Bash", "tool_use_id": "toolu_4"}),
    );
    tab.reads(&ws, "thinking");

    // --- two calls of one name, told apart by their ids -----------------------------
    tab.reports(
        &ws,
        "PreToolUse",
        json!({"tool_name": "Bash", "tool_use_id": "toolu_a", "tool_input": {"command": "first"}}),
    );
    tab.reports(
        &ws,
        "PreToolUse",
        json!({"tool_name": "Bash", "tool_use_id": "toolu_b", "tool_input": {"command": "second"}}),
    );
    tab.reports(
        &ws,
        "PostToolUse",
        json!({"tool_name": "Bash", "tool_use_id": "toolu_a"}),
    );
    let row = tab.reads(&ws, "running");
    assert_eq!(
        row["state"]["args"], r#"{"command":"second"}"#,
        "the first ended, the second runs: {row}"
    );
    tab.reports(
        &ws,
        "PostToolUse",
        json!({"tool_name": "Bash", "tool_use_id": "toolu_b"}),
    );
    tab.reads(&ws, "thinking");

    // --- a sub-agent of its own, nested under it ----------------------------------
    tab.reports(
        &ws,
        "SubagentStart",
        json!({"agent_id": "agent-1", "agent_type": "explore",
               "subagent_input": {"prompt": "find where the shelf goes"}}),
    );
    let row = tab.reads(&ws, "running");
    assert_eq!(row["state"]["tool"], "sub-agent", "{row}");
    assert_eq!(row["children"][0]["name"], "explore", "{row}");
    assert_eq!(
        row["children"][0]["description"], "find where the shelf goes",
        "{row}"
    );
    // The sub-agent asks: the hand is the sub-agent's, the session keeps its
    // word, and the Inbox row says whose the wait is. Its tool then runs —
    // approved — and the sub-agent's hand drops, the Inbox row with it.
    tab.reports(
        &ws,
        "PreToolUse",
        json!({"agent_id": "agent-1", "tool_name": "Bash", "tool_use_id": "toolu_s", "tool_input": {"command": "ls shelf"}}),
    );
    tab.reports(
        &ws,
        "PermissionRequest",
        json!({"agent_id": "agent-1", "tool_name": "Bash", "tool_use_id": "toolu_s", "tool_input": {"command": "ls shelf"}}),
    );
    let row = tab.reads(&ws, "running");
    assert_eq!(
        row["state"]["tool"], "sub-agent",
        "the parent keeps its word: {row}"
    );
    assert_eq!(row["children"][0]["state"]["state"], "waiting", "{row}");
    let waiting = waiting_in_a_terminal(&ws);
    assert_eq!(waiting.len(), 1, "{waiting:?}");
    assert_eq!(waiting[0]["waiting"]["subagent"], "explore", "{waiting:?}");
    tab.reports(
        &ws,
        "PostToolUse",
        json!({"agent_id": "agent-1", "tool_name": "Bash", "tool_use_id": "toolu_s"}),
    );
    let row = tab.reads(&ws, "running");
    assert_eq!(row["children"][0]["state"]["state"], "thinking", "{row}");
    assert_eq!(waiting_in_a_terminal(&ws), Vec::<Value>::new());
    tab.reports(&ws, "SubagentStop", json!({"agent_id": "agent-1"}));
    assert_eq!(tab.reads(&ws, "thinking")["children"], json!([]));

    // --- a call the journey's rule refuses ----------------------------------------
    // The verdict is the harness's to obey, with the reason a person reads.
    let refused = tab
        .asks_before(
            &ws,
            "Bash",
            "toolu_3",
            json!({"command": "fake-refused now"}),
        )
        .expect("a verdict");
    let verdict = &refused["hookSpecificOutput"];
    assert_eq!(verdict["hookEventName"], "PreToolUse", "{refused}");
    assert_eq!(verdict["permissionDecision"], "deny", "{refused}");
    assert!(
        verdict["permissionDecisionReason"]
            .as_str()
            .is_some_and(|reason| reason.contains("refused by the journey")),
        "{refused}"
    );
    // A refused call never runs: the row does not read *running* it,
    // whichever of the two hooks the node heard first.
    tab.reads(&ws, "thinking");
    // The turn ends: whatever the harness said it was about to run, nothing
    // of the turn outlives it.
    tab.reports(&ws, "Stop", json!({}));
    tab.reads(&ws, "idle");

    // --- the secret opens one row's doors and nothing else ------------------------
    let bearer = format!("Bearer {}", tab.secret());
    let (status, _) = ws.call_with("GET", "/sessions", &[("authorization", &bearer)], None);
    assert_eq!(status, 401, "never the control plane");
    let (status, _) = ws.call(
        "POST",
        &format!("/sessions/{}/exit", tab.session),
        &[],
        Some(&json!({"code": 0})),
    );
    assert_eq!(status, 401, "and the token never a session's door");
    tab.reads(&ws, "idle");

    // --- the process ends by itself: the row is held beside its tab ---------------
    let code = ended(shell);
    let (status, answer) = tab.host(&ws, "exit", Some(json!({"code": code, "signal": null})));
    assert_eq!(status, 200, "{answer}");
    let row = tab.reads(&ws, "done");
    assert!(row.get("pid").is_none(), "{row}");
    // Hooks still in flight land after it — the start of a session among
    // them — and move nothing.
    tab.reports(&ws, "SessionStart", json!({"source": "resume"}));
    tab.reports(&ws, "UserPromptSubmit", json!({}));
    tab.asks_before(
        &ws,
        "Bash",
        "toolu_1",
        json!({"command": "fake-tool --level"}),
    );
    tab.reads(&ws, "done");

    // --- the tab closes: the row is gone, and its files with it -------------------
    let (status, answer) = tab.host(&ws, "close", None);
    assert_eq!(status, 200, "{answer}");
    assert_eq!(tab.row(&ws).0, 404);
    assert!(
        !tab.settings.exists() && !tab.settings.parent().is_some_and(|dir| dir.exists()),
        "the session's files went with it"
    );
    // A hook that fires after the close is told so, and costs its harness
    // nothing: it ends well, says why where a harness keeps a hook's words,
    // and a guard with nobody to ask gives no verdict.
    let late = tab.fires(&ws, "Stop", json!({}));
    assert!(
        String::from_utf8_lossy(&late[0].stderr).contains("404"),
        "{}",
        String::from_utf8_lossy(&late[0].stderr)
    );
    assert_eq!(
        tab.asks_before(
            &ws,
            "Bash",
            "toolu_3",
            json!({"command": "fake-refused now"})
        ),
        None
    );
    let (status, _) = tab.host(&ws, "close", None);
    assert_eq!(status, 404, "closed twice is a session nobody knows");
    ws.stop();
}

#[test]
fn a_node_that_starts_again_has_forgotten_every_terminal_and_kept_nothing_of_them() {
    let mut ws = Sealed::bare();
    let port = a_port();
    let node = ws.start_listening(port);
    let checkout = a_project(&ws, "shelf");
    let tab = Tab::open(&ws, &node, &checkout);
    let shell = a_shell();
    tab.started(&ws, &shell);
    tab.reports(&ws, "SessionStart", json!({"source": "startup"}));
    tab.reads(&ws, "idle");
    let (_, info) = ws.call("GET", "/node", &[], None);
    assert_eq!(info["live_sessions"], 1, "{info}");

    // The node goes and comes back at the address the hooks were given. The
    // tab and its process are the desktop's, and are still there.
    ws.stop();
    assert!(
        tab.settings.exists(),
        "a node that stops leaves what its tabs were handed"
    );
    assert_eq!(ws.start_listening(port), node);

    // The row is in memory, and memory is gone: nothing of it is left, the
    // files of a session nobody knows included.
    assert_eq!(tab.row(&ws).0, 404);
    let (_, all) = ws.call("GET", "/sessions", &[], None);
    assert_eq!(all["sessions"], json!([]), "{all}");
    assert!(
        !ws.data().join("run").join("interactive").exists(),
        "what the last process left is put away at the start"
    );
    // The harness runs on, and its secret opens nothing any more: each hook
    // is told so and ends well, the guard gives no verdict, the host's close
    // finds nobody.
    let told = tab.fires(&ws, "UserPromptSubmit", json!({}));
    assert!(
        String::from_utf8_lossy(&told[0].stderr).contains("404"),
        "{}",
        String::from_utf8_lossy(&told[0].stderr)
    );
    assert_eq!(
        tab.asks_before(
            &ws,
            "Bash",
            "toolu_1",
            json!({"command": "fake-tool --level"})
        ),
        None
    );
    assert_eq!(ended(shell), Some(0));
    assert_eq!(tab.host(&ws, "exit", Some(json!({"code": 0}))).0, 404);
    assert_eq!(tab.host(&ws, "close", None).0, 404);

    // A tab opened now is a session like the first.
    let again = Tab::open(&ws, &node, &checkout);
    assert_ne!(again.session, tab.session);
    assert_ne!(again.secret(), tab.secret());
    again.reads(&ws, "starting");
    assert_eq!(again.host(&ws, "close", None).0, 200);
    ws.stop();
}

#[test]
fn a_tab_whose_process_went_with_nobody_to_say_so_is_ended_by_the_node() {
    let mut ws = Sealed::bare();
    let node = ws.start_listening(a_port());
    let checkout = a_project(&ws, "shelf");
    let tab = Tab::open(&ws, &node, &checkout);
    let shell = a_shell();
    tab.started(&ws, &shell);
    tab.reports(&ws, "SessionStart", json!({"source": "startup"}));
    tab.reports(&ws, "UserPromptSubmit", json!({}));
    tab.reports(
        &ws,
        "SubagentStart",
        json!({"agent_id": "agent-1", "agent_type": "explore"}),
    );
    tab.reads(&ws, "running");

    // The desktop died and its shells with it: no exit is posted, no close.
    assert_eq!(ended(shell), Some(0));
    let row = ws.until("the node to see that the process is gone", || {
        let (state, row) = tab.state(&ws);
        (state == "failed").then_some(row)
    });
    assert_eq!(row["state"]["reason"], "the process is gone", "{row}");
    assert_eq!(row["children"], json!([]), "its sub-agents with it: {row}");
    assert!(row.get("pid").is_none(), "{row}");
    // Held, as an exit is: a desktop that comes back closes it.
    assert_eq!(tab.host(&ws, "close", None).0, 200);
    assert_eq!(tab.row(&ws).0, 404);
    ws.stop();
}

/// What a harness's account has left is read from the harness's own
/// sign-in: a machine that turned the reads off asks nobody — every harness
/// the node knows answers *off*, asked plainly or asked to read again.
#[test]
fn a_machine_that_turned_the_usage_reads_off_asks_no_account() {
    let mut ws = Sealed::bare();
    ws.start();
    let (status, known) = ws.call("GET", "/harnesses", &[], None);
    assert_eq!(status, 200, "{known}");
    let harnesses: Vec<String> = known["harnesses"]
        .as_array()
        .expect("the harnesses")
        .iter()
        .filter_map(|harness| harness["id"].as_str().map(str::to_string))
        .collect();
    assert!(
        harnesses.iter().any(|id| id == HARNESS),
        "the catalog's own: {harnesses:?}"
    );
    for id in &harnesses {
        for asked in ["", "?refresh=true"] {
            let (status, answer) =
                ws.call("GET", &format!("/harnesses/{id}/usage{asked}"), &[], None);
            assert_eq!(status, 200, "{id}{asked}: {answer}");
            assert_eq!(answer["usage"], json!({"state": "off"}), "{id}{asked}");
        }
    }
    ws.stop();
}
