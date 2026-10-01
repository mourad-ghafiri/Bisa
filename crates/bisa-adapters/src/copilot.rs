//! GitHub Copilot CLI adapter — `copilot --acp --stdio`, the CLI's Agent
//! Client Protocol server over stdio, one process per session.
//!
//! The protocol is `acp.rs`'s; this module is what only Copilot CLI knows.
//! Read on 2026-09-30 from
//! https://docs.github.com/en/copilot/reference/copilot-cli-reference/acp-server,
//! .../cli-command-reference and the CLI's own changelog
//! (https://github.com/github/copilot-cli/blob/main/changelog.md):
//!
//! - **The command.** `--acp` starts the server and `--stdio` names the
//!   transport ("stdio mode is inferred by default … you can also use the
//!   `--stdio` option for disambiguation"). `--no-ask-user` "disable[s] the
//!   `ask_user` tool (the agent works autonomously without asking
//!   questions)": nothing documents how that tool reaches an ACP client, and
//!   a question nobody is shown stalls a run — an agent asks a person through
//!   the platform's own tools.
//! - **Model and effort** are session config options ("ACP server supports
//!   changing models during a session", "ACP clients can configure reasoning
//!   effort via session config options"), so both are set the protocol's way
//!   by `acp::open` and the command carries neither word.
//! - **Permission.** The server asks (`session/request_permission`) before a
//!   tool that needs it, unless it was started with an allow-all word or
//!   `COPILOT_ALLOW_ALL` is in its environment ("Allow all permissions
//!   automatically"). Neither word is ever passed, and the variable is taken
//!   out of the child's environment: the ask the guard judges is always
//!   made. A person's own saved approvals still answer before it — the
//!   guard judges what is asked.
//! - **MCP servers** ride `session/new` ("ACP clients can now provide MCP
//!   servers (stdio, HTTP, SSE) when starting or loading sessions"), and a
//!   session is loaded again by its id ("ACP server supports loading
//!   existing sessions").
//! - **Effort levels.** `--effort` documents `low`, `medium`, `high`,
//!   `xhigh`, `max`, and "`max` is the highest-depth tier for Anthropic
//!   models": a `claude-*` model may be asked for all five, any other for
//!   the first four, and the session holds the level to what it offers.
//! - **Account usage** has no documented source: `usage()` is the default,
//!   *unsupported*, in the harness's own name.

use async_trait::async_trait;
use bisa_core::HarnessCaps;
use bisa_harness::{
    Effort, HarnessAdapter, HarnessError, HarnessSession, InteractiveLaunch, ModelInfo, ModelPlan,
    ProbeResult, ReportingContext, ReportingPlan, ResumeToken, SessionEvent, SessionSpec,
};

use crate::acp::{self, AcpCommand};
use crate::util;

pub const ADAPTER_ID: &str = "copilot";

/// The words that put the CLI in protocol mode, and nothing else: no model,
/// no effort, and never a word that allows a tool unasked.
const PROTOCOL_WORDS: [&str; 3] = ["--acp", "--stdio", "--no-ask-user"];

/// The variable that makes the CLI allow every tool unasked. A session the
/// engine drives never inherits it.
pub const ALLOW_ALL_ENV: &str = "COPILOT_ALLOW_ALL";

/// The model the CLI runs when none is named — "General-purpose coding
/// (default)" in the reference's table — and the one the platform leads
/// with: the setup gate's plan, and the judge.
pub const DEFAULT_MODEL: &str = "claude-sonnet-4.6";

/// The reference's *Supported models* table, in its order. `--model` "also
/// takes" `auto`; it is not listed, because a session on it says nothing of
/// which model ran. The UI keeps free-text entry, so a newer model is never
/// locked out.
const LISTED_MODELS: [&str; 11] = [
    DEFAULT_MODEL,
    "gpt-5.4",
    "gpt-6-astra",
    "gpt-6-sol",
    "gpt-6-luna",
    "claude-opus-5.5",
    "claude-haiku-4.5",
    "gpt-5.3-codex",
    "gemini-3.5-flash",
    "gemini-3.6-flash",
    "gemini-3.7-flash",
];

