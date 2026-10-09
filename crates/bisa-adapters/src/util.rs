//! Shared session machinery for subprocess-backed adapters, and the one
//! place in the tree that reads a harness's prose about a dead model.
//!
//! Every adapter session follows the same shape: one driver task owns the
//! [`ProcHandle`] (its line receiver cannot be shared), `select!`ing between
//! child stdout lines and an outbound command channel fed by the session's
//! trait methods. Session state lives behind a mutex and is only advanced by
//! the driver, keeping the snapshot's `revision` monotonic by construction.
//!
//! # Adapters classify; the engine switches on types
//!
//! The codebase rule **"never parse prose"** applies to the *engine*, not to
//! the adapter. An adapter's whole job is to turn one harness's private
//! vocabulary into the shared one, so prose is parsed **once, here, at the
//! adapter boundary**, into a typed value — [`ModelFailure`], and from there
//! [`bisa_harness::HarnessError::ModelUnavailable`] or
//! [`Outcome::ModelUnavailable`] — and never again. Nothing above this layer
//! ever looks at a harness's wording; the engine matches on the variant.
//!
//! **Typed signals win.** Where a harness reports the fact in a machine
//! readable field — Claude Code puts an HTTP status on its `result` frame and
//! a `trigger` on its `model_fallback` system event — the adapter keys off
//! that and never reaches the table below. The tables exist for the harnesses
//! that offer nothing better.
//!
//! # Never over-match
//!
//! A false positive here is worse than a false negative: it silently retries
//! genuinely failed work on another model and buries the real error. So:
//!
//! - The classifier is only ever fed **error channels** — a stderr tail after
//!   a non-zero exit, a `result` frame's `errors[]`, a JSON-RPC `error`
//!   message, an `ended{outcome:"failed"}` line. It is never fed assistant
//!   text, so an agent implementing a rate limiter cannot classify itself to
//!   death.
//! - Patterns are **conjunctions of literal fragments**, not loose keywords,
//!   and several carry explicit exclusions (a harness reporting *GitHub's*
//!   rate limit is not a model failure — both Claude Code and opencode ship
//!   that exact message).
//! - When in doubt the pattern is left out. A missed classification costs one
//!   honest `Failed`; a wrong one costs the truth.
//!
//! Every fragment in the tables is marked `captured` (read out of a shipped
//! harness binary or a harness source tree on this machine) or `inferred`
//! (written from documentation or from a sibling harness's wording, for a
//! harness not installed here).
//!
//! # Launch versus mid-run, and why subprocess adapters only have one of them
//!
//! [`bisa_harness::HarnessError::ModelUnavailable`] is for a model wall
//! seen while `launch()` still owns the `Result`;
//! [`Outcome::ModelUnavailable`] is for one seen after a session exists.
//!
//! Every subprocess adapter here reaches the caller through the **second**
//! one, and that is not an oversight. A CLI that will not run `--model x`
//! reports it by exiting, and an exit is only observable by waiting: this
//! machine takes ~250 ms just to spawn and reap a two-line shell script, and a
//! real harness binary is slower still, so a window wide enough to be reliable
//! would put a second of latency on **every** successful launch to catch the
//! rare failing one. That trade is not worth making, because the fact loses
//! nothing by arriving one step later: a session whose *first* lifecycle event
//! is a terminal `ModelUnavailable` produced no progress, which is precisely a
//! failed launch, and the engine's launch walk reads it as one.
//!
//! The `HarnessError` form is for adapters whose launch really is synchronous
//! — today the A2A adapter, whose `launch` performs a JSON-RPC call and can be
//! told "out of quota" before it returns.

use std::sync::{Arc, Mutex};

use bisa_core::sync::Locked;
use bisa_harness::traits::EventBroadcaster;
use bisa_harness::{
    Effort, HarnessError, LifecycleEvent, Outcome, Phase, ResumeToken, SessionCost, SessionEvent,
    SessionSnapshot,
};
use std::time::Duration;
use tokio::sync::mpsc;

/// Outbound instruction from a session method to its driver task.
#[derive(Debug)]
pub enum OutMsg {
    /// Write one JSON object as a stdin line.
    Json(serde_json::Value),
    /// Write a raw stdin line.
    Line(String),
    /// Close stdin (EOF — many CLIs finish their run on this).
    CloseStdin,
    /// Kill the child process.
    Kill,
}

/// Mutable session state, driver-owned.
#[derive(Debug)]
pub struct State {
    pub revision: u64,
    pub phase: Phase,
    pub activity: Option<String>,
    pub cost: SessionCost,
    /// Harness-native session id, once learned from the wire.
    pub native_id: Option<String>,
    /// Local transcript path, once learned from the wire.
    pub transcript: Option<std::path::PathBuf>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            revision: 1,
            phase: Phase::Starting,
            activity: None,
            cost: SessionCost::default(),
            native_id: None,
            transcript: None,
        }
    }
}

/// Which harness this session is, and which model it was pinned to — the two
/// things a model failure has to name.
#[derive(Debug, Clone)]
pub struct ModelCtx {
    /// Adapter id, e.g. `claude-code`, `acp:goose`, `custom:mine`.
    pub harness: String,
    /// The pinned model, or `None` when the harness runs its own default.
    pub model: Option<String>,
}

impl ModelCtx {
    pub fn new(harness: impl Into<String>, model: Option<String>) -> Self {
        Self {
            harness: harness.into(),
            model,
        }
    }

    /// What to call the model in a report. A session with no pin still names
    /// something the health ledger can key on, because "the harness default"
    /// is exactly the thing that just went away.
    pub fn model_name(&self) -> String {
        self.model
            .clone()
            .unwrap_or_else(|| format!("{} default", self.harness))
    }

    /// Mid-run: a model wall observed on an error channel.
    pub fn outcome(&self, text: &str) -> Option<Outcome> {
        classify_model_failure(&self.harness, text).map(|f| Outcome::ModelUnavailable {
            model: self.model_name(),
            reason: f.reason,
            retry_after: f.retry_after,
        })
    }

    /// Same fact, observed while `launch()` still owns the Result.
    pub fn error(&self, text: &str) -> Option<HarnessError> {
        classify_model_failure(&self.harness, text).map(|f| HarnessError::ModelUnavailable {
            model: self.model_name(),
            reason: f.reason,
            retry_after: f.retry_after,
        })
    }

    /// Build the outcome from an already-typed signal (an HTTP status, an
    /// error code) — no prose involved.
    pub fn typed_outcome(&self, reason: impl Into<String>, retry_after: Option<u64>) -> Outcome {
        Outcome::ModelUnavailable {
            model: self.model_name(),
            reason: reason.into(),
            retry_after,
        }
    }
}

/// The session's way to its driver: the facade's, which keeps the channel
/// open, or the driver's own, which does not — so a facade let go of closes
/// the channel, and the driver sees it and ends the child.
#[derive(Clone, Debug)]
enum Outbound {
    Facade(mpsc::Sender<OutMsg>),
    Driver(mpsc::WeakSender<OutMsg>),
}

impl Outbound {
    fn sender(&self) -> Option<mpsc::Sender<OutMsg>> {
        match self {
            Outbound::Facade(tx) => Some(tx.clone()),
            Outbound::Driver(weak) => weak.upgrade(),
        }
    }
}

