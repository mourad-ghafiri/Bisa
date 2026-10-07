//! The Decision-Making Agent: the one door every decision point asks through.
//!
//! A decision point — which of an agent's models leads, who of a pool takes a
//! work item, whether a tool call is harmful — builds a [`DecisionRequest`],
//! says which [`DecisionPoint`] it is and where it stands ([`Standing`]), and
//! gets back a [`Judged`]: an answer sure enough to act on, or the reason it
//! runs its own rule instead. **The fallback is always the caller's own
//! logic**; nothing here decides *for* a point that was not answered, and a
//! security point reads anything but a sure answer as no verdict.
//!
//! What happens between the question and the answer, in order:
//!
//! 1. **Is the point on?** A point somebody selected by name (`auto_route`,
//!    an effort of `auto`, a `judge` step, a classifier set to the
//!    Decision-Making Agent) is on. The rest are on when `decisions.enabled`
//!    resolves true for the project, or the agent or the workflow standing at
//!    the point has its own switch on — and the point is not listed in
//!    `decisions.points_off`. Off asks nobody.
//! 2. **The redactor.** The state and every sentence of every question pass
//!    the redactor before they leave: an HTTP provider is an outside service,
//!    and a harness never learns a secret either.
//! 3. **The provider** `decisions.provider` names, built by
//!    [`bisa_decision::build`] — already wrapped in the contract check, the
//!    retry budget and the one deadline.
//! 4. **The threshold**: `decisions.confidence.act`, or
//!    `decisions.confidence.security` at a security point. Below it the answer
//!    is recorded and not acted on.
//! 5. **The record**: a `Judged` event on the bus — which the activity log
//!    keeps, goal or no goal — and, on a goal, a `judgement` fact in its
//!    journal, signed as the platform.
//!
//! A judgement's own session is never routed, never asked how hard to work
//! and never classified: `ask_once` pins no lead, carries no judged level —
//! a harness that answers runs at `decisions.harness.effort` — and judges its
//! tool calls with the classifier off, so a judgement never waits on another
//! judgement.

use crate::ask::{ask_once, Whom};
use crate::events::{EngineEvent, EnginePayload};
use crate::{EngineError, Inner};
use async_trait::async_trait;
use bisa_connectors as cx;
use bisa_core::{
    DecisionPoint, DecisionProviderKind, DecisionQuestion, DecisionRequest, DecisionResponse,
    DecisionUsage, Effort, EffortChoice, Home, JournalPayload, Judgement, JudgementOutcome,
    ModelPlan, NoulCriteria, ProjectId, RunId,
};
use bisa_decision::{
    build, AskTarget, Asked, Asker, DecisionProvider, DecisionSettings, KeySource,
    ProviderDescriptor, ProviderError, RlcdAuth,
};
use serde_json::Value;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// The `decisions.*` keys, spelled once.
pub mod keys {
    pub const ENABLED: &str = "decisions.enabled";
    pub const POINTS_OFF: &str = "decisions.points_off";
    pub const PROVIDER: &str = "decisions.provider";
    pub const HARNESS: &str = "decisions.harness.id";
    pub const HARNESS_MODEL: &str = "decisions.harness.model";
    pub const HARNESS_EFFORT: &str = "decisions.harness.effort";
    pub const AGENT: &str = "decisions.agent.id";
    pub const JEV_MODEL: &str = "decisions.jev.model";
    pub const RLCD_ENDPOINT: &str = "decisions.rlcd.endpoint";
    pub const RLCD_MODEL: &str = "decisions.rlcd.model";
    pub const RLCD_AUTH: &str = "decisions.rlcd.auth";
    pub const DEADLINE: &str = "decisions.deadline_secs";
    pub const RETRIES: &str = "decisions.retries";
    pub const CONFIDENCE_ACT: &str = "decisions.confidence.act";
    pub const CONFIDENCE_SECURITY: &str = "decisions.confidence.security";
}

/// The keystore name a remote provider's API key is kept under. The key is
/// this machine's and never a setting.
pub fn key_name(provider: DecisionProviderKind) -> Option<String> {
    provider
        .is_remote()
        .then(|| format!("decision:{}:api_key", provider.as_str()))
}

