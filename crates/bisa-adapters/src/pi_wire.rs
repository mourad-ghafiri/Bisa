//! Shared wire mapping for the pi RPC protocol family (pi, omp).
//!
//! Reference: pi's `packages/coding-agent/src/modes/rpc/rpc-types.ts` —
//! JSONL over stdio: commands in (`{"id"?, "type": "prompt"|"steer"|...}`),
//! responses (`{"type":"response","command":...,"success":...}`) and agent
//! session events (`agent_start`, `message_end`, `tool_execution_*`,
//! `agent_end`, ...) out.

use bisa_core::ToolTier;
use bisa_harness::{Effort, LifecycleEvent, Outcome, Phase, ProgressEvent, SessionEvent};

use crate::util::{Drive, Shared};

/// State-request id we use to learn the native session id/file.
pub const STATE_REQ_ID: &str = "bisa-state";

/// Map one pi-wire JSON object. Returns `Drive::Continue` always — pi RPC
/// sessions end on process exit, not on any event.
/// A tool call's own id (`toolCallId`), when the event carries one — what
/// ties `tool_execution_end` to its start.
fn tool_call_id(value: &serde_json::Value) -> Option<String> {
    value
        .get("toolCallId")
        .and_then(|v| v.as_str())
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}

pub fn map_pi_event(shared: &Shared, value: serde_json::Value) -> Drive {
    let kind = value.get("type").and_then(|t| t.as_str()).unwrap_or("");
    match kind {
        "response" => {
            let success = value
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let command = value.get("command").and_then(|v| v.as_str()).unwrap_or("");
            if !success {
                let error = value
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                tracing::warn!(target: "bisa_adapters::pi", "command {command} failed: {error}");
            }
            if command == "get_state" && success {
                if let Some(data) = value.get("data") {
                    if let Some(id) = data.get("sessionId").and_then(|v| v.as_str()) {
                        shared.set_native_id(id);
                    }
                    if let Some(file) = data.get("sessionFile").and_then(|v| v.as_str()) {
                        shared.set_transcript(file);
                    }
                }
            }
            shared.broadcaster.emit(SessionEvent::Raw(value));
        }
        "agent_start" => {
            shared.set_phase(Phase::Turn);
            shared
                .broadcaster
                .emit(SessionEvent::Progress(ProgressEvent::TurnStarted));
        }
        "tool_execution_start" => {
            let name = value
                .get("toolName")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_string();
            let args = value
                .get("args")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            shared.set_activity(name.clone());
            shared
                .broadcaster
                .emit(SessionEvent::Progress(ProgressEvent::ToolStarted {
                    tier: ToolTier::classify(&name),
                    args_summary: crate::util::summarize_args(&args, 160),
                    name,
                    id: tool_call_id(&value),
                }));
        }
        "tool_execution_end" => {
            let name = value
                .get("toolName")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_string();
            let ok = !value
                .get("isError")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            shared
                .broadcaster
                .emit(SessionEvent::Progress(ProgressEvent::ToolEnded {
                    name,
                    ok,
                    id: tool_call_id(&value),
                }));
        }
        "message_end" => {
            // Final authoritative message for the step: extract assistant text.
            if let Some(content) = value.pointer("/message/content").and_then(|c| c.as_array()) {
                for block in content {
                    if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                            shared.broadcaster.emit(SessionEvent::Progress(
                                ProgressEvent::TextDelta {
                                    text: text.to_string(),
                                },
                            ));
                        }
                    }
                }
            }
        }
        "agent_end" => {
            // omp extension: `isTerminal: false` means a retry/continuation is
            // coming — not even a turn boundary. Absent = normal turn end.
            let coming_back = value.get("isTerminal").and_then(|v| v.as_bool()) == Some(false);
            if coming_back {
                shared.broadcaster.emit(SessionEvent::Raw(value));
                return Drive::Continue;
            }
            shared
                .broadcaster
                .emit(SessionEvent::Progress(ProgressEvent::TurnEnded));
            shared.set_phase(Phase::Idle);
            // Turn complete; the session stays alive for steering/follow-ups.
            shared
                .broadcaster
                .emit(SessionEvent::Lifecycle(LifecycleEvent::Ended {
                    outcome: Outcome::Completed,
                    is_terminal: false,
                }));
        }
        // High-frequency deltas and everything else: raw passthrough.
        _ => shared.broadcaster.emit(SessionEvent::Raw(value)),
    }
    Drive::Continue
}

/// Build a pi RPC command line.
pub fn command(id: Option<&str>, kind: &str, message: Option<&str>) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    if let Some(id) = id {
        obj.insert("id".into(), id.into());
    }
    obj.insert("type".into(), kind.into());
    if let Some(message) = message {
        obj.insert("message".into(), message.into());
    }
    serde_json::Value::Object(obj)
}

/// The command that sets how hard the session's model works, from then on:
/// `{"type":"set_thinking_level","level":"<level>"}` — the wire's own name
/// for an effort (https://github.com/can1357/oh-my-pi, its RPC reference,
/// read 2026-09-29). The wire takes all six levels and holds the one it is given
/// to what the model can do.
pub fn set_thinking_level(level: Effort) -> serde_json::Value {
    serde_json::json!({ "type": "set_thinking_level", "level": level.as_str() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_effort_command_names_the_level_in_its_own_word() {
        for level in Effort::ALL {
            assert_eq!(
                set_thinking_level(level),
                serde_json::json!({ "type": "set_thinking_level", "level": level.as_str() })
            );
        }
        let wire = set_thinking_level(Effort::Xhigh);
        assert_eq!(wire["level"], "xhigh");
        assert!(wire.get("id").is_none(), "nothing waits for its answer");
        assert!(wire.get("message").is_none());
    }

    #[test]
    fn a_command_carries_its_id_and_its_message_only_when_given() {
        assert_eq!(
            command(Some(STATE_REQ_ID), "get_state", None),
            serde_json::json!({ "id": STATE_REQ_ID, "type": "get_state" })
        );
        assert_eq!(
            command(None, "prompt", Some("go")),
            serde_json::json!({ "type": "prompt", "message": "go" })
        );
    }
}