/// Shared handle between the session facade and its driver task.
#[derive(Clone)]
pub struct Shared {
    pub broadcaster: EventBroadcaster,
    pub state: Arc<Mutex<State>>,
    out: Outbound,
    /// The child's process group, when the session has a local process:
    /// what a stop ends — the harness, the commands its tools run, the MCP
    /// server it was handed — without the driver's help
    /// (`bisa_harness::proc::ProcGroup`).
    pub group: Arc<bisa_harness::proc::ProcGroup>,
    /// Harness + model, so any layer holding a `Shared` can report a model
    /// failure without threading the spec through.
    pub model_ctx: Arc<ModelCtx>,
    /// Where this session is running, for the same reason `model_ctx` is
    /// here: `resume_token()` has to name it and does not see the spec.
    ///
    /// Carrying it is what lets a resume be *placed* rather than derived.
    /// Every adapter used to reconstruct it at attach time from whatever was
    /// nearby — a transcript's parent, the daemon's own directory, `/` — and
    /// each of those is a different wrong answer to a question the launch
    /// already knew.
    pub cwd: Arc<std::path::PathBuf>,
    /// The effort the session was launched at, for the same reason again:
    /// the resume token says it, so a revived session runs at it. `None`
    /// when none was sent.
    pub effort: Option<Effort>,
}

impl Shared {
    pub fn new(
        out_tx: mpsc::Sender<OutMsg>,
        harness: impl Into<String>,
        model: Option<String>,
        cwd: impl Into<std::path::PathBuf>,
    ) -> Self {
        Self {
            broadcaster: EventBroadcaster::default(),
            state: Arc::new(Mutex::new(State::default())),
            out: Outbound::Facade(out_tx),
            group: Arc::new(bisa_harness::proc::ProcGroup::none()),
            model_ctx: Arc::new(ModelCtx::new(harness, model)),
            cwd: Arc::new(cwd.into()),
            effort: None,
        }
    }

    /// The same handle, for a session whose child leads `group`.
    pub fn in_group(mut self, group: Arc<bisa_harness::proc::ProcGroup>) -> Self {
        self.group = group;
        self
    }

    /// The handle the driver task holds: everything the facade's is, but
    /// its way back to the driver does not keep the channel open — so once
    /// every facade is gone the driver reads the end of its channel and ends
    /// the child, rather than keeping it alive on its own clone.
    pub fn for_driver(&self) -> Self {
        let out = match &self.out {
            Outbound::Facade(tx) => Outbound::Driver(tx.downgrade()),
            Outbound::Driver(weak) => Outbound::Driver(weak.clone()),
        };
        Self {
            out,
            ..self.clone()
        }
    }

    /// Queue one message for the driver without waiting; a channel full or
    /// closed is the error, with the message.
    pub fn try_send(&self, msg: OutMsg) -> Result<(), mpsc::error::TrySendError<OutMsg>> {
        match self.out.sender() {
            Some(tx) => tx.try_send(msg),
            None => Err(mpsc::error::TrySendError::Closed(msg)),
        }
    }

    /// End the child's whole group in two steps — `SIGTERM`, `grace`, then
    /// `SIGKILL` — and wait for it to be gone. What every `abort` ends with,
    /// after the cancel the harness understands: a harness that goes on
    /// after its cancel, and the MCP server it was handed, are not left
    /// running. Answers whether anything was there to end.
    pub async fn terminate_group(&self, grace: Duration) -> bool {
        self.group.terminate(grace).await
    }

    /// Give the child's group `grace` to leave on the EOF it was given,
    /// then kill what stayed. What every `dispose` ends with.
    pub async fn leave_or_kill(&self, grace: Duration) {
        self.group.gone_or_killed(grace).await;
    }

    /// End the child's whole group the way every `abort` ends, after the
    /// cancel the harness understands was sent and the session ended: its
    /// stdin closed — the EOF a `dispose` gives — then half of `grace` to
    /// leave on it, `SIGTERM`, the other half, `SIGKILL`. A harness that
    /// leaves on EOF ends on its own terms and says so; `SIGTERM` first cut
    /// it off mid-word, an agent's record of its own end among the losses.
    /// Answers whether it left on its own.
    pub async fn abort_group(&self, grace: Duration) -> bool {
        self.close_stdin_quietly().await;
        self.group.leave_or_terminate(grace).await
    }

    /// The same handle, for a session launched at `effort`.
    pub fn at_effort(mut self, effort: Option<Effort>) -> Self {
        self.effort = effort;
        self
    }

    /// The handle this session is revived by, once the harness has named it:
    /// its id, where it ran, and the model and the effort it was launched
    /// with — a harness keeps the conversation, not the flags.
    pub fn resume_token(&self, adapter_id: &str) -> Option<ResumeToken> {
        self.native_id().map(|native_id| ResumeToken {
            adapter_id: adapter_id.to_string(),
            native_id,
            cwd: self.cwd(),
            transcript_path: self.transcript(),
            model: self.model_ctx.model.clone(),
            effort: self.effort,
        })
    }

    /// The directory this session was launched in.
    pub fn cwd(&self) -> std::path::PathBuf {
        self.cwd.as_ref().clone()
    }

    /// End the session the way a dead process deserves: a model wall in
    /// `text` becomes [`Outcome::ModelUnavailable`], anything else stays
    /// [`Outcome::Failed`] with `fallback` as the error.
    pub fn end_classified(&self, text: &str, fallback: impl FnOnce() -> String) {
        match self.model_ctx.outcome(text) {
            Some(outcome) => self.end(outcome),
            None => self.end(Outcome::Failed { error: fallback() }),
        }
    }

    pub fn snapshot(&self) -> SessionSnapshot {
        let s = self.state.locked();
        SessionSnapshot {
            revision: s.revision,
            phase: s.phase,
            activity: s.activity.clone(),
            cost: s.cost,
        }
    }

    pub fn phase(&self) -> Phase {
        self.state.locked().phase
    }

    pub fn native_id(&self) -> Option<String> {
        self.state.locked().native_id.clone()
    }

    pub fn set_native_id(&self, id: impl Into<String>) {
        let mut s = self.state.locked();
        if s.native_id.is_none() {
            s.native_id = Some(id.into());
            s.revision += 1;
        }
    }

    pub fn transcript(&self) -> Option<std::path::PathBuf> {
        self.state.locked().transcript.clone()
    }

    pub fn set_transcript(&self, path: impl Into<std::path::PathBuf>) {
        let mut s = self.state.locked();
        if s.transcript.is_none() {
            s.transcript = Some(path.into());
            s.revision += 1;
        }
    }

    pub fn set_phase(&self, phase: Phase) {
        let mut s = self.state.locked();
        if s.phase != phase {
            s.phase = phase;
            s.revision += 1;
        }
    }

    pub fn set_activity(&self, activity: impl Into<String>) {
        let mut s = self.state.locked();
        s.activity = Some(activity.into());
        s.revision += 1;
    }

    pub fn add_cost(&self, input_tokens: u64, output_tokens: u64, usd_cents: u64) {
        let mut s = self.state.locked();
        s.cost.input_tokens += input_tokens;
        s.cost.output_tokens += output_tokens;
        s.cost.usd_cents += usd_cents;
        s.revision += 1;
    }

    /// Emit a terminal end and move to `Ended`. Idempotent: only the first
    /// call emits (drivers race process-exit against explicit ends).
    pub fn end(&self, outcome: Outcome) {
        {
            let mut s = self.state.locked();
            if s.phase == Phase::Ended {
                return;
            }
            s.phase = Phase::Ended;
            s.revision += 1;
        }
        self.broadcaster
            .emit(SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome,
                is_terminal: true,
            }));
    }

    /// Ask the driver to close the child's stdin — the graceful end every
    /// adapter's `dispose` asks for: EOF lets the CLI finish and exit, and the
    /// driver ends the session. A driver that has already ended is not news.
    pub async fn close_stdin_quietly(&self) {
        if let Err(e) = self.send(OutMsg::CloseStdin).await {
            tracing::debug!("the driver had already ended: {e}");
        }
    }

    pub async fn send(&self, msg: OutMsg) -> Result<(), HarnessError> {
        let Some(tx) = self.out.sender() else {
            return Err(HarnessError::Terminated);
        };
        tx.send(msg).await.map_err(|_| HarnessError::Terminated)
    }

    pub fn is_ended(&self) -> bool {
        self.phase() == Phase::Ended
    }
}

