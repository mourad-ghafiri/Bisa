//! The scripted agent as a Muse Session Protocol host — what `muse serve`
//! is to the Muse Code adapter (`crates/bisa-adapters/src/msp.rs`), driven
//! by the same script the ACP half reads: `say`, `think`, `write`, `tools`
//! (with `ask`), `hold`, `models` and `model_efforts`, `usage`.
//!
//! The frames are the recorded transcripts' (the SDK's `schema/msp/
//! transcripts/`, read 2026-10-19): `initialize` answered with the host and
//! its schema, then `initialized`; `model/list` as a query; `session/start`
//! said as `session/started` before its result; `turn/start` acknowledged
//! with its disposition, then `turn/started`, the items — an `agentMessage`
//! started, streamed as `item/delta` and completed; a `reasoning`; a
//! `toolCall` with its `args` as a JSON string — and `turn/completed`; an
//! ask as the `approval/requested` notification and the `approval/request`
//! request, settled by the `approval/decide` command; `session/resume` by
//! id, metadata only; `turn/cancel` ending a held turn `cancelled`.
//!
//! What it kept is recorded as the ACP half records it, under the same
//! event names where the fact is the same (`started`, `prompt`, `config`,
//! `permission`, `tool`, `write`, `ended`), so a journey reads a session on
//! either wire the same way; what is this wire's alone is its own
//! (`session_start`, `session_resume`, `turn`).

use serde_json::{json, Value};
use std::path::PathBuf;

use super::{filled, listed_models, text, Agent, Scope, TurnEnd};

/// The fingerprint the host answers when the script names none.
const FINGERPRINT: &str = "sha256:scripted";

/// Serve one client over stdin and stdout until its input closes.
pub(super) fn serve(agent: &mut Agent) {
    while let Some(message) = agent.next_message() {
        handle(agent, message);
    }
}

fn notify(agent: &Agent, method: &str, params: Value) {
    agent.send(json!({ "jsonrpc": "2.0", "method": method, "params": params }));
}

/// A session's MCP servers as this wire hands them — an object by name,
/// `env` as an object — in the shape the ACP half keeps and starts them
/// from: a list, `env` as pairs.
fn servers_listed(object: &Value) -> Value {
    let listed: Vec<Value> = object
        .as_object()
        .into_iter()
        .flatten()
        .map(|(name, spec)| {
            let env: Vec<Value> = spec["env"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(k, v)| json!({ "name": k, "value": v }))
                .collect();
            json!({
                "name": name,
                "transport": spec["transport"],
                "command": spec["command"],
                "args": spec["args"].as_array().cloned().unwrap_or_default(),
                "env": env,
                "url": spec["url"],
            })
        })
        .collect();
    Value::Array(listed)
}

/// The rows of `model/list`: the script's models, the first the default,
/// each with the levels the script gives it.
fn models(agent: &Agent) -> Value {
    let rows: Vec<Value> = listed_models(&agent.script)
        .iter()
        .enumerate()
        .map(|(n, id)| {
            let variants = agent.script["model_efforts"][id]
                .as_array()
                .cloned()
                .or_else(|| agent.script["efforts"].as_array().cloned())
                .unwrap_or_else(|| vec![json!("low"), json!("medium"), json!("high")]);
            json!({
                "modelId": id,
                "providerId": "muse",
                "displayLabel": format!("Model {id}"),
                "isDefault": n == 0,
                "isActive": false,
                "variants": variants,
                "defaultReasoningEffort": variants.first().cloned().unwrap_or(json!("medium")),
            })
        })
        .collect();
    json!({ "providerId": "muse", "source": "scripted", "models": rows })
}

fn session_view(agent: &Agent, session: &str) -> Value {
    let known = agent.beside.known(session).unwrap_or(Value::Null);
    let model = known["model"]
        .as_str()
        .map(str::to_string)
        .or_else(|| listed_models(&agent.script).first().cloned());
    json!({
        "sessionId": session,
        "status": "idle",
        "turnCount": known["history"].as_array().map(Vec::len).unwrap_or(0),
        "workspaceRoot": known["cwd"],
        "providerId": "muse",
        "modelId": model,
        "approvalMode": { "mode": known["approval_mode"], "source": "startup" },
    })
}

