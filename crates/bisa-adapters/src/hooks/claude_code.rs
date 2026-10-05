//! Claude Code's hooks → session events.
//!
//! The recipe is a one-session settings file passed with `--settings`, whose
//! `hooks` block runs the reporter at the lifecycle points a roster row is
//! made of. The file merges with the person's own settings for that launch and
//! writes nothing anywhere; hooks add, they never replace.
//!
//! Every hook payload carries `hook_event_name`, `session_id`, `cwd` and
//! `transcript_path`; a hook fired inside a sub-agent also carries `agent_id`,
//! which is what nests its events under the right child. `Stop` does not fire
//! on a person's interrupt (the reference says so; an API error fires
//! `StopFailure` instead), so a turn a person cut short ends on the
//! `Notification` that says the harness waits for input, or on the next
//! prompt (the engine closes an open turn when a new one starts).
//!
//! **No hook says how a person answered a permission dialog** (the reference,
//! read 2026-10-05: `PermissionRequest` fires when the dialog is about to
//! show; `PostToolUse` only after a tool ran; a call the person declined
//! fires neither). So a wait raised here ends on the call's `tool_use_id` —
//! its `PostToolUse` or `PostToolUseFailure`, or `PermissionDenied` when
//! Claude Code itself refused it — on the turn's end, or when the person's
//! answer in the tab is told to the node by the terminal that shows it. A
//! `SessionStart` whose `source` is `compact` is the same session going on
//! and says nothing; one whose `source` is `clear` ends the turn.
//!
//! When the context names a guard, `PreToolUse` runs a second hook beside the
//! reporter — Claude Code runs an event's hooks at once, not in order
//! (https://code.claude.com/docs/en/hooks, read 2026-09-30: "All matching
//! hooks run in parallel"), so neither waits on the other and the node
//! takes them in either order: the guard personality, which waits for the node's verdict and
//! prints Claude Code's `permissionDecision` — `deny` with the reason, `ask`
//! for the person at the keyboard, or `allow` with the input the node hands
//! back (a placeholder restored). It is the only hook allowed to be slow, and
//! its timeout is the node's classifier deadline plus a margin; when it says
//! nothing, Claude Code's own prompt stands.

use super::{
    args_summary, hook_subagent, hook_subagent_ended, hook_subagent_started, hook_tool_id,
    hook_tool_name, hook_tool_started, hook_tool_tier, hook_tool_use_id, shell_command, Verdict,
};
use bisa_harness::{
    InputRequest, LaunchFile, LifecycleEvent, ProgressEvent, ReportingContext, ReportingPlan,
    SessionEvent,
};
use serde_json::{json, Value};

/// The hook events the recipe subscribes to — exactly the ones a row reads.
const HOOKED: &[&str] = &[
    "SessionStart",
    "PostModelSwitch",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "PermissionRequest",
    "PermissionDenied",
    "Stop",
    "StopFailure",
    "Notification",
    "SubagentStart",
    "SubagentStop",
];

/// A model as a hook names it: a string id, or an object with an `id`.
fn model_of(value: Option<&Value>) -> Option<String> {
    let v = value?;
    let id = v
        .as_str()
        .or_else(|| v.get("id").and_then(|i| i.as_str()))?;
    (!id.is_empty()).then(|| id.to_string())
}

/// The `--settings` file and the arguments that make one session report.
pub fn reporting(ctx: &ReportingContext) -> ReportingPlan {
    let command = shell_command(&ctx.reporter);
    let mut hooks = serde_json::Map::new();
    for event in HOOKED {
        let mut entries = vec![json!({ "type": "command", "command": command, "timeout": 5 })];
        if *event == "PreToolUse" {
            if let Some(guard) = &ctx.guard {
                entries.push(json!({
                    "type": "command",
                    "command": shell_command(guard),
                    "timeout": ctx.guard_timeout_secs.max(1),
                }));
            }
        }
        hooks.insert((*event).to_string(), json!([{ "hooks": entries }]));
    }
    let settings = LaunchFile {
        name: "claude-settings.json".into(),
        contents: serde_json::to_string_pretty(&json!({ "hooks": hooks })).unwrap_or_default(),
    };
    let path = ctx.files_dir.join(&settings.name).display().to_string();
    ReportingPlan {
        files: vec![settings],
        args: vec!["--settings".into(), path],
        intercept_approval_notifications: false,
        pull: None,
    }
}

