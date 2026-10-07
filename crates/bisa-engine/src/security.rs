//! The engine's side of the three security features: where the policy is
//! read, where the vault lives, where text is redacted on its way out and
//! where a tool call is judged before it runs.
//!
//! The rules themselves are the `bisa-security` crate's — pure, tested
//! against synthetic tokens. This module owns the *seams*:
//!
//! - [`RedactedSession`] wraps every harness session the executor launches, so
//!   every prompt, steer, follow-up and text answer is redacted before the
//!   harness sees it, and every `args_summary`, brief and streamed word a
//!   harness reports is redacted before a person sees it. One line in
//!   `resolve_and_launch` covers every driver — the executor, chat, the
//!   Workflow Agent, notes, `ask_agent_once` and the classifier itself.
//! - [`SecurityState::redact_inbound`] is the other direction: what an agent
//!   hands *back* — an MCP request, a reply, a note's answer, a commit
//!   message, a pull request — is redacted once where it enters the platform,
//!   so a secret the agent read with its own tools is stored, synced and sent
//!   as a placeholder, never as the value.
//! - [`decide_tool`] judges one tool call for every caller that has one: the
//!   permission funnel (`inputs.rs`), the terminal guard route, a `check`
//!   step's command and a check start's. Rules first, then the classifier
//!   where a rule asks for it, and the placeholders restored **only** in the
//!   input that is about to run.
//!
//! The vault holds the only copy of what a placeholder stands for. It is in
//! memory, it is never written, and a restart forgets it — which is why a
//! command still carrying a placeholder after a restore is refused rather than
//! run with a literal in it.

use std::collections::VecDeque;

use bisa_cache::Bounded;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bisa_core::event::{GuardJudge, GuardVerdict, JournalPayload};
use bisa_core::settings::Scope;
use bisa_core::{AskKind, Gate, Home, ToolTier};
use bisa_harness::{
    BoxEventStream, HarnessError, HarnessSession, InputAnswer, InputKind, LifecycleEvent, Phase,
    ProgressEvent, PromptInput, ResumeToken, SessionEvent, SessionSnapshot, Steer,
};
use bisa_security::classify::{Subject, Verdict as ClassifierVerdict};
use bisa_security::guard::{unresolved_deny, Host, ToolCall, Verdict as RuleVerdict};
use bisa_security::policy::{Problem, SecurityPolicy, SettingsLayer};
use bisa_security::redact::Vault;
use bisa_store::Workspace;
use futures::StreamExt;
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::broadcast;

use crate::events::{EngineEvent, EnginePayload};
use crate::registry::LiveRunId;
use crate::Inner;

// The rule types, re-exported so the node and its schema generator can name
// them without a dependency of their own on the security crate.
pub use bisa_security::guard::{Action, GuardRule, Host as GuardHost, Matcher};
pub use bisa_security::policy::{Feature, Problem as PolicyProblem};
pub use bisa_security::redact::{Detector, Origin, RedactRule};

/// How many decisions the status route remembers.
const RECENT: usize = 50;
/// How many classifier verdicts are kept, keyed by the redacted subject.
const VERDICT_CACHE: usize = 256;
/// How many of a person's answers are kept across every live scope. A goal's
/// — or a run of the workspace's — go with it (`forget_home`); this is the
/// ceiling for the rest — a bound, not an expiry: an answer is a consent
/// decision and is never aged out while its home lives.
const ANSWER_RECORD: usize = 4096;

/// Settings keys, spelled once.
pub mod keys {
    pub const REDACTOR_ENABLED: &str = "security.redactor.enabled";
    pub const REDACTOR_RULES: &str = "security.redactor.rules";
    pub const REDACTOR_BUILTINS_OFF: &str = "security.redactor.builtins_off";
    pub const REDACTOR_ENV_AUTO: &str = "security.redactor.env_auto";
    pub const GUARD_ENABLED: &str = "security.guard.enabled";
    pub const GUARD_RULES: &str = "security.guard.rules";
    pub const GUARD_BUILTINS_OFF: &str = "security.guard.builtins_off";
    pub const GUARD_TERMINAL_HOOKS: &str = "security.guard.terminal_hooks";
    pub const CLASSIFIER_ENABLED: &str = "security.classifier.enabled";
    pub const CLASSIFIER_AGENT: &str = "security.classifier.agent";
    pub const CLASSIFIER_PROVIDER: &str = "security.classifier.provider";
    pub const CLASSIFIER_HARNESS: &str = "security.classifier.harness";
    pub const CLASSIFIER_MODEL: &str = "security.classifier.model";
    pub const CLASSIFIER_EFFORT: &str = "security.classifier.effort";
    pub const CLASSIFIER_DEADLINE: &str = "security.classifier.deadline_secs";
    pub const CLASSIFIER_ON_HARMFUL: &str = "security.classifier.on_harmful";
    pub const CONTENT_SCREEN: &str = "security.content.screen";
    pub const CONTENT_ON_HARMFUL: &str = "security.content.on_harmful";
    pub const NET_DENY_HOSTS: &str = "security.net.deny_hosts";
    pub const NET_ALLOW_HOSTS: &str = "security.net.allow_hosts";
    pub const COLLAB_CLASSIFY: &str = bisa_core::collab_settings::keys::SECURITY_COLLAB_CLASSIFY;
    pub const COLLAB_AGENT_TOOLS: &str =
        bisa_core::collab_settings::keys::SECURITY_COLLAB_AGENT_TOOLS;
}

/// What the classifier does with a harmful verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OnHarmful {
    Ask,
    Deny,
}

/// Who reads a subject for the classifier.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClassifierProvider {
    /// The classifier agent, on its own harness and models.
    #[default]
    Agent,
    /// One harness with one of its models, no agent in between.
    Harness,
    /// The Decision-Making Agent, as the `decisions.*` settings set it up.
    DecisionMakingAgent,
}

impl ClassifierProvider {
    fn of(word: Option<&str>) -> Self {
        match word {
            Some("harness") => ClassifierProvider::Harness,
            Some("decision_making_agent") => ClassifierProvider::DecisionMakingAgent,
            _ => ClassifierProvider::Agent,
        }
    }
}

/// The classifier's settings, resolved.
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct ClassifierSettings {
    pub enabled: bool,
    pub provider: ClassifierProvider,
    pub agent: String,
    /// The harness and the model that read when the provider is `harness`; an
    /// empty model is the harness's own default.
    pub harness: String,
    pub model: String,
    /// How hard that model works on a reading: a level, never `auto`, fitted
    /// at launch to what the model takes.
    pub effort: bisa_core::Effort,
    pub deadline_secs: u64,
    pub on_harmful: OnHarmful,
}

impl ClassifierSettings {
    pub fn deadline(&self) -> Duration {
        Duration::from_secs(self.deadline_secs.max(1))
    }
}

/// The content screen's two words (11-security §What an agent reads from
/// outside): whether what an agent reads from outside is put to the
/// classifier first, and what a harmful reading does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ContentSettings {
    pub screen: bool,
    pub on_harmful: OnHarmful,
}