/// Where a decision point stands when it asks: what a record hangs on, and
/// whose switches count.
#[derive(Clone, Debug, Default)]
pub struct Standing {
    /// The goal, or the run of the workspace, the point asks for: whose
    /// journal keeps the judgement and whose row the feed files it under.
    pub home: Option<Home>,
    pub run: Option<RunId>,
    pub step: Option<bisa_core::workflow::StepId>,
    /// The project whose `decisions.*` settings apply, when there is one.
    pub project: Option<ProjectId>,
    /// An agent or a workflow standing here has its own switch on.
    pub switched_on: bool,
    /// The agent the judgement is about or for, for the record.
    pub agent: Option<String>,
    /// How sure the answer must be, when the point says so itself (a `judge`
    /// step's `min_confidence`); the settings' threshold otherwise.
    pub min_confidence: Option<f64>,
}

impl Standing {
    pub fn on(home: Option<Home>) -> Self {
        Self {
            home,
            ..Self::default()
        }
    }
}

/// What came of asking.
#[derive(Clone, Debug, PartialEq)]
pub enum Judged {
    /// Sure enough to act on.
    Answered(DecisionResponse),
    /// Answered, below the threshold. The caller runs its own rule; a security
    /// caller reads it as no verdict.
    Unsure(DecisionResponse),
    /// No answer that holds to the contract.
    Failed(String),
    /// The point is off here: nobody was asked.
    Off,
}

impl Judged {
    /// The answers to act on, and nothing otherwise.
    pub fn answered(&self) -> Option<&DecisionResponse> {
        match self {
            Judged::Answered(response) => Some(response),
            _ => None,
        }
    }
}

/// The Decision-Making Agent's state: a provider a test put in the real one's
/// place.
#[derive(Default)]
pub struct DeciderState {
    stand_in: RwLock<Option<Arc<dyn DecisionProvider>>>,
}

impl DeciderState {
    /// Answer from `provider` instead of the one the settings name — a test's
    /// scripted provider. `None` puts the settings back in charge.
    pub fn stand_in(&self, provider: Option<Arc<dyn DecisionProvider>>) {
        *self.stand_in.write().unwrap_or_else(|e| e.into_inner()) = provider;
    }

