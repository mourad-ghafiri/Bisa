//! OpenCode adapter — `opencode run --format json`, one process per turn,
//! one JSON object per line on stdout, resume via `--session <id>`.
//!
//! CLI surface verified against OpenCode 1.18 (`opencode run --help`). The
//! run-mode vocabulary, one line each (OpenCode's own SDK types, and the
//! stream cheat-sheets that document them):
//!
//! | `type` | carries | here |
//! |---|---|---|
//! | `text` | `part.text` | `TextDelta` |
//! | `reasoning` | `part.text` | `ThinkingDelta` |
//! | `tool_use` | `part.tool`, `part.callID`, `part.state { status: completed \| error, input, output, title }` | `ToolStarted` **and** `ToolEnded` — run mode prints finished states only, so one line is the whole call |
//! | `step_start` | a step's snapshot | nothing a person reads |
//! | `step_finish` | `part.cost` (USD), `part.tokens { input, output, reasoning, cache { read, write } }`, `part.reason` | `CostDelta` |
//! | `error` | `error.name`, `error.data.message` | the classifier's word, else the turn failed with that message |
//!
//! Every line carries `sessionID` at the top, the resume handle. Sub-agents
//! arrive as the `task` tool and stay a tool here: nothing of their own
//! progress reaches stdout, and a capability is earned by what is emitted.
//! **Permissions**: run mode prints none; a tool OpenCode's config sets to
//! *ask* stalls an unattended run, and nothing here widens what a run may
//! do (`--auto` is not passed) — the person sets the policy in OpenCode.
//!
//! **Effort**: `--variant <name>` after `--model`, on every turn — "Model
//! variant (provider-specific reasoning effort)"
//! (https://opencode.ai/docs/cli/, read 2026-09-29). Which variants exist is
//! the provider's, read off the model id's prefix
//! (https://opencode.ai/docs/models/, read 2026-09-29); a model whose
//! provider this table does not know is sent none.

use std::sync::Arc;

use async_trait::async_trait;
use bisa_core::{HarnessCaps, ToolTier};
use bisa_harness::proc::ProcSpec;
use bisa_harness::{
    BoxEventStream, Effort, HarnessAdapter, HarnessError, HarnessSession, InteractiveLaunch,
    ModelInfo, Outcome, Phase, ProbeResult, ProgressEvent, PromptInput, ResumeToken, SessionEvent,
    SessionSnapshot, SessionSpec, Steer,
};

use crate::oneshot::{MapFn, OneShotSession, SpawnFn};
use crate::util::{self, Drive, Shared};

pub const ADAPTER_ID: &str = "opencode";

pub struct OpencodeAdapter {
    pub program: String,
    /// The engine's clients, for the account's usage endpoint.
    pub http: Arc<bisa_http::Clients>,
}

impl Default for OpencodeAdapter {
    fn default() -> Self {
        Self {
            program: "opencode".into(),
            http: bisa_http::Clients::shared(),
        }
    }
}

/// The variants each provider's models take, by the prefix of the model id
/// (`provider/model`), each under the level it is.
const PROVIDER_EFFORTS: &[(&str, &[Effort])] = &[
    ("anthropic/", &[Effort::High, Effort::Max]),
    (
        "openai/",
        &[
            Effort::Minimal,
            Effort::Low,
            Effort::Medium,
            Effort::High,
            Effort::Xhigh,
        ],
    ),
    ("google/", &[Effort::Low, Effort::High]),
];

/// The levels `model` takes, lowest first: its provider's, or none for a
/// provider this table does not know and for no model at all — a variant
/// OpenCode does not have would fail the run.
pub fn efforts_for(model: Option<&str>) -> &'static [Effort] {
    let Some(model) = model.map(str::trim) else {
        return &[];
    };
    PROVIDER_EFFORTS
        .iter()
        .find(|(prefix, _)| model.starts_with(prefix))
        .map(|(_, efforts)| *efforts)
        .unwrap_or(&[])
}

