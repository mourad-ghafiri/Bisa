//! Custom-JSON adapter: wraps an arbitrary user command (tier-3 catalog
//! descriptor) speaking the minimal Bisa NDJSON contract.
//!
//! # Wire contract (documented here, the only place it exists)
//!
//! stdout (one JSON object per line):
//! - `{"type":"started"}` — optional; session counts as started anyway
//! - `{"type":"text","text":"..."}` — assistant text
//! - `{"type":"tool","name":"...","args":{...}}` — tool started
//! - `{"type":"tool_end","name":"...","ok":true}` — tool finished (optional)
//! - `{"type":"result","output":{...}}` — structured result (passed as Raw;
//!   contracted results still go through the MCP `result_submit` tool)
//! - `{"type":"ended","outcome":"completed"|"failed","error":"..."}` — turn end
//!
//! stdin: `{"type":"prompt","text":"..."}` per turn. EOF ends the session.
//! Process exit without an `ended` line maps exit code 0 → completed.

use std::collections::BTreeMap;

use async_trait::async_trait;
use bisa_core::{HarnessCaps, ToolTier};
use bisa_harness::proc::{Line, ProcHandle, ProcSpec};
use bisa_harness::{
    BoxEventStream, CustomHarnessSpec, HarnessAdapter, HarnessError, HarnessSession,
    InteractiveLaunch, LifecycleEvent, Outcome, Phase, ProbeResult, ProgressEvent, PromptInput,
    ResumeToken, SessionEvent, SessionSnapshot, SessionSpec, Steer,
};
use tokio::sync::mpsc;

use crate::util::{self, Drive, OutMsg, Shared};

pub struct CustomJsonAdapter {
    pub spec: CustomHarnessSpec,
    full_id: String,
}

impl CustomJsonAdapter {
    pub fn new(spec: CustomHarnessSpec) -> Self {
        let full_id = format!("custom:{}", spec.id);
        Self { spec, full_id }
    }

    fn full_id(&self) -> String {
        self.full_id.clone()
    }
}

#[async_trait]
impl HarnessAdapter for CustomJsonAdapter {
    fn id(&self) -> &str {
        &self.full_id
    }

    fn display_name(&self) -> &str {
        &self.spec.label
    }

    fn caps(&self) -> HarnessCaps {
        HarnessCaps::FOLLOW_UP
    }

    /// A custom descriptor's command *is* its interactive form. This catalog
    /// has never had an opinion about what that binary does, and it does not
    /// acquire one here — the descriptor is the user's own.
    fn interactive(&self) -> Option<InteractiveLaunch> {
        Some(
            InteractiveLaunch::with_args(&self.spec.command, self.spec.args.clone())
                .resumable_with(self.spec.resume_args.clone()),
        )
    }

    async fn probe(&self) -> ProbeResult {
        util::probe_binary(&self.spec.command, &[]).await
    }