fn handle(agent: &mut Agent, message: Value) {
    let id = message.get("id").cloned();
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        // An answer to a request of the host's, after its wait.
        return;
    };
    match (method, id) {
        ("initialize", Some(id)) => {
            agent.record(json!({ "event": "initialize", "params": params }));
            let fingerprint = agent.script["fingerprint"]
                .as_str()
                .unwrap_or(FINGERPRINT)
                .to_string();
            agent.answer(
                &id,
                json!({
                    "serverInfo": { "name": "scripted-muse", "version": "0.0.0-scripted" },
                    "userAgent": "scripted-muse/0.0.0",
                    "museHome": agent.beside.folder(".muse-home"),
                    "platformFamily": "unix",
                    "platformOs": "linux",
                    "schema": { "version": 1, "fingerprint": fingerprint },
                    "grantedCapabilities": [],
                    "experimentalApi": true,
                    "sessionDurability": "durable",
                }),
            );
        }
        ("initialized", None) => {}
        ("model/list", Some(id)) => {
            agent.record(json!({ "event": "models" }));
            agent.answer(&id, models(agent));
        }
        ("usage/read", Some(id)) => {
            let usage = agent.script["usage"].clone();
            agent.answer(
                &id,
                if usage.is_object() {
                    json!({ "usage": usage })
                } else {
                    json!({})
                },
            );
        }
        ("session/start", Some(id)) => {
            let model = params["modelId"].as_str().map(str::to_string);
            if let Some(model) = &model {
                if !listed_models(&agent.script).contains(model) {
                    agent.record(json!({ "event": "start_refused", "model": model }));
                    agent.refuse(&id, -32030, &format!("commandRejected: no model {model}"));
                    return;
                }
            }
            let session = agent.beside.begin_session();
            let servers = servers_listed(&params["config"]["mcpServers"]);
            let scope = Scope::of(&servers);
            agent.record(json!({
                "event": "session_start",
                "session": session,
                "cwd": params["workspaceRoot"],
                "model": model,
                "approval_mode": params["approvalMode"],
                "servers": servers,
                "scope": scope.kind,
                "agent": scope.named("agent"),
            }));
            let mode = params["approvalMode"].clone();
            agent.beside.change(&session, |known| {
                known["cwd"] = params["workspaceRoot"].clone();
                known["servers"] = servers;
                known["approval_mode"] = mode;
                if let Some(model) = &model {
                    known["model"] = json!(model);
                }
            });
            let view = session_view(agent, &session);
            notify(agent, "session/started", json!({ "session": view }));
            agent.answer(
                &id,
                json!({ "session": view, "viewCursor": format!("v4:{session}:1") }),
            );
        }
        ("session/resume", Some(id)) => {
            let session = text(&params["sessionId"]);
            if agent.beside.known(&session).is_none() {
                agent.record(json!({ "event": "resume_refused", "session": session }));
                agent.refuse(&id, -32020, &format!("sessionNotFound: {session}"));
                return;
            }
            let servers = servers_listed(&params["config"]["mcpServers"]);
            if !servers.as_array().is_some_and(Vec::is_empty) {
                agent
                    .beside
                    .change(&session, |known| known["servers"] = servers.clone());
            }
            agent.record(
                json!({ "event": "session_resume", "session": session, "servers": servers }),
            );
            let view = session_view(agent, &session);
            agent.answer(
                &id,
                json!({
                    "session": view,
                    "viewCursor": format!("v4:{session}:9"),
                    "history": { "mode": "none", "items": null, "noneReason": "excluded" },
                    "pendingRequests": [],
                    "lastTurn": null,
                }),
            );
        }
        ("session/setModel", Some(id)) => {
            let session = text(&params["sessionId"]);
            let value = text(&params["model"]["modelId"]);
            agent.record(json!({
                "event": "config",
                "session": session,
                "config_id": "model",
                "value": params["model"]["modelId"],
                "method": "session/setModel",
            }));
            if !listed_models(&agent.script).contains(&value) {
                agent.refuse(&id, -32030, &format!("commandRejected: no model {value}"));
                return;
            }
            agent.beside.change(&session, |known| {
                known["model"] = params["model"]["modelId"].clone()
            });
            agent.answer(
                &id,
                json!({ "commandId": params["commandId"], "status": "accepted" }),
            );
        }
        ("turn/start", Some(id)) => {
            let session = text(&params["sessionId"]);
            let prompt = params["input"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|part| part["type"] == json!("text"))
                .map(|part| text(&part["text"]))
                .collect::<Vec<_>>()
                .join("\n");
            let turn_id = format!("t-{}", text(&params["commandId"]));
            agent.record(json!({
                "event": "turn",
                "session": session,
                "text": prompt,
                "effort": params["reasoningEffort"],
                "if_busy": params["ifBusy"],
                "parts": params["input"].as_array().map(Vec::len).unwrap_or(0),
            }));
            agent.answer(
                &id,
                json!({
                    "commandId": params["commandId"],
                    "status": "accepted",
                    "turnId": turn_id,
                    "disposition": "started",
                    "startedNewTurn": true,
                }),
            );
            notify(
                agent,
                "turn/started",
                json!({ "sessionId": session, "turnId": turn_id }),
            );
            let end = turn(agent, &session, &turn_id, &prompt);
            notify(
                agent,
                "turn/completed",
                json!({
                    "sessionId": session,
                    "turnId": turn_id,
                    "terminal": match end { TurnEnd::Stop(_) => "completed", TurnEnd::Cancelled => "cancelled" },
                    "usage": { "inputTokens": 12, "outputTokens": 3 },
                }),
            );
        }
        ("turn/cancel", Some(id)) => {
            // Between two turns there is nothing to cancel.
            agent.record(
                json!({ "event": "cancel", "session": params["sessionId"], "during": "nothing" }),
            );
            agent.answer(
                &id,
                json!({ "commandId": params["commandId"], "status": "accepted" }),
            );
        }
        ("approval/decide", Some(id)) => {
            // A decision after its wait: accepted, and nothing moves.
            agent.answer(
                &id,
                json!({ "commandId": params["commandId"], "status": "accepted", "terminal": true }),
            );
        }
        (other, Some(id)) => agent.refuse(&id, -32601, &format!("methodNotFound: {other}")),
        (_, None) => {}
    }
}

