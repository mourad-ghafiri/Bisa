//! Muse Code adapter — `muse serve`, the Muse Session Protocol host, one
//! process per session.
//!
//! The protocol is `msp.rs`'s; this module is what only Muse Code knows.
//! Read on 2026-10-19 from the manual (<https://dev.meta.ai/docs/muse-code>
//! and its pages on configuration, extending and automating, permissions,
//! the interactive session, subscriptions, authentication and the
//! changelog), the installer (<https://dev.meta.ai/install.sh>, read as
//! text), the SDK's documentation
//! (<https://meta-models.github.io/muse-code-sdk/1.4/>) and schema
//! (<https://github.com/meta-models/muse-code-sdk>, `schema/msp/`):
//!
//! - **The binary.** `muse`, put in `~/.local/bin` by the installer (a
//!   folder the desktop's login PATH already knows); `muse --version`
//!   checks it landed. `muse` is a common word for a binary, so it is there
//!   only when its first line carries a version.
//! - **The host.** `muse serve` speaks the protocol over stdio. It is
//!   started with `--trust-workspace`: a workstream's checkout is a fresh
//!   folder every time, and a session on an untrusted root is refused —
//!   trust is what loads the checkout's `AGENTS.md`, `CLAUDE.md` and
//!   skills, which every other harness reads, and it is the platform's to
//!   give, as the guard is. Nothing else: the sandbox stays on and the
//!   approvals stay on (`promptUnmatched`, set on the session — the
//!   strictest mode that still asks); never `--yolo`, `--disable-approval`,
//!   `--disable-sandbox`, `--disable-write`, `--disable-shell`,
//!   `--sandbox-network`, `--no-session-log` (ephemeral: no resume),
//!   `--provider` or `--model`. A host asked a question and nothing else
//!   (`model/list`, `usage/read`) opens no session, and is given no trust.
//! - **Model and effort** reach the session the protocol's way: the model
//!   on `session/start`, checked against the host's own `model/list` first;
//!   the effort on every `turn/start`, held to the levels the model's row
//!   lists — `minimal`, `low`, `medium`, `high`, `xhigh`, `max` are the
//!   platform's words too; `none` is refused by the Meta provider and
//!   `ultra` is the provider's highest, and neither is ever sent.
//! - **Models** are what the host answers `model/list`: the catalog of the
//!   sign-in, the default first — `muse-spark-1.2` by default, the manual
//!   says, with `muse-spark-1.3`, the `-contributor` variants and Muse
//!   Wonder on the higher plans. It recommends no plan: the catalog is the
//!   account's.
//! - **Permission.** The host asks (`approval/request`) before a shell stage
//!   or a tool its policy does not already answer; the guard judges what is
//!   asked and the answer is the *once* choice, never a standing grant. An
//!   in-workspace file edit the sandbox allows is not asked about: the
//!   guard judges commands and what the host asks, never every write.
//! - **The terminal** is the bare `muse`, and `muse resume --last` reopens
//!   the most recent session in the folder. It reports nothing: hooks live
//!   in `~/.config/muse/settings.json` or a project's `.muse/hooks.json`,
//!   neither of which the platform writes, and the CLI takes none for one
//!   launch. So the reporting plan is the empty one: a plain terminal,
//!   never a roster row, and not guarded there.
//! - **Account usage** is the subscription's windows — the one the host
//!   names by its length (five hours, the manual says) and the week — with
//!   the plan's tier: what a live session hears (`usage/changed`), else what
//!   a host answers `usage/read`. An API-key sign-in has none.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bisa_core::sync::Locked;
use bisa_core::HarnessCaps;
use bisa_harness::usage::now_secs;
use bisa_harness::{
    Effort, HarnessAdapter, HarnessError, HarnessSession, InteractiveLaunch, ModelInfo,
    ProbeResult, ResumeToken, SessionSpec, UsageState,
};
use serde_json::json;

use crate::msp::{self, ModelRow, MspCommand, ObservedUsage};
use crate::util;