/// Truncate arbitrary JSON args into a short human summary for
/// `ProgressEvent::ToolStarted`: a bare string is its text (a command reads
/// `cargo test`, not `"cargo test"`), nothing is empty, anything else is its
/// JSON, cut at `max` on a character boundary.
pub fn summarize_args(args: &serde_json::Value, max: usize) -> String {
    let s = match args {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    if s.len() > max {
        let mut cut = max;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        format!("{}…", &s[..cut])
    } else {
        s
    }
}

/// Version probe helper: run `<program> <args>` and take the first stdout line.
pub async fn version_of(program: &str, args: &[&str]) -> Option<String> {
    let out = tokio::process::Command::new(program)
        .args(args)
        .output()
        .await
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .next()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
}

/// Standard two-phase probe: PATH lookup, then a cheap version command.
pub async fn probe_binary(program: &str, version_args: &[&str]) -> bisa_harness::ProbeResult {
    if which::which(program).is_err() {
        return bisa_harness::ProbeResult::unavailable(format!("{program} not found on PATH"));
    }
    bisa_harness::ProbeResult::available(version_of(program, version_args).await)
}

/// Whether a line carries a version number: a digit, a dot, a digit —
/// `1.4.2`, `copilot 0.0.400`, `grok 0.1.42 (abc123)`.
pub fn has_version(line: &str) -> bool {
    line.as_bytes()
        .windows(3)
        .any(|w| w[0].is_ascii_digit() && w[1] == b'.' && w[2].is_ascii_digit())
}

/// The two-phase probe for a binary whose name other tools answer to: PATH
/// lookup, then the version command — and the harness is there only when the
/// first line it answers carries a version number.
///
/// [`probe_binary`] takes any first line for the version. That is wrong for
/// a name like `copilot`, which an editor's launcher also installs: with no
/// CLI behind it, that launcher answers `--version` with a question (*Install
/// GitHub Copilot CLI?*) and exits 0 — a harness "installed", whose every
/// launch then ends at once.
pub async fn probe_versioned(program: &str, version_args: &[&str]) -> bisa_harness::ProbeResult {
    if which::which(program).is_err() {
        return bisa_harness::ProbeResult::unavailable(format!("{program} not found on PATH"));
    }
    versioned(
        program,
        version_args,
        version_of(program, version_args).await,
    )
}

/// What a version command's first line says of the binary that answered.
fn versioned(
    program: &str,
    version_args: &[&str],
    first_line: Option<String>,
) -> bisa_harness::ProbeResult {
    match first_line {
        Some(line) if has_version(&line) => bisa_harness::ProbeResult::available(Some(line)),
        _ => bisa_harness::ProbeResult::unavailable(format!(
            "the {program} on PATH did not answer `{program} {}` with a version",
            version_args.join(" ")
        )),
    }
}

/// What the per-line mapper tells the driver.
#[derive(Debug)]
pub enum Drive {
    Continue,
    /// Terminal end with this outcome (driver emits and exits).
    End(Outcome),
}

/// Run the standard driver loop: one owner for the [`ProcHandle`], select
/// between child stdout lines and outbound messages. The mapper never blocks;
/// it emits events through `shared.broadcaster` and returns [`Drive`].
///
/// When stdout closes without an explicit end, the exit status decides:
/// 0 → `Completed`, anything else → `Failed` with the stderr tail.
pub async fn drive<M>(
    proc: bisa_harness::proc::ProcHandle,
    out_rx: mpsc::Receiver<OutMsg>,
    shared: Shared,
    on_line: M,
) where
    M: FnMut(&Shared, bisa_harness::proc::Line) -> Drive + Send + 'static,
{
    drive_until(proc, out_rx, shared, on_line, |_| Outcome::Completed).await;
}

/// [`drive`], for a harness whose clean exit is not always a finished run:
/// `on_clean_exit` says what an exit with status 0 means, given the stderr
/// tail — an agent that speaks a protocol and leaves before its session
/// exists has not completed anything.
pub async fn drive_until<M, E>(
    mut proc: bisa_harness::proc::ProcHandle,
    mut out_rx: mpsc::Receiver<OutMsg>,
    shared: Shared,
    mut on_line: M,
    on_clean_exit: E,
) where
    M: FnMut(&Shared, bisa_harness::proc::Line) -> Drive + Send + 'static,
    E: FnOnce(&[String]) -> Outcome + Send + 'static,
{
    let mut on_clean_exit = Some(on_clean_exit);
    // This loop owns the running child: announce its pid so the desktop can
    // trace a port the harness (or a server it starts) opens back to this
    // session. One place, rather than each adapter's `Started`,
    // which fires before the process exists.
    shared
        .broadcaster
        .emit(bisa_harness::SessionEvent::Lifecycle(
            bisa_harness::LifecycleEvent::ProcessStarted { pid: proc.pid() },
        ));
    loop {
        tokio::select! {
            line = proc.recv() => match line {
                Some(line) => match on_line(&shared, line) {
                    Drive::Continue => {}
                    Drive::End(outcome) => {
                        shared.end(outcome);
                        // Keep draining the outbound channel briefly? No —
                        // a terminal end means the run is over; kill and exit.
                        if let Err(e) = proc.kill().await {
                            tracing::warn!("the harness child did not end on request: {e}");
                        }
                        return;
                    }
                },
                None => {
                    // stdout closed: process is ending. Decide by exit code
                    // unless the wire already ended the session.
                    let status = proc.wait().await;
                    if !shared.is_ended() {
                        match status {
                            Ok(s) if s.success() => {
                                let outcome = match on_clean_exit.take() {
                                    Some(means) => means(&proc.stderr_tail()),
                                    None => Outcome::Completed,
                                };
                                shared.end(outcome);
                            }
                            // The key site: an unknown or exhausted `--model`
                            // usually dies right here, and used to arrive as a
                            // plain `Failed` indistinguishable from a defect.
                            Ok(s) => {
                                let tail = proc.stderr_tail().join(" | ");
                                shared.end_classified(&tail, || {
                                    format!("process exited with {s}; stderr tail: {tail}")
                                });
                            }
                            Err(e) => shared.end(Outcome::Failed { error: e.to_string() }),
                        }
                    }
                    return;
                }
            },
            msg = out_rx.recv() => match msg {
                Some(OutMsg::Json(v)) => {
                    if let Err(e) = proc.send_json(&v).await {
                        // LCOV_EXCL_START: the child's stdin breaking under a write — whether the select sees the exit or the write first is the kernel's order, a race no test can stage
                        tracing::warn!("stdin write failed: {e}");
                        shared.end(Outcome::Failed { error: format!("stdin write failed: {e}") });
                        return;
                        // LCOV_EXCL_STOP
                    }
                }
                Some(OutMsg::Line(l)) => {
                    if let Err(e) = proc.send_line(&l).await {
                        shared.end(Outcome::Failed { error: format!("stdin write failed: {e}") });
                        return;
                    }
                }
                Some(OutMsg::CloseStdin) => {
                    // A stdin already gone is a child already leaving.
                    if let Err(e) = proc.close_stdin().await {
                        tracing::debug!("closing the harness's stdin: {e}");
                    }
                }
                Some(OutMsg::Kill) => {
                    if let Err(e) = proc.kill().await {
                        tracing::warn!("the harness child did not end on request: {e}");
                    }
                    shared.end(Outcome::Aborted);
                    return;
                }
                None => {
                    // Every session facade is gone: nothing can write to the
                    // child any more, and nobody will read its end. It is
                    // given the let-go grace to leave on EOF, then ended with
                    // its group.
                    if let Err(e) = proc
                        .terminate(bisa_harness::proc::DISPOSE_GRACE)
                        .await
                    {
                        tracing::warn!("the harness child did not end on request: {e}");
                    }
                    if !shared.is_ended() {
                        shared.end(Outcome::Aborted);
                    }
                    return;
                }
            },
        }
    }
}

/// What a harness's own command printed on stdout, given eight seconds and
/// no stdin; `None` when the binary is not on `PATH`, does not answer in
/// time, or exits with a failure.
pub async fn command_output(program: &str, args: &[&str]) -> Option<String> {
    if which::which(program).is_err() {
        return None;
    }
    let out = tokio::time::timeout(
        std::time::Duration::from_secs(8),
        tokio::process::Command::new(program)
            .args(args)
            .stdin(std::process::Stdio::null())
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Run a harness's own "list models" command and return one model per output
/// line. Empty on any failure — an unknown list must never be mistaken for
/// "this harness supports no models"; callers fall back to free-text entry.
pub async fn models_from_command(
    program: &str,
    args: &[&str],
    parse_line: fn(&str) -> Option<bisa_harness::types::ModelInfo>,
) -> Vec<bisa_harness::types::ModelInfo> {
    let Some(output) = command_output(program, args).await else {
        return Vec::new();
    };
    let mut seen = std::collections::BTreeSet::new();
    output
        .lines()
        .filter_map(parse_line)
        .filter(|m| seen.insert(m.id.clone()))
        .collect()
}

// ---------------------------------------------------------------------------
// Model-failure classification — the one prose parser in the tree
// ---------------------------------------------------------------------------

/// A harness's error text, understood: this is the *model* refusing, not the
/// work failing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelFailure {
    /// Short label plus the matched excerpt, for the journal.
    pub reason: String,
    /// Seconds, and only when the harness said so itself. Never guessed —
    /// the engine's cooldown policy owns the guess.
    pub retry_after: Option<u64>,
}

/// One entry in a harness's table.
///
/// A pattern matches when **every** fragment in `all_of` appears in the
/// lowercased text and **no** fragment in `none_of` does. Conjunctions, not
/// keywords: `["model", "not available"]` is a pattern, `"model"` is not.
///
/// A fragment containing `_` is treated as an **error code** and must appear
/// as a whole token — `"model_not_found"` in a JSON body matches,
/// `fn model_not_found_page()` in a stack trace does not. Codes are the
/// fragments most likely to collide with a codebase's own identifiers, and a
/// stderr tail can carry both.
struct Pattern {
    all_of: &'static [&'static str],
    none_of: &'static [&'static str],
    reason: &'static str,
}