/// The whole security policy as this node runs it right now.
pub struct Policy {
    pub rules: SecurityPolicy,
    /// The hosts the platform's own outbound calls — a connector step's —
    /// may and may not reach, beyond what each connector declares.
    pub hosts: bisa_security::net::HostPolicy,
    pub problems: Vec<Problem>,
    pub redactor_enabled: bool,
    /// Whether the node's own environment variables whose names say *secret*
    /// are armed as detectors.
    pub env_auto: bool,
    pub guard_enabled: bool,
    pub terminal_hooks: bool,
    pub classifier: ClassifierSettings,
    /// What people on other nodes get (14-collaboration): whether their
    /// messages are read first, what an agent they wake may do.
    pub collaboration: bisa_core::CollaborationSecurity,
    /// What an agent reads from outside — a page, a review — and whether it
    /// is screened first (`crate::content`).
    pub content: ContentSettings,
}

/// One decision the guard made, as the status route shows it — the subject is
/// redacted, so this is safe to hand to a screen.
#[derive(Clone, Debug, Serialize, JsonSchema)]
#[schemars(rename = "GuardDecision")]
pub struct Decision {
    pub at: u64,
    pub tool: String,
    pub subject: String,
    pub verdict: GuardVerdict,
    pub by: GuardJudge,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The goal, or the run of the workspace, the call was made for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub home: Option<Home>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<LiveRunId>,
    /// The person on another node the session was working for, when it was.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_behalf_of: Option<bisa_core::PrincipalId>,
}

/// The vault, the policy cache and the recent decisions. Shared as an `Arc`
/// so a session decorator can outlive the borrow it was made from.
pub struct SecurityState {
    ws: Arc<Workspace>,
    bus: broadcast::Sender<EngineEvent>,
    vault: Vault,
    policy: Mutex<Option<Arc<Policy>>>,
    recent: Mutex<VecDeque<Decision>>,
    verdicts: Mutex<Bounded<String, ClassifierVerdict>>,
    /// A person's answers to the guard's questions, by scope (`goal:<id>`,
    /// `run:<id>` or `session:<id>`) and the redacted subject's digest — so
    /// the same question is asked once per goal or run, not once per call.
    /// Bounded by count; a goal's are forgotten when it closes, a run of the
    /// workspace's when it ends.
    answers: Mutex<Bounded<(String, String), bool>>,
}

impl std::fmt::Debug for SecurityState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecurityState")
            .field("vault", &self.vault)
            .finish()
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The home directory the guard expands `~` against.
pub fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// The names of this node's environment variables — names only; a value is
/// read by the redactor at compile time and nowhere else.
fn env_names() -> Vec<String> {
    std::env::vars_os()
        .filter_map(|(k, _)| k.into_string().ok())
        .collect()
}

/// The reason recorded when a person's earlier answer decided a call.
pub const REMEMBERED: &str = "remembered from your earlier answer on this goal or run";

impl SecurityState {
    pub fn new(ws: Arc<Workspace>, bus: broadcast::Sender<EngineEvent>) -> Self {
        SecurityState {
            ws,
            bus,
            vault: Vault::new(crate::interactive::mint_secret()),
            policy: Mutex::new(None),
            recent: Mutex::new(VecDeque::new()),
            verdicts: Mutex::new(Bounded::new(VERDICT_CACHE)),
            answers: Mutex::new(Bounded::new(ANSWER_RECORD)),
        }
    }

    pub fn vault(&self) -> &Vault {
        &self.vault
    }

