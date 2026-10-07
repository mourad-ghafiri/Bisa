//! Oh My Pi adapter — `omp --mode rpc`, the pi wire protocol plus RPC v2
//! framing (Oh My Pi's RPC document, `rpc.md` in its own docs): a `ready` frame at startup, opt-in
//! protocol negotiation, and lossless `rpc_chunk` reassembly for oversized
//! frames.
//!
//! Host tools and host URI schemes (the embedder-side inversion of control)
//! are not implemented: the Bisa goal operations reach the harness through
//! the MCP server, not as host tools.
//!
//! Effort: the RPC command `set_thinking_level`, written right after
//! `get_state` and before the first prompt — no CLI flag is documented for it
//! (https://github.com/can1357/oh-my-pi, its RPC reference, read 2026-09-29). It
//! is written again when a session is revived. Oh My Pi takes all six levels
//! and holds the one it is given to what the model can do, so the command
//! does not wait to hear which levels the session has.

use async_trait::async_trait;
use base64::Engine as _;
use bisa_core::HarnessCaps;
use bisa_harness::proc::{Line, ProcHandle, ProcSpec};
use bisa_harness::{
    BoxEventStream, Effort, HarnessAdapter, HarnessError, HarnessSession, InteractiveLaunch,
    LifecycleEvent, ModelInfo, Phase, ProbeResult, ProgressEvent, PromptInput, ResumeToken,
    SessionEvent, SessionSnapshot, SessionSpec, Steer,
};
use tokio::sync::mpsc;

use crate::pi_wire::{self, STATE_REQ_ID};
use crate::util::{self, Drive, OutMsg, Shared};

pub const ADAPTER_ID: &str = "omp";

/// Hard ceiling on chunk reassembly regardless of what the server advertises.
const MAX_REASSEMBLY_BYTES: usize = 64 * 1024 * 1024;

pub struct OmpAdapter {
    pub program: String,
    pub extra_args: Vec<String>,
}

impl Default for OmpAdapter {
    fn default() -> Self {
        Self {
            program: "omp".into(),
            extra_args: vec![],
        }
    }
}

/// Reassembly state for one `rpc_chunk` sequence.
#[derive(Default)]
struct ChunkState {
    chunk_id: String,
    next_index: u64,
    count: u64,
    expected_len: usize,
    buf: Vec<u8>,
}

#[async_trait]
impl HarnessAdapter for OmpAdapter {
    fn id(&self) -> &str {
        ADAPTER_ID
    }

    fn display_name(&self) -> &str {
        "Oh My Pi"
    }

    fn caps(&self) -> HarnessCaps {
        // No `SUBAGENTS`: omp can spawn them, but this wire mapping emits
        // nothing for one, and a capability is earned by what is emitted.
        HarnessCaps::STEER
            | HarnessCaps::FOLLOW_UP
            | HarnessCaps::RESUME
            | HarnessCaps::FORK
            | HarnessCaps::USAGE_REPORTING
            | HarnessCaps::EFFORT
    }

    fn efforts(&self, model: Option<&str>) -> Vec<Effort> {
        let _ = model;
        Effort::ALL.to_vec()
    }

    /// Every provider omp is signed into, with its windows — `omp usage --json`.
    async fn usage(&self) -> bisa_harness::UsageState {
        crate::usage::omp::read(&self.program, self.id()).await
    }

    /// `omp models` prints a box-drawn table whose first column is the id;
    /// every model takes all six efforts.
    async fn models(&self) -> Vec<ModelInfo> {
        util::models_from_command(&self.program, &["models"], listed_model).await
    }