const fn p(all_of: &'static [&'static str], reason: &'static str) -> Pattern {
    Pattern {
        all_of,
        none_of: &[],
        reason,
    }
}

const fn p_not(
    all_of: &'static [&'static str],
    none_of: &'static [&'static str],
    reason: &'static str,
) -> Pattern {
    Pattern {
        all_of,
        none_of,
        reason,
    }
}

/// Something else's rate limit is not our model's rate limit. Both Claude Code
/// and opencode ship a message about GitHub throttling `gh`/`bun`, and an
/// agent's shell step hitting one must not cost it its model.
///
/// captured — Claude Code 2.1.246: `API rate limit (?:already )?exceeded|
/// exceeded a secondary rate limit|\bRATE_LIMITED\b` and the
/// `GitHub API rate limit exceeded (5,000/hr …)` system reminder.
/// captured — opencode 1.18.23: `GitHub returned 403. This usually means
/// GitHub is rate limiting your requests.`
const NOT_THE_MODEL: &[&str] = &["github", "gh api", "npm registry"];

/// Wording that belongs to the **model provider**, not to any one harness, so
/// it leaks through all of them. Checked for every harness, after that
/// harness's own table.
const PROVIDER: &[Pattern] = &[
    // captured — Claude Code 2.1.246 blocked-prose regex:
    //   `credit balance (?:is )?too low|usage limit reached|…`
    // captured — opencode 1.18.23: `${plan} usage limit reached. It will
    //   reset in ${d}. To continue using this model now, enable usage from
    //   your available balance`
    // captured — pi `packages/ai/src/utils/retry.ts`: "Monthly usage limit reached"
    p(&["usage limit reached"], "usage limit reached"),
    // captured — Claude Code 2.1.246 prefix list:
    //   ["You've hit your", "You've reached your", "You're out of usage
    //    credits", "Your org is out of usage · add funds to continue", …]
    // This is the M5 string: "You've reached your Fable 5 limit".
    p(&["you've reached your"], "plan limit reached"),
    p(&["you have reached your"], "plan limit reached"),
    p(&["you've hit your"], "plan limit reached"),
    // captured — omp 18.0.3: `You have hit your ChatGPT usage limit${plan}.
    //   Try again in ~${n} min.`
    p(&["you have hit your", "usage limit"], "plan limit reached"),
    // captured — Claude Code 2.1.246 prefix list.
    p(&["out of usage credits"], "out of usage credits"),
    p(&["out of extra usage"], "out of extra usage"),
    p(&["requires usage credits"], "model requires usage credits"),
    // captured — Claude Code 2.1.246: "Credit balance is too low"; and its
    //   blocked-prose list "credit balance too low".
    p(&["credit balance", "too low"], "credit balance too low"),
    // captured — omp 18.0.3 classifier: "run out of credits" / "out of credits"
    p(&["out of credits"], "out of credits"),
    // captured — pi retry.ts non-retryable list; omp 18.0.3 `vsi` regex:
    //   /GoUsageLimitError|FreeUsageLimitError|Monthly usage limit reached|
    //    available balance|insufficient_quota|out of budget|quota exceeded|billing/i
    p(&["insufficient_quota"], "quota exhausted"),
    p(&["quota exceeded"], "quota exhausted"),
    p(&["quota_exhausted"], "quota exhausted"),
    p(&["out of budget"], "budget exhausted"),
    p(&["gousagelimiterror"], "subscription usage limit reached"),
    p(&["freeusagelimiterror"], "free-tier usage limit reached"),
    // captured — omp 18.0.3: Google/Vertex `RESOURCE_EXHAUSTED` detail parsing.
    p(&["resource_exhausted"], "provider quota exhausted"),
    // captured — omp 18.0.3 classifier keywords.
    p(&["spending limit"], "spending limit reached"),
    p(&["spending-limit"], "spending limit reached"),
    // captured — Anthropic error taxonomy shipped in the Claude Code binary:
    //   {429:"rate_limit_error", 529:"overloaded_error", 404:"not_found_error"}
    // captured — omp 18.0.3: `u === "overloaded_error" || u === "rate_limit_error"`
    p(&["rate_limit_error"], "rate limited"),
    p(&["rate_limit_exceeded"], "rate limited"),
    p(&["overloaded_error"], "provider overloaded"),
    // captured — opencode 1.18.23 `GatewayRateLimitError` default message.
    p_not(&["rate limit exceeded"], NOT_THE_MODEL, "rate limited"),
    // captured — pi retry.ts RETRYABLE list ("too many requests").
    p_not(&["too many requests"], NOT_THE_MODEL, "rate limited"),
    // captured — Claude Code 2.1.246 API error code enum:
    //   ["authentication_failed","oauth_org_not_allowed","account_on_hold",
    //    "billing_error","rate_limit","overloaded","invalid_request",
    //    "model_not_found","server_error","unknown","max_output_tokens"]
    p(&["model_not_found"], "model not found"),
    // captured — Claude Code 2.1.246 `mHe`: HTTP 404 whose body carries both
    //   `"type":"not_found_error"` and `model:`.
    p(&["not_found_error", "model:"], "model not found"),
];

