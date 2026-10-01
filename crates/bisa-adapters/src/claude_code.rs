//! Claude Code adapter.
//!
//! Wire: `claude -p --output-format stream-json --input-format stream-json
//! --verbose --include-partial-messages` — NDJSON both ways. The process
//! serves multiple turns: each prompt is a `{"type":"user"}` line on stdin,
//! each turn ends with a `{"type":"result"}` event (mapped to a
//! **non-terminal** `Ended`), and the session ends for real when stdin closes
//! / the process exits. The words and the thinking stream as they are
//! written: `{"type":"stream_event"}` lines carry the API's
//! `content_block_delta` with a `text_delta` or a `thinking_delta`, mapped to
//! `TextDelta` and `ThinkingDelta`; the whole `assistant` message that follows
//! repeats neither — it is read for its `tool_use` blocks alone.
//!
//! Resume: `--resume <session_id>` (id learned from the `system:init` event).
//! Model and effort: `--model <id>` and `--effort <level>`, on a launch and
//! on a resume alike — the flag holds for the session it starts and is not
//! kept by the CLI, so a revived session is told again. Which levels a model
//! takes is [`efforts_for`]'s table.
//! MCP: `--mcp-config <inline json>`. Tool permissions in two layers:
//! `--allowedTools` pre-allows only what never needs a judgement — the
//! harness's own `TodoWrite` and the platform's injected MCP servers — and
//! `--permission-prompt-tool stdio`, the **control protocol**, turns every
//! other tool use — every read, edit and command — into a `control_request`
//! on stdout (`subtype: can_use_tool`) the adapter surfaces as
//! `InputRequested` and answers on stdin with a `control_response` once the
//! engine has decided: the guard's rules first, then the classifier, then the
//! tier ceiling.
//! `AskUserQuestion` arrives the same way and is a `Question`. Sub-agents:
//! a `Task` tool use is `SubagentStarted`, and every frame carrying
//! `parent_tool_use_id` is wrapped in `Nested` under it.
//!
//! **Model failures come typed here.** Claude Code puts the HTTP status of a
//! failed API call on its `result` frame as `api_error_status`, and announces
//! an internal model switch as `{"type":"system","subtype":"model_fallback",
//! "trigger":…}` (verified against the schemas shipped in the 2.1.246
//! binary). The adapter keys off those numbers and enums; the shared prose
//! table in [`crate::util`] is only the last resort, for the paths where the
//! CLI dies before it can emit a frame at all.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bisa_core::{HarnessCaps, ToolTier};
use bisa_harness::proc::{Line, ProcHandle, ProcSpec};
use bisa_harness::{
    Attachment, BoxEventStream, Effort, HarnessAdapter, HarnessError, HarnessSession, InputAnswer,
    InputRequest, InteractiveLaunch, LifecycleEvent, McpMount, McpServerConfig, ModelInfo,
    ModelPlan, Outcome, Phase, ProbeResult, ProgressEvent, PromptInput, ResumeToken, SessionEvent,
    SessionSnapshot, SessionSpec, Steer, SubagentId,
};
use tokio::sync::mpsc;

use crate::util::{self, Drive, OutMsg, Shared};
use bisa_core::sync::Locked;

pub const ADAPTER_ID: &str = "claude-code";

/// What every built-in agent runs on, in order: Opus 5.5, then Sonnet 5.5,
/// both with the 1M window. The judge runs on the second.
pub const RECOMMENDED_MODELS: [&str; 2] = ["claude-opus-5-5[1m]", "claude-sonnet-5-5[1m]"];

/// The suffix Claude Code takes on a model id or an alias for the 1M window.
const LONG_WINDOW: &str = "[1m]";

const FOUR_LEVELS: &[Effort] = &[Effort::Low, Effort::Medium, Effort::High, Effort::Max];
const FIVE_LEVELS: &[Effort] = &[
    Effort::Low,
    Effort::Medium,
    Effort::High,
    Effort::Xhigh,
    Effort::Max,
];

/// The models that take `xhigh` beside the four levels, by id and by alias.
const FIVE_LEVEL_MODELS: &[&str] = &[
    "claude-fable-5-1",
    "claude-fable-5",
    "claude-opus-5-5",
    "claude-opus-5",
    "claude-opus-4-8",
    "claude-opus-4-7",
    "claude-sonnet-5-5",
    "claude-sonnet-5",
    "opus",
    "sonnet",
    "fable",
    "best",
    "opusplan",
    "default",
];

/// The levels `--effort` takes for `model`, lowest first
/// (https://code.claude.com/docs/en/model-config, read 2026-09-29). The
/// `[1m]` suffix names a window, not a model, and is set aside first. Haiku
/// has no effort control; Fable, Opus from 4.7 and Sonnet from 5 take five
/// levels; Opus 4.6 and Sonnet 4.6 take four. A model this table does not
/// know, and no model at all, get the four every model with the control
/// takes: a level the CLI refuses would fail the launch.
pub fn efforts_for(model: Option<&str>) -> &'static [Effort] {
    let Some(model) = model.map(str::trim).filter(|m| !m.is_empty()) else {
        return FOUR_LEVELS;
    };
    let id = model.strip_suffix(LONG_WINDOW).unwrap_or(model);
    if id == "haiku" || id.starts_with("claude-haiku-") {
        &[]
    } else if FIVE_LEVEL_MODELS.contains(&id) {
        FIVE_LEVELS
    } else {
        FOUR_LEVELS
    }
}

/// The effort to send for `model`: the one asked, held to what the model
/// takes. The engine clamps before it launches; this is the adapter's own
/// word that a level the CLI would refuse never reaches it.
fn effort_sent(model: Option<&str>, effort: Option<Effort>) -> Option<Effort> {
    effort.and_then(|e| e.clamp_to(efforts_for(model)))
}