    /// The policy, read from the settings once and kept until a `security.*`
    /// key changes.
    pub fn policy(&self) -> Arc<Policy> {
        if let Some(p) = self
            .policy
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        {
            return p;
        }
        let loaded = Arc::new(self.load());
        *self.policy.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::clone(&loaded));
        loaded
    }

    /// Forget the cached policy, the classifier's verdicts and the answers
    /// people gave under the old rules; the next reader rebuilds it.
    /// How many classifier verdicts stand cached — what a test reads to
    /// prove a settings change ended them.
    pub fn cached_verdicts(&self) -> usize {
        self.verdicts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }

    pub fn invalidate(&self) {
        *self.policy.lock().unwrap_or_else(|e| e.into_inner()) = None;
        self.verdicts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.answers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }

    /// A person's earlier answer to this question in this scope, if any.
    fn remembered(&self, judge: &Judge<'_>, digest: &str) -> Option<bool> {
        let scope = answer_scope(judge)?;
        self.answers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&(scope, digest.to_string()))
            .copied()
    }

    fn remember_answer(&self, judge: &Judge<'_>, digest: String, approve: bool) {
        let Some(scope) = answer_scope(judge) else {
            return;
        };
        let evicted = self
            .answers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert((scope, digest), approve);
        if let Some(((scope, _), _)) = evicted {
            tracing::debug!(
                target: "bisa_engine::security",
                "the answer record is full; the oldest answer, on {scope}, is forgotten"
            );
        }
    }

    /// The goal — or the run of the workspace — is over: what its owner
    /// answered goes with it.
    pub fn forget_home(&self, home: &Home) {
        let scope = home.to_string();
        self.answers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|(s, _), _| s != &scope);
    }

    fn load(&self) -> Policy {
        let value = |key: &str| {
            self.ws
                .setting(key, None)
                .map(|r| r.value)
                .unwrap_or(Value::Null)
        };
        let mut problems = Vec::new();
        let mut layers = Vec::new();
        // The team's rules first, then this machine's: both apply, in that
        // order, because a list resolved first-holder-wins would silently
        // drop one of them.
        for scope in [Scope::Workspace, Scope::Machine] {
            // A layer that cannot be read — a torn write, a hand edit — must
            // not read as *no rules here*: that is a policy failing open in
            // silence. It is said, and it is a problem on the status.
            let layer = match self.ws.settings_layer(scope, None) {
                Ok(layer) => layer,
                Err(e) => {
                    tracing::error!(target: "bisa_engine::security", scope = ?scope, "the settings layer could not be read; its rules are not applied: {e}");
                    problems.push(bisa_security::Problem {
                        feature: bisa_security::Feature::Guard,
                        rule: format!("settings:{scope:?}"),
                        reason: format!("the {scope:?} settings could not be read, so its rules are not applied: {e}"),
                    });
                    Default::default()
                }
            };
            let get = |key: &str| layer.get(key).cloned().unwrap_or(Value::Null);
            let (parsed, bad) = SettingsLayer::from_values(
                &get(keys::REDACTOR_RULES),
                &get(keys::REDACTOR_BUILTINS_OFF),
                &get(keys::GUARD_RULES),
                &get(keys::GUARD_BUILTINS_OFF),
            );
            problems.extend(bad);
            layers.push(parsed);
        }
        let flag = |key: &str, default: bool| value(key).as_bool().unwrap_or(default);
        let env_auto = flag(keys::REDACTOR_ENV_AUTO, true);
        let names = if env_auto { env_names() } else { Vec::new() };
        let (rules, bad) = SecurityPolicy::from_layers(&layers, &env, &names);
        problems.extend(bad);
        // The host lists concatenate across the two scopes, like the rules.
        let mut hosts = bisa_security::net::HostPolicy::default();
        for scope in [Scope::Workspace, Scope::Machine] {
            let layer = self.ws.settings_layer(scope, None).unwrap_or_default();
            for (key, into) in [
                (keys::NET_DENY_HOSTS, &mut hosts.deny),
                (keys::NET_ALLOW_HOSTS, &mut hosts.allow),
            ] {
                let (parsed, bad) = bisa_security::net::parse_hosts(
                    &layer.get(key).cloned().unwrap_or(Value::Null),
                );
                into.extend(parsed);
                problems.extend(bad.into_iter().map(|p| Problem {
                    feature: bisa_security::policy::Feature::Guard,
                    rule: format!("{key}{}", p.entry),
                    reason: p.reason,
                }));
            }
        }
        let redactor_enabled = flag(keys::REDACTOR_ENABLED, true);
        let guard_enabled = flag(keys::GUARD_ENABLED, true);
        // Said once per load, so a node running open is never quietly open.
        if !redactor_enabled {
            tracing::warn!("the redactor is off on this node: nothing is redacted");
        }
        if !guard_enabled {
            tracing::warn!("the guard is off on this node: no tool call is judged");
        }
        let collaboration = bisa_core::CollaborationSecurity::from_resolved(
            &self.ws.settings(None).unwrap_or_default(),
        );
        Policy {
            rules,
            hosts,
            problems,
            redactor_enabled,
            env_auto,
            guard_enabled,
            collaboration,
            content: ContentSettings {
                screen: flag(keys::CONTENT_SCREEN, true),
                on_harmful: match value(keys::CONTENT_ON_HARMFUL).as_str() {
                    Some("deny") => OnHarmful::Deny,
                    _ => OnHarmful::Ask,
                },
            },
            terminal_hooks: flag(keys::GUARD_TERMINAL_HOOKS, true),
            classifier: ClassifierSettings {
                enabled: flag(keys::CLASSIFIER_ENABLED, true),
                provider: ClassifierProvider::of(value(keys::CLASSIFIER_PROVIDER).as_str()),
                harness: value(keys::CLASSIFIER_HARNESS)
                    .as_str()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .unwrap_or("claude-code")
                    .to_string(),
                model: value(keys::CLASSIFIER_MODEL)
                    .as_str()
                    .map(str::trim)
                    .unwrap_or_default()
                    .to_string(),
                effort: crate::effort::level_of(&value(keys::CLASSIFIER_EFFORT)),
                agent: value(keys::CLASSIFIER_AGENT)
                    .as_str()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or(bisa_core::AgentId::GENERAL)
                    .to_string(),
                deadline_secs: value(keys::CLASSIFIER_DEADLINE)
                    .as_u64()
                    .unwrap_or(20)
                    .clamp(5, 120),
                on_harmful: match value(keys::CLASSIFIER_ON_HARMFUL).as_str() {
                    Some("deny") => OnHarmful::Deny,
                    _ => OnHarmful::Ask,
                },
            },
        }
    }

    /// Redact one text and say so on the bus when something was replaced —
    /// `at` names where (`connector slack.post_message`), as the intake's
    /// own redactions do.
    pub fn redact_at(&self, at: &str, text: &str) -> bisa_security::redact::Redaction {
        let redaction = self.redact(text);
        self.redacted(at, &redaction);
        redaction
    }

    /// Redact one text on its way to an agent. Returns the text unchanged
    /// when the redactor is off.
    pub fn redact(&self, text: &str) -> bisa_security::redact::Redaction {
        let policy = self.policy();
        if !policy.redactor_enabled {
            return bisa_security::redact::Redaction {
                text: text.to_string(),
                ..Default::default()
            };
        }
        policy.rules.redactor.redact(&self.vault, text)
    }

    /// Redact every string leaf of a JSON value on its way to an agent.
    pub fn redact_value(&self, value: &mut Value) -> bisa_security::redact::Redaction {
        let policy = self.policy();
        if !policy.redactor_enabled {
            return Default::default();
        }
        policy.rules.redactor.redact_value(&self.vault, value)
    }

    /// Redact a text an agent handed *back* — a reply, an answer, a message
    /// it wrote — where it enters the platform, and say so (`Redacted` at
    /// `at`). What comes out is what gets stored, synced and sent; the vault
    /// keeps the value like any other, so a placeholder the agent quotes
    /// back later still resolves at an execution point.
    pub fn redact_inbound(&self, text: &str, at: &str) -> String {
        let redaction = self.redact(text);
        self.redacted(at, &redaction);
        redaction.text
    }

    /// A sentence the platform is about to post: the message is the
    /// catalog's, its arguments are the world's words — a harness's failure,
    /// a name an agent chose — and every string among them is redacted as
    /// [`Self::redact_inbound`] redacts a text.
    pub fn redact_said(&self, mut said: bisa_core::Text, at: &str) -> bisa_core::Text {
        for arg in said.args.values_mut() {
            if let bisa_core::Arg::Str(words) = arg {
                *words = self.redact_inbound(words, at);
            }
        }
        said
    }

    /// [`Self::redact_inbound`] over every string leaf of a JSON value — an
    /// MCP request, every field at once.
    pub fn redact_inbound_value(&self, value: &mut Value, at: &str) {
        let redaction = self.redact_value(value);
        self.redacted(at, &redaction);
    }

    /// Redact what a harness reports about itself before a person sees it —
    /// a tool's argument summary, a sub-agent's brief, a permission's summary.
    pub fn redact_event(&self, event: SessionEvent) -> SessionEvent {
        match event {
            SessionEvent::Progress(p) => SessionEvent::Progress(self.redact_progress(p)),
            SessionEvent::Lifecycle(LifecycleEvent::InputRequested { mut request }) => {
                if let InputKind::Permission { args_summary, .. } = &mut request.kind {
                    *args_summary = self.redact(args_summary).text;
                }
                SessionEvent::Lifecycle(LifecycleEvent::InputRequested { request })
            }
            other => other,
        }
    }

    fn redact_progress(&self, event: ProgressEvent) -> ProgressEvent {
        match event {
            ProgressEvent::ToolStarted {
                name,
                args_summary,
                tier,
                id,
            } => ProgressEvent::ToolStarted {
                name,
                args_summary: self.redact(&args_summary).text,
                tier,
                id,
            },
            ProgressEvent::SubagentStarted {
                id,
                name,
                description,
            } => ProgressEvent::SubagentStarted {
                id,
                name,
                description: self.redact(&description).text,
            },
            ProgressEvent::Nested { parent, event } => ProgressEvent::Nested {
                parent,
                event: Box::new(self.redact_progress(*event)),
            },
            // The agent's own words: a secret it read with its tools and is
            // now quoting. A delta boundary can split a token, which is why
            // the assembled reply is redacted again where it is posted.
            ProgressEvent::TextDelta { text } => ProgressEvent::TextDelta {
                text: self.redact(&text).text,
            },
            // Its thinking quotes what it read the same way.
            ProgressEvent::ThinkingDelta { text } => ProgressEvent::ThinkingDelta {
                text: self.redact(&text).text,
            },
            other => other,
        }
    }

    fn announce(&self, event: EngineEvent) {
        if self.bus.send(event).is_err() {
            tracing::trace!("security event dropped: no subscribers");
        }
    }

    fn redacted(&self, at: &str, redaction: &bisa_security::redact::Redaction) {
        if redaction.count > 0 {
            self.announce(EngineEvent::global(EnginePayload::Redacted {
                count: redaction.count,
                kinds: redaction.kinds.clone(),
                at: at.to_string(),
            }));
        }
    }

    /// The last decisions, newest first.
    pub fn recent(&self) -> Vec<Decision> {
        self.recent
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .cloned()
            .collect()
    }

    fn remember(&self, decision: Decision) {
        let mut recent = self.recent.lock().unwrap_or_else(|e| e.into_inner());
        recent.push_front(decision);
        recent.truncate(RECENT);
    }

    pub(crate) fn cached_verdict(&self, digest: &str) -> Option<ClassifierVerdict> {
        self.verdicts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(digest)
            .cloned()
    }

    pub(crate) fn cache_verdict(&self, digest: String, verdict: ClassifierVerdict) {
        self.verdicts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(digest, verdict);
    }
}