pub const ADAPTER_ID: &str = "muse-code";

/// The binary's name.
pub const PROGRAM: &str = "muse";

/// The words that make the CLI a host of a session in its folder, trusted
/// there, and nothing else: no model, no effort, and never a word that
/// widens what the host may do.
const HOST_WORDS: [&str; 2] = ["serve", "--trust-workspace"];

/// The words for a host asked a question and nothing else: no session, so
/// no trust.
const QUERY_WORDS: [&str; 1] = ["serve"];

/// How the terminal reopens the folder's most recent session.
const RESUME_WORDS: [&str; 2] = ["resume", "--last"];

/// How long a host asked a question gets to answer it.
pub const HOST_BUDGET: Duration = Duration::from_secs(8);

/// How long a usage a live session heard answers the footer before a host
/// is asked again.
const OBSERVED_FOR: u64 = 600;

pub struct MuseCodeAdapter {
    /// Binary name; overridable for tests (stub scripts).
    pub program: String,
    /// The rows the last `model/list` answered, read by `efforts(model)`,
    /// which spawns nothing.
    rows: std::sync::Mutex<Vec<ModelRow>>,
    /// The account usage the last live session heard.
    observed: ObservedUsage,
}

impl Default for MuseCodeAdapter {
    fn default() -> Self {
        Self {
            program: PROGRAM.into(),
            rows: std::sync::Mutex::new(Vec::new()),
            observed: Arc::default(),
        }
    }
}

impl MuseCodeAdapter {
    /// The adapter over a binary of another name — a stub, in a test.
    pub fn with_program(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            ..Default::default()
        }
    }

    /// How the CLI is started as the host of a session.
    pub fn command(&self) -> MspCommand {
        MspCommand::new(ADAPTER_ID, &self.program, HOST_WORDS)
    }
}

#[async_trait]
impl HarnessAdapter for MuseCodeAdapter {
    fn id(&self) -> &str {
        ADAPTER_ID
    }

    fn display_name(&self) -> &str {
        "Muse Code"
    }

    /// The host asks before a shell stage or a tool its policy does not
    /// answer, and obeys a refusal, so the guard can veto; a choice is
    /// selected, never an input handed back, so no rewrite. A steer joins
    /// the running turn and a follow-up waits for its end; an image rides a
    /// turn as a part of its own; a sub-agent is an item the host names;
    /// token usage and the account's windows are the host's own.
    fn caps(&self) -> HarnessCaps {
        HarnessCaps::MCP_SERVERS
            | HarnessCaps::INPUT_REQUESTS
            | HarnessCaps::RESUME
            | HarnessCaps::TOOL_GUARD
            | HarnessCaps::EFFORT
            | HarnessCaps::STEER
            | HarnessCaps::FOLLOW_UP
            | HarnessCaps::IMAGE_INPUT
            | HarnessCaps::COST_REPORTING
            | HarnessCaps::SUBAGENTS
            | HarnessCaps::USAGE_REPORTING
    }

    /// The levels a model's row lists, once the host has listed it; every
    /// level until then, and for a model nobody lists.
    fn efforts(&self, model: Option<&str>) -> Vec<Effort> {
        msp::levels_of(&self.rows.locked(), model)
    }

    /// What the host answers `model/list`, the default first; nothing when
    /// the host cannot be asked — an unknown list is never "no models".
    async fn models(&self) -> Vec<ModelInfo> {
        match msp::ask(
            &self.program,
            &QUERY_WORDS,
            "model/list",
            json!({}),
            HOST_BUDGET,
        )
        .await
        {
            Ok(result) => {
                let rows = msp::rows_of(&result);
                let listed = msp::listed(&rows);
                *self.rows.locked() = rows;
                listed
            }
            Err(refusal) => {
                tracing::debug!(target: "bisa_adapters::muse_code", "no model list: {refusal}");
                Vec::new()
            }
        }
    }

