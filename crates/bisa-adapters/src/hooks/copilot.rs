//! GitHub Copilot CLI's hooks → session events, and the guard's verdict as
//! its `PreToolUse` hook reads it.
//!
//! Copilot CLI loads hooks from its own config, from the repository — and
//! from a plugin, which `--plugin-dir=DIRECTORY` mounts for one launch. The
//! recipe is that plugin: two files in the session's own run folder, a
//! `plugin.json` that names a `hooks.json`, and the one word that points the
//! CLI at the folder. It writes nothing under `~/.copilot` and nothing in a
//! project, and the plugin is gone with the session's files.
//! (https://docs.github.com/en/copilot/reference/hooks-reference and
//! .../copilot-cli-reference/cli-plugin-reference, read 2026-09-30.)
//!
//! **Every event is subscribed under its PascalCase name.** Copilot CLI
//! picks the payload by the name's case: `preToolUse` delivers its own
//! camelCase shape (`toolName`, `toolArgs`), `PreToolUse` the shape Claude
//! Code defined — `hook_event_name`, `session_id`, `cwd`, `tool_name` (as
//! Claude names the tool: `Bash`, not `bash`), `tool_input`. The second is
//! what the reporter and the node's guard already read, so the CLI's guard
//! personality — which answers only a `PreToolUse` — would never be asked by
//! a camelCase key.
//!
//! What a row reads: `SessionStart`; `UserPromptSubmit` (a turn);
//! `PreToolUse`, `PostToolUse`, `PostToolUseFailure` (tools, the last with
//! the tool's own verdict); `Notification` of type `permission_prompt` or
//! `elicitation_dialog` (the harness waits for the person, in its own
//! words); `Stop` (the turn over). Not read, and so not subscribed:
//! `PermissionRequest` — it fires before Copilot's own rules and standing
//! approvals are consulted, for calls no person is ever asked about, so it
//! is no sign of a wait; `Notification`'s `agent_idle` — a *background*
//! agent's, never the session's turn; `subagentStart` — camelCase only, with
//! no id to nest a sub-agent's events under, so sub-agents are not reported.
//!
//! **The guard** is a second `PreToolUse` entry, after the reporter —
//! Copilot CLI runs an event's hooks in order. Its verdict is flat:
//! `permissionDecision` (`allow`, `deny`, `ask`), `permissionDecisionReason`,
//! and `modifiedArgs` for an input handed back with a placeholder restored.
//! A command hook that exits non-zero on `PreToolUse` denies the call
//! (Copilot CLI fails closed there), which is why both personalities of the
//! CLI exit 0 whatever happens; one that times out leaves Copilot's own
//! prompt standing.

use super::{
    hook_subagent, hook_tool_name, hook_tool_started, hook_tool_tier, shell_command, Verdict,
};
use bisa_harness::{
    InputRequest, LaunchFile, LifecycleEvent, ProgressEvent, ReportingContext, ReportingPlan,
    SessionEvent,
};
use serde_json::{json, Value};

/// The hook events the recipe subscribes to — exactly the ones a row reads,
/// each under the name that delivers the Claude-shaped payload.
const HOOKED: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "Notification",
    "Stop",
];

/// The plugin's manifest, at the root of the folder `--plugin-dir` names.
pub const PLUGIN_MANIFEST: &str = "plugin.json";

/// The hooks file the manifest points at, beside it.
pub const PLUGIN_HOOKS: &str = "hooks.json";

/// The flag that mounts a plugin folder for one launch.
pub const PLUGIN_DIR_FLAG: &str = "--plugin-dir";

/// The id a wait on the person is filed under: a terminal shows one dialog
/// at a time, so one id — what then runs, or the turn that then ends,
/// resolves it.
const WAITING: &str = "waiting";

/// One command hook: the line a shell runs, and how long it may take.
fn command_hook(argv: &[String], timeout_secs: u64) -> Value {
    json!({ "type": "command", "command": shell_command(argv), "timeoutSec": timeout_secs })
}

