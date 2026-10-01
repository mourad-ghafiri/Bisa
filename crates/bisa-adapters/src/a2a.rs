//! A2A (Agent2Agent) consume side — feature `a2a`.
//!
//! Adapter id pattern: `a2a:<base-url>` (e.g. `a2a:http://127.0.0.1:8080`),
//! resolved dynamically through the catalog's resolver hook. Speaks the A2A
//! v0.3.0 wire: agent card at `<base>/.well-known/agent-card.json`, JSON-RPC
//! 2.0 (`message/send`, `tasks/get`, `tasks/cancel`) at the card's `url`
//! (fallback `<base>/a2a`).
//!
//! The remote A2A task is driven by polling `tasks/get` (2s, backing off to
//! 10s while the state is unchanged) — v0.3 streaming is not consumed in v1.
//! A task that stops at `input-required` or `auth-required` is raised as an
//! input request (`HarnessCaps::INPUT_REQUESTS`), answered the way every
//! harness's is, and polling continues.
//!
//! On completion, when the work item contracts a structured result, the first
//! `data` part of the task's artifacts is submitted to the engine's intake
//! socket (the same JSONL `result_submit` the MCP layer uses); with no data
//! artifact, `{"text": <joined text parts>}` is submitted instead.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use bisa_core::sync::Locked;
use bisa_core::HarnessCaps;
use bisa_harness::catalog::AdapterResolver;
use bisa_harness::traits::{BoxEventStream, EventBroadcaster, HarnessAdapter, HarnessSession};
use bisa_harness::{
    HarnessError, InputAnswer, InputRequest, LifecycleEvent, McpServerConfig, Outcome, Phase,
    ProbeResult, ProgressEvent, PromptInput, ResumeToken, SessionCost, SessionEvent,
    SessionSnapshot, SessionSpec, Steer,
};
use bisa_http::Clients;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};

pub const ID_PREFIX: &str = "a2a:";

/// How long one call to a peer may take, connect and body included.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
/// The agent card is a health probe's read: short, so an unreachable peer is
/// known to be one before a task is offered to it.
const CARD_TIMEOUT: Duration = Duration::from_secs(3);

fn new_ulid() -> String {
    ulid::Ulid::from_datetime(std::time::SystemTime::now()).to_string()
}

pub struct A2aAdapter {
    id: String,
    base: String,
    /// The engine's clients — the proxy and the HTTP version the person
    /// chose — or the process's shared set for a peer resolved with none.
    http: Arc<Clients>,
    /// How often a task is asked after ([`PollPlan`]).
    poll: PollPlan,
}

/// How a remote task is polled: the first wait, how it lengthens while the
/// task's state stays the same, and where it stops lengthening. A state that
/// moves puts the wait back to the first. The defaults are a remote agent's —
/// seconds; a test against a loopback stub names milliseconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PollPlan {
    pub first: Duration,
    pub factor: f32,
    pub max: Duration,
}

impl Default for PollPlan {
    fn default() -> Self {
        Self {
            first: Duration::from_secs(2),
            factor: 1.5,
            max: Duration::from_secs(10),
        }
    }
}

impl PollPlan {
    /// The wait after this one: back to the first when the state moved,
    /// longer — never past `max`, never shorter than it was — when it did not.
    pub fn next(&self, delay: Duration, moved: bool) -> Duration {
        if moved {
            return self.first;
        }
        delay
            .mul_f32(self.factor.max(1.0))
            .min(self.max.max(self.first))
    }
}

impl A2aAdapter {
    /// `base` is the remote origin, e.g. `http://127.0.0.1:8080`.
    pub fn new(base: impl Into<String>) -> Self {
        let base = base.into();
        Self {
            id: format!("{ID_PREFIX}{base}"),
            base,
            http: Clients::shared(),
            poll: PollPlan::default(),
        }
    }

    /// The same peer, asked after on another plan.
    pub fn with_poll(mut self, poll: PollPlan) -> Self {
        self.poll = poll;
        self
    }

    /// The same peer over the engine's clients.
    pub fn with_http(mut self, http: Arc<Clients>) -> Self {
        self.http = http;
        self
    }