/// Claude Code. Mostly redundant with its typed signals — kept for the paths
/// where the CLI dies before it can emit a `result` frame.
///
/// captured — strings read out of the installed binary,
/// `~/.local/share/claude/versions/2.1.246` (Claude Code 2.1.246).
const CLAUDE_CODE: &[Pattern] = &[
    // captured: "The model ${m} is not available on your ${deployment}
    //   deployment. … or ask your admin to enable this model."
    p(
        &["is not available on your"],
        "model not enabled for this account",
    ),
    // captured: "There's an issue with the selected model (${m}). It may not
    //   exist or you may not have access to it."
    p(
        &["there's an issue with the selected model"],
        "model missing or not permitted",
    ),
    // captured: "Model '${m}' not found" (model validation path).
    p(&["model '", "' not found"], "model not found"),
    // captured: "' is not in the list of available models"
    p(
        &["is not in the list of available models"],
        "unknown model id",
    ),
    // captured: "Your seat type doesn't include usage credits" / "… extra usage"
    p(
        &["seat type doesn't include"],
        "seat has no usage allowance",
    ),
    // captured: "Your usage allocation has been disabled by your admin"
    p(
        &["usage allocation has been disabled"],
        "usage disabled by admin",
    ),
    // captured: "Your group's usage limit is set to $0"
    p(&["usage limit is set to $0"], "group usage limit is zero"),
];

/// Codex CLI. **Not installed on this machine** — every fragment below is
/// `inferred` from the public Codex/OpenAI error vocabulary and from the way
/// omp (which drives the same backend) words the same conditions.
const CODEX: &[Pattern] = &[
    // inferred from omp 18.0.3's Codex path:
    //   `/rate_limit_exceeded/i.test(p) || e.status === 429` →
    //   "You have hit your ChatGPT usage limit". `rate_limit_exceeded` itself
    //   is in PROVIDER; this catches the CLI's own phrasing.
    p(&["chatgpt usage limit"], "ChatGPT plan limit reached"),
    // inferred — OpenAI's `model_not_found` body wording.
    p(&["the model", "does not exist"], "model not found"),
    p(&["unsupported model"], "unsupported model"),
    p(&["invalid model"], "unknown model id"),
];

/// opencode.
///
/// captured — strings read out of the installed binary,
/// `~/.opencode/bin/opencode` (opencode 1.18.23).
const OPENCODE: &[Pattern] = &[
    // captured: `Model not found: ${providerID}/${modelID}` (+ "Did you mean:
    //   …", "Try: `opencode models` to list available models")
    p(&["model not found:"], "model not found"),
    // captured: the tagged error class names on the wire.
    p(&["providermodelnotfounderror"], "model not found"),
    p(&["gatewaymodelnotfounderror"], "model not found"),
    p(&["gatewayratelimiterror"], "rate limited"),
    // captured: `No models found for provider ${providerID}`
    p(&["no models found for provider"], "provider has no models"),
    // captured: "… usage limit reached. It will reset in ${d}. To continue
    //   using this model now, enable usage from your available balance"
    p(
        &["available balance"],
        "plan limit reached; balance usage disabled",
    ),
];

/// omp and pi share `packages/ai`'s error classifier, so they share a table.
/// omp 18.0.3 is installed here; **pi is not**, but the fragments come from
/// pi's own source tree (`packages/ai/src/utils/retry.ts`, read at capture
/// time), so they are captured rather than guessed — together with strings
/// read out of the installed `omp` binary (omp 18.0.3), whose
/// `packages/ai/src/error/rate-limit.ts` ships readable.
const PI_FAMILY: &[Pattern] = &[
    // captured — omp: `Unknown model: ${id}. Use ACP \`session/setModel\` for
    //   picker-driven selection or list available models with /model.`
    p(&["unknown model:"], "unknown model id"),
    // captured — omp `rate-limit.ts`: `u === "usage_limit_reached"`
    p(&["usage_limit_reached"], "usage limit reached"),
    // captured — omp `JC()` returns these labels on the wire.
    p(&["model_capacity_exhausted"], "model capacity exhausted"),
    p(&["concurrent_limit"], "provider concurrency limit"),
    // captured — omp `JC()`: `n.includes("quota will reset")` /
    //   `n.includes("exhausted your capacity")`
    p(&["quota will reset"], "quota exhausted"),
    p(&["exhausted your capacity"], "capacity exhausted"),
    // captured — pi retry.ts: "available balance" (OpenCode Go subscription
    //   limits, which pi and omp both surface verbatim).
    p(
        &["available balance"],
        "plan limit reached; balance usage disabled",
    ),
];

/// Generic ACP agents. The agent behind the socket owns the wording, so this
/// table stays deliberately thin and leans on PROVIDER.
///
/// captured — opencode 1.18.23's ACP bridge: `ACPInvalidModelError` →
/// `model not found: ${modelId}`.
const ACP: &[Pattern] = &[p(&["model not found:"], "model not found")];

/// The per-harness table. `custom:*` harnesses have none of their own — the
/// command is the user's, so only provider wording is safe to key on. Nor
/// have `copilot` and `grok`: neither CLI's own wording has been captured,
/// and a fragment is never invented — the provider's wording is what both
/// pass through. (`ACP` is not theirs either: its one fragment is OpenCode's
/// bridge.)
fn table(harness: &str) -> &'static [Pattern] {
    // `acp:goose`, `custom:mine` — the family is the part before the colon.
    let family = harness.split(':').next().unwrap_or(harness);
    match family {
        "claude-code" => CLAUDE_CODE,
        "codex" => CODEX,
        "opencode" => OPENCODE,
        "omp" | "pi" => PI_FAMILY,
        "acp" => ACP,
        _ => &[],
    }
}

/// Turn one harness's error text into a typed [`ModelFailure`], or `None`.
///
/// `harness` is the adapter id (`claude-code`, `acp:goose`, `custom:mine`).
/// `text` must come from an **error channel** — see the module docs; feeding
/// it assistant output is the one way to make this lie.
pub fn classify_model_failure(harness: &str, text: &str) -> Option<ModelFailure> {
    if text.trim().is_empty() {
        return None;
    }
    let lower = text.to_lowercase();
    let hit = table(harness)
        .iter()
        .chain(PROVIDER.iter())
        .find(|pat| matches(pat, &lower))?;
    Some(ModelFailure {
        reason: format!("{} ({})", hit.reason, excerpt(text, hit.all_of[0])),
        retry_after: extract_retry_after(&lower),
    })
}

fn matches(pat: &Pattern, lower: &str) -> bool {
    pat.all_of.iter().all(|f| present(lower, f)) && !pat.none_of.iter().any(|f| present(lower, f))
}

fn present(hay: &str, needle: &str) -> bool {
    if needle.contains('_') {
        contains_token(hay, needle)
    } else {
        hay.contains(needle)
    }
}

