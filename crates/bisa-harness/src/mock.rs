//! Mock adapter/session for tests (this crate's and, via the `mock` feature,
//! other crates').

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use bisa_core::HarnessCaps;

use crate::error::HarnessError;
use crate::event::{
    InputAnswer, InputRequest, LifecycleEvent, Outcome, ProgressEvent, SessionEvent, SubagentId,
};
use crate::traits::{BoxEventStream, EventBroadcaster, HarnessAdapter, HarnessSession};
use crate::types::{
    Effort, InteractiveLaunch, ModelInfo, ModelPlan, Phase, ProbeResult, PromptInput,
    ReportingContext, ReportingPlan, ResumeToken, SessionCost, SessionSnapshot, SessionSpec, Steer,
};
use bisa_core::sync::Locked;

/// A scripted intake conversation: the JSONL requests a mock session sends
/// back to the engine when prompted, in order.
///
/// This is how a mock stands in for a real harness's MCP tool calls without
/// an LLM. The session learns the socket path and its scope exactly the way a
/// real harness does — by reading the `bisa` MCP server's argv out of
/// [`SessionSpec::mcp_servers`] — then speaks the intake wire protocol
/// directly.
///
/// Requests may use placeholders, substituted recursively in every string:
///
/// - `{{goal}}` / `{{work_item}}` — this session's scope, from its spec
/// - `{{gate}}` — the `gate` id from the most recent reply that carried one,
///   so `ask_human` can be followed by a blocking `await_decision`
#[derive(Clone, Default)]
pub struct IntakeScript {
    pub requests: Vec<serde_json::Value>,
    /// Replies, in order — tests assert against these after the run.
    pub replies: Arc<Mutex<Vec<serde_json::Value>>>,
}

impl IntakeScript {
    pub fn new(requests: Vec<serde_json::Value>) -> Self {
        Self {
            requests,
            replies: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// The recorded replies so far.
    pub fn replies(&self) -> Vec<serde_json::Value> {
        self.replies.locked().clone()
    }
}

/// The intake socket path and scope flags a session was launched with.
fn intake_target(spec: &SessionSpec) -> Option<(String, Option<String>, Option<String>)> {
    for mount in &spec.mcp_servers {
        let crate::types::McpServerConfig::Stdio { name, args, .. } = &mount.config else {
            continue;
        };
        if name != "bisa" {
            continue;
        }
        let flag = |f: &str| {
            args.iter()
                .position(|a| a == f)
                .and_then(|i| args.get(i + 1))
                .cloned()
        };
        if let Some(socket) = flag("--socket") {
            return Some((socket, flag("--goal"), flag("--work-item")));
        }
    }
    None
}

/// Substitute `{{…}}` placeholders throughout a request.
fn fill(value: &serde_json::Value, goal: &str, work_item: &str, gate: &str) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) => serde_json::Value::String(
            s.replace("{{goal}}", goal)
                .replace("{{work_item}}", work_item)
                .replace("{{gate}}", gate),
        ),
        serde_json::Value::Array(items) => serde_json::Value::Array(
            items
                .iter()
                .map(|v| fill(v, goal, work_item, gate))
                .collect(),
        ),
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), fill(v, goal, work_item, gate)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Run a scripted intake conversation over the engine's unix socket.
async fn run_intake_script(script: IntakeScript, spec: SessionSpec) {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let Some((socket, goal, work_item)) = intake_target(&spec) else {
        tracing::warn!("mock: no bisa MCP server in spec; intake script skipped");
        return;
    };
    let goal = goal.unwrap_or_default();
    let work_item = work_item.unwrap_or_default();
    let mut gate = String::new();

    for request in &script.requests {
        let request = fill(request, &goal, &work_item, &gate);
        let stream = match tokio::net::UnixStream::connect(&socket).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("mock: intake connect failed: {e}");
                return;
            }
        };
        let (read, mut write) = stream.into_split();
        let mut line = request.to_string();
        line.push('\n');
        if write.write_all(line.as_bytes()).await.is_err() {
            return;
        }
        let mut reader = BufReader::new(read);
        let mut reply = String::new();
        if reader.read_line(&mut reply).await.is_err() {
            return;
        }
        let reply: serde_json::Value =
            serde_json::from_str(reply.trim()).unwrap_or(serde_json::Value::Null);
        // Carry a freshly-opened gate forward so the next step can await it.
        if let Some(g) = reply.get("gate").and_then(|g| g.as_str()) {
            if !g.is_empty() {
                gate = g.to_string();
            }
        }
        script.replies.locked().push(reply);
    }
}

/// How a mock harness refuses a model — the three shapes a real one can take.
///
/// See `bisa-adapters`'s "Launch versus mid-run": a subprocess harness
/// cannot report a dead model synchronously, so [`DeadModel::NoProgress`] is
/// what a *failed launch* actually looks like to the engine, and
/// [`DeadModel::AtLaunch`] only happens for adapters whose `launch` is a real
/// round trip (today, a2a).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadModel {
    /// `launch()` returns [`HarnessError::ModelUnavailable`].
    AtLaunch,
    /// The session launches, and its first lifecycle event is a **terminal**
    /// [`Outcome::ModelUnavailable`] with no progress before it.
    NoProgress,
    /// The session runs a tool and emits text, *then* hits the wall. A retry
    /// here may find a partial edit already on disk.
    MidRun,
    /// The session is over before `launch` returns — a terminal
    /// [`Outcome::ModelUnavailable`] said with nobody listening yet — and it
    /// takes its first prompt all the same. What an agent that speaks a
    /// protocol does when it is asked, in its handshake, for a model it does
    /// not offer: the wall is the session's end, heard by whoever subscribes
    /// afterwards, and never the prompt's refusal.
    BeforeListening,
}

