//! Codex CLI adapter — `codex exec --json`, one process per turn, JSONL
//! events on stdout, resume via `codex exec resume <session_id>`.
//!
//! Effort: `-c model_reasoning_effort="<level>"` after `-m`, on every turn —
//! a config override for this run and no other
//! (https://learn.chatgpt.com/docs/config-file/config-reference, read
//! 2026-09-29: a `-c` value is read as TOML, so the level is a quoted
//! string). The levels offered are the three every Codex model takes.
//!
//! Event shapes verified against the public Codex CLI reference (non-
//! interactive mode): flat `{"type": ...}` lines — `thread.started`,
//! `turn.started`, `item.started` / `item.completed` (a command, a file
//! change, an MCP call, a web search, a message), `turn.completed` with the
//! turn's `usage`, `turn.failed`, `error`. The binary is not assumed
//! installed. Unrecognized events pass through as `Raw`.

use std::sync::Arc;

use async_trait::async_trait;
use bisa_core::{HarnessCaps, ToolTier};
use bisa_harness::proc::ProcSpec;
use bisa_harness::{
    BoxEventStream, Effort, HarnessAdapter, HarnessError, HarnessSession, InteractiveLaunch,
    LifecycleEvent, Phase, ProbeResult, ProgressEvent, PromptInput, ResumeToken, SessionEvent,
    SessionSnapshot, SessionSpec, Steer,
};

use crate::oneshot::{MapFn, OneShotSession, SpawnFn};
use crate::util::{self, Drive, Shared};

pub const ADAPTER_ID: &str = "codex";

pub struct CodexAdapter {
    pub program: String,
}

impl Default for CodexAdapter {
    fn default() -> Self {
        Self {
            program: "codex".into(),
        }
    }
}

/// The levels Codex is sent, whatever the model: the config reference says
/// the available ones "depend on the model and client", and these three are
/// the ones every model takes.
pub const EFFORTS: &[Effort] = &[Effort::Low, Effort::Medium, Effort::High];

/// The effort to send: the one asked, held to [`EFFORTS`].
fn effort_sent(effort: Option<Effort>) -> Option<Effort> {
    effort.and_then(|e| e.clamp_to(EFFORTS))
}

/// The config override that sets the effort for one run. The value is TOML,
/// so the level is a string in double quotes.
fn effort_override(level: Effort) -> String {
    format!("model_reasoning_effort=\"{}\"", level.as_str())
}