    async fn launch(&self, spec: SessionSpec) -> Result<Box<dyn HarnessSession>, HarnessError> {
        let mut proc_spec = ProcSpec::new(&self.spec.command).cwd(&spec.cwd);
        for arg in &self.spec.args {
            proc_spec = proc_spec.arg(arg);
        }
        let mut env: BTreeMap<String, String> = self.spec.env.clone();
        env.extend(spec.env.clone());
        for (k, v) in env {
            proc_spec = proc_spec.env(k, v);
        }
        for k in &spec.env_remove {
            proc_spec = proc_spec.env_remove(k);
        }

        let proc = ProcHandle::spawn(proc_spec)?;
        let (out_tx, out_rx) = mpsc::channel(64);
        let shared = Shared::new(out_tx, self.full_id(), spec.model.clone(), &spec.cwd);
        shared
            .broadcaster
            .emit(SessionEvent::Lifecycle(LifecycleEvent::Started));
        shared.set_phase(Phase::Idle);

        let driver_shared = shared.clone();
        tokio::spawn(util::drive(
            proc,
            out_rx,
            driver_shared,
            move |shared, line| {
                let value = match line {
                    Line::Json(v) => v,
                    Line::Text(t) => {
                        tracing::debug!(target: "bisa_adapters::custom", "non-json stdout: {t}");
                        return Drive::Continue;
                    }
                };
                match value.get("type").and_then(|t| t.as_str()) {
                    Some("started") => {}
                    Some("text") => {
                        if let Some(text) = value.get("text").and_then(|v| v.as_str()) {
                            shared.broadcaster.emit(SessionEvent::Progress(
                                ProgressEvent::TextDelta {
                                    text: text.to_string(),
                                },
                            ));
                        }
                    }
                    Some("tool") => {
                        let name = value
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("?")
                            .to_string();
                        let args = value
                            .get("args")
                            .cloned()
                            .unwrap_or(serde_json::Value::Null);
                        shared.broadcaster.emit(SessionEvent::Progress(
                            ProgressEvent::ToolStarted {
                                tier: ToolTier::classify(&name),
                                args_summary: util::summarize_args(&args, 160),
                                name,
                            },
                        ));
                    }
                    Some("tool_end") => {
                        let name = value
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("?")
                            .to_string();
                        let ok = value.get("ok").and_then(|v| v.as_bool()).unwrap_or(true);
                        shared
                            .broadcaster
                            .emit(SessionEvent::Progress(ProgressEvent::ToolEnded {
                                name,
                                ok,
                            }));
                    }
                    Some("ended") => {
                        // Mid-run site: the contract's own error channel. A
                        // custom command has no vocabulary of its own, so only
                        // provider wording is keyed on here.
                        let outcome = match value.get("outcome").and_then(|v| v.as_str()) {
                            Some("completed") | None => Outcome::Completed,
                            Some(other) => {
                                let error = value
                                    .get("error")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(other)
                                    .to_string();
                                shared
                                    .model_ctx
                                    .outcome(&error)
                                    .unwrap_or(Outcome::Failed { error })
                            }
                        };
                        shared
                            .broadcaster
                            .emit(SessionEvent::Progress(ProgressEvent::TurnEnded));
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
            },
        ));

        let session = CustomJsonSession { shared };
        if !spec.prompt.is_empty() {
            session.send_prompt(&spec.prompt).await?;
        }
        Ok(Box::new(session))
    }

    async fn attach(&self, _token: &ResumeToken) -> Result<Box<dyn HarnessSession>, HarnessError> {
        Err(HarnessError::NotSupported("resume"))
    }
}

/// A session over a custom JSON harness: no resume token — a tier-3
/// harness has no session to come back to — so nothing but its wire.
pub struct CustomJsonSession {
    shared: Shared,
}

impl CustomJsonSession {
    async fn send_prompt(&self, text: &str) -> Result<(), HarnessError> {
        self.shared.set_phase(Phase::Turn);
        self.shared
            .broadcaster
            .emit(SessionEvent::Progress(ProgressEvent::TurnStarted));
        self.shared
            .send(OutMsg::Json(
                serde_json::json!({ "type": "prompt", "text": text }),
            ))
            .await
    }
}

#[async_trait]
impl HarnessSession for CustomJsonSession {
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

    async fn steer(&self, _msg: Steer) -> Result<(), HarnessError> {
        Err(HarnessError::NotSupported("steer"))
    }

    async fn follow_up(&self, msg: Steer) -> Result<(), HarnessError> {
        if self.shared.is_ended() {
            return Err(HarnessError::Terminated);
        }
        self.shared
            .send(OutMsg::Json(
                serde_json::json!({ "type": "prompt", "text": msg.text }),
            ))
            .await
    }

    async fn abort(&self) -> Result<(), HarnessError> {
        self.shared.send(OutMsg::Kill).await
    }

    fn subscribe(&self) -> BoxEventStream {
        self.shared.broadcaster.subscribe()
    }

    fn resume_token(&self) -> Option<ResumeToken> {
        None
    }

    async fn dispose(self: Box<Self>) -> Result<(), HarnessError> {
        self.shared.close_stdin_quietly().await;
        Ok(())
    }
}