/// One `(model, how it dies)` rule for [`MockAdapter::dead_models`].
#[derive(Debug, Clone)]
pub struct ModelFailure {
    pub model: String,
    pub how: DeadModel,
    pub reason: String,
    pub retry_after: Option<u64>,
    /// Die only for the first `n` launches of this model, then behave. A
    /// quota that comes back is the normal case, and it is the only way to
    /// test that a cooldown *expires* rather than merely existing.
    pub first_n: Option<u32>,
}

impl ModelFailure {
    pub fn new(model: impl Into<String>, how: DeadModel) -> Self {
        Self {
            model: model.into(),
            how,
            reason: "quota exhausted".into(),
            retry_after: None,
            first_n: None,
        }
    }

    pub fn reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = reason.into();
        self
    }

    pub fn retry_after(mut self, secs: u64) -> Self {
        self.retry_after = Some(secs);
        self
    }

    /// Die for the first `n` launches only.
    pub fn only_first(mut self, n: u32) -> Self {
        self.first_n = Some(n);
        self
    }
}

pub struct MockAdapter {
    pub caps: HarnessCaps,
    /// What `usage()` answers; `None` is the trait's default — *unsupported*,
    /// in the mock's name.
    pub usage: Option<crate::usage::UsageState>,
    /// How many times `usage()` was asked — so a test can tell a cached
    /// answer from a fresh one without a global counter.
    pub usage_asked: Arc<std::sync::atomic::AtomicUsize>,
    pub available: bool,
    /// How long `probe()` takes to answer — a slow `--version`, for a test of
    /// what a listing does with a probe that outstays its budget.
    pub probe_delay: std::time::Duration,
    /// How many times `probe()` was asked — so a test can tell a cached
    /// listing from a fresh probe.
    pub probes: Arc<std::sync::atomic::AtomicUsize>,
    /// Adapter id override, so tests can register several mocks side by side.
    pub id: String,
    /// Models this harness will not run, and how it says so. Matched against
    /// [`SessionSpec::model`]; a `None` model matches the entry named
    /// `"<id> default"`, which is what the engine's ledger calls it.
    pub dead_models: Vec<ModelFailure>,
    /// Every spec this adapter was launched with, in order — so a test can
    /// assert the *order a strategy produced*, and the *directory a retry
    /// re-entered*, rather than only the outcome.
    pub launches: Arc<Mutex<Vec<SessionSpec>>>,
    /// Every prompt any of this adapter's sessions received, in order — what
    /// the engine actually told the agent, roster and directives included.
    pub prompts: Arc<Mutex<Vec<String>>>,
    /// Every follow-up any of this adapter's sessions received, in order —
    /// the steer path's text, so a test can read what a warm session was told.
    pub follow_ups: Arc<Mutex<Vec<String>>>,
    /// Delay before a turn produces anything, so a test can make a run cost
    /// wall-clock time it can then assert about.
    pub turn_delay: std::time::Duration,
    /// When set, `prompt` emits exactly these events (in order) instead of the
    /// default echo script. A script with no terminal `Ended` leaves the
    /// session running — useful for permission/abort tests.
    pub script: Option<Vec<SessionEvent>>,
    /// When set, `prompt` runs this intake conversation (as a real harness's
    /// tool calls would) before ending the turn. Combines with `script`:
    /// scripted events are emitted first, then the intake steps run.
    pub intake_script: Option<IntakeScript>,
    /// When true, a `steer` call ends the session with `Completed` — lets
    /// tests unblock a scripted session that is waiting on a decision.
    pub end_on_steer: bool,
    /// Panic inside `launch` — the shape of an adapter bug, for the engine's
    /// guards to be tested against a real unwind.
    pub panic_on_launch: bool,
    /// Panic inside `prompt` — an adapter bug after the session was
    /// registered, for the engine's row guard to be tested against a real
    /// unwind in a driver.
    pub panic_on_prompt: bool,
    /// Refuse the first prompt with `Terminated` — a harness that died
    /// between its launch and its first word, for the drivers' early
    /// returns.
    pub refuse_prompt: bool,
    /// Swallow `abort`: it is recorded and nothing ends — a harness that
    /// does not stop when told, for the engine's deadline and its
    /// termination of the process to be tested against.
    pub ignore_abort: bool,
    /// The process id every session of this mock announces at launch — as a
    /// long-lived adapter announces its child from its own task, before the
    /// engine listens — so a test can see the driver record it.
    pub pid: Option<u32>,
    /// When set, a turn raises this request right after it starts and parks
    /// until [`HarnessSession::answer`] is called; then `InputResolved` is
    /// emitted and the turn goes on with `script` (or the echo). What every
    /// session was answered is in [`Self::answered`].
    pub input_request: Option<InputRequest>,
    /// Every answer any of this adapter's sessions received, in order.
    pub answered: Arc<Mutex<Vec<(String, InputAnswer)>>>,
    /// The interactive form a person could open in a terminal, when the mock
    /// stands in for a harness that has one.
    pub interactive: Option<InteractiveLaunch>,
    /// What one interactive session of this mock reports with; the empty plan
    /// means it opens as a plain terminal.
    pub reporting: ReportingPlan,
    /// The models this mock lists, each with the efforts it takes. Empty is
    /// the trait's default: unknown.
    pub listed_models: Vec<ModelInfo>,
    /// The efforts this mock takes for a model [`Self::model_efforts`] has no
    /// entry for, and for no model at all. Empty is no control. A mock that
    /// lists any effort declares [`HarnessCaps::EFFORT`] whatever `caps` says.
    pub efforts: Vec<Effort>,
    /// Per model id, the efforts that model takes — so two models of one
    /// plan can take different levels, and a test can watch each attempt
    /// clamped to its own.
    pub model_efforts: Vec<(String, Vec<Effort>)>,
    /// The plan this mock recommends for an agent; `None` is no opinion.
    pub recommended_plan: Option<ModelPlan>,
    /// The model this mock recommends for judging; `None` is no opinion.
    pub recommended_judge: Option<String>,
    /// Every token a session was revived from, in order — so a test can
    /// assert the model and the effort a revived session came back with.
    pub attached: Arc<Mutex<Vec<ResumeToken>>>,
    /// How each of this adapter's sessions was closed from outside, in order
    /// — so a test can tell a harness that was told to stop from a roster
    /// row that only says so.
    pub closes: Arc<Mutex<Vec<Close>>>,
}