    fn standing_in(&self) -> Option<Arc<dyn DecisionProvider>> {
        self.stand_in
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

fn setting(inner: &Inner, key: &str, project: Option<ProjectId>) -> Value {
    inner
        .ws
        .setting(key, project)
        .map(|r| r.value)
        .unwrap_or(Value::Null)
}

fn text(inner: &Inner, key: &str) -> String {
    setting(inner, key, None)
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// The `decisions.*` settings a provider is built from.
pub fn settings(inner: &Inner) -> DecisionSettings {
    let defaults = DecisionSettings::default();
    let or = |value: String, default: String| if value.is_empty() { default } else { value };
    DecisionSettings {
        provider: text(inner, keys::PROVIDER)
            .parse()
            .unwrap_or(defaults.provider),
        harness: or(text(inner, keys::HARNESS), defaults.harness),
        // Empty is a choice here: the harness's own default model.
        harness_model: text(inner, keys::HARNESS_MODEL),
        agent: or(text(inner, keys::AGENT), defaults.agent),
        jev_model: or(text(inner, keys::JEV_MODEL), defaults.jev_model),
        rlcd_endpoint: text(inner, keys::RLCD_ENDPOINT),
        rlcd_model: text(inner, keys::RLCD_MODEL),
        rlcd_auth: text(inner, keys::RLCD_AUTH)
            .parse()
            .unwrap_or(RlcdAuth::Bearer),
        retries: setting(inner, keys::RETRIES, None)
            .as_u64()
            .unwrap_or(2)
            .min(5) as u8,
    }
}

/// `decisions.harness.effort`: how hard the model works when a harness
/// answers for the Decision-Making Agent. A level, fitted at launch to what
/// the model takes.
pub fn harness_effort(inner: &Inner) -> Effort {
    crate::effort::level_setting(inner, keys::HARNESS_EFFORT)
}

/// One deadline for a whole judgement, retries included.
pub fn deadline(inner: &Inner) -> Duration {
    Duration::from_secs(
        setting(inner, keys::DEADLINE, None)
            .as_u64()
            .unwrap_or(20)
            .clamp(5, 120),
    )
}

/// How sure an answer must be at `point` to be acted on.
pub fn threshold(inner: &Inner, point: DecisionPoint, project: Option<ProjectId>) -> f64 {
    if point.is_security() {
        setting(inner, keys::CONFIDENCE_SECURITY, None)
            .as_f64()
            .unwrap_or(0.9)
            .clamp(0.5, 1.0)
    } else {
        setting(inner, keys::CONFIDENCE_ACT, project)
            .as_f64()
            .unwrap_or(0.7)
            .clamp(0.0, 1.0)
    }
}

/// The points `decisions.points_off` leaves to their own rule.
fn points_off(inner: &Inner, project: Option<ProjectId>) -> Vec<DecisionPoint> {
    setting(inner, keys::POINTS_OFF, project)
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|v| v.as_str()?.parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Whether `point` asks the Decision-Making Agent from where `standing` stands.
pub fn is_on(inner: &Inner, point: DecisionPoint, standing: &Standing) -> bool {
    if point.is_selected_explicitly() {
        return true;
    }
    let switched = standing.switched_on
        || setting(inner, keys::ENABLED, standing.project)
            .as_bool()
            .unwrap_or(false);
    switched && !points_off(inner, standing.project).contains(&point)
}

/// Whether the global switch alone reaches `point` — what a surface with no
/// agent and no workflow of its own asks.
pub fn is_on_globally(inner: &Inner, point: DecisionPoint) -> bool {
    is_on(inner, point, &Standing::default())
}

/// The engine's one-shot session, as the providers' [`Asker`] — asking for
/// what the judgement is for, so its roster row says so.
struct EngineAsker<'a> {
    inner: &'a Inner,
    asking: crate::ask::Asking,
}

#[async_trait]
impl Asker for EngineAsker<'_> {
    async fn ask(
        &self,
        target: &AskTarget,
        prompt: &str,
        output_schema: &Value,
        deadline: Duration,
    ) -> Result<Asked, String> {
        let whom = match target {
            AskTarget::Agent(agent) => Whom::Agent(agent.clone()),
            // The judge's own model works as hard as its setting says: the
            // session it runs in asks nobody.
            AskTarget::Harness { harness, model } => Whom::Harness {
                harness: harness.clone(),
                model: model.clone(),
                effort: Some(harness_effort(self.inner)),
            },
        };
        ask_once(
            self.inner,
            &whom,
            self.asking.clone(),
            prompt,
            Some(output_schema.clone()),
            deadline,
        )
        .await
        .map(|asked| Asked {
            text: asked.text,
            model: Some(asked.model),
        })
        .map_err(|e| e.to_string())
    }
}

/// This machine's keystore, as the providers' [`KeySource`].
struct Keystore<'a> {
    inner: &'a Inner,
}

impl KeySource for Keystore<'_> {
    fn key(&self, provider: DecisionProviderKind) -> Option<cx::Secret> {
        let name = key_name(provider)?;
        match self.inner.ws.identity.secret(&name) {
            Ok(value) => value.and_then(cx::Secret::some),
            Err(e) => {
                tracing::warn!("the decision provider's key cannot be read: {e}");
                None
            }
        }
    }
}

/// Whether a key is stored for `provider`. Never the key.
pub fn has_key(inner: &Inner, provider: DecisionProviderKind) -> bool {
    Keystore { inner }.key(provider).is_some()
}

/// Keep `value` as `provider`'s API key.
pub fn set_key(
    inner: &Inner,
    provider: DecisionProviderKind,
    value: &str,
) -> Result<(), EngineError> {
    let name = key_name(provider).ok_or_else(|| {
        EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-provider-takes-no-api-key",
            a0 = (provider.as_str()).to_string()
        ))
    })?;
    let value = value.trim();
    if value.is_empty() {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-api-key-never-empty"
        )));
    }
    inner.ws.identity.set_secret(&name, value)?;
    Ok(())
}