// ---------------------------------------------------------------------------
// The session decorator
// ---------------------------------------------------------------------------

/// A harness session whose every outbound text is redacted and whose every
/// reported summary is redacted on the way back. Everything else is the
/// inner session's.
pub struct RedactedSession {
    inner: Box<dyn HarnessSession>,
    security: Arc<SecurityState>,
}

impl RedactedSession {
    pub fn wrap(
        inner: Box<dyn HarnessSession>,
        security: Arc<SecurityState>,
    ) -> Box<dyn HarnessSession> {
        Box::new(RedactedSession { inner, security })
    }

    fn text(&self, at: &str, text: &str) -> String {
        let redaction = self.security.redact(text);
        self.security.redacted(at, &redaction);
        redaction.text
    }
}

#[async_trait::async_trait]
impl HarnessSession for RedactedSession {
    fn snapshot(&self) -> SessionSnapshot {
        self.inner.snapshot()
    }

    fn phase(&self) -> Phase {
        self.inner.phase()
    }

    async fn prompt(&self, mut input: PromptInput) -> Result<(), HarnessError> {
        input.text = self.text("prompt", &input.text);
        self.inner.prompt(input).await
    }

    async fn steer(&self, mut msg: Steer) -> Result<(), HarnessError> {
        msg.text = self.text("steer", &msg.text);
        self.inner.steer(msg).await
    }

    async fn follow_up(&self, mut msg: Steer) -> Result<(), HarnessError> {
        msg.text = self.text("follow_up", &msg.text);
        self.inner.follow_up(msg).await
    }

    async fn abort(&self) -> Result<(), HarnessError> {
        self.inner.abort().await
    }

    async fn answer(&self, request_id: &str, answer: InputAnswer) -> Result<(), HarnessError> {
        let answer = match answer {
            InputAnswer::Text { text } => InputAnswer::Text {
                text: self.text("answer", &text),
            },
            other => other,
        };
        self.inner.answer(request_id, answer).await
    }

    fn subscribe(&self) -> BoxEventStream {
        let security = Arc::clone(&self.security);
        self.inner
            .subscribe()
            .map(move |event| security.redact_event(event))
            .boxed()
    }

    fn resume_token(&self) -> Option<ResumeToken> {
        self.inner.resume_token()
    }

    async fn dispose(self: Box<Self>) -> Result<(), HarnessError> {
        self.inner.dispose().await
    }
}

/// Wrap a freshly launched session when the redactor is on.
pub fn wrap_session(inner: &Inner, session: Box<dyn HarnessSession>) -> Box<dyn HarnessSession> {
    if inner.security.policy().redactor_enabled {
        RedactedSession::wrap(session, Arc::clone(&inner.security))
    } else {
        session
    }
}

// ---------------------------------------------------------------------------
// Judging a tool call
// ---------------------------------------------------------------------------

/// Who is asking, for the record and for the gate.
#[derive(Clone, Debug)]
pub struct Judge<'a> {
    /// The goal, or the run of the workspace, the call is made for: where a
    /// question goes and an answer is remembered.
    pub home: Option<Home>,
    pub session: Option<LiveRunId>,
    pub cwd: Option<&'a Path>,
    /// Where the call comes from — a session the platform drives or the
    /// platform's own command, or a person's harness in a terminal — which
    /// is what a rule's `applies_to` is read against.
    pub host: Host,
    /// Whether a `classify` rule may ask the classifier. Off for the
    /// classifier's own session (and any other one-shot ask), so a verdict
    /// can never wait on another verdict: there, `classify` reads as `ask`.
    pub classifier: bool,
    /// The person on another node whose message this session answers, with
    /// their role (14-collaboration). Under `security.collaboration.agent_tools
    /// = ask`, a classify verdict reads as ask and nothing beyond reading
    /// runs without the owner.
    pub on_behalf_of: Option<(bisa_core::PrincipalId, bisa_core::MemberRole)>,
}

impl<'a> Judge<'a> {
    /// A judge for a call with no session — a command the platform runs for
    /// itself, for a goal, a run of the workspace, or nothing at all.
    pub fn platform(home: Option<Home>, cwd: Option<&'a Path>) -> Self {
        Judge {
            home,
            session: None,
            cwd,
            host: Host::Platform,
            classifier: true,
            on_behalf_of: None,
        }
    }

    /// Whether this call runs for an outsider whose tools are put to the
    /// owner: the session works for a person on another node and the policy
    /// says `ask`.
    pub fn asks_for_outsider(&self, policy: &Policy) -> bool {
        self.on_behalf_of.is_some()
            && policy.collaboration.agent_tools == bisa_core::OutsiderTools::Ask
    }
}

/// The scope a person's answer is remembered under: the home — `goal:<id>`
/// or `run:<id>` — else the session; a judge with neither remembers nothing.
fn answer_scope(judge: &Judge<'_>) -> Option<String> {
    judge
        .home
        .map(|h| h.to_string())
        .or_else(|| judge.session.map(|s| format!("session:{s}")))
}

/// What the guard decided about one call.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// Run it — with this input, placeholders restored.
    Allow { input: Value, rule: Option<String> },
    /// Refuse it; the agent hears the reason.
    Deny {
        reason: String,
        rule: Option<String>,
    },
    /// Put it to the person: the question to ask, and the input to run with
    /// should they say yes.
    Ask {
        question: String,
        input: Value,
        rule: Option<String>,
        reason: Option<String>,
    },
    /// No rule had an opinion: the caller's own default applies, on the
    /// restored input.
    Fallthrough { input: Value },
}

/// The redacted rendering of a call — what a journal, a gate and a screen see.
fn subject_of(inner: &Inner, tool: &str, input: &Value) -> String {
    // Only the command is read here; who calls and what the tool is also
    // called bear on no subject.
    let call = ToolCall {
        tool,
        input,
        cwd: None,
        home: None,
        host: Host::Platform,
        canonical: None,
    };
    let raw = call.command().unwrap_or_else(|| {
        if input.is_null() {
            String::new()
        } else {
            input.to_string()
        }
    });
    let redacted = inner.security.redact(&raw).text;
    redacted.chars().take(600).collect()
}

fn question_for(tool: &str, subject: &str, why: &str) -> String {
    if subject.is_empty() {
        format!("Allow `{tool}`?\n\n{why}")
    } else {
        format!("Allow `{tool}`?\n\n```\n{subject}\n```\n\n{why}")
    }
}

/// The redacted call as the classifier and the answer memory see it: the
/// subject, the paths and the working directory, home-relative.
fn model_subject(
    inner: &Inner,
    tool: &str,
    subject: &str,
    call: &ToolCall<'_>,
    home: Option<&Path>,
) -> Subject {
    Subject {
        tool: tool.to_string(),
        summary: subject.to_string(),
        paths: call
            .paths()
            .into_iter()
            .map(|p| inner.security.redact(&p).text)
            .collect(),
        cwd: call.cwd.map(|c| home_relative(c, home)),
    }
}

/// A question the guard would put to the person: who raised it, the rule,
/// the reason as recorded, and the sentence the person reads.
struct Asking {
    by: GuardJudge,
    rule: String,
    reason: Option<String>,
    why: String,
}

