//! OpenCode → session events, from the server the TUI already runs.
//!
//! OpenCode is a TUI over its own HTTP server, so nothing is injected: the
//! launch names a loopback port and the engine subscribes to `GET /event`,
//! the server-sent bus every client of that process shares. Each frame is
//! `{type, properties}`; what a row reads:
//!
//! Every frame names its session (`sessionID` on the properties, or on the
//! part) and a sub-agent is a **child session** — one `parentID` — so every
//! session-scoped event here is raised *by* its session id and the engine
//! sorts the root from the children (`presence.rs`: a harness that names
//! its sessions). What a row reads:
//!
//! | frame | here |
//! |---|---|
//! | `server.connected` (the bus's first frame) | `Started` |
//! | `session.created` / `session.updated` with a `parentID` | a sub-agent, by its session id and title |
//! | `session.status` `busy` / `idle`, `session.idle` | a turn's start and end — by the session; an idle child session has left |
//! | `session.error` | a child session that failed |
//! | `permission.updated` / `permission.asked` | an input request — a **question** when the permission's subject is OpenCode's `question` tool, a permission otherwise |
//! | `permission.replied` | resolved |
//! | `message.part.updated` with a `tool` part `running` / `completed` / `error` | a tool's start and end |
//! | `message.part.updated` with a `step-finish` part | the step's cost and tokens |
//! | `message.updated` for an assistant message | the model that wrote it, `providerID/modelID` |
//!
//! The engine redacts every pulled event before the roster, as it does a
//! reported one; nothing here sees a secret twice.

use super::{args_summary, first_str};
use bisa_core::ToolTier;
use bisa_harness::{
    InputRequest, LifecycleEvent, ProgressEvent, PullSource, ReportingContext, ReportingPlan,
    SessionEvent, SubagentId,
};
use serde_json::Value;

/// The session a frame is about: the properties' `sessionID`, or the part's.
fn session_of(props: &Value) -> Option<SubagentId> {
    first_str(props, &["sessionID"])
        .or_else(|| props.pointer("/part/sessionID").and_then(|s| s.as_str()))
        .or_else(|| props.pointer("/info/sessionID").and_then(|s| s.as_str()))
        .filter(|s| !s.is_empty())
        .map(|s| SubagentId(s.to_string()))
}

pub fn reporting(ctx: &ReportingContext) -> ReportingPlan {
    let Some(port) = ctx.port else {
        return ReportingPlan::default();
    };
    ReportingPlan {
        files: Vec::new(),
        args: vec!["--port".into(), port.to_string()],
        intercept_approval_notifications: false,
        pull: Some(PullSource::Sse {
            url: format!("http://127.0.0.1:{port}/event"),
        }),
    }
}