/// Forget `provider`'s API key. Idempotent.
pub fn clear_key(inner: &Inner, provider: DecisionProviderKind) -> Result<(), EngineError> {
    if let Some(name) = key_name(provider) {
        inner.ws.identity.delete_secret(&name)?;
    }
    Ok(())
}

/// The request as it may leave the process: the state and every sentence of
/// every question through the redactor.
fn redacted(inner: &Inner, request: &DecisionRequest) -> DecisionRequest {
    let clean = |text: &str| inner.security.redact(text).text;
    let state = match &request.state {
        Value::String(text) => Value::String(clean(text)),
        other => serde_json::from_str(&clean(&other.to_string())).unwrap_or_else(|_| {
            // A redaction that broke the JSON is still the redacted state.
            Value::String(clean(&other.to_string()))
        }),
    };
    let questions = request
        .questions
        .iter()
        .map(|(id, question)| {
            let question = match question {
                DecisionQuestion::Noul {
                    instructions,
                    criteria,
                } => DecisionQuestion::Noul {
                    instructions: clean(instructions),
                    criteria: criteria.as_ref().map(|c| NoulCriteria {
                        yes: c.yes.as_deref().map(clean),
                        no: c.no.as_deref().map(clean),
                    }),
                },
                DecisionQuestion::Choice {
                    instructions,
                    criteria,
                } => DecisionQuestion::Choice {
                    instructions: clean(instructions),
                    // An option's id is the caller's own word and what the
                    // answer is matched on; its meaning is what is read.
                    criteria: criteria
                        .iter()
                        .map(|(k, v)| (k.clone(), clean(v)))
                        .collect(),
                },
                DecisionQuestion::Score {
                    instructions,
                    criteria,
                } => DecisionQuestion::Score {
                    instructions: clean(instructions),
                    criteria: criteria.iter().map(|level| clean(level)).collect(),
                },
            };
            (id.clone(), question)
        })
        .collect();
    DecisionRequest { state, questions }
}

/// Ask the provider the settings name — or the one standing in — with nothing
/// decided about what becomes of the answer: `POST /decisions/try`, a
/// judgement at no point.
pub async fn ask(
    inner: &Inner,
    request: &DecisionRequest,
) -> (ProviderDescriptor, Result<DecisionResponse, ProviderError>) {
    ask_for(
        inner,
        request,
        crate::ask::Asking::of(bisa_core::AskPurpose::Decision { point: None }),
    )
    .await
}

/// [`ask`], saying what the judgement is for — the roster row of the
/// session it may launch names the point and its home. `POST /decisions/try`
/// and [`judge`] both come through here.
pub async fn ask_for(
    inner: &Inner,
    request: &DecisionRequest,
    asking: crate::ask::Asking,
) -> (ProviderDescriptor, Result<DecisionResponse, ProviderError>) {
    let wait = deadline(inner);
    if let Some(provider) = inner.decider.standing_in() {
        // Held to the contract and the deadline like the real one.
        let provider = bisa_decision::Bounded(bisa_decision::Checked(provider));
        return (provider.descriptor(), provider.decide(request, wait).await);
    }
    let settings = settings(inner);
    let asker = EngineAsker { inner, asking };
    let keystore = Keystore { inner };
    // The endpoint a person named is the one host this call declares; the
    // node's deny list still comes first.
    let declared: Vec<String> = endpoint_host(&settings).into_iter().collect();
    let hosts = crate::connectors::PolicyHostJudge {
        inner,
        home: None,
        subject: format!("POST {}", bisa_decision::SYSTEM_ONE_PATH),
        declared: &declared,
    };
    let ports = bisa_decision::Ports {
        transport: Arc::new(cx::ReqwestTransport::with_http(Arc::clone(&inner.http))),
        hosts: &hosts,
        keys: &keystore,
        asker: &asker,
        clock: Arc::new(cx::SystemClock),
        entropy: Arc::new(cx::OsEntropy),
    };
    // Bound, so the provider — which borrows the ports — is dropped before
    // what the ports borrow.
    let outcome = match build(&settings, &ports) {
        Ok(provider) => (provider.descriptor(), provider.decide(request, wait).await),
        Err(e) => (described(&settings), Err(e)),
    };
    outcome
}

