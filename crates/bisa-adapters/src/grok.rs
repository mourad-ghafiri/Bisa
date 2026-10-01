//! Grok Build adapter — `grok agent stdio`, the CLI's Agent Client Protocol
//! agent over stdio, one process per session.
//!
//! The protocol is `acp.rs`'s; this module is what only Grok Build knows.
//! Read on 2026-09-30 from the xai-org/grok-build repository — its user
//! guide (`crates/codegen/xai-grok-pager/docs/user-guide/`: 15-agent-mode,
//! 22-permissions-and-safety, 09-plugins, 10-hooks, 14-headless-mode) and
//! its command line's own definition (`.../src/app/cli.rs`, `models.rs`) —
//! and https://docs.x.ai/build/overview:
//!
//! - **The command.** `grok agent stdio` "runs Grok as an ACP agent over
//!   JSON-RPC on stdin/stdout"; agent options go "after `agent` and before
//!   the mode name". `--no-leader` "start[s] a new agent even when config
//!   enables leader mode": a session is a process of its own, whatever the
//!   person's `[cli] use_leader` says.
//! - **Model and effort** are session config options — `model` (category
//!   `model`) and `reasoning_effort` (category `thought_level`: `minimal`,
//!   `low`, `medium`, `high`, `xhigh`; "dropped with a warning when the
//!   model does not advertise `supportsReasoningEffort`") — so both are set
//!   the protocol's way by `acp::open` and the command carries neither word.
//! - **Permission.** The agent asks (`session/request_permission`) "when the
//!   session is not always-approve". `--always-approve` is never passed. A
//!   person's own `permission_mode` still answers before the agent asks —
//!   the guard judges what is asked.
//! - **Models** are what `grok models` prints: a sign-in line, the default,
//!   then one row a model — `  * <id> (default)` or `  - <id>`.
//! - **The terminal** is the bare `grok`, and `--continue` "continue[s] the
//!   most recent session for the current working directory". It reports
//!   nothing: the TUI takes no hook and no plugin for one launch
//!   (`--plugin-dir` is an option of `grok agent` alone), and its hooks
//!   otherwise live under `~/.grok` or in the project — neither of which the
//!   platform writes. So the reporting plan is the empty one: a plain
//!   terminal, never a roster row, and not guarded there.
//! - **Account usage** has no source: `grok usage <session>` is one
//!   session's tokens. `usage()` is the default, *unsupported*.

use async_trait::async_trait;
use bisa_core::HarnessCaps;
use bisa_harness::{
    Effort, HarnessAdapter, HarnessError, HarnessSession, InteractiveLaunch, ModelInfo,
    ProbeResult, ResumeToken, SessionSpec,
};

use crate::acp::{self, AcpCommand};
use crate::util;

pub const ADAPTER_ID: &str = "grok";

/// The words that make the CLI an ACP agent of its own process, and nothing
/// else: no model, no effort, and never `--always-approve`.
const PROTOCOL_WORDS: [&str; 3] = ["agent", "--no-leader", "stdio"];

/// The levels the session's `reasoning_effort` option takes, lowest first.
const LEVELS: [Effort; 5] = [
    Effort::Minimal,
    Effort::Low,
    Effort::Medium,
    Effort::High,
    Effort::Xhigh,
];

/// The mark of the default among the rows `grok models` prints.
const DEFAULT_ROW: &str = "* ";

/// The mark of every other model's row.
const MODEL_ROW: &str = "- ";

/// The model one row of `grok models` names, and whether it is the default;
/// `None` for the sign-in line, the headings and the blanks around the rows.
fn listed_row(line: &str) -> Option<(&str, bool)> {
    let line = line.trim();
    let (rest, default) = match line.strip_prefix(DEFAULT_ROW) {
        Some(rest) => (rest, true),
        None => (line.strip_prefix(MODEL_ROW)?, false),
    };
    let id = rest.split_whitespace().next()?;
    Some((id, default))
}

/// The models `grok models` printed, the default first, each once, with the
/// levels a session may be asked for.
fn listed(output: &str) -> Vec<ModelInfo> {
    let rows: Vec<(&str, bool)> = output.lines().filter_map(listed_row).collect();
    let mut seen = std::collections::BTreeSet::new();
    rows.iter()
        .filter(|(_, default)| *default)
        .chain(rows.iter().filter(|(_, default)| !*default))
        .filter(|(id, _)| seen.insert(*id))
        .map(|(id, _)| ModelInfo::new(*id, None, LEVELS.to_vec()))
        .collect()
}

pub struct GrokAdapter {
    /// Binary name; overridable for tests (stub scripts).
    pub program: String,
}

impl Default for GrokAdapter {
    fn default() -> Self {
        Self {
            program: "grok".into(),
        }
    }
}

impl GrokAdapter {
    /// How the CLI is started as an ACP agent.
    pub fn command(&self) -> AcpCommand {
        AcpCommand::new(ADAPTER_ID, &self.program, PROTOCOL_WORDS)
    }
}

#[async_trait]
impl HarnessAdapter for GrokAdapter {
    fn id(&self) -> &str {
        ADAPTER_ID
    }