    /// The subscription's windows: what a live session heard lately, else
    /// what a host answers.
    async fn usage(&self) -> UsageState {
        let heard = self.observed.locked().clone();
        if let Some((payload, at)) = heard {
            if now_secs().saturating_sub(at) <= OBSERVED_FOR {
                return match crate::usage::muse_code::parse(&payload, ADAPTER_ID, at) {
                    Ok(report) => UsageState::Report { report },
                    Err(state) => state,
                };
            }
        }
        crate::usage::muse_code::read(&self.program, &QUERY_WORDS, ADAPTER_ID).await
    }

    /// The bare CLI, resumed with `resume --last`; the reporting plan stays
    /// the empty one — the CLI takes no hook for one launch.
    fn interactive(&self) -> Option<InteractiveLaunch> {
        Some(
            InteractiveLaunch::new(&self.program)
                .resumable_with(RESUME_WORDS.iter().map(|w| w.to_string()).collect()),
        )
    }

    async fn probe(&self) -> ProbeResult {
        util::probe_versioned(&self.program, &["--version"]).await
    }

    async fn launch(&self, spec: SessionSpec) -> Result<Box<dyn HarnessSession>, HarnessError> {
        msp::open(
            self.command(),
            &spec,
            None,
            Some(Arc::clone(&self.observed)),
        )
        .await
    }