/// One bus frame → what it means.
pub fn translate(payload: &Value) -> Vec<SessionEvent> {
    let Some(kind) = payload.get("type").and_then(|t| t.as_str()) else {
        return Vec::new();
    };
    let props = payload.get("properties").cloned().unwrap_or(Value::Null);
    let by = session_of(&props);
    let raised = |p: ProgressEvent| SessionEvent::Progress(p.raised_by(by.clone()));
    // A session that went idle: its turn is over — and a child session that
    // went idle has left (the engine ignores the same word about the root).
    let over = || {
        let mut out = vec![raised(ProgressEvent::TurnEnded)];
        if let Some(id) = by.clone() {
            out.push(SessionEvent::Progress(ProgressEvent::SubagentEnded {
                id,
                ok: true,
            }));
        }
        out
    };
    match kind {
        "server.connected" => vec![SessionEvent::Lifecycle(LifecycleEvent::Started)],
        // A session with a parent is a sub-agent — announced by its id, named
        // by its title; the root announces nothing.
        "session.created" | "session.updated" => {
            let info = props.get("info").cloned().unwrap_or(Value::Null);
            let Some(parent) = first_str(&info, &["parentID"]).filter(|p| !p.is_empty()) else {
                return Vec::new();
            };
            let _ = parent;
            let Some(id) = first_str(&info, &["id"]).filter(|s| !s.is_empty()) else {
                return Vec::new();
            };
            let title = first_str(&info, &["title"]).unwrap_or("task").to_string();
            vec![SessionEvent::Progress(ProgressEvent::SubagentStarted {
                id: SubagentId(id.to_string()),
                name: title.clone(),
                description: title.chars().take(120).collect(),
            })]
        }
        "session.status" => match props.pointer("/status/type").and_then(|t| t.as_str()) {
            Some("busy") => vec![raised(ProgressEvent::TurnStarted)],
            Some("idle") => over(),
            _ => Vec::new(),
        },
        "session.idle" => over(),
        // A child session that failed; the root's error is not a state.
        "session.error" => match by.clone() {
            Some(id) => vec![SessionEvent::Progress(ProgressEvent::SubagentEnded {
                id,
                ok: false,
            })],
            None => Vec::new(),
        },
        // An assistant message names the model that wrote it: `providerID/modelID`.
        "message.updated" => {
            let info = props.get("info").cloned().unwrap_or(Value::Null);
            if info.get("role").and_then(|r| r.as_str()) != Some("assistant") {
                return Vec::new();
            }
            let Some(model) = info
                .get("modelID")
                .and_then(|m| m.as_str())
                .filter(|m| !m.is_empty())
            else {
                return Vec::new();
            };
            let model = match info
                .get("providerID")
                .and_then(|p| p.as_str())
                .filter(|p| !p.is_empty())
            {
                Some(provider) => format!("{provider}/{model}"),
                None => model.to_string(),
            };
            vec![SessionEvent::Progress(ProgressEvent::ModelChanged {
                model,
            })]
        }
        "permission.updated" | "permission.asked" => {
            // The subject: OpenCode's own key (`permission`), else the older
            // `type`, else the title. Its `question` tool asks the person
            // something — that is a question, not a permission.
            let subject = first_str(&props, &["permission", "type", "title"])
                .unwrap_or("tool")
                .to_string();
            let id = first_str(&props, &["id"])
                .unwrap_or("permission")
                .to_string();
            let metadata = props.get("metadata").cloned().unwrap_or(Value::Null);
            if subject.eq_ignore_ascii_case("question") {
                let text = first_str(&props, &["title", "message"])
                    .or_else(|| first_str(&metadata, &["question", "message", "text"]))
                    .unwrap_or("The agent has a question")
                    .to_string();
                return vec![SessionEvent::Lifecycle(LifecycleEvent::InputRequested {
                    request: InputRequest::question(id, text, Vec::new()),
                })];
            }
            vec![SessionEvent::Lifecycle(LifecycleEvent::InputRequested {
                request: InputRequest::permission(
                    id,
                    subject.clone(),
                    ToolTier::classify(&subject.to_ascii_lowercase()),
                    args_summary(&metadata),
                    metadata,
                ),
            })]
        }
        "permission.replied" => vec![SessionEvent::Lifecycle(LifecycleEvent::InputResolved {
            id: first_str(&props, &["permissionID", "id"])
                .unwrap_or("permission")
                .to_string(),
        })],
        "message.part.updated" => {
            let part = props.get("part").cloned().unwrap_or(Value::Null);
            match part.get("type").and_then(|t| t.as_str()) {
                Some("tool") => {
                    let name = first_str(&part, &["tool"]).unwrap_or("tool").to_string();
                    match part.pointer("/state/status").and_then(|s| s.as_str()) {
                        Some("running") => vec![raised(ProgressEvent::ToolStarted {
                            tier: ToolTier::classify(&name.to_ascii_lowercase()),
                            args_summary: args_summary(
                                part.pointer("/state/input").unwrap_or(&Value::Null),
                            ),
                            name,
                        })],
                        Some("completed") => {
                            vec![raised(ProgressEvent::ToolEnded { name, ok: true })]
                        }
                        Some("error") => vec![raised(ProgressEvent::ToolEnded { name, ok: false })],
                        _ => Vec::new(),
                    }
                }
                Some("step-finish") => {
                    let input = part
                        .pointer("/tokens/input")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let output = part
                        .pointer("/tokens/output")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let cents = part
                        .get("cost")
                        .and_then(|v| v.as_f64())
                        .map(|usd| (usd * 100.0).round() as u64)
                        .unwrap_or(0);
                    if input == 0 && output == 0 && cents == 0 {
                        return Vec::new();
                    }
                    vec![raised(ProgressEvent::CostDelta {
                        input_tokens: input,
                        output_tokens: output,
                        usd_cents: cents,
                    })]
                }
                _ => Vec::new(),
            }
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_recipe_names_a_port_and_pulls_the_event_bus() {
        let ctx = ReportingContext {
            session: "01S".into(),
            reporter: vec![],
            files_dir: "/ws/run/interactive/01S".into(),
            port: Some(41234),
            guard: None,
            guard_timeout_secs: 0,
        };
        let plan = reporting(&ctx);
        assert_eq!(plan.args, ["--port", "41234"]);
        assert!(
            matches!(plan.pull, Some(PullSource::Sse { ref url }) if url == "http://127.0.0.1:41234/event")
        );
        assert!(reporting(&ReportingContext { port: None, ..ctx }).is_none());
    }

    #[test]
    fn the_bus_first_frame_starts_the_session_a_question_is_a_question_and_a_step_finish_is_the_cost(
    ) {
        assert!(matches!(
            translate(&json!({"type": "server.connected", "properties": {}}))[0],
            SessionEvent::Lifecycle(LifecycleEvent::Started)
        ));
        let asked = translate(
            &json!({"type": "permission.asked", "properties": {"id": "q1", "permission": "question", "title": "Which database?", "metadata": {}}}),
        );
        assert!(
            matches!(&asked[0], SessionEvent::Lifecycle(LifecycleEvent::InputRequested { request }) if request.id == "q1" && matches!(&request.kind, bisa_harness::InputKind::Question { text, .. } if text == "Which database?")),
            "{asked:?}"
        );
        let bash = translate(
            &json!({"type": "permission.asked", "properties": {"id": "p2", "permission": "bash", "metadata": {"command": "rm -r build"}}}),
        );
        assert!(
            matches!(&bash[0], SessionEvent::Lifecycle(LifecycleEvent::InputRequested { request }) if matches!(&request.kind, bisa_harness::InputKind::Permission { tool_name, .. } if tool_name == "bash"))
        );
        let cost = translate(
            &json!({"type": "message.part.updated", "properties": {"part": {"type": "step-finish", "reason": "stop", "cost": 0.0123, "tokens": {"input": 671, "output": 8, "reasoning": 0, "cache": {"read": 21415, "write": 0}}}}}),
        );
        assert!(matches!(
            &cost[0],
            SessionEvent::Progress(ProgressEvent::CostDelta {
                input_tokens: 671,
                output_tokens: 8,
                usd_cents: 1
            })
        ));
        assert!(translate(&json!({"type": "message.part.updated", "properties": {"part": {"type": "step-finish", "cost": 0, "tokens": {"input": 0, "output": 0}}}})).is_empty(), "nothing spent is nothing to say");
        assert!(translate(&json!({"type": "message.part.updated", "properties": {"part": {"type": "text", "text": "hi"}}})).is_empty());
        let model = translate(
            &json!({"type": "message.updated", "properties": {"info": {"role": "assistant", "providerID": "anthropic", "modelID": "claude-opus-5"}}}),
        );
        assert!(
            matches!(&model[0], SessionEvent::Progress(ProgressEvent::ModelChanged { model }) if model == "anthropic/claude-opus-5")
        );
        assert!(
            translate(
                &json!({"type": "message.updated", "properties": {"info": {"role": "user"}}})
            )
            .is_empty(),
            "a person's message names no model"
        );
    }

    #[test]
    fn status_permissions_and_tool_parts_translate() {
        assert!(
            matches!(
                translate(
                    &json!({"type": "session.status", "properties": {"status": {"type": "busy"}}})
                )[0],
                SessionEvent::Progress(ProgressEvent::TurnStarted)
            ),
            "a frame naming no session is the session's own"
        );
        assert!(matches!(
            translate(&json!({"type": "session.idle", "properties": {}}))[0],
            SessionEvent::Progress(ProgressEvent::TurnEnded)
        ));
        assert!(
            matches!(&translate(&json!({"type": "permission.updated", "properties": {"id": "p1", "type": "bash"}}))[0],
            SessionEvent::Lifecycle(LifecycleEvent::InputRequested { request }) if request.id == "p1")
        );
        assert!(
            matches!(&translate(&json!({"type": "permission.replied", "properties": {"permissionID": "p1", "response": "once"}}))[0],
            SessionEvent::Lifecycle(LifecycleEvent::InputResolved { id }) if id == "p1")
        );
        let running = json!({"type": "message.part.updated", "properties": {"part": {"type": "tool", "tool": "edit", "state": {"status": "running", "input": {"file": "x"}}}}});
        assert!(
            matches!(&translate(&running)[0], SessionEvent::Progress(ProgressEvent::ToolStarted { name, tier: ToolTier::Write, .. }) if name == "edit")
        );
        assert!(translate(&json!({"type": "message.updated", "properties": {}})).is_empty());
    }

    #[test]
    fn every_frame_is_raised_by_its_session_and_a_child_session_is_a_sub_agent() {
        // The root's frames name the root: the engine learns it and reads them as the session's.
        let busy = translate(
            &json!({"type": "session.status", "properties": {"sessionID": "ses_root", "status": {"type": "busy"}}}),
        );
        assert!(
            matches!(&busy[0], SessionEvent::Progress(ProgressEvent::Nested { parent, event }) if parent.0 == "ses_root" && matches!(**event, ProgressEvent::TurnStarted))
        );
        let tool = translate(
            &json!({"type": "message.part.updated", "properties": {"part": {"type": "tool", "sessionID": "ses_root", "tool": "bash", "state": {"status": "running", "input": {}}}}}),
        );
        assert!(
            matches!(&tool[0], SessionEvent::Progress(ProgressEvent::Nested { parent, .. }) if parent.0 == "ses_root")
        );
        // A session with a parent is a sub-agent, by its own id and title.
        let child = translate(
            &json!({"type": "session.created", "properties": {"info": {"id": "ses_child", "parentID": "ses_root", "title": "map the crate", "directory": "/w"}}}),
        );
        assert!(
            matches!(&child[0], SessionEvent::Progress(ProgressEvent::SubagentStarted { id, name, .. }) if id.0 == "ses_child" && name == "map the crate")
        );
        assert!(
            translate(&json!({"type": "session.updated", "properties": {"info": {"id": "ses_root", "title": "root"}}})).is_empty(),
            "the root announces nothing"
        );
        // Its idle is its turn over and its leaving; its error its failure.
        let idle =
            translate(&json!({"type": "session.idle", "properties": {"sessionID": "ses_child"}}));
        assert!(
            matches!(&idle[0], SessionEvent::Progress(ProgressEvent::Nested { parent, event }) if parent.0 == "ses_child" && matches!(**event, ProgressEvent::TurnEnded))
        );
        assert!(
            matches!(&idle[1], SessionEvent::Progress(ProgressEvent::SubagentEnded { id, ok: true }) if id.0 == "ses_child")
        );
        let error = translate(
            &json!({"type": "session.error", "properties": {"sessionID": "ses_child", "error": {"name": "UnknownError"}}}),
        );
        assert!(
            matches!(&error[0], SessionEvent::Progress(ProgressEvent::SubagentEnded { id, ok: false }) if id.0 == "ses_child")
        );
        assert!(
            translate(&json!({"type": "session.error", "properties": {}})).is_empty(),
            "an error naming no session is not a state"
        );
        // The cost of a part is raised by its session too.
        let cost = translate(
            &json!({"type": "message.part.updated", "properties": {"part": {"type": "step-finish", "sessionID": "ses_child", "cost": 0.02, "tokens": {"input": 1, "output": 1}}}}),
        );
        assert!(
            matches!(&cost[0], SessionEvent::Progress(ProgressEvent::Nested { parent, .. }) if parent.0 == "ses_child")
        );
    }
}