/// The host of the endpoint the settings name, for a remote provider.
fn endpoint_host(settings: &DecisionSettings) -> Option<String> {
    let endpoint = match settings.provider {
        DecisionProviderKind::Jev => bisa_decision::JEV_ENDPOINT,
        DecisionProviderKind::Rlcd => settings.rlcd_endpoint.as_str(),
        DecisionProviderKind::Harness | DecisionProviderKind::Agent => return None,
    };
    url::Url::parse(endpoint)
        .ok()
        .and_then(|u| cx::hosts::host_of(&u))
}

/// Who the settings name, for a record of a provider that could not be built.
pub fn described(settings: &DecisionSettings) -> ProviderDescriptor {
    let model = match settings.provider {
        DecisionProviderKind::Harness => AskTarget::Harness {
            harness: settings.harness.clone(),
            model: (!settings.harness_model.is_empty()).then(|| settings.harness_model.clone()),
        }
        .model_name(),
        DecisionProviderKind::Agent => settings.agent.clone(),
        DecisionProviderKind::Jev => settings.jev_model.clone(),
        DecisionProviderKind::Rlcd => settings.rlcd_model.clone(),
    };
    ProviderDescriptor {
        kind: settings.provider,
        model,
    }
}

/// Put `request` to the Decision-Making Agent for `point`, from where
/// `standing` stands. Never an error: a point that was not answered runs its
/// own rule.
pub async fn judge(
    inner: &Inner,
    point: DecisionPoint,
    standing: &Standing,
    request: DecisionRequest,
) -> Judged {
    if !is_on(inner, point, standing) {
        return Judged::Off;
    }
    let request = redacted(inner, &request);
    let started = Instant::now();
    let (descriptor, result) = ask_for(
        inner,
        &request,
        crate::ask::Asking::of(bisa_core::AskPurpose::Decision { point: Some(point) })
            .on(standing.home),
    )
    .await;
    let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let bar = standing
        .min_confidence
        .unwrap_or_else(|| threshold(inner, point, standing.project));

    let (judged, outcome, reason) = match result {
        Ok(response) if response.certainty() >= bar => {
            (Judged::Answered(response), JudgementOutcome::Applied, None)
        }
        Ok(response) => {
            let reason = format!(
                "sure to {:.2}, and this point acts from {bar:.2}",
                response.certainty()
            );
            (
                Judged::Unsure(response),
                JudgementOutcome::Unsure,
                Some(reason),
            )
        }
        Err(e) => {
            let reason = e.to_string();
            tracing::warn!(point = point.as_str(), "no judgement: {reason}");
            (
                Judged::Failed(reason.clone()),
                JudgementOutcome::Failed,
                Some(reason),
            )
        }
    };
    let (model, answers, usage) = match &judged {
        Judged::Answered(r) | Judged::Unsure(r) => (r.model.clone(), r.answers.clone(), r.usage),
        _ => (
            descriptor.model.clone(),
            Default::default(),
            DecisionUsage::default(),
        ),
    };
    record(
        inner,
        standing,
        Judgement {
            point,
            provider: descriptor.kind,
            model,
            calibrated: descriptor.calibrated(),
            questions: request.questions,
            answers,
            outcome,
            reason,
            latency_ms,
            usage,
        },
    );
    judged
}

/// One judgement into the feed, and into its home's journal — the goal's,
/// or the run of the workspace's — when it has one.
fn record(inner: &Inner, standing: &Standing, judgement: Judgement) {
    let payload = EnginePayload::Judged {
        judgement: judgement.clone(),
        agent: standing.agent.clone(),
        run: standing.run,
        step: standing.step.clone(),
    };
    inner.emit(match &standing.home {
        Some(home) => inner.home_scope(home).event(None, payload),
        None => EngineEvent::global(payload),
    });
    if let Some(home) = standing.home {
        let (signer, attestation) = crate::ops::signer_for(&inner.ws, None);
        crate::warn_on_err(
            inner.ws.append_journal(
                &home,
                JournalPayload::Judgement {
                    judgement,
                    run: standing.run,
                    step: standing.step.clone(),
                },
                &signer,
                attestation,
            ),
            "journaling a judgement",
        );
    }
}