    /// Resolver for the catalog: `a2a:<http(s)-url>` ids, over the process's
    /// shared clients.
    pub fn resolve(id: &str) -> Option<Arc<dyn HarnessAdapter>> {
        Self::resolve_with(id, Clients::shared())
    }

    /// The resolver the catalog registers: every peer it makes goes through
    /// `http`, the engine's clients.
    pub fn resolver(http: Arc<Clients>) -> AdapterResolver {
        Arc::new(move |id: &str| Self::resolve_with(id, Arc::clone(&http)))
    }

    fn resolve_with(id: &str, http: Arc<Clients>) -> Option<Arc<dyn HarnessAdapter>> {
        let base = id.strip_prefix(ID_PREFIX)?;
        if !(base.starts_with("http://") || base.starts_with("https://")) {
            return None;
        }
        Some(Arc::new(A2aAdapter::new(base).with_http(http)))
    }

    async fn fetch_card(&self) -> Result<Value, String> {
        let url = format!(
            "{}/.well-known/agent-card.json",
            self.base.trim_end_matches('/')
        );
        let resp = self
            .http
            .outbound()
            .get(&url)
            .timeout(CARD_TIMEOUT)
            .send()
            .await
            .map_err(|e| format!("agent card unreachable: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("agent card HTTP {}", resp.status()));
        }
        resp.json::<Value>()
            .await
            .map_err(|e| format!("agent card not JSON: {e}"))
    }

    async fn endpoint(&self) -> Result<String, String> {
        let card = self.fetch_card().await?;
        Ok(card
            .get("url")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("{}/a2a", self.base.trim_end_matches('/'))))
    }
}

#[async_trait]
impl HarnessAdapter for A2aAdapter {
    fn id(&self) -> &str {
        &self.id
    }

    fn display_name(&self) -> &str {
        "Remote A2A agent"
    }

    fn caps(&self) -> HarnessCaps {
        // Honest: no steering/follow-up on the 0.3 polling wire in v1. A task
        // that stops on `input-required` / `auth-required` is an input request
        // answered with the next `message/send`.
        HarnessCaps::RESUME | HarnessCaps::INPUT_REQUESTS
    }

    async fn probe(&self) -> ProbeResult {
        match self.fetch_card().await {
            Ok(card) => ProbeResult::available(
                card.get("protocolVersion")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            ),
            Err(reason) => ProbeResult::unavailable(reason),
        }
    }

    async fn launch(&self, spec: SessionSpec) -> Result<Box<dyn HarnessSession>, HarnessError> {
        let endpoint = self.endpoint().await.map_err(HarnessError::Unavailable)?;
        let session = A2aSession::new(
            self.id.clone(),
            endpoint,
            Arc::clone(&self.http),
            intake_target(&spec),
            spec.model.clone(),
            spec.cwd.clone(),
            self.poll,
        );
        session
            .shared
            .broadcaster
            .emit(SessionEvent::Lifecycle(LifecycleEvent::Started));
        // M1 contract: non-empty launch prompt auto-sends; empty starts idle.
        if !spec.prompt.is_empty() {
            session.send(&spec.prompt).await?;
        }
        Ok(Box::new(session))
    }

    async fn attach(&self, token: &ResumeToken) -> Result<Box<dyn HarnessSession>, HarnessError> {
        let endpoint = self.endpoint().await.map_err(HarnessError::Unavailable)?;
        // Limitation (documented): the resume token carries no intake target,
        // so a revived session completes without submitting a result.
        let session = A2aSession::new(
            self.id.clone(),
            endpoint,
            Arc::clone(&self.http),
            None,
            None,
            token.cwd.clone(),
            self.poll,
        );
        *session.shared.task_id.locked() = Some(token.native_id.clone());
        session.shared.set_phase(Phase::Turn);
        session
            .shared
            .broadcaster
            .emit(SessionEvent::Lifecycle(LifecycleEvent::Revived));
        session.spawn_poll();
        Ok(Box::new(session))
    }
}