/// The plugin — its manifest and its hooks — and the word that mounts it:
/// what makes one terminal session of Copilot CLI report, and be judged.
pub fn reporting(ctx: &ReportingContext) -> ReportingPlan {
    let mut hooks = serde_json::Map::new();
    for event in HOOKED {
        let mut entries = vec![command_hook(&ctx.reporter, 5)];
        if *event == "PreToolUse" {
            if let Some(guard) = &ctx.guard {
                entries.push(command_hook(guard, ctx.guard_timeout_secs.max(1)));
            }
        }
        hooks.insert((*event).to_string(), Value::Array(entries));
    }
    let manifest = json!({
        "name": "bisa-session",
        "description": "Reports this terminal session to the Bisa workspace it was opened from.",
        "version": "1.0.0",
        "hooks": PLUGIN_HOOKS,
    });
    let pretty = |v: &Value| serde_json::to_string_pretty(v).unwrap_or_default();
    ReportingPlan {
        files: vec![
            LaunchFile {
                name: PLUGIN_MANIFEST.into(),
                contents: pretty(&manifest),
            },
            LaunchFile {
                name: PLUGIN_HOOKS.into(),
                contents: pretty(&json!({ "version": 1, "hooks": hooks })),
            },
        ],
        args: vec![format!("{PLUGIN_DIR_FLAG}={}", ctx.files_dir.display())],
        intercept_approval_notifications: false,
        pull: None,
    }
}

/// The guard's verdict as Copilot CLI's `PreToolUse` hook reads it: flat,
/// the input to run with as `modifiedArgs`.
pub(crate) fn guard_output(verdict: &Verdict<'_>) -> Value {
    let mut out = json!({ "permissionDecision": verdict.decision });
    if let Some(reason) = verdict.reason {
        out["permissionDecisionReason"] = Value::String(reason.to_string());
    }
    if let Some(input) = verdict.updated_input {
        out["modifiedArgs"] = input.clone();
    }
    out
}