/// The spec with its effort held to what Codex takes — what every turn's
/// command line and the session's resume token are built from.
fn held(spec: SessionSpec) -> SessionSpec {
    SessionSpec {
        effort: effort_sent(spec.effort),
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
    let effort = effort_sent(spec.effort);
    let env = spec.env.clone();
    let env_remove = spec.env_remove.clone();
    // The MCP servers the engine handed the session — the `bisa` server
    // among them, so the agent has the browser (ide/18) — as the config
    // overrides Codex reads for this run and no other.
    let transports: Vec<_> = spec.mcp_servers.iter().map(|m| m.config.clone()).collect();
    let mcp = crate::mcp_inject::codex_overrides(&transports);
    Arc::new(move |prompt: &str, resume: Option<&str>| {
        let mut p = ProcSpec::new(&program).arg("exec").cwd(&cwd);
        if let Some(id) = resume {
            p = p.arg("resume").arg(id);
        }
        p = p.arg("--json");
        if let Some(m) = &model {
            p = p.arg("-m").arg(m);
        }
        if let Some(level) = effort {
            p = p.arg("-c").arg(effort_override(level));
        }
        for a in &mcp {
            p = p.arg(a);
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

fn map_fn() -> Arc<MapFn> {
    Arc::new(|shared: &Shared, value: serde_json::Value| map_codex_event(shared, value))
}

/// The kind of work an item is, as `codex exec --json` names it, and the
/// tier and word a row reads it as. `None` for an item that is not a tool —
/// a message, reasoning, a plan.
fn item_tool(item: &serde_json::Value) -> Option<(&'static str, ToolTier)> {
    match item.get("type").and_then(|t| t.as_str())? {
        "command_execution" => Some(("shell", ToolTier::Exec)),
        "file_change" => Some(("apply_patch", ToolTier::Write)),
        "mcp_tool_call" => Some(("mcp", ToolTier::Exec)),
        "web_search" => Some(("web_search", ToolTier::Read)),
        _ => None,
    }
}

/// An item's own id — what ties `item.completed` to its `item.started`.
fn item_id(item: &serde_json::Value) -> Option<String> {
    item.get("id")
        .and_then(|v| v.as_str())
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}

/// What an item's arguments read as on the row: a command, the files a
/// patch touches, an MCP tool's name, a search's query.
fn item_args(item: &serde_json::Value) -> String {
    if let Some(cmd) = item.get("command") {
        return util::summarize_args(cmd, 160);
    }
    if let Some(changes) = item.get("changes").and_then(|c| c.as_array()) {
        let paths: Vec<&str> = changes
            .iter()
            .filter_map(|c| c.get("path").and_then(|p| p.as_str()))
            .collect();
        return paths.join(", ");
    }
    if let Some(query) = item.get("query").and_then(|q| q.as_str()) {
        return query.to_string();
    }
    match (
        item.get("server").and_then(|s| s.as_str()),
        item.get("tool").and_then(|t| t.as_str()),
    ) {
        (Some(server), Some(tool)) => format!("{server}/{tool}"),
        (_, Some(tool)) => tool.to_string(),
        _ => String::new(),
    }
}

/// One line of `codex exec --json` → the events it means. The vocabulary is
/// the current one — `thread.started`, `turn.started`, `item.started`,
/// `item.completed`, `turn.completed` with its `usage`, `turn.failed`,
/// `error` — verified against the public reference; a line the row does
/// not read passes through as `Raw`.
fn map_codex_event(shared: &Shared, value: serde_json::Value) -> Drive {
    let kind = value.get("type").and_then(|t| t.as_str()).unwrap_or("");
    match kind {
        "thread.started" => {
            if let Some(id) = value.get("thread_id").and_then(|v| v.as_str()) {
                shared.set_native_id(id);
            }
            // The model the thread runs on, when the line names it.
            if let Some(model) = value
                .get("model")
                .and_then(|v| v.as_str())
                .filter(|m| !m.is_empty())
            {
                shared
                    .broadcaster
                    .emit(SessionEvent::Progress(ProgressEvent::ModelChanged {
                        model: model.to_string(),
                    }));
            }
            shared
                .broadcaster
                .emit(SessionEvent::Lifecycle(LifecycleEvent::Started));
        }
        "turn.started" => shared
            .broadcaster
            .emit(SessionEvent::Progress(ProgressEvent::TurnStarted)),
        // The turn's usage arrives with its end: the cost, then the turn over.
        "turn.completed" => {
            if let Some(usage) = value.get("usage") {
                let count = |key: &str| usage.get(key).and_then(|v| v.as_u64()).unwrap_or(0);
                shared
                    .broadcaster
                    .emit(SessionEvent::Progress(ProgressEvent::CostDelta {
                        input_tokens: count("input_tokens"),
                        output_tokens: count("output_tokens") + count("reasoning_output_tokens"),
                        usd_cents: 0,
                    }));
            }
            shared
                .broadcaster
                .emit(SessionEvent::Progress(ProgressEvent::TurnEnded));
        }
        "item.started" => {
            let item = value
                .get("item")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            match item_tool(&item) {
                Some((name, tier)) => {
                    shared.set_activity(name);
                    shared
                        .broadcaster
                        .emit(SessionEvent::Progress(ProgressEvent::ToolStarted {
                            name: name.into(),
                            args_summary: item_args(&item),
                            tier,
                            id: item_id(&item),
                        }));
                }
                None => shared.broadcaster.emit(SessionEvent::Raw(value.clone())),
            }
        }
        "item.completed" => {
            let item = value
                .get("item")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            match item_tool(&item) {
                Some((name, _)) => {
                    // A command says its exit code; a patch, its status; the
                    // rest are over when they are over.
                    let ok = item
                        .get("exit_code")
                        .and_then(|v| v.as_i64())
                        .map(|c| c == 0)
                        .or_else(|| {
                            item.get("status")
                                .and_then(|s| s.as_str())
                                .map(|s| s != "failed")
                        })
                        .unwrap_or(true);
                    shared
                        .broadcaster
                        .emit(SessionEvent::Progress(ProgressEvent::ToolEnded {
                            name: name.into(),
                            ok,
                            id: item_id(&item),
                        }));
                }
                None => match item.get("type").and_then(|t| t.as_str()) {
                    Some("agent_message") => {
                        if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                            shared.broadcaster.emit(SessionEvent::Progress(
                                ProgressEvent::TextDelta {
                                    text: text.to_string(),
                                },
                            ));
                        }
                    }
                    // The model's reasoning, whole, as Codex hands it when
                    // the item completes.
                    Some("reasoning") => {
                        if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                            shared.broadcaster.emit(SessionEvent::Progress(
                                ProgressEvent::ThinkingDelta {
                                    text: text.to_string(),
                                },
                            ));
                        }
                    }
                    _ => shared.broadcaster.emit(SessionEvent::Raw(value.clone())),
                },
            }
        }
        // Codex's own error channel — the mid-run site. A model wall here
        // ends the turn as `ModelUnavailable` so the engine retries on the
        // next model instead of recording a failure.
        "error" | "turn.failed" => {
            let message = value
                .get("message")
                .and_then(|v| v.as_str())
                .or_else(|| value.pointer("/error/message").and_then(|v| v.as_str()))
                .unwrap_or("unknown error");
            if let Some(outcome) = shared.model_ctx.outcome(message) {
                tracing::warn!(target: "bisa_adapters::codex", "model unavailable: {message}");
                return Drive::End(outcome);
            }
            tracing::warn!(target: "bisa_adapters::codex", "error event: {message}");
            shared.broadcaster.emit(SessionEvent::Raw(value.clone()));
        }
        // Plans, deltas: not the row's; the one-shot driver emits the
        // non-terminal Ended on exit.
        _ => shared.broadcaster.emit(SessionEvent::Raw(value.clone())),
    }
    Drive::Continue
}

#[cfg(test)]
mod stream_tests {
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
    async fn a_thread_starts_the_session_and_a_turn_opens_and_closes_with_its_usage() {
        let (s, mut stream) = shared();
        assert!(matches!(
            map_codex_event(
                &s,
                json!({"type": "thread.started", "thread_id": "0199a213"})
            ),
            Drive::Continue
        ));
        let events = drain(&mut stream, 1).await;
        assert!(matches!(
            &events[0],
            SessionEvent::Lifecycle(LifecycleEvent::Started)
        ));
        map_codex_event(&s, json!({"type": "turn.started"}));
        let events = drain(&mut stream, 1).await;
        assert!(matches!(
            &events[0],
            SessionEvent::Progress(ProgressEvent::TurnStarted)
        ));
        map_codex_event(
            &s,
            json!({"type": "turn.completed", "usage": {"input_tokens": 24763, "cached_input_tokens": 24448, "output_tokens": 122, "reasoning_output_tokens": 8}}),
        );
        let events = drain(&mut stream, 2).await;
        assert!(
            matches!(
                &events[0],
                SessionEvent::Progress(ProgressEvent::CostDelta {
                    input_tokens: 24763,
                    output_tokens: 130,
                    usd_cents: 0
                })
            ),
            "the turn's usage is the cost, reasoning tokens counted as output: {events:?}"
        );
        assert!(matches!(
            &events[1],
            SessionEvent::Progress(ProgressEvent::TurnEnded)
        ));
    }

    #[tokio::test]
    async fn an_item_is_a_tool_while_it_runs_and_says_how_it_ended() {
        let (s, mut stream) = shared();
        map_codex_event(
            &s,
            json!({"type": "item.started", "item": {"id": "item_1", "type": "command_execution", "command": "cargo test", "status": "in_progress"}}),
        );
        let events = drain(&mut stream, 1).await;
        assert!(
            matches!(&events[0], SessionEvent::Progress(ProgressEvent::ToolStarted { name, args_summary, tier: ToolTier::Exec, .. }) if name == "shell" && args_summary == "cargo test")
        );
        map_codex_event(
            &s,
            json!({"type": "item.completed", "item": {"id": "item_1", "type": "command_execution", "command": "cargo test", "exit_code": 101, "status": "completed"}}),
        );
        let events = drain(&mut stream, 1).await;
        assert!(
            matches!(&events[0], SessionEvent::Progress(ProgressEvent::ToolEnded { name, ok: false, .. }) if name == "shell"),
            "a non-zero exit is the tool's failure"
        );
        map_codex_event(
            &s,
            json!({"type": "item.started", "item": {"id": "item_2", "type": "file_change", "changes": [{"path": "src/cart.rs", "kind": "update"}], "status": "in_progress"}}),
        );
        let events = drain(&mut stream, 1).await;
        assert!(
            matches!(&events[0], SessionEvent::Progress(ProgressEvent::ToolStarted { name, args_summary, tier: ToolTier::Write, .. }) if name == "apply_patch" && args_summary == "src/cart.rs")
        );
        map_codex_event(
            &s,
            json!({"type": "item.completed", "item": {"id": "item_2", "type": "file_change", "status": "completed"}}),
        );
        let events = drain(&mut stream, 1).await;
        assert!(matches!(
            &events[0],
            SessionEvent::Progress(ProgressEvent::ToolEnded { ok: true, .. })
        ));
        map_codex_event(
            &s,
            json!({"type": "item.completed", "item": {"id": "item_3", "type": "agent_message", "text": "Done."}}),
        );
        let events = drain(&mut stream, 1).await;
        assert!(
            matches!(&events[0], SessionEvent::Progress(ProgressEvent::TextDelta { text }) if text == "Done.")
        );
        map_codex_event(
            &s,
            json!({"type": "item.completed", "item": {"id": "item_4", "type": "reasoning", "text": "hmm"}}),
        );
        let events = drain(&mut stream, 1).await;
        assert!(
            matches!(&events[0], SessionEvent::Progress(ProgressEvent::ThinkingDelta { text }) if text == "hmm"),
            "reasoning is the turn's thinking, never its words: {events:?}"
        );
    }

    #[tokio::test]
    async fn a_failed_turn_is_the_models_wall_when_the_classifier_says_so() {
        let (s, _stream) = shared();
        assert!(matches!(
            map_codex_event(
                &s,
                json!({"type": "turn.failed", "error": {"message": "rate limit exceeded, try again"}})
            ),
            Drive::End(bisa_harness::Outcome::ModelUnavailable { .. })
        ));
        assert!(matches!(
            map_codex_event(&s, json!({"type": "error", "message": "the tool crashed"})),
            Drive::Continue
        ));
    }
}

#[async_trait]
impl HarnessAdapter for CodexAdapter {
    fn id(&self) -> &str {
        ADAPTER_ID
    }

    fn display_name(&self) -> &str {
        "Codex CLI"
    }

    /// `SUBAGENTS`: the terminal form's hooks announce them
    /// (`SubagentStart` / `SubagentStop`, `hooks/codex.rs`).
    fn caps(&self) -> HarnessCaps {
        HarnessCaps::RESUME
            | HarnessCaps::MCP_SERVERS
            | HarnessCaps::COST_REPORTING
            | HarnessCaps::USAGE_REPORTING
            | HarnessCaps::SUBAGENTS
            | HarnessCaps::EFFORT
    }

    fn efforts(&self, model: Option<&str>) -> Vec<Effort> {
        let _ = model;
        EFFORTS.to_vec()
    }

    /// The account's windows and plan from `codex app-server` —
    /// `account/read` and `account/rateLimits/read`; Codex reads its own sign-in.
    async fn usage(&self) -> bisa_harness::UsageState {
        crate::usage::codex::read(&self.program, self.id()).await
    }

    /// The bare CLI. `launch` above runs it in `exec --json`; this is the command a
    /// person types. `resume --last` is the CLI reference's own form: "Skip
    /// the picker and resume the most recent chat from the current working
    /// directory" — directory-scoped, as every resume form here must be.
    fn interactive(&self) -> Option<InteractiveLaunch> {
        Some(
            InteractiveLaunch::new(&self.program)
                .resumable_with(vec!["resume".into(), "--last".into()]),
        )
    }

    fn interactive_reporting(
        &self,
        ctx: &bisa_harness::ReportingContext,
    ) -> bisa_harness::ReportingPlan {
        crate::hooks::codex::reporting(ctx)
    }

    fn translate_report(&self, payload: &serde_json::Value) -> Vec<SessionEvent> {
        crate::hooks::codex::translate(payload)
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
        Ok(Box::new(CodexSession(session)))
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
        Ok(Box::new(CodexSession(session)))
    }
}

pub struct CodexSession(OneShotSession);

#[async_trait]
impl HarnessSession for CodexSession {
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
mod interactive_tests {
    use super::*;

    #[test]
    fn the_terminal_form_resumes_the_most_recent_chat_here_without_a_picker() {
        let launch = CodexAdapter::default().interactive().unwrap();
        assert_eq!(launch.program, "codex");
        assert_eq!(
            launch.resume_args,
            vec!["resume".to_string(), "--last".to_string()]
        );
    }

    /// The MCP servers the engine hands the session ride `codex exec` as
    /// config overrides — the `bisa` server among them, so an agent on Codex
    /// has the browser (ide/18) — and a spec with none adds nothing.
    #[test]
    fn a_launch_carries_the_engines_mcp_servers_as_config_overrides() {
        let spec = SessionSpec {
            work_item: None,
            cwd: std::env::temp_dir(),
            prompt: String::new(),
            model: Some("o4-mini".into()),
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
        let proc = spawn_fn("codex".into(), &spec)("hello", None);
        let args = proc.args.clone();
        let at = args.iter().position(|a| a == "-c").expect("an override");
        assert_eq!(
            &args[at..at + 4],
            &[
                "-c".to_string(),
                "mcp_servers.bisa.command=\"/opt/bisa\"".to_string(),
                "-c".to_string(),
                "mcp_servers.bisa.args=[\"mcp\", \"--socket\", \"/tmp/i.sock\"]".to_string(),
            ]
        );
        assert_eq!(
            args.last().map(String::as_str),
            Some("hello"),
            "the prompt stays last"
        );
        assert!(
            args.iter().position(|a| a == "-m").unwrap() < at,
            "the model before the overrides"
        );
        let bare = SessionSpec {
            mcp_servers: vec![],
            ..spec.clone()
        };
        let proc = spawn_fn("codex".into(), &bare)("hello", Some("thread-1"));
        assert!(
            !proc.args.iter().any(|a| a == "-c"),
            "no server, no override"
        );
        assert_eq!(&proc.args[..3], &["exec", "resume", "thread-1"]);
        assert!(
            CodexAdapter::default()
                .caps()
                .contains(HarnessCaps::MCP_SERVERS),
            "the cap says so"
        );

        // An effort is one more override: right after the model, before the
        // servers', and the prompt still last.
        let asked = SessionSpec {
            effort: Some(Effort::High),
            ..spec
        };
        let proc = spawn_fn("codex".into(), &asked)("hello", None);
        let args = proc.args.clone();
        let model_at = args.iter().position(|a| a == "-m").unwrap();
        assert_eq!(
            &args[model_at..model_at + 4],
            &[
                "-m".to_string(),
                "o4-mini".to_string(),
                "-c".to_string(),
                "model_reasoning_effort=\"high\"".to_string(),
            ]
        );
        assert_eq!(
            args[model_at + 4..model_at + 6],
            [
                "-c".to_string(),
                "mcp_servers.bisa.command=\"/opt/bisa\"".to_string()
            ],
            "the servers' overrides follow"
        );
        assert_eq!(args.last().map(String::as_str), Some("hello"));
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

    fn effort_args(args: &[String]) -> Vec<&str> {
        args.iter()
            .filter(|a| a.starts_with("model_reasoning_effort="))
            .map(String::as_str)
            .collect()
    }

    #[test]
    fn every_turn_is_given_the_effort_and_none_is_sent_when_none_is_set() {
        // None set: no override, on the first turn and on a resumed one.
        let spawn = spawn_fn("codex".into(), &bare());
        for resume in [None, Some("thread-1")] {
            let proc = spawn("go", resume);
            assert!(!proc.args.iter().any(|a| a == "-c"), "{resume:?}");
            assert_eq!(proc.args.last().map(String::as_str), Some("go"));
        }
        // Set, with no model: the override alone, after `--json`.
        let asked = SessionSpec {
            effort: Some(Effort::Medium),
            ..bare()
        };
        let spawn = spawn_fn("codex".into(), &asked);
        let first = spawn("go", None);
        assert_eq!(
            first.args,
            [
                "exec",
                "--json",
                "-c",
                "model_reasoning_effort=\"medium\"",
                "go"
            ]
        );
        // The second turn resumes the thread and is told again.
        let second = spawn("and then", Some("thread-1"));
        assert_eq!(
            second.args,
            [
                "exec",
                "resume",
                "thread-1",
                "--json",
                "-c",
                "model_reasoning_effort=\"medium\"",
                "and then"
            ]
        );
    }

    #[test]
    fn codex_is_sent_one_of_its_three_levels_whatever_was_asked() {
        let adapter = CodexAdapter::default();
        assert!(adapter.caps().contains(HarnessCaps::EFFORT));
        let three = [Effort::Low, Effort::Medium, Effort::High];
        assert_eq!(adapter.efforts(None), three);
        assert_eq!(adapter.efforts(Some("gpt-5-codex")), three);
        assert_eq!(adapter.efforts(Some("anything-else")), three);
        let sent = |asked: Effort| {
            let spec = SessionSpec {
                effort: Some(asked),
                ..bare()
            };
            let proc = spawn_fn("codex".into(), &spec)("go", None);
            effort_args(&proc.args)
                .first()
                .map(|a| a.to_string())
                .unwrap_or_default()
        };
        assert_eq!(sent(Effort::Low), "model_reasoning_effort=\"low\"");
        assert_eq!(sent(Effort::High), "model_reasoning_effort=\"high\"");
        // Above the highest: the nearest below. Below the lowest: the lowest.
        assert_eq!(sent(Effort::Xhigh), "model_reasoning_effort=\"high\"");
        assert_eq!(sent(Effort::Max), "model_reasoning_effort=\"high\"");
        assert_eq!(sent(Effort::Minimal), "model_reasoning_effort=\"low\"");
        // The value is a TOML string: the level in double quotes.
        assert_eq!(
            effort_override(Effort::Medium),
            "model_reasoning_effort=\"medium\""
        );
        assert_eq!(held(bare()).effort, None);
    }

    #[test]
    fn a_revived_session_is_given_the_model_and_the_effort_it_ran_with() {
        let token = ResumeToken {
            adapter_id: ADAPTER_ID.to_string(),
            native_id: "thread-7".into(),
            cwd: std::path::PathBuf::from("/work/here"),
            transcript_path: None,
            model: Some("gpt-5-codex".into()),
            effort: Some(Effort::High),
        };
        let spec = revival_spec(&token);
        assert_eq!(spec.cwd, token.cwd);
        assert_eq!(spec.model.as_deref(), Some("gpt-5-codex"));
        assert_eq!(spec.effort, Some(Effort::High));
        let proc = spawn_fn("codex".into(), &spec)("go on", Some(token.native_id.as_str()));
        assert_eq!(
            proc.args,
            [
                "exec",
                "resume",
                "thread-7",
                "--json",
                "-m",
                "gpt-5-codex",
                "-c",
                "model_reasoning_effort=\"high\"",
                "go on"
            ]
        );
        // A session that ran with neither is revived with neither.
        let plain = ResumeToken {
            model: None,
            effort: None,
            ..token
        };
        let proc = spawn_fn("codex".into(), &revival_spec(&plain))("go on", Some("thread-7"));
        assert_eq!(proc.args, ["exec", "resume", "thread-7", "--json", "go on"]);
    }

    #[tokio::test]
    async fn a_sessions_token_says_the_model_and_the_effort_it_was_launched_with() {
        let spec = held(SessionSpec {
            model: Some("gpt-5-codex".into()),
            effort: Some(Effort::Max),
            ..bare()
        });
        // No prompt, so no process is started: the facade alone.
        let session = OneShotSession::start(
            ADAPTER_ID,
            spawn_fn("codex".into(), &spec),
            map_fn(),
            &spec,
            Some("thread-7"),
            true,
        )
        .await
        .unwrap();
        let token = session.resume_token().unwrap();
        assert_eq!(token.adapter_id, ADAPTER_ID);
        assert_eq!(token.native_id, "thread-7");
        assert_eq!(token.model.as_deref(), Some("gpt-5-codex"));
        assert_eq!(token.effort, Some(Effort::High), "held to what Codex takes");
    }
}