/// One item, started and completed whole — or streamed as a delta between.
fn item(agent: &Agent, session: &str, turn_id: &str, item: Value, delta: Option<(&str, &str)>) {
    let mut started = item.clone();
    started["status"] = json!("inProgress");
    started["revision"] = json!(1);
    started["turnId"] = json!(turn_id);
    notify(
        agent,
        "item/started",
        json!({ "sessionId": session, "item": started }),
    );
    if let Some((field, text)) = delta {
        notify(
            agent,
            "item/delta",
            json!({ "sessionId": session, "itemId": item["itemId"], "field": field, "delta": text }),
        );
    }
    let mut completed = item;
    completed["status"] = json!(completed["status"].as_str().unwrap_or("completed"));
    completed["revision"] = json!(2);
    completed["turnId"] = json!(turn_id);
    notify(
        agent,
        "item/completed",
        json!({ "sessionId": session, "item": completed }),
    );
}

fn turn(agent: &mut Agent, session: &str, turn_id: &str, prompt: &str) -> TurnEnd {
    let known = agent.beside.known(session).unwrap_or(Value::Null);
    let scope = Scope::of(&known["servers"]);
    let chosen = agent.choose(&scope, prompt);
    agent.record(json!({
        "event": "prompt",
        "session": session,
        "scope": scope.kind,
        "agent": scope.named("agent"),
        "text": prompt,
        "turn": chosen.as_ref().map(|(place, _)| place.clone()),
    }));
    agent.remember(session, "person", prompt);
    let Some((_, script_turn)) = chosen else {
        return TurnEnd::Stop("completed".into());
    };
    let mut n = 0usize;
    let mut next_id = || {
        n += 1;
        format!("{turn_id}-i{n}")
    };
    if let Some(thought) = script_turn["think"].as_str() {
        let id = next_id();
        item(
            agent,
            session,
            turn_id,
            json!({ "itemId": id, "kind": "reasoning", "text": thought, "summary": [thought] }),
            Some(("summary.0", thought)),
        );
    }
    let cwd = PathBuf::from(text(&known["cwd"]));
    for file in script_turn["write"].as_array().into_iter().flatten() {
        agent.write(session, &cwd, file);
    }
    let calls: Vec<Value> = match &script_turn["tools"] {
        Value::String(word) if word == "every" => agent.every_tool(session, &known),
        listed => listed.as_array().cloned().unwrap_or_default(),
    };
    for (k, call) in calls.iter().enumerate() {
        if let Some(end) = tool_call(agent, session, turn_id, &known, &scope, prompt, k, call) {
            return end;
        }
    }
    for words in script_turn["say"].as_array().into_iter().flatten() {
        let words = text(&filled(words, &scope, &known, prompt));
        let id = next_id();
        item(
            agent,
            session,
            turn_id,
            json!({ "itemId": id, "kind": "agentMessage", "text": words }),
            Some(("text", &words)),
        );
        agent.remember(session, "agent", &words);
    }
    if script_turn["hold"].as_bool() == Some(true) {
        return hold(agent, session);
    }
    TurnEnd::Stop("completed".into())
}