/// `needle` appears in `hay` with a non-identifier character on each side.
fn contains_token(hay: &str, needle: &str) -> bool {
    let ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut from = 0usize;
    while let Some(rel) = hay[from..].find(needle) {
        let at = from + rel;
        let end = at + needle.len();
        let before_ok = hay[..at].chars().next_back().is_none_or(|c| !ident(c));
        let after_ok = hay[end..].chars().next().is_none_or(|c| !ident(c));
        if before_ok && after_ok {
            return true;
        }
        from = at + needle.len();
    }
    false
}

/// The line the first fragment landed on, trimmed — enough for a journal
/// entry, short enough not to become one. Each line is lowercased on its own
/// and only searched; what is kept is the original line, so a lowercasing
/// that changes byte length (`İ` → `i̇`) can never mis-slice the text.
fn excerpt(text: &str, fragment: &str) -> String {
    const MAX: usize = 160;
    let line = text
        .lines()
        .find(|line| present(&line.to_lowercase(), fragment))
        .unwrap_or(text)
        .trim();
    if line.chars().count() > MAX {
        let cut = line
            .char_indices()
            .nth(MAX)
            .map(|(i, _)| i)
            .unwrap_or(line.len());
        format!("{}…", &line[..cut])
    } else {
        line.to_string()
    }
}

/// Cues a harness uses right before it tells you how long to wait.
///
/// captured — `Retry-After: 12` (Anthropic/OpenAI headers, echoed by Claude
/// Code and opencode); `Try again in ~14 min.` (omp 18.0.3); `It will reset in
/// 3 hours` (opencode 1.18.23); `Rate limit exceeded, retry in ${after}`
/// (opencode's fastify limiter); `please retry after 30 seconds` (pi tests).
const RETRY_CUES: &[&str] = &[
    "retry-after",
    "retry_after",
    "retry after",
    "retry in",
    "try again in",
    "reset in",
    "resets in",
];

/// One day. A longer wait is a human problem, not a cooldown.
const MAX_RETRY_AFTER: u64 = 24 * 60 * 60;

/// Pull a wait hint out of `lower` (already lowercased), in seconds.
///
/// Deliberately strict: the number must follow the cue across nothing but
/// separators, so `reset in less than a minute` yields `None` rather than a
/// number scavenged from further down the line.
fn extract_retry_after(lower: &str) -> Option<u64> {
    RETRY_CUES
        .iter()
        .filter_map(|cue| {
            let at = lower.find(cue)? + cue.len();
            parse_duration_after(&lower[at..])
        })
        .next()
}

fn parse_duration_after(rest: &str) -> Option<u64> {
    let mut chars = rest.char_indices().peekable();
    let mut start = None;
    for (i, c) in chars.by_ref() {
        if c.is_ascii_digit() {
            start = Some(i);
            break;
        }
        // Only separators may sit between the cue and its number.
        if !matches!(c, ' ' | ':' | '=' | '~' | '"' | '\'' | ',' | '\t') {
            return None;
        }
    }
    let start = start?;
    let digits_end = rest[start..]
        .find(|c: char| !c.is_ascii_digit())
        .map(|i| start + i)
        .unwrap_or(rest.len());
    let value: u64 = rest[start..digits_end].parse().ok()?;
    let unit: String = rest[digits_end..]
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    let secs = match unit.as_str() {
        // A bare number after `retry-after` is the HTTP header: seconds.
        "" | "s" | "sec" | "secs" | "second" | "seconds" => value,
        "m" | "min" | "mins" | "minute" | "minutes" => value.saturating_mul(60),
        "h" | "hr" | "hrs" | "hour" | "hours" => value.saturating_mul(3600),
        _ => return None,
    };
    Some(secs.min(MAX_RETRY_AFTER))
}

#[cfg(test)]
mod classify_tests {
    use super::*;

    /// Every fixture is tagged with where it came from:
    ///
    /// - **captured** — read out of a harness shipped on this machine
    ///   (`claude` 2.1.246, `opencode` 1.18.23, `omp` 18.0.3) or out of a
    ///   harness source tree read at capture time (pi's). The wording
    ///   is the harness's own, not ours.
    /// - **inferred** — written from documentation or from a sibling
    ///   harness's wording for the same condition, because the harness is not
    ///   installed here (`codex`, `pi`). No real session was ever started for
    ///   any of these: nothing in this file cost a token.
    fn reason(harness: &str, text: &str) -> String {
        classify_model_failure(harness, text)
            .unwrap_or_else(|| panic!("{harness}: expected a model failure for {text:?}"))
            .reason
    }

    fn none(harness: &str, text: &str) {
        assert_eq!(
            classify_model_failure(harness, text),
            None,
            "{harness}: must NOT classify {text:?} as a model failure",
        );
    }

    // --- claude-code ---------------------------------------------------------