/// The most of a task a question about a launch — its route, its effort —
/// is shown.
const MAX_ROUTED_TASK_CHARS: usize = 4_000;
/// The one question of a route.
pub const ROUTE_QUESTION: &str = "model";
/// The one question of an effort.
pub const EFFORT_QUESTION: &str = "effort";

/// The task as a question about a launch carries it.
fn capped(task: &str) -> String {
    task.chars().take(MAX_ROUTED_TASK_CHARS).collect()
}

/// `auto_route`: the model of `plan` that should lead a launch on `harness`
/// for `task` — the one the Decision-Making Agent picks among the plan's ready
/// models, by what each is suited for. `None` when the plan is not routed, has
/// fewer than two ready models, or no sure pick came back: the plan's own
/// order stands.
pub async fn route(
    inner: &Inner,
    plan: &bisa_core::ModelPlan,
    harness: &str,
    task: &str,
    standing: &Standing,
) -> Option<String> {
    let options: Vec<(String, String)> = plan
        .routable(&inner.models.view(harness))
        .into_iter()
        .map(|(model, suited_for)| {
            (
                model.to_string(),
                suited_for
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("the model `{model}`")),
            )
        })
        .collect();
    if options.is_empty() {
        return None;
    }
    let request = DecisionRequest::one(
        serde_json::json!({ "task": capped(task) }),
        ROUTE_QUESTION,
        DecisionQuestion::choice(
            "Which model is the right one for this task? Prefer the least capable model that \
             will do it well.",
            options,
        ),
    );
    let judged = judge(inner, DecisionPoint::ModelRoute, standing, request).await;
    judged
        .answered()?
        .answer(ROUTE_QUESTION)?
        .chosen()
        .map(str::to_string)
}

/// Who the work is for, as the effort question's state names them: the
/// agent's name and what it says it does. Nothing for a session no agent
/// definition stands behind.
fn about(inner: &Inner, agent: Option<&str>) -> Value {
    agent
        .and_then(|id| bisa_core::AgentId::new(id).ok())
        .and_then(|id| inner.ws.get_agent(&id).ok())
        .map(|def| {
            serde_json::json!({
                "name": def.name,
                "description": def.description.unwrap_or_default(),
            })
        })
        .unwrap_or(Value::Null)
}

/// An effort of `auto`: the level `model` should work at on `task` — the
/// one the Decision-Making Agent picks among the levels `harness` takes for
/// it, each said with its rank and what it is for. `None` when fewer than
/// two levels are offered or no sure pick came back: the attempt's own
/// fallback runs.
pub(crate) async fn effort(
    inner: &Inner,
    harness: &str,
    model: Option<&str>,
    task: &str,
    agent: Option<&str>,
    standing: &Standing,
) -> Option<Effort> {
    let levels = inner.catalog.get(harness)?.efforts(model);
    let question = crate::effort::question(&levels)?;
    let request = DecisionRequest::one(
        serde_json::json!({
            "task": capped(task),
            "agent": about(inner, agent),
            "model": crate::models::model_key(harness, model),
        }),
        EFFORT_QUESTION,
        question,
    );
    let judged = judge(inner, DecisionPoint::ModelEffort, standing, request).await;
    let word = judged.answered()?.answer(EFFORT_QUESTION)?.chosen()?;
    crate::effort::judged_level(word, &levels)
}

/// One launch walk, as the Decision-Making Agent is asked about it.
pub(crate) struct WalkAsk<'a> {
    pub plan: &'a ModelPlan,
    /// The harness chain; the questions are about the first.
    pub candidates: &'a [String],
    /// A step's hard model pin: never routed, and the one model whose
    /// effort is read.
    pub model_pin: Option<&'a str>,
    pub effort_pin: Option<EffortChoice>,
    pub effort_setting: EffortChoice,
    /// The walk's rotation, so the effort is asked about the model the plan
    /// leads with.
    pub rotation: u64,
    pub task: &'a str,
    /// The agent definition the session runs, by id.
    pub agent: Option<&'a str>,
}

