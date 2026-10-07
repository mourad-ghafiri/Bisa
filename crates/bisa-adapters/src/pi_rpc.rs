//! pi adapter — `pi --mode rpc`, strict JSONL over stdio (LF only).
//!
//! Reference client: pi's `packages/coding-agent/src/modes/rpc/rpc-client.ts`.
//! Resume: relaunch with `--session <path|id>`; the native id and session
//! file are learned via a `get_state` request at startup.
//!
//! Effort: `--thinking <level>`, on a launch and on a resume alike — pi's own
//! name for it, "clamped to the model's capabilities"
//! (https://github.com/badlogic/pi-mono,
//! `packages/coding-agent/docs/cli.md`, read 2026-09-29). pi takes all six
//! levels for every model and holds them itself.

use async_trait::async_trait;
use bisa_core::HarnessCaps;
use bisa_harness::proc::{Line, ProcHandle, ProcSpec};
use bisa_harness::{
    BoxEventStream, Effort, HarnessAdapter, HarnessError, HarnessSession, InteractiveLaunch,
    LifecycleEvent, Phase, ProbeResult, ProgressEvent, PromptInput, ResumeToken, SessionEvent,
    SessionSnapshot, SessionSpec, Steer,
};
use tokio::sync::mpsc;

use crate::pi_wire::{self, STATE_REQ_ID};
use crate::util::{self, OutMsg, Shared};

pub const ADAPTER_ID: &str = "pi";

pub struct PiRpcAdapter {
    pub program: String,
    /// Extra args before `--mode rpc` (tests inject stub behavior here).
    pub extra_args: Vec<String>,
}

impl Default for PiRpcAdapter {
    fn default() -> Self {
        Self {
            program: "pi".into(),
            extra_args: vec![],
        }
    }
}

/// The session's arguments, in order: the caller's own, `--mode rpc`, the
/// model, `--thinking <level>`, the session to continue.
fn argv(extra_args: &[String], spec: &SessionSpec, session: Option<&str>) -> Vec<String> {
    let mut args: Vec<String> = extra_args.to_vec();
    args.extend(["--mode".to_string(), "rpc".to_string()]);
    if let Some(model) = &spec.model {
        // pi takes provider + model separately; accept "provider/model".
        if let Some((provider, model)) = model.split_once('/') {
            args.extend(["--provider".to_string(), provider.to_string()]);
            args.extend(["--model".to_string(), model.to_string()]);
        } else {
            args.extend(["--model".to_string(), model.clone()]);
        }
    }
    if let Some(level) = spec.effort {
        args.extend(["--thinking".to_string(), level.as_str().to_string()]);
    }
    if let Some(session) = session {
        args.extend(["--session".to_string(), session.to_string()]);
    }
    args
}

/// The spec a session is revived with: where it ran, and the model and the
/// effort it ran with.
fn revival_spec(token: &ResumeToken, cwd: std::path::PathBuf) -> SessionSpec {
    SessionSpec {
        work_item: None,
        cwd,
        prompt: String::new(),
        model: token.model.clone(),
        effort: token.effort,
        mcp_servers: vec![],
        env: Default::default(),
        env_remove: Vec::new(),
        tier_ceiling: bisa_core::ToolTier::Write,
        output_schema: None,
        skills: vec![],
    }
}

impl PiRpcAdapter {
    fn caps_() -> HarnessCaps {
        HarnessCaps::STEER
            | HarnessCaps::FOLLOW_UP
            | HarnessCaps::RESUME
            | HarnessCaps::FORK
            | HarnessCaps::EFFORT
    }

    async fn spawn_session(
        &self,
        spec: &SessionSpec,
        session_arg: Option<&str>,
        revived: bool,
    ) -> Result<Box<dyn HarnessSession>, HarnessError> {
        let mut proc_spec = ProcSpec::new(&self.program).cwd(&spec.cwd).args(argv(
            &self.extra_args,
            spec,
            session_arg,
        ));
        for (k, v) in &spec.env {
            proc_spec = proc_spec.env(k, v);
        }
        for k in &spec.env_remove {
            proc_spec = proc_spec.env_remove(k);
        }

        let proc = ProcHandle::spawn(proc_spec)?;
        let (out_tx, out_rx) = mpsc::channel(64);
        let shared =
            Shared::new(out_tx, ADAPTER_ID, spec.model.clone(), &spec.cwd).at_effort(spec.effort);
        shared.broadcaster.emit(SessionEvent::Lifecycle(if revived {
            LifecycleEvent::Revived
        } else {
            LifecycleEvent::Started
        }));
        shared.set_phase(Phase::Idle);

        let shared = shared.in_group(proc.group());
        let driver_shared = shared.for_driver();
        tokio::spawn(util::drive(
            proc,
            out_rx,
            driver_shared,
            move |shared, line| match line {
                Line::Json(v) => pi_wire::map_pi_event(shared, v),
                Line::Text(t) => {
                    tracing::debug!(target: "bisa_adapters::pi", "non-json stdout: {t}");
                    util::Drive::Continue
                }
            },
        ));

        // Learn native session id/file.
        shared
            .send(OutMsg::Json(pi_wire::command(
                Some(STATE_REQ_ID),
                "get_state",
                None,
            )))
            .await?;

        let session = PiRpcSession { shared };
        if !spec.prompt.is_empty() {
            session.send_prompt(&spec.prompt).await?;
        }
        Ok(Box::new(session))
    }
}