const LEVELS: [Effort; 4] = [Effort::Low, Effort::Medium, Effort::High, Effort::Xhigh];
const ANTHROPIC_LEVELS: [Effort; 5] = [
    Effort::Low,
    Effort::Medium,
    Effort::High,
    Effort::Xhigh,
    Effort::Max,
];

/// The levels `model` may be asked for, lowest first: all five for an
/// Anthropic model, the first four for any other — and for none, since the
/// model the CLI then runs is the person's own default, which nobody here
/// knows.
pub fn efforts_for(model: Option<&str>) -> &'static [Effort] {
    match model.map(str::trim) {
        Some(model) if model.starts_with("claude-") => &ANTHROPIC_LEVELS,
        _ => &LEVELS,
    }
}

/// The spec as a session of Copilot CLI is started with it: whatever would
/// allow every tool unasked is taken out of its environment.
fn asking(mut spec: SessionSpec) -> SessionSpec {
    if !spec.env_remove.iter().any(|name| name == ALLOW_ALL_ENV) {
        spec.env_remove.push(ALLOW_ALL_ENV.to_string());
    }
    spec
}

pub struct CopilotAdapter {
    /// Binary name; overridable for tests (stub scripts).
    pub program: String,
}

impl Default for CopilotAdapter {
    fn default() -> Self {
        Self {
            program: "copilot".into(),
        }
    }
}

impl CopilotAdapter {
    /// How the CLI is started as an ACP server.
    pub fn command(&self) -> AcpCommand {
        AcpCommand::new(ADAPTER_ID, &self.program, PROTOCOL_WORDS)
    }
}

#[async_trait]
impl HarnessAdapter for CopilotAdapter {
    fn id(&self) -> &str {
        ADAPTER_ID
    }

    fn display_name(&self) -> &str {
        "GitHub Copilot CLI"
    }

    /// As every ACP agent: the ask stops before the tool runs and obeys a
    /// refusal, so the guard can veto; the reply picks an option, so no
    /// input can be rewritten.
    fn caps(&self) -> HarnessCaps {
        HarnessCaps::MCP_SERVERS
            | HarnessCaps::INPUT_REQUESTS
            | HarnessCaps::RESUME
            | HarnessCaps::TOOL_GUARD
            | HarnessCaps::EFFORT
    }

    fn efforts(&self, model: Option<&str>) -> Vec<Effort> {
        efforts_for(model).to_vec()
    }

    /// The reference's table — Copilot CLI has no list-models command — each
    /// with the levels it may be asked for.
    async fn models(&self) -> Vec<ModelInfo> {
        LISTED_MODELS
            .iter()
            .map(|id| ModelInfo::new(*id, None, efforts_for(Some(*id)).to_vec()))
            .collect()
    }

    fn recommended_plan(&self) -> Option<ModelPlan> {
        Some(ModelPlan::pinned(DEFAULT_MODEL))
    }

    fn recommended_judge(&self) -> Option<String> {
        Some(DEFAULT_MODEL.to_string())
    }

    /// The bare CLI. `launch` below runs it as a protocol server; this is
    /// the command a person types, and `--continue` "resume[s] the most
    /// recent session in the current working directory".
    fn interactive(&self) -> Option<InteractiveLaunch> {
        Some(InteractiveLaunch::new(&self.program).resumable_with(vec!["--continue".into()]))
    }

    fn interactive_reporting(&self, ctx: &ReportingContext) -> ReportingPlan {
        crate::hooks::copilot::reporting(ctx)
    }

    fn translate_report(&self, payload: &serde_json::Value) -> Vec<SessionEvent> {
        crate::hooks::copilot::translate(payload)
    }