    fn display_name(&self) -> &str {
        "Grok Build"
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

    /// The option's five levels for every model: which of them a model
    /// takes, it says only in a session, and the level is held to those
    /// there.
    fn efforts(&self, model: Option<&str>) -> Vec<Effort> {
        let _ = model;
        LEVELS.to_vec()
    }

    /// What `grok models` lists for this sign-in, the default first. No
    /// model id is written here — one that retires would be recommended for
    /// ever — so there is no recommended plan either: the setup gate's fix
    /// takes the models listed first.
    async fn models(&self) -> Vec<ModelInfo> {
        util::command_output(&self.program, &["models"])
            .await
            .map(|output| listed(&output))
            .unwrap_or_default()
    }

    /// The bare TUI. `launch` below runs it as a protocol agent; this is the
    /// command a person types. It has no reporting plan: see the module's
    /// words on the terminal.
    fn interactive(&self) -> Option<InteractiveLaunch> {
        Some(InteractiveLaunch::new(&self.program).resumable_with(vec!["--continue".into()]))
    }

    /// `grok` is a common word for a binary: the CLI is there only when it
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
    fn the_command_is_an_agent_of_its_own_process_and_never_always_approves() {
        let adapter = GrokAdapter::default();
        assert_eq!(
            adapter.command(),
            AcpCommand::new("grok", "grok", ["agent", "--no-leader", "stdio"])
        );
        let words = adapter.command().args;
        // The mode is the last word: agent options go before it.
        assert_eq!(words.first().map(String::as_str), Some("agent"));
        assert_eq!(words.last().map(String::as_str), Some("stdio"));
        for word in &words {
            assert!(
                !word.contains("approve") && !word.contains("yolo"),
                "{word} widens what the CLI may do"
            );
            assert!(
                word != "-m" && !word.contains("model") && !word.contains("effort"),
                "{word}: the model and the effort are the session's"
            );
        }
        let stub = GrokAdapter {
            program: "/tmp/stub/grok".into(),
        };
        assert_eq!(stub.command().program, "/tmp/stub/grok");
        assert_eq!(stub.command().adapter_id, "grok");
    }

    #[test]
    fn the_levels_are_the_options_five_for_every_model() {
        let adapter = GrokAdapter::default();
        assert!(adapter.caps().contains(HarnessCaps::EFFORT));
        for model in [Some("grok-4.7"), Some("anything"), None] {
            assert_eq!(
                adapter.efforts(model),
                [
                    Effort::Minimal,
                    Effort::Low,
                    Effort::Medium,
                    Effort::High,
                    Effort::Xhigh
                ],
                "{model:?}"
            );
        }
        assert!(!adapter.efforts(None).contains(&Effort::Max));
    }

    #[test]
    fn the_models_are_the_rows_the_cli_prints_the_default_first() {
        let printed = "\
You are logged in with grok.com.

Default model: grok-b

Available models:
  - grok-a
  * grok-b (default)
  - grok-c
";
        let models = listed(printed);
        assert_eq!(
            models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            ["grok-b", "grok-a", "grok-c"]
        );
        for model in &models {
            assert_eq!(model.efforts, LEVELS);
            assert!(model.label.is_none());
        }
        // A row is read as a row and nothing else is.
        assert_eq!(listed_row("  * grok-b (default)"), Some(("grok-b", true)));
        assert_eq!(listed_row("  - grok-a"), Some(("grok-a", false)));
        assert_eq!(listed_row("- grok-a   "), Some(("grok-a", false)));
        for line in [
            "",
            "   ",
            "You are not authenticated.",
            "You are using XAI_API_KEY.",
            "Default model: grok-b",
            "Available models:",
            "-",
            "*",
            "-grok-a",
            "grok-a",
        ] {
            assert_eq!(listed_row(line), None, "{line:?}");
        }
        // A model printed twice is listed once; nothing printed, nothing
        // listed — an unknown list is never "no models".
        assert_eq!(listed("  - a\n  - a\n  * a (default)\n").len(), 1);
        assert!(listed("You are not authenticated.\n\n").is_empty());
        assert!(listed("").is_empty());
    }

    #[test]
    fn the_capabilities_are_the_protocols_and_no_plan_is_recommended() {
        let adapter = GrokAdapter::default();
        let caps = adapter.caps();
        for held in [
            HarnessCaps::MCP_SERVERS,
            HarnessCaps::INPUT_REQUESTS,
            HarnessCaps::RESUME,
            HarnessCaps::TOOL_GUARD,
            HarnessCaps::EFFORT,
        ] {
            assert!(caps.contains(held), "{held:?}");
        }
        for absent in [
            HarnessCaps::INPUT_REWRITE,
            HarnessCaps::USAGE_REPORTING,
            HarnessCaps::SUBAGENTS,
            HarnessCaps::COST_REPORTING,
        ] {
            assert!(!caps.contains(absent), "{absent:?}");
        }
        assert!(adapter.recommended_plan().is_none());
        assert!(adapter.recommended_judge().is_none());
    }

    #[test]
    fn the_terminal_form_is_the_bare_cli_resumed_with_continue_and_reports_nothing() {
        let adapter = GrokAdapter {
            program: "grok-somewhere-else".into(),
        };
        let launch = adapter.interactive().unwrap();
        assert_eq!(launch.program, "grok-somewhere-else");
        assert!(launch.args.is_empty(), "no protocol word in a terminal");
        assert_eq!(launch.resume_args, ["--continue"]);
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
    async fn a_token_of_another_adapter_is_refused_and_a_missing_binary_lists_nothing() {
        let adapter = GrokAdapter {
            program: "a-grok-that-is-not-there".into(),
        };
        let token = ResumeToken {
            adapter_id: "copilot".into(),
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
        assert!(adapter.models().await.is_empty());
        let probe = adapter.probe().await;
        assert!(!probe.available);
        assert_eq!(
            probe.reason.as_deref(),
            Some("a-grok-that-is-not-there not found on PATH")
        );
    }
}
