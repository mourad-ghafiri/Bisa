//! Who hears an event, and whether what was heard is the event a start, a
//! wait or a boundary is waiting for.
//!
//! A **listener** is a start event armed for a **host**: the workspace, for a
//! library workflow a person turned On, or a goal whose workflow begins on an
//! event. What a host listens with — the inputs its event runs bind, the
//! budget each run spends against, since when, and whether a failure paused it
//! — is its [`Listening`].
//!
//! Everything the engine observes — a message, a project change, a run that
//! finished, a named signal, one of its own bus events — reaches the filters
//! below as one shape, [`Heard`], and each filter answers one question with
//! a pure function: *is this the event I wait for?* Matching is exact and
//! typed — a string against itself, anything else against its compact JSON,
//! `contains` case-insensitive — and there is no expression language (I35):
//! another way to match is a code change with a test.
//!
//! The filters are written once and used three ways: a start event's fields
//! render against the listening inputs, a `wait`'s and a boundary's against
//! the run. [`MessageFilter::resolve`] and its siblings take the renderer and
//! return the same shape with every template rendered and every input read;
//! `hears` is only ever asked of a resolved filter.
//!
//! The **causal chain** ([`Chain`]) is what keeps an event from feeding
//! itself: every signal carries the listeners that led to it, a run keeps the
//! chain of what started it and widens it with what it hears, and a listener
//! already in the chain — or a chain deeper than the workspace allows — is
//! refused.

use crate::assignee::Assignee;
use crate::goal::Budget;
use crate::id::{GoalId, ProjectId, RunId, WorkflowId};
use crate::workflow::{InputName, StepId, ValueRef};
use serde::de::Error as _;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

// ---------------------------------------------------------------------------
// Hosts, keys, listening
// ---------------------------------------------------------------------------

/// Who listens: the workspace, for a library workflow that is On, or a goal,
/// for the workflow it runs. Wire form `workspace:<WorkflowId>` or
/// `goal:<GoalId>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ListenerHost {
    Workspace { workflow: WorkflowId },
    Goal { goal: GoalId },
}

impl ListenerHost {
    /// The index's word for the host's kind.
    pub fn kind(&self) -> &'static str {
        match self {
            ListenerHost::Workspace { .. } => "workspace",
            ListenerHost::Goal { .. } => "goal",
        }
    }

    /// The host's own id, as text.
    pub fn id(&self) -> String {
        match self {
            ListenerHost::Workspace { workflow } => workflow.to_string(),
            ListenerHost::Goal { goal } => goal.to_string(),
        }
    }

    pub fn goal(&self) -> Option<GoalId> {
        match self {
            ListenerHost::Goal { goal } => Some(*goal),
            ListenerHost::Workspace { .. } => None,
        }
    }

    /// Rebuild a host from the index's two columns.
    pub fn from_parts(kind: &str, id: &str) -> Option<Self> {
        match kind {
            "workspace" => id
                .parse::<WorkflowId>()
                .ok()
                .map(|workflow| ListenerHost::Workspace { workflow }),
            "goal" => id
                .parse::<GoalId>()
                .ok()
                .map(|goal| ListenerHost::Goal { goal }),
            _ => None,
        }
    }
}

impl fmt::Display for ListenerHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.kind(), self.id())
    }
}

impl FromStr for ListenerHost {
    type Err = crate::CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.split_once(':')
            .and_then(|(kind, id)| ListenerHost::from_parts(kind, id))
            .ok_or_else(|| crate::CoreError::InvalidId {
                what: "listener host".to_string(),
                value: s.to_string(),
            })
    }
}

impl Serialize for ListenerHost {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ListenerHost {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?.parse().map_err(D::Error::custom)
    }
}

impl schemars::JsonSchema for ListenerHost {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("ListenerHost")
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "description": "who listens: `workspace:<WorkflowId>` or `goal:<GoalId>`"
        })
    }
}

/// One listener: a start step, armed for a host. Wire form
/// `<host>/<step>` — `workspace:<wf>/nightly`, `goal:<goal>/ticket`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ListenerKey {
    pub host: ListenerHost,
    pub step: StepId,
}

impl fmt::Display for ListenerKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.host, self.step)
    }
}

impl FromStr for ListenerKey {
    type Err = crate::CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || crate::CoreError::InvalidId {
            what: "listener".to_string(),
            value: s.to_string(),
        };
        let (host, step) = s.rsplit_once('/').ok_or_else(invalid)?;
        Ok(ListenerKey {
            host: host.parse().map_err(|_| invalid())?,
            step: StepId::new(step).map_err(|_| invalid())?,
        })
    }
}

impl Serialize for ListenerKey {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ListenerKey {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?.parse().map_err(D::Error::custom)
    }
}

impl schemars::JsonSchema for ListenerKey {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("ListenerKey")
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "description": "a listener: `<host>/<step>`, e.g. `workspace:<WorkflowId>/nightly`"
        })
    }
}

/// A host's standing: listening since when, with which inputs for the runs
/// its events start, under which per-run budget — and why it paused, when a
/// failed run or a spent budget paused it. Kept apart from a workflow's
/// definition, so turning a workflow On or Off is never a revision of it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Listening {
    /// The values the host gave when it began listening: what every run its
    /// events start binds, beneath what a start's mapping reads off the event.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, serde_json::Value>,
    /// Each run's own ceiling — a library workflow's; a goal's runs spend
    /// against the goal's budget. Absent, the workspace default applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<Budget>,
    pub since: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paused: Option<Paused>,
}

impl Listening {
    pub fn is_paused(&self) -> bool {
        self.paused.is_some()
    }
}

/// Why a host stopped hearing its events without being turned off.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Paused {
    pub reason: PauseReason,
    pub at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "reason")]
pub enum PauseReason {
    /// A run the events started failed: nothing more starts until a repair is
    /// adopted or the person says *listen again*.
    RunFailed { run: RunId },
    /// The goal's budget is spent.
    BudgetSpent,
}

impl PauseReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            PauseReason::RunFailed { .. } => "run_failed",
            PauseReason::BudgetSpent => "budget_spent",
        }
    }
}

