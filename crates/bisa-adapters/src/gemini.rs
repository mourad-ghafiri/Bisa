//! Gemini CLI adapter — `gemini --acp`, Google's agent CLI as an Agent
//! Client Protocol agent over stdio, one process per session.
//!
//! The protocol is `acp.rs`'s; this module is what only Gemini CLI knows.
//! Read on 2026-10-05 from https://geminicli.com/docs/ (the CLI's own site,
//! linked from the google-gemini/gemini-cli repository) — `cli/acp-mode`,
//! `cli/cli-reference`, `cli/model`, `cli/session-management`,
//! `reference/policy-engine`, `get-started/installation`,
//! `get-started/authentication`, `resources/quota-and-pricing` — and from
//! the repository's `packages/cli/src/config/config.ts` and
//! `packages/cli/src/acp/`:
//!
//! - **The command.** `gemini --acp` "starts the agent in ACP mode": a
//!   JSON-RPC 2.0 agent on stdin and stdout, a process of its own. The
//!   earlier spelling `--experimental-acp` is deprecated for it.
//! - **Model.** A session says its models the draft way — the `models` of
//!   its `session/new` answer, set with `session/set_model` — so the model
//!   is set the protocol's way by `acp::open` and the command carries no
//!   `-m`. There is **no effort**: the session offers no `thought_level`
//!   option and the CLI documents no control for how hard a model works.
//! - **Permission.** The agent asks (`session/request_permission`) before
//!   a tool its default approval mode does not run unasked — every write,
//!   every shell command — and runs read-only tools on its own. `--yolo`,
//!   `--approval-mode` and `--skip-trust` are never passed: the guard judges
//!   what is asked, and a person's own folder trust stands.
//! - **Models** are the page's: `auto` (the default, which "lets the system
//!   choose the best Gemini 3 model for your task"), then the four it names
//!   a person may pick by hand. The CLI prints no list of its own, so the
//!   table is written here with the date it was read; no plan is
//!   recommended — a preview id written here could retire.
//! - **The terminal** is the bare `gemini`, and `--resume latest` "resume[s]
//!   a previous session … the most recent". It reports nothing: the CLI's
//!   hooks live in its `settings.json`, which the platform never writes, and
//!   it takes none for one launch. So the reporting plan is the empty one: a
//!   plain terminal, never a roster row, and not guarded there.
//! - **Account usage** has no source: the quota page states each tier's
//!   daily requests, and nothing reads what is left of them. `usage()` is the
//!   default, *unsupported*.

use async_trait::async_trait;
use bisa_core::HarnessCaps;
use bisa_harness::{
    Effort, HarnessAdapter, HarnessError, HarnessSession, InteractiveLaunch, ModelInfo,
    ProbeResult, ResumeToken, SessionSpec,
};

use crate::acp::{self, AcpCommand};
use crate::util;

pub const ADAPTER_ID: &str = "gemini";

/// The one word that makes the CLI an ACP agent of its own process, and
/// nothing else: no model, no effort, and never an approval word.
const PROTOCOL_WORDS: [&str; 1] = ["--acp"];

/// The words that continue the most recent session of the directory, in a
/// terminal.
const RESUME_WORDS: [&str; 2] = ["--resume", "latest"];

/// The models the CLI's own page names, the default first — read on
/// 2026-10-05 from https://geminicli.com/docs/cli/model. No level beside any:
/// the CLI has no effort control.
const MODELS: [(&str, &str); 5] = [
    ("auto", "Auto"),
    ("gemini-3-pro-preview", "Gemini 3 Pro Preview"),
    ("gemini-3-flash-preview", "Gemini 3 Flash Preview"),
    ("gemini-2.5-pro", "Gemini 2.5 Pro"),
    ("gemini-2.5-flash", "Gemini 2.5 Flash"),
];

pub struct GeminiAdapter {
    /// Binary name; overridable for tests (stub scripts).
    pub program: String,
}

impl Default for GeminiAdapter {
    fn default() -> Self {
        Self {
            program: "gemini".into(),
        }
    }
}

impl GeminiAdapter {
    /// How the CLI is started as an ACP agent.
    pub fn command(&self) -> AcpCommand {
        AcpCommand::new(ADAPTER_ID, &self.program, PROTOCOL_WORDS)
    }
}

#[async_trait]
impl HarnessAdapter for GeminiAdapter {
    fn id(&self) -> &str {
        ADAPTER_ID
    }

    fn display_name(&self) -> &str {
        "Gemini CLI"
    }

    /// As every ACP agent: the ask stops before the tool runs and obeys a
    /// refusal, so the guard can veto; the reply picks an option, so no
    /// input can be rewritten. No effort: the CLI has no such control.
    fn caps(&self) -> HarnessCaps {
        HarnessCaps::MCP_SERVERS
            | HarnessCaps::INPUT_REQUESTS
            | HarnessCaps::RESUME
            | HarnessCaps::TOOL_GUARD
    }

    /// None, for any model: the session offers no level, and is sent none.
    fn efforts(&self, model: Option<&str>) -> Vec<Effort> {
        let _ = model;
        Vec::new()
    }

    /// The page's models, the default first. No plan is recommended: the
    /// setup gate's fix takes the first listed — `auto`, the CLI's own
    /// default — and a preview id written as a recommendation could retire.
    async fn models(&self) -> Vec<ModelInfo> {
        MODELS
            .iter()
            .map(|(id, label)| ModelInfo::new(*id, Some(label), Vec::new()))
            .collect()
    }