/// Ask the person — unless they already answered this very question on this
/// goal, in which case their answer stands and is recorded as theirs. A
/// refusal a rule made never comes through here, so a rule's *deny* is never
/// remembered past.
fn ask_or_remembered(
    inner: &Inner,
    judge: &Judge<'_>,
    tool: &str,
    subject: &str,
    model: &Subject,
    input: Value,
    asking: Asking,
) -> Outcome {
    let digest = bisa_security::classify::subject_digest(model);
    if let Some(approve) = inner.security.remembered(judge, &digest) {
        record(
            inner,
            judge,
            tool,
            subject,
            if approve {
                GuardVerdict::Allowed
            } else {
                GuardVerdict::Denied
            },
            GuardJudge::Person,
            Some(asking.rule.clone()),
            Some(REMEMBERED.to_string()),
        );
        return if approve {
            Outcome::Allow {
                input,
                rule: Some(asking.rule),
            }
        } else {
            Outcome::Deny {
                reason: format!("refused by the person who owns the work ({REMEMBERED})"),
                rule: Some(asking.rule),
            }
        };
    }
    record(
        inner,
        judge,
        tool,
        subject,
        GuardVerdict::Asked,
        asking.by,
        Some(asking.rule.clone()),
        asking.reason.clone(),
    );
    Outcome::Ask {
        question: question_for(tool, subject, &asking.why),
        input,
        rule: Some(asking.rule),
        reason: asking.reason,
    }
}

/// What a permission above a step's ceiling does when no rule decided it:
/// put to the person (a guided or manual goal, a session with no goal) or
/// read by the classifier (an auto goal under `goals.auto.permissions =
/// classify`). The rules come first either way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AboveCeiling {
    Ask,
    Classify,
}

/// How far a session may go on its own: the tier the call wants, the
/// ceiling its step allows, and what happens above it. A call within the
/// ceiling that no rule decided runs at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reach {
    pub tier: ToolTier,
    pub ceiling: ToolTier,
    pub above: AboveCeiling,
}

/// The rule name a decision above the ceiling is recorded under — not a
/// rule anyone wrote, the step's own ceiling.
pub const CEILING_RULE: &str = "tier_ceiling";

/// Judge one tool call: restore placeholders, apply the rules, ask the
/// classifier where a rule says so, settle what no rule decided against the
/// session's reach when one is given, and record what was decided. With no
/// reach — the platform's own commands, a terminal's hook — no rule's
/// opinion is `Fallthrough`, the caller's to settle. The call says where it
/// comes from (`Judge::host`): a rule that applies to one host alone is
/// passed over for a call from the other. The tool is read by its own name
/// and by its harness-neutral one (`bisa_core::caps::canonical_tool`), so a
/// rule written in the tiers' words reads Claude Code's `WebFetch` as
/// `fetch`.
pub async fn decide_tool(
    inner: &Inner,
    tool: &str,
    input: &Value,
    judge: Judge<'_>,
    reach: Option<Reach>,
) -> Outcome {
    let policy = inner.security.policy();
    // Restore first: the rules judge the real command, and a placeholder
    // nothing here can resolve is a literal the command must not run with.
    let mut restored_input = input.clone();
    let restored = if policy.redactor_enabled {
        inner.security.vault().restore_value(&mut restored_input)
    } else {
        Default::default()
    };
    let subject = subject_of(inner, tool, input);
    if !restored.unresolved.is_empty() {
        let RuleVerdict::Deny { rule, reason } = unresolved_deny(&restored.unresolved) else {
            unreachable!("an unresolved placeholder is a deny");
        };
        record(
            inner,
            &judge,
            tool,
            &subject,
            GuardVerdict::Denied,
            GuardJudge::Rule,
            Some(rule.clone()),
            Some(reason.clone()),
        );
        return Outcome::Deny {
            reason,
            rule: Some(rule),
        };
    }
    let home = home();
    // The harness's own name and its neutral one: a `Tool` rule reads both.
    let canonical = bisa_core::caps::canonical_tool(tool);
    if !policy.guard_enabled {
        // No rules: the reach alone stands, as it does for a call no rule
        // had an opinion on.
        let call = ToolCall {
            tool,
            input: &restored_input,
            cwd: judge.cwd,
            home: home.as_deref(),
            host: judge.host,
            canonical,
        };
        return beyond_the_rules(
            inner,
            &policy,
            &judge,
            tool,
            &subject,
            &call,
            home.as_deref(),
            restored_input.clone(),
            reach,
        )
        .await;
    }
    let call = ToolCall {
        tool,
        input: &restored_input,
        cwd: judge.cwd,
        home: home.as_deref(),
        host: judge.host,
        canonical,
    };
    // A session woken by a person on another node: a classify verdict is a
    // question for the owner, whatever the classifier would have said — an
    // outsider's words do not get to spend the owner's trust.
    let verdict = match policy.rules.guard.evaluate(&call) {
        RuleVerdict::Classify { rule } if judge.asks_for_outsider(&policy) => {
            RuleVerdict::Ask { rule }
        }
        other => other,
    };
    match verdict {
        RuleVerdict::Fallthrough => {
            beyond_the_rules(
                inner,
                &policy,
                &judge,
                tool,
                &subject,
                &call,
                home.as_deref(),
                restored_input.clone(),
                reach,
            )
            .await
        }
        RuleVerdict::Allow { rule } => {
            record(
                inner,
                &judge,
                tool,
                &subject,
                GuardVerdict::Allowed,
                GuardJudge::Rule,
                Some(rule.clone()),
                None,
            );
            Outcome::Allow {
                input: restored_input,
                rule: Some(rule),
            }
        }
        RuleVerdict::Deny { rule, reason } => {
            record(
                inner,
                &judge,
                tool,
                &subject,
                GuardVerdict::Denied,
                GuardJudge::Rule,
                Some(rule.clone()),
                Some(reason.clone()),
            );
            Outcome::Deny {
                reason,
                rule: Some(rule),
            }
        }
        RuleVerdict::Ask { rule } => {
            let label = rule_label(&policy, &rule);
            let model = model_subject(inner, tool, &subject, &call, home.as_deref());
            ask_or_remembered(
                inner,
                &judge,
                tool,
                &subject,
                &model,
                restored_input,
                Asking {
                    by: GuardJudge::Rule,
                    rule,
                    reason: None,
                    why: format!("The guard rule “{label}” asks you first."),
                },
            )
        }
        RuleVerdict::Classify { rule } => {
            let wanted = format!("The guard rule “{}”", rule_label(&policy, &rule));
            let model = model_subject(inner, tool, &subject, &call, home.as_deref());
            classify_or_ask(
                inner,
                &policy,
                &judge,
                tool,
                &subject,
                &model,
                restored_input,
                rule,
                &wanted,
            )
            .await
        }
    }
}

/// What no rule decided, settled against the session's reach: within the
/// ceiling it runs; above it the person is asked — or, in an auto goal,
/// the classifier reads it first — under the ceiling's own name. An
/// outsider's session asks whatever the reach says: their words do not get
/// to spend the owner's classifier. No reach is the caller's `Fallthrough`.
#[allow(clippy::too_many_arguments)]
async fn beyond_the_rules(
    inner: &Inner,
    policy: &Policy,
    judge: &Judge<'_>,
    tool: &str,
    subject: &str,
    call: &ToolCall<'_>,
    home: Option<&Path>,
    input: Value,
    reach: Option<Reach>,
) -> Outcome {
    let Some(reach) = reach else {
        return Outcome::Fallthrough { input };
    };
    if reach.tier <= reach.ceiling {
        return Outcome::Allow { input, rule: None };
    }
    let model = model_subject(inner, tool, subject, call, home);
    let above = if judge.asks_for_outsider(policy) {
        AboveCeiling::Ask
    } else {
        reach.above
    };
    match above {
        AboveCeiling::Ask => ask_or_remembered(
            inner,
            judge,
            tool,
            subject,
            &model,
            input,
            Asking {
                by: GuardJudge::Rule,
                rule: CEILING_RULE.to_string(),
                reason: None,
                // True of a step's ceiling and of a conversation's mode
                // alike: the reach does not say which it is.
                why: format!(
                    "`{tool}` wants {:?}, and {:?} is as far as this session goes on its own.",
                    reach.tier, reach.ceiling
                ),
            },
        ),
        AboveCeiling::Classify => {
            classify_or_ask(
                inner,
                policy,
                judge,
                tool,
                subject,
                &model,
                input,
                CEILING_RULE.to_string(),
                "This session's ceiling",
            )
            .await
        }
    }
}