// ---------------------------------------------------------------------------
// Signals' scope, sources and the causal chain
// ---------------------------------------------------------------------------

/// Where a heard event belongs: the whole workspace, or one goal. A goal's
/// run raises its signals on its goal.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "scope")]
pub enum SignalScope {
    #[default]
    Workspace,
    Goal {
        goal: GoalId,
    },
}

impl SignalScope {
    pub fn goal(&self) -> Option<GoalId> {
        match self {
            SignalScope::Workspace => None,
            SignalScope::Goal { goal } => Some(*goal),
        }
    }

    /// Whether `hearer` hears an event of this scope. The workspace's
    /// listeners hear everything; a goal's listener and a goal's run hear
    /// what is their goal's and what is nobody's in particular; a run of the
    /// workspace never hears a goal's.
    pub fn heard_by(&self, hearer: Hearer) -> bool {
        match (hearer, self) {
            (Hearer::WorkspaceListener, _) => true,
            (_, SignalScope::Workspace) => true,
            (Hearer::GoalListener(g) | Hearer::GoalRun(g), SignalScope::Goal { goal }) => {
                g == *goal
            }
            (Hearer::WorkspaceRun, SignalScope::Goal { .. }) => false,
        }
    }
}

/// Who is asking whether an event reaches them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hearer {
    WorkspaceListener,
    GoalListener(GoalId),
    GoalRun(GoalId),
    WorkspaceRun,
}

/// What kind of event a signal records — the start events' words, and
/// `test` for a run a person started as if an event had happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SignalSource {
    Schedule,
    Hook,
    Message,
    Signal,
    Project,
    Run,
    Platform,
    Connector,
    Check,
    Test,
}

impl SignalSource {
    pub const NAMES: [&'static str; 10] = [
        "schedule",
        "hook",
        "message",
        "signal",
        "project",
        "run",
        "platform",
        "connector",
        "check",
        "test",
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            SignalSource::Schedule => "schedule",
            SignalSource::Hook => "hook",
            SignalSource::Message => "message",
            SignalSource::Signal => "signal",
            SignalSource::Project => "project",
            SignalSource::Run => "run",
            SignalSource::Platform => "platform",
            SignalSource::Connector => "connector",
            SignalSource::Check => "check",
            SignalSource::Test => "test",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "schedule" => SignalSource::Schedule,
            "hook" => SignalSource::Hook,
            "message" => SignalSource::Message,
            "signal" => SignalSource::Signal,
            "project" => SignalSource::Project,
            "run" => SignalSource::Run,
            "platform" => SignalSource::Platform,
            "connector" => SignalSource::Connector,
            "check" => SignalSource::Check,
            "test" => SignalSource::Test,
            _ => return None,
        })
    }
}

/// The listeners one causal line of events passed through, oldest first,
/// and how many hops deep it is.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Chain {
    #[serde(default)]
    pub depth: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub listeners: Vec<ListenerKey>,
}

/// Why a listener refused a signal the chain would have carried to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChainRefusal {
    /// The listener already led to this event: it would feed itself.
    Repeats { chain: Vec<ListenerKey> },
    /// The line is deeper than the workspace allows.
    TooDeep { depth: u32, cap: u32 },
}

impl Chain {
    pub fn is_empty(&self) -> bool {
        self.depth == 0 && self.listeners.is_empty()
    }

    /// The chain a signal to `key` carries: this one, one hop deeper.
    pub fn extend(&self, key: &ListenerKey) -> Chain {
        let mut listeners = self.listeners.clone();
        listeners.push(key.clone());
        Chain {
            depth: self.depth.saturating_add(1),
            listeners,
        }
    }

    /// Widen this chain with another a run heard: the listeners of both, the
    /// deeper depth — so a line that passes through a `wait` is still seen.
    pub fn absorb(&mut self, other: &Chain) {
        for key in &other.listeners {
            if !self.listeners.contains(key) {
                self.listeners.push(key.clone());
            }
        }
        self.depth = self.depth.max(other.depth);
    }

    /// Whether carrying this chain to `key` is refused, and why.
    pub fn refuses(&self, key: &ListenerKey, cap: u32) -> Option<ChainRefusal> {
        if self.listeners.contains(key) {
            return Some(ChainRefusal::Repeats {
                chain: self.listeners.clone(),
            });
        }
        let depth = self.depth.saturating_add(1);
        let cap = cap.max(1);
        (depth > cap).then_some(ChainRefusal::TooDeep { depth, cap })
    }
}

/// One thing the engine observed, as every filter reads it: where it came
/// from, its name when it has one (a named signal's, a platform topic), who
/// it belongs to, what it carries, and the causal line behind it.
#[derive(Clone, Debug, PartialEq)]
pub struct Heard {
    pub source: SignalSource,
    pub name: Option<String>,
    pub scope: SignalScope,
    pub payload: serde_json::Value,
    pub chain: Chain,
}

// ---------------------------------------------------------------------------
// Matching primitives
// ---------------------------------------------------------------------------

/// An exact field match: a string against itself, anything else when the
/// wanted text parses to the same JSON — `count = "3"` matches the number 3,
/// `ok = "true"` the boolean. Nothing is guessed beyond that: `"03"` never
/// matches 3, and a missing field never matches anything, because a filter
/// that guessed would hear events its author never meant.
pub fn field_matches(found: Option<&serde_json::Value>, want: &str) -> bool {
    match found {
        Some(serde_json::Value::String(s)) => s == want,
        Some(other) => serde_json::from_str::<serde_json::Value>(want).is_ok_and(|w| &w == other),
        None => false,
    }
}

/// Every `path → value` of `fields` matches `payload` exactly.
pub fn fields_match(payload: &serde_json::Value, fields: &BTreeMap<String, String>) -> bool {
    fields
        .iter()
        .all(|(path, want)| field_matches(crate::template::json_path(payload, path), want))
}