/// One tool call as an item: announced with its arguments, asked about
/// where the script says so — the notification, then the request, settled
/// by the decide command — done, and completed or failed.
#[allow(clippy::too_many_arguments)]
fn tool_call(
    agent: &mut Agent,
    session: &str,
    turn_id: &str,
    known: &Value,
    scope: &Scope,
    prompt: &str,
    k: usize,
    call: &Value,
) -> Option<TurnEnd> {
    let name = text(&call["name"]);
    let server = call["server"]
        .as_str()
        .unwrap_or(super::PLATFORM_SERVER)
        .to_string();
    let arguments = filled(&call["arguments"], scope, known, prompt);
    let call_id = format!("call-{k}");
    let item_id = format!("{turn_id}-c{k}");
    let own = call["own"].as_str();
    let ask = call.get("ask").filter(|a| !a.is_null());
    let args_text = arguments.to_string();
    let mut started = json!({
        "itemId": item_id, "kind": "toolCall", "status": "inProgress", "revision": 1,
        "turnId": turn_id, "tool": name, "callId": call_id, "args": args_text,
    });
    notify(
        agent,
        "item/started",
        json!({ "sessionId": session, "item": started }),
    );
    if ask.is_some() {
        let raw = ask
            .and_then(|a| a.get("raw_input"))
            .cloned()
            .unwrap_or_else(|| arguments.clone());
        match approval(agent, session, turn_id, &item_id, &call_id, &name, &raw) {
            Asked::Allowed => {}
            Asked::Refused => {
                agent.record(json!({ "event": "tool_refused", "session": session, "name": name }));
                started["status"] = json!("rejected");
                started["revision"] = json!(2);
                started["failureKind"] = json!("denied");
                notify(
                    agent,
                    "item/completed",
                    json!({ "sessionId": session, "item": started }),
                );
                return None;
            }
            Asked::Cancelled => return Some(TurnEnd::Cancelled),
        }
    }
    let cwd = PathBuf::from(text(&known["cwd"]));
    let failed = match own {
        Some("edit") => agent.write(session, &cwd, &arguments),
        Some(_) => {
            let mut failed = false;
            for file in call["leaves"].as_array().into_iter().flatten() {
                failed |= agent.write(session, &cwd, file);
            }
            failed
        }
        None => {
            let answered = agent
                .client(session, &server, known)
                .and_then(|client| client.call(&name, &arguments));
            let (result, failed) = match answered {
                Ok(result) => {
                    let failed = result["isError"].as_bool() == Some(true);
                    (result, failed)
                }
                Err(e) => (json!({ "error": e }), true),
            };
            agent.record(json!({
                "event": "tool",
                "session": session,
                "server": server,
                "name": name,
                "arguments": arguments,
                "result": result,
                "failed": failed,
            }));
            failed
        }
    };
    started["status"] = json!(if failed { "failed" } else { "completed" });
    started["revision"] = json!(2);
    notify(
        agent,
        "item/completed",
        json!({ "sessionId": session, "item": started }),
    );
    None
}

enum Asked {
    Allowed,
    Refused,
    Cancelled,
}