#[async_trait]
impl HarnessAdapter for PiRpcAdapter {
    fn id(&self) -> &str {
        ADAPTER_ID
    }

    fn display_name(&self) -> &str {
        "pi"
    }

    /// pi's RPC and CLI expose a session's tokens and cost, never the
    /// provider's windows: the honest answer, in pi's name.
    async fn usage(&self) -> bisa_harness::UsageState {
        bisa_harness::UsageState::Unsupported {
            reason: "pi reports no usage limits — its session stats count tokens; the windows are the provider's.".to_string(),
        }
    }

    fn caps(&self) -> HarnessCaps {
        Self::caps_()
    }

    fn efforts(&self, model: Option<&str>) -> Vec<Effort> {
        let _ = model;
        Effort::ALL.to_vec()
    }

    /// The bare CLI. `launch` above runs it in `--mode rpc`; this is the command a
    /// person types. `extra_args` comes along because it is how a caller points
    /// this adapter at a wrapper or a stub, and an interactive session that
    /// ignored it would be a different program from the one being probed.
    /// `--continue` is pi's own "Continue most recent session" (`-c`), scoped
    /// to the directory it starts in; `--resume` is its picker.
    fn interactive(&self) -> Option<InteractiveLaunch> {
        Some(
            InteractiveLaunch::with_args(&self.program, self.extra_args.clone())
                .resumable_with(vec!["--continue".into()]),
        )
    }

    fn interactive_reporting(
        &self,
        ctx: &bisa_harness::ReportingContext,
    ) -> bisa_harness::ReportingPlan {
        crate::hooks::pi_like::reporting(ctx)
    }

    fn translate_report(&self, payload: &serde_json::Value) -> Vec<SessionEvent> {
        crate::hooks::pi_like::translate(payload)
    }

    async fn probe(&self) -> ProbeResult {
        util::probe_binary(&self.program, &["--version"]).await
    }

    async fn launch(&self, spec: SessionSpec) -> Result<Box<dyn HarnessSession>, HarnessError> {
        self.spawn_session(&spec, None, false).await
    }

    async fn attach(&self, token: &ResumeToken) -> Result<Box<dyn HarnessSession>, HarnessError> {
        if token.adapter_id != ADAPTER_ID {
            return Err(HarnessError::protocol(format!(
                "resume token for {:?} handed to {ADAPTER_ID}",
                token.adapter_id
            )));
        }
        let session_arg = token
            .transcript_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| token.native_id.clone());
        // No silent fallback to the daemon's working directory: a revived
        // session that lands wherever the node was started is misplaced just
        // like any other, and used to say nothing about it.
        let cwd = token
            .transcript_path
            .as_ref()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .ok_or_else(|| {
                HarnessError::protocol(format!(
                    "resume token for {ADAPTER_ID} names no transcript, so the session's \
                     working directory cannot be resolved"
                ))
            })?;
        let spec = revival_spec(token, cwd);
        self.spawn_session(&spec, Some(&session_arg), true).await
    }
}

pub struct PiRpcSession {
    shared: Shared,
}

impl PiRpcSession {
    async fn send_prompt(&self, text: &str) -> Result<(), HarnessError> {
        self.shared.set_phase(Phase::Turn);
        self.shared
            .broadcaster
            .emit(SessionEvent::Progress(ProgressEvent::TurnStarted));
        self.shared
            .send(OutMsg::Json(pi_wire::command(None, "prompt", Some(text))))
            .await
    }
}

#[async_trait]
impl HarnessSession for PiRpcSession {
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
            _ => self.send_prompt(&input.text).await,
        }
    }

    async fn steer(&self, msg: Steer) -> Result<(), HarnessError> {
        if self.shared.is_ended() {
            return Err(HarnessError::Terminated);
        }
        self.shared
            .send(OutMsg::Json(pi_wire::command(
                None,
                "steer",
                Some(&msg.text),
            )))
            .await
    }

    async fn follow_up(&self, msg: Steer) -> Result<(), HarnessError> {
        if self.shared.is_ended() {
            return Err(HarnessError::Terminated);
        }
        self.shared
            .send(OutMsg::Json(pi_wire::command(
                None,
                "follow_up",
                Some(&msg.text),
            )))
            .await
    }

    async fn abort(&self) -> Result<(), HarnessError> {
        // Told the way it understands first — the turn aborted — then its
        // whole group ended with a grace: the process stays after an abort
        // command, and a stop means it must not.
        if let Err(e) = self
            .shared
            .send(OutMsg::Json(pi_wire::command(None, "abort", None)))
            .await
        {
            tracing::debug!("the driver had already ended: {e}");
        }
        self.shared.end(bisa_harness::Outcome::Aborted);
        self.shared
            .abort_group(bisa_harness::proc::ABORT_GRACE)
            .await;
        Ok(())
    }

    fn subscribe(&self) -> BoxEventStream {
        self.shared.broadcaster.subscribe()
    }

    fn resume_token(&self) -> Option<ResumeToken> {
        self.shared.resume_token(ADAPTER_ID)
    }

    async fn dispose(self: Box<Self>) -> Result<(), HarnessError> {
        self.shared.close_stdin_quietly().await;
        self.shared
            .leave_or_kill(bisa_harness::proc::DISPOSE_GRACE)
            .await;
        Ok(())
    }
}