/// The effort to send for `model`: the one asked, held to what its provider
/// takes.
fn effort_sent(model: Option<&str>, effort: Option<Effort>) -> Option<Effort> {
    effort.and_then(|e| e.clamp_to(efforts_for(model)))
}

/// The variant that runs `model` at `effort`, by name — the level's own
/// word — or nothing when the model's provider has none.
fn variant(model: Option<&str>, effort: Effort) -> Option<&'static str> {
    effort_sent(model, Some(effort)).map(Effort::as_str)
}

/// The spec with its effort held to what its model takes — what every
/// turn's command line and the session's resume token are built from.
fn held(spec: SessionSpec) -> SessionSpec {
    SessionSpec {
        effort: effort_sent(spec.model.as_deref(), spec.effort),
        ..spec
    }
}

/// The spec a session is revived with: where it ran, and the model and the
/// effort it ran with.
fn revival_spec(token: &ResumeToken) -> SessionSpec {
    held(SessionSpec {
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
    })
}

fn spawn_fn(program: String, spec: &SessionSpec) -> Arc<SpawnFn> {
    let cwd = spec.cwd.clone();
    let model = spec.model.clone();
    let variant = spec
        .effort
        .and_then(|effort| variant(spec.model.as_deref(), effort));
    let env = spec.env.clone();
    let env_remove = spec.env_remove.clone();
    // The MCP servers the engine handed the session — the `bisa` server
    // among them, so the agent has the browser (ide/18) — as the inline
    // configuration OpenCode reads from its environment for this run.
    let transports: Vec<_> = spec.mcp_servers.iter().map(|m| m.config.clone()).collect();
    let mcp = (!transports.is_empty()).then(|| crate::mcp_inject::opencode_config(&transports));
    Arc::new(move |prompt: &str, resume: Option<&str>| {
        let mut p = ProcSpec::new(&program)
            .arg("run")
            .arg("--format")
            .arg("json")
            .arg("--dir")
            .arg(cwd.display().to_string())
            .cwd(&cwd);
        if let Some(m) = &model {
            p = p.arg("--model").arg(m);
        }
        if let Some(name) = variant {
            p = p.arg("--variant").arg(name);
        }
        if let Some(id) = resume {
            p = p.arg("--session").arg(id);
        }
        if let Some(config) = &mcp {
            p = p.env(crate::mcp_inject::OPENCODE_CONFIG_CONTENT, config);
        }
        for (k, v) in &env {
            p = p.env(k, v);
        }
        for k in &env_remove {
            p = p.env_remove(k);
        }
        p.arg(prompt)
    })
}