    async fn attach(&self, token: &ResumeToken) -> Result<Box<dyn HarnessSession>, HarnessError> {
        let spec = msp::revival(ADAPTER_ID, token)?;
        msp::open(
            self.command(),
            &spec,
            Some(token.native_id.clone()),
            Some(Arc::clone(&self.observed)),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_harness::ReportingContext;

    #[test]
    fn the_command_is_the_trusted_host_and_never_a_word_that_widens_it() {
        let adapter = MuseCodeAdapter::default();
        assert_eq!(adapter.program, "muse");
        assert_eq!(
            adapter.command(),
            MspCommand::new("muse-code", "muse", ["serve", "--trust-workspace"])
        );
        for word in adapter.command().args {
            for never in [
                "yolo",
                "approval",
                "sandbox",
                "disable",
                "model",
                "effort",
                "session-log",
                "provider",
            ] {
                assert!(!word.contains(never), "`{word}` widens the host");
            }
        }
        assert_eq!(
            QUERY_WORDS,
            ["serve"],
            "a question opens no session: no trust"
        );
    }

    #[test]
    fn the_levels_are_the_six_until_the_host_lists_a_models_variants() {
        let adapter = MuseCodeAdapter::default();
        assert!(adapter.caps().contains(HarnessCaps::EFFORT));
        for model in [Some("muse-spark-1.2"), Some("anything"), None] {
            assert_eq!(adapter.efforts(model), Effort::ALL, "{model:?}");
        }
        *adapter.rows.locked() = msp::rows_of(&json!({ "models": [
            { "modelId": "muse-spark-1.2", "isDefault": true, "variants": ["low", "high", "ultra"] },
            { "modelId": "muse-wonder-1.0", "variants": ["medium", "max"] },
        ]}));
        assert_eq!(
            adapter.efforts(Some("muse-spark-1.2")),
            [Effort::Low, Effort::High]
        );
        assert_eq!(
            adapter.efforts(None),
            [Effort::Low, Effort::High],
            "the default row"
        );
        assert_eq!(
            adapter.efforts(Some("muse-wonder-1.0")),
            [Effort::Medium, Effort::Max]
        );
        assert_eq!(adapter.efforts(Some("retired")), Effort::ALL);
    }

    #[test]
    fn the_capabilities_hold_the_guard_steering_usage_and_cost_and_no_rewrite_and_no_plan_is_recommended(
    ) {
        let adapter = MuseCodeAdapter::default();
        assert_eq!(adapter.id(), "muse-code");
        assert_eq!(adapter.display_name(), "Muse Code");
        let caps = adapter.caps();
        for held in [
            HarnessCaps::MCP_SERVERS,
            HarnessCaps::INPUT_REQUESTS,
            HarnessCaps::RESUME,
            HarnessCaps::TOOL_GUARD,
            HarnessCaps::EFFORT,
            HarnessCaps::STEER,
            HarnessCaps::FOLLOW_UP,
            HarnessCaps::IMAGE_INPUT,
            HarnessCaps::COST_REPORTING,
            HarnessCaps::SUBAGENTS,
            HarnessCaps::USAGE_REPORTING,
        ] {
            assert!(caps.contains(held), "{held:?}");
        }
        for absent in [
            HarnessCaps::INPUT_REWRITE,
            HarnessCaps::FORK,
            HarnessCaps::STRUCTURED_OUTPUT,
        ] {
            assert!(!caps.contains(absent), "{absent:?}");
        }
        assert!(adapter.recommended_plan().is_none());
        assert!(adapter.recommended_judge().is_none());
    }

    #[test]
    fn the_terminal_form_is_the_bare_cli_resumed_with_resume_last_and_reports_nothing() {
        let adapter = MuseCodeAdapter {
            program: "muse-somewhere-else".into(),
            ..Default::default()
        };
        let launch = adapter.interactive().unwrap();
        assert_eq!(launch.program, "muse-somewhere-else");
        assert!(launch.args.is_empty(), "no protocol word in a terminal");
        assert_eq!(launch.resume_args, ["resume", "--last"]);
        // The CLI takes no hook for one launch: the plan is the empty one,
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
        let stop = json!({ "hook_event_name": "Stop" });
        assert!(adapter.translate_report(&stop).is_empty());
    }

    #[tokio::test]
    async fn a_token_of_another_adapter_is_refused_and_a_missing_binary_lists_nothing_and_reports_a_failed_read(
    ) {
        let adapter = MuseCodeAdapter {
            program: "a-muse-that-is-not-there".into(),
            ..Default::default()
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
        assert!(adapter.models().await.is_empty());
        let probe = adapter.probe().await;
        assert!(!probe.available);
        assert_eq!(
            probe.reason.as_deref(),
            Some("a-muse-that-is-not-there not found on PATH")
        );
        match adapter.usage().await {
            UsageState::Failed { reason } => {
                assert!(reason.contains("could not be started"), "{reason}")
            }
            other => panic!("{other:?}"),
        }
        // A usage a session heard answers without a host — until it is old.
        *adapter.observed.locked() = Some((
            json!({ "usage": { "tier": "power", "window": { "usedPercent": 40, "windowDurationMins": 300, "resetsAtMs": 1_788_782_400_000u64 }, "weekly": { "usedPercent": 5, "resetsAtMs": 1_789_128_000_000u64 } } }),
            now_secs(),
        ));
        match adapter.usage().await {
            UsageState::Report { report } => {
                assert_eq!(report.harness, "muse-code");
                assert_eq!(
                    report.account.and_then(|a| a.plan).as_deref(),
                    Some("power")
                );
                assert_eq!(report.windows.len(), 2);
            }
            other => panic!("{other:?}"),
        }
        *adapter.observed.locked() = Some((
            json!({ "usage": { "tier": "power" } }),
            now_secs() - OBSERVED_FOR - 1,
        ));
        assert!(
            matches!(adapter.usage().await, UsageState::Failed { .. }),
            "too old: the host is asked, and is not there"
        );
        // A launch on a missing binary is refused as one.
        let spec = SessionSpec {
            work_item: None,
            cwd: "/work/here".into(),
            prompt: "hello".into(),
            model: None,
            effort: None,
            mcp_servers: vec![],
            env: Default::default(),
            env_remove: Vec::new(),
            tier_ceiling: bisa_core::ToolTier::Exec,
            output_schema: None,
            skills: vec![],
        };
        assert!(matches!(
            adapter.launch(spec).await,
            Err(HarnessError::Unavailable(_))
        ));
    }
}