/// `(socket path, work item)` parsed from the injected Bisa MCP server
/// config (`… mcp --socket <path> --work-item <ulid>` — the engine's
/// `build_session_spec` arg layout).
fn intake_target(spec: &SessionSpec) -> Option<(PathBuf, String)> {
    for mount in &spec.mcp_servers {
        let McpServerConfig::Stdio { args, .. } = &mount.config else {
            continue;
        };
        let mut socket = None;
        let mut work_item = None;
        let mut it = args.iter();
        while let Some(a) = it.next() {
            match a.as_str() {
                "--socket" => socket = it.next().cloned(),
                "--work-item" => work_item = it.next().cloned(),
                _ => {}
            }
        }
        if let (Some(s), Some(w)) = (socket, work_item) {
            return Some((PathBuf::from(s), w));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Session
// ---------------------------------------------------------------------------

struct SharedState {
    revision: std::sync::atomic::AtomicU64,
    phase: Mutex<Phase>,
    activity: Mutex<Option<String>>,
    task_id: Mutex<Option<String>>,
    /// The task id the remote stopped on, while it waits for input.
    pending_input: Mutex<Option<String>>,
    broadcaster: EventBroadcaster,
}

impl SharedState {
    fn set_phase(&self, phase: Phase) {
        let mut p = self.phase.locked();
        if *p != phase {
            *p = phase;
            self.revision
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    fn set_activity(&self, a: impl Into<String>) {
        *self.activity.locked() = Some(a.into());
        self.revision
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    fn phase(&self) -> Phase {
        *self.phase.locked()
    }
}

struct A2aSession {
    adapter_id: String,
    endpoint: String,
    http: Arc<Clients>,
    intake: Option<(PathBuf, String)>,
    shared: Arc<SharedState>,
    poll: Mutex<Option<tokio::task::JoinHandle<()>>>,
    plan: PollPlan,
    /// Harness + model, for naming a model failure. See [`crate::util`].
    model_ctx: crate::util::ModelCtx,
    /// The launch's working directory, carried so the resume token can state
    /// it. **Nothing here is placed by it** — an A2A agent runs on another
    /// machine and this adapter spawns no local process. It is recorded
    /// because a token that omitted it would be the one token whose placement
    /// a caller could not check, and "this session ran nowhere local" is a
    /// fact worth being able to read rather than infer from a missing field.
    cwd: PathBuf,
}

impl A2aSession {
    fn new(
        adapter_id: String,
        endpoint: String,
        http: Arc<Clients>,
        intake: Option<(PathBuf, String)>,
        model: Option<String>,
        cwd: PathBuf,
        plan: PollPlan,
    ) -> Self {
        Self {
            plan,
            model_ctx: crate::util::ModelCtx::new(adapter_id.clone(), model),
            adapter_id,
            endpoint,
            http,
            intake,
            cwd,
            shared: Arc::new(SharedState {
                revision: std::sync::atomic::AtomicU64::new(1),
                phase: Mutex::new(Phase::Idle),
                activity: Mutex::new(None),
                task_id: Mutex::new(None),
                pending_input: Mutex::new(None),
                broadcaster: EventBroadcaster::default(),
            }),
            poll: Mutex::new(None),
        }
    }

    /// One JSON-RPC round trip.
    ///
    /// This is the one adapter in the crate whose `launch()` is genuinely
    /// synchronous, so it is also the one that can return
    /// [`HarnessError::ModelUnavailable`] rather than reporting the same fact
    /// through the event stream. Two signals, typed first:
    ///
    /// 1. **HTTP 429** with its `Retry-After` header — no prose involved.
    /// 2. the JSON-RPC `error` object's message, through the shared classifier.
    async fn rpc(&self, method: &str, params: Value) -> Result<Value, HarnessError> {
        let body = json!({"jsonrpc": "2.0", "id": new_ulid(), "method": method, "params": params});
        let resp = self
            .http
            .outbound()
            .post(&self.endpoint)
            .timeout(REQUEST_TIMEOUT)
            .json(&body)
            .send()
            .await
            .map_err(|e| HarnessError::Protocol(format!("{method}: {e}")))?;
        if resp.status().as_u16() == 429 {
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok());
            return Err(HarnessError::model_unavailable(
                self.model_ctx.model_name(),
                "remote agent rate limited (HTTP 429)",
                retry_after,
            ));
        }
        let v: Value = resp
            .json()
            .await
            .map_err(|e| HarnessError::Protocol(format!("{method}: bad JSON: {e}")))?;
        if let Some(err) = v.get("error") {
            let message = err
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| err.to_string());
            if let Some(e) = self.model_ctx.error(&message) {
                return Err(e);
            }
            return Err(HarnessError::Protocol(format!("{method}: {err}")));
        }
        Ok(v.get("result").cloned().unwrap_or(Value::Null))
    }

    async fn send(&self, text: &str) -> Result<(), HarnessError> {
        let task_id = self.shared.task_id.locked().clone();
        let mut message = json!({
            "role": "user",
            "messageId": new_ulid(),
            "parts": [{"kind": "text", "text": text}],
        });
        if let Some(tid) = task_id {
            message["taskId"] = json!(tid);
        }
        let task = self
            .rpc("message/send", json!({"message": message}))
            .await?;
        if let Some(id) = task.get("id").and_then(Value::as_str) {
            *self.shared.task_id.locked() = Some(id.to_string());
        }
        self.shared.set_phase(Phase::Turn);
        self.shared
            .broadcaster
            .emit(SessionEvent::Progress(ProgressEvent::TurnStarted));
        self.spawn_poll();
        Ok(())
    }

    fn spawn_poll(&self) {
        let shared = Arc::clone(&self.shared);
        let http = Arc::clone(&self.http);
        let endpoint = self.endpoint.clone();
        let intake = self.intake.clone();
        let model_ctx = self.model_ctx.clone();
        let plan = self.plan;
        let handle = tokio::spawn(async move {
            poll_task(shared, http, endpoint, intake, model_ctx, plan).await;
        });
        let mut slot = self.poll.locked();
        if let Some(old) = slot.replace(handle) {
            old.abort();
        }
    }
}

/// Poll `tasks/get` until terminal, on the session's [`PollPlan`]: by default
/// a 2 s wait, lengthening ×1.5 to 10 s while the state is unchanged, back to
/// 2 s when it moves.
async fn poll_task(
    shared: Arc<SharedState>,
    http: Arc<Clients>,
    endpoint: String,
    intake: Option<(PathBuf, String)>,
    model_ctx: crate::util::ModelCtx,
    plan: PollPlan,
) {
    let mut delay = plan.first;
    let mut last_state = String::new();
    let mut failures = 0u32;
    loop {
        tokio::time::sleep(delay).await;
        let task_id = match shared.task_id.locked().clone() {
            Some(t) => t,
            None => return,
        };
        let body = json!({
            "jsonrpc": "2.0", "id": new_ulid(),
            "method": "tasks/get", "params": {"id": task_id},
        });
        let task = match http
            .outbound()
            .post(&endpoint)
            .timeout(REQUEST_TIMEOUT)
            .json(&body)
            .send()
            .await
        {
            Ok(resp) => match resp.json::<Value>().await {
                Ok(v) if v.get("error").is_none() => {
                    failures = 0;
                    v.get("result").cloned().unwrap_or(Value::Null)
                }
                Ok(v) => {
                    shared.set_phase(Phase::Ended);
                    shared.broadcaster.emit_failure(format!(
                        "tasks/get error: {}",
                        v.get("error").cloned().unwrap_or(Value::Null)
                    ));
                    return;
                }
                Err(e) => {
                    failures += 1;
                    if failures >= 5 {
                        shared.set_phase(Phase::Ended);
                        shared.broadcaster.emit_failure(format!("tasks/get: {e}"));
                        return;
                    }
                    continue;
                }
            },
            Err(e) => {
                failures += 1;
                if failures >= 5 {
                    shared.set_phase(Phase::Ended);
                    shared
                        .broadcaster
                        .emit_failure(format!("remote unreachable: {e}"));
                    return;
                }
                continue;
            }
        };
        let state = task
            .pointer("/status/state")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let moved = state != last_state;
        delay = plan.next(delay, moved);
        if moved {
            last_state = state.clone();
            shared.set_activity(format!("a2a task {state}"));
            shared
                .broadcaster
                .emit(SessionEvent::Raw(json!({"a2a_state": state})));
        }
        match state.as_str() {
            "completed" => {
                let submitted_ok = match &intake {
                    Some((socket, work_item)) => {
                        submit_result(socket, work_item, &task).await.map_err(|e| {
                            tracing::warn!("a2a result intake failed: {e}");
                            e
                        })
                    }
                    None => Ok(()),
                }
                .is_ok();
                shared
                    .broadcaster
                    .emit(SessionEvent::Progress(ProgressEvent::TurnEnded));
                shared.set_phase(Phase::Ended);
                shared
                    .broadcaster
                    .emit(SessionEvent::Lifecycle(LifecycleEvent::Ended {
                        outcome: if submitted_ok {
                            Outcome::Completed
                        } else {
                            Outcome::Failed {
                                error: "a2a task completed but result intake failed".into(),
                            }
                        },
                        is_terminal: true,
                    }));
                return;
            }
            // Mid-run site: the remote agent's own failure text. A model wall
            // there is the remote's model, but the retry is ours to make.
            "failed" | "rejected" => {
                let detail = task
                    .pointer("/status/message")
                    .map(|m| m.to_string())
                    .unwrap_or_else(|| format!("a2a task {state}"));
                shared.set_phase(Phase::Ended);
                match model_ctx.outcome(&detail) {
                    Some(outcome) => {
                        shared
                            .broadcaster
                            .emit(SessionEvent::Lifecycle(LifecycleEvent::Ended {
                                outcome,
                                is_terminal: true,
                            }));
                    }
                    None => shared.broadcaster.emit_failure(detail),
                }
                return;
            }
            "canceled" => {
                shared.set_phase(Phase::Ended);
                shared
                    .broadcaster
                    .emit(SessionEvent::Lifecycle(LifecycleEvent::Ended {
                        outcome: Outcome::Aborted,
                        is_terminal: true,
                    }));
                return;
            }
            // The remote stopped for a person: a question, or a sign-in it
            // cannot do itself. Surfaced once per state change; the answer is
            // the next `message/send` on the task (`answer` below).
            "input-required" | "auth-required" => {
                let already = shared.pending_input.locked().is_some();
                if !already {
                    *shared.pending_input.locked() = Some(task_id.clone());
                    let text = task
                        .pointer("/status/message/parts")
                        .and_then(Value::as_array)
                        .map(|parts| {
                            parts
                                .iter()
                                .filter_map(|p| p.get("text").and_then(Value::as_str))
                                .collect::<Vec<_>>()
                                .join(" ")
                        })
                        .filter(|t| !t.trim().is_empty());
                    let request = if state == "auth-required" {
                        InputRequest::auth(task_id.clone(), "the remote agent", text)
                    } else {
                        InputRequest::question(
                            task_id.clone(),
                            text.unwrap_or_else(|| "The remote agent needs more input".to_string()),
                            vec![],
                        )
                    };
                    shared.set_phase(Phase::AwaitingInput);
                    shared.broadcaster.emit(SessionEvent::Lifecycle(
                        LifecycleEvent::InputRequested { request },
                    ));
                }
            }
            _ => {}
        }
    }
}

/// Submit the completed task's payload to the engine's intake socket using
/// the JSONL `result_submit` op (kept dependency-free of bisa-mcp).
async fn submit_result(
    socket: &std::path::Path,
    work_item: &str,
    task: &Value,
) -> Result<(), String> {
    let empty = Vec::new();
    let artifacts = task
        .get("artifacts")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let data_part = artifacts
        .iter()
        .flat_map(|a| {
            a.get("parts")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .find(|p| p.get("kind").and_then(Value::as_str) == Some("data"))
        .and_then(|p| p.get("data").cloned());
    let output = data_part.unwrap_or_else(|| {
        let text = artifacts
            .iter()
            .flat_map(|a| {
                a.get("parts")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
            })
            .filter(|p| p.get("kind").and_then(Value::as_str) == Some("text"))
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n");
        json!({"text": text})
    });

    let stream = tokio::net::UnixStream::connect(socket)
        .await
        .map_err(|e| format!("intake connect: {e}"))?;
    let (read, mut write) = stream.into_split();
    let line = json!({"op": "result_submit", "work_item": work_item, "output": output});
    write
        .write_all(format!("{line}\n").as_bytes())
        .await
        .map_err(|e| format!("intake write: {e}"))?;
    let mut reply = String::new();
    BufReader::new(read)
        .read_line(&mut reply)
        .await
        .map_err(|e| format!("intake read: {e}"))?;
    let reply: Value =
        serde_json::from_str(reply.trim()).map_err(|e| format!("intake reply: {e}"))?;
    if reply.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(())
    } else {
        Err(format!("intake rejected: {reply}"))
    }
}

#[async_trait]
impl HarnessSession for A2aSession {
    fn snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            revision: self
                .shared
                .revision
                .load(std::sync::atomic::Ordering::Relaxed),
            phase: self.shared.phase(),
            activity: self.shared.activity.locked().clone(),
            cost: SessionCost::default(), // remote agents don't report cost on this wire
        }
    }

    fn phase(&self) -> Phase {
        self.shared.phase()
    }

    async fn prompt(&self, input: PromptInput) -> Result<(), HarnessError> {
        match self.shared.phase() {
            Phase::Turn => Err(HarnessError::Busy),
            Phase::Ended => Err(HarnessError::Terminated),
            _ => self.send(&input.text).await,
        }
    }

    async fn steer(&self, _msg: Steer) -> Result<(), HarnessError> {
        Err(HarnessError::NotSupported("steer"))
    }

    async fn follow_up(&self, _msg: Steer) -> Result<(), HarnessError> {
        Err(HarnessError::NotSupported("follow_up"))
    }

    async fn answer(&self, request_id: &str, answer: InputAnswer) -> Result<(), HarnessError> {
        let pending = self.shared.pending_input.locked().take();
        if pending.as_deref() != Some(request_id) {
            return Err(HarnessError::protocol(format!(
                "no input request {request_id} is waiting"
            )));
        }
        match answer {
            // The person's words go to the remote as the next message on the task.
            InputAnswer::Text { text } => self.send(&text).await?,
            // A sign-in done out of band: the task resumes on its own; keep polling.
            InputAnswer::Allow { .. } => {
                self.shared.set_phase(Phase::Turn);
                self.spawn_poll();
            }
            InputAnswer::Deny { .. } => return self.abort().await,
        }
        self.shared
            .broadcaster
            .emit(SessionEvent::Lifecycle(LifecycleEvent::InputResolved {
                id: request_id.to_string(),
            }));
        Ok(())
    }

    async fn abort(&self) -> Result<(), HarnessError> {
        let task_id = { self.shared.task_id.locked().clone() };
        if let Some(task_id) = task_id {
            if let Err(e) = self.rpc("tasks/cancel", json!({"id": task_id})).await {
                tracing::debug!("tasks/cancel was not taken; the poll ends anyway: {e}");
            }
        }
        if let Some(handle) = self.poll.locked().take() {
            handle.abort();
        }
        self.shared.set_phase(Phase::Ended);
        self.shared
            .broadcaster
            .emit(SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: Outcome::Aborted,
                is_terminal: true,
            }));
        Ok(())
    }

    fn subscribe(&self) -> BoxEventStream {
        self.shared.broadcaster.subscribe()
    }

    fn resume_token(&self) -> Option<ResumeToken> {
        let task_id = self.shared.task_id.locked().clone()?;
        Some(ResumeToken {
            adapter_id: self.adapter_id.clone(),
            native_id: task_id,
            cwd: self.cwd.clone(),
            transcript_path: None,
            model: None,
            effort: None,
        })
    }

    async fn dispose(self: Box<Self>) -> Result<(), HarnessError> {
        if let Some(handle) = self.poll.locked().take() {
            handle.abort();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wait_lengthens_while_nothing_moves_stops_at_its_cap_and_starts_over_when_something_does() {
        let plan = PollPlan::default();
        let secs = |d: Duration| (d.as_secs_f32() * 100.0).round() / 100.0;
        let mut delay = plan.first;
        let mut waits = vec![secs(delay)];
        for _ in 0..6 {
            delay = plan.next(delay, false);
            waits.push(secs(delay));
        }
        assert_eq!(waits, vec![2.0, 3.0, 4.5, 6.75, 10.0, 10.0, 10.0]);
        assert_eq!(
            plan.next(delay, true),
            plan.first,
            "a state that moved is asked after soon again"
        );
        // A plan nobody should write still never spins and never shrinks.
        let odd = PollPlan {
            first: Duration::from_millis(5),
            factor: 0.1,
            max: Duration::from_millis(1),
        };
        assert_eq!(
            odd.next(Duration::from_millis(5), false),
            Duration::from_millis(5)
        );
        assert!(odd.next(Duration::ZERO, true) > Duration::ZERO);
    }
}
