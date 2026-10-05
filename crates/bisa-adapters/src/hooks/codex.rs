//! Codex CLI's hooks → session events.
//!
//! Codex adopted the lifecycle-hook shape Claude Code defined — one command
//! per event, the payload on stdin with `hook_event_name`, `session_id`,
//! `turn_id`, `tool_name`, `tool_input`, `agent_id` inside a sub-agent — and
//! reads hook configs from its config layers: the user's, the project's, and
//! the session's. The recipe is the session layer: one `-c hooks.<Event>=…`
//! override per event a row reads, for this launch alone, writing nothing
//! anywhere. A hook not persisted in a trusted config needs the CLI's own
//! `--dangerously-bypass-hook-trust` to run — the flag's name is the
//! vendor's; the hook it lets through is this platform's reporter.
//!
//! What a row reads, from the CLI's hooks reference
//! (https://learn.chatgpt.com/docs/hooks, read 2026-10-05): `SessionStart`,
//! `UserPromptSubmit` (a turn), `PreToolUse` / `PostToolUse` (tools, each
//! with its `tool_use_id`), `PermissionRequest` (a wait — asked **after**
//! `PreToolUse`, before the tool runs, and over when that call ends, when the
//! turn ends, or when the person's answer in the tab is told to the node),
//! `SubagentStart` / `SubagentStop` (sub-agents, their events nested by
//! `agent_id`), `Stop` and `Interrupt` (the turn over). Codex fires no
//! event while the model writes, none when the person answers its prompt,
//! and no failure verdict on a tool or a sub-agent; a row reads what is
//! reported and nothing invented.

use super::{
    hook_subagent, hook_subagent_ended, hook_subagent_started, hook_tool_id, hook_tool_name,
    hook_tool_started, shell_command,
};
use bisa_core::ToolTier;
use bisa_harness::{
    InputRequest, LifecycleEvent, ProgressEvent, ReportingContext, ReportingPlan, SessionEvent,
};
use serde_json::Value;

/// The hook events the recipe subscribes to — exactly the ones a row reads.
const HOOKED: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PermissionRequest",
    "Stop",
    "Interrupt",
    "SubagentStart",
    "SubagentStop",
];

/// The CLI's flag that runs a hook the person never persisted trust for.
pub const BYPASS_HOOK_TRUST: &str = "--dangerously-bypass-hook-trust";

/// A TOML basic string.
fn toml_string(raw: &str) -> String {
    format!("\"{}\"", raw.replace('\\', "\\\\").replace('"', "\\\""))
}

/// One event's hook table, as a `-c` override: the reporter, five seconds.
fn hook_override(event: &str, command: &str) -> String {
    format!(
        "hooks.{event}=[{{hooks=[{{type=\"command\",command={},timeout=5}}]}}]",
        toml_string(command)
    )
}

/// Config overrides for one launch: the reporter behind every event a row
/// reads, on the session layer, and the CLI's leave to run it.
pub fn reporting(ctx: &ReportingContext) -> ReportingPlan {
    let reporter = shell_command(&ctx.reporter);
    let mut args = Vec::with_capacity(HOOKED.len() * 2 + 1);
    for event in HOOKED {
        args.push("-c".into());
        args.push(hook_override(event, &reporter));
    }
    args.push(BYPASS_HOOK_TRUST.into());
    ReportingPlan {
        files: Vec::new(),
        args,
        intercept_approval_notifications: false,
        pull: None,
    }
}

/// The id a permission wait is filed under: the call's own `tool_use_id`, so
/// the call that then ends resolves it; one per turn when the payload names
/// no call, so the turn that then ends does.
fn permission_id(payload: &Value) -> String {
    if let Some(id) = hook_tool_id(payload) {
        return id;
    }
    match payload.get("turn_id").and_then(|t| t.as_str()) {
        Some(turn) if !turn.is_empty() => format!("permission:{turn}"),
        _ => "permission".into(),
    }
}