    /// The bare CLI. `launch` above runs it in `--mode rpc`; this is the command a
    /// person types. `extra_args` comes along because it is how a caller points
    /// this adapter at a wrapper or a stub, and an interactive session that
    /// ignored it would be a different program from the one being probed.
    ///
    /// `--continue`, not `--resume`: in `omp --help`, `-c, --continue` is
    /// "Continue previous session", while `-r, --resume=<value>` is "Resume a
    /// session (by ID prefix, path, or picker if omitted)" — and a person
    /// opening a terminal has no id to give, so the bare `--resume` was a
    /// picker waiting for a choice, not a session picking its work up.
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

/// One row of `omp models` as a listed model.
fn listed_model(line: &str) -> Option<ModelInfo> {
    // Data rows look like: "│ id │ context │ … │"
    let cells: Vec<&str> = line.split('│').collect();
    if cells.len() < 3 {
        return None;
    }
    let id = cells[1].trim();
    let looks_like_id = !id.is_empty()
        && id != "model"
        && !id.starts_with('─')
        && id.chars().all(|c| !c.is_whitespace());
    looks_like_id.then(|| ModelInfo::new(id, None, Effort::ALL.to_vec()))
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

/// What a session is told as it opens, before any prompt, in order: the
/// question that names it, then its effort when one is set.
fn opening(spec: &SessionSpec) -> Vec<serde_json::Value> {
    let mut lines = vec![pi_wire::command(Some(STATE_REQ_ID), "get_state", None)];
    if let Some(level) = spec.effort {
        lines.push(pi_wire::set_thinking_level(level));
    }
    lines
}

impl OmpAdapter {
    async fn spawn_session(
        &self,
        spec: &SessionSpec,
        session_arg: Option<&str>,
        revived: bool,
    ) -> Result<Box<dyn HarnessSession>, HarnessError> {
        let mut proc_spec = ProcSpec::new(&self.program).cwd(&spec.cwd);
        for arg in &self.extra_args {
            proc_spec = proc_spec.arg(arg);
        }
        proc_spec = proc_spec.arg("--mode").arg("rpc");
        if let Some(model) = &spec.model {
            proc_spec = proc_spec.arg("--model").arg(model);
        }
        if let Some(session) = session_arg {
            proc_spec = proc_spec.arg("--session").arg(session);
        }
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

        let mut chunk: Option<ChunkState> = None;
        let shared = shared.in_group(proc.group());
        let driver_shared = shared.for_driver();
        tokio::spawn(util::drive(
            proc,
            out_rx,
            driver_shared,
            move |shared, line| {
                let value = match line {
                    Line::Json(v) => v,
                    Line::Text(t) => {
                        tracing::debug!(target: "bisa_adapters::omp", "non-json stdout: {t}");
                        return Drive::Continue;
                    }
                };
                match value.get("type").and_then(|t| t.as_str()) {
                    Some("ready") => {
                        let supports_v2 = value
                            .get("supportedProtocolVersions")
                            .and_then(|v| v.as_array())
                            .is_some_and(|versions| versions.iter().any(|v| v.as_u64() == Some(2)));
                        if supports_v2 {
                            let negotiate = serde_json::json!({
                                "id": "bisa-proto", "type": "negotiate_protocol", "protocolVersion": 2
                            });
                            if let Err(e) = shared.try_send(OutMsg::Json(negotiate)) {
                                tracing::warn!("omp: the protocol negotiation was not queued: {e}");
                            }
                        }
                        shared.broadcaster.emit(SessionEvent::Raw(value));
                        Drive::Continue
                    }
                    Some("rpc_chunk") => match reassemble(&mut chunk, &value) {
                        Ok(Some(complete)) => pi_wire::map_pi_event(shared, complete),
                        Ok(None) => Drive::Continue,
                        Err(reason) => {
                            tracing::warn!(target: "bisa_adapters::omp", "chunk sequence rejected: {reason}");
                            chunk = None;
                            Drive::Continue
                        }
                    },
                    _ => {
                        if chunk.is_some() {
                            // Interleaved non-chunk frame inside a sequence: the
                            // spec says reject the sequence.
                            tracing::warn!(target: "bisa_adapters::omp", "chunk sequence interrupted; dropping");
                            chunk = None;
                        }
                        pi_wire::map_pi_event(shared, value)
                    }
                }
            },
        ));

        let session = OmpSession { shared };
        session.open(spec).await?;
        Ok(Box::new(session))
    }
}

/// Feed one `rpc_chunk` frame; returns the reassembled JSON object when the
/// sequence completes.
fn reassemble(
    state: &mut Option<ChunkState>,
    frame: &serde_json::Value,
) -> Result<Option<serde_json::Value>, String> {
    let chunk_id = frame
        .get("chunkId")
        .and_then(|v| v.as_str())
        .ok_or("missing chunkId")?;
    let index = frame
        .get("index")
        .and_then(|v| v.as_u64())
        .ok_or("missing index")?;
    let count = frame
        .get("count")
        .and_then(|v| v.as_u64())
        .ok_or("missing count")?;
    let byte_length = frame
        .get("byteLength")
        .and_then(|v| v.as_u64())
        .ok_or("missing byteLength")? as usize;
    let data = frame
        .get("data")
        .and_then(|v| v.as_str())
        .ok_or("missing data")?;

    if byte_length > MAX_REASSEMBLY_BYTES {
        *state = None;
        return Err(format!(
            "byteLength {byte_length} exceeds reassembly ceiling"
        ));
    }

    let st = match state {
        None => {
            if index != 0 {
                return Err(format!("sequence starts at index {index}, expected 0"));
            }
            state.insert(ChunkState {
                chunk_id: chunk_id.to_string(),
                next_index: 0,
                count,
                expected_len: byte_length,
                buf: Vec::with_capacity(byte_length.min(MAX_REASSEMBLY_BYTES)),
            })
        }
        Some(st) => {
            if st.chunk_id != chunk_id {
                *state = None;
                return Err("interleaved chunk id".into());
            }
            if st.count != count || st.expected_len != byte_length {
                *state = None;
                return Err("inconsistent count/byteLength".into());
            }
            st
        }
    };
    if index != st.next_index {
        *state = None;
        return Err(format!("out-of-order chunk index {index}"));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| format!("bad base64: {e}"))?;
    if st.buf.len() + bytes.len() > st.expected_len {
        *state = None;
        return Err("chunk data exceeds declared byteLength".into());
    }
    st.buf.extend_from_slice(&bytes);
    st.next_index += 1;

    if st.next_index == st.count {
        let st = state.take().expect("state present");
        if st.buf.len() != st.expected_len {
            return Err(format!(
                "reassembled {} bytes, declared {}",
                st.buf.len(),
                st.expected_len
            ));
        }
        let text = String::from_utf8(st.buf).map_err(|e| format!("invalid utf-8: {e}"))?;
        let value = serde_json::from_str(&text).map_err(|e| format!("invalid json: {e}"))?;
        return Ok(Some(value));
    }
    Ok(None)
}

pub struct OmpSession {
    shared: Shared,
}

impl OmpSession {
    /// Open the session: what [`opening`] says, then the launch prompt when
    /// there is one — so the first turn already runs at the effort asked.
    async fn open(&self, spec: &SessionSpec) -> Result<(), HarnessError> {
        for line in opening(spec) {
            self.shared.send(OutMsg::Json(line)).await?;
        }
        if !spec.prompt.is_empty() {
            self.send_prompt(&spec.prompt).await?;
        }
        Ok(())
    }

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
impl HarnessSession for OmpSession {
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
            .terminate_group(bisa_harness::proc::ABORT_GRACE)
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
    fn the_terminal_form_continues_the_previous_session_and_never_opens_the_picker() {
        let launch = OmpAdapter::default().interactive().unwrap();
        assert_eq!(launch.resume_args, vec!["--continue".to_string()]);
    }

    fn spec(prompt: &str, effort: Option<Effort>) -> SessionSpec {
        SessionSpec {
            work_item: None,
            cwd: std::path::PathBuf::from("/work/here"),
            prompt: prompt.to_string(),
            model: None,
            effort,
            mcp_servers: vec![],
            env: Default::default(),
            env_remove: Vec::new(),
            tier_ceiling: bisa_core::ToolTier::Write,
            output_schema: None,
            skills: vec![],
        }
    }

    /// A session over a channel the test reads, with no process behind it.
    fn facade(effort: Option<Effort>) -> (OmpSession, mpsc::Receiver<OutMsg>) {
        let (out_tx, out_rx) = mpsc::channel(8);
        let shared =
            Shared::new(out_tx, ADAPTER_ID, Some("opus".into()), "/work/here").at_effort(effort);
        (OmpSession { shared }, out_rx)
    }

    fn written(out_rx: &mut mpsc::Receiver<OutMsg>) -> Vec<serde_json::Value> {
        let mut lines = Vec::new();
        while let Ok(msg) = out_rx.try_recv() {
            match msg {
                OutMsg::Json(v) => lines.push(v),
                other => panic!("{other:?}"),
            }
        }
        lines
    }

    #[tokio::test]
    async fn the_effort_is_written_after_get_state_and_before_the_first_prompt() {
        let (session, mut out_rx) = facade(Some(Effort::Xhigh));
        session
            .open(&spec("write the parser", Some(Effort::Xhigh)))
            .await
            .unwrap();
        assert_eq!(
            written(&mut out_rx),
            [
                serde_json::json!({ "id": STATE_REQ_ID, "type": "get_state" }),
                serde_json::json!({ "type": "set_thinking_level", "level": "xhigh" }),
                serde_json::json!({ "type": "prompt", "message": "write the parser" }),
            ]
        );
    }

    #[tokio::test]
    async fn no_effort_set_writes_no_command_and_no_prompt_writes_none_either() {
        let (session, mut out_rx) = facade(None);
        session.open(&spec("go", None)).await.unwrap();
        assert_eq!(
            written(&mut out_rx),
            [
                serde_json::json!({ "id": STATE_REQ_ID, "type": "get_state" }),
                serde_json::json!({ "type": "prompt", "message": "go" }),
            ]
        );
        // A session opened idle is still told its effort: the caller's first
        // prompt comes later, and runs at it.
        let (session, mut out_rx) = facade(Some(Effort::Low));
        session.open(&spec("", Some(Effort::Low))).await.unwrap();
        assert_eq!(
            written(&mut out_rx),
            [
                serde_json::json!({ "id": STATE_REQ_ID, "type": "get_state" }),
                serde_json::json!({ "type": "set_thinking_level", "level": "low" }),
            ]
        );
    }

    #[test]
    fn oh_my_pi_takes_all_six_levels_for_every_model() {
        let adapter = OmpAdapter::default();
        assert!(adapter.caps().contains(HarnessCaps::EFFORT));
        for model in [None, Some("anthropic/claude-opus-5-5"), Some("anything")] {
            assert_eq!(adapter.efforts(model), Effort::ALL, "{model:?}");
        }
        for level in Effort::ALL {
            assert_eq!(
                opening(&spec("", Some(level))),
                [
                    pi_wire::command(Some(STATE_REQ_ID), "get_state", None),
                    pi_wire::set_thinking_level(level),
                ]
            );
        }
        // A listed model says the same.
        let listed = listed_model("│ anthropic/claude-opus-5-5 │ 1M │").unwrap();
        assert_eq!(listed.id, "anthropic/claude-opus-5-5");
        assert_eq!(listed.efforts, Effort::ALL);
        assert_eq!(listed_model("│ model │ context │"), None);
        assert_eq!(listed_model("├───────┼───────┤"), None);
    }

    #[test]
    fn a_revived_session_is_given_the_model_and_the_effort_it_ran_with() {
        let token = ResumeToken {
            adapter_id: ADAPTER_ID.to_string(),
            native_id: "sess-1".into(),
            cwd: std::path::PathBuf::from("/work/here"),
            transcript_path: Some(std::path::PathBuf::from("/work/here/sess-1.jsonl")),
            model: Some("anthropic/claude-sonnet-5-5".into()),
            effort: Some(Effort::High),
        };
        let revived = revival_spec(&token, std::path::PathBuf::from("/work/here"));
        assert_eq!(revived.model, token.model);
        assert_eq!(revived.effort, Some(Effort::High));
        assert!(revived.prompt.is_empty());
        assert_eq!(
            opening(&revived),
            [
                pi_wire::command(Some(STATE_REQ_ID), "get_state", None),
                pi_wire::set_thinking_level(Effort::High),
            ]
        );
        // A session that ran with neither is revived with neither.
        let plain = ResumeToken {
            model: None,
            effort: None,
            ..token
        };
        let revived = revival_spec(&plain, std::path::PathBuf::from("/work/here"));
        assert_eq!(revived.model, None);
        assert_eq!(
            opening(&revived),
            [pi_wire::command(Some(STATE_REQ_ID), "get_state", None)]
        );
    }

    #[test]
    fn a_sessions_token_says_the_model_and_the_effort_it_was_launched_with() {
        let (session, _out_rx) = facade(Some(Effort::Max));
        assert_eq!(session.resume_token(), None, "not named yet");
        session.shared.set_native_id("sess-1");
        let token = session.resume_token().unwrap();
        assert_eq!(token.adapter_id, ADAPTER_ID);
        assert_eq!(token.model.as_deref(), Some("opus"));
        assert_eq!(token.effort, Some(Effort::Max));
    }
}