    /// A name an editor's launcher answers to as well: the CLI is there only
    /// when it answers with a version.
    async fn probe(&self) -> ProbeResult {
        util::probe_versioned(&self.program, &["--version"]).await
    }

    async fn launch(&self, spec: SessionSpec) -> Result<Box<dyn HarnessSession>, HarnessError> {
        acp::open(self.command(), &asking(spec), None).await
    }

    async fn attach(&self, token: &ResumeToken) -> Result<Box<dyn HarnessSession>, HarnessError> {
        let spec = asking(acp::revival(ADAPTER_ID, token)?);
        acp::open(self.command(), &spec, Some(token.native_id.clone())).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_core::ToolTier;

    fn bare() -> SessionSpec {
        SessionSpec {
            work_item: None,
            cwd: "/work/here".into(),
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

    #[test]
    fn the_command_is_the_protocol_server_and_never_a_word_that_allows_a_tool_unasked() {
        let adapter = CopilotAdapter::default();
        assert_eq!(
            adapter.command(),
            AcpCommand::new("copilot", "copilot", ["--acp", "--stdio", "--no-ask-user"])
        );
        for word in &adapter.command().args {
            assert!(
                !word.contains("allow") && !word.contains("yolo"),
                "{word} widens what the CLI may do"
            );
            // The model and the effort are the session's, set the protocol's
            // way: a word here would say them twice.
            assert!(
                !word.starts_with("--model") && !word.contains("effort"),
                "{word}"
            );
        }
        // A stub's path is the program, the words unchanged.
        let stub = CopilotAdapter {
            program: "/tmp/stub/copilot".into(),
        };
        assert_eq!(stub.command().program, "/tmp/stub/copilot");
        assert_eq!(stub.command().adapter_id, "copilot");
    }

    #[test]
    fn a_session_never_inherits_the_variable_that_allows_every_tool() {
        let spec = asking(bare());
        assert_eq!(spec.env_remove, ["COPILOT_ALLOW_ALL"]);
        // Said once, however often asked; what else was to go still goes.
        let mut again = asking(spec);
        again.env_remove.insert(0, "HTTPS_PROXY".into());
        assert_eq!(
            asking(again).env_remove,
            ["HTTPS_PROXY", "COPILOT_ALLOW_ALL"]
        );
        // An agent's own environment that names it loses to the removal:
        // `ProcHandle::spawn` takes names away after it adds them.
        let mut named = bare();
        named.env.insert(ALLOW_ALL_ENV.into(), "true".into());
        assert!(asking(named)
            .env_remove
            .contains(&ALLOW_ALL_ENV.to_string()));
    }

    #[test]
    fn the_levels_are_the_references_and_max_is_an_anthropic_models() {
        let adapter = CopilotAdapter::default();
        assert!(adapter.caps().contains(HarnessCaps::EFFORT));
        assert_eq!(
            adapter.efforts(Some("claude-opus-5.5")),
            [
                Effort::Low,
                Effort::Medium,
                Effort::High,
                Effort::Xhigh,
                Effort::Max
            ]
        );
        for model in [
            Some("gpt-5.4"),
            Some("gemini-3.7-flash"),
            Some("auto"),
            None,
        ] {
            assert_eq!(
                adapter.efforts(model),
                [Effort::Low, Effort::Medium, Effort::High, Effort::Xhigh],
                "{model:?}"
            );
        }
        assert_eq!(
            adapter.efforts(Some("  claude-sonnet-4.6 ")).last(),
            Some(&Effort::Max)
        );
        // Never a level the flag does not document.
        for model in [Some("claude-haiku-4.5"), Some("gpt-5.4"), None] {
            assert!(!adapter.efforts(model).contains(&Effort::Minimal));
        }
    }

    #[tokio::test]
    async fn the_models_are_the_references_table_each_with_its_levels() {
        let adapter = CopilotAdapter::default();
        let models = adapter.models().await;
        assert_eq!(models.len(), 11);
        assert_eq!(models[0].id, "claude-sonnet-4.6", "the default leads");
        let mut ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 11, "no model twice");
        for model in &models {
            assert_eq!(
                model.efforts,
                efforts_for(Some(model.id.as_str())),
                "{}",
                model.id
            );
            assert!(model.label.is_none());
        }
        assert!(!models.iter().any(|m| m.id == "auto"));
        // What the platform leads with is a model the table lists.
        assert_eq!(
            adapter.recommended_plan(),
            Some(ModelPlan::pinned("claude-sonnet-4.6"))
        );
        assert_eq!(
            adapter.recommended_judge().as_deref(),
            Some("claude-sonnet-4.6")
        );
        assert!(models.iter().any(|m| m.id == DEFAULT_MODEL));
    }

    #[test]
    fn the_capabilities_are_the_protocols() {
        let caps = CopilotAdapter::default().caps();
        for held in [
            HarnessCaps::MCP_SERVERS,
            HarnessCaps::INPUT_REQUESTS,
            HarnessCaps::RESUME,
            HarnessCaps::TOOL_GUARD,
            HarnessCaps::EFFORT,
        ] {
            assert!(caps.contains(held), "{held:?}");
        }
        // Nothing is declared that the session does not emit or the adapter
        // cannot do: no rewritten input, no usage, no sub-agents, no cost.
        for absent in [
            HarnessCaps::INPUT_REWRITE,
            HarnessCaps::USAGE_REPORTING,
            HarnessCaps::SUBAGENTS,
            HarnessCaps::COST_REPORTING,
            HarnessCaps::STEER,
        ] {
            assert!(!caps.contains(absent), "{absent:?}");
        }
    }

    #[test]
    fn the_terminal_form_is_the_bare_cli_resumed_with_continue_and_reports_through_its_plugin() {
        let adapter = CopilotAdapter {
            program: "copilot-somewhere-else".into(),
        };
        let launch = adapter.interactive().unwrap();
        assert_eq!(launch.program, "copilot-somewhere-else");
        assert!(launch.args.is_empty(), "no protocol word in a terminal");
        assert_eq!(launch.resume_args, ["--continue"]);

        let plan = adapter.interactive_reporting(&ReportingContext {
            session: "01S".into(),
            reporter: vec!["bisa".into(), "session".into(), "report".into()],
            files_dir: "/ws/run/interactive/01S".into(),
            port: None,
            guard: None,
            guard_timeout_secs: 0,
        });
        assert_eq!(plan.args, ["--plugin-dir=/ws/run/interactive/01S"]);
        assert_eq!(plan.files.len(), 2);
        // A terminal session is the person's: the plan names no model and no
        // effort.
        assert!(!plan
            .args
            .iter()
            .any(|a| a.contains("model") || a.contains("effort")));
        // And what the plugin's hooks report is read by this adapter.
        let stop = serde_json::json!({ "hook_event_name": "Stop" });
        assert_eq!(adapter.translate_report(&stop).len(), 2);
    }

    #[tokio::test]
    async fn a_token_of_another_adapter_is_refused_before_anything_is_started() {
        let adapter = CopilotAdapter {
            program: "a-copilot-that-is-not-there".into(),
        };
        let token = ResumeToken {
            adapter_id: "grok".into(),
            native_id: "sess-1".into(),
            cwd: "/work/here".into(),
            transcript_path: None,
            model: None,
            effort: None,
        };
        assert!(matches!(
            adapter.attach(&token).await,
            Err(HarnessError::Protocol(_))
        ));
    }

    #[tokio::test]
    async fn a_binary_that_is_not_there_is_not_installed() {
        let probe = CopilotAdapter {
            program: "a-copilot-that-is-not-there".into(),
        }
        .probe()
        .await;
        assert!(!probe.available);
        assert_eq!(
            probe.reason.as_deref(),
            Some("a-copilot-that-is-not-there not found on PATH")
        );
    }
}