/// The classifier's reading of a call, under a rule's name or the
/// ceiling's: safe runs and is recorded as the classifier's allow; harmful
/// asks the person with the reason or refuses outright as
/// `security.classifier.on_harmful` says; no classifier for this session,
/// a classifier that is off, or no verdict, asks the person with the
/// reason. Never an allow the rules did not name.
#[allow(clippy::too_many_arguments)]
async fn classify_or_ask(
    inner: &Inner,
    policy: &Policy,
    judge: &Judge<'_>,
    tool: &str,
    subject: &str,
    model: &Subject,
    input: Value,
    rule: String,
    wanted: &str,
) -> Outcome {
    if !judge.classifier {
        return ask_or_remembered(
            inner,
            judge,
            tool,
            subject,
            model,
            input,
            Asking {
                by: GuardJudge::Rule,
                rule,
                reason: Some("no classifier for this session".into()),
                why: format!(
                    "{wanted} wanted the classifier's reading, which this session may not ask for."
                ),
            },
        );
    }
    if !policy.classifier.enabled {
        return ask_or_remembered(
            inner,
            judge,
            tool,
            subject,
            model,
            input,
            Asking {
                by: GuardJudge::Rule,
                rule,
                reason: Some("the classifier is off".into()),
                why: format!(
                    "{wanted} wanted the classifier's reading, and the classifier is off."
                ),
            },
        );
    }
    match crate::classifier::classify(inner, model, &policy.classifier, judge.home).await {
        Ok(ClassifierVerdict::Safe) => {
            record(
                inner,
                judge,
                tool,
                subject,
                GuardVerdict::Allowed,
                GuardJudge::Classifier,
                Some(rule.clone()),
                None,
            );
            Outcome::Allow {
                input,
                rule: Some(rule),
            }
        }
        Ok(ClassifierVerdict::Harmful { reason }) => match policy.classifier.on_harmful {
            OnHarmful::Deny => {
                let reason = format!("the classifier judged it harmful: {reason}");
                record(
                    inner,
                    judge,
                    tool,
                    subject,
                    GuardVerdict::Denied,
                    GuardJudge::Classifier,
                    Some(rule.clone()),
                    Some(reason.clone()),
                );
                Outcome::Deny {
                    reason,
                    rule: Some(rule),
                }
            }
            OnHarmful::Ask => ask_or_remembered(
                inner,
                judge,
                tool,
                subject,
                model,
                input,
                Asking {
                    by: GuardJudge::Classifier,
                    rule,
                    reason: Some(reason.clone()),
                    why: format!("The classifier judged it harmful: {reason}"),
                },
            ),
        },
        Err(e) => ask_or_remembered(
            inner,
            judge,
            tool,
            subject,
            model,
            input,
            Asking {
                by: GuardJudge::Classifier,
                rule,
                reason: Some(format!("no verdict from the classifier ({e})")),
                why: format!("The classifier gave no verdict ({e}), so it is yours to decide."),
            },
        ),
    }
}

fn rule_label(policy: &Policy, id: &str) -> String {
    policy
        .rules
        .guard_rules
        .iter()
        .find(|r| r.id == id)
        .map(|r| r.label.clone())
        .unwrap_or_else(|| id.to_string())
}