/// One run-mode line → what it means. Pure over the line; the effects go
/// through `Shared`.
pub(crate) fn map_line(shared: &Shared, value: serde_json::Value) -> Drive {
    if let Some(id) = value.get("sessionID").and_then(|v| v.as_str()) {
        shared.set_native_id(id);
    }
    let kind = value.get("type").and_then(|t| t.as_str()).unwrap_or("");
    let part = value
        .get("part")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    match kind {
        "text" => {
            if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                shared
                    .broadcaster
                    .emit(SessionEvent::Progress(ProgressEvent::TextDelta {
                        text: text.to_string(),
                    }));
            }
            Drive::Continue
        }
        "reasoning" => {
            if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                shared
                    .broadcaster
                    .emit(SessionEvent::Progress(ProgressEvent::ThinkingDelta {
                        text: text.to_string(),
                    }));
            }
            Drive::Continue
        }
        "tool_use" => {
            // The part's own id: what ties the call's end to its start.
            fn part_id(part: &serde_json::Value) -> Option<String> {
                part.get("id")
                    .and_then(|v| v.as_str())
                    .filter(|id| !id.is_empty())
                    .map(str::to_string)
            }
            let name = part
                .get("tool")
                .and_then(|v| v.as_str())
                .unwrap_or("tool")
                .to_string();
            let input = part
                .pointer("/state/input")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let ok = part.pointer("/state/status").and_then(|s| s.as_str()) != Some("error");
            shared.set_activity(name.clone());
            shared
                .broadcaster
                .emit(SessionEvent::Progress(ProgressEvent::ToolStarted {
                    tier: ToolTier::classify(&name),
                    args_summary: util::summarize_args(&input, 160),
                    name: name.clone(),
                    id: part_id(&part),
                }));
            shared
                .broadcaster
                .emit(SessionEvent::Progress(ProgressEvent::ToolEnded {
                    name,
                    ok,
                    id: part_id(&part),
                }));
            Drive::Continue
        }
        "step_finish" => {
            // The step names its model when the line carries one: `providerID/modelID`.
            if let Some(model) = part
                .get("modelID")
                .and_then(|v| v.as_str())
                .filter(|m| !m.is_empty())
            {
                let model = match part
                    .get("providerID")
                    .and_then(|v| v.as_str())
                    .filter(|p| !p.is_empty())
                {
                    Some(provider) => format!("{provider}/{model}"),
                    None => model.to_string(),
                };
                shared
                    .broadcaster
                    .emit(SessionEvent::Progress(ProgressEvent::ModelChanged {
                        model,
                    }));
            }
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
            shared.add_cost(input, output, cents);
            shared
                .broadcaster
                .emit(SessionEvent::Progress(ProgressEvent::CostDelta {
                    input_tokens: input,
                    output_tokens: output,
                    usd_cents: cents,
                }));
            Drive::Continue
        }
        "error" => {
            let message = value
                .pointer("/error/data/message")
                .or_else(|| value.pointer("/error/message"))
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| {
                    value
                        .get("error")
                        .map(|e| e.to_string())
                        .unwrap_or_else(|| "OpenCode reported an error".to_string())
                });
            if let Some(outcome) = shared.model_ctx.outcome(&message) {
                tracing::warn!(target: "bisa_adapters::opencode", "model unavailable: {message}");
                return Drive::End(outcome);
            }
            Drive::End(Outcome::Failed { error: message })
        }
        _ => {
            shared.broadcaster.emit(SessionEvent::Raw(value));
            Drive::Continue
        }
    }
}

fn map_fn() -> Arc<MapFn> {
    Arc::new(map_line)
}

#[async_trait]
impl HarnessAdapter for OpencodeAdapter {
    fn id(&self) -> &str {
        ADAPTER_ID
    }

    fn display_name(&self) -> &str {
        "OpenCode"
    }

    /// `SUBAGENTS`: the event stream names child sessions by `parentID`
    /// (`hooks/opencode.rs`), announced and ended as sub-agents.
    fn caps(&self) -> HarnessCaps {
        HarnessCaps::RESUME
            | HarnessCaps::FORK
            | HarnessCaps::MCP_SERVERS
            | HarnessCaps::COST_REPORTING
            | HarnessCaps::USAGE_REPORTING
            | HarnessCaps::SUBAGENTS
            | HarnessCaps::EFFORT
    }

    fn efforts(&self, model: Option<&str>) -> Vec<Effort> {
        efforts_for(model).to_vec()
    }

    /// The account's windows through OpenCode's own Claude Pro/Max sign-in —
    /// the same provider endpoint Claude Code reads, with OpenCode's token.
    async fn usage(&self) -> bisa_harness::UsageState {
        crate::usage::opencode::read(&self.http, self.id()).await
    }

    /// `opencode models` prints one provider-qualified id per line; the
    /// efforts beside each are its provider's.
    async fn models(&self) -> Vec<ModelInfo> {
        util::models_from_command(&self.program, &["models"], listed_model).await
    }

    /// The bare TUI. `launch` above runs it in `run --format json`; this is
    /// the command a person types, and `--continue` picks its last session
    /// up in the same directory.
    fn interactive(&self) -> Option<InteractiveLaunch> {
        Some(InteractiveLaunch::new(&self.program).resumable_with(vec!["--continue".into()]))
    }