/// The guard's verdict as Claude Code's `PreToolUse` hook reads it: nested
/// under `hookSpecificOutput`, the input to run with as `updatedInput`.
pub(crate) fn guard_output(verdict: &Verdict<'_>) -> Value {
    let mut specific = json!({
        "hookEventName": "PreToolUse",
        "permissionDecision": verdict.decision,
    });
    if let Some(reason) = verdict.reason {
        specific["permissionDecisionReason"] = Value::String(reason.to_string());
    }
    if let Some(input) = verdict.updated_input {
        specific["updatedInput"] = input.clone();
    }
    json!({ "hookSpecificOutput": specific })
}

/// One hook payload → the events it means.
pub fn translate(payload: &Value) -> Vec<SessionEvent> {
    let Some(event) = payload.get("hook_event_name").and_then(|e| e.as_str()) else {
        return Vec::new();
    };
    let parent = hook_subagent(payload);
    let progress = |p: ProgressEvent| SessionEvent::Progress(p.raised_by(parent.clone()));
    match event {
        // Started — and the model, when the hook carries it (Claude Code does
        // not always include it; a string, or an object naming an `id`). A
        // compaction mid-turn is the same session going on, not a start; a
        // `/clear` is the turn over.
        "SessionStart" => match payload.get("source").and_then(|s| s.as_str()) {
            Some("compact") => Vec::new(),
            Some("clear") => vec![progress(ProgressEvent::TurnEnded)],
            _ => {
                let mut events = vec![SessionEvent::Lifecycle(LifecycleEvent::Started)];
                if let Some(model) = model_of(payload.get("model")) {
                    events.push(SessionEvent::Progress(ProgressEvent::ModelChanged {
                        model,
                    }));
                }
                events
            }
        },
        // The person switched models in the terminal: the row follows.
        "PostModelSwitch" => model_of(payload.get("to_model"))
            .map(|model| {
                vec![SessionEvent::Progress(ProgressEvent::ModelChanged {
                    model,
                })]
            })
            .unwrap_or_default(),
        "UserPromptSubmit" => vec![progress(ProgressEvent::TurnStarted)],
        // A `Stop` fires inside a sub-agent too, with its `agent_id`: that one
        // ends the sub-agent's turn, never the session's. `StopFailure` is a
        // turn ended by an API error — over, not the session's failure.
        "Stop" | "StopFailure" => vec![progress(ProgressEvent::TurnEnded)],
        // The harness waits for a person: whatever turn was open is over —
        // the one word after an interrupt, which fires no `Stop`. A dialog an
        // MCP server or an agent raised is a wait of its own, on no tool:
        // `permission_prompt` is not, since `PermissionRequest` already said
        // it with the call's id.
        "Notification" => match payload.get("notification_type").and_then(|t| t.as_str()) {
            Some("idle_prompt") => vec![progress(ProgressEvent::TurnEnded)],
            Some(kind @ ("elicitation_dialog" | "agent_needs_input")) => {
                let text = payload
                    .get("message")
                    .and_then(|m| m.as_str())
                    .filter(|m| !m.is_empty())
                    .unwrap_or("the harness asks for your input")
                    .to_string();
                vec![SessionEvent::Lifecycle(LifecycleEvent::InputRequested {
                    request: InputRequest::question(
                        format!("notification:{kind}"),
                        text,
                        Vec::new(),
                    )
                    .raised_by(parent.clone()),
                })]
            }
            _ => Vec::new(),
        },
        "PreToolUse" => {
            let name = hook_tool_name(payload);
            let input = payload.get("tool_input").cloned().unwrap_or(Value::Null);
            match name.as_str() {
                // The sub-agent tool — `Agent` since Claude Code 2.1.63, `Task`
                // its alias — is what the `SubagentStart` hook announces.
                "Task" | "Agent" => Vec::new(),
                "AskUserQuestion" => {
                    let request = crate::claude_code::input_request(
                        &hook_tool_use_id(payload),
                        &json!({ "tool_name": name, "input": input }),
                        parent.clone(),
                    );
                    vec![SessionEvent::Lifecycle(LifecycleEvent::InputRequested {
                        request,
                    })]
                }
                _ => {
                    let tier = hook_tool_tier(&name);
                    vec![progress(hook_tool_started(payload, name, tier, &input))]
                }
            }
        }
        "PostToolUse" | "PostToolUseFailure" => {
            let name = hook_tool_name(payload);
            match name.as_str() {
                "Task" | "Agent" => Vec::new(),
                "AskUserQuestion" => vec![SessionEvent::Lifecycle(LifecycleEvent::InputResolved {
                    id: hook_tool_use_id(payload),
                })],
                _ => vec![progress(ProgressEvent::ToolEnded {
                    name,
                    ok: event == "PostToolUse",
                    id: hook_tool_id(payload),
                })],
            }
        }
        "PermissionRequest" => {
            let name = hook_tool_name(payload);
            let input = payload.get("tool_input").cloned().unwrap_or(Value::Null);
            let request = InputRequest::permission(
                hook_tool_use_id(payload),
                name.clone(),
                hook_tool_tier(&name),
                args_summary(&input),
                input,
            )
            .raised_by(parent.clone());
            vec![SessionEvent::Lifecycle(LifecycleEvent::InputRequested {
                request,
            })]
        }
        // Claude Code itself refused the call (auto mode, a rule): it will
        // not run — the wait on it is over, and so is the tool its
        // `PreToolUse` announced.
        "PermissionDenied" => {
            let name = hook_tool_name(payload);
            vec![
                SessionEvent::Lifecycle(LifecycleEvent::InputResolved {
                    id: hook_tool_use_id(payload),
                }),
                progress(ProgressEvent::ToolEnded {
                    name,
                    ok: false,
                    id: hook_tool_id(payload),
                }),
            ]
        }
        "SubagentStart" => hook_subagent_started(payload)
            .map(|e| vec![SessionEvent::Progress(e)])
            .unwrap_or_default(),
        // The hook carries the sub-agent's last words and no verdict: it left.
        "SubagentStop" => hook_subagent_ended(payload, true)
            .map(|e| vec![SessionEvent::Progress(e)])
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_core::ToolTier;
    use bisa_harness::InputKind;

    #[test]
    fn the_start_names_the_model_when_the_hook_carries_it_and_a_switch_follows() {
        let bare = translate(&hook("SessionStart", json!({"source": "startup"})));
        assert_eq!(bare.len(), 1, "no model, just started");
        assert!(matches!(
            bare[0],
            SessionEvent::Lifecycle(LifecycleEvent::Started)
        ));
        let named = translate(&hook(
            "SessionStart",
            json!({"source": "startup", "model": "claude-opus-5"}),
        ));
        assert!(
            matches!(&named[1], SessionEvent::Progress(ProgressEvent::ModelChanged { model }) if model == "claude-opus-5")
        );
        let object = translate(&hook(
            "SessionStart",
            json!({"model": {"id": "claude-sonnet-5", "display_name": "Sonnet"}}),
        ));
        assert!(
            matches!(&object[1], SessionEvent::Progress(ProgressEvent::ModelChanged { model }) if model == "claude-sonnet-5"),
            "an object names its id"
        );
        let switched = translate(&hook(
            "PostModelSwitch",
            json!({"from_model": "claude-opus-5", "to_model": "claude-sonnet-5"}),
        ));
        assert!(
            matches!(&switched[0], SessionEvent::Progress(ProgressEvent::ModelChanged { model }) if model == "claude-sonnet-5")
        );
        assert!(translate(&hook("PostModelSwitch", json!({}))).is_empty());
        let plan = reporting(&ReportingContext {
            session: "01S".into(),
            reporter: vec!["bisa".into(), "session".into(), "report".into()],
            files_dir: "/ws/run/interactive/01S".into(),
            port: None,
            guard: None,
            guard_timeout_secs: 0,
        });
        assert!(
            plan.files[0].contents.contains("\"PostModelSwitch\""),
            "the recipe subscribes the switch hook"
        );
    }

    fn hook(name: &str, extra: Value) -> Value {
        let mut v = json!({ "hook_event_name": name, "session_id": "s", "cwd": "/w" });
        if let Value::Object(map) = extra {
            for (k, val) in map {
                v[k] = val;
            }
        }
        v
    }

    fn ctx(guard: Option<Vec<String>>) -> ReportingContext {
        ReportingContext {
            session: "01S".into(),
            reporter: vec![
                "/bin/bisa".into(),
                "session".into(),
                "report".into(),
                "--harness".into(),
                "claude-code".into(),
            ],
            files_dir: "/ws/run/interactive/01S".into(),
            port: None,
            guard,
            guard_timeout_secs: 25,
        }
    }

    #[test]
    fn the_recipe_hooks_every_event_a_row_reads_and_names_the_reporter() {
        let plan = reporting(&ctx(None));
        assert_eq!(
            plan.args,
            ["--settings", "/ws/run/interactive/01S/claude-settings.json"]
        );
        let settings: Value = serde_json::from_str(&plan.files[0].contents).unwrap();
        for event in HOOKED {
            let entries = settings["hooks"][*event][0]["hooks"].as_array().unwrap();
            assert_eq!(
                entries.len(),
                1,
                "{event}: the reporter alone when nothing guards"
            );
            let command = entries[0]["command"].as_str().unwrap();
            assert!(
                command.contains("'session' 'report' '--harness' 'claude-code'"),
                "{command}"
            );
        }
        assert!(plan.pull.is_none() && !plan.intercept_approval_notifications);
    }

    #[test]
    fn a_guard_is_a_second_pre_tool_use_hook_with_its_own_timeout() {
        let guard = vec![
            "/bin/bisa".into(),
            "session".into(),
            "guard".into(),
            "--harness".into(),
            "claude-code".into(),
        ];
        let plan = reporting(&ctx(Some(guard)));
        let settings: Value = serde_json::from_str(&plan.files[0].contents).unwrap();
        let entries = settings["hooks"]["PreToolUse"][0]["hooks"]
            .as_array()
            .unwrap();
        assert_eq!(entries.len(), 2);
        assert!(
            entries[0]["command"].as_str().unwrap().contains("'report'"),
            "the reporter keeps the first slot"
        );
        assert!(entries[1]["command"]
            .as_str()
            .unwrap()
            .contains("'session' 'guard' '--harness' 'claude-code'"));
        assert_eq!(entries[1]["timeout"], 25);
        for event in HOOKED.iter().filter(|e| **e != "PreToolUse") {
            assert_eq!(
                settings["hooks"][*event][0]["hooks"]
                    .as_array()
                    .unwrap()
                    .len(),
                1,
                "{event}"
            );
        }
    }

    #[test]
    fn a_turn_a_tool_and_a_stop_read_as_thinking_running_and_idle() {
        assert!(matches!(
            translate(&hook("UserPromptSubmit", json!({})))[0],
            SessionEvent::Progress(ProgressEvent::TurnStarted)
        ));
        let started = translate(&hook(
            "PreToolUse",
            json!({"tool_name": "Bash", "tool_input": {"command": "ls"}, "tool_use_id": "t1"}),
        ));
        match &started[0] {
            SessionEvent::Progress(ProgressEvent::ToolStarted { name, tier, .. }) => {
                assert_eq!(name, "Bash");
                assert_eq!(*tier, ToolTier::Exec);
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            translate(&hook("PostToolUseFailure", json!({"tool_name": "Bash"})))[0],
            SessionEvent::Progress(ProgressEvent::ToolEnded { ok: false, .. })
        ));
        assert!(matches!(
            translate(&hook("Stop", json!({})))[0],
            SessionEvent::Progress(ProgressEvent::TurnEnded)
        ));
    }

    #[test]
    fn a_permission_and_a_question_wait_on_the_person_and_resolve() {
        let asked = translate(&hook(
            "PermissionRequest",
            json!({"tool_name": "Bash", "tool_input": {"command": "rm -rf x"}, "tool_use_id": "t2"}),
        ));
        match &asked[0] {
            SessionEvent::Lifecycle(LifecycleEvent::InputRequested { request }) => {
                assert_eq!(request.id, "t2");
                assert!(
                    matches!(&request.kind, InputKind::Permission { tool_name, .. } if tool_name == "Bash")
                );
            }
            other => panic!("{other:?}"),
        }
        let question = translate(&hook(
            "PreToolUse",
            json!({"tool_name": "AskUserQuestion", "tool_use_id": "t3", "tool_input": {"questions": [{"question": "Which db?", "options": [{"label": "pg"}]}]}}),
        ));
        match &question[0] {
            SessionEvent::Lifecycle(LifecycleEvent::InputRequested { request }) => {
                assert!(
                    matches!(&request.kind, InputKind::Question { text, options } if text == "Which db?" && options == &["pg"])
                );
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            translate(&hook("PostToolUse", json!({"tool_name": "AskUserQuestion", "tool_use_id": "t3"})))[0],
            SessionEvent::Lifecycle(LifecycleEvent::InputResolved { ref id }) if id == "t3"
        ));
        // The call's own id rides its start and its end: what ties the wait
        // asked under `t2` to the call that then runs or ends.
        assert!(matches!(
            &translate(&hook("PreToolUse", json!({"tool_name": "Bash", "tool_use_id": "t2", "tool_input": {"command": "ls"}})))[0],
            SessionEvent::Progress(ProgressEvent::ToolStarted { id: Some(id), .. }) if id == "t2"
        ));
        assert!(matches!(
            &translate(&hook("PostToolUse", json!({"tool_name": "Bash", "tool_use_id": "t2"})))[0],
            SessionEvent::Progress(ProgressEvent::ToolEnded { id: Some(id), ok: true, .. }) if id == "t2"
        ));
        // Claude Code refusing the call itself: the wait is over, the call closed.
        let denied = translate(&hook(
            "PermissionDenied",
            json!({"tool_name": "Bash", "tool_use_id": "t2", "tool_input": {"command": "rm -rf x"}}),
        ));
        assert_eq!(denied.len(), 2, "{denied:?}");
        assert!(matches!(
            &denied[0],
            SessionEvent::Lifecycle(LifecycleEvent::InputResolved { id }) if id == "t2"
        ));
        assert!(matches!(
            &denied[1],
            SessionEvent::Progress(ProgressEvent::ToolEnded { ok: false, id: Some(id), .. }) if id == "t2"
        ));
        assert!(
            HOOKED.contains(&"PermissionDenied"),
            "the recipe hooks it, or the row never hears a refusal"
        );
    }

    #[test]
    fn a_sub_agent_is_announced_once_and_its_tools_nest_under_it() {
        // The spawn tool is `Agent` on a current Claude Code and `Task` on an
        // older one; neither is a tool of the session's.
        for tool in ["Task", "Agent"] {
            assert!(
                translate(&hook(
                    "PreToolUse",
                    json!({"tool_name": tool, "tool_input": {}})
                ))
                .is_empty(),
                "{tool}"
            );
            assert!(
                translate(&hook("PostToolUse", json!({"tool_name": tool}))).is_empty(),
                "{tool}"
            );
        }
        let started = translate(&hook(
            "SubagentStart",
            json!({"agent_id": "a1", "agent_type": "explore", "subagent_input": {"prompt": "find the router"}}),
        ));
        assert!(
            matches!(&started[0], SessionEvent::Progress(ProgressEvent::SubagentStarted { id, name, description }) if id.0 == "a1" && name == "explore" && description == "find the router"),
            "the description is the prompt the reference names, `subagent_input.prompt`: {started:?}"
        );
        let nested = translate(&hook(
            "PreToolUse",
            json!({"agent_id": "a1", "tool_name": "Read", "tool_input": {"file_path": "x"}}),
        ));
        assert!(
            matches!(&nested[0], SessionEvent::Progress(ProgressEvent::Nested { parent, .. }) if parent.0 == "a1")
        );
        assert!(matches!(
            translate(&hook("SubagentStop", json!({"agent_id": "a1"})))[0],
            SessionEvent::Progress(ProgressEvent::SubagentEnded { ok: true, .. })
        ));
        let stop = translate(&hook("Stop", json!({"agent_id": "a1"})));
        assert!(
            matches!(&stop[0], SessionEvent::Progress(ProgressEvent::Nested { parent, event }) if parent.0 == "a1" && matches!(**event, ProgressEvent::TurnEnded)),
            "a Stop inside a sub-agent ends the sub-agent's turn, never the session's: {stop:?}"
        );
    }

    #[test]
    fn a_turn_the_harness_never_stopped_ends_on_the_idle_prompt_or_the_api_error() {
        // `Stop` does not fire on an interrupt: the notification that the
        // harness waits for input is the turn's end; so is `StopFailure`.
        assert!(matches!(
            translate(&hook(
                "Notification",
                json!({"notification_type": "idle_prompt"})
            ))[0],
            SessionEvent::Progress(ProgressEvent::TurnEnded)
        ));
        assert!(matches!(
            translate(&hook(
                "StopFailure",
                json!({"error_type": "rate_limit", "error_message": "Rate limit exceeded"})
            ))[0],
            SessionEvent::Progress(ProgressEvent::TurnEnded)
        ));
        assert!(
            translate(&hook(
                "Notification",
                json!({"notification_type": "permission_prompt"})
            ))
            .is_empty(),
            "a permission prompt is the `PermissionRequest` hook's word, not a turn's end"
        );
        // A dialog an MCP server or an agent raised is a wait of its own,
        // on no tool, in the notification's words.
        let dialog = translate(&hook(
            "Notification",
            json!({"notification_type": "elicitation_dialog", "message": "The db server needs a name"}),
        ));
        match &dialog[0] {
            SessionEvent::Lifecycle(LifecycleEvent::InputRequested { request }) => {
                assert_eq!(request.id, "notification:elicitation_dialog");
                assert!(
                    matches!(&request.kind, InputKind::Question { text, .. } if text == "The db server needs a name")
                );
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            &translate(&hook("Notification", json!({"notification_type": "agent_needs_input"})))[0],
            SessionEvent::Lifecycle(LifecycleEvent::InputRequested { request }) if request.id == "notification:agent_needs_input"
        ));
    }

    #[test]
    fn a_compaction_is_the_same_session_going_on_and_a_clear_ends_the_turn() {
        assert!(
            translate(&hook(
                "SessionStart",
                json!({"source": "compact", "model": "claude-opus-5"})
            ))
            .is_empty(),
            "a compaction mid-turn is no start: the turn, its tools and its waits stand"
        );
        let cleared = translate(&hook("SessionStart", json!({"source": "clear"})));
        assert_eq!(cleared.len(), 1);
        assert!(matches!(
            cleared[0],
            SessionEvent::Progress(ProgressEvent::TurnEnded)
        ));
        for source in ["startup", "resume", "fork"] {
            assert!(
                matches!(
                    translate(&hook("SessionStart", json!({"source": source})))[0],
                    SessionEvent::Lifecycle(LifecycleEvent::Started)
                ),
                "{source}"
            );
        }
    }

    #[test]
    fn what_a_row_does_not_read_is_silent() {
        assert!(translate(&hook(
            "Notification",
            json!({"notification_type": "auth_success"})
        ))
        .is_empty());
        assert!(translate(&hook("SessionEnd", json!({"end_reason": "other"}))).is_empty());
        assert!(translate(&json!({"no": "name"})).is_empty());
    }
}