/// `*` and `?` against a name — the whole glob language, on purpose: a
/// character class or `**` would need a grammar nobody asked for, and `*.csv`
/// is what a watched tree is for. A pattern with a `/` matches the whole
/// path; one without matches the last component.
pub fn glob_match(pattern: &str, path: &str) -> bool {
    fn go(p: &[char], n: &[char]) -> bool {
        match (p.first(), n.first()) {
            (None, None) => true,
            (Some('*'), _) => go(&p[1..], n) || (!n.is_empty() && go(p, &n[1..])),
            (Some('?'), Some(_)) => go(&p[1..], &n[1..]),
            (Some(a), Some(b)) if a == b => go(&p[1..], &n[1..]),
            _ => false,
        }
    }
    let subject = if pattern.contains('/') {
        path
    } else {
        path.rsplit('/').next().unwrap_or(path)
    };
    go(
        &pattern.chars().collect::<Vec<_>>(),
        &subject.chars().collect::<Vec<_>>(),
    )
}

fn text_contains(haystack: Option<&serde_json::Value>, needle: &str) -> bool {
    haystack
        .and_then(serde_json::Value::as_str)
        .is_some_and(|h| h.to_lowercase().contains(&needle.to_lowercase()))
}

// ---------------------------------------------------------------------------
// The filters
// ---------------------------------------------------------------------------

/// Who wrote a message a message event waits for.
///
/// `you` is this node's person; `agents` any agent; an assignee names one
/// agent, one person — a person hosted from another node only this way — or
/// a team, whose members' messages all count; `{ input = … }` reads an input
/// of kind `assignee`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum MessageFrom {
    #[default]
    You,
    Agents,
    Someone(ValueRef<Assignee>),
}

impl MessageFrom {
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageFrom::You => "you",
            MessageFrom::Agents => "agents",
            MessageFrom::Someone(_) => "someone",
        }
    }
}

impl Serialize for MessageFrom {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            MessageFrom::You => s.serialize_str("you"),
            MessageFrom::Agents => s.serialize_str("agents"),
            MessageFrom::Someone(who) => who.serialize(s),
        }
    }
}

/// The wire's own shape — a word or an assignee reference — rather than the
/// enum's, which the derive would spell as a tagged object nobody writes.
impl schemars::JsonSchema for MessageFrom {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("MessageFrom")
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let someone = generator.subschema_for::<ValueRef<Assignee>>();
        schemars::json_schema!({
            "description": "who wrote a message: `you`, `agents`, or one agent, person or team — or an input of kind `assignee` naming one",
            "anyOf": [
                { "type": "string", "enum": ["you", "agents"] },
                someone
            ]
        })
    }
}

impl<'de> Deserialize<'de> for MessageFrom {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = serde_json::Value::deserialize(d)?;
        match &raw {
            serde_json::Value::String(s) if s == "you" => Ok(MessageFrom::You),
            serde_json::Value::String(s) if s == "agents" => Ok(MessageFrom::Agents),
            _ => serde_json::from_value::<ValueRef<Assignee>>(raw)
                .map(MessageFrom::Someone)
                .map_err(|_| {
                    D::Error::custom(
                        "`from` is `you`, `agents`, an assignee ({ agent = … }, { human = … }, \
                         { team = … }) or { input = … }",
                    )
                }),
        }
    }
}

/// A message posted into a conversation: where, by whom, mentioning whom,
/// saying what. Heard where conversation dispatch hears a message — after the
/// hold a message from another node waits in — and never an announcement.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MessageFilter {
    /// The conversation it lands in — a channel id, a direct channel's, a
    /// conversation's or a goal's thread (a template); absent, anywhere.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#in: Option<String>,
    #[serde(default)]
    pub from: MessageFrom,
    /// It mentions this agent, team or person.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mentions: Option<ValueRef<Assignee>>,
    /// Its text contains this, case-insensitively (a template).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contains: Option<String>,
}

impl MessageFilter {
    pub const FIELDS: &'static [&'static str] = &["in", "from", "mentions", "contains"];

    /// The strings this filter renders.
    pub fn templates(&self) -> Vec<&str> {
        self.r#in
            .iter()
            .chain(self.contains.iter())
            .map(String::as_str)
            .collect()
    }

    /// The assignees this filter names: who wrote it, whom it mentions.
    pub fn assignee_refs(&self) -> Vec<&ValueRef<Assignee>> {
        let from = match &self.from {
            MessageFrom::Someone(who) => Some(who),
            MessageFrom::You | MessageFrom::Agents => None,
        };
        from.into_iter().chain(self.mentions.iter()).collect()
    }

    /// Every input this filter reads through a reference.
    pub fn input_refs(&self) -> Vec<(&InputName, &'static str)> {
        let mut out = Vec::new();
        if let MessageFrom::Someone(ValueRef::Input { input }) = &self.from {
            out.push((input, "assignee"));
        }
        if let Some(ValueRef::Input { input }) = &self.mentions {
            out.push((input, "assignee"));
        }
        out
    }

    /// This filter with every template rendered and every input read.
    pub fn resolve<E>(
        &self,
        render: &dyn Fn(&str) -> Result<String, E>,
        input: &dyn Fn(&InputName) -> Result<Assignee, E>,
    ) -> Result<Self, E> {
        let fixed = |r: &ValueRef<Assignee>| -> Result<ValueRef<Assignee>, E> {
            Ok(ValueRef::Fixed(match r {
                ValueRef::Fixed(a) => a.clone(),
                ValueRef::Input { input: name } => input(name)?,
            }))
        };
        Ok(MessageFilter {
            r#in: self.r#in.as_deref().map(render).transpose()?,
            from: match &self.from {
                MessageFrom::Someone(who) => MessageFrom::Someone(fixed(who)?),
                other => other.clone(),
            },
            mentions: self.mentions.as_ref().map(fixed).transpose()?,
            contains: self.contains.as_deref().map(render).transpose()?,
        })
    }

    /// Whether a heard message is the one this (resolved) filter waits for.
    /// The payload the engine builds: `{ scope, author, author_kind: you |
    /// agent | person, teams: [..], mentions: [..], text }`.
    pub fn hears(&self, heard: &Heard) -> bool {
        if heard.source != SignalSource::Message {
            return false;
        }
        let p = &heard.payload;
        let get = |k: &str| p.get(k);
        if let Some(place) = &self.r#in {
            if get("scope").and_then(serde_json::Value::as_str) != Some(place.as_str()) {
                return false;
            }
        }
        let kind = get("author_kind").and_then(serde_json::Value::as_str);
        let author = get("author").and_then(serde_json::Value::as_str);
        let from = match &self.from {
            MessageFrom::You => kind == Some("you"),
            MessageFrom::Agents => kind == Some("agent"),
            MessageFrom::Someone(ValueRef::Fixed(who)) => match who {
                Assignee::Agent(id) => kind == Some("agent") && author == Some(id.as_str()),
                Assignee::Human(pk) => author == Some(pk.as_hex()),
                Assignee::Team(team) => get("teams")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|ts| ts.iter().any(|t| t.as_str() == Some(team.as_str()))),
            },
            MessageFrom::Someone(ValueRef::Input { .. }) => false,
        };
        if !from {
            return false;
        }
        if let Some(ValueRef::Fixed(who)) = &self.mentions {
            let want = match who {
                Assignee::Agent(id) | Assignee::Team(id) => id.clone(),
                Assignee::Human(pk) => pk.as_hex().to_string(),
            };
            let named = get("mentions")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|ms| ms.iter().any(|m| m.as_str() == Some(want.as_str())));
            if !named {
                return false;
            }
        }
        match &self.contains {
            Some(needle) => text_contains(get("text"), needle),
            None => true,
        }
    }
}