/// Ask before a tool runs — the notification and the request, with the
/// three choices a host offers — and wait for the decide command, or for a
/// cancel.
#[allow(clippy::too_many_arguments)]
fn approval(
    agent: &mut Agent,
    session: &str,
    turn_id: &str,
    item_id: &str,
    call_id: &str,
    tool: &str,
    raw: &Value,
) -> Asked {
    let approval_id = format!("approval-{call_id}");
    let params = json!({
        "sessionId": session,
        "turnId": turn_id,
        "itemId": item_id,
        "toolCallId": call_id,
        "taskId": "task-0",
        "approvalId": approval_id,
        "toolName": tool,
        "rawArgs": raw.to_string(),
        "subject": { "kind": "shell", "command": raw["command"] },
        "currentRequirementId": { "approvalId": approval_id, "sourceIndex": 0 },
        "availableChoices": [
            { "choiceId": "allow_session", "decision": "approvedForSession", "scope": "session", "label": "Allow for this session" },
            { "choiceId": "allow_once", "decision": "approved", "scope": "once", "label": "Allow once" },
            { "choiceId": "abort", "decision": "abort", "scope": "once", "label": "Abort", "acceptsFeedback": true },
        ],
        "judgeEscalated": false,
        "protectedWrite": false,
        "viewCursor": format!("v4:{session}:5"),
    });
    notify(agent, "approval/requested", params.clone());
    let request = json!(format!("req-{approval_id}"));
    agent.send(
        json!({ "jsonrpc": "2.0", "id": request, "method": "approval/request", "params": params }),
    );
    let outcome = loop {
        let Some(message) = agent.next_message() else {
            break Asked::Cancelled;
        };
        if message["method"] == json!("turn/cancel") {
            agent.record(json!({ "event": "cancel", "session": session, "during": "a question" }));
            if let Some(id) = message.get("id") {
                agent.answer(
                    id,
                    json!({ "commandId": message["params"]["commandId"], "status": "accepted" }),
                );
            }
            break Asked::Cancelled;
        }
        if message["method"] == json!("approval/decide")
            && message["params"]["approvalId"] == json!(approval_id)
        {
            let choice = text(&message["params"]["choiceId"]);
            if let Some(id) = message.get("id") {
                agent.answer(
                    id,
                    json!({ "commandId": message["params"]["commandId"], "status": "accepted", "terminal": true }),
                );
            }
            let decision = match choice.as_str() {
                "allow_once" => "approved",
                "allow_session" => "approvedForSession",
                _ => "abort",
            };
            notify(
                agent,
                "approval/resolved",
                json!({ "sessionId": session, "approvalId": approval_id, "decision": decision, "policyResult": if decision.starts_with("approved") { "allow" } else { "deny" }, "resolvedBy": "user" }),
            );
            agent.record(json!({ "event": "choice", "session": session, "tool": tool, "choice": choice, "feedback": message["params"]["feedback"] }));
            break if decision.starts_with("approved") {
                Asked::Allowed
            } else {
                Asked::Refused
            };
        }
        // An answer to the request itself (the acknowledgement), or
        // anything else: handled or passed over.
        if message.get("id") == Some(&request) {
            continue;
        }
        handle(agent, message);
    };
    agent.record(json!({
        "event": "permission",
        "session": session,
        "tool": tool,
        "outcome": match outcome {
            Asked::Allowed => "allowed",
            Asked::Refused => "refused",
            Asked::Cancelled => "cancelled",
        },
    }));
    outcome
}

/// A turn that goes on until it is cancelled, or the client is gone.
fn hold(agent: &mut Agent, session: &str) -> TurnEnd {
    agent.record(json!({ "event": "holding", "session": session }));
    while let Some(message) = agent.next_message() {
        if message["method"] == json!("turn/cancel") {
            agent.record(json!({ "event": "cancel", "session": session, "during": "a turn" }));
            if let Some(id) = message.get("id") {
                agent.answer(
                    id,
                    json!({ "commandId": message["params"]["commandId"], "status": "accepted" }),
                );
            }
            return TurnEnd::Cancelled;
        }
        handle(agent, message);
    }
    TurnEnd::Cancelled
}
