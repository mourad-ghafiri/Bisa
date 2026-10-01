//! What a harness says about itself while a person drives it in a terminal —
//! its own lifecycle hooks, notifications or event stream — translated into
//! the one vocabulary the engine folds ([`SessionEvent`]).
//!
//! One translator per harness, beside that harness's protocol adapter, so the
//! two mappings of one tool's events live together. Every translator is pure:
//! a payload in, events out, nothing else — which is what lets the reporter
//! CLI, the engine's event puller and a test all call the same function.
//!
//! The reporter argv every recipe points at is `bisa session report
//! --harness <id>` — this binary in its reporter personality — with the hook's
//! payload on stdin, or as its one argument for a harness that passes it that
//! way (Codex's `notify`).
//!
//! The other direction lives here too. A harness whose pre-execution hook
//! waits for the node's guard reads the verdict in a shape of its own;
//! [`guard_output`] is that shape per harness, so the guard personality of
//! the CLI prints what it is handed and knows no harness.

pub mod claude_code;
pub mod codex;
pub mod copilot;
pub mod opencode;
pub mod pi_like;

use bisa_core::ToolTier;
use bisa_harness::{ProgressEvent, SessionEvent, SubagentId};
use serde_json::Value;

/// The events one reported payload means for `harness`, or none when the
/// harness has no translator or the payload says nothing a person reads.
pub fn translate(harness: &str, payload: &Value) -> Vec<SessionEvent> {
    match harness {
        "claude-code" => claude_code::translate(payload),
        "codex" => codex::translate(payload),
        "copilot" => copilot::translate(payload),
        "omp" | "pi" => pi_like::translate(payload),
        "opencode" => opencode::translate(payload),
        _ => Vec::new(),
    }
}

/// The node's verdict on one tool call, as its guard route answers it: what
/// to do, why, and the input to run with when a placeholder was restored.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Verdict<'a> {
    /// `allow`, `deny` or `ask`.
    pub decision: &'a str,
    pub reason: Option<&'a str>,
    pub updated_input: Option<&'a Value>,
}

impl<'a> Verdict<'a> {
    /// The verdict a reply carries, or `None` when the guard has no opinion
    /// — the harness's own prompt stands, and nothing is printed.
    fn of(reply: &'a Value) -> Option<Self> {
        Some(Self {
            decision: reply.get("decision").and_then(|d| d.as_str())?,
            reason: reply.get("reason").and_then(|r| r.as_str()),
            updated_input: reply.get("updated_input").filter(|i| !i.is_null()),
        })
    }
}

/// What `harness`'s pre-execution hook prints for the guard's `reply`, in
/// the shape that harness reads; `None` when the reply carries no decision,
/// or the harness has no hook that reads one.
pub fn guard_output(harness: &str, reply: &Value) -> Option<Value> {
    let verdict = Verdict::of(reply)?;
    match harness {
        "claude-code" | "claude_code" => Some(claude_code::guard_output(&verdict)),
        "copilot" => Some(copilot::guard_output(&verdict)),
        _ => None,
    }
}

/// A shell word: single-quoted, with the one character a single-quoted word
/// cannot hold escaped the way `sh` reads it.
pub(crate) fn shell_word(raw: &str) -> String {
    format!("'{}'", raw.replace('\'', r"'\''"))
}

/// The reporter as one shell command line, for a harness that runs its hooks
/// through a shell.
pub(crate) fn shell_command(argv: &[String]) -> String {
    argv.iter()
        .map(|w| shell_word(w))
        .collect::<Vec<_>>()
        .join(" ")
}

/// A short, single-line rendering of a tool's input for a status line.
pub(crate) fn args_summary(input: &Value) -> String {
    crate::util::summarize_args(input, 160)
}

/// The first of several keys a loosely-specified payload may use for one fact.
pub(crate) fn first_str<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|k| value.get(k).and_then(|v| v.as_str()))
}

// The lifecycle-hook shape Claude Code defined and Codex adopted — one
// payload per event on stdin, `hook_event_name`, `tool_name`, `tool_input`,
// `tool_use_id`, and `agent_id` inside a sub-agent — read once here, so the
// two translators are two tables over one set of arms.

/// The sub-agent a hook fired inside, when the payload says so.
pub(crate) fn hook_subagent(payload: &Value) -> Option<SubagentId> {
    payload
        .get("agent_id")
        .and_then(|a| a.as_str())
        .filter(|a| !a.is_empty())
        .map(|a| SubagentId(a.to_string()))
}

pub(crate) fn hook_tool_name(payload: &Value) -> String {
    first_str(payload, &["tool_name"])
        .unwrap_or("?")
        .to_string()
}

pub(crate) fn hook_tool_use_id(payload: &Value) -> String {
    first_str(payload, &["tool_use_id"])
        .unwrap_or("tool")
        .to_string()
}

/// The tier of a tool a Claude-shaped hook names — `Bash`, `Read`, `Edit`:
/// Claude Code's own names, which Copilot CLI's hooks speak too — by what it
/// can change.
pub(crate) fn hook_tool_tier(name: &str) -> ToolTier {
    ToolTier::classify(&crate::claude_code::normalize_tool(name))
}

/// A tool the hook says started, its tier the caller's word.
pub(crate) fn hook_tool_started(name: String, tier: ToolTier, input: &Value) -> ProgressEvent {
    ProgressEvent::ToolStarted {
        tier,
        args_summary: args_summary(input),
        name,
    }
}