/// The protocol session's arguments, in the order the CLI is given them:
/// the stream-json wire, `--model <id>`, `--effort <level>` right after it
/// (https://code.claude.com/docs/en/cli-reference, read 2026-09-29: the flag
/// holds for this session and does not persist), the session to resume, the
/// MCP servers, the allowlist.
fn argv(spec: &SessionSpec, resume: Option<&str>) -> Vec<String> {
    let mut args: Vec<String> = [
        "-p",
        "--output-format",
        "stream-json",
        "--input-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--permission-prompt-tool",
        "stdio",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    if let Some(model) = &spec.model {
        args.extend(["--model".to_string(), model.clone()]);
    }
    if let Some(effort) = effort_sent(spec.model.as_deref(), spec.effort) {
        args.extend(["--effort".to_string(), effort.as_str().to_string()]);
    }
    if let Some(id) = resume {
        args.extend(["--resume".to_string(), id.to_string()]);
    }
    if !spec.mcp_servers.is_empty() {
        args.extend([
            "--mcp-config".to_string(),
            mcp_config_json(&spec.mcp_servers),
        ]);
    }
    args.extend(["--allowedTools".to_string(), allowed_tools(spec)]);
    args
}

/// The spec a session is revived with: where it ran, and the model and the
/// effort it ran with. The caller supplies the next prompt, so there is none
/// here, and the platform's servers are the next launch's to mount.
fn revival_spec(token: &ResumeToken) -> SessionSpec {
    SessionSpec {
        work_item: None,
        cwd: token.cwd.clone(),
        prompt: String::new(),
        model: token.model.clone(),
        effort: token.effort,
        mcp_servers: vec![],
        env: Default::default(),
        env_remove: Vec::new(),
        tier_ceiling: ToolTier::Write,
        output_schema: None,
        skills: vec![],
    }
}

/// The curated list: the two defaults, their bare ids, Fable and Haiku, the
/// aliases, then the earlier models. Both the `[1m]` and the bare form are
/// listed, because a step's model pin is checked by exact equality.
const LISTED_MODELS: &[(&str, &str)] = &[
    ("claude-opus-5-5[1m]", "Claude Opus 5.5 (1M context)"),
    ("claude-sonnet-5-5[1m]", "Claude Sonnet 5.5 (1M context)"),
    ("claude-opus-5-5", "Claude Opus 5.5"),
    ("claude-sonnet-5-5", "Claude Sonnet 5.5"),
    ("claude-fable-5-1", "Claude Fable 5.1"),
    ("claude-haiku-4-5", "Claude Haiku 4.5"),
    ("opus", "Opus (latest alias)"),
    ("sonnet", "Sonnet (latest alias)"),
    ("haiku", "Haiku (latest alias)"),
    ("fable", "Fable (latest alias)"),
    ("best", "Best (alias)"),
    ("opus[1m]", "Opus, 1M context (latest alias)"),
    ("sonnet[1m]", "Sonnet, 1M context (latest alias)"),
    ("opusplan", "Opusplan (alias)"),
    ("claude-opus-5", "Claude Opus 5"),
    ("claude-fable-5", "Claude Fable 5"),
    ("claude-opus-4-8", "Claude Opus 4.8"),
    ("claude-opus-4-7", "Claude Opus 4.7"),
    ("claude-sonnet-5", "Claude Sonnet 5"),
    ("claude-sonnet-4-6", "Claude Sonnet 4.6"),
];

pub struct ClaudeCodeAdapter {
    /// Binary name; overridable for tests (stub scripts).
    pub program: String,
    /// The engine's clients, for the account's usage endpoint — the proxy
    /// the person chose; the process's shared set until `with_http`.
    pub http: Arc<bisa_http::Clients>,
}

impl Default for ClaudeCodeAdapter {
    fn default() -> Self {
        Self {
            program: "claude".into(),
            http: bisa_http::Clients::shared(),
        }
    }
}

impl ClaudeCodeAdapter {
    fn caps_() -> HarnessCaps {
        HarnessCaps::FOLLOW_UP
            | HarnessCaps::RESUME
            | HarnessCaps::MCP_SERVERS
            | HarnessCaps::IMAGE_INPUT
            | HarnessCaps::COST_REPORTING
            | HarnessCaps::INPUT_REQUESTS
            | HarnessCaps::SUBAGENTS
            | HarnessCaps::TOOL_GUARD
            | HarnessCaps::INPUT_REWRITE
            | HarnessCaps::USAGE_REPORTING
            | HarnessCaps::EFFORT
    }

    async fn launch_inner(
        &self,
        spec: &SessionSpec,
        resume: Option<&str>,
    ) -> Result<Box<dyn HarnessSession>, HarnessError> {
        let mut proc_spec = ProcSpec::new(&self.program)
            .args(argv(spec, resume))
            .cwd(&spec.cwd);
        for (k, v) in &spec.env {
            proc_spec = proc_spec.env(k, v);
        }
        for k in &spec.env_remove {
            proc_spec = proc_spec.env_remove(k);
        }

        let proc = ProcHandle::spawn(proc_spec)?;
        let (out_tx, out_rx) = mpsc::channel(64);
        let shared = Shared::new(out_tx, ADAPTER_ID, spec.model.clone(), &spec.cwd)
            .at_effort(effort_sent(spec.model.as_deref(), spec.effort));
        shared
            .broadcaster
            .emit(SessionEvent::Lifecycle(LifecycleEvent::Started));
        shared.set_phase(Phase::Idle);

        let wire: WireState = Arc::default();
        let driver_wire = Arc::clone(&wire);
        let driver_shared = shared.clone();
        tokio::spawn(util::drive(
            proc,
            out_rx,
            driver_shared,
            move |shared, line| map_line(shared, line, &driver_wire),
        ));

        let session = ClaudeCodeSession {
            shared,
            wire,
            transcript: None,
        };
        if !spec.prompt.is_empty() {
            // A launch prompt carries no attachments: `SessionSpec` is how a
            // session starts, and a conversation's files arrive with the turn.
            session.send_prompt(&spec.prompt, &[]).await?;
        }
        Ok(Box::new(session))
    }
}

#[async_trait]
impl HarnessAdapter for ClaudeCodeAdapter {
    fn id(&self) -> &str {
        ADAPTER_ID
    }

    fn display_name(&self) -> &str {
        "Claude Code"
    }

    /// The account's five-hour and weekly windows and the per-model weeks
    /// the plan carries (*Fable* on Max), from the provider's usage endpoint
    /// with Claude Code's own sign-in.
    async fn usage(&self) -> bisa_harness::UsageState {
        crate::usage::claude::read(&self.http, self.id()).await
    }

    fn caps(&self) -> HarnessCaps {
        Self::caps_()
    }

    /// Claude Code has no list-models command, so this is a curated list:
    /// the current model ids and the aliases `claude --model` accepts, each
    /// with the efforts it takes
    /// (https://platform.claude.com/docs/en/about-claude/models/overview and
    /// https://code.claude.com/docs/en/model-config, read 2026-09-29).
    /// Reviewed 2026-09; `--model` also takes any id verbatim, and the UI
    /// keeps free-text entry, so a newer model is never locked out.
    async fn models(&self) -> Vec<ModelInfo> {
        LISTED_MODELS
            .iter()
            .map(|&(id, label)| ModelInfo::new(id, Some(label), efforts_for(Some(id)).to_vec()))
            .collect()
    }

    fn efforts(&self, model: Option<&str>) -> Vec<Effort> {
        efforts_for(model).to_vec()
    }

    fn recommended_plan(&self) -> Option<ModelPlan> {
        Some(ModelPlan::fallback(RECOMMENDED_MODELS))
    }

    fn recommended_judge(&self) -> Option<String> {
        Some(RECOMMENDED_MODELS[1].to_string())
    }

    /// The bare CLI. `launch` above runs it in a bidirectional stream-json session; this is the command a
    /// person types.
    fn interactive(&self) -> Option<InteractiveLaunch> {
        // `--continue`, not `--resume`: the latter opens a picker of this
        // directory's sessions, and the ask is for the most recent one without
        // a question. The engine's own resume above names an id because it
        // knows one; a person opening a terminal does not.
        Some(InteractiveLaunch::new(&self.program).resumable_with(vec!["--continue".into()]))
    }

    fn interactive_reporting(
        &self,
        ctx: &bisa_harness::ReportingContext,
    ) -> bisa_harness::ReportingPlan {
        crate::hooks::claude_code::reporting(ctx)
    }

    fn translate_report(&self, payload: &serde_json::Value) -> Vec<SessionEvent> {
        crate::hooks::claude_code::translate(payload)
    }

    async fn probe(&self) -> ProbeResult {
        util::probe_binary(&self.program, &["--version"]).await
    }

    async fn launch(&self, spec: SessionSpec) -> Result<Box<dyn HarnessSession>, HarnessError> {
        self.launch_inner(&spec, None).await
    }

    async fn attach(&self, token: &ResumeToken) -> Result<Box<dyn HarnessSession>, HarnessError> {
        if token.adapter_id != ADAPTER_ID {
            return Err(HarnessError::protocol(format!(
                "resume token for {:?} handed to {ADAPTER_ID}",
                token.adapter_id
            )));
        }
        // Re-attach continues the conversation; the caller supplies the next
        // prompt via `prompt()`, so the launch prompt is empty here — but
        // Claude Code needs *a* first message, so we use a neutral resume nudge.
        // A resumed session is placed, not defaulted. The token carries the
        // directory the launch chose; deriving it here from the transcript's
        // parent was the subtler of two wrong answers, because a Claude Code
        // transcript lives under the harness's own data directory rather than
        // beside the work. It carries the model and the effort for the same
        // reason: the CLI keeps the conversation, not the flags it was
        // started with.
        let spec = revival_spec(token);
        let mut proc_spec = ProcSpec::new(&self.program)
            .args(argv(&spec, Some(token.native_id.as_str())))
            .cwd(&spec.cwd);
        for (k, v) in &spec.env {
            proc_spec = proc_spec.env(k, v);
        }
        for k in &spec.env_remove {
            proc_spec = proc_spec.env_remove(k);
        }
        let proc = ProcHandle::spawn(proc_spec)?;
        let (out_tx, out_rx) = mpsc::channel(64);
        let shared = Shared::new(out_tx, ADAPTER_ID, spec.model.clone(), &spec.cwd)
            .at_effort(effort_sent(spec.model.as_deref(), spec.effort));
        shared.set_native_id(&token.native_id);
        shared
            .broadcaster
            .emit(SessionEvent::Lifecycle(LifecycleEvent::Revived));
        shared.set_phase(Phase::Idle);
        let wire: WireState = Arc::default();
        let driver_wire = Arc::clone(&wire);
        let driver_shared = shared.clone();
        tokio::spawn(util::drive(
            proc,
            out_rx,
            driver_shared,
            move |shared, line| map_line(shared, line, &driver_wire),
        ));
        Ok(Box::new(ClaudeCodeSession {
            shared,
            wire,
            transcript: None,
        }))
    }
}

/// What the CLI may run without asking: the agent's own to-do list and the
/// MCP servers the engine injected as the platform's own (trusted by
/// construction — the engine builds them and judges their tools at the
/// intake). Every other tool — a read, a write, a shell command, and every
/// tool of an MCP server a person installed on the agent — comes back as a
/// `can_use_tool` control request, so the engine's guard sees the call
/// before anything runs and can hand back the input to run with. The tier
/// ceiling is the engine's to apply, in one place for every harness, not a
/// pre-computed allowlist here.
fn allowed_tools(spec: &SessionSpec) -> String {
    let mut tools: Vec<String> = vec!["TodoWrite".into()];
    for mount in spec.mcp_servers.iter().filter(|m| m.is_platform()) {
        tools.push(format!("mcp__{}", mount.name()));
    }
    tools.join(",")
}

/// The `--mcp-config` JSON, as Claude Code's own reference shapes it
/// (code.claude.com/docs/en/mcp): a stdio entry is `command`, `args`,
/// `env` — the reference has no `cwd`, so one is dropped with a warning; a
/// remote entry is `type: "http"` or `type: "sse"` with `url` and `headers`.
fn mcp_config_json(mounts: &[McpMount]) -> String {
    let mut map = serde_json::Map::new();
    for mount in mounts {
        let server = &mount.config;
        let entry = match server {
            McpServerConfig::Stdio {
                command,
                args,
                env,
                cwd,
                ..
            } => {
                if cwd.is_some() {
                    crate::mcp_inject::warn_dropped("claude-code", server.name(), "cwd");
                }
                serde_json::json!({ "command": command, "args": args, "env": env })
            }
            McpServerConfig::Http { url, headers, .. } => {
                serde_json::json!({ "type": "http", "url": url, "headers": headers })
            }
            McpServerConfig::Sse { url, headers, .. } => {
                serde_json::json!({ "type": "sse", "url": url, "headers": headers })
            }
        };
        map.insert(server.name().to_string(), entry);
    }
    serde_json::json!({ "mcpServers": map }).to_string()
}

/// What the driver and the session both need to know about the wire: which
/// tool-use ids are open (and which of them are sub-agents), and which
/// control requests wait for an answer, with the input each carried.
#[derive(Default)]
struct Wire {
    tool_uses: HashMap<String, ToolUse>,
    pending: HashMap<String, serde_json::Value>,
}

struct ToolUse {
    name: String,
    subagent: bool,
}

type WireState = Arc<Mutex<Wire>>;

/// The sub-agent a frame was produced in, when the CLI says so.
fn parent_of(value: &serde_json::Value) -> Option<SubagentId> {
    value
        .get("parent_tool_use_id")
        .and_then(|p| p.as_str())
        .map(|p| SubagentId(p.to_string()))
}

/// A `can_use_tool` control request as an [`InputRequest`]: a question when
/// the tool is `AskUserQuestion`, a permission otherwise.
pub(crate) fn input_request(
    request_id: &str,
    request: &serde_json::Value,
    parent: Option<SubagentId>,
) -> InputRequest {
    let tool_name = request
        .get("tool_name")
        .and_then(|n| n.as_str())
        .unwrap_or("?")
        .to_string();
    let input = request
        .get("input")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let req = if tool_name == "AskUserQuestion" {
        let first = input
            .get("questions")
            .and_then(|q| q.as_array())
            .and_then(|q| q.first())
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let text = first
            .get("question")
            .and_then(|q| q.as_str())
            .unwrap_or("The agent has a question")
            .to_string();
        let options = first
            .get("options")
            .and_then(|o| o.as_array())
            .map(|o| {
                o.iter()
                    .filter_map(|opt| {
                        opt.get("label")
                            .and_then(|l| l.as_str())
                            .map(str::to_string)
                    })
                    .collect()
            })
            .unwrap_or_default();
        InputRequest::question(request_id, text, options)
    } else {
        InputRequest::permission(
            request_id,
            tool_name.clone(),
            ToolTier::classify(&normalize_tool(&tool_name)),
            util::summarize_args(&input, 160),
            input,
        )
    };
    req.raised_by(parent)
}

/// The `control_response` that answers a pending request. An `Allow` runs
/// the tool with the input the engine hands back when it gives one (a
/// restored placeholder), else with the input asked about; `Text` on an
/// `AskUserQuestion` fills `answers` for every question it asked; `Text` on a
/// permission is an allow.
fn control_response(
    request_id: &str,
    input: serde_json::Value,
    answer: InputAnswer,
) -> serde_json::Value {
    let response = match answer {
        InputAnswer::Allow { input: rewritten } => {
            serde_json::json!({ "behavior": "allow", "updatedInput": rewritten.unwrap_or(input) })
        }
        InputAnswer::Deny { reason } => {
            serde_json::json!({ "behavior": "deny", "message": reason })
        }
        InputAnswer::Text { text } => {
            let mut updated = input;
            let mut answers = serde_json::Map::new();
            if let Some(questions) = updated.get("questions").and_then(|q| q.as_array()) {
                for q in questions {
                    if let Some(question) = q.get("question").and_then(|s| s.as_str()) {
                        answers.insert(
                            question.to_string(),
                            serde_json::Value::String(text.clone()),
                        );
                    }
                }
            }
            if !answers.is_empty() {
                updated["answers"] = serde_json::Value::Object(answers);
            }
            serde_json::json!({ "behavior": "allow", "updatedInput": updated })
        }
    };
    serde_json::json!({
        "type": "control_response",
        "response": { "subtype": "success", "request_id": request_id, "response": response }
    })
}

/// Map one stream-json line to session events.
fn map_line(shared: &Shared, line: Line, wire: &WireState) -> Drive {
    let value = match line {
        Line::Json(v) => v,
        Line::Text(t) => {
            tracing::debug!(target: "bisa_adapters::claude", "non-json stdout: {t}");
            return Drive::Continue;
        }
    };
    let kind = value.get("type").and_then(|t| t.as_str()).unwrap_or("");
    match kind {
        // The control protocol: the CLI asks before a tool outside the
        // allowlist runs, and stops until we answer (`answer` below).
        "control_request" => {
            let request_id = value.get("request_id").and_then(|r| r.as_str());
            let request = value.get("request");
            let subtype = request
                .and_then(|r| r.get("subtype"))
                .and_then(|s| s.as_str());
            match (request_id, request, subtype) {
                (Some(request_id), Some(request), Some("can_use_tool")) => {
                    let req = input_request(request_id, request, parent_of(&value));
                    wire.locked().pending.insert(
                        request_id.to_string(),
                        request
                            .get("input")
                            .cloned()
                            .unwrap_or(serde_json::Value::Null),
                    );
                    shared.set_activity(format!("asked to {}", req.summary()));
                    shared.set_phase(Phase::AwaitingInput);
                    shared.broadcaster.emit(SessionEvent::Lifecycle(
                        LifecycleEvent::InputRequested { request: req },
                    ));
                }
                _ => shared.broadcaster.emit(SessionEvent::Raw(value)),
            }
        }
        "system" => {
            match value.get("subtype").and_then(|s| s.as_str()) {
                Some("init") => {
                    if let Some(id) = value.get("session_id").and_then(|s| s.as_str()) {
                        shared.set_native_id(id);
                    }
                    // The model the session opened on, as the CLI names it.
                    if let Some(model) = value
                        .get("model")
                        .and_then(|m| m.as_str())
                        .filter(|m| !m.is_empty())
                    {
                        shared.broadcaster.emit(SessionEvent::Progress(
                            ProgressEvent::ModelChanged {
                                model: model.to_string(),
                            },
                        ));
                    }
                }
                // Typed signal: the CLI switched the turn to its *own*
                // configured fallback because the primary model failed
                // (trigger ∈ model_not_found | permission_denied | overloaded
                // | server_error | last_resort | model_blocked). The turn is
                // still running and still useful, so this is not an end — but
                // the plan's model just went away, so it is worth saying so
                // loudly rather than letting it pass as noise.
                Some("model_fallback") => {
                    tracing::warn!(
                        target: "bisa_adapters::claude",
                        trigger = value.get("trigger").and_then(|v| v.as_str()).unwrap_or("?"),
                        original = value.get("original_model").and_then(|v| v.as_str()).unwrap_or("?"),
                        fallback = value.get("fallback_model").and_then(|v| v.as_str()).unwrap_or("?"),
                        "claude code switched models under us",
                    );
                    // The row follows the switch: it now runs on the fallback.
                    if let Some(model) = value
                        .get("fallback_model")
                        .and_then(|v| v.as_str())
                        .filter(|m| !m.is_empty())
                    {
                        shared.broadcaster.emit(SessionEvent::Progress(
                            ProgressEvent::ModelChanged {
                                model: model.to_string(),
                            },
                        ));
                    }
                }
                _ => {}
            }
            shared.broadcaster.emit(SessionEvent::Raw(value));
        }
        "assistant" => {
            let parent = parent_of(&value);
            let blocks = value
                .pointer("/message/content")
                .and_then(|c| c.as_array())
                .cloned()
                .unwrap_or_default();
            // The words and the thinking were streamed by the `stream_event`
            // lines before this whole message arrived: a `text` or a
            // `thinking` block here is said already, and is not said twice.
            for block in blocks {
                if let Some("tool_use") = block.get("type").and_then(|t| t.as_str()) {
                    {
                        let name = block
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("?")
                            .to_string();
                        let id = block.get("id").and_then(|i| i.as_str()).map(str::to_string);
                        let args = block
                            .get("input")
                            .cloned()
                            .unwrap_or(serde_json::Value::Null);
                        // The sub-agent tool — `Agent` since 2.1.63, `Task` its
                        // alias — is a sub-agent, not a tool: it gets its own
                        // start and end, and everything it does arrives nested.
                        let subagent =
                            matches!(name.as_str(), "Task" | "Agent") && parent.is_none();
                        if let Some(id) = &id {
                            wire.locked().tool_uses.insert(
                                id.clone(),
                                ToolUse {
                                    name: name.clone(),
                                    subagent,
                                },
                            );
                        }
                        if subagent {
                            let sub_name = args
                                .get("subagent_type")
                                .and_then(|s| s.as_str())
                                .unwrap_or("agent")
                                .to_string();
                            let description = args
                                .get("description")
                                .and_then(|s| s.as_str())
                                .unwrap_or("")
                                .to_string();
                            shared.set_activity(format!("sub-agent {sub_name}"));
                            shared.broadcaster.emit(SessionEvent::Progress(
                                ProgressEvent::SubagentStarted {
                                    id: SubagentId(id.unwrap_or_else(|| "?".into())),
                                    name: sub_name,
                                    description: util::summarize_args(
                                        &serde_json::Value::String(description),
                                        160,
                                    ),
                                },
                            ));
                            continue;
                        }
                        shared.set_activity(name.clone());
                        shared.broadcaster.emit(SessionEvent::Progress(
                            ProgressEvent::ToolStarted {
                                tier: ToolTier::classify(&normalize_tool(&name)),
                                args_summary: util::summarize_args(&args, 160),
                                name,
                            }
                            .raised_by(parent.clone()),
                        ));
                    }
                }
            }
        }
        // A token of the reply, or of the reasoning before it, as the API
        // streams it (`--include-partial-messages`): the one source of the
        // agent's words and thinking, in order, as they are written.
        "stream_event" => {
            let parent = parent_of(&value);
            let event = value
                .get("event")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            if event.get("type").and_then(|t| t.as_str()) == Some("content_block_delta") {
                let delta = event
                    .get("delta")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                match delta.get("type").and_then(|t| t.as_str()) {
                    Some("text_delta") => {
                        if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                            shared.broadcaster.emit(SessionEvent::Progress(
                                ProgressEvent::TextDelta {
                                    text: text.to_string(),
                                }
                                .raised_by(parent),
                            ));
                        }
                    }
                    Some("thinking_delta") => {
                        if let Some(text) = delta.get("thinking").and_then(|t| t.as_str()) {
                            shared.broadcaster.emit(SessionEvent::Progress(
                                ProgressEvent::ThinkingDelta {
                                    text: text.to_string(),
                                }
                                .raised_by(parent),
                            ));
                        }
                    }
                    _ => {}
                }
            }
        }
        "user" => {
            // Tool results come back as user messages with tool_result blocks.
            let parent = parent_of(&value);
            let blocks = value
                .pointer("/message/content")
                .and_then(|c| c.as_array())
                .cloned()
                .unwrap_or_default();
            for block in blocks {
                if block.get("type").and_then(|t| t.as_str()) == Some("tool_result") {
                    let ok = !block
                        .get("is_error")
                        .and_then(|e| e.as_bool())
                        .unwrap_or(false);
                    let id = block
                        .get("tool_use_id")
                        .and_then(|i| i.as_str())
                        .map(str::to_string);
                    let use_ = id
                        .as_ref()
                        .and_then(|id| wire.locked().tool_uses.remove(id));
                    match use_ {
                        Some(ToolUse { subagent: true, .. }) => {
                            shared.broadcaster.emit(SessionEvent::Progress(
                                ProgressEvent::SubagentEnded {
                                    id: SubagentId(id.unwrap_or_else(|| "?".into())),
                                    ok,
                                },
                            ));
                        }
                        Some(ToolUse { name, .. }) => {
                            shared.broadcaster.emit(SessionEvent::Progress(
                                ProgressEvent::ToolEnded { name, ok }.raised_by(parent.clone()),
                            ));
                        }
                        None => {
                            shared.broadcaster.emit(SessionEvent::Progress(
                                ProgressEvent::ToolEnded {
                                    name: "?".to_string(),
                                    ok,
                                }
                                .raised_by(parent.clone()),
                            ));
                        }
                    }
                }
            }
        }
        "result" => {
            if let Some(id) = value.get("session_id").and_then(|s| s.as_str()) {
                shared.set_native_id(id);
            }
            let usage = value.get("usage");
            let input = usage
                .and_then(|u| u.get("input_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let output = usage
                .and_then(|u| u.get("output_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let cents = value
                .get("total_cost_usd")
                .and_then(|v| v.as_f64())
                .map(|usd| (usd * 100.0).round() as u64)
                .unwrap_or(0);
            shared.add_cost(input, output, cents);
            shared
                .broadcaster
                .emit(SessionEvent::Progress(ProgressEvent::CostDelta {
                    input_tokens: input,
                    output_tokens: output,
                    usd_cents: cents,
                }));
            shared
                .broadcaster
                .emit(SessionEvent::Progress(ProgressEvent::TurnEnded));

            let is_error = value
                .get("is_error")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let subtype = value
                .get("subtype")
                .and_then(|v| v.as_str())
                .unwrap_or("success");
            let outcome = if is_error || subtype != "success" {
                result_failure(shared, &value, subtype)
            } else {
                Outcome::Completed
            };
            // Turn end, session still open for follow-ups: non-terminal.
            shared.set_phase(Phase::Idle);
            shared
                .broadcaster
                .emit(SessionEvent::Lifecycle(LifecycleEvent::Ended {
                    outcome,
                    is_terminal: false,
                }));
        }
        _ => shared.broadcaster.emit(SessionEvent::Raw(value)),
    }
    Drive::Continue
}

/// Decide what a failed `result` frame means. Typed signals first, prose last.
///
/// Order, and why:
/// 1. `api_error_status` — an integer HTTP status Claude Code puts on the
///    frame when the turn died on an API call. 429/529 are the model saying
///    "not now", 404/403 are "not this model, not for you". Nothing to parse.
/// 2. `error` — the CLI's own error code enum, when present.
/// 3. `result` / `errors[]` prose, through the shared table.
/// 4. Otherwise an honest `Failed` carrying the subtype and the message.
fn result_failure(shared: &Shared, value: &serde_json::Value, subtype: &str) -> Outcome {
    // 1. Typed: HTTP status of the failed API call.
    if let Some(status) = value
        .get("api_error_status")
        .and_then(serde_json::Value::as_u64)
    {
        let typed = match status {
            429 => Some("rate limited (HTTP 429)"),
            529 => Some("provider overloaded (HTTP 529)"),
            404 => Some("model not found (HTTP 404)"),
            403 => Some("model not permitted for this account (HTTP 403)"),
            _ => None,
        };
        if let Some(reason) = typed {
            return shared.model_ctx.typed_outcome(reason, None);
        }
    }
    // 2. Typed: the CLI's error-code enum.
    if let Some(code) = value.get("error").and_then(|v| v.as_str()) {
        let typed = match code {
            "rate_limit" => Some("rate limited"),
            "overloaded" => Some("provider overloaded"),
            "model_not_found" => Some("model not found"),
            "billing_error" => Some("billing/credit exhausted"),
            "account_on_hold" => Some("account on hold"),
            _ => None,
        };
        if let Some(reason) = typed {
            return shared.model_ctx.typed_outcome(reason, None);
        }
    }
    // 3/4. Prose, from the frame's error channels only — never from
    // assistant text.
    let mut text = String::new();
    if let Some(msg) = value.get("result").and_then(|v| v.as_str()) {
        text.push_str(msg);
    }
    if let Some(errors) = value.get("errors").and_then(|v| v.as_array()) {
        for e in errors.iter().filter_map(|e| e.as_str()) {
            text.push('\n');
            text.push_str(e);
        }
    }
    if let Some(outcome) = shared.model_ctx.outcome(&text) {
        return outcome;
    }
    Outcome::Failed {
        error: if text.trim().is_empty() {
            format!("result subtype: {subtype}")
        } else {
            format!("result subtype: {subtype}; {}", text.trim())
        },
    }
}

/// Claude tool names are TitleCase; the shared classifier speaks snake_case.
pub(crate) fn normalize_tool(name: &str) -> String {
    match name {
        "Read" => "read",
        "Grep" => "grep",
        "Glob" => "glob",
        "LS" => "ls",
        "WebSearch" => "web_search",
        "WebFetch" => "fetch",
        "Edit" => "edit",
        "MultiEdit" => "multi_edit",
        "Write" => "write",
        other => return other.to_ascii_lowercase(),
    }
    .to_string()
}

pub struct ClaudeCodeSession {
    shared: Shared,
    wire: WireState,
    transcript: Option<std::path::PathBuf>,
}

/// The content array for one user turn: the text, and any images beside it.
///
/// **This is the one place in the tree where an attachment is more than a
/// path.** `HarnessCaps::IMAGE_INPUT` is declared by this adapter alone, and
/// this function is what it means: claude-code's wire already carries `content`
/// as an array of blocks, so an image goes in as an image rather than as a
/// filename the model has to go and read.
///
/// Non-image attachments are deliberately left out. The transcript already
/// names every attachment with its absolute path, and a PDF is something the
/// harness reads with its own tools — inlining one here would be a large
/// base64 payload the model cannot use.
///
/// A file that will not read is skipped with a warning rather than failing the
/// turn: the path is still in the text, so the model can try itself, and a
/// missing byte is not a reason for a person's question to go unanswered.
fn user_content(text: &str, attachments: &[Attachment]) -> serde_json::Value {
    let mut blocks = Vec::new();
    if !text.is_empty() {
        blocks.push(serde_json::json!({ "type": "text", "text": text }));
    }
    for a in attachments.iter().filter(|a| a.is_image()) {
        match std::fs::read(&a.path) {
            Ok(bytes) => blocks.push(serde_json::json!({
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": a.mime,
                    "data": base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        &bytes,
                    ),
                }
            })),
            Err(e) => tracing::warn!(
                "could not read {} to send as an image: {e}; its path is in the prompt",
                a.path.display()
            ),
        }
    }
    serde_json::Value::Array(blocks)
}

impl ClaudeCodeSession {
    async fn send_prompt(
        &self,
        text: &str,
        attachments: &[Attachment],
    ) -> Result<(), HarnessError> {
        if text.is_empty() && attachments.is_empty() {
            return Ok(());
        }
        self.shared.set_phase(Phase::Turn);
        self.shared
            .broadcaster
            .emit(SessionEvent::Progress(ProgressEvent::TurnStarted));
        self.shared
            .send(OutMsg::Json(serde_json::json!({
                "type": "user",
                "message": { "role": "user", "content": user_content(text, attachments) }
            })))
            .await
    }
}

#[async_trait]
impl HarnessSession for ClaudeCodeSession {
    fn snapshot(&self) -> SessionSnapshot {
        self.shared.snapshot()
    }

    fn phase(&self) -> Phase {
        self.shared.phase()
    }

    async fn prompt(&self, input: PromptInput) -> Result<(), HarnessError> {
        match self.shared.phase() {
            Phase::Turn => Err(HarnessError::Busy),
            Phase::Ended => Err(HarnessError::Terminated),
            _ => self.send_prompt(&input.text, &input.attachments).await,
        }
    }

    async fn steer(&self, _msg: Steer) -> Result<(), HarnessError> {
        Err(HarnessError::NotSupported("steer"))
    }

    async fn follow_up(&self, msg: Steer) -> Result<(), HarnessError> {
        if self.shared.is_ended() {
            return Err(HarnessError::Terminated);
        }
        // Between turns a follow-up *is* the next turn: announce it so
        // consumers that track turn boundaries (chat reply capture) don't
        // fold it into the previous one. Mid-turn it stays queued and the
        // in-flight turn keeps its own boundary.
        if !matches!(self.shared.phase(), Phase::Turn) {
            self.shared.set_phase(Phase::Turn);
            self.shared
                .broadcaster
                .emit(SessionEvent::Progress(ProgressEvent::TurnStarted));
        }
        self.shared
            .send(OutMsg::Json(serde_json::json!({
                "type": "user",
                "message": {
                    "role": "user",
                    "content": user_content(&msg.text, &msg.attachments),
                }
            })))
            .await
    }

    async fn abort(&self) -> Result<(), HarnessError> {
        self.shared.send(OutMsg::Kill).await
    }

    async fn answer(&self, request_id: &str, answer: InputAnswer) -> Result<(), HarnessError> {
        let input = self
            .wire
            .locked()
            .pending
            .remove(request_id)
            .ok_or_else(|| {
                HarnessError::protocol(format!("no control request {request_id} is waiting"))
            })?;
        self.shared
            .send(OutMsg::Json(control_response(request_id, input, answer)))
            .await?;
        self.shared.set_phase(Phase::Turn);
        self.shared
            .broadcaster
            .emit(SessionEvent::Lifecycle(LifecycleEvent::InputResolved {
                id: request_id.to_string(),
            }));
        Ok(())
    }

    fn subscribe(&self) -> BoxEventStream {
        self.shared.broadcaster.subscribe()
    }

    fn resume_token(&self) -> Option<ResumeToken> {
        self.shared
            .resume_token(ADAPTER_ID)
            .map(|token| ResumeToken {
                transcript_path: self.transcript.clone(),
                ..token
            })
    }

    async fn dispose(self: Box<Self>) -> Result<(), HarnessError> {
        // Graceful: EOF lets the CLI finish and exit; driver ends the session.
        self.shared.close_stdin_quietly().await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_harness::InputKind;
    use futures::StreamExt;

    fn harness() -> (Shared, mpsc::Receiver<OutMsg>, WireState) {
        let (out_tx, out_rx) = mpsc::channel(8);
        let shared = Shared::new(out_tx, ADAPTER_ID, None, std::env::temp_dir());
        (shared, out_rx, Arc::default())
    }

    fn feed(shared: &Shared, wire: &WireState, line: &str) {
        let v: serde_json::Value = serde_json::from_str(line).expect("fixture json");
        map_line(shared, Line::Json(v), wire);
    }

    async fn next(stream: &mut BoxEventStream) -> SessionEvent {
        tokio::time::timeout(std::time::Duration::from_secs(1), stream.next())
            .await
            .expect("an event in time")
            .expect("stream open")
    }

    #[tokio::test]
    async fn the_session_names_its_model_at_init_and_again_when_the_cli_falls_back() {
        let (shared, _out_rx, wire) = harness();
        let mut events = shared.broadcaster.subscribe();
        feed(
            &shared,
            &wire,
            r#"{"type":"system","subtype":"init","session_id":"s1","model":"claude-opus-5","cwd":"/w"}"#,
        );
        assert!(
            matches!(next(&mut events).await, SessionEvent::Progress(ProgressEvent::ModelChanged { model }) if model == "claude-opus-5")
        );
        assert!(
            matches!(next(&mut events).await, SessionEvent::Raw(_)),
            "the init line still passes through raw"
        );
        feed(
            &shared,
            &wire,
            r#"{"type":"system","subtype":"model_fallback","trigger":"overloaded","original_model":"claude-opus-5","fallback_model":"claude-sonnet-5"}"#,
        );
        assert!(
            matches!(next(&mut events).await, SessionEvent::Progress(ProgressEvent::ModelChanged { model }) if model == "claude-sonnet-5"),
            "the row follows the fallback"
        );
        feed(
            &shared,
            &wire,
            r#"{"type":"system","subtype":"init","session_id":"s1"}"#,
        );
        assert!(
            matches!(next(&mut events).await, SessionEvent::Raw(_)),
            "no model named, no model said"
        );
    }

    #[tokio::test]
    async fn a_can_use_tool_request_is_an_input_request_and_allow_answers_it_on_stdin() {
        let (shared, mut out_rx, wire) = harness();
        let mut events = shared.broadcaster.subscribe();
        feed(
            &shared,
            &wire,
            r#"{"type":"control_request","request_id":"req_1","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"cargo test"}}}"#,
        );
        let SessionEvent::Lifecycle(LifecycleEvent::InputRequested { request }) =
            next(&mut events).await
        else {
            panic!("expected an input request");
        };
        assert_eq!(request.id, "req_1");
        assert!(
            matches!(&request.kind, InputKind::Permission { tool_name, tier: ToolTier::Exec, input, .. } if tool_name == "Bash" && input["command"] == "cargo test")
        );
        assert_eq!(shared.phase(), Phase::AwaitingInput);

        let session = ClaudeCodeSession {
            shared: shared.clone(),
            wire: Arc::clone(&wire),
            transcript: None,
        };
        session.answer("req_1", InputAnswer::ALLOW).await.unwrap();
        let OutMsg::Json(sent) = out_rx.recv().await.unwrap() else {
            panic!("expected a json line on stdin");
        };
        assert_eq!(sent["type"], "control_response");
        assert_eq!(sent["response"]["request_id"], "req_1");
        assert_eq!(sent["response"]["response"]["behavior"], "allow");
        assert_eq!(
            sent["response"]["response"]["updatedInput"]["command"],
            "cargo test"
        );
        assert!(
            matches!(next(&mut events).await, SessionEvent::Lifecycle(LifecycleEvent::InputResolved { id }) if id == "req_1")
        );
        assert_eq!(shared.phase(), Phase::Turn);
        assert!(
            session.answer("req_1", InputAnswer::ALLOW).await.is_err(),
            "answered once"
        );
    }

    #[tokio::test]
    async fn an_allow_with_an_input_runs_that_input_instead() {
        let (shared, mut out_rx, wire) = harness();
        feed(
            &shared,
            &wire,
            r#"{"type":"control_request","request_id":"req_4","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"echo «secret:k:abcdef»"}}}"#,
        );
        let session = ClaudeCodeSession {
            shared,
            wire,
            transcript: None,
        };
        session
            .answer(
                "req_4",
                InputAnswer::Allow {
                    input: Some(serde_json::json!({ "command": "echo restored-fake" })),
                },
            )
            .await
            .unwrap();
        let OutMsg::Json(sent) = out_rx.recv().await.unwrap() else {
            panic!()
        };
        assert_eq!(sent["response"]["response"]["behavior"], "allow");
        assert_eq!(
            sent["response"]["response"]["updatedInput"]["command"],
            "echo restored-fake"
        );
    }

    #[tokio::test]
    async fn a_denied_request_carries_the_reason() {
        let (shared, mut out_rx, wire) = harness();
        feed(
            &shared,
            &wire,
            r#"{"type":"control_request","request_id":"req_2","request":{"subtype":"can_use_tool","tool_name":"Write","input":{"file_path":"a"}}}"#,
        );
        let session = ClaudeCodeSession {
            shared,
            wire,
            transcript: None,
        };
        session
            .answer(
                "req_2",
                InputAnswer::Deny {
                    reason: "not in this step".into(),
                },
            )
            .await
            .unwrap();
        let OutMsg::Json(sent) = out_rx.recv().await.unwrap() else {
            panic!()
        };
        assert_eq!(sent["response"]["response"]["behavior"], "deny");
        assert_eq!(sent["response"]["response"]["message"], "not in this step");
    }

    #[tokio::test]
    async fn ask_user_question_is_a_question_and_a_text_answer_fills_the_answers() {
        let (shared, mut out_rx, wire) = harness();
        let mut events = shared.broadcaster.subscribe();
        feed(
            &shared,
            &wire,
            r#"{"type":"control_request","request_id":"req_3","request":{"subtype":"can_use_tool","tool_name":"AskUserQuestion","input":{"questions":[{"question":"Which tone?","options":[{"label":"Formal"},{"label":"Casual"}]}]}}}"#,
        );
        let SessionEvent::Lifecycle(LifecycleEvent::InputRequested { request }) =
            next(&mut events).await
        else {
            panic!()
        };
        assert!(
            matches!(&request.kind, InputKind::Question { text, options } if text == "Which tone?" && options == &["Formal".to_string(), "Casual".to_string()])
        );
        let session = ClaudeCodeSession {
            shared,
            wire,
            transcript: None,
        };
        session
            .answer(
                "req_3",
                InputAnswer::Text {
                    text: "Casual".into(),
                },
            )
            .await
            .unwrap();
        let OutMsg::Json(sent) = out_rx.recv().await.unwrap() else {
            panic!()
        };
        assert_eq!(sent["response"]["response"]["behavior"], "allow");
        assert_eq!(
            sent["response"]["response"]["updatedInput"]["answers"]["Which tone?"],
            "Casual"
        );
    }

    /// The words and the thinking stream as the API writes them; the whole
    /// message that follows is read for its tools alone, so nothing is said
    /// twice.
    #[tokio::test]
    async fn partial_messages_stream_the_words_and_the_thinking_and_the_whole_message_repeats_neither(
    ) {
        let (shared, _out_rx, wire) = harness();
        let mut events = shared.broadcaster.subscribe();
        feed(
            &shared,
            &wire,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"the test names a port"}}}"#,
        );
        assert!(matches!(
            next(&mut events).await,
            SessionEvent::Progress(ProgressEvent::ThinkingDelta { text }) if text == "the test names a port"
        ));
        feed(
            &shared,
            &wire,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"The port "}}}"#,
        );
        feed(
            &shared,
            &wire,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"moved."}}}"#,
        );
        assert!(matches!(
            next(&mut events).await,
            SessionEvent::Progress(ProgressEvent::TextDelta { text }) if text == "The port "
        ));
        assert!(matches!(
            next(&mut events).await,
            SessionEvent::Progress(ProgressEvent::TextDelta { text }) if text == "moved."
        ));
        // A start or a stop of a block is not a word.
        feed(
            &shared,
            &wire,
            r#"{"type":"stream_event","event":{"type":"content_block_stop","index":1}}"#,
        );
        // The whole message: its tool is a tool, its text and thinking are
        // not said again.
        feed(
            &shared,
            &wire,
            r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"the test names a port"},{"type":"text","text":"The port moved."},{"type":"tool_use","id":"toolu_read","name":"Read","input":{"file_path":"Cargo.toml"}}]}}"#,
        );
        assert!(matches!(
            next(&mut events).await,
            SessionEvent::Progress(ProgressEvent::ToolStarted { name, .. }) if name == "Read"
        ));
    }

    #[tokio::test]
    async fn a_task_is_a_sub_agent_and_its_frames_arrive_nested() {
        let (shared, _out_rx, wire) = harness();
        let mut events = shared.broadcaster.subscribe();
        feed(
            &shared,
            &wire,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_task","name":"Task","input":{"subagent_type":"explore","description":"map the crate"}}]}}"#,
        );
        assert!(matches!(
            next(&mut events).await,
            SessionEvent::Progress(ProgressEvent::SubagentStarted { id, name, .. }) if id.0 == "toolu_task" && name == "explore"
        ));
        feed(
            &shared,
            &wire,
            r#"{"type":"assistant","parent_tool_use_id":"toolu_task","message":{"content":[{"type":"tool_use","id":"toolu_read","name":"Read","input":{"file_path":"src/lib.rs"}},{"type":"text","text":"found it"}]}}"#,
        );
        let started = next(&mut events).await;
        assert_eq!(started.parent().map(|p| p.0.as_str()), Some("toolu_task"));
        assert!(
            matches!(started, SessionEvent::Progress(ProgressEvent::Nested { event, .. }) if matches!(*event, ProgressEvent::ToolStarted { ref name, .. } if name == "Read"))
        );
        feed(
            &shared,
            &wire,
            r#"{"type":"stream_event","parent_tool_use_id":"toolu_task","event":{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"found it"}}}"#,
        );
        let text = next(&mut events).await;
        assert!(
            matches!(text, SessionEvent::Progress(ProgressEvent::Nested { event, .. }) if matches!(*event, ProgressEvent::TextDelta { .. })),
            "sub-agent text is nested, never the agent's own"
        );
        feed(
            &shared,
            &wire,
            r#"{"type":"user","parent_tool_use_id":"toolu_task","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_read","is_error":false}]}}"#,
        );
        assert!(matches!(
            next(&mut events).await,
            SessionEvent::Progress(ProgressEvent::Nested { .. })
        ));
        feed(
            &shared,
            &wire,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_task","is_error":false}]}}"#,
        );
        assert!(matches!(
            next(&mut events).await,
            SessionEvent::Progress(ProgressEvent::SubagentEnded { id, ok: true }) if id.0 == "toolu_task"
        ));
    }

    #[test]
    fn the_allowlist_pre_allows_only_the_to_do_list_and_injected_servers_whatever_the_ceiling() {
        let mut spec = SessionSpec {
            work_item: None,
            cwd: std::env::temp_dir(),
            prompt: String::new(),
            model: None,
            effort: None,
            mcp_servers: vec![
                McpMount::platform(McpServerConfig::Stdio {
                    name: "bisa".into(),
                    command: "bisa".into(),
                    args: vec![],
                    env: Default::default(),
                    cwd: None,
                }),
                McpMount::installed(McpServerConfig::Stdio {
                    name: "gh".into(),
                    command: "gh-mcp".into(),
                    args: vec![],
                    env: Default::default(),
                    cwd: None,
                }),
            ],
            env: Default::default(),
            env_remove: Vec::new(),
            tier_ceiling: ToolTier::Exec,
            output_schema: None,
            skills: vec![],
        };
        assert_eq!(
            allowed_tools(&spec),
            "TodoWrite,mcp__bisa",
            "an installed server's tools are judged, never pre-allowed"
        );
        spec.tier_ceiling = ToolTier::Read;
        assert_eq!(
            allowed_tools(&spec),
            "TodoWrite,mcp__bisa",
            "the ceiling is the engine's to apply"
        );
        let allowed = allowed_tools(&spec);
        for tool in ["Read", "Edit", "Write", "Bash", "mcp__gh"] {
            assert!(!allowed.split(',').any(|t| t == tool), "{tool} must ask");
        }
        let config: serde_json::Value =
            serde_json::from_str(&mcp_config_json(&spec.mcp_servers)).unwrap();
        assert_eq!(
            config["mcpServers"]["gh"]["command"], "gh-mcp",
            "the installed server is still mounted — its tools ask"
        );
        let remote = vec![
            McpMount::installed(McpServerConfig::Http {
                name: "docs".into(),
                url: "https://mcp.example.com/mcp".into(),
                headers: [("Authorization".to_string(), "Bearer t".to_string())].into(),
            }),
            McpMount::installed(McpServerConfig::Sse {
                name: "old".into(),
                url: "https://mcp.example.com/sse".into(),
                headers: Default::default(),
            }),
        ];
        let config: serde_json::Value = serde_json::from_str(&mcp_config_json(&remote)).unwrap();
        assert_eq!(config["mcpServers"]["docs"]["type"], "http");
        assert_eq!(
            config["mcpServers"]["docs"]["headers"]["Authorization"],
            "Bearer t"
        );
        assert_eq!(
            config["mcpServers"]["old"]["type"], "sse",
            "the deprecated transport is still Claude Code's own kind"
        );
    }

    fn bare() -> SessionSpec {
        SessionSpec {
            work_item: None,
            cwd: std::path::PathBuf::from("/work/here"),
            prompt: String::new(),
            model: None,
            effort: None,
            mcp_servers: vec![],
            env: Default::default(),
            env_remove: Vec::new(),
            tier_ceiling: ToolTier::Write,
            output_schema: None,
            skills: vec![],
        }
    }

    fn asking(model: Option<&str>, effort: Option<Effort>) -> SessionSpec {
        SessionSpec {
            model: model.map(str::to_string),
            effort,
            ..bare()
        }
    }

    /// Where `flag` stands in the line, and the value after it.
    fn flag<'a>(args: &'a [String], flag: &str) -> Option<(usize, &'a str)> {
        let at = args.iter().position(|a| a == flag)?;
        Some((at, args.get(at + 1)?.as_str()))
    }

    #[test]
    fn the_levels_a_model_takes_are_read_from_its_id() {
        let none: &[Effort] = &[];
        let table: &[(Option<&str>, &[Effort])] = &[
            // The two defaults, with and without the window.
            (Some("claude-opus-5-5[1m]"), FIVE_LEVELS),
            (Some("claude-sonnet-5-5[1m]"), FIVE_LEVELS),
            (Some("claude-opus-5-5"), FIVE_LEVELS),
            (Some("claude-sonnet-5-5"), FIVE_LEVELS),
            // Fable, and the earlier models that take five.
            (Some("claude-fable-5-1"), FIVE_LEVELS),
            (Some("claude-fable-5"), FIVE_LEVELS),
            (Some("claude-opus-5"), FIVE_LEVELS),
            (Some("claude-opus-4-8"), FIVE_LEVELS),
            (Some("claude-opus-4-7"), FIVE_LEVELS),
            (Some("claude-sonnet-5"), FIVE_LEVELS),
            // The aliases.
            (Some("opus"), FIVE_LEVELS),
            (Some("sonnet"), FIVE_LEVELS),
            (Some("fable"), FIVE_LEVELS),
            (Some("best"), FIVE_LEVELS),
            (Some("opusplan"), FIVE_LEVELS),
            (Some("default"), FIVE_LEVELS),
            (Some("opus[1m]"), FIVE_LEVELS),
            (Some("sonnet[1m]"), FIVE_LEVELS),
            // The two that take four.
            (Some("claude-opus-4-6"), FOUR_LEVELS),
            (Some("claude-sonnet-4-6"), FOUR_LEVELS),
            (Some("claude-sonnet-4-6[1m]"), FOUR_LEVELS),
            // Haiku has no control, by id and by alias.
            (Some("haiku"), none),
            (Some("claude-haiku-4-5"), none),
            (Some("claude-haiku-4-5-20251001"), none),
            (Some("haiku[1m]"), none),
            // An id nobody here knows, and no model: the four.
            (Some("claude-opus-9"), FOUR_LEVELS),
            (Some("claude-opus-5-5-20260801"), FOUR_LEVELS),
            (Some("us.anthropic.claude-opus-5-5-v1:0"), FOUR_LEVELS),
            (Some("   "), FOUR_LEVELS),
            (None, FOUR_LEVELS),
        ];
        for (model, levels) in table {
            assert_eq!(efforts_for(*model), *levels, "{model:?}");
            assert_eq!(
                ClaudeCodeAdapter::default().efforts(*model),
                levels.to_vec(),
                "{model:?}"
            );
        }
        assert_eq!(
            FOUR_LEVELS,
            [Effort::Low, Effort::Medium, Effort::High, Effort::Max]
        );
        assert_eq!(
            FIVE_LEVELS,
            [
                Effort::Low,
                Effort::Medium,
                Effort::High,
                Effort::Xhigh,
                Effort::Max
            ]
        );
        assert!(ClaudeCodeAdapter::default()
            .caps()
            .contains(HarnessCaps::EFFORT));
    }

    #[test]
    fn the_effort_follows_the_model_in_the_line_and_is_absent_when_none_is_set() {
        // Neither: the line is what it was.
        let plain = argv(&bare(), None);
        assert!(!plain.iter().any(|a| a == "--effort" || a == "--model"));
        assert_eq!(plain.last().map(String::as_str), Some("TodoWrite"));
        assert_eq!(plain[plain.len() - 2], "--allowedTools");

        // A model alone.
        let model_only = argv(&asking(Some("claude-opus-5-5[1m]"), None), None);
        assert_eq!(
            flag(&model_only, "--model").map(|(_, v)| v),
            Some("claude-opus-5-5[1m]")
        );
        assert!(!model_only.iter().any(|a| a == "--effort"));

        // Both: `--effort` right after the model's id.
        let both = argv(
            &asking(Some("claude-opus-5-5[1m]"), Some(Effort::Xhigh)),
            None,
        );
        let (model_at, _) = flag(&both, "--model").unwrap();
        let (effort_at, level) = flag(&both, "--effort").unwrap();
        assert_eq!(effort_at, model_at + 2);
        assert_eq!(level, "xhigh");
        assert!(effort_at < both.iter().position(|a| a == "--allowedTools").unwrap());

        // An effort alone: the CLI's own default model, at that level.
        let effort_only = argv(&asking(None, Some(Effort::Max)), None);
        assert_eq!(flag(&effort_only, "--effort").map(|(_, v)| v), Some("max"));
        assert!(!effort_only.iter().any(|a| a == "--model"));
    }

    #[test]
    fn a_level_the_model_does_not_take_is_never_sent() {
        // Haiku has no control: nothing, whatever was asked.
        for level in Effort::ALL {
            let line = argv(&asking(Some("claude-haiku-4-5"), Some(level)), None);
            assert!(!line.iter().any(|a| a == "--effort"), "{level}");
            assert_eq!(effort_sent(Some("haiku"), Some(level)), None);
        }
        // Held to what the model takes: the nearest below, else the lowest.
        assert_eq!(
            effort_sent(Some("claude-sonnet-4-6"), Some(Effort::Xhigh)),
            Some(Effort::High)
        );
        assert_eq!(
            effort_sent(Some("claude-opus-5-5"), Some(Effort::Minimal)),
            Some(Effort::Low)
        );
        assert_eq!(effort_sent(None, Some(Effort::Xhigh)), Some(Effort::High));
        let line = argv(&asking(Some("claude-unknown-1"), Some(Effort::Xhigh)), None);
        assert_eq!(flag(&line, "--effort").map(|(_, v)| v), Some("high"));
        // Nothing asked, nothing sent.
        assert_eq!(effort_sent(Some("opus"), None), None);
    }

    #[test]
    fn a_revived_session_is_told_its_model_and_its_effort_again() {
        let token = ResumeToken {
            adapter_id: ADAPTER_ID.to_string(),
            native_id: "sess-9".into(),
            cwd: std::path::PathBuf::from("/work/here"),
            transcript_path: None,
            model: Some("claude-sonnet-5-5[1m]".into()),
            effort: Some(Effort::High),
        };
        let spec = revival_spec(&token);
        assert_eq!(spec.cwd, token.cwd);
        assert_eq!(spec.model, token.model);
        assert_eq!(spec.effort, token.effort);
        assert!(spec.prompt.is_empty());
        let line = argv(&spec, Some(token.native_id.as_str()));
        let (model_at, model) = flag(&line, "--model").unwrap();
        let (effort_at, level) = flag(&line, "--effort").unwrap();
        let (resume_at, id) = flag(&line, "--resume").unwrap();
        assert_eq!(model, "claude-sonnet-5-5[1m]");
        assert_eq!(level, "high");
        assert_eq!(id, "sess-9");
        assert_eq!(effort_at, model_at + 2);
        assert!(effort_at < resume_at);

        // A session that ran with neither is revived with neither.
        let plain = ResumeToken {
            model: None,
            effort: None,
            ..token
        };
        let line = argv(&revival_spec(&plain), Some("sess-9"));
        assert!(!line.iter().any(|a| a == "--effort" || a == "--model"));
        assert_eq!(flag(&line, "--resume").map(|(_, v)| v), Some("sess-9"));
    }

    #[test]
    fn a_sessions_token_says_the_model_and_the_effort_it_was_launched_with() {
        let (out_tx, _out_rx) = mpsc::channel(8);
        let shared = Shared::new(
            out_tx,
            ADAPTER_ID,
            Some("claude-opus-5-5[1m]".into()),
            "/work/here",
        )
        .at_effort(Some(Effort::Max));
        let session = ClaudeCodeSession {
            shared: shared.clone(),
            wire: Arc::default(),
            transcript: None,
        };
        assert_eq!(session.resume_token(), None, "the CLI has not named it yet");
        shared.set_native_id("sess-3");
        let token = session.resume_token().unwrap();
        assert_eq!(token.adapter_id, ADAPTER_ID);
        assert_eq!(token.native_id, "sess-3");
        assert_eq!(token.model.as_deref(), Some("claude-opus-5-5[1m]"));
        assert_eq!(token.effort, Some(Effort::Max));
    }

    #[tokio::test]
    async fn the_list_leads_with_the_two_defaults_and_every_model_says_its_efforts() {
        let adapter = ClaudeCodeAdapter::default();
        let listed = adapter.models().await;
        let ids: Vec<&str> = listed.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "claude-opus-5-5[1m]",
                "claude-sonnet-5-5[1m]",
                "claude-opus-5-5",
                "claude-sonnet-5-5",
                "claude-fable-5-1",
                "claude-haiku-4-5",
                "opus",
                "sonnet",
                "haiku",
                "fable",
                "best",
                "opus[1m]",
                "sonnet[1m]",
                "opusplan",
                "claude-opus-5",
                "claude-fable-5",
                "claude-opus-4-8",
                "claude-opus-4-7",
                "claude-sonnet-5",
                "claude-sonnet-4-6",
            ]
        );
        let unique: std::collections::BTreeSet<&str> = ids.iter().copied().collect();
        assert_eq!(unique.len(), ids.len(), "no id twice");
        for model in &listed {
            assert!(model.label.is_some(), "{}", model.id);
            assert_eq!(
                model.efforts,
                efforts_for(Some(&model.id)).to_vec(),
                "{}",
                model.id
            );
        }
        let efforts = |id: &str| listed.iter().find(|m| m.id == id).unwrap().efforts.clone();
        assert_eq!(efforts("claude-opus-5-5[1m]"), FIVE_LEVELS);
        assert_eq!(efforts("claude-sonnet-4-6"), FOUR_LEVELS);
        assert!(efforts("claude-haiku-4-5").is_empty());
        assert!(efforts("haiku").is_empty());

        // What the setup gate's fixes write: both are listed models.
        assert_eq!(
            adapter.recommended_plan(),
            Some(ModelPlan::fallback([
                "claude-opus-5-5[1m]",
                "claude-sonnet-5-5[1m]"
            ]))
        );
        assert_eq!(
            adapter.recommended_judge().as_deref(),
            Some("claude-sonnet-5-5[1m]")
        );
        assert_eq!(&ids[..2], RECOMMENDED_MODELS);
    }

    #[test]
    fn a_terminal_session_is_given_no_model_and_no_effort() {
        let launch = ClaudeCodeAdapter::default().interactive().unwrap();
        assert!(launch.args.is_empty());
        assert_eq!(launch.resume_args, ["--continue"]);
    }
}
