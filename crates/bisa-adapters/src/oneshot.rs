//! Machinery for one-shot-per-turn CLIs (codex, opencode): each prompt spawns
//! one process run; the session facade stays stable across runs and later
//! turns resume by native session id.

use std::sync::Arc;

use bisa_harness::proc::{Line, ProcHandle, ProcSpec};
use bisa_harness::{
    BoxEventStream, HarnessError, LifecycleEvent, Outcome, Phase, ProgressEvent, PromptInput,
    ResumeToken, SessionEvent, SessionSnapshot, SessionSpec,
};
use tokio::sync::mpsc;

use crate::util::{Drive, Shared};
use bisa_core::sync::Locked;

/// Builds the ProcSpec for one run: `(prompt, resume_native_id) -> spec`.
pub type SpawnFn = dyn Fn(&str, Option<&str>) -> ProcSpec + Send + Sync;
/// Maps one stdout line during a run.
pub type MapFn = dyn Fn(&Shared, serde_json::Value) -> Drive + Send + Sync;

pub struct OneShotSession {
    pub adapter_id: &'static str,
    pub shared: Shared,
    spawn: Arc<SpawnFn>,
    map: Arc<MapFn>,
    /// Kill switch for the current run.
    kill_tx: std::sync::Mutex<Option<mpsc::Sender<()>>>,
}

impl OneShotSession {
    /// Create the facade and start the first run if the spec has a prompt.
    ///
    /// Takes the whole spec rather than `prompt`/`model`/`cwd` apart: the
    /// three always come from one, and passing them separately is how a
    /// caller ends up handing over a prompt from the launch and a directory
    /// from somewhere else.
    pub async fn start(
        adapter_id: &'static str,
        spawn: Arc<SpawnFn>,
        map: Arc<MapFn>,
        spec: &SessionSpec,
        resume_id: Option<&str>,
        revived: bool,
    ) -> Result<Self, HarnessError> {
        let (out_tx, _out_rx) = mpsc::channel(1); // unused; Shared requires one
        let shared =
            Shared::new(out_tx, adapter_id, spec.model.clone(), &spec.cwd).at_effort(spec.effort);
        shared.broadcaster.emit(SessionEvent::Lifecycle(if revived {
            LifecycleEvent::Revived
        } else {
            LifecycleEvent::Started
        }));
        shared.set_phase(Phase::Idle);
        if let Some(id) = resume_id {
            shared.set_native_id(id);
        }
        let session = Self {
            adapter_id,
            shared,
            spawn,
            map,
            kill_tx: std::sync::Mutex::new(None),
        };
        if !spec.prompt.is_empty() {
            session.run(&spec.prompt).await?;
        }
        Ok(session)
    }

    /// Spawn one process run for `prompt`.
    pub async fn run(&self, prompt: &str) -> Result<(), HarnessError> {
        match self.shared.phase() {
            Phase::Turn => return Err(HarnessError::Busy),
            Phase::Ended => return Err(HarnessError::Terminated),
            _ => {}
        }
        let native = self.shared.native_id();
        let spec = (self.spawn)(prompt, native.as_deref());
        let mut proc = ProcHandle::spawn(spec)?;
        // Each run is its own process: announce it the way `util::drive`
        // does, so the driver records the pid a restart would have to end.
        self.shared
            .broadcaster
            .emit(SessionEvent::Lifecycle(LifecycleEvent::ProcessStarted {
                pid: proc.pid(),
            }));

        self.shared.set_phase(Phase::Turn);
        self.shared
            .broadcaster
            .emit(SessionEvent::Progress(ProgressEvent::TurnStarted));

        let (kill_tx, mut kill_rx) = mpsc::channel::<()>(1);
        *self.kill_tx.locked() = Some(kill_tx);

        let shared = self.shared.clone();
        let map = Arc::clone(&self.map);
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    line = proc.recv() => match line {
                        Some(Line::Json(v)) => {
                            if let Drive::End(outcome) = (map)(&shared, v) {
                                shared.end(outcome);
                                if let Err(e) = proc.kill().await {
                                    tracing::warn!("the harness child did not end on request: {e}");
                                }
                                return;
                            }
                        }
                        Some(Line::Text(t)) => {
                            tracing::debug!(target: "bisa_adapters::oneshot", "stdout: {t}");
                        }
                        None => {
                            // Run over. The session facade survives: the next
                            // prompt resumes by native id.
                            let status = proc.wait().await;
                            let outcome = match status {
                                Ok(s) if s.success() => Outcome::Completed,
                                Ok(s) => {
                                    let tail = proc.stderr_tail().join(" | ");
                                    shared.model_ctx.outcome(&tail).unwrap_or(Outcome::Failed {
                                        error: format!(
                                            "process exited with {s}; stderr tail: {tail}"
                                        ),
                                    })
                                }
                                Err(e) => Outcome::Failed { error: e.to_string() },
                            };
                            shared.broadcaster.emit(SessionEvent::Progress(ProgressEvent::TurnEnded));
                            shared.set_phase(Phase::Idle);
                            shared.broadcaster.emit(SessionEvent::Lifecycle(LifecycleEvent::Ended {
                                outcome,
                                is_terminal: false,
                            }));
                            return;
                        }
                    },
                    _ = kill_rx.recv() => {
                        if let Err(e) = proc.kill().await {
                            tracing::warn!("the harness child did not end on request: {e}");
                        }
                        shared.set_phase(Phase::Idle);
                        shared.broadcaster.emit(SessionEvent::Lifecycle(LifecycleEvent::Ended {
                            outcome: Outcome::Aborted,
                            is_terminal: false,
                        }));
                        return;
                    }
                }
            }
        });
        Ok(())
    }

    pub fn snapshot(&self) -> SessionSnapshot {
        self.shared.snapshot()
    }

    pub fn phase(&self) -> Phase {
        self.shared.phase()
    }

    pub async fn prompt(&self, input: PromptInput) -> Result<(), HarnessError> {
        self.run(&input.text).await
    }

    pub async fn abort(&self) -> Result<(), HarnessError> {
        let tx = self.kill_tx.locked().clone();
        match tx {
            Some(tx) => {
                if tx.send(()).await.is_err() {
                    tracing::debug!("the one-shot session had already ended");
                }
                Ok(())
            }
            None => Ok(()),
        }
    }

    pub fn subscribe(&self) -> BoxEventStream {
        self.shared.broadcaster.subscribe()
    }

    /// The handle the session is revived by: every turn is a process of its
    /// own, so the model and the effort the first was given are what the
    /// token says and what each later turn is given again.
    pub fn resume_token(&self) -> Option<ResumeToken> {
        self.shared.resume_token(self.adapter_id)
    }

    pub async fn dispose(&self) -> Result<(), HarnessError> {
        self.abort().await
    }
}