/// `SubagentStart`: the sub-agent by its `agent_id`, named by its
/// `agent_type`, described by what it was asked — Claude Code's
/// `subagent_input.prompt`; Codex carries no input; none without an id.
pub(crate) fn hook_subagent_started(payload: &Value) -> Option<ProgressEvent> {
    let id = hook_subagent(payload)?;
    let description = payload
        .pointer("/subagent_input/prompt")
        .map(|i| match i {
            Value::String(s) => s.clone(),
            other => args_summary(other),
        })
        .unwrap_or_default();
    Some(ProgressEvent::SubagentStarted {
        id,
        name: first_str(payload, &["agent_type"])
            .unwrap_or("agent")
            .to_string(),
        description: description.chars().take(120).collect(),
    })
}

/// `SubagentStop`: the sub-agent left. Neither hook carries a verdict, so
/// `ok` is the caller's word — true, for both harnesses today.
pub(crate) fn hook_subagent_ended(payload: &Value, ok: bool) -> Option<ProgressEvent> {
    Some(ProgressEvent::SubagentEnded {
        id: hook_subagent(payload)?,
        ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_harness_reports_nothing() {
        assert!(translate(
            "custom:thing",
            &serde_json::json!({"hook_event_name": "Stop"})
        )
        .is_empty());
    }

    #[test]
    fn a_copilot_payload_is_read_by_copilots_translator() {
        let events = translate("copilot", &serde_json::json!({"hook_event_name": "Stop"}));
        assert!(events
            .iter()
            .any(|e| matches!(e, SessionEvent::Progress(ProgressEvent::TurnEnded))));
        assert!(
            translate("grok", &serde_json::json!({"hook_event_name": "Stop"})).is_empty(),
            "Grok Build's terminal has no hook of ours: nothing is reported, nothing read"
        );
    }

    #[test]
    fn a_verdict_is_printed_in_the_shape_its_harness_reads_and_no_opinion_prints_nothing() {
        let deny = serde_json::json!({
            "decision": "deny", "reason": "refused by the guard rule “sudo, doas, su”"
        });
        let allow = serde_json::json!({
            "decision": "allow", "updated_input": { "command": "echo restored-fake" }
        });
        let ask = serde_json::json!({ "decision": "ask", "updated_input": null });

        // Claude Code: nested under `hookSpecificOutput`.
        let out = guard_output("claude-code", &deny).unwrap();
        assert_eq!(out["hookSpecificOutput"]["hookEventName"], "PreToolUse");
        assert_eq!(out["hookSpecificOutput"]["permissionDecision"], "deny");
        assert!(out["hookSpecificOutput"]["permissionDecisionReason"]
            .as_str()
            .unwrap()
            .contains("sudo"));
        assert!(out["hookSpecificOutput"].get("updatedInput").is_none());
        let out = guard_output("claude_code", &allow).unwrap();
        assert_eq!(out["hookSpecificOutput"]["permissionDecision"], "allow");
        assert_eq!(
            out["hookSpecificOutput"]["updatedInput"]["command"],
            "echo restored-fake"
        );

        // Copilot CLI: flat, and the restored input is `modifiedArgs`.
        assert_eq!(
            guard_output("copilot", &deny).unwrap(),
            serde_json::json!({
                "permissionDecision": "deny",
                "permissionDecisionReason": "refused by the guard rule “sudo, doas, su”"
            })
        );
        assert_eq!(
            guard_output("copilot", &allow).unwrap(),
            serde_json::json!({
                "permissionDecision": "allow",
                "modifiedArgs": { "command": "echo restored-fake" }
            })
        );
        assert_eq!(
            guard_output("copilot", &ask).unwrap(),
            serde_json::json!({ "permissionDecision": "ask" }),
            "an input that is null is no input"
        );

        // No decision, no output — for every harness.
        for harness in ["claude-code", "copilot"] {
            assert!(guard_output(harness, &serde_json::json!({})).is_none());
            assert!(guard_output(harness, &serde_json::json!({ "reason": "x" })).is_none());
        }
        // A harness with no hook that reads a verdict is printed nothing.
        for harness in ["codex", "opencode", "pi", "omp", "grok", "custom:mine"] {
            assert!(guard_output(harness, &deny).is_none(), "{harness}");
        }
    }

    #[test]
    fn a_hooks_tool_is_tiered_by_what_it_can_change() {
        for (name, tier) in [
            ("Read", ToolTier::Read),
            ("Grep", ToolTier::Read),
            ("Glob", ToolTier::Read),
            ("WebFetch", ToolTier::Read),
            ("Edit", ToolTier::Write),
            ("Write", ToolTier::Write),
            ("Bash", ToolTier::Exec),
            ("Agent", ToolTier::Exec),
            ("a_tool_nobody_knows", ToolTier::Exec),
        ] {
            assert_eq!(hook_tool_tier(name), tier, "{name}");
        }
    }

    #[test]
    fn a_reporter_line_is_shell_safe() {
        let argv = vec![
            "/opt/it's/bisa".to_string(),
            "session".into(),
            "report".into(),
        ];
        assert_eq!(
            shell_command(&argv),
            r"'/opt/it'\''s/bisa' 'session' 'report'"
        );
    }
}