#[cfg(test)]
mod interactive_tests {
    use super::*;

    #[test]
    fn the_terminal_form_continues_the_most_recent_session_here() {
        let launch = PiRpcAdapter::default().interactive().unwrap();
        assert_eq!(launch.resume_args, vec!["--continue".to_string()]);
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
            tier_ceiling: bisa_core::ToolTier::Write,
            output_schema: None,
            skills: vec![],
        }
    }

    #[test]
    fn the_effort_follows_the_model_in_the_line_and_is_absent_when_none_is_set() {
        // Neither: the line is what it was.
        assert_eq!(argv(&[], &bare(), None), ["--mode", "rpc"]);
        let model_only = SessionSpec {
            model: Some("anthropic/claude-opus-5-5".into()),
            ..bare()
        };
        assert_eq!(
            argv(&[], &model_only, None),
            [
                "--mode",
                "rpc",
                "--provider",
                "anthropic",
                "--model",
                "claude-opus-5-5"
            ]
        );
        // Both, after the caller's own arguments.
        let both = SessionSpec {
            effort: Some(Effort::Xhigh),
            ..model_only
        };
        assert_eq!(
            argv(&["--stub".to_string()], &both, None),
            [
                "--stub",
                "--mode",
                "rpc",
                "--provider",
                "anthropic",
                "--model",
                "claude-opus-5-5",
                "--thinking",
                "xhigh"
            ]
        );
        // An effort alone: pi's own default model, at that level.
        let effort_only = SessionSpec {
            effort: Some(Effort::Minimal),
            ..bare()
        };
        assert_eq!(
            argv(&[], &effort_only, None),
            ["--mode", "rpc", "--thinking", "minimal"]
        );
    }

    #[test]
    fn pi_takes_all_six_levels_for_every_model() {
        let adapter = PiRpcAdapter::default();
        assert!(adapter.caps().contains(HarnessCaps::EFFORT));
        for model in [None, Some("anthropic/claude-opus-5-5"), Some("anything")] {
            assert_eq!(adapter.efforts(model), Effort::ALL, "{model:?}");
        }
        for level in Effort::ALL {
            let spec = SessionSpec {
                effort: Some(level),
                ..bare()
            };
            assert_eq!(
                argv(&[], &spec, None),
                ["--mode", "rpc", "--thinking", level.as_str()]
            );
        }
    }

    #[test]
    fn a_revived_session_is_given_the_model_and_the_effort_it_ran_with() {
        let token = ResumeToken {
            adapter_id: ADAPTER_ID.to_string(),
            native_id: "sess-1".into(),
            cwd: std::path::PathBuf::from("/work/here"),
            transcript_path: Some(std::path::PathBuf::from("/work/here/sess-1.jsonl")),
            model: Some("claude-sonnet-5-5".into()),
            effort: Some(Effort::High),
        };
        let spec = revival_spec(&token, std::path::PathBuf::from("/work/here"));
        assert_eq!(spec.model, token.model);
        assert_eq!(spec.effort, token.effort);
        assert_eq!(
            argv(&[], &spec, Some("/work/here/sess-1.jsonl")),
            [
                "--mode",
                "rpc",
                "--model",
                "claude-sonnet-5-5",
                "--thinking",
                "high",
                "--session",
                "/work/here/sess-1.jsonl"
            ]
        );
        // A session that ran with neither is revived with neither.
        let plain = ResumeToken {
            model: None,
            effort: None,
            ..token
        };
        let spec = revival_spec(&plain, std::path::PathBuf::from("/work/here"));
        assert_eq!(
            argv(&[], &spec, Some("sess-1")),
            ["--mode", "rpc", "--session", "sess-1"]
        );
    }

    #[test]
    fn a_sessions_token_says_the_model_and_the_effort_it_was_launched_with() {
        let (out_tx, _out_rx) = mpsc::channel(8);
        let shared = Shared::new(out_tx, ADAPTER_ID, Some("claude-opus-5-5".into()), "/work")
            .at_effort(Some(Effort::Max));
        shared.set_native_id("sess-1");
        let token = PiRpcSession { shared }.resume_token().unwrap();
        assert_eq!(token.adapter_id, ADAPTER_ID);
        assert_eq!(token.model.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(token.effort, Some(Effort::Max));
    }
}