/// What the Decision-Making Agent named for one launch walk. It travels
/// with the walk — across a model wall, through a relaunch — so one walk
/// asks once.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Walk {
    /// The model an `auto_route` plan leads with.
    pub lead: Option<String>,
    /// The level an attempt that comes to `auto` runs at.
    pub effort: Option<Effort>,
}

/// Ask about one launch walk: the model that leads and the level to work
/// at, together and under the one deadline, so a walk that needs both waits
/// once. The effort is asked only when an attempt of the walk may come to
/// `auto`, and about the model the plan leads with before anybody routed
/// it; every attempt then fits the answer to its own model.
pub(crate) async fn walk(inner: &Inner, ask: &WalkAsk<'_>, standing: &Standing) -> Walk {
    let Some(harness) = ask.candidates.first() else {
        return Walk::default();
    };
    let lead = async {
        match ask.model_pin {
            Some(_) => None,
            None => route(inner, ask.plan, harness, ask.task, standing).await,
        }
    };
    let level = async {
        if !crate::effort::asks(ask.plan, ask.model_pin, ask.effort_pin, ask.effort_setting) {
            return None;
        }
        let first = match ask.model_pin {
            Some(pin) => Some(pin.to_string()),
            None => ask
                .plan
                .order(&inner.models.view(harness), ask.rotation, None)
                .first()
                .map(|model| model.to_string()),
        };
        effort(
            inner,
            harness,
            first.as_deref(),
            ask.task,
            ask.agent,
            standing,
        )
        .await
    };
    let (lead, effort) = tokio::join!(lead, level);
    Walk { lead, effort }
}

/// The activity kind every judgement is recorded under — the `Judged` event's
/// own tag.
pub const JUDGED_KIND: &str = "judged";

/// One judgement as the feed kept it.
#[derive(Clone, Debug, PartialEq, serde::Serialize, schemars::JsonSchema)]
pub struct JudgementRecord {
    pub seq: i64,
    pub at: u64,
    /// The goal it was made on, when it was made on one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<String>,
    pub judgement: Judgement,
}

/// The newest `limit` judgements this node asked for, newest first — read
/// from the activity feed, which keeps every one, goal or no goal. A row that
/// no longer reads as a judgement is passed over.
pub fn recent(inner: &Inner, limit: usize) -> Result<Vec<JudgementRecord>, EngineError> {
    let rows = inner.ws.activity_by_kinds(&[JUDGED_KIND], limit)?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let event: Value = serde_json::from_str(&row.event).ok()?;
            let judgement = serde_json::from_value(event.get("judgement")?.clone()).ok()?;
            let word = |key: &str| event.get(key).and_then(Value::as_str).map(str::to_string);
            Some(JudgementRecord {
                seq: row.seq,
                at: row.at,
                goal: (row.source_kind == "goal").then(|| row.source_id.clone()),
                agent: word("agent"),
                run: word("run"),
                step: word("step"),
                judgement,
            })
        })
        .collect())
}

/// What Settings › Decision Making and `bisa decisions status` read.
#[derive(Clone, Debug, PartialEq, serde::Serialize, schemars::JsonSchema)]
pub struct DeciderStatus {
    /// The Decision-Making Agent itself: its reserved id, its name, what it
    /// is.
    pub agent: bisa_store::DecisionMakingAgent,
    /// `decisions.enabled`, as it resolves for the workspace.
    pub enabled: bool,
    pub provider: DecisionProviderKind,
    /// The model that answers, as a record names it.
    pub answers_as: String,
    /// How hard that model works when a harness answers:
    /// `decisions.harness.effort`, fitted to what the harness takes for the
    /// model. Absent for any other provider, and for a harness that takes no
    /// effort.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
    /// Whether the provider's probabilities are a calibrated model's.
    pub calibrated: bool,
    /// Whether the provider can be asked as it is set up — and why not.
    pub ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    /// Whether an API key is stored for the provider; `None` for a provider
    /// that takes none. Never the key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_stored: Option<bool>,
    /// Every decision point, and whether the workspace's switch reaches it.
    pub points: Vec<PointStatus>,
    pub deadline_secs: u64,
    pub confidence_act: f64,
    pub confidence_security: f64,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, schemars::JsonSchema)]