fn home_relative(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|h| path.strip_prefix(h).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// Remember a decision, announce it, and journal it on its home — the goal,
/// or the run of the workspace — when it has one.
#[allow(clippy::too_many_arguments)]
fn record(
    inner: &Inner,
    judge: &Judge<'_>,
    tool: &str,
    subject: &str,
    verdict: GuardVerdict,
    by: GuardJudge,
    rule: Option<String>,
    reason: Option<String>,
) {
    inner.security.remember(Decision {
        at: now_secs(),
        tool: tool.to_string(),
        subject: subject.to_string(),
        verdict,
        by,
        rule: rule.clone(),
        reason: reason.clone(),
        home: judge.home,
        session: judge.session,
        on_behalf_of: judge.on_behalf_of.as_ref().map(|(p, _)| p.clone()),
    });
    let payload = EnginePayload::GuardDecided {
        session: judge.session,
        tool: tool.to_string(),
        subject: subject.to_string(),
        verdict,
        by,
        rule: rule.clone(),
        reason: reason.clone(),
    };
    inner.security.announce(match &judge.home {
        Some(home) => inner.home_scope(home).event(None, payload),
        None => EngineEvent::global(payload),
    });
    if let Some(home) = judge.home {
        let (signer, attestation) = crate::ops::signer_for(&inner.ws, None);
        crate::warn_on_err(
            inner.ws.append_journal(
                &home,
                JournalPayload::Guard {
                    tool: tool.to_string(),
                    subject: subject.to_string(),
                    verdict,
                    by,
                    rule,
                    reason,
                },
                &signer,
                attestation,
            ),
            "journaling a guard decision",
        );
    }
}

/// The content screen decided about what an agent was about to read from
/// outside (`crate::content`): recorded like any guard decision — the tool is
/// `content`, the subject the source in words, `by` the screen or the person.
pub(crate) fn record_content(
    inner: &Inner,
    judge: &Judge<'_>,
    source: &str,
    verdict: GuardVerdict,
    by: GuardJudge,
    reason: Option<String>,
) {
    record(inner, judge, "content", source, verdict, by, None, reason);
}

/// The platform's own outbound call was stopped by the host policy: a
/// connector step or an account check asked for a host `security.net.*`
/// forbids, or one the definition never declared. Recorded like any guard
/// decision — the tool is `connector`, the subject the method and the host.
pub fn record_host_refusal(inner: &Inner, home: Option<Home>, subject: &str, reason: &str) {
    let judge = Judge::platform(home, None);
    let subject = inner.security.redact(subject).text;
    record(
        inner,
        &judge,
        "connector",
        &subject,
        GuardVerdict::Denied,
        GuardJudge::Rule,
        Some("security.net".to_string()),
        Some(reason.to_string()),
    );
}

/// The platform was about to hand a placeholder to a host as a literal — a
/// connector step whose rendered parameter still carries one. Recorded as a
/// refusal by the unresolved rule, like a command's; returns the sentence
/// the step fails with.
pub fn record_unresolved(
    inner: &Inner,
    home: Option<Home>,
    tool: &str,
    subject: &str,
    placeholders: &[String],
) -> String {
    let RuleVerdict::Deny { rule, reason } = unresolved_deny(placeholders) else {
        unreachable!("an unresolved placeholder is a deny");
    };
    let judge = Judge::platform(home, None);
    let subject = inner.security.redact(subject).text;
    record(
        inner,
        &judge,
        tool,
        &subject,
        GuardVerdict::Denied,
        GuardJudge::Rule,
        Some(rule),
        Some(reason.clone()),
    );
    reason
}

/// A person answered a guard's question in the Inbox: record it as theirs,
/// and keep it for the next identical call on this goal or run.
pub fn record_person(inner: &Inner, judge: &Judge<'_>, tool: &str, input: &Value, approve: bool) {
    let subject = subject_of(inner, tool, input);
    let home = home();
    let call = ToolCall {
        tool,
        input,
        cwd: judge.cwd,
        home: home.as_deref(),
        host: judge.host,
        canonical: bisa_core::caps::canonical_tool(tool),
    };
    let model = model_subject(inner, tool, &subject, &call, home.as_deref());
    inner.security.remember_answer(
        judge,
        bisa_security::classify::subject_digest(&model),
        approve,
    );
    record(
        inner,
        judge,
        tool,
        &subject,
        if approve {
            GuardVerdict::Allowed
        } else {
            GuardVerdict::Denied
        },
        GuardJudge::Person,
        None,
        None,
    );
}

// ---------------------------------------------------------------------------
// Platform-run commands
// ---------------------------------------------------------------------------

/// What to do with a command the platform itself is about to run.
#[derive(Clone, Debug, PartialEq)]
pub enum CommandOutcome {
    /// Run `command`; show and journal `shown` (the redacted text).
    Run { command: String, shown: String },
    /// Do not run it; the sentence says why.
    Refused(String),
}

/// Judge a `sh -c` command the platform runs on its own — a `check` step, a
/// probe. `ask` and a harmful or missing verdict go to the person as an
/// Escalation gate in the command's home — its goal, or its run of the
/// workspace — and are refused when it has none.
pub async fn decide_command(inner: &Inner, command: &str, judge: Judge<'_>) -> CommandOutcome {
    let input = serde_json::json!({ "command": command });
    let shown = subject_of(inner, "sh", &input);
    let run = |input: Value| CommandOutcome::Run {
        command: input
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or(command)
            .to_string(),
        shown: shown.clone(),
    };
    match decide_tool(inner, "sh", &input, judge.clone(), None).await {
        Outcome::Allow { input, .. } | Outcome::Fallthrough { input } => run(input),
        Outcome::Deny { reason, .. } => CommandOutcome::Refused(reason),
        Outcome::Ask {
            question, input, ..
        } => {
            let Some(home) = judge.home else {
                return CommandOutcome::Refused("the guard wanted a person's answer and this command runs for no goal and no run, so there is nobody to ask".into());
            };
            let (gate_id, rx) = inner.gates.open(
                home,
                None,
                Gate::Escalation,
                "guard:sh".to_string(),
                question.clone(),
                AskKind::Decision,
            );
            crate::ops::journal_question(
                inner,
                home,
                None,
                "guard:sh",
                &question,
                AskKind::Decision,
            );
            inner.emit(inner.home_scope(&home).event(
                None,
                EnginePayload::GateOpened {
                    gate_id,
                    gate: Gate::Escalation,
                    question,
                },
            ));
            let resolution = inner.gates.wait(rx).await;
            record_person(inner, &judge, "sh", &input, resolution.approve);
            if resolution.approve {
                run(input)
            } else {
                CommandOutcome::Refused("refused by the person who owns the work".into())
            }
        }
    }
}

/// The rules alone over a script, line by line — a workstream script the
/// machine trusts by hash still runs nothing a rule refuses. `None` when the
/// guard is off or no line is refused.
pub fn refuse_by_rules_lines(
    inner: &Inner,
    script: &str,
    cwd: Option<&Path>,
) -> Option<bisa_security::RefusedLine> {
    let policy = inner.security.policy();
    if !policy.guard_enabled {
        return None;
    }
    let home = home();
    policy
        .rules
        .guard
        .evaluate_lines(script, cwd, home.as_deref())
}

/// The rules alone, synchronously — for refusing a command at the moment it
/// is armed (a check start's, when its host turns on), before anything would
/// run it.
pub fn refuse_by_rules(inner: &Inner, command: &str) -> Option<String> {
    let policy = inner.security.policy();
    if !policy.guard_enabled {
        return None;
    }
    let input = serde_json::json!({ "command": command });
    let home = home();
    let call = ToolCall {
        tool: "sh",
        input: &input,
        cwd: None,
        home: home.as_deref(),
        host: Host::Platform,
        canonical: None,
    };
    match policy.rules.guard.evaluate(&call) {
        RuleVerdict::Deny { reason, .. } => Some(reason),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Terminal-hosted sessions
// ---------------------------------------------------------------------------

/// The guard hook's argv for a terminal-hosted harness, or `None` when this
/// machine does not guard them.
pub fn terminal_guard_argv(inner: &Inner, harness: &str) -> Option<Vec<String>> {
    let policy = inner.security.policy();
    if !policy.guard_enabled || !policy.terminal_hooks {
        return None;
    }
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "bisa".to_string());
    Some(vec![
        exe,
        "session".into(),
        "guard".into(),
        "--harness".into(),
        harness.to_string(),
    ])
}

/// How long the hook may wait: the classifier's deadline and a margin.
pub fn terminal_guard_timeout_secs(inner: &Inner) -> u64 {
    inner.security.policy().classifier.deadline_secs + 5
}

/// What the guard hook prints back to Claude Code.
#[derive(Clone, Debug, Default, PartialEq, Serialize, JsonSchema)]
pub struct GuardReply {
    /// `allow`, `deny` or `ask`; absent when the guard has no opinion and the
    /// harness's own prompt stands.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The input to run with, when a placeholder was restored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_input: Option<Value>,
}

/// Judge one hook payload from a terminal-hosted harness. The desk hosts a
/// person's own harnesses alone, so every payload here is a terminal's
/// ([`Host::Terminal`]): the rules that steer an agent to the platform's own
/// tools pass it over, and what protects the machine reads it as anyone's.
pub async fn guard_hook(inner: &Inner, session: LiveRunId, payload: &Value) -> GuardReply {
    let tool = payload
        .get("tool_name")
        .and_then(Value::as_str)
        .unwrap_or("?")
        .to_string();
    let input = payload.get("tool_input").cloned().unwrap_or(Value::Null);
    let cwd = payload
        .get("cwd")
        .and_then(Value::as_str)
        .map(PathBuf::from);
    let judge = Judge {
        home: None,
        session: Some(session),
        cwd: cwd.as_deref(),
        host: Host::Terminal,
        classifier: true,
        on_behalf_of: None,
    };
    match decide_tool(inner, &tool, &input, judge, None).await {
        Outcome::Deny { reason, .. } => GuardReply {
            decision: Some("deny".into()),
            reason: Some(reason),
            updated_input: None,
        },
        Outcome::Allow {
            input: restored, ..
        } => GuardReply {
            decision: Some("allow".into()),
            reason: None,
            updated_input: (restored != input).then_some(restored),
        },
        Outcome::Ask {
            reason,
            input: restored,
            ..
        } => GuardReply {
            decision: Some("ask".into()),
            reason,
            updated_input: (restored != input).then_some(restored),
        },
        // No opinion and nothing restored: say nothing, and Claude Code's own
        // prompt stands. A restored placeholder still has to reach the tool,
        // so that case asks — the person at the keyboard sees what will run.
        Outcome::Fallthrough { input: restored } => {
            if restored == input {
                GuardReply::default()
            } else {
                GuardReply {
                    decision: Some("ask".into()),
                    reason: None,
                    updated_input: Some(restored),
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

/// One harness's standing with the guard.
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct HarnessGuard {
    pub id: String,
    pub tool_guard: bool,
    pub input_rewrite: bool,
}

/// What Settings › Security shows.
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct SecurityStatus {
    pub redactor_enabled: bool,
    pub redact_rules: Vec<bisa_security::redact::RedactRule>,
    /// Whether the node's own environment is read for detectors, and how
    /// many of its variables are armed — a count, never a name or a value.
    pub env_auto: bool,
    pub env_detectors: usize,
    pub guard_enabled: bool,
    pub guard_rules: Vec<bisa_security::guard::GuardRule>,
    pub terminal_hooks: bool,
    pub problems: Vec<Problem>,
    pub classifier: ClassifierSettings,
    /// The content screen's two words (11-security §What an agent reads from outside).
    pub content: ContentSettings,
    /// Whether the classifier's agent exists, is enabled and names a harness
    /// this node can launch.
    pub classifier_ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub classifier_note: Option<String>,
    pub harnesses: Vec<HarnessGuard>,
    pub recent: Vec<Decision>,
    /// How many secrets the vault holds right now — a count, never a value.
    pub vault_size: usize,
}

/// Whether whoever reads for the classifier can be asked, and why not.
fn classifier_readiness(inner: &Inner, classifier: &ClassifierSettings) -> (bool, Option<String>) {
    match classifier.provider {
        ClassifierProvider::Agent => {}
        ClassifierProvider::Harness => {
            let launchable = inner.catalog.get(&classifier.harness).is_some();
            return (
                launchable,
                (!launchable).then(|| {
                    format!(
                        "{} is not a harness this node can launch",
                        classifier.harness
                    )
                }),
            );
        }
        ClassifierProvider::DecisionMakingAgent => {
            return match crate::decider::status(inner) {
                Ok(decider) => (
                    decider.ready,
                    decider
                        .problem
                        .map(|p| format!("the Decision-Making Agent cannot be asked: {p}")),
                ),
                Err(e) => (false, Some(e.to_string())),
            };
        }
    }
    match bisa_core::AgentId::new(&classifier.agent)
        .ok()
        .and_then(|id| crate::ops::agent_info(&inner.ws, &id))
    {
        Some(info) => {
            let launchable = info.harness.iter().any(|h| inner.catalog.get(h).is_some());
            (
                launchable,
                (!launchable).then(|| {
                    format!(
                        "none of {} is a harness this node can launch",
                        info.harness.join(", ")
                    )
                }),
            )
        }
        None => (
            false,
            Some(format!(
                "agent {:?} is missing or disabled",
                classifier.agent
            )),
        ),
    }
}

pub fn status(inner: &Inner) -> SecurityStatus {
    let policy = inner.security.policy();
    let (ready, note) = classifier_readiness(inner, &policy.classifier);
    SecurityStatus {
        redactor_enabled: policy.redactor_enabled,
        redact_rules: policy.rules.redact_rules.clone(),
        env_auto: policy.env_auto,
        env_detectors: policy.rules.env_detectors,
        guard_enabled: policy.guard_enabled,
        guard_rules: policy.rules.guard_rules.clone(),
        terminal_hooks: policy.terminal_hooks,
        problems: policy.problems.clone(),
        classifier: policy.classifier.clone(),
        content: policy.content,
        classifier_ready: ready,
        classifier_note: note,
        harnesses: inner
            .catalog
            .adapters()
            .iter()
            .map(|a| HarnessGuard {
                id: a.id().to_string(),
                tool_guard: a.caps().contains(bisa_core::HarnessCaps::TOOL_GUARD),
                input_rewrite: a.caps().contains(bisa_core::HarnessCaps::INPUT_REWRITE),
            })
            .collect(),
        recent: inner.security.recent(),
        vault_size: inner.security.vault().len(),
    }
}

/// What a redaction of `text` would do — on a vault of its own, so a preview
/// never teaches the real vault a secret.
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct RedactPreview {
    pub text: String,
    pub count: usize,
    pub kinds: Vec<String>,
}

pub fn redact_preview(inner: &Inner, text: &str) -> RedactPreview {
    let policy = inner.security.policy();
    let scratch = Vault::new(crate::interactive::mint_secret());
    let out = policy.rules.redactor.redact(&scratch, text);
    RedactPreview {
        text: out.text,
        count: out.count,
        kinds: out.kinds,
    }
}

/// What the rules alone say about a call — no classifier, no restore.
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct GuardPreview {
    /// `allow`, `deny`, `ask`, `classify` or `fallthrough`.
    pub verdict: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub paths: Vec<String>,
}

/// What the rules alone say about a call, as a session the platform drives
/// would meet them — no classifier, no restore, nothing recorded.
pub fn guard_preview(inner: &Inner, tool: &str, input: &Value, cwd: Option<&Path>) -> GuardPreview {
    let policy = inner.security.policy();
    let home = home();
    let call = ToolCall {
        tool,
        input,
        cwd,
        home: home.as_deref(),
        host: Host::Platform,
        canonical: bisa_core::caps::canonical_tool(tool),
    };
    let paths = call.paths();
    let (verdict, rule, reason) = match policy.rules.guard.evaluate(&call) {
        RuleVerdict::Allow { rule } => ("allow", Some(rule), None),
        RuleVerdict::Deny { rule, reason } => ("deny", Some(rule), Some(reason)),
        RuleVerdict::Ask { rule } => ("ask", Some(rule), None),
        RuleVerdict::Classify { rule } => ("classify", Some(rule), None),
        RuleVerdict::Fallthrough => ("fallthrough", None, None),
    };
    GuardPreview {
        verdict: verdict.to_string(),
        label: rule.as_deref().map(|r| rule_label(&policy, r)),
        rule,
        reason,
        paths,
    }
}

/// A `security.*` key changed: the next reader rebuilds the policy. A
/// `decisions.*` key changed too: with the classifier set to the
/// Decision-Making Agent, what it answered before was answered under the
/// provider, the model and the confidence of then, so every cached verdict
/// ends.
pub fn refresh_for(inner: &Inner, key: &str) {
    if key.starts_with("security.") || key.starts_with("decisions.") {
        inner.security.invalidate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_classifiers_provider_words_are_the_settings_choices() {
        assert_eq!(
            ClassifierProvider::of(Some("agent")),
            ClassifierProvider::Agent
        );
        assert_eq!(
            ClassifierProvider::of(Some("harness")),
            ClassifierProvider::Harness
        );
        assert_eq!(
            ClassifierProvider::of(Some("decision_making_agent")),
            ClassifierProvider::DecisionMakingAgent
        );
        assert_eq!(
            serde_json::to_value(ClassifierProvider::DecisionMakingAgent).unwrap(),
            serde_json::json!("decision_making_agent")
        );
    }

    /// A word the registry does not list never reaches here through a write;
    /// one that does — a file edited by hand — reads as the default, and the
    /// retired word is such a word and nothing more.
    #[test]
    fn the_retired_provider_word_reads_as_any_unknown_word_does() {
        let unknown = ClassifierProvider::of(Some("oracle"));
        let retired = ClassifierProvider::of(Some("decision_maker")); // terminology-lint-ignore: decision-maker - proves the retired word is refused
        assert_eq!(unknown, ClassifierProvider::Agent);
        assert_eq!(retired, unknown);
        assert_eq!(ClassifierProvider::of(None), ClassifierProvider::Agent);
    }
}