/// What changed in a project.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ProjectChange {
    /// A branch head moved — a commit, whoever made it.
    #[default]
    Commit,
    /// A remote-tracking branch moved: this machine pushed or fetched.
    Push,
    /// A pull request the platform's workstreams opened or adopted changed.
    PullRequest,
    /// One of those pull requests was merged.
    Merge,
    /// Files in the tree changed.
    Files,
}

impl ProjectChange {
    pub fn as_str(self) -> &'static str {
        match self {
            ProjectChange::Commit => "commit",
            ProjectChange::Push => "push",
            ProjectChange::PullRequest => "pull_request",
            ProjectChange::Merge => "merge",
            ProjectChange::Files => "files",
        }
    }
}

/// A change in one project — never a path on a disk, which would mean
/// nothing on another machine.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectFilter {
    /// Absent while the designer has not chosen one: a problem, never `""`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<ValueRef<ProjectId>>,
    #[serde(default)]
    pub change: ProjectChange,
    /// Only this branch (a template).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Only files matching this (`*` and `?`), for `files` (a template).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glob: Option<String>,
}

impl ProjectFilter {
    pub const FIELDS: &'static [&'static str] = &["project", "change", "branch", "glob"];

    pub fn templates(&self) -> Vec<&str> {
        self.branch
            .iter()
            .chain(self.glob.iter())
            .map(String::as_str)
            .collect()
    }

    pub fn input_refs(&self) -> Vec<(&InputName, &'static str)> {
        match &self.project {
            Some(ValueRef::Input { input }) => vec![(input, "project")],
            _ => vec![],
        }
    }

    pub fn resolve<E>(
        &self,
        render: &dyn Fn(&str) -> Result<String, E>,
        input: &dyn Fn(&InputName) -> Result<ProjectId, E>,
    ) -> Result<Self, E> {
        Ok(ProjectFilter {
            project: match &self.project {
                Some(ValueRef::Input { input: name }) => Some(ValueRef::Fixed(input(name)?)),
                other => other.clone(),
            },
            change: self.change,
            branch: self.branch.as_deref().map(render).transpose()?,
            glob: self.glob.as_deref().map(render).transpose()?,
        })
    }

    /// The project this (resolved) filter names, when it names one.
    pub fn project_id(&self) -> Option<ProjectId> {
        match &self.project {
            Some(ValueRef::Fixed(p)) => Some(*p),
            _ => None,
        }
    }

    /// The payload the engine builds: `{ project, change, branch?, paths? }`.
    pub fn hears(&self, heard: &Heard) -> bool {
        if heard.source != SignalSource::Project {
            return false;
        }
        let p = &heard.payload;
        let Some(project) = self.project_id() else {
            return false;
        };
        if p.get("project").and_then(serde_json::Value::as_str)
            != Some(project.to_string().as_str())
        {
            return false;
        }
        if p.get("change").and_then(serde_json::Value::as_str) != Some(self.change.as_str()) {
            return false;
        }
        if let Some(branch) = &self.branch {
            if p.get("branch").and_then(serde_json::Value::as_str) != Some(branch.as_str()) {
                return false;
            }
        }
        match &self.glob {
            Some(glob) => p
                .get("paths")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|ps| {
                    ps.iter()
                        .filter_map(serde_json::Value::as_str)
                        .any(|path| glob_match(glob, path))
                }),
            None => true,
        }
    }
}

/// How a run ended, as a run event hears it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunEnd {
    Done,
    Failed,
    Cancelled,
}

impl RunEnd {
    pub fn as_str(self) -> &'static str {
        match self {
            RunEnd::Done => "done",
            RunEnd::Failed => "failed",
            RunEnd::Cancelled => "cancelled",
        }
    }
}

/// A run that ended — of one workflow, or of any.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunFilter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow: Option<WorkflowId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<RunEnd>,
}

impl RunFilter {
    pub const FIELDS: &'static [&'static str] = &["workflow", "outcome"];

    /// The payload the engine builds: `{ run, workflow, outcome, goal? }`.
    pub fn hears(&self, heard: &Heard) -> bool {
        if heard.source != SignalSource::Run {
            return false;
        }
        let p = &heard.payload;
        if let Some(wf) = &self.workflow {
            if p.get("workflow").and_then(serde_json::Value::as_str)
                != Some(wf.to_string().as_str())
            {
                return false;
            }
        }
        match self.outcome {
            Some(end) => p.get("outcome").and_then(serde_json::Value::as_str) == Some(end.as_str()),
            None => true,
        }
    }
}

/// One of the engine's own bus events, by topic (`goal.closed`,
/// `step.changed`) and exact fields — the advanced door, for what the other
/// events do not name.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlatformFilter {
    pub topic: String,
    /// Exact matches on the event's own fields (templates).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, String>,
}