pub struct PointStatus {
    pub point: DecisionPoint,
    /// On whatever the switch says: it is selected where it is used.
    pub selected_explicitly: bool,
    pub on: bool,
}

/// The Decision-Making Agent as it stands on this node. Asks nobody.
pub fn status(inner: &Inner) -> Result<DeciderStatus, EngineError> {
    let settings = settings(inner);
    let descriptor = described(&settings);
    let problem = readiness(inner, &settings).err();
    Ok(DeciderStatus {
        agent: bisa_store::decision_making_agent()?,
        enabled: setting(inner, keys::ENABLED, None)
            .as_bool()
            .unwrap_or(false),
        provider: descriptor.kind,
        calibrated: descriptor.calibrated(),
        answers_as: descriptor.model,
        effort: fitted_effort(inner, &settings),
        ready: problem.is_none(),
        problem,
        key_stored: key_name(settings.provider).map(|_| has_key(inner, settings.provider)),
        points: DecisionPoint::ALL
            .into_iter()
            .map(|point| PointStatus {
                point,
                selected_explicitly: point.is_selected_explicitly(),
                on: is_on_globally(inner, point),
            })
            .collect(),
        deadline_secs: deadline(inner).as_secs(),
        confidence_act: threshold(inner, DecisionPoint::AssignPick, None),
        confidence_security: threshold(inner, DecisionPoint::SecurityTool, None),
    })
}

/// The effort a judgement's session runs at when a harness answers: the
/// setting's level, fitted to the model the settings name as a launch would
/// fit it.
fn fitted_effort(inner: &Inner, settings: &DecisionSettings) -> Option<Effort> {
    if settings.provider != DecisionProviderKind::Harness {
        return None;
    }
    let adapter = inner.catalog.get(&settings.harness)?;
    let model = (!settings.harness_model.is_empty()).then_some(settings.harness_model.as_str());
    harness_effort(inner).clamp_to(&adapter.efforts(model))
}

/// Whether the provider the settings name can be asked, without asking it.
fn readiness(inner: &Inner, settings: &DecisionSettings) -> Result<(), String> {
    match settings.provider {
        DecisionProviderKind::Harness => launchable(inner, &settings.harness),
        DecisionProviderKind::Agent => {
            let id = bisa_core::AgentId::new(settings.agent.as_str()).map_err(|e| e.to_string())?;
            let info = crate::ops::agent_info(&inner.ws, &id)
                .ok_or_else(|| format!("agent `{}` is missing or disabled", settings.agent))?;
            info.harness
                .iter()
                .find_map(|h| launchable(inner, h).ok())
                .ok_or_else(|| format!("no harness of `{}` is available here", settings.agent))
        }
        DecisionProviderKind::Jev => {
            if settings.jev_model.is_empty() {
                return Err("no model is named".into());
            }
            has_key(inner, settings.provider)
                .then_some(())
                .ok_or_else(|| "no API key is stored for Jev".to_string())
        }
        DecisionProviderKind::Rlcd => {
            if settings.rlcd_endpoint.is_empty() {
                return Err("no endpoint is named".into());
            }
            if settings.rlcd_model.is_empty() {
                return Err("no model is named".into());
            }
            if settings.rlcd_auth == RlcdAuth::Bearer && !has_key(inner, settings.provider) {
                return Err("no API key is stored for the endpoint".into());
            }
            Ok(())
        }
    }
}

/// Whether `harness` is one this node can launch — the classifier's own
/// readiness rule: a registered adapter, and — when the listing has been
/// read — one that probed as installed. A harness the last probe found
/// missing cannot answer, whatever the settings say; a cold listing is not
/// held against it (nothing is spawned here).
fn launchable(inner: &Inner, harness: &str) -> Result<(), String> {
    if inner.catalog.get(harness).is_none() {
        return Err(format!("`{harness}` is not a harness this node can launch"));
    }
    let ttl = inner.cache.settings().harness_listing_ttl();
    match inner.catalog.installed_cached(harness, ttl) {
        Some(false) => Err(format!("`{harness}` is not installed on this machine")),
        _ => Ok(()),
    }
}