    fn interactive_reporting(
        &self,
        ctx: &bisa_harness::ReportingContext,
    ) -> bisa_harness::ReportingPlan {
        crate::hooks::opencode::reporting(ctx)
    }

    fn translate_report(&self, payload: &serde_json::Value) -> Vec<SessionEvent> {
        crate::hooks::opencode::translate(payload)
    }

    async fn probe(&self) -> ProbeResult {
        util::probe_binary(&self.program, &["--version"]).await
    }

    async fn launch(&self, spec: SessionSpec) -> Result<Box<dyn HarnessSession>, HarnessError> {
        let spec = held(spec);
        let session = OneShotSession::start(
            ADAPTER_ID,
            spawn_fn(self.program.clone(), &spec),
            map_fn(),
            &spec,
            None,
            false,
        )
        .await?;
        Ok(Box::new(OpencodeSession(session)))
    }

    async fn attach(&self, token: &ResumeToken) -> Result<Box<dyn HarnessSession>, HarnessError> {
        if token.adapter_id != ADAPTER_ID {
            return Err(HarnessError::protocol(format!(
                "resume token for {:?} handed to {ADAPTER_ID}",
                token.adapter_id
            )));
        }
        // The token carries the directory the launch chose. This used to be
        // `current_dir()`, falling back to `/` — the daemon's own placement,
        // which is never the session's. It carries the model and the effort
        // too: every turn is its own process, and is told both again.
        let spec = revival_spec(token);
        let session = OneShotSession::start(
            ADAPTER_ID,
            spawn_fn(self.program.clone(), &spec),
            map_fn(),
            &spec,
            Some(&token.native_id),
            true,
        )
        .await?;
        Ok(Box::new(OpencodeSession(session)))
    }
}

/// One line of `opencode models` as a listed model, with the efforts its
/// provider takes.
fn listed_model(line: &str) -> Option<ModelInfo> {
    let id = line.trim();
    (!id.is_empty() && !id.starts_with(char::is_whitespace))
        .then(|| ModelInfo::new(id, None, efforts_for(Some(id)).to_vec()))
}

pub struct OpencodeSession(OneShotSession);

#[async_trait]
impl HarnessSession for OpencodeSession {
    fn snapshot(&self) -> SessionSnapshot {
        self.0.snapshot()
    }

    fn phase(&self) -> Phase {
        self.0.phase()
    }

    async fn prompt(&self, input: PromptInput) -> Result<(), HarnessError> {
        self.0.prompt(input).await
    }

    async fn steer(&self, _msg: Steer) -> Result<(), HarnessError> {
        Err(HarnessError::NotSupported("steer"))
    }

    async fn follow_up(&self, _msg: Steer) -> Result<(), HarnessError> {
        Err(HarnessError::NotSupported("follow_up"))
    }

    async fn abort(&self) -> Result<(), HarnessError> {
        self.0.abort().await
    }

    fn subscribe(&self) -> BoxEventStream {
        self.0.subscribe()
    }

    fn resume_token(&self) -> Option<ResumeToken> {
        self.0.resume_token()
    }