impl PlatformFilter {
    pub const FIELDS: &'static [&'static str] = &["topic", "fields"];

    pub fn templates(&self) -> Vec<&str> {
        self.fields.values().map(String::as_str).collect()
    }

    pub fn resolve<E>(&self, render: &dyn Fn(&str) -> Result<String, E>) -> Result<Self, E> {
        Ok(PlatformFilter {
            topic: self.topic.clone(),
            fields: self
                .fields
                .iter()
                .map(|(k, v)| Ok((k.clone(), render(v)?)))
                .collect::<Result<_, E>>()?,
        })
    }

    /// The payload the engine builds: `{ event, fields: { … }, goal?, workflow? }`.
    pub fn hears(&self, heard: &Heard) -> bool {
        heard.source == SignalSource::Platform
            && heard.name.as_deref() == Some(self.topic.as_str())
            && heard
                .payload
                .get("fields")
                .is_some_and(|f| fields_match(f, &self.fields))
    }
}

/// A named signal — raised by an `emit` step, a session's `emit_signal`, a
/// person through the CLI or the node, an A2A task — with exact fields on its
/// payload.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SignalFilter {
    /// The signal's name (a template).
    pub name: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, String>,
}

impl SignalFilter {
    pub const FIELDS: &'static [&'static str] = &["name", "fields"];

    pub fn templates(&self) -> Vec<&str> {
        std::iter::once(self.name.as_str())
            .chain(self.fields.values().map(String::as_str))
            .collect()
    }

    pub fn resolve<E>(&self, render: &dyn Fn(&str) -> Result<String, E>) -> Result<Self, E> {
        Ok(SignalFilter {
            name: render(&self.name)?,
            fields: self
                .fields
                .iter()
                .map(|(k, v)| Ok((k.clone(), render(v)?)))
                .collect::<Result<_, E>>()?,
        })
    }

    pub fn hears(&self, heard: &Heard) -> bool {
        heard.source == SignalSource::Signal
            && heard.name.as_deref() == Some(self.name.as_str())
            && fields_match(&heard.payload, &self.fields)
    }
}