/// One hook payload → the events it means.
pub fn translate(payload: &Value) -> Vec<SessionEvent> {
    let Some(event) = payload.get("hook_event_name").and_then(|e| e.as_str()) else {
        return Vec::new();
    };
    let parent = hook_subagent(payload);
    let progress = |p: ProgressEvent| SessionEvent::Progress(p.raised_by(parent.clone()));
    match event {
        "SessionStart" => {
            let mut events = vec![SessionEvent::Lifecycle(LifecycleEvent::Started)];
            if let Some(model) = payload
                .get("model")
                .and_then(|m| m.as_str())
                .filter(|m| !m.is_empty())
            {
                events.push(SessionEvent::Progress(ProgressEvent::ModelChanged {
                    model: model.to_string(),
                }));
            }
            events
        }
        "UserPromptSubmit" => vec![progress(ProgressEvent::TurnStarted)],
        // The call is announced before Codex asks about it: a start answers
        // nothing — its end, or the turn's, does.
        "PreToolUse" => {
            let name = hook_tool_name(payload);
            let input = payload.get("tool_input").cloned().unwrap_or(Value::Null);
            let tier = ToolTier::classify(&name.to_ascii_lowercase());
            vec![progress(hook_tool_started(payload, name, tier, &input))]
        }
        "PostToolUse" => vec![progress(ProgressEvent::ToolEnded {
            name: hook_tool_name(payload),
            ok: true,
            id: hook_tool_id(payload),
        })],
        "PermissionRequest" => {
            let name = hook_tool_name(payload);
            let input = payload.get("tool_input").cloned().unwrap_or(Value::Null);
            let request = InputRequest::permission(
                permission_id(payload),
                name.clone(),
                ToolTier::classify(&name.to_ascii_lowercase()),
                super::args_summary(&input),
                input,
            )
            .raised_by(parent.clone());
            vec![SessionEvent::Lifecycle(LifecycleEvent::InputRequested {
                request,
            })]
        }
        // The turn is over — finished, or interrupted by the person; a
        // permission still asked is over with it (the turn's end clears
        // every wait). Inside a sub-agent, the sub-agent's turn.
        "Stop" | "Interrupt" => vec![progress(ProgressEvent::TurnEnded)],
        "SubagentStart" => hook_subagent_started(payload)
            .map(|e| vec![SessionEvent::Progress(e)])
            .unwrap_or_default(),
        "SubagentStop" => hook_subagent_ended(payload, true)
            .map(|e| vec![SessionEvent::Progress(e)])
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_harness::InputKind;
    use serde_json::json;

    fn ctx() -> ReportingContext {
        ReportingContext {
            session: "01S".into(),
            reporter: vec![
                "/bin/bisa".into(),
                "session".into(),
                "report".into(),
                "--harness".into(),
                "codex".into(),
            ],
            files_dir: "/ws/run/interactive/01S".into(),
            port: None,
            guard: None,
            guard_timeout_secs: 0,
        }
    }

    fn hook(name: &str, extra: Value) -> Value {
        let mut v =
            json!({"hook_event_name": name, "session_id": "s", "cwd": "/w", "turn_id": "t1"});
        if let (Some(a), Some(b)) = (v.as_object_mut(), extra.as_object()) {
            for (k, val) in b {
                a.insert(k.clone(), val.clone());
            }
        }
        v
    }

    #[test]
    fn the_recipe_hooks_every_event_a_row_reads_on_the_session_layer_and_nothing_else() {
        let plan = reporting(&ctx());
        let overrides: Vec<&str> = plan
            .args
            .iter()
            .filter(|a| a.starts_with("hooks."))
            .map(|a| a.as_str())
            .collect();
        assert_eq!(overrides.len(), HOOKED.len());
        for event in HOOKED {
            let o = overrides
                .iter()
                .find(|o| o.starts_with(&format!("hooks.{event}=")))
                .unwrap_or_else(|| panic!("{event}"));
            assert!(o.contains(r#"type=\"command\""#) || o.contains("type=\"command\""));
            assert!(o.contains("'session' 'report' '--harness' 'codex'"));
            assert!(o.contains("timeout=5"));
        }
        assert_eq!(
            plan.args.last().map(String::as_str),
            Some(BYPASS_HOOK_TRUST)
        );
        assert!(
            !plan.args.iter().any(|a| a.starts_with("notify")),
            "the hooks say everything notify said"
        );
        assert!(
            !plan.intercept_approval_notifications && plan.files.is_empty() && plan.pull.is_none()
        );
    }

    #[test]
    fn a_turn_a_tool_and_a_stop_read_as_thinking_running_and_idle() {
        assert!(matches!(
            translate(&hook("UserPromptSubmit", json!({"prompt": "go"})))[0],
            SessionEvent::Progress(ProgressEvent::TurnStarted)
        ));
        let started = translate(&hook(
            "PreToolUse",
            json!({"tool_name": "shell", "tool_use_id": "c1", "tool_input": {"command": "ls"}}),
        ));
        assert_eq!(started.len(), 1, "a start answers nothing: {started:?}");
        assert!(
            matches!(&started[0], SessionEvent::Progress(ProgressEvent::ToolStarted { name, tier: ToolTier::Exec, id: Some(id), .. }) if name == "shell" && id == "c1")
        );
        assert!(matches!(
            &translate(&hook("PostToolUse", json!({"tool_name": "shell", "tool_use_id": "c1"})))[0],
            SessionEvent::Progress(ProgressEvent::ToolEnded { ok: true, id: Some(id), .. }) if id == "c1"
        ));
        let stop = translate(&hook("Stop", json!({"last_assistant_message": "done"})));
        assert_eq!(stop.len(), 1);
        assert!(matches!(
            stop[0],
            SessionEvent::Progress(ProgressEvent::TurnEnded)
        ));
        assert!(matches!(
            translate(&hook("Interrupt", json!({})))[0],
            SessionEvent::Progress(ProgressEvent::TurnEnded)
        ));
        let start = translate(&hook(
            "SessionStart",
            json!({"model": "gpt-5-codex", "source": "startup"}),
        ));
        assert!(matches!(
            start[0],
            SessionEvent::Lifecycle(LifecycleEvent::Started)
        ));
        assert!(
            matches!(&start[1], SessionEvent::Progress(ProgressEvent::ModelChanged { model }) if model == "gpt-5-codex")
        );
        assert!(translate(&hook("SessionEnd", json!({"reason": "other"}))).is_empty());
    }

    #[test]
    fn a_permission_waits_under_its_calls_id_and_nothing_here_answers_it() {
        // Codex asks after the call is announced: the wait is filed under the
        // call's own id, so the fold ends it when that call ends — or when
        // the turn does, or when the person's answer in the tab is told.
        let asked = translate(&hook(
            "PermissionRequest",
            json!({"tool_name": "shell", "tool_use_id": "c7", "tool_input": {"command": "rm -r x"}}),
        ));
        assert_eq!(asked.len(), 1);
        match &asked[0] {
            SessionEvent::Lifecycle(LifecycleEvent::InputRequested { request }) => {
                assert_eq!(request.id, "c7");
                assert!(
                    matches!(&request.kind, InputKind::Permission { tool_name, .. } if tool_name == "shell")
                );
            }
            other => panic!("{other:?}"),
        }
        // A payload that names no call: one wait a turn, the turn's end its end.
        let turn = translate(&hook(
            "PermissionRequest",
            json!({"tool_name": "shell", "tool_input": {}}),
        ));
        assert!(matches!(
            &turn[0],
            SessionEvent::Lifecycle(LifecycleEvent::InputRequested { request }) if request.id == "permission:t1"
        ));
        // Neither a start nor a stop says the person answered.
        for (event, body) in [
            (
                "PreToolUse",
                json!({"tool_name": "shell", "tool_use_id": "c7", "tool_input": {}}),
            ),
            ("Stop", json!({})),
            ("Interrupt", json!({})),
        ] {
            assert!(
                translate(&hook(event, body)).iter().all(|e| !matches!(
                    e,
                    SessionEvent::Lifecycle(LifecycleEvent::InputResolved { .. })
                )),
                "{event}"
            );
        }
    }

    #[test]
    fn a_sub_agent_is_announced_and_its_events_nest_under_it() {
        let started = translate(&hook(
            "SubagentStart",
            json!({"agent_id": "a1", "agent_type": "explorer", "permission_mode": "default"}),
        ));
        assert!(
            matches!(&started[0], SessionEvent::Progress(ProgressEvent::SubagentStarted { id, name, .. }) if id.0 == "a1" && name == "explorer")
        );
        let nested = translate(&hook(
            "PreToolUse",
            json!({"agent_id": "a1", "tool_name": "read_file", "tool_input": {"path": "x"}}),
        ));
        assert_eq!(nested.len(), 1);
        assert!(
            matches!(&nested[0], SessionEvent::Progress(ProgressEvent::Nested { parent, .. }) if parent.0 == "a1")
        );
        assert!(matches!(
            translate(&hook(
                "SubagentStop",
                json!({"agent_id": "a1", "agent_type": "explorer"})
            ))[0],
            SessionEvent::Progress(ProgressEvent::SubagentEnded { ok: true, .. })
        ));
        assert!(
            translate(&hook("SubagentStart", json!({}))).is_empty(),
            "no id, no sub-agent"
        );
    }
}