    async fn dispose(self: Box<Self>) -> Result<(), HarnessError> {
        self.0.dispose().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt as _;
    use serde_json::json;

    /// A `Shared` over a subscribed stream, so each line's events can be read back.
    fn shared() -> (Shared, BoxEventStream) {
        let (tx, _rx) = tokio::sync::mpsc::channel(8);
        let s = Shared::new(tx, ADAPTER_ID, None, std::env::temp_dir());
        let stream = s.broadcaster.subscribe();
        (s, stream)
    }

    async fn drain(stream: &mut BoxEventStream, n: usize) -> Vec<SessionEvent> {
        let mut out = Vec::new();
        for _ in 0..n {
            if let Some(e) = stream.next().await {
                out.push(e);
            }
        }
        out
    }

    #[tokio::test]
    async fn a_text_line_is_a_delta_and_names_the_session() {
        let (s, mut stream) = shared();
        let line = json!({"type": "text", "timestamp": 1, "sessionID": "ses_abc", "part": {"type": "text", "text": "Hello."}});
        assert!(matches!(map_line(&s, line), Drive::Continue));
        assert_eq!(
            s.native_id().as_deref(),
            Some("ses_abc"),
            "the resume handle is the top-level session id"
        );
        let events = drain(&mut stream, 1).await;
        assert!(
            matches!(&events[0], SessionEvent::Progress(ProgressEvent::TextDelta { text }) if text == "Hello.")
        );
    }

    #[tokio::test]
    async fn a_reasoning_line_is_the_turns_thinking() {
        let (s, mut stream) = shared();
        let line = json!({"type": "reasoning", "sessionID": "ses_abc", "part": {"type": "reasoning", "text": "check the port first"}});
        assert!(matches!(map_line(&s, line), Drive::Continue));
        let events = drain(&mut stream, 1).await;
        assert!(
            matches!(&events[0], SessionEvent::Progress(ProgressEvent::ThinkingDelta { text }) if text == "check the port first")
        );
    }

    #[tokio::test]
    async fn a_tool_line_is_a_whole_call_started_with_its_arguments_and_ended() {
        let (s, mut stream) = shared();
        let line = json!({"type": "tool_use", "sessionID": "ses_abc", "part": {"type": "tool", "callID": "c1", "tool": "edit", "state": {"status": "completed", "input": {"filePath": "src/cart.rs", "oldString": "a", "newString": "b"}, "output": "ok", "title": "Edit src/cart.rs"}}});
        map_line(&s, line);
        let events = drain(&mut stream, 2).await;
        match &events[0] {
            SessionEvent::Progress(ProgressEvent::ToolStarted {
                name,
                tier,
                args_summary,
                ..
            }) => {
                assert_eq!(name, "edit");
                assert_eq!(*tier, ToolTier::Write);
                assert!(args_summary.contains("src/cart.rs"), "{args_summary}");
            }
            other => panic!("{other:?}"),
        }
        assert!(
            matches!(&events[1], SessionEvent::Progress(ProgressEvent::ToolEnded { name, ok: true, .. }) if name == "edit")
        );
        assert_eq!(s.snapshot().activity.as_deref(), Some("edit"));

        let failed = json!({"type": "tool_use", "part": {"type": "tool", "tool": "bash", "state": {"status": "error", "input": {"command": "cargo test"}, "error": "exit 1"}}});
        map_line(&s, failed);
        let events = drain(&mut stream, 2).await;
        assert!(
            matches!(&events[1], SessionEvent::Progress(ProgressEvent::ToolEnded { name, ok: false, .. }) if name == "bash")
        );
    }

    #[tokio::test]
    async fn a_step_finish_is_the_cost_in_cents_and_tokens_and_names_the_model_when_it_carries_one()
    {
        let (s, mut stream) = shared();
        let line = json!({"type": "step_finish", "sessionID": "ses_abc", "part": {"type": "step-finish", "reason": "stop", "cost": 0.0123, "tokens": {"input": 671, "output": 8, "reasoning": 0, "cache": {"read": 21415, "write": 0}}}});
        map_line(&s, line);
        let events = drain(&mut stream, 1).await;
        assert!(matches!(
            &events[0],
            SessionEvent::Progress(ProgressEvent::CostDelta {
                input_tokens: 671,
                output_tokens: 8,
                usd_cents: 1
            })
        ));
        let cost = s.snapshot().cost;
        assert_eq!(
            (cost.input_tokens, cost.output_tokens, cost.usd_cents),
            (671, 8, 1)
        );

        let named = json!({"type": "step_finish", "part": {"type": "step-finish", "reason": "stop", "cost": 0, "tokens": {"input": 1, "output": 1}, "providerID": "anthropic", "modelID": "claude-opus-5"}});
        map_line(&s, named);
        let events = drain(&mut stream, 2).await;
        assert!(
            matches!(&events[0], SessionEvent::Progress(ProgressEvent::ModelChanged { model }) if model == "anthropic/claude-opus-5"),
            "the model, provider-qualified, before the cost"
        );
        assert!(matches!(
            &events[1],
            SessionEvent::Progress(ProgressEvent::CostDelta { .. })
        ));
    }

    #[tokio::test]
    async fn an_error_line_ends_the_turn_as_the_classifier_says_else_failed() {
        let (s, _stream) = shared();
        let wall = json!({"type": "error", "sessionID": "ses_abc", "error": {"name": "APIError", "data": {"message": "GatewayRateLimitError: too many requests"}}});
        assert!(
            matches!(
                map_line(&s, wall),
                Drive::End(Outcome::ModelUnavailable { .. })
            ),
            "a rate limit is the model's wall, not the work's failure"
        );
        let plain = json!({"type": "error", "error": {"name": "UnknownError", "data": {"message": "the tool crashed"}}});
        assert!(
            matches!(map_line(&s, plain), Drive::End(Outcome::Failed { error }) if error == "the tool crashed")
        );
        let bare = json!({"type": "error"});
        assert!(matches!(
            map_line(&s, bare),
            Drive::End(Outcome::Failed { .. })
        ));
    }

    #[tokio::test]
    async fn anything_else_passes_through_raw() {
        let (s, mut stream) = shared();
        map_line(
            &s,
            json!({"type": "step_start", "sessionID": "ses_abc", "part": {"type": "step-start", "snapshot": "deadbeef"}}),
        );
        let events = drain(&mut stream, 1).await;
        assert!(matches!(&events[0], SessionEvent::Raw(_)));
    }

    #[test]
    fn the_terminal_form_resumes_with_continue_and_the_caps_say_what_is_emitted() {
        let a = OpencodeAdapter::default();
        let launch = a.interactive().unwrap();
        assert_eq!(launch.program, "opencode");
        assert_eq!(launch.resume_args, vec!["--continue".to_string()]);
        assert!(a.caps().contains(
            HarnessCaps::COST_REPORTING | HarnessCaps::USAGE_REPORTING | HarnessCaps::RESUME
        ));
        assert!(
            a.caps().contains(HarnessCaps::SUBAGENTS),
            "a child session by `parentID` is a sub-agent the stream announces"
        );
        assert!(
            a.caps().contains(HarnessCaps::MCP_SERVERS),
            "the engine's MCP servers reach OpenCode through its inline configuration"
        );
    }

    /// The MCP servers the engine hands the session ride the run as the
    /// JSON OpenCode reads from `OPENCODE_CONFIG_CONTENT` — the `bisa`
    /// server among them, so an agent on OpenCode has the browser (ide/18)
    /// — and a spec with none sets nothing.
    #[test]
    fn a_launch_carries_the_engines_mcp_servers_in_the_inline_configuration() {
        let spec = SessionSpec {
            work_item: None,
            cwd: std::env::temp_dir(),
            prompt: String::new(),
            model: None,
            effort: None,
            mcp_servers: vec![bisa_harness::McpMount::platform(
                bisa_harness::McpServerConfig::Stdio {
                    name: "bisa".into(),
                    command: "/opt/bisa".into(),
                    args: vec!["mcp".into(), "--socket".into(), "/tmp/i.sock".into()],
                    env: Default::default(),
                    cwd: None,
                },
            )],
            env: Default::default(),
            env_remove: Vec::new(),
            tier_ceiling: ToolTier::Write,
            output_schema: None,
            skills: vec![],
        };
        let proc = spawn_fn("opencode".into(), &spec)("hello", None);
        let config = proc
            .env
            .get(crate::mcp_inject::OPENCODE_CONFIG_CONTENT)
            .expect("the inline configuration");
        let json: serde_json::Value = serde_json::from_str(config).unwrap();
        assert_eq!(
            json["mcp"]["bisa"]["command"],
            json!(["/opt/bisa", "mcp", "--socket", "/tmp/i.sock"])
        );
        assert_eq!(json["mcp"]["bisa"]["type"], json!("local"));
        assert_eq!(proc.args.last().map(String::as_str), Some("hello"));
        let bare = SessionSpec {
            mcp_servers: vec![],
            ..spec
        };
        let proc = spawn_fn("opencode".into(), &bare)("hello", None);
        assert!(
            !proc
                .env
                .contains_key(crate::mcp_inject::OPENCODE_CONFIG_CONTENT),
            "no server, no configuration"
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

    #[test]
    fn the_levels_a_model_takes_are_its_providers() {
        let none: &[Effort] = &[];
        let table: &[(Option<&str>, &[Effort])] = &[
            (
                Some("anthropic/claude-opus-5-5"),
                &[Effort::High, Effort::Max],
            ),
            (
                Some("anthropic/claude-haiku-4-5"),
                &[Effort::High, Effort::Max],
            ),
            (
                Some("openai/gpt-5"),
                &[
                    Effort::Minimal,
                    Effort::Low,
                    Effort::Medium,
                    Effort::High,
                    Effort::Xhigh,
                ],
            ),
            (Some("google/gemini-2.5-pro"), &[Effort::Low, Effort::High]),
            // A provider nobody here knows, an id with no prefix, no model.
            (Some("openrouter/anthropic/claude-opus-5-5"), none),
            (Some("ollama/llama3"), none),
            (Some("claude-opus-5-5"), none),
            (Some("anthropic"), none),
            (Some(""), none),
            (None, none),
        ];
        let adapter = OpencodeAdapter::default();
        for (model, levels) in table {
            assert_eq!(efforts_for(*model), *levels, "{model:?}");
            assert_eq!(adapter.efforts(*model), levels.to_vec(), "{model:?}");
        }
        assert!(adapter.caps().contains(HarnessCaps::EFFORT));
    }

    #[test]
    fn a_variant_is_the_level_held_to_what_the_provider_takes() {
        // Itself where the provider has it.
        assert_eq!(variant(Some("anthropic/x"), Effort::High), Some("high"));
        assert_eq!(variant(Some("anthropic/x"), Effort::Max), Some("max"));
        assert_eq!(variant(Some("openai/x"), Effort::Xhigh), Some("xhigh"));
        assert_eq!(variant(Some("openai/x"), Effort::Minimal), Some("minimal"));
        // The nearest below.
        assert_eq!(variant(Some("anthropic/x"), Effort::Xhigh), Some("high"));
        assert_eq!(variant(Some("openai/x"), Effort::Max), Some("xhigh"));
        assert_eq!(variant(Some("google/x"), Effort::Medium), Some("low"));
        assert_eq!(variant(Some("google/x"), Effort::Max), Some("high"));
        // Nothing below: the lowest above.
        assert_eq!(variant(Some("anthropic/x"), Effort::Low), Some("high"));
        assert_eq!(variant(Some("google/x"), Effort::Minimal), Some("low"));
        // No known provider: nothing, whatever was asked.
        for level in Effort::ALL {
            assert_eq!(variant(Some("ollama/llama3"), level), None);
            assert_eq!(variant(Some("claude-opus-5-5"), level), None);
            assert_eq!(variant(None, level), None);
        }
    }

    #[test]
    fn every_turn_names_the_variant_after_the_model_and_none_when_none_is_set() {
        // None set: the line is what it was.
        let spawn = spawn_fn("opencode".into(), &asking(Some("openai/gpt-5"), None));
        let proc = spawn("go", None);
        assert!(!proc.args.iter().any(|a| a == "--variant"));
        assert_eq!(proc.args.last().map(String::as_str), Some("go"));

        // Set: right after the model's id, on the first turn and on the next.
        let spec = asking(Some("openai/gpt-5"), Some(Effort::Xhigh));
        let spawn = spawn_fn("opencode".into(), &spec);
        let first = spawn("go", None);
        assert_eq!(
            first.args,
            [
                "run",
                "--format",
                "json",
                "--dir",
                "/work/here",
                "--model",
                "openai/gpt-5",
                "--variant",
                "xhigh",
                "go"
            ]
        );
        let second = spawn("and then", Some("ses_abc"));
        assert_eq!(
            second.args,
            [
                "run",
                "--format",
                "json",
                "--dir",
                "/work/here",
                "--model",
                "openai/gpt-5",
                "--variant",
                "xhigh",
                "--session",
                "ses_abc",
                "and then"
            ]
        );

        // A model whose provider has no variant, and no model: none is named.
        for model in [Some("ollama/llama3"), None] {
            let proc = spawn_fn("opencode".into(), &asking(model, Some(Effort::High)))("go", None);
            assert!(!proc.args.iter().any(|a| a == "--variant"), "{model:?}");
            assert_eq!(held(asking(model, Some(Effort::High))).effort, None);
        }
    }

    #[test]
    fn a_revived_session_is_given_the_model_and_the_effort_it_ran_with() {
        let token = ResumeToken {
            adapter_id: ADAPTER_ID.to_string(),
            native_id: "ses_abc".into(),
            cwd: std::path::PathBuf::from("/work/here"),
            transcript_path: None,
            model: Some("anthropic/claude-sonnet-5-5".into()),
            effort: Some(Effort::Max),
        };
        let spec = revival_spec(&token);
        assert_eq!(spec.cwd, token.cwd);
        assert_eq!(spec.model, token.model);
        assert_eq!(spec.effort, Some(Effort::Max));
        let proc = spawn_fn("opencode".into(), &spec)("go on", Some("ses_abc"));
        let at = proc.args.iter().position(|a| a == "--model").unwrap();
        assert_eq!(
            proc.args[at..at + 4],
            ["--model", "anthropic/claude-sonnet-5-5", "--variant", "max"]
        );
        assert_eq!(proc.args.last().map(String::as_str), Some("go on"));
        // A session that ran with neither is revived with neither.
        let plain = ResumeToken {
            model: None,
            effort: None,
            ..token
        };
        let proc = spawn_fn("opencode".into(), &revival_spec(&plain))("go on", Some("ses_abc"));
        assert!(!proc.args.iter().any(|a| a == "--variant" || a == "--model"));
    }

    #[tokio::test]
    async fn a_sessions_token_says_the_model_and_the_effort_it_was_launched_with() {
        let spec = held(asking(
            Some("anthropic/claude-opus-5-5"),
            Some(Effort::Xhigh),
        ));
        // No prompt, so no process is started: the facade alone.
        let session = OneShotSession::start(
            ADAPTER_ID,
            spawn_fn("opencode".into(), &spec),
            map_fn(),
            &spec,
            Some("ses_abc"),
            true,
        )
        .await
        .unwrap();
        let token = session.resume_token().unwrap();
        assert_eq!(token.native_id, "ses_abc");
        assert_eq!(token.model.as_deref(), Some("anthropic/claude-opus-5-5"));
        assert_eq!(
            token.effort,
            Some(Effort::High),
            "held to what the provider takes"
        );
    }

    #[test]
    fn a_listed_model_says_the_efforts_its_provider_takes() {
        let listed = listed_model("anthropic/claude-opus-5-5\n").unwrap();
        assert_eq!(listed.id, "anthropic/claude-opus-5-5");
        assert_eq!(listed.label, None);
        assert_eq!(listed.efforts, [Effort::High, Effort::Max]);
        assert!(listed_model("ollama/llama3").unwrap().efforts.is_empty());
        // A blank line is not a model.
        assert_eq!(listed_model(""), None);
        assert_eq!(listed_model("   "), None);
    }
}