/// A signal's name: dotted words of `a-z`, `0-9`, `_` and `-`, so a name reads
/// the same everywhere it is typed (`report.ready`, `deploy.finished`).
pub fn valid_signal_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn wf(n: u64) -> WorkflowId {
        WorkflowId::from_ulid(ulid::Ulid::from_parts(n, 1))
    }

    fn goal(n: u64) -> GoalId {
        GoalId::from_ulid(ulid::Ulid::from_parts(n, 2))
    }

    fn key(host: ListenerHost, step: &str) -> ListenerKey {
        ListenerKey {
            host,
            step: StepId::new(step).unwrap(),
        }
    }

    fn heard(source: SignalSource, name: Option<&str>, payload: serde_json::Value) -> Heard {
        Heard {
            source,
            name: name.map(str::to_string),
            scope: SignalScope::Workspace,
            payload,
            chain: Chain::default(),
        }
    }

    #[test]
    fn hosts_and_keys_round_trip_as_words() {
        for host in [
            ListenerHost::Workspace { workflow: wf(1) },
            ListenerHost::Goal { goal: goal(2) },
        ] {
            let text = host.to_string();
            assert_eq!(text.parse::<ListenerHost>().unwrap(), host);
            assert_eq!(
                ListenerHost::from_parts(host.kind(), &host.id()),
                Some(host)
            );
            let k = key(host, "nightly");
            assert_eq!(k.to_string(), format!("{text}/nightly"));
            assert_eq!(k.to_string().parse::<ListenerKey>().unwrap(), k);
            let json = serde_json::to_string(&k).unwrap();
            assert_eq!(serde_json::from_str::<ListenerKey>(&json).unwrap(), k);
        }
        assert!("workflow:x/y".parse::<ListenerKey>().is_err());
        assert!("goal:not-a-ulid".parse::<ListenerHost>().is_err());
        assert!(format!("goal:{}/Bad Step", goal(1))
            .parse::<ListenerKey>()
            .is_err());
    }

    #[test]
    fn a_workspace_listener_hears_everything_and_a_goal_only_its_own() {
        let a = goal(1);
        let b = goal(2);
        let ws = SignalScope::Workspace;
        let of_a = SignalScope::Goal { goal: a };
        assert!(of_a.heard_by(Hearer::WorkspaceListener));
        assert!(ws.heard_by(Hearer::GoalListener(a)));
        assert!(of_a.heard_by(Hearer::GoalListener(a)));
        assert!(!of_a.heard_by(Hearer::GoalListener(b)));
        assert!(of_a.heard_by(Hearer::GoalRun(a)));
        assert!(!of_a.heard_by(Hearer::GoalRun(b)));
        assert!(ws.heard_by(Hearer::WorkspaceRun));
        assert!(
            !of_a.heard_by(Hearer::WorkspaceRun),
            "a run of the workspace never hears a goal's"
        );
    }

    #[test]
    fn the_chain_refuses_a_repeat_and_a_line_too_deep() {
        let one = key(ListenerHost::Workspace { workflow: wf(1) }, "a");
        let two = key(ListenerHost::Workspace { workflow: wf(2) }, "b");
        let start = Chain::default();
        assert!(start.is_empty());
        assert_eq!(start.refuses(&one, 3), None);
        let c1 = start.extend(&one);
        assert_eq!(c1.depth, 1);
        assert!(matches!(
            c1.refuses(&one, 3),
            Some(ChainRefusal::Repeats { .. })
        ));
        assert_eq!(c1.refuses(&two, 3), None);
        let c2 = c1.extend(&two);
        assert_eq!(
            c2.refuses(&key(ListenerHost::Goal { goal: goal(9) }, "c"), 2),
            Some(ChainRefusal::TooDeep { depth: 3, cap: 2 })
        );
        // A zero cap still allows one hop: an event heard is one hop.
        assert_eq!(Chain::default().refuses(&one, 0), None);
        let mut run = c1.clone();
        run.absorb(&c2);
        assert_eq!(run.listeners, vec![one.clone(), two.clone()]);
        assert_eq!(run.depth, 2);
        run.absorb(&Chain::default());
        assert_eq!(run.depth, 2, "absorbing nothing changes nothing");
    }

    #[test]
    fn a_message_is_heard_by_who_wrote_it_where_and_what_it_says() {
        let msg = |kind: &str, author: &str, text: &str| {
            heard(
                SignalSource::Message,
                None,
                json!({
                    "scope": "support",
                    "author": author,
                    "author_kind": kind,
                    "teams": ["ops"],
                    "mentions": ["triager"],
                    "text": text
                }),
            )
        };
        let mine = msg("you", "aa", "Please REFUND me");
        let default = MessageFilter::default();
        assert!(default.hears(&mine), "`from` defaults to you");
        assert!(!default.hears(&msg("agent", "triager", "hi")));
        assert!(
            !default.hears(&msg("person", "bb", "hi")),
            "a person hosted from another node is heard only when named"
        );
        let agents = MessageFilter {
            from: MessageFrom::Agents,
            ..MessageFilter::default()
        };
        assert!(agents.hears(&msg("agent", "triager", "hi")));
        let named = MessageFilter {
            from: MessageFrom::Someone(ValueRef::Fixed(
                "human:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                    .parse()
                    .unwrap(),
            )),
            ..MessageFilter::default()
        };
        assert!(named.hears(&msg(
            "person",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "hi"
        )));
        let team = MessageFilter {
            from: MessageFrom::Someone(ValueRef::Fixed(Assignee::Team("ops".into()))),
            ..MessageFilter::default()
        };
        assert!(team.hears(&msg("agent", "anyone", "hi")));
        let precise = MessageFilter {
            r#in: Some("support".into()),
            mentions: Some(ValueRef::Fixed(Assignee::Agent("triager".into()))),
            contains: Some("refund".into()),
            ..MessageFilter::default()
        };
        assert!(precise.hears(&mine), "contains is case-insensitive");
        assert!(!precise.hears(&msg("you", "aa", "hello")));
        let elsewhere = MessageFilter {
            r#in: Some("general".into()),
            ..MessageFilter::default()
        };
        assert!(!elsewhere.hears(&mine));
        assert!(
            !default.hears(&heard(SignalSource::Signal, Some("x"), json!({}))),
            "a message filter hears messages only"
        );
    }

    #[test]
    fn message_from_reads_its_words_and_assignees() {
        let parse = |v: serde_json::Value| serde_json::from_value::<MessageFrom>(v);
        assert_eq!(parse(json!("you")).unwrap(), MessageFrom::You);
        assert_eq!(parse(json!("agents")).unwrap(), MessageFrom::Agents);
        assert_eq!(
            parse(json!({"agent": "sre"})).unwrap(),
            MessageFrom::Someone(ValueRef::Fixed(Assignee::Agent("sre".into())))
        );
        assert!(matches!(
            parse(json!({"input": "who"})).unwrap(),
            MessageFrom::Someone(ValueRef::Input { .. })
        ));
        assert!(parse(json!("everyone")).is_err());
        for from in [
            MessageFrom::You,
            MessageFrom::Agents,
            MessageFrom::Someone(ValueRef::Fixed(Assignee::Team("ops".into()))),
        ] {
            let json = serde_json::to_value(&from).unwrap();
            assert_eq!(parse(json).unwrap(), from);
        }
        assert!(
            serde_json::from_value::<MessageFilter>(json!({"inn": "x"})).is_err(),
            "a misspelled key is refused"
        );
    }

    #[test]
    fn a_project_change_is_heard_by_project_change_branch_and_files() {
        let project = ProjectId::from_ulid(ulid::Ulid::from_parts(5, 5));
        let change = |c: &str, branch: &str, paths: serde_json::Value| {
            heard(
                SignalSource::Project,
                None,
                json!({"project": project.to_string(), "change": c, "branch": branch, "paths": paths}),
            )
        };
        let commits = ProjectFilter {
            project: Some(ValueRef::Fixed(project)),
            change: ProjectChange::Commit,
            branch: Some("main".into()),
            glob: None,
        };
        assert!(commits.hears(&change("commit", "main", json!([]))));
        assert!(!commits.hears(&change("commit", "dev", json!([]))));
        assert!(!commits.hears(&change("merge", "main", json!([]))));
        let csv = ProjectFilter {
            project: Some(ValueRef::Fixed(project)),
            change: ProjectChange::Files,
            branch: None,
            glob: Some("*.csv".into()),
        };
        assert!(csv.hears(&change("files", "", json!(["in/a.csv", "b.txt"]))));
        assert!(!csv.hears(&change("files", "", json!(["b.txt"]))));
        let unchosen = ProjectFilter::default();
        assert!(
            !unchosen.hears(&change("commit", "main", json!([]))),
            "a filter naming no project hears nothing"
        );
        let other = ProjectId::from_ulid(ulid::Ulid::from_parts(6, 6));
        assert!(!ProjectFilter {
            project: Some(ValueRef::Fixed(other)),
            ..commits
        }
        .hears(&change("commit", "main", json!([]))));
    }

    #[test]
    fn globs_match_names_or_whole_paths() {
        assert!(glob_match("*.csv", "drop/in/a.csv"));
        assert!(glob_match("report-?.md", "docs/report-1.md"));
        assert!(!glob_match("*.csv", "a.tsv"));
        assert!(glob_match("docs/*.md", "docs/a.md"));
        assert!(!glob_match("docs/*.md", "src/a.md"));
    }

    #[test]
    fn a_run_filter_hears_a_workflow_and_an_outcome() {
        let ended = |wf_id: WorkflowId, outcome: &str| {
            heard(
                SignalSource::Run,
                None,
                json!({"run": "r", "workflow": wf_id.to_string(), "outcome": outcome}),
            )
        };
        let any_failure = RunFilter {
            workflow: None,
            outcome: Some(RunEnd::Failed),
        };
        assert!(any_failure.hears(&ended(wf(1), "failed")));
        assert!(!any_failure.hears(&ended(wf(1), "done")));
        let of_one = RunFilter {
            workflow: Some(wf(2)),
            outcome: None,
        };
        assert!(of_one.hears(&ended(wf(2), "cancelled")));
        assert!(!of_one.hears(&ended(wf(1), "done")));
    }

    #[test]
    fn platform_and_signal_fields_match_exactly_and_never_coerce() {
        let bus = heard(
            SignalSource::Platform,
            Some("step.changed"),
            json!({"event": "step.changed", "fields": {"state": "failed", "n": 12}}),
        );
        let failed = PlatformFilter {
            topic: "step.changed".into(),
            fields: BTreeMap::from([("state".into(), "failed".into())]),
        };
        assert!(failed.hears(&bus));
        assert!(PlatformFilter {
            topic: "step.changed".into(),
            fields: BTreeMap::from([("n".into(), "12".into())]),
        }
        .hears(&bus));
        assert!(!PlatformFilter {
            topic: "run.finished".into(),
            fields: BTreeMap::new(),
        }
        .hears(&bus));
        let sig = heard(
            SignalSource::Signal,
            Some("deploy.finished"),
            json!({"env": "prod", "count": "3"}),
        );
        let prod = SignalFilter {
            name: "deploy.finished".into(),
            fields: BTreeMap::from([("env".into(), "prod".into())]),
        };
        assert!(prod.hears(&sig));
        assert!(!SignalFilter {
            name: "deploy.finished".into(),
            fields: BTreeMap::from([("env".into(), "dev".into())]),
        }
        .hears(&sig));
        assert!(
            !SignalFilter {
                name: "hook".into(),
                fields: BTreeMap::new(),
            }
            .hears(&heard(SignalSource::Hook, None, json!({}))),
            "a named signal is never a hook call"
        );
        assert!(field_matches(Some(&json!(3)), "3"));
        assert!(field_matches(Some(&json!(true)), "true"));
        assert!(field_matches(Some(&json!({"a": 1})), "{ \"a\": 1 }"));
        assert!(!field_matches(Some(&json!(3)), "03"));
        assert!(!field_matches(Some(&json!("3")), "03"));
        assert!(!field_matches(None, "x"));
    }

    #[test]
    fn filters_resolve_their_templates_and_inputs() {
        let render =
            |t: &str| -> Result<String, String> { Ok(t.replace("{inputs.chan}", "support")) };
        let who = |_: &InputName| -> Result<Assignee, String> { Ok(Assignee::Agent("sre".into())) };
        let filter = MessageFilter {
            r#in: Some("{inputs.chan}".into()),
            from: MessageFrom::Someone(ValueRef::Input {
                input: InputName::new("who").unwrap(),
            }),
            mentions: None,
            contains: None,
        };
        assert_eq!(filter.input_refs().len(), 1);
        let resolved = filter.resolve(&render, &who).unwrap();
        assert_eq!(resolved.r#in.as_deref(), Some("support"));
        assert_eq!(
            resolved.from,
            MessageFrom::Someone(ValueRef::Fixed(Assignee::Agent("sre".into())))
        );
        let failing = |_: &str| -> Result<String, String> { Err("no".into()) };
        assert!(SignalFilter {
            name: "{inputs.x}".into(),
            fields: BTreeMap::new(),
        }
        .resolve(&failing)
        .is_err());
    }

    #[test]
    fn signal_names_are_dotted_words() {
        for good in ["report.ready", "deploy-finished", "a2a.task", "x"] {
            assert!(valid_signal_name(good), "{good}");
        }
        for bad in ["", "Report", "a..b", ".a", "a b", "a/b"] {
            assert!(!valid_signal_name(bad), "{bad}");
        }
    }

    #[test]
    fn listening_and_pauses_round_trip() {
        let l = Listening {
            inputs: BTreeMap::from([("command".into(), json!("sh -c 'true'"))]),
            budget: None,
            since: 7,
            paused: Some(Paused {
                reason: PauseReason::RunFailed {
                    run: RunId::from_ulid(ulid::Ulid::from_parts(3, 3)),
                },
                at: 9,
            }),
        };
        let json = serde_json::to_string(&l).unwrap();
        assert_eq!(serde_json::from_str::<Listening>(&json).unwrap(), l);
        assert!(l.is_paused());
        assert_eq!(l.paused.as_ref().unwrap().reason.as_str(), "run_failed");
        for s in SignalSource::NAMES {
            assert_eq!(SignalSource::parse(s).unwrap().as_str(), s);
        }
    }

    // added by the coverage pass: listen.rs

    #[test]
    fn the_wire_words_of_a_pause_a_source_a_change_an_end_and_a_sender() {
        assert_eq!(PauseReason::BudgetSpent.as_str(), "budget_spent");
        for source in [
            SignalSource::Schedule,
            SignalSource::Hook,
            SignalSource::Message,
            SignalSource::Signal,
            SignalSource::Project,
            SignalSource::Run,
            SignalSource::Platform,
            SignalSource::Connector,
            SignalSource::Check,
            SignalSource::Test,
        ] {
            assert_eq!(SignalSource::parse(source.as_str()), Some(source));
        }
        assert_eq!(SignalSource::parse("nope"), None);
        for (change, word) in [
            (ProjectChange::Commit, "commit"),
            (ProjectChange::Push, "push"),
            (ProjectChange::PullRequest, "pull_request"),
            (ProjectChange::Merge, "merge"),
            (ProjectChange::Files, "files"),
        ] {
            assert_eq!(change.as_str(), word);
        }
        for (end, word) in [
            (RunEnd::Done, "done"),
            (RunEnd::Failed, "failed"),
            (RunEnd::Cancelled, "cancelled"),
        ] {
            assert_eq!(end.as_str(), word);
        }
        let someone = MessageFrom::Someone(ValueRef::Fixed(Assignee::Agent("triager".into())));
        assert_eq!(
            (
                MessageFrom::You.as_str(),
                MessageFrom::Agents.as_str(),
                someone.as_str()
            ),
            ("you", "agents", "someone")
        );
    }

    #[test]
    fn a_goal_host_names_its_goal_and_a_host_is_read_from_json_as_its_word() {
        let host = ListenerHost::Goal { goal: goal(1) };
        assert_eq!(host.goal(), Some(goal(1)));
        assert_eq!(ListenerHost::Workspace { workflow: wf(1) }.goal(), None);
        let read: ListenerHost = serde_json::from_value(json!(host.to_string())).unwrap();
        assert_eq!(read, host);
        assert!(serde_json::from_value::<ListenerHost>(json!("planet:x")).is_err());
    }

    #[test]
    fn a_message_filter_reads_a_mention_input_and_hears_nobody_through_an_unresolved_sender_or_a_mention_the_message_lacks(
    ) {
        let input = InputName::new("who").unwrap();
        let filter = MessageFilter {
            mentions: Some(ValueRef::Input {
                input: input.clone(),
            }),
            ..MessageFilter::default()
        };
        assert_eq!(filter.input_refs(), vec![(&input, "assignee")]);
        let msg = heard(
            SignalSource::Message,
            None,
            json!({"scope": "s", "author": "aa", "author_kind": "you", "mentions": [], "text": "hi"}),
        );
        let unresolved = MessageFilter {
            from: MessageFrom::Someone(ValueRef::Input { input }),
            ..MessageFilter::default()
        };
        assert!(
            !unresolved.hears(&msg),
            "an unresolved sender hears nothing"
        );
        let hex = "bb".repeat(32);
        let person: Assignee = format!("human:{hex}").parse().unwrap();
        let mentions_a_person = MessageFilter {
            mentions: Some(ValueRef::Fixed(person)),
            ..MessageFilter::default()
        };
        assert!(
            !mentions_a_person.hears(&msg),
            "a person the message does not mention"
        );
    }

    // added by the coverage pass: b4-listen.rs
    #[test]
    fn a_host_writes_as_its_word_a_scope_names_its_goal_a_project_filter_reads_its_input_and_a_person_is_heard_by_their_key(
    ) {
        let host = ListenerHost::Goal { goal: goal(2) };
        let word = host.to_string();
        assert_eq!(serde_json::to_value(host).unwrap(), json!(word));
        assert_eq!(SignalScope::Goal { goal: goal(2) }.goal(), Some(goal(2)));
        assert_eq!(SignalScope::Workspace.goal(), None);
        let repo = InputName::new("repo").unwrap();
        let filter = ProjectFilter {
            project: Some(ValueRef::Input {
                input: repo.clone(),
            }),
            ..ProjectFilter::default()
        };
        assert_eq!(filter.input_refs(), vec![(&repo, "project")]);
        assert_eq!(filter.project_id(), None);
        assert!(
            !RunFilter::default().hears(&heard(SignalSource::Signal, Some("x"), json!({}))),
            "a run filter hears runs alone"
        );
        assert!(
            !ProjectFilter::default().hears(&heard(SignalSource::Signal, Some("x"), json!({}))),
            "a project filter hears projects alone"
        );
        let hex = "bb".repeat(32);
        let person: Assignee = format!("human:{hex}").parse().unwrap();
        let from_them = MessageFilter {
            from: MessageFrom::Someone(ValueRef::Fixed(person)),
            ..MessageFilter::default()
        };
        let theirs = heard(
            SignalSource::Message,
            None,
            json!({"scope": "s", "author": hex, "author_kind": "person", "mentions": [], "text": "hi"}),
        );
        assert!(from_them.hears(&theirs));
    }

    #[test]
    fn a_filter_resolves_its_fixed_parts_as_themselves_and_its_templates_through_the_renderer() {
        let render = |t: &str| -> Result<String, String> { Ok(t.replace("{x}", "X")) };
        let input = |n: &InputName| -> Result<Assignee, String> { Err(format!("{n} is not read")) };
        let fixed = MessageFilter {
            from: MessageFrom::Someone(ValueRef::Fixed(Assignee::Agent("a".into()))),
            mentions: Some(ValueRef::Fixed(Assignee::Agent("b".into()))),
            ..MessageFilter::default()
        };
        assert_eq!(fixed.resolve(&render, &input).unwrap(), fixed);
        let you = MessageFilter::default();
        assert_eq!(you.resolve(&render, &input).unwrap(), you);
        let platform = PlatformFilter {
            topic: "t".into(),
            fields: BTreeMap::from([("k".to_string(), "{x}".to_string())]),
        };
        assert_eq!(
            platform.resolve(&render).unwrap().fields["k"],
            "X",
            "a platform filter's fields are templates"
        );
        let signal = SignalFilter {
            name: "s.{x}".into(),
            fields: BTreeMap::from([("k".to_string(), "{x}".to_string())]),
        };
        let resolved = signal.resolve(&render).unwrap();
        assert_eq!(
            (resolved.name.as_str(), resolved.fields["k"].as_str()),
            ("s.X", "X")
        );
    }

    #[test]
    fn the_hand_written_schemas_of_the_listening_words_are_a_string_or_a_choice() {
        for schema in [
            schemars::schema_for!(ListenerHost),
            schemars::schema_for!(ListenerKey),
        ] {
            let v = serde_json::to_value(&schema).unwrap();
            assert_eq!(v["type"], "string", "{v}");
            assert!(v["description"].as_str().is_some_and(|d| !d.is_empty()));
        }
        let from = serde_json::to_value(schemars::schema_for!(MessageFrom)).unwrap();
        assert!(from["anyOf"].is_array(), "{from}");
    }

    // added by the coverage pass: b5-listen.rs
    #[test]
    fn a_message_from_one_agent_is_heard_by_its_id_and_a_fixed_project_resolves_as_itself() {
        let from_agent = MessageFilter {
            from: MessageFrom::Someone(ValueRef::Fixed(Assignee::Agent("dev".into()))),
            ..MessageFilter::default()
        };
        let theirs = |author: &str| {
            heard(
                SignalSource::Message,
                None,
                json!({"scope": "s", "author": author, "author_kind": "agent", "mentions": [], "text": "hi"}),
            )
        };
        assert!(from_agent.hears(&theirs("dev")));
        assert!(!from_agent.hears(&theirs("ops")));
        let fixed = ProjectFilter {
            project: Some(ValueRef::Fixed(ProjectId::from_ulid(
                ulid::Ulid::from_parts(3, 1),
            ))),
            ..ProjectFilter::default()
        };
        let render = |t: &str| -> Result<String, String> { Ok(t.to_string()) };
        let input =
            |n: &InputName| -> Result<ProjectId, String> { Err(format!("{n} is not read")) };
        assert_eq!(fixed.resolve(&render, &input).unwrap(), fixed);
    }
}