    #[test]
    fn claude_code_patterns() {
        // captured — the M5 incident, verbatim from the 2.1.246 binary:
        //   `A ? "You've reached your Fable 5 limit" : …`
        assert!(reason("claude-code", "You've reached your Fable 5 limit")
            .starts_with("plan limit reached"));
        // captured — 2.1.246 prefix list.
        assert!(
            reason("claude-code", "You've hit your monthly spend limit.")
                .starts_with("plan limit reached")
        );
        assert!(reason("claude-code", "You're out of usage credits")
            .starts_with("out of usage credits"));
        assert!(reason("claude-code", "Fable 5 requires usage credits")
            .starts_with("model requires usage credits"));
        // captured — 2.1.246: "Credit balance is too low"
        assert!(reason("claude-code", "Credit balance is too low")
            .starts_with("credit balance too low"));
        // captured — 2.1.246 model-validation path.
        assert!(
            reason("claude-code", "Model 'claude-opus-9' not found").starts_with("model not found")
        );
        assert!(reason(
            "claude-code",
            "'claude-opus-9' is not in the list of available models",
        )
        .starts_with("unknown model id"));
        // captured — 2.1.246 deployment/access message.
        assert!(reason(
            "claude-code",
            "The model claude-opus-5 is not available on your Bedrock deployment. \
             Try /model to switch to sonnet, or ask your admin to enable this model.",
        )
        .starts_with("model not enabled for this account"));
        assert!(reason(
            "claude-code",
            "There's an issue with the selected model (foo). It may not exist \
             or you may not have access to it.",
        )
        .starts_with("model missing or not permitted"));
        // captured — 2.1.246 seat/admin messages.
        assert!(
            reason("claude-code", "Your seat type doesn't include extra usage")
                .starts_with("seat has no usage allowance")
        );
        assert!(reason(
            "claude-code",
            "Your usage allocation has been disabled by your admin",
        )
        .starts_with("usage disabled by admin"));
        // captured — Anthropic error taxonomy embedded in the binary.
        assert!(reason("claude-code", r#"{"type":"rate_limit_error"}"#).starts_with("rate limited"));
        assert!(reason("claude-code", r#"{"type":"overloaded_error"}"#)
            .starts_with("provider overloaded"));
        // captured — 2.1.246 `mHe`: 404 whose body has not_found_error + model:.
        assert!(reason(
            "claude-code",
            r#"404 {"type":"error","error":{"type":"not_found_error","message":"model: bogus"}}"#,
        )
        .starts_with("model not found"));
    }

    // --- opencode ------------------------------------------------------------

    #[test]
    fn opencode_patterns() {
        // captured — 1.18.23: `Model not found: ${providerID}/${modelID}` plus
        //   the suggestion lines it appends.
        assert!(reason(
            "opencode",
            "Model not found: anthropic/claude-opus-9\nDid you mean: claude-opus-5\n\
             Try: `opencode models` to list available models",
        )
        .starts_with("model not found"));
        // captured — 1.18.23 tagged error class names as they appear on the wire.
        assert!(reason("opencode", "ProviderModelNotFoundError").starts_with("model not found"));
        assert!(reason("opencode", "GatewayRateLimitError").starts_with("rate limited"));
        assert!(reason("opencode", "GatewayModelNotFoundError").starts_with("model not found"));
        // captured — 1.18.23: `No models found for provider ${providerID}`
        assert!(reason("opencode", "No models found for provider zen")
            .starts_with("provider has no models"));
        // captured — 1.18.23 subscription-limit sentence, with its reset hint.
        let f = classify_model_failure(
            "opencode",
            "Pro usage limit reached. It will reset in 3 hours. To continue using \
             this model now, enable usage from your available balance",
        )
        .expect("classified");
        assert!(f.reason.starts_with("plan limit reached"));
        assert_eq!(f.retry_after, Some(3 * 3600));
    }

    // --- omp / pi ------------------------------------------------------------

    #[test]
    fn pi_family_patterns() {
        // captured — omp 18.0.3 CLI + ACP bridge.
        assert!(reason(
            "omp",
            "Unknown model: acme/turbo-9. Use ACP `session/setModel` for \
             picker-driven selection or list available models with /model.",
        )
        .starts_with("unknown model id"));
        // captured — omp 18.0.3 `packages/ai/src/error/rate-limit.ts`.
        assert!(reason("omp", "usage_limit_reached").starts_with("usage limit reached"));
        assert!(reason("omp", "MODEL_CAPACITY_EXHAUSTED").starts_with("model capacity exhausted"));
        assert!(
            reason("omp", "your quota will reset at midnight UTC").starts_with("quota exhausted")
        );
        assert!(
            reason("omp", "You have exhausted your capacity for this model")
                .starts_with("capacity exhausted")
        );
        // captured — pi `packages/ai/src/utils/retry.ts`, non-retryable list.
        assert!(reason("pi", "insufficient_quota").starts_with("quota exhausted"));
        assert!(reason("pi", "GoUsageLimitError").starts_with("subscription usage limit reached"));
        assert!(reason("pi", "FreeUsageLimitError").starts_with("free-tier usage limit reached"));
        assert!(reason("pi", "Monthly usage limit reached").starts_with("usage limit reached"));
        assert!(reason("pi", "out of budget").starts_with("budget exhausted"));
        // captured — pi retry.ts "available balance".
        assert!(reason("pi", "enable usage from your available balance")
            .starts_with("plan limit reached"));
    }

    // --- codex (not installed here) -----------------------------------------

    #[test]
    fn codex_patterns_are_inferred() {
        // inferred — from omp 18.0.3's Codex path, which words the same
        //   backend condition as `You have hit your ChatGPT usage limit
        //   (Plus plan). Try again in ~14 min.`
        let f = classify_model_failure(
            "codex",
            "You have hit your ChatGPT usage limit (Plus plan). Try again in ~14 min.",
        )
        .expect("classified");
        assert!(
            f.reason.starts_with("ChatGPT plan limit reached"),
            "{}",
            f.reason
        );
        assert_eq!(f.retry_after, Some(14 * 60));
        // inferred — OpenAI's `model_not_found` body wording.
        assert!(reason("codex", "The model `gpt-9` does not exist").starts_with("model not found"));
        assert!(reason("codex", "unsupported model: o9-preview").starts_with("unsupported model"));
        // captured — the shared OpenAI/gateway code, seen in omp's Codex path.
        assert!(reason("codex", "rate_limit_exceeded").starts_with("rate limited"));
    }

    // --- acp + custom --------------------------------------------------------

    #[test]
    fn acp_and_custom_lean_on_provider_wording() {
        // captured — opencode 1.18.23's ACP bridge: `ACPInvalidModelError` →
        //   `model not found: ${modelId}`.
        assert!(reason("acp:goose", "model not found: acme/turbo").starts_with("model not found"));
        // A custom harness has no vocabulary of its own; only the provider's
        // wording is safe to key on.
        assert!(reason("custom:mine", "insufficient_quota").starts_with("quota exhausted"));
        none("custom:mine", "Model not found: acme/turbo");
    }

    #[test]
    fn copilot_and_grok_lean_on_provider_wording_alone() {
        for harness in ["copilot", "grok"] {
            assert!(table(harness).is_empty(), "{harness}");
            assert!(reason(harness, "insufficient_quota").starts_with("quota exhausted"));
            // OpenCode's bridge wording is OpenCode's.
            none(harness, "Model not found: acme/turbo");
        }
        // A limit that names GitHub is GitHub's — the code host's, as far as
        // a classifier can tell — and stays a plain failure, Copilot's
        // included: a missed classification costs one honest `Failed`.
        none("copilot", "GitHub API rate limit exceeded for this token");
    }

    #[test]
    fn a_version_line_carries_a_number_and_a_question_does_not() {
        for line in [
            "1.4.2",
            "copilot 0.0.400",
            "GitHub Copilot CLI 1.12.0",
            "grok 0.1.42 (a1b2c3d)",
            "v2.0",
        ] {
            assert!(has_version(line), "{line}");
        }
        for line in [
            "",
            "Install GitHub Copilot CLI? ['y/N']",
            "usage: grok [OPTIONS]",
            "3 files",
            "1. first",
        ] {
            assert!(!has_version(line), "{line}");
        }
        let there = versioned("copilot", &["--version"], Some("copilot 0.0.400".into()));
        assert!(there.available);
        assert_eq!(there.version.as_deref(), Some("copilot 0.0.400"));
        for answer in [
            Some("Install GitHub Copilot CLI? ['y/N']".to_string()),
            None,
        ] {
            let missing = versioned("copilot", &["--version"], answer);
            assert!(!missing.available);
            assert_eq!(
                missing.reason.as_deref(),
                Some("the copilot on PATH did not answer `copilot --version` with a version")
            );
        }
    }

    // --- the thing that must never happen -----------------------------------

    #[test]
    fn genuine_failures_stay_failures() {
        for text in [
            "error: test failed, 3 assertions did not hold",
            "thread 'main' panicked at src/main.rs:12: index out of bounds",
            "npm ERR! code ELIFECYCLE",
            "fatal: not a git repository",
            "TypeError: Cannot read properties of undefined",
            "Command failed with exit code 1",
            "Compilation failed: expected `;`, found `}`",
            "",
            "   ",
        ] {
            none("claude-code", text);
            none("opencode", text);
            none("omp", text);
            none("codex", text);
        }
    }

    #[test]
    fn someone_elses_rate_limit_is_not_our_model() {
        // captured — Claude Code 2.1.246 ships this exact system reminder, and
        // opencode ships the GitHub 403 message. An agent whose shell step hits
        // GitHub's limiter must not lose its model over it.
        none(
            "claude-code",
            "GitHub API rate limit exceeded (5,000/hr shared across all tools \
             and agents). Run `gh api rate_limit --jq .resources`",
        );
        none(
            "opencode",
            "error: GitHub returned 403. This usually means GitHub is rate \
             limiting your requests.",
        );
        none(
            "omp",
            "too many requests to the npm registry, retry in 30 seconds",
        );
    }

    #[test]
    fn work_about_rate_limiting_is_not_a_rate_limit() {
        // The classifier only ever sees error channels, but a work item's own
        // wording can reach one (a failing test's name, a stderr line from the
        // code under test). Conjunctive patterns keep these out.
        for text in [
            "implement rate limiting for the checkout API",
            "test rate_limit::returns_429_when_over_budget ... FAILED",
            "assertion failed: quota.is_some()",
            "src/limits.rs:44: fn model_not_found_page() is unused",
        ] {
            none("claude-code", text);
            none("opencode", text);
        }
    }

    // --- retry-after ---------------------------------------------------------

    #[test]
    fn retry_after_forms() {
        // captured — HTTP header form, echoed by every harness here.
        assert_eq!(
            classify_model_failure("claude-code", "rate_limit_error; Retry-After: 42")
                .unwrap()
                .retry_after,
            Some(42)
        );
        // captured — opencode's fastify limiter: "retry in ${after}".
        assert_eq!(
            classify_model_failure("opencode", "Rate limit exceeded, retry in 90 seconds")
                .unwrap()
                .retry_after,
            Some(90)
        );
        // captured — pi test corpus: "please retry after 30 seconds".
        assert_eq!(
            classify_model_failure("pi", "insufficient_quota, please retry after 30 seconds")
                .unwrap()
                .retry_after,
            Some(30)
        );
        // captured — omp: "Try again in ~14 min."
        assert_eq!(
            classify_model_failure("omp", "usage_limit_reached. Try again in ~5 min.")
                .unwrap()
                .retry_after,
            Some(300)
        );
        // captured — opencode: "It will reset in 2 hours"
        assert_eq!(
            classify_model_failure("opencode", "usage limit reached. It will reset in 2 hours.")
                .unwrap()
                .retry_after,
            Some(7200)
        );
    }

    #[test]
    fn retry_after_is_absent_rather_than_guessed() {
        // captured — opencode renders "less than a minute" when the window is
        // short. No number follows the cue, so no number is invented.
        assert_eq!(
            classify_model_failure(
                "opencode",
                "usage limit reached. It will reset in less than a minute.",
            )
            .unwrap()
            .retry_after,
            None
        );
        // No cue at all.
        assert_eq!(
            classify_model_failure("claude-code", "You've reached your Fable 5 limit")
                .unwrap()
                .retry_after,
            None
        );
        // A cue with an unusable unit is not scavenged for a number.
        assert_eq!(
            classify_model_failure("pi", "insufficient_quota; retry in 3 lightyears")
                .unwrap()
                .retry_after,
            None
        );
    }

    #[test]
    fn retry_after_is_capped_at_a_day() {
        assert_eq!(
            classify_model_failure("pi", "quota exceeded; try again in 900 hours")
                .unwrap()
                .retry_after,
            Some(24 * 60 * 60)
        );
    }

    // --- ModelCtx ------------------------------------------------------------

    #[test]
    fn model_ctx_names_the_default_when_nothing_is_pinned() {
        let ctx = ModelCtx::new("claude-code", None);
        assert_eq!(ctx.model_name(), "claude-code default");
        let ctx = ModelCtx::new("claude-code", Some("claude-fable-5".into()));
        assert_eq!(ctx.model_name(), "claude-fable-5");
    }

    #[test]
    fn model_ctx_produces_both_shapes_of_the_same_fact() {
        let ctx = ModelCtx::new("claude-code", Some("claude-fable-5".into()));
        let text = "You've reached your Fable 5 limit";
        match ctx.outcome(text) {
            Some(Outcome::ModelUnavailable { model, .. }) => assert_eq!(model, "claude-fable-5"),
            other => panic!("expected ModelUnavailable, got {other:?}"),
        }
        let err = ctx.error(text).expect("error");
        assert!(err.is_model_unavailable());
        assert!(!err.is_unavailable(), "must not be the harness chain");
        assert!(ctx.outcome("tests failed").is_none());
        assert!(ctx.error("tests failed").is_none());
    }

    #[test]
    fn excerpt_keeps_the_matching_line_only() {
        let f = classify_model_failure(
            "claude-code",
            "line one\nYou've reached your Fable 5 limit\nline three",
        )
        .unwrap();
        assert!(f.reason.contains("Fable 5 limit"), "{}", f.reason);
        assert!(!f.reason.contains("line three"), "{}", f.reason);
    }

    #[test]
    fn excerpt_survives_a_lowercasing_that_changes_byte_length() {
        // `İ` (U+0130, two bytes) lowercases to `i̇` (three bytes): searching
        // the lowercase text and slicing the original with its offsets
        // panicked here — a harness line with one such letter took the adapter
        // down with it.
        let f = classify_model_failure(
            "claude-code",
            "İİİİ preamble\nİ You've reached your Fable 5 limit\nİ trailer",
        )
        .unwrap();
        assert!(
            f.reason.contains("You've reached your Fable 5 limit"),
            "{}",
            f.reason
        );
        assert!(!f.reason.contains("trailer"), "{}", f.reason);
    }
}

#[cfg(test)]
mod resume_tests {
    use super::*;

    fn shared(model: Option<&str>, effort: Option<Effort>) -> Shared {
        let (out_tx, _out_rx) = mpsc::channel(1);
        Shared::new(out_tx, "pi", model.map(str::to_string), "/work/here").at_effort(effort)
    }

    #[test]
    fn a_token_is_minted_once_the_harness_named_the_session_and_says_what_it_ran_with() {
        let session = shared(Some("claude-opus-5-5[1m]"), Some(Effort::Xhigh));
        assert_eq!(session.resume_token("pi"), None, "no id yet, no token");
        session.set_native_id("sess-1");
        session.set_transcript("/work/here/sess-1.jsonl");
        let token = session.resume_token("pi").unwrap();
        assert_eq!(token.adapter_id, "pi");
        assert_eq!(token.native_id, "sess-1");
        assert_eq!(token.cwd, std::path::PathBuf::from("/work/here"));
        assert_eq!(
            token.transcript_path,
            Some(std::path::PathBuf::from("/work/here/sess-1.jsonl"))
        );
        assert_eq!(token.model.as_deref(), Some("claude-opus-5-5[1m]"));
        assert_eq!(token.effort, Some(Effort::Xhigh));
    }

    #[test]
    fn a_session_launched_with_neither_says_neither() {
        let session = shared(None, None);
        session.set_native_id("sess-2");
        let token = session.resume_token("pi").unwrap();
        assert_eq!(token.model, None);
        assert_eq!(token.effort, None);
        assert_eq!(token.transcript_path, None);
    }

    /// Every facade of a session gone, the driver reads the end of its
    /// channel: the child — still running — is told to leave, its group
    /// ended with it, and the session ends *aborted*.
    #[tokio::test]
    async fn a_driver_whose_facades_are_all_gone_ends_the_child_and_says_aborted() {
        let mut spec = bisa_harness::proc::ProcSpec::new("sh");
        spec.args = vec!["-c".into(), "sleep 30".into()];
        let proc = bisa_harness::proc::ProcHandle::spawn(spec).unwrap();
        let (out_tx, out_rx) = mpsc::channel(8);
        let shared =
            Shared::new(out_tx, "test-harness", None, std::env::temp_dir()).in_group(proc.group());
        let driver = shared.for_driver();
        let watch = driver.clone();
        let task = tokio::spawn(drive_until(
            proc,
            out_rx,
            driver,
            |_, _| Drive::Continue,
            |_| Outcome::Completed,
        ));
        drop(shared);
        tokio::time::timeout(Duration::from_secs(20), task)
            .await
            .expect("the driver ends once its facades are gone")
            .unwrap();
        assert!(watch.is_ended());
        assert!(!watch.group.alive(), "the child's group is gone with it");
    }
}