/// How a session was closed by whoever held it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Close {
    /// `abort()`: stopped at once, whatever it was doing.
    Aborted,
    /// `dispose()`: let go of.
    Disposed,
}

impl Default for MockAdapter {
    fn default() -> Self {
        Self {
            // The mock reads `spec.mcp_servers`, so it truthfully takes the
            // platform's tools: a conversation about a note or a drawing
            // frames them for it. A test wanting the no-tools frame passes
            // `caps: HarnessCaps::empty()`.
            caps: HarnessCaps::STEER
                | HarnessCaps::FOLLOW_UP
                | HarnessCaps::RESUME
                | HarnessCaps::INPUT_REQUESTS
                | HarnessCaps::SUBAGENTS
                | HarnessCaps::MCP_SERVERS,
            usage: None,
            usage_asked: Arc::default(),
            available: true,
            probe_delay: std::time::Duration::ZERO,
            probes: Arc::default(),
            id: "mock".into(),
            dead_models: Vec::new(),
            launches: Arc::new(Mutex::new(Vec::new())),
            prompts: Arc::new(Mutex::new(Vec::new())),
            follow_ups: Arc::new(Mutex::new(Vec::new())),
            turn_delay: std::time::Duration::ZERO,
            script: None,
            intake_script: None,
            end_on_steer: false,
            panic_on_launch: false,
            panic_on_prompt: false,
            refuse_prompt: false,
            ignore_abort: false,
            pid: None,
            input_request: None,
            answered: Arc::new(Mutex::new(Vec::new())),
            interactive: None,
            reporting: ReportingPlan::default(),
            listed_models: Vec::new(),
            efforts: Vec::new(),
            model_efforts: Vec::new(),
            recommended_plan: None,
            recommended_judge: None,
            attached: Arc::new(Mutex::new(Vec::new())),
            closes: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

/// The events a harness emits around one sub-agent: it starts, does one read
/// inside, ends well, and the turn completes. For tests of nesting.
pub fn subagent_script(id: &str, name: &str) -> Vec<SessionEvent> {
    let sub = SubagentId(id.to_string());
    vec![
        SessionEvent::Lifecycle(LifecycleEvent::Started),
        SessionEvent::Progress(ProgressEvent::TurnStarted),
        SessionEvent::Progress(ProgressEvent::SubagentStarted {
            id: sub.clone(),
            name: name.to_string(),
            description: "look around".into(),
        }),
        SessionEvent::Progress(
            ProgressEvent::ToolStarted {
                name: "Read".into(),
                args_summary: "README.md".into(),
                tier: bisa_core::ToolTier::Read,
                id: None,
            }
            .raised_by(Some(sub.clone())),
        ),
        SessionEvent::Progress(
            ProgressEvent::TextDelta {
                text: "the sub-agent's own words".into(),
            }
            .raised_by(Some(sub.clone())),
        ),
        SessionEvent::Progress(
            ProgressEvent::ToolEnded {
                name: "Read".into(),
                ok: true,
                id: None,
            }
            .raised_by(Some(sub.clone())),
        ),
        SessionEvent::Progress(ProgressEvent::SubagentEnded { id: sub, ok: true }),
        SessionEvent::Progress(ProgressEvent::TurnEnded),
        SessionEvent::Lifecycle(LifecycleEvent::Ended {
            outcome: Outcome::Completed,
            is_terminal: true,
        }),
    ]
}

impl MockAdapter {
    /// Every spec this adapter was launched with, oldest first.
    pub fn launches(&self) -> Vec<SessionSpec> {
        self.launches.locked().clone()
    }

    /// Every `(request id, answer)` this adapter's sessions received, oldest first.
    pub fn answered(&self) -> Vec<(String, InputAnswer)> {
        self.answered.locked().clone()
    }

    /// Every prompt this adapter's sessions received, oldest first.
    pub fn prompts(&self) -> Vec<String> {
        self.prompts.locked().clone()
    }

    /// Every follow-up this adapter's sessions received, oldest first.
    pub fn follow_ups(&self) -> Vec<String> {
        self.follow_ups.locked().clone()
    }

    /// The models this adapter was asked for, oldest first.
    pub fn launched_models(&self) -> Vec<Option<String>> {
        self.launches().into_iter().map(|s| s.model).collect()
    }

    /// The effort each launch carried, oldest first — beside
    /// [`Self::launched_models`], what each attempt of a walk was sent.
    pub fn launched_efforts(&self) -> Vec<Option<Effort>> {
        self.launches().into_iter().map(|s| s.effort).collect()
    }

    /// Every token a session was revived from, oldest first.
    pub fn attached(&self) -> Vec<ResumeToken> {
        self.attached.locked().clone()
    }

    /// How this adapter's sessions were closed from outside, oldest first.
    pub fn closes(&self) -> Vec<Close> {
        self.closes.locked().clone()
    }

    /// Whether this mock was given any effort to take.
    fn takes_effort(&self) -> bool {
        !self.efforts.is_empty() || self.model_efforts.iter().any(|(_, e)| !e.is_empty())
    }

    /// The ledger's name for a spec's model — the id, or "<adapter> default".
    fn asked_for(&self, model: Option<&str>) -> String {
        match model {
            Some(m) if !m.trim().is_empty() => m.to_string(),
            _ => format!("{} default", self.id),
        }
    }

    /// The rule for a launch spec's model, if this harness refuses it *this
    /// time*. Call after recording the launch: `first_n` counts inclusively.
    fn refusal(&self, model: Option<&str>) -> Option<ModelFailure> {
        let asked = self.asked_for(model);
        let rule = self.dead_models.iter().find(|d| d.model == asked)?;
        if let Some(n) = rule.first_n {
            let so_far = self
                .launches()
                .iter()
                .filter(|s| self.asked_for(s.model.as_deref()) == asked)
                .count() as u32;
            if so_far > n {
                return None;
            }
        }
        Some(rule.clone())
    }
}

#[async_trait::async_trait]
impl HarnessAdapter for MockAdapter {
    fn id(&self) -> &str {
        &self.id
    }

    fn display_name(&self) -> &str {
        "Mock Harness"
    }

    fn caps(&self) -> HarnessCaps {
        if self.takes_effort() {
            self.caps | HarnessCaps::EFFORT
        } else {
            self.caps
        }
    }

    async fn probe(&self) -> ProbeResult {
        self.probes
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if !self.probe_delay.is_zero() {
            tokio::time::sleep(self.probe_delay).await;
        }
        if self.available {
            ProbeResult::available(Some("mock 0.0.0".into()))
        } else {
            ProbeResult::unavailable("mock disabled")
        }
    }

    async fn models(&self) -> Vec<ModelInfo> {
        self.listed_models.clone()
    }

    fn efforts(&self, model: Option<&str>) -> Vec<Effort> {
        model
            .and_then(|m| self.model_efforts.iter().find(|(id, _)| id == m))
            .map(|(_, efforts)| efforts.clone())
            .unwrap_or_else(|| self.efforts.clone())
    }

    fn recommended_plan(&self) -> Option<ModelPlan> {
        self.recommended_plan.clone()
    }

    fn recommended_judge(&self) -> Option<String> {
        self.recommended_judge.clone()
    }

    fn interactive(&self) -> Option<InteractiveLaunch> {
        self.interactive.clone()
    }

    fn interactive_reporting(&self, _ctx: &ReportingContext) -> ReportingPlan {
        self.reporting.clone()
    }

    async fn usage(&self) -> crate::usage::UsageState {
        self.usage_asked
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.usage
            .clone()
            .unwrap_or_else(|| crate::usage::UsageState::unsupported(self.display_name()))
    }

    async fn launch(&self, spec: SessionSpec) -> Result<Box<dyn HarnessSession>, HarnessError> {
        if self.panic_on_launch {
            panic!("the mock adapter panicked on launch");
        }
        if !self.available {
            return Err(HarnessError::unavailable("mock disabled"));
        }
        self.launches.locked().push(spec.clone());
        let refusal = self.refusal(spec.model.as_deref());
        if let Some(r) = &refusal {
            if r.how == DeadModel::AtLaunch {
                return Err(HarnessError::model_unavailable(
                    r.model.clone(),
                    r.reason.clone(),
                    r.retry_after,
                ));
            }
        }
        let mut session = MockSession::new(self.caps, spec);
        session.prompts = Arc::clone(&self.prompts);
        session.followed_up = Arc::clone(&self.follow_ups);
        session.script = self.script.clone();
        session.intake_script = self.intake_script.clone();
        session.end_on_steer = self.end_on_steer;
        session.input_request = self.input_request.clone();
        session.answered = Arc::clone(&self.answered);
        session.adapter_id = self.id.clone();
        session.turn_delay = self.turn_delay;
        session.closes = Arc::clone(&self.closes);
        session.panic_on_prompt = self.panic_on_prompt;
        session.refuse_prompt = self.refuse_prompt;
        session.ignore_abort = self.ignore_abort;
        // Announced at launch, before anybody subscribes — the shape of a
        // long-lived adapter's child.
        if let Some(pid) = self.pid {
            session
                .broadcaster
                .emit(SessionEvent::Lifecycle(LifecycleEvent::ProcessStarted {
                    pid: Some(pid),
                }));
        }
        if let Some(r) = refusal
            .as_ref()
            .filter(|r| r.how == DeadModel::BeforeListening)
        {
            session.set_phase(Phase::Ended);
            session
                .broadcaster
                .emit(SessionEvent::Lifecycle(LifecycleEvent::Ended {
                    outcome: Outcome::ModelUnavailable {
                        model: r.model.clone(),
                        reason: r.reason.clone(),
                        retry_after: r.retry_after,
                    },
                    is_terminal: true,
                }));
        }
        session.refusal = refusal;
        Ok(Box::new(session))
    }

    async fn attach(&self, token: &ResumeToken) -> Result<Box<dyn HarnessSession>, HarnessError> {
        if !self.caps.contains(HarnessCaps::RESUME) {
            return Err(HarnessError::NotSupported("resume"));
        }
        self.attached.locked().push(token.clone());
        // A revived session runs the model and the effort it ran before.
        let mut spec_less = MockSession::new(
            self.caps,
            SessionSpec {
                work_item: None,
                cwd: std::path::PathBuf::from("."),
                prompt: String::new(),
                model: token.model.clone(),
                effort: token.effort,
                mcp_servers: vec![],
                env: Default::default(),
                env_remove: Vec::new(),
                tier_ceiling: bisa_core::ToolTier::Read,
                output_schema: None,
                skills: vec![],
            },
        );
        spec_less.native_id = token.native_id.clone();
        spec_less.adapter_id = self.id.clone();
        spec_less.closes = Arc::clone(&self.closes);
        Ok(Box::new(spec_less))
    }
}

pub struct MockSession {
    caps: HarnessCaps,
    native_id: String,
    phase: Arc<Mutex<Phase>>,
    revision: Arc<AtomicU64>,
    broadcaster: EventBroadcaster,
    pub steered: Arc<Mutex<Vec<String>>>,
    pub followed_up: Arc<Mutex<Vec<String>>>,
    /// Shared with the launching adapter: see [`MockAdapter::prompts`].
    pub prompts: Arc<Mutex<Vec<String>>>,
    /// See [`MockAdapter::script`].
    pub script: Option<Vec<SessionEvent>>,
    /// See [`MockAdapter::intake_script`].
    pub intake_script: Option<IntakeScript>,
    /// See [`MockAdapter::end_on_steer`].
    pub end_on_steer: bool,
    /// See [`MockAdapter::panic_on_prompt`].
    pub panic_on_prompt: bool,
    /// See [`MockAdapter::refuse_prompt`].
    pub refuse_prompt: bool,
    /// See [`MockAdapter::ignore_abort`].
    pub ignore_abort: bool,
    /// See [`MockAdapter::input_request`].
    pub input_request: Option<InputRequest>,
    /// See [`MockAdapter::answered`].
    pub answered: Arc<Mutex<Vec<(String, InputAnswer)>>>,
    /// The turn parked on an input request: where its answer goes.
    answer_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<InputAnswer>>>>,
    /// Adapter id echoed into resume tokens (matches the launching adapter).
    pub adapter_id: String,
    /// Set when this session's model is one the adapter refuses mid-stream.
    pub refusal: Option<ModelFailure>,
    /// See [`MockAdapter::turn_delay`].
    pub turn_delay: std::time::Duration,
    /// Shared with the launching adapter: see [`MockAdapter::closes`].
    pub closes: Arc<Mutex<Vec<Close>>>,
    spec: SessionSpec,
}

impl MockSession {
    pub fn new(caps: HarnessCaps, spec: SessionSpec) -> Self {
        Self {
            caps,
            native_id: "mock-session-1".into(),
            phase: Arc::new(Mutex::new(Phase::Idle)),
            revision: Arc::new(AtomicU64::new(1)),
            broadcaster: EventBroadcaster::default(),
            steered: Arc::new(Mutex::new(Vec::new())),
            followed_up: Arc::new(Mutex::new(Vec::new())),
            prompts: Arc::new(Mutex::new(Vec::new())),
            script: None,
            intake_script: None,
            end_on_steer: false,
            panic_on_prompt: false,
            refuse_prompt: false,
            ignore_abort: false,
            input_request: None,
            answered: Arc::new(Mutex::new(Vec::new())),
            answer_tx: Arc::new(Mutex::new(None)),
            adapter_id: "mock".into(),
            refusal: None,
            turn_delay: std::time::Duration::ZERO,
            closes: Arc::new(Mutex::new(Vec::new())),
            spec,
        }
    }

    fn set_phase(&self, phase: Phase) {
        *self.phase.locked() = phase;
        self.revision.fetch_add(1, Ordering::SeqCst);
    }
}

#[async_trait::async_trait]
impl HarnessSession for MockSession {
    fn snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            revision: self.revision.load(Ordering::SeqCst),
            phase: *self.phase.locked(),
            activity: None,
            cost: SessionCost::default(),
        }
    }

    fn phase(&self) -> Phase {
        *self.phase.locked()
    }

    async fn prompt(&self, input: PromptInput) -> Result<(), HarnessError> {
        if self.phase() == Phase::Turn {
            return Err(HarnessError::Busy);
        }
        if self.panic_on_prompt {
            panic!("the mock harness panics on its prompt, as the test asked");
        }
        if self.refuse_prompt {
            return Err(HarnessError::Terminated);
        }
        self.prompts.locked().push(input.text.clone());
        // Over before anybody listened: the prompt is taken and nothing more
        // is said — how the session ended is already on its stream.
        if self
            .refusal
            .as_ref()
            .is_some_and(|r| r.how == DeadModel::BeforeListening)
        {
            return Ok(());
        }
        self.set_phase(Phase::Turn);
        let broadcaster = self.broadcaster.clone();
        let phase = Arc::clone(&self.phase);
        let revision = Arc::clone(&self.revision);
        let script = self.script.clone();
        let intake_script = self.intake_script.clone();
        let spec = self.spec.clone();
        let refusal = self.refusal.clone();
        let turn_delay = self.turn_delay;
        let input_request = self.input_request.clone();
        let answer_rx = input_request.as_ref().map(|_| {
            let (tx, rx) = tokio::sync::oneshot::channel();
            *self.answer_tx.locked() = Some(tx);
            rx
        });
        tokio::spawn(async move {
            if !turn_delay.is_zero() {
                tokio::time::sleep(turn_delay).await;
            }
            // An input request parks the turn exactly as a real harness would:
            // nothing more happens until the engine answers it.
            if let (Some(request), Some(rx)) = (input_request, answer_rx) {
                broadcaster.emit(SessionEvent::Lifecycle(LifecycleEvent::Started));
                broadcaster.emit(SessionEvent::Progress(ProgressEvent::TurnStarted));
                *phase.locked() = Phase::AwaitingInput;
                revision.fetch_add(1, Ordering::SeqCst);
                let id = request.id.clone();
                broadcaster.emit(SessionEvent::Lifecycle(LifecycleEvent::InputRequested {
                    request,
                }));
                if rx.await.is_err() {
                    return; // the session went away before anyone answered
                }
                *phase.locked() = Phase::Turn;
                revision.fetch_add(1, Ordering::SeqCst);
                broadcaster.emit(SessionEvent::Lifecycle(LifecycleEvent::InputResolved {
                    id,
                }));
            }
            // A dead model outranks every script: the harness never got as far
            // as the work. `NoProgress` is the failed-launch signature (a
            // terminal wall with nothing before it); `MidRun` does real work
            // first, so the engine must treat the retry as non-transparent.
            if let Some(r) = refusal {
                broadcaster.emit(SessionEvent::Lifecycle(LifecycleEvent::Started));
                if r.how == DeadModel::MidRun {
                    broadcaster.emit(SessionEvent::Progress(ProgressEvent::TurnStarted));
                    broadcaster.emit(SessionEvent::Progress(ProgressEvent::ToolStarted {
                        name: "edit_file".into(),
                        args_summary: "src/lib.rs".into(),
                        tier: bisa_core::ToolTier::Write,
                        id: None,
                    }));
                    broadcaster.emit(SessionEvent::Progress(ProgressEvent::ToolEnded {
                        name: "edit_file".into(),
                        ok: true,
                        id: None,
                    }));
                    broadcaster.emit(SessionEvent::Progress(ProgressEvent::TextDelta {
                        text: "half-way through the change…".into(),
                    }));
                }
                *phase.locked() = Phase::Ended;
                revision.fetch_add(1, Ordering::SeqCst);
                broadcaster.emit(SessionEvent::Lifecycle(LifecycleEvent::Ended {
                    outcome: Outcome::ModelUnavailable {
                        model: r.model,
                        reason: r.reason,
                        retry_after: r.retry_after,
                    },
                    is_terminal: true,
                }));
                return;
            }
            // A scripted intake conversation stands in for a real harness's
            // MCP tool calls: it runs first (a blocking `await_decision` holds
            // the turn open exactly as it would in a live session), then the
            // turn ends.
            if let Some(intake) = intake_script {
                broadcaster.emit(SessionEvent::Lifecycle(LifecycleEvent::Started));
                broadcaster.emit(SessionEvent::Progress(ProgressEvent::TurnStarted));
                run_intake_script(intake, spec).await;
                broadcaster.emit(SessionEvent::Progress(ProgressEvent::TurnEnded));
                *phase.locked() = Phase::Ended;
                revision.fetch_add(1, Ordering::SeqCst);
                broadcaster.emit(SessionEvent::Lifecycle(LifecycleEvent::Ended {
                    outcome: Outcome::Completed,
                    is_terminal: true,
                }));
                return;
            }
            match script {
                Some(events) => {
                    // Scripted mode: emit exactly what the test asked for.
                    // Only a scripted terminal end flips the phase; a script
                    // without one leaves the session running.
                    let mut ended = false;
                    for ev in events {
                        // Small gap so subscribers observe ordering reliably.
                        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                        ended |= ev.is_terminal_end();
                        broadcaster.emit(ev);
                    }
                    if ended {
                        *phase.locked() = Phase::Ended;
                        revision.fetch_add(1, Ordering::SeqCst);
                    }
                }
                None => {
                    broadcaster.emit(SessionEvent::Lifecycle(LifecycleEvent::Started));
                    broadcaster.emit(SessionEvent::Progress(ProgressEvent::TurnStarted));
                    broadcaster.emit(SessionEvent::Progress(ProgressEvent::TextDelta {
                        text: format!("echo: {}", input.text),
                    }));
                    broadcaster.emit(SessionEvent::Progress(ProgressEvent::TurnEnded));
                    *phase.locked() = Phase::Ended;
                    revision.fetch_add(1, Ordering::SeqCst);
                    broadcaster.emit(SessionEvent::Lifecycle(LifecycleEvent::Ended {
                        outcome: Outcome::Completed,
                        is_terminal: true,
                    }));
                }
            }
        });
        Ok(())
    }

    async fn steer(&self, msg: Steer) -> Result<(), HarnessError> {
        if !self.caps.contains(HarnessCaps::STEER) {
            return Err(HarnessError::NotSupported("steer"));
        }
        self.steered.locked().push(msg.text);
        if self.end_on_steer {
            self.set_phase(Phase::Ended);
            self.broadcaster
                .emit(SessionEvent::Lifecycle(LifecycleEvent::Ended {
                    outcome: Outcome::Completed,
                    is_terminal: true,
                }));
        }
        Ok(())
    }

    async fn follow_up(&self, msg: Steer) -> Result<(), HarnessError> {
        if !self.caps.contains(HarnessCaps::FOLLOW_UP) {
            return Err(HarnessError::NotSupported("follow_up"));
        }
        // A real harness's process is gone once its session ended: a turn
        // handed to it fails, and the engine launches afresh. Answering Ok
        // here would swallow the turn — the shape of a reply that never comes.
        if self.phase() == Phase::Ended {
            return Err(HarnessError::protocol(
                "the session has ended; nothing takes a follow-up",
            ));
        }
        self.followed_up.locked().push(msg.text);
        Ok(())
    }

    async fn abort(&self) -> Result<(), HarnessError> {
        self.closes.locked().push(Close::Aborted);
        if self.ignore_abort {
            return Ok(());
        }
        self.set_phase(Phase::Ended);
        self.broadcaster
            .emit(SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: Outcome::Aborted,
                is_terminal: true,
            }));
        Ok(())
    }

    async fn answer(&self, request_id: &str, answer: InputAnswer) -> Result<(), HarnessError> {
        if !self.caps.contains(HarnessCaps::INPUT_REQUESTS) {
            return Err(HarnessError::NotSupported("answer"));
        }
        let tx = self.answer_tx.locked().take();
        let Some(tx) = tx else {
            return Err(HarnessError::protocol(format!(
                "no input request {request_id} is waiting"
            )));
        };
        self.answered
            .locked()
            .push((request_id.to_string(), answer.clone()));
        // The asker may have given up before the answer.
        let _asker_gone = tx.send(answer);
        Ok(())
    }

    fn subscribe(&self) -> BoxEventStream {
        self.broadcaster.subscribe()
    }

    fn resume_token(&self) -> Option<ResumeToken> {
        self.caps
            .contains(HarnessCaps::RESUME)
            .then(|| ResumeToken::of(self.adapter_id.clone(), self.native_id.clone(), &self.spec))
    }

    async fn dispose(self: Box<Self>) -> Result<(), HarnessError> {
        self.closes.locked().push(Close::Disposed);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    /// An intake script over a socket nobody answers on ends at the first
    /// connect, quietly: a session scripting past the engine's end sends
    /// nothing and panics nowhere.
    #[tokio::test]
    async fn an_intake_script_over_a_socket_nobody_answers_ends_at_the_connect() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("nobody.sock");
        let mut spec = spec();
        spec.mcp_servers = vec![bisa_core::McpMount::platform(
            bisa_core::McpServerConfig::Stdio {
                name: "bisa".into(),
                command: "bisa-mcp".into(),
                args: vec![
                    "--socket".into(),
                    socket.display().to_string(),
                    "--goal".into(),
                    "g1".into(),
                ],
                env: Default::default(),
                cwd: None,
            },
        )];
        let script = IntakeScript::new(vec![
            serde_json::json!({"op": "get_goal", "goal": "{{goal}}"}),
        ]);
        let replies = script.replies.clone();
        run_intake_script(script, spec).await;
        assert!(replies.lock().unwrap().is_empty(), "nothing was answered");
    }

    fn spec() -> SessionSpec {
        SessionSpec {
            work_item: None,
            cwd: std::path::PathBuf::from("."),
            prompt: String::new(),
            model: None,
            effort: None,
            mcp_servers: vec![],
            env: Default::default(),
            env_remove: Vec::new(),
            tier_ceiling: bisa_core::ToolTier::Read,
            output_schema: None,
            skills: vec![],
        }
    }

    #[test]
    fn a_mock_takes_the_efforts_it_was_given_and_says_so_in_its_caps() {
        let plain = MockAdapter::default();
        assert!(plain.efforts(None).is_empty());
        assert!(plain.efforts(Some("opus")).is_empty());
        assert!(!plain.caps().contains(HarnessCaps::EFFORT));
        assert_eq!(plain.recommended_plan(), None);
        assert_eq!(plain.recommended_judge(), None);

        let adapter = MockAdapter {
            efforts: vec![Effort::Low, Effort::Medium, Effort::High],
            model_efforts: vec![
                ("opus".into(), vec![Effort::Low, Effort::High, Effort::Max]),
                ("haiku".into(), vec![]),
            ],
            recommended_plan: Some(ModelPlan::fallback(["opus", "sonnet"])),
            recommended_judge: Some("sonnet".into()),
            ..Default::default()
        };
        assert!(adapter.caps().contains(HarnessCaps::EFFORT));
        assert!(
            adapter.caps().contains(HarnessCaps::RESUME),
            "and what it had"
        );
        // A model with its own entry, even an empty one, answers for itself.
        assert_eq!(
            adapter.efforts(Some("opus")),
            [Effort::Low, Effort::High, Effort::Max]
        );
        assert!(adapter.efforts(Some("haiku")).is_empty());
        // Any other model, and no model, take the adapter's own.
        let own = [Effort::Low, Effort::Medium, Effort::High];
        assert_eq!(adapter.efforts(Some("elsewhere")), own);
        assert_eq!(adapter.efforts(None), own);
        assert_eq!(
            adapter.recommended_plan(),
            Some(ModelPlan::fallback(["opus", "sonnet"]))
        );
        assert_eq!(adapter.recommended_judge().as_deref(), Some("sonnet"));
    }

    #[tokio::test]
    async fn a_mock_lists_the_models_it_was_given() {
        assert!(MockAdapter::default().models().await.is_empty());
        let listed = vec![
            ModelInfo::new("opus", Some("Opus"), vec![Effort::High, Effort::Max]),
            ModelInfo::new("haiku", None, vec![]),
        ];
        let adapter = MockAdapter {
            listed_models: listed.clone(),
            ..Default::default()
        };
        assert_eq!(adapter.models().await, listed);
    }

    #[tokio::test]
    async fn every_launch_is_recorded_with_its_model_and_effort_and_a_revival_keeps_both() {
        let adapter = MockAdapter {
            efforts: Effort::ALL.to_vec(),
            ..Default::default()
        };
        let first = SessionSpec {
            model: Some("opus".into()),
            effort: Some(Effort::Xhigh),
            ..spec()
        };
        let session = adapter.launch(first).await.unwrap();
        let _second = adapter.launch(spec()).await.unwrap();
        assert_eq!(adapter.launched_models(), [Some("opus".to_string()), None]);
        assert_eq!(adapter.launched_efforts(), [Some(Effort::Xhigh), None]);

        // The token says what the session ran with, and a session revived
        // from it says the same.
        let token = session.resume_token().unwrap();
        assert_eq!(token.model.as_deref(), Some("opus"));
        assert_eq!(token.effort, Some(Effort::Xhigh));
        let revived = adapter.attach(&token).await.unwrap();
        assert_eq!(adapter.attached(), std::slice::from_ref(&token));
        let again = revived.resume_token().unwrap();
        assert_eq!(again.model.as_deref(), Some("opus"));
        assert_eq!(again.effort, Some(Effort::Xhigh));
        // A revival is not a launch.
        assert_eq!(adapter.launches().len(), 2);
    }

    /// A real harness's process is gone once its session ended, so a turn
    /// handed to it fails and the engine launches afresh; the mock says the
    /// same rather than swallowing the turn.
    #[tokio::test]
    async fn a_follow_up_after_the_session_ended_is_refused() {
        let session = MockAdapter::default().launch(spec()).await.unwrap();
        let mut events = session.subscribe();
        session
            .prompt(PromptInput {
                text: "hello".into(),
                attachments: vec![],
            })
            .await
            .unwrap();
        while let Some(ev) = events.next().await {
            if ev.is_terminal_end() {
                break;
            }
        }
        let refused = session
            .follow_up(Steer {
                text: "still there?".into(),
                attachments: vec![],
            })
            .await;
        assert!(
            matches!(refused, Err(HarnessError::Protocol(_))),
            "{refused:?}"
        );
    }

    /// While the session lives, a follow-up is taken and recorded.
    #[tokio::test]
    async fn a_follow_up_to_a_live_session_is_recorded() {
        let adapter = MockAdapter {
            script: Some(vec![
                SessionEvent::Lifecycle(LifecycleEvent::Started),
                SessionEvent::Progress(ProgressEvent::TurnStarted),
                SessionEvent::Progress(ProgressEvent::TurnEnded),
            ]),
            ..Default::default()
        };
        let session = adapter.launch(spec()).await.unwrap();
        session
            .prompt(PromptInput {
                text: "hello".into(),
                attachments: vec![],
            })
            .await
            .unwrap();
        session
            .follow_up(Steer {
                text: "and this".into(),
                attachments: vec![],
            })
            .await
            .unwrap();
        assert_eq!(adapter.follow_ups.locked().as_slice(), ["and this"]);
    }
}
