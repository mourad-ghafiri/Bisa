//! Settings, security and the log from the command line while a node runs:
//! a setting written at each scope and resolved with the scope it came from,
//! refused at a scope its key does not allow, unset and falling back, every
//! write heard on the bus; a secret from this machine's environment reaching
//! an agent as a placeholder and coming back as one; a call above a step's
//! ceiling asked of the person in the Inbox and refused, the refusal
//! remembered on the goal, and a call a rule refuses refused by nobody's
//! hand — every decision on the record and in the node's recent ones; the
//! previews; the log's folder and the paths.

use super::sealed::{Sealed, AGENT_HARNESS};
use serde_json::{json, Value};

const TOKEN: &str = "svc-0000-not-a-real-token-abcdef";

fn origin(setting: &Value) -> String {
    setting["origin"]
        .as_str()
        .map(str::to_lowercase)
        .or_else(|| serde_json::to_string(&setting["origin"]).ok())
        .unwrap_or_default()
}

/// The decisions the node remembers, oldest first.
fn recent(ws: &Sealed) -> Vec<Value> {
    let status = ws.json(&["security", "status"]);
    let mut decisions = status["recent"].as_array().cloned().unwrap_or_default();
    decisions.reverse();
    decisions
}

#[test]
fn a_setting_is_written_by_scope_resolved_by_origin_and_refused_where_its_key_forbids() {
    let mut ws = Sealed::bare();
    ws.start();
    let listening = ws.listen();
    let project = ws.json(&["project", "new", "shelf"])["project"]["id"]
        .as_str()
        .expect("the project")
        .to_string();

    let before = ws.json(&["settings", "get", "editor.tab_size"]);
    assert_eq!(origin(&before["setting"]), "default", "{before}");
    let default = before["setting"]["value"].clone();

    let at_workspace = ws.json(&["settings", "set", "workspace", "editor.tab_size", "2"]);
    assert_eq!(at_workspace["setting"]["value"], 2, "{at_workspace}");
    assert_eq!(origin(&at_workspace["setting"]), "workspace");
    let at_project = ws.json(&[
        "settings",
        "set",
        "project",
        "editor.tab_size",
        "8",
        "--project",
        &project,
    ]);
    assert_eq!(at_project["setting"]["value"], 8, "{at_project}");
    assert_eq!(origin(&at_project["setting"]), "project");
    // Nearest wins: the project's for the project, the workspace's elsewhere.
    let for_the_project = ws.json(&["settings", "get", "editor.tab_size", "--project", &project]);
    assert_eq!(for_the_project["setting"]["value"], 8);
    let elsewhere = ws.json(&["settings", "get", "editor.tab_size"]);
    assert_eq!(elsewhere["setting"]["value"], 2);
    assert_eq!(origin(&elsewhere["setting"]), "workspace");

    // A key its scope does not admit is refused, naming the scopes it does.
    let refused = ws.bisa(&[
        "settings",
        "set",
        "project",
        "editor.font_size",
        "14",
        "--project",
        &project,
    ]);
    assert!(!refused.status.success());
    let words = String::from_utf8_lossy(&refused.stderr).to_string();
    assert!(
        words.contains("allowed: machine") && !words.contains("Machine]"),
        "the allowed scope, in the word a person types: {words}"
    );
    assert!(!words.contains("another engine holds"), "{words}");
    let unmoved = ws.json(&["settings", "get", "editor.font_size", "--project", &project]);
    assert_eq!(origin(&unmoved["setting"]), "default", "{unmoved}");
    // And a key nobody registered.
    let unknown = ws.bisa(&["settings", "set", "workspace", "editor.nobody_knows", "1"]);
    assert!(!unknown.status.success());

    // Unset falls back to the next layer, then the default.
    let fell = ws.json(&[
        "settings",
        "unset",
        "project",
        "editor.tab_size",
        "--project",
        &project,
    ]);
    assert_eq!(fell["setting"]["value"], 2, "{fell}");
    assert_eq!(origin(&fell["setting"]), "workspace");
    let fell = ws.json(&["settings", "unset", "workspace", "editor.tab_size"]);
    assert_eq!(fell["setting"]["value"], default, "{fell}");
    assert_eq!(origin(&fell["setting"]), "default");

    // The machine's own, and the group shown.
    let machine = ws.json(&[
        "settings",
        "set",
        "machine",
        "appearance.language",
        "system",
    ]);
    assert_eq!(origin(&machine["setting"]), "machine", "{machine}");
    let shown = ws.json(&["settings", "show", "--group", "editor"]);
    let keys: Vec<&str> = shown["settings"]
        .as_array()
        .expect("the rows")
        .iter()
        .filter_map(|r| r["key"].as_str())
        .collect();
    assert!(
        keys.iter().all(|k| k.starts_with("editor.")) && keys.contains(&"editor.tab_size"),
        "{keys:?}"
    );
    let registry = ws.json(&["settings", "registry"]);
    assert!(
        registry.to_string().contains("editor.tab_size"),
        "the registry names every key: {}",
        registry.to_string().len()
    );

    // Every write was heard on the bus, with its scope and its keys.
    let heard = ws.until("the settings changes to be heard", || {
        let changes: Vec<(String, Vec<String>)> = listening
            .heard()
            .frames
            .iter()
            .filter(|frame| frame["stream"] == "engine")
            .map(|frame| &frame["payload"]["payload"])
            .filter(|said| said["type"] == "settings_changed")
            .map(|said| {
                (
                    said["scope"].as_str().unwrap_or_default().to_string(),
                    said["keys"]
                        .as_array()
                        .map(|k| {
                            k.iter()
                                .filter_map(|v| v.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default(),
                )
            })
            .collect();
        (changes.len() >= 5).then_some(changes)
    });
    assert!(
        heard.contains(&("workspace".to_string(), vec!["editor.tab_size".to_string()]))
            && heard.contains(&("project".to_string(), vec!["editor.tab_size".to_string()]))
            && heard.contains(&(
                "machine".to_string(),
                vec!["appearance.language".to_string()]
            )),
        "{heard:?}"
    );
    ws.stop();
}

#[test]
fn a_secret_reaches_an_agent_as_a_placeholder_and_a_call_above_the_ceiling_is_the_persons_to_refuse(
) {
    let mut ws = Sealed::with_script(&json!({ "turns": [
        // The conversation: the agent repeats what it was told, placeholder
        // and all.
        {
            "scope": "conversation", "when": "the service token", "times": 1,
            "say": ["Noted: {{prompt.last}}"],
        },
        // The work item: a command above a read ceiling, asked; then a read
        // a rule refuses, asked too; then the same command again — which a
        // remembered answer settles; then the result.
        {
            "scope": "work_item", "times": 1,
            "tools": [
                { "name": "Bash", "own": "command", "ask": { "kind": "execute" },
                  "arguments": { "command": "echo hello" }, "leaves": [] },
                { "name": "Read", "own": "command", "ask": { "kind": "read" },
                  "arguments": { "file_path": "~/.ssh/known_hosts" }, "leaves": [] },
                { "name": "Bash", "own": "command", "ask": { "kind": "execute" },
                  "arguments": { "command": "echo hello" }, "leaves": [] },
                { "name": "yield_result", "arguments": { "output": { "done": true } } },
            ],
            "say": ["Done what I could."],
        },
    ]}));
    ws.start_in(&[("MY_SERVICE_TOKEN", TOKEN)]);
    let listening = ws.listen();

    // --- the redactor: a secret of this machine never reaches the model --------
    let status = ws.json(&["security", "status"]);
    assert_eq!(status["redactor_enabled"], true, "{status}");
    assert!(
        status["env_detectors"].as_u64().unwrap_or_default() >= 1,
        "the environment's own token is a detector: {status}"
    );
    assert!(
        !status.to_string().contains(TOKEN),
        "the status names the variable, never its value"
    );
    let preview = ws.json(&["security", "try", "--text", &format!("the key is {TOKEN}")]);
    let previewed = preview.to_string();
    assert!(
        previewed.contains("«secret:env:MY_SERVICE_TOKEN:") && !previewed.contains(TOKEN),
        "{preview}"
    );

    let scout = ws.json(&[
        "agent",
        "add",
        "--name",
        "Scout",
        "--prompt",
        "You repeat what you are told.",
        "--harness",
        AGENT_HARNESS,
    ])["agent"]["id"]
        .as_str()
        .expect("the agent")
        .to_string();
    let talk = ws.json(&["conversation", "new", "workspace", "--title", "Keys"]);
    let conversation = talk["conversation"]["id"]
        .as_str()
        .expect("the conversation")
        .to_string();
    ws.ok(&[
        "conversation",
        "post",
        &conversation,
        &format!("the service token is {TOKEN}"),
        "--mention",
        &scout,
    ]);
    let prompt = ws.until("the agent to be told", || {
        ws.recorded("prompt")
            .into_iter()
            .find(|p| p["agent"] == scout.as_str())
    });
    let told = prompt["text"].as_str().unwrap_or_default();
    assert!(
        !told.contains(TOKEN),
        "the value never reaches the model: {told}"
    );
    assert!(
        told.contains("«secret:env:MY_SERVICE_TOKEN:"),
        "a placeholder stands where it was: {told}"
    );
    // What the agent says back keeps the placeholder, and the record holds
    // the value nowhere the agent could read it.
    let reply = ws.until("the agent's reply", || {
        ws.json(&["conversation", "show", &conversation])["messages"]
            .as_array()?
            .iter()
            .find(|m| {
                m["content"]
                    .as_str()
                    .is_some_and(|c| c.starts_with("Noted:"))
            })
            .cloned()
    });
    let said = reply["content"].as_str().unwrap_or_default();
    assert!(
        said.contains("«secret:env:MY_SERVICE_TOKEN:") && !said.contains(TOKEN),
        "{said}"
    );
    let redactions = listening
        .heard()
        .frames
        .iter()
        .filter(|frame| frame["stream"] == "engine")
        .filter(|frame| frame["payload"]["payload"]["type"] == "redacted")
        .count();
    assert!(
        redactions >= 1,
        "every redaction is said on the bus, never which"
    );

    // --- the guard: above the ceiling, the person; under a rule, nobody ----------
    let workflow = ws.json(&[
        "workflow",
        "new",
        "--from",
        &ws.file(
            "read-only.json",
            &json!({
                "name": "Read only",
                "steps": [
                    { "id": "look", "name": "Look", "kind": "agent", "instructions": "look around",
                      "assignee": { "agent": scout }, "tier_ceiling": "read", "then": ["finish"] },
                    { "id": "finish", "name": "Done", "kind": "end", "finish": "done" },
                ],
            })
            .to_string(),
        )
        .to_string_lossy(),
    ])["workflow"]["id"]
        .as_str()
        .expect("the workflow")
        .to_string();
    let goal = ws.json(&[
        "new",
        "look around",
        "--mode",
        "manual",
        "--workflow",
        &workflow,
    ])["goal"]
        .as_str()
        .expect("the goal")
        .to_string();
    let mut following = ws.begin("following", &["run", &goal]);
    // The command is above `read`: the person is asked, in the Inbox.
    let ask = ws.until("the question in the Inbox", || {
        ws.json(&["inbox"])["rows"]
            .as_array()?
            .iter()
            .filter(|row| row["key"] == goal.as_str())
            .flat_map(|row| row["needs_action"].as_array().cloned().unwrap_or_default())
            .find(|ask| ask["gate_kind"] == "escalation")
    });
    assert!(
        ask["question"]
            .as_str()
            .unwrap_or_default()
            .contains("Bash"),
        "the question names the tool: {ask}"
    );
    assert!(ask["gate_id"].is_string(), "a live gate: {ask}");
    ws.ok(&["approve", &goal, "--no", "--rationale", "not on my machine"]);
    ws.until("the refusal to reach the agent", || {
        (!ws.recorded("tool_refused").is_empty()).then_some(())
    });
    // The read under `~/.ssh` is a rule's refusal: nobody is asked.
    // The second command is the person's earlier answer, remembered.
    ws.until("the step to end", || {
        let status = ws.json(&["status", &goal]);
        (status["status"] == "done").then_some(())
    });
    let followed = ws.until("the verb that followed the run to end with it", || {
        following.ended()
    });
    assert!(followed.success(), "{followed:?}");
    let refusals = ws.recorded("tool_refused");
    assert_eq!(
        refusals.len(),
        3,
        "three calls, three refusals, one question: {refusals:?}"
    );
    let asks: Vec<Value> = ws.json(&["inbox"])["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .filter(|row| row["key"] == goal.as_str())
        .flat_map(|row| row["needs_action"].as_array().cloned().unwrap_or_default())
        .collect();
    assert!(asks.is_empty(), "nothing is left asked: {asks:?}");

    let decisions = recent(&ws);
    let of_the_goal: Vec<&Value> = decisions
        .iter()
        .filter(|d| d["home"]["goal"] == goal.as_str() || d["home"].to_string().contains(&goal))
        .collect();
    let words = |d: &Value| {
        (
            d["tool"].as_str().unwrap_or_default().to_string(),
            d["verdict"].as_str().unwrap_or_default().to_string(),
            d["by"].as_str().unwrap_or_default().to_string(),
            d["rule"].as_str().unwrap_or_default().to_string(),
        )
    };
    let summary: Vec<_> = of_the_goal.iter().map(|d| words(d)).collect();
    assert!(
        summary.contains(&(
            "Bash".into(),
            "denied".into(),
            "person".into(),
            String::new()
        )),
        "the person's refusal: {summary:?}"
    );
    assert!(
        summary.contains(&(
            "Read".into(),
            "denied".into(),
            "rule".into(),
            "ssh_dir".into()
        )),
        "the rule's refusal: {summary:?}"
    );
    assert!(
        of_the_goal.iter().any(|d| d["tool"] == "Bash"
            && d["by"] == "person"
            && d["reason"]
                .as_str()
                .is_some_and(|r| r.contains("remembered"))),
        "the second command settled by what was answered: {summary:?}"
    );
    // The journal carries the same facts, and no secret.
    let journal = ws.json(&["log", &goal]).to_string();
    assert!(
        journal.contains("ssh_dir"),
        "the rule's refusal is on the record"
    );
    assert!(!journal.contains(TOKEN));

    // --- the guard's preview: judged, never run ----------------------------------
    let denied = ws.json(&[
        "security",
        "try",
        "--tool",
        "Read",
        "--path",
        "~/.ssh/known_hosts",
    ]);
    // A preview answers the rule's action — what the guard would do — and
    // a decision on the record says what happened (`denied`).
    assert_eq!(denied["verdict"], "deny", "{denied}");
    assert_eq!(denied["rule"], "ssh_dir", "{denied}");
    // A path no rule speaks of falls through to the step's ceiling.
    let unruled = ws.json(&["security", "try", "--tool", "Read", "--path", "README.md"]);
    assert_eq!(unruled["verdict"], "fallthrough", "{unruled}");
    assert!(unruled.get("rule").is_none(), "{unruled}");

    // --- the log's folder and the paths ----------------------------------------------
    let paths = ws.json(&["paths"]);
    assert_eq!(
        paths["data_dir"].as_str().map(std::path::PathBuf::from),
        Some(ws.data()),
        "{paths}"
    );
    let logs = ws.json(&["logs"]);
    let families: Vec<&str> = logs["families"]
        .as_array()
        .expect("the families")
        .iter()
        .filter_map(|f| f["process"].as_str())
        .collect();
    assert!(
        families.contains(&"node") && families.contains(&"cli"),
        "the daemon's and the command line's own files: {families:?}"
    );
    assert!(
        std::path::Path::new(logs["dir"].as_str().expect("the folder")).starts_with(ws.data()),
        "{logs}"
    );
    ws.stop();
}