/// One hook payload → the events it means.
pub fn translate(payload: &Value) -> Vec<SessionEvent> {
    let Some(event) = payload.get("hook_event_name").and_then(|e| e.as_str()) else {
        return Vec::new();
    };
    let parent = hook_subagent(payload);
    let progress = |p: ProgressEvent| SessionEvent::Progress(p.raised_by(parent.clone()));
    // Whatever the harness waited on the person for is over.
    let answered = || SessionEvent::Lifecycle(LifecycleEvent::InputResolved { id: WAITING.into() });
    match event {
        "SessionStart" => vec![SessionEvent::Lifecycle(LifecycleEvent::Started)],
        "UserPromptSubmit" => vec![answered(), progress(ProgressEvent::TurnStarted)],
        "PreToolUse" => {
            let name = hook_tool_name(payload);
            let input = payload.get("tool_input").cloned().unwrap_or(Value::Null);
            let tier = hook_tool_tier(&name);
            vec![answered(), progress(hook_tool_started(name, tier, &input))]
        }
        "PostToolUse" | "PostToolUseFailure" => vec![
            answered(),
            progress(ProgressEvent::ToolEnded {
                name: hook_tool_name(payload),
                ok: event == "PostToolUse",
            }),
        ],
        // The harness shows a dialog and waits: a permission, or a question
        // of the agent's. The notification carries the words a person reads
        // and no tool — so it is asked as those words, never as a call the
        // platform would have to invent.
        "Notification" => {
            let kind = payload
                .get("notification_type")
                .and_then(|t| t.as_str())
                .unwrap_or_default();
            match kind {
                "permission_prompt" | "elicitation_dialog" => {
                    let words = ["message", "title"]
                        .iter()
                        .filter_map(|key| payload.get(key).and_then(|w| w.as_str()))
                        .find(|w| !w.trim().is_empty())
                        .unwrap_or(kind);
                    let request = InputRequest::question(WAITING, words, Vec::new())
                        .raised_by(parent.clone());
                    vec![SessionEvent::Lifecycle(LifecycleEvent::InputRequested {
                        request,
                    })]
                }
                _ => Vec::new(),
            }
        }
        "Stop" => vec![answered(), progress(ProgressEvent::TurnEnded)],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_core::ToolTier;
    use bisa_harness::InputKind;

    fn ctx(guard: Option<Vec<String>>) -> ReportingContext {
        ReportingContext {
            session: "01S".into(),
            reporter: vec![
                "/bin/bisa".into(),
                "session".into(),
                "report".into(),
                "--harness".into(),
                "copilot".into(),
            ],
            files_dir: "/ws/run/interactive/01S".into(),
            port: None,
            guard,
            guard_timeout_secs: 35,
        }
    }

    fn guard() -> Vec<String> {
        vec![
            "/bin/bisa".into(),
            "session".into(),
            "guard".into(),
            "--harness".into(),
            "copilot".into(),
        ]
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

    fn file(plan: &ReportingPlan, name: &str) -> Value {
        let file = plan
            .files
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("no {name}"));
        serde_json::from_str(&file.contents).unwrap()
    }

    #[test]
    fn the_recipe_is_a_plugin_of_two_flat_files_and_the_one_word_that_mounts_it() {
        let plan = reporting(&ctx(None));
        assert_eq!(
            plan.files
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>(),
            ["plugin.json", "hooks.json"],
            "flat names: the session's folder is the plugin"
        );
        assert!(plan.files.iter().all(|f| !f.name.contains('/')));
        assert_eq!(
            plan.args,
            ["--plugin-dir=/ws/run/interactive/01S"],
            "one word, so nothing after it is read as the folder"
        );
        assert!(!plan.intercept_approval_notifications && plan.pull.is_none());

        let manifest = file(&plan, "plugin.json");
        assert_eq!(manifest["name"], "bisa-session");
        assert_eq!(
            manifest["hooks"], "hooks.json",
            "the manifest names the hooks file beside it"
        );
    }

    #[test]
    fn every_event_a_row_reads_runs_the_reporter_under_its_pascal_case_name() {
        let plan = reporting(&ctx(None));
        let hooks = file(&plan, "hooks.json");
        assert_eq!(hooks["version"], 1);
        let subscribed = hooks["hooks"].as_object().unwrap();
        let mut names: Vec<&str> = subscribed.keys().map(String::as_str).collect();
        names.sort_unstable();
        assert_eq!(
            names,
            [
                "Notification",
                "PostToolUse",
                "PostToolUseFailure",
                "PreToolUse",
                "SessionStart",
                "Stop",
                "UserPromptSubmit"
            ]
        );
        for (event, entries) in subscribed {
            // A camelCase key would deliver another payload, and the guard
            // would never be asked.
            assert!(
                event.starts_with(|c: char| c.is_ascii_uppercase()),
                "{event}"
            );
            let entries = entries.as_array().unwrap();
            assert_eq!(entries.len(), 1, "{event}: the reporter alone");
            assert_eq!(entries[0]["type"], "command");
            assert_eq!(
                entries[0]["command"],
                "'/bin/bisa' 'session' 'report' '--harness' 'copilot'"
            );
            assert_eq!(entries[0]["timeoutSec"], 5);
        }
        // Never the events that say nothing a row reads.
        for unread in [
            "PermissionRequest",
            "SubagentStop",
            "SessionEnd",
            "preToolUse",
        ] {
            assert!(subscribed.get(unread).is_none(), "{unread}");
        }
    }

    #[test]
    fn the_guard_is_a_second_pre_tool_use_entry_with_its_own_time_and_only_there() {
        let plan = reporting(&ctx(Some(guard())));
        let hooks = file(&plan, "hooks.json");
        let pre = hooks["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pre.len(), 2, "the reporter, then the guard");
        assert_eq!(
            pre[0]["command"],
            "'/bin/bisa' 'session' 'report' '--harness' 'copilot'"
        );
        assert_eq!(
            pre[1]["command"],
            "'/bin/bisa' 'session' 'guard' '--harness' 'copilot'"
        );
        assert_eq!(pre[1]["timeoutSec"], 35);
        for (event, entries) in hooks["hooks"].as_object().unwrap() {
            if event != "PreToolUse" {
                assert_eq!(entries.as_array().unwrap().len(), 1, "{event}");
            }
        }
        // A deadline of nothing is still a second: a hook with no time never
        // hears the verdict.
        let mut instant = ctx(Some(guard()));
        instant.guard_timeout_secs = 0;
        let hooks = file(&reporting(&instant), "hooks.json");
        assert_eq!(hooks["hooks"]["PreToolUse"][1]["timeoutSec"], 1);
    }

    #[test]
    fn a_turn_a_tool_and_a_stop_read_as_thinking_running_and_idle() {
        assert!(matches!(
            translate(&hook("SessionStart", json!({"source": "startup"})))[..],
            [SessionEvent::Lifecycle(LifecycleEvent::Started)]
        ));
        assert!(matches!(
            translate(&hook("UserPromptSubmit", json!({"prompt": "go"})))[1],
            SessionEvent::Progress(ProgressEvent::TurnStarted)
        ));
        // Copilot names a tool as Claude Code does under this event name.
        let started = translate(&hook(
            "PreToolUse",
            json!({"tool_name": "Bash", "tool_input": {"command": "ls"}}),
        ));
        match &started[1] {
            SessionEvent::Progress(ProgressEvent::ToolStarted {
                name,
                tier,
                args_summary,
            }) => {
                assert_eq!(name, "Bash");
                assert_eq!(*tier, ToolTier::Exec);
                assert!(args_summary.contains("ls"), "{args_summary}");
            }
            other => panic!("{other:?}"),
        }
        let read = translate(&hook(
            "PreToolUse",
            json!({"tool_name": "Read", "tool_input": {"path": "a.rs"}}),
        ));
        assert!(matches!(
            &read[1],
            SessionEvent::Progress(ProgressEvent::ToolStarted {
                tier: ToolTier::Read,
                ..
            })
        ));
        assert!(matches!(
            &translate(&hook("PostToolUse", json!({"tool_name": "Bash"})))[1],
            SessionEvent::Progress(ProgressEvent::ToolEnded { name, ok: true }) if name == "Bash"
        ));
        assert!(matches!(
            &translate(&hook("PostToolUseFailure", json!({"tool_name": "Bash"})))[1],
            SessionEvent::Progress(ProgressEvent::ToolEnded { ok: false, .. })
        ));
        assert!(matches!(
            translate(&hook("Stop", json!({"stop_reason": "end_turn"})))[1],
            SessionEvent::Progress(ProgressEvent::TurnEnded)
        ));
    }

    #[test]
    fn a_dialog_waits_on_the_person_in_the_harnesss_own_words_until_something_moves() {
        for kind in ["permission_prompt", "elicitation_dialog"] {
            let asked = translate(&hook(
                "Notification",
                json!({
                    "notification_type": kind,
                    "title": "Permission needed",
                    "message": "Copilot wants to run: git push"
                }),
            ));
            match &asked[..] {
                [SessionEvent::Lifecycle(LifecycleEvent::InputRequested { request })] => {
                    assert_eq!(request.id, "waiting");
                    assert!(matches!(
                        &request.kind,
                        InputKind::Question { text, options }
                            if text == "Copilot wants to run: git push" && options.is_empty()
                    ));
                }
                other => panic!("{kind}: {other:?}"),
            }
        }
        // No message: the title; neither: the kind itself, never nothing.
        let titled = translate(&hook(
            "Notification",
            json!({"notification_type": "permission_prompt", "title": "Permission needed", "message": " "}),
        ));
        assert!(matches!(
            &titled[0],
            SessionEvent::Lifecycle(LifecycleEvent::InputRequested { request })
                if matches!(&request.kind, InputKind::Question { text, .. } if text == "Permission needed")
        ));
        let bare = translate(&hook(
            "Notification",
            json!({"notification_type": "elicitation_dialog"}),
        ));
        assert!(matches!(
            &bare[0],
            SessionEvent::Lifecycle(LifecycleEvent::InputRequested { request })
                if matches!(&request.kind, InputKind::Question { text, .. } if text == "elicitation_dialog")
        ));

        // What then runs, ends, is said or stops resolves it.
        for (event, extra) in [
            ("PreToolUse", json!({"tool_name": "Bash", "tool_input": {}})),
            ("PostToolUse", json!({"tool_name": "Bash"})),
            ("PostToolUseFailure", json!({"tool_name": "Bash"})),
            ("UserPromptSubmit", json!({"prompt": "never mind"})),
            ("Stop", json!({})),
        ] {
            assert!(
                matches!(
                    &translate(&hook(event, extra))[0],
                    SessionEvent::Lifecycle(LifecycleEvent::InputResolved { id }) if id == "waiting"
                ),
                "{event}"
            );
        }
    }

    #[test]
    fn what_says_nothing_a_row_reads_is_nothing() {
        // A background agent going idle is not the session's turn ending, and
        // a finished shell is not a wait.
        for kind in ["agent_idle", "agent_completed", "shell_completed", ""] {
            assert!(
                translate(&hook("Notification", json!({"notification_type": kind}))).is_empty(),
                "{kind}"
            );
        }
        for event in [
            "PermissionRequest",
            "SubagentStop",
            "SessionEnd",
            "ErrorOccurred",
            "PreCompact",
        ] {
            assert!(translate(&hook(event, json!({}))).is_empty(), "{event}");
        }
        // The camelCase payload names no event this way: nothing is read.
        assert!(translate(&json!({"sessionId": "s", "toolName": "bash"})).is_empty());
    }

    #[test]
    fn a_verdict_is_flat_and_carries_only_what_was_said() {
        let deny = Verdict {
            decision: "deny",
            reason: Some("refused by a rule"),
            updated_input: None,
        };
        assert_eq!(
            guard_output(&deny),
            json!({ "permissionDecision": "deny", "permissionDecisionReason": "refused by a rule" })
        );
        let restored = json!({ "command": "echo restored-fake" });
        let allow = Verdict {
            decision: "allow",
            reason: None,
            updated_input: Some(&restored),
        };
        assert_eq!(
            guard_output(&allow),
            json!({ "permissionDecision": "allow", "modifiedArgs": { "command": "echo restored-fake" } })
        );
        assert!(
            guard_output(&allow).get("hookSpecificOutput").is_none(),
            "Claude Code's nesting is Claude Code's"
        );
    }
}