    /// The bare TUI. `launch` below runs it as a protocol agent; this is the
    /// command a person types. It has no reporting plan: see the module's
    /// words on the terminal.
    fn interactive(&self) -> Option<InteractiveLaunch> {
        Some(
            InteractiveLaunch::new(&self.program)
                .resumable_with(RESUME_WORDS.iter().map(|w| w.to_string()).collect()),
        )
    }

    /// `gemini` is a common word for a binary: the CLI is there only when it
    /// answers with a version.
    async fn probe(&self) -> ProbeResult {
        util::probe_versioned(&self.program, &["--version"]).await
    }

    async fn launch(&self, spec: SessionSpec) -> Result<Box<dyn HarnessSession>, HarnessError> {
        acp::open(self.command(), &spec, None).await
    }

    async fn attach(&self, token: &ResumeToken) -> Result<Box<dyn HarnessSession>, HarnessError> {
        let spec = acp::revival(ADAPTER_ID, token)?;
        acp::open(self.command(), &spec, Some(token.native_id.clone())).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_harness::ReportingContext;

    #[test]
    fn the_command_is_the_one_protocol_word_and_never_an_approval_word() {
        let adapter = GeminiAdapter::default();
        assert_eq!(
            adapter.command(),
            AcpCommand::new("gemini", "gemini", ["--acp"])
        );
        for word in &adapter.command().args {
            assert!(
                !word.contains("approv") && !word.contains("yolo") && !word.contains("trust"),
                "{word} widens what the CLI may do"
            );
            assert!(
                word != "-m" && !word.contains("model") && !word.contains("effort"),
                "{word}: the model is the session's, and there is no effort"
            );
            assert!(
                !word.contains("experimental"),
                "{word}: the deprecated spelling"
            );
        }
        let stub = GeminiAdapter {
            program: "/tmp/stub/gemini".into(),
        };
        assert_eq!(stub.command().program, "/tmp/stub/gemini");
        assert_eq!(stub.command().adapter_id, "gemini");
    }

    #[tokio::test]
    async fn the_models_are_the_pages_the_default_first_with_no_level_beside_any() {
        let adapter = GeminiAdapter::default();
        let models = adapter.models().await;
        assert_eq!(
            models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            [
                "auto",
                "gemini-3-pro-preview",
                "gemini-3-flash-preview",
                "gemini-2.5-pro",
                "gemini-2.5-flash"
            ]
        );
        assert_eq!(models[0].label.as_deref(), Some("Auto"));
        for model in &models {
            assert!(model.efforts.is_empty(), "{}: no effort control", model.id);
            assert!(adapter.efforts(Some(&model.id)).is_empty());
        }
        assert!(adapter.efforts(None).is_empty());
        assert!(adapter.recommended_plan().is_none());
        assert!(adapter.recommended_judge().is_none());
    }

    #[test]
    fn the_capabilities_are_the_protocols_without_an_effort() {
        let caps = GeminiAdapter::default().caps();
        for held in [
            HarnessCaps::MCP_SERVERS,
            HarnessCaps::INPUT_REQUESTS,
            HarnessCaps::RESUME,
            HarnessCaps::TOOL_GUARD,
        ] {
            assert!(caps.contains(held), "{held:?}");
        }
        for absent in [
            HarnessCaps::EFFORT,
            HarnessCaps::INPUT_REWRITE,
            HarnessCaps::USAGE_REPORTING,
            HarnessCaps::SUBAGENTS,
            HarnessCaps::COST_REPORTING,
        ] {
            assert!(!caps.contains(absent), "{absent:?}");
        }
    }

    #[test]
    fn the_terminal_form_is_the_bare_cli_resumed_with_latest_and_reports_nothing() {
        let adapter = GeminiAdapter {
            program: "gemini-somewhere-else".into(),
        };
        let launch = adapter.interactive().unwrap();
        assert_eq!(launch.program, "gemini-somewhere-else");
        assert!(launch.args.is_empty(), "no protocol word in a terminal");
        assert_eq!(launch.resume_args, ["--resume", "latest"]);
        // The TUI takes no hook for one launch: the plan is the empty one,
        // with a guard on offer or without — a plain terminal, no roster row.
        for guard in [None, Some(vec!["bisa".to_string(), "session".into()])] {
            let plan = adapter.interactive_reporting(&ReportingContext {
                session: "01S".into(),
                reporter: vec!["bisa".into(), "session".into(), "report".into()],
                files_dir: "/ws/run/interactive/01S".into(),
                port: Some(4100),
                guard,
                guard_timeout_secs: 35,
            });
            assert!(plan.is_none(), "{plan:?}");
        }
        let stop = serde_json::json!({ "hook_event_name": "Stop" });
        assert!(adapter.translate_report(&stop).is_empty());
    }

    #[tokio::test]
    async fn a_token_of_another_adapter_is_refused_and_a_missing_binary_is_not_installed() {
        let adapter = GeminiAdapter {
            program: "a-gemini-that-is-not-there".into(),
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
        let probe = adapter.probe().await;
        assert!(!probe.available);
        assert_eq!(
            probe.reason.as_deref(),
            Some("a-gemini-that-is-not-there not found on PATH")
        );
    }
}
