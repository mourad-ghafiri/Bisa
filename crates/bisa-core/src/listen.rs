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
    Goal,
    Workflow,
    Agent,
    Test,
}

impl SignalSource {
    pub const NAMES: [&'static str; 13] = [
        "schedule",
        "hook",
        "message",
        "signal",
        "project",
        "run",
        "platform",
        "connector",
        "check",
        "goal",
        "workflow",
        "agent",
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
            SignalSource::Goal => "goal",
            SignalSource::Workflow => "workflow",
            SignalSource::Agent => "agent",
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
            "goal" => SignalSource::Goal,
            "workflow" => SignalSource::Workflow,
            "agent" => SignalSource::Agent,
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
// One or many
// ---------------------------------------------------------------------------

/// One value or a list of them, as a filter field takes it: `project = "…"`
/// or `project = ["…", "…"]`, `change = "commit"` or `change = ["commit",
/// "push"]`. A field that takes it hears **any** of the list, each entry
/// matched exactly as a single value is. A single value reads as `One` and
/// writes as the bare value, so a definition written before lists existed
/// reads and re-serializes as it was; a list writes as a list, as written.
/// An empty list is refused by name on a required field — a filter that
/// hears nothing is a mistake its author should see — and reads as absent
/// on an optional one ([`none_when_empty`]), which is how a designer clears
/// it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, schemars::JsonSchema)]
#[serde(untagged)]
#[schemars(rename = "{T}OrMany")]
pub enum OneOrMany<T> {
    One(T),
    Many(Vec<T>),
}

impl<T> OneOrMany<T> {
    /// Every entry, a single value being one.
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        match self {
            OneOrMany::One(one) => std::slice::from_ref(one).iter(),
            OneOrMany::Many(many) => many.iter(),
        }
    }

    pub fn len(&self) -> usize {
        self.iter().len()
    }

    /// Only a list built empty in code: the reader refuses one.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn first(&self) -> Option<&T> {
        self.iter().next()
    }

    /// Whether any entry is `want`.
    pub fn has(&self, want: &T) -> bool
    where
        T: PartialEq,
    {
        self.iter().any(|t| t == want)
    }

    /// The same shape with every entry mapped, the first failure answered.
    pub fn try_map<U, E>(&self, mut f: impl FnMut(&T) -> Result<U, E>) -> Result<OneOrMany<U>, E> {
        Ok(match self {
            OneOrMany::One(one) => OneOrMany::One(f(one)?),
            OneOrMany::Many(many) => OneOrMany::Many(many.iter().map(f).collect::<Result<_, E>>()?),
        })
    }

    /// The entries in words, joined — what a summary says of a list.
    pub fn words(&self, word: impl Fn(&T) -> String) -> String {
        self.iter().map(word).collect::<Vec<_>>().join(" · ")
    }
}

impl<T: Default> Default for OneOrMany<T> {
    fn default() -> Self {
        OneOrMany::One(T::default())
    }
}

impl<T> From<T> for OneOrMany<T> {
    fn from(one: T) -> Self {
        OneOrMany::One(one)
    }
}

impl From<&str> for OneOrMany<String> {
    fn from(one: &str) -> Self {
        OneOrMany::One(one.to_string())
    }
}

impl<T> From<Vec<T>> for OneOrMany<T> {
    fn from(many: Vec<T>) -> Self {
        OneOrMany::Many(many)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for OneOrMany<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match serde_json::Value::deserialize(d)? {
            serde_json::Value::Array(items) => {
                if items.is_empty() {
                    return Err(D::Error::custom("a list needs at least one entry"));
                }
                items
                    .into_iter()
                    .map(|item| T::deserialize(item).map_err(D::Error::custom))
                    .collect::<Result<Vec<_>, _>>()
                    .map(OneOrMany::Many)
            }
            one => T::deserialize(one)
                .map(OneOrMany::One)
                .map_err(D::Error::custom),
        }
    }
}

/// An optional list field: absent, `null` and `[]` all read as nothing —
/// the designer clears a list by emptying it — while a value or a list with
/// entries reads as [`OneOrMany`] does.
pub fn none_when_empty<'de, D, T>(d: D) -> Result<Option<OneOrMany<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    match serde_json::Value::deserialize(d)? {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::Array(items) if items.is_empty() => Ok(None),
        raw => OneOrMany::<T>::deserialize(raw)
            .map(Some)
            .map_err(D::Error::custom),
    }
}

/// `any` of a list of filter words against one payload word.
fn word_in<T>(list: &OneOrMany<T>, found: Option<&str>, word: impl Fn(&T) -> &str) -> bool {
    found.is_some_and(|f| list.iter().any(|t| word(t) == f))
}

/// `any` of an optional list against one payload word: absent hears every.
fn word_in_opt<T>(
    list: Option<&OneOrMany<T>>,
    found: Option<&str>,
    word: impl Fn(&T) -> String,
) -> bool {
    match list {
        None => true,
        Some(list) => found.is_some_and(|f| list.iter().any(|t| word(t) == f)),
    }
}

// ---------------------------------------------------------------------------
// The filters
// ---------------------------------------------------------------------------

/// Who wrote a message a message event waits for.
///
/// `you` is this node's person; `agents` any agent; `people` any person —
/// you, or one hosted from another node; `anyone` anybody at all; an
/// assignee names one agent, one person — a person hosted from another node
/// only this way — or a team, whose members' messages all count; `{ input =
/// … }` reads an input of kind `assignee`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum MessageFrom {
    #[default]
    You,
    Agents,
    People,
    Anyone,
    Someone(ValueRef<Assignee>),
}

impl MessageFrom {
    /// The words a sender is written as, beside an assignee.
    pub const WORDS: [&'static str; 4] = ["you", "agents", "people", "anyone"];

    pub fn as_str(&self) -> &'static str {
        match self {
            MessageFrom::You => "you",
            MessageFrom::Agents => "agents",
            MessageFrom::People => "people",
            MessageFrom::Anyone => "anyone",
            MessageFrom::Someone(_) => "someone",
        }
    }

    fn from_word(word: &str) -> Option<Self> {
        Some(match word {
            "you" => MessageFrom::You,
            "agents" => MessageFrom::Agents,
            "people" => MessageFrom::People,
            "anyone" => MessageFrom::Anyone,
            _ => return None,
        })
    }

    /// Whether a message by `author` (`author_kind` as the ear says it: `you`
    /// · `agent` · `person`, in `teams`) is from this sender.
    fn hears(
        &self,
        kind: Option<&str>,
        author: Option<&str>,
        teams: Option<&[serde_json::Value]>,
    ) -> bool {
        match self {
            MessageFrom::You => kind == Some("you"),
            MessageFrom::Agents => kind == Some("agent"),
            MessageFrom::People => matches!(kind, Some("you") | Some("person")),
            MessageFrom::Anyone => true,
            MessageFrom::Someone(ValueRef::Fixed(who)) => match who {
                Assignee::Agent(id) => kind == Some("agent") && author == Some(id.as_str()),
                Assignee::Human(pk) => author == Some(pk.as_hex()),
                Assignee::Team(team) => {
                    teams.is_some_and(|ts| ts.iter().any(|t| t.as_str() == Some(team.as_str())))
                }
            },
            MessageFrom::Someone(ValueRef::Input { .. }) => false,
        }
    }
}

impl Serialize for MessageFrom {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            MessageFrom::Someone(who) => who.serialize(s),
            word => s.serialize_str(word.as_str()),
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
            "description": "who wrote a message: `you`, `agents`, `people`, `anyone`, or one agent, person or team — or an input of kind `assignee` naming one",
            "anyOf": [
                { "type": "string", "enum": MessageFrom::WORDS },
                someone
            ]
        })
    }
}

impl<'de> Deserialize<'de> for MessageFrom {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = serde_json::Value::deserialize(d)?;
        if let Some(word) = raw.as_str().and_then(MessageFrom::from_word) {
            return Ok(word);
        }
        serde_json::from_value::<ValueRef<Assignee>>(raw)
            .map(MessageFrom::Someone)
            .map_err(|_| {
                D::Error::custom(
                    "`from` is `you`, `agents`, `people`, `anyone`, an assignee ({ agent = … }, \
                     { human = … }, { team = … }) or { input = … }",
                )
            })
    }
}

/// The kind of place a message lands in: a standing channel, a direct
/// channel, a goal's thread, a conversation in a checkout (`workstream`),
/// any other conversation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MessagePlace {
    Channel,
    Direct,
    Goal,
    Workstream,
    Conversation,
}

impl MessagePlace {
    pub const NAMES: [&'static str; 5] =
        ["channel", "direct", "goal", "workstream", "conversation"];

    pub fn as_str(self) -> &'static str {
        match self {
            MessagePlace::Channel => "channel",
            MessagePlace::Direct => "direct",
            MessagePlace::Goal => "goal",
            MessagePlace::Workstream => "workstream",
            MessagePlace::Conversation => "conversation",
        }
    }
}

/// A message posted into a conversation: where, by whom, mentioning whom,
/// saying what. Heard where conversation dispatch hears a message — after the
/// hold a message from another node waits in — and never an announcement.
/// Every field but `contains` takes one value or a list, heard as any of
/// them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MessageFilter {
    /// The conversation it lands in — a channel id, a direct channel's, a
    /// conversation's or a goal's thread (a template); absent, anywhere.
    #[serde(
        default,
        deserialize_with = "none_when_empty",
        skip_serializing_if = "Option::is_none"
    )]
    pub r#in: Option<OneOrMany<String>>,
    #[serde(default)]
    pub from: OneOrMany<MessageFrom>,
    /// It mentions this agent, team or person.
    #[serde(
        default,
        deserialize_with = "none_when_empty",
        skip_serializing_if = "Option::is_none"
    )]
    pub mentions: Option<OneOrMany<ValueRef<Assignee>>>,
    /// Its text contains this, case-insensitively (a template).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contains: Option<String>,
    /// The kind of place it lands in — a direct channel, a standing one, a
    /// goal's thread…; absent, any kind.
    #[serde(
        default,
        deserialize_with = "none_when_empty",
        skip_serializing_if = "Option::is_none"
    )]
    pub kind: Option<OneOrMany<MessagePlace>>,
}

impl MessageFilter {
    pub const FIELDS: &'static [&'static str] = &["in", "from", "mentions", "contains", "kind"];

    /// The strings this filter renders.
    pub fn templates(&self) -> Vec<&str> {
        self.r#in
            .iter()
            .flat_map(OneOrMany::iter)
            .chain(self.contains.iter())
            .map(String::as_str)
            .collect()
    }

    /// The assignees this filter names: who wrote it, whom it mentions.
    pub fn assignee_refs(&self) -> Vec<&ValueRef<Assignee>> {
        self.from
            .iter()
            .filter_map(|from| match from {
                MessageFrom::Someone(who) => Some(who),
                _ => None,
            })
            .chain(self.mentions.iter().flat_map(OneOrMany::iter))
            .collect()
    }

    /// Every input this filter reads through a reference.
    pub fn input_refs(&self) -> Vec<(&InputName, &'static str)> {
        self.assignee_refs()
            .into_iter()
            .filter_map(|r| match r {
                ValueRef::Input { input } => Some((input, "assignee")),
                ValueRef::Fixed(_) => None,
            })
            .collect()
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
            r#in: self
                .r#in
                .as_ref()
                .map(|places| places.try_map(|place| render(place)))
                .transpose()?,
            from: self.from.try_map(|from| {
                Ok(match from {
                    MessageFrom::Someone(who) => MessageFrom::Someone(fixed(who)?),
                    other => other.clone(),
                })
            })?,
            mentions: self
                .mentions
                .as_ref()
                .map(|who| who.try_map(fixed))
                .transpose()?,
            contains: self.contains.as_deref().map(render).transpose()?,
            kind: self.kind.clone(),
        })
    }

    /// Whether a heard message is the one this (resolved) filter waits for.
    /// The payload the engine builds: `{ scope, scope_kind, author,
    /// author_kind: you | agent | person, teams: [..], mentions: [..], text }`.
    pub fn hears(&self, heard: &Heard) -> bool {
        if heard.source != SignalSource::Message {
            return false;
        }
        let p = &heard.payload;
        let get = |k: &str| p.get(k);
        let said = |k: &str| get(k).and_then(serde_json::Value::as_str);
        if !word_in_opt(self.r#in.as_ref(), said("scope"), String::clone) {
            return false;
        }
        if !word_in_opt(self.kind.as_ref(), said("scope_kind"), |k| {
            k.as_str().to_string()
        }) {
            return false;
        }
        let kind = said("author_kind");
        let author = said("author");
        let teams = get("teams").and_then(serde_json::Value::as_array);
        if !self
            .from
            .iter()
            .any(|from| from.hears(kind, author, teams.map(Vec::as_slice)))
        {
            return false;
        }
        if let Some(mentions) = &self.mentions {
            let named = get("mentions").and_then(serde_json::Value::as_array);
            let any = mentions.iter().any(|who| match who {
                ValueRef::Fixed(who) => {
                    let want = match who {
                        Assignee::Agent(id) | Assignee::Team(id) => id.clone(),
                        Assignee::Human(pk) => pk.as_hex().to_string(),
                    };
                    named.is_some_and(|ms| ms.iter().any(|m| m.as_str() == Some(want.as_str())))
                }
                ValueRef::Input { .. } => false,
            });
            if !any {
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
    /// A branch appeared or went.
    Branch,
    /// A local branch caught up with its remote-tracking branch: pulled —
    /// fetched and merged, or fast-forwarded.
    Pull,
    /// A checkout of the project moved: opened, committed, pushed, its pull
    /// request opened or merged, closed.
    Workstream,
}

impl ProjectChange {
    pub const NAMES: [&'static str; 8] = [
        "commit",
        "push",
        "pull_request",
        "merge",
        "files",
        "branch",
        "pull",
        "workstream",
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ProjectChange::Commit => "commit",
            ProjectChange::Push => "push",
            ProjectChange::PullRequest => "pull_request",
            ProjectChange::Merge => "merge",
            ProjectChange::Files => "files",
            ProjectChange::Branch => "branch",
            ProjectChange::Pull => "pull",
            ProjectChange::Workstream => "workstream",
        }
    }
}

/// A change in one project, or in any of a list — never a path on a disk,
/// which would mean nothing on another machine.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectFilter {
    /// Absent while the designer has not chosen one: a problem, never `""`.
    #[serde(
        default,
        deserialize_with = "none_when_empty",
        skip_serializing_if = "Option::is_none"
    )]
    pub project: Option<OneOrMany<ValueRef<ProjectId>>>,
    #[serde(default)]
    pub change: OneOrMany<ProjectChange>,
    /// Only this branch, or any of these (templates).
    #[serde(
        default,
        deserialize_with = "none_when_empty",
        skip_serializing_if = "Option::is_none"
    )]
    pub branch: Option<OneOrMany<String>>,
    /// Only files matching this (`*` and `?`), or any of these, for `files`
    /// (templates).
    #[serde(
        default,
        deserialize_with = "none_when_empty",
        skip_serializing_if = "Option::is_none"
    )]
    pub glob: Option<OneOrMany<String>>,
}

impl ProjectFilter {
    pub const FIELDS: &'static [&'static str] = &["project", "change", "branch", "glob"];

    pub fn templates(&self) -> Vec<&str> {
        self.branch
            .iter()
            .chain(self.glob.iter())
            .flat_map(OneOrMany::iter)
            .map(String::as_str)
            .collect()
    }

    pub fn input_refs(&self) -> Vec<(&InputName, &'static str)> {
        self.project
            .iter()
            .flat_map(OneOrMany::iter)
            .filter_map(|p| match p {
                ValueRef::Input { input } => Some((input, "project")),
                ValueRef::Fixed(_) => None,
            })
            .collect()
    }

    pub fn resolve<E>(
        &self,
        render: &dyn Fn(&str) -> Result<String, E>,
        input: &dyn Fn(&InputName) -> Result<ProjectId, E>,
    ) -> Result<Self, E> {
        Ok(ProjectFilter {
            project: self
                .project
                .as_ref()
                .map(|projects| {
                    projects.try_map(|p| {
                        Ok(match p {
                            ValueRef::Input { input: name } => ValueRef::Fixed(input(name)?),
                            other => other.clone(),
                        })
                    })
                })
                .transpose()?,
            change: self.change.clone(),
            branch: self
                .branch
                .as_ref()
                .map(|b| b.try_map(|t| render(t)))
                .transpose()?,
            glob: self
                .glob
                .as_ref()
                .map(|g| g.try_map(|t| render(t)))
                .transpose()?,
        })
    }

    /// The projects this (resolved) filter names — the fixed ones; an
    /// unresolved reference names none.
    pub fn project_ids(&self) -> Vec<ProjectId> {
        self.project
            .iter()
            .flat_map(OneOrMany::iter)
            .filter_map(|p| match p {
                ValueRef::Fixed(p) => Some(*p),
                ValueRef::Input { .. } => None,
            })
            .collect()
    }

    /// The changes it hears.
    pub fn changes(&self) -> impl Iterator<Item = ProjectChange> + '_ {
        self.change.iter().copied()
    }

    /// The payload the engine builds: `{ project, change, branch?, paths? }`.
    pub fn hears(&self, heard: &Heard) -> bool {
        if heard.source != SignalSource::Project {
            return false;
        }
        let p = &heard.payload;
        let said = |k: &str| p.get(k).and_then(serde_json::Value::as_str);
        let projects = self.project_ids();
        if projects.is_empty() {
            return false;
        }
        if !said("project").is_some_and(|found| projects.iter().any(|p| p.to_string() == found)) {
            return false;
        }
        if !word_in(&self.change, said("change"), |c| c.as_str()) {
            return false;
        }
        if !word_in_opt(self.branch.as_ref(), said("branch"), String::clone) {
            return false;
        }
        match &self.glob {
            Some(globs) => p
                .get("paths")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|ps| {
                    ps.iter()
                        .filter_map(serde_json::Value::as_str)
                        .any(|path| globs.iter().any(|glob| glob_match(glob, path)))
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

/// What a run did, as a run event hears it: it was made behind a live one,
/// it started, it ended — the one a `run` event meant before the others
/// were named, so a filter that says nothing hears ends alone — or one of
/// its steps moved.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RunChange {
    Queued,
    Started,
    #[default]
    Ended,
    Step,
}

impl RunChange {
    pub const NAMES: [&'static str; 4] = ["queued", "started", "ended", "step"];

    pub fn as_str(self) -> &'static str {
        match self {
            RunChange::Queued => "queued",
            RunChange::Started => "started",
            RunChange::Ended => "ended",
            RunChange::Step => "step",
        }
    }
}

/// A run that moved — of one workflow or of any, on one goal or any, ending
/// how, which of its steps into which state. Every field takes one value or
/// a list; `change` says what of the run is heard and means `ended` when
/// nothing is said.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunFilter {
    #[serde(
        default,
        deserialize_with = "none_when_empty",
        skip_serializing_if = "Option::is_none"
    )]
    pub workflow: Option<OneOrMany<WorkflowId>>,
    #[serde(default, skip_serializing_if = "RunFilter::ended_alone")]
    pub change: OneOrMany<RunChange>,
    /// How it ended, for `ended`.
    #[serde(
        default,
        deserialize_with = "none_when_empty",
        skip_serializing_if = "Option::is_none"
    )]
    pub outcome: Option<OneOrMany<RunEnd>>,
    /// Which step, for `step`.
    #[serde(
        default,
        deserialize_with = "none_when_empty",
        skip_serializing_if = "Option::is_none"
    )]
    pub step: Option<OneOrMany<StepId>>,
    /// Which state the step moved into, for `step` — the words of
    /// `step.changed`.
    #[serde(
        default,
        deserialize_with = "none_when_empty",
        skip_serializing_if = "Option::is_none"
    )]
    pub state: Option<OneOrMany<String>>,
    /// Only a run of this goal, or of any of these (templates: an id, or an
    /// input).
    #[serde(
        default,
        deserialize_with = "none_when_empty",
        skip_serializing_if = "Option::is_none"
    )]
    pub goal: Option<OneOrMany<String>>,
}

impl RunFilter {
    pub const FIELDS: &'static [&'static str] =
        &["workflow", "change", "outcome", "step", "state", "goal"];

    /// What a `run` event said before `change` was written: ends alone —
    /// left unwritten, so a definition from before reads back as it was.
    fn ended_alone(change: &OneOrMany<RunChange>) -> bool {
        *change == OneOrMany::One(RunChange::Ended)
    }

    /// Whether this filter hears a step's move: what `step` and `state`
    /// narrow.
    pub fn hears_steps(&self) -> bool {
        self.change.has(&RunChange::Step)
    }

    pub fn templates(&self) -> Vec<&str> {
        self.goal
            .iter()
            .flat_map(OneOrMany::iter)
            .map(String::as_str)
            .collect()
    }

    pub fn resolve<E>(&self, render: &dyn Fn(&str) -> Result<String, E>) -> Result<Self, E> {
        Ok(RunFilter {
            goal: self
                .goal
                .as_ref()
                .map(|g| g.try_map(|t| render(t)))
                .transpose()?,
            ..self.clone()
        })
    }

    /// The payload the engine builds: `{ run, workflow, change, outcome,
    /// position, start, step, state, kind, goal }` — a payload with no
    /// `change` (written before it was) is an end.
    pub fn hears(&self, heard: &Heard) -> bool {
        if heard.source != SignalSource::Run {
            return false;
        }
        let p = &heard.payload;
        let said = |k: &str| p.get(k).and_then(serde_json::Value::as_str);
        let change = said("change").unwrap_or("ended");
        if !self.change.iter().any(|c| c.as_str() == change) {
            return false;
        }
        // `outcome` narrows an end and `step`/`state` a step's move: a filter
        // hearing several changes reads each narrowing on the occurrences it
        // is about, never against one that carries no such field.
        let outcome = change != "ended"
            || word_in_opt(self.outcome.as_ref(), said("outcome"), |o| {
                o.as_str().to_string()
            });
        let step = change != "step"
            || (word_in_opt(self.step.as_ref(), said("step"), |s| s.to_string())
                && word_in_opt(self.state.as_ref(), said("state"), String::clone));
        outcome
            && step
            && word_in_opt(self.workflow.as_ref(), said("workflow"), |w| w.to_string())
            && word_in_opt(self.goal.as_ref(), said("goal"), String::clone)
    }
}

/// One of the engine's own bus events, by topic (`goal.closed`,
/// `step.changed`) — or any of a list — and exact fields: the advanced
/// door, for what the other events do not name.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlatformFilter {
    pub topic: OneOrMany<String>,
    /// Exact matches on the event's own fields (templates).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, String>,
}

impl PlatformFilter {
    pub const FIELDS: &'static [&'static str] = &["topic", "fields"];

    pub fn templates(&self) -> Vec<&str> {
        self.fields.values().map(String::as_str).collect()
    }

    /// The topics it names.
    pub fn topics(&self) -> impl Iterator<Item = &str> {
        self.topic.iter().map(String::as_str)
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
            && word_in(&self.topic, heard.name.as_deref(), String::as_str)
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

/// What moved about a goal, as a goal event hears it: its record, its
/// projects, its design, its decisions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GoalChange {
    /// Captured by a person, spawned by a step or an agent.
    Created,
    /// Its statement, title, assignees, tags or budget changed.
    Updated,
    /// Abandoned, or superseded by another.
    Closed,
    /// Put away.
    Archived,
    /// Taken back out.
    Restored,
    Deleted,
    /// A project was attached to it.
    Attached,
    /// A project was detached from it.
    Detached,
    /// The Workflow Agent proposed a design for it.
    Proposed,
    /// A gate opened on it: a question, an approval, an adoption.
    Asked,
    /// A gate on it was decided.
    Decided,
}

impl GoalChange {
    pub const NAMES: [&'static str; 11] = [
        "created", "updated", "closed", "archived", "restored", "deleted", "attached", "detached",
        "proposed", "asked", "decided",
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            GoalChange::Created => "created",
            GoalChange::Updated => "updated",
            GoalChange::Closed => "closed",
            GoalChange::Archived => "archived",
            GoalChange::Restored => "restored",
            GoalChange::Deleted => "deleted",
            GoalChange::Attached => "attached",
            GoalChange::Detached => "detached",
            GoalChange::Proposed => "proposed",
            GoalChange::Asked => "asked",
            GoalChange::Decided => "decided",
        }
    }
}

/// A goal that moved — one named by id or by an input (templates), or any
/// goal of the workspace — in one of the ways `change` names. A goal's
/// lifecycle is the workspace's news about it: every host hears it, narrowed
/// here, so a goal may depend on another.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GoalFilter {
    /// The goal, or any of these (templates: an id, or `{inputs.<name>}`);
    /// absent, any goal.
    #[serde(
        default,
        deserialize_with = "none_when_empty",
        skip_serializing_if = "Option::is_none"
    )]
    pub goal: Option<OneOrMany<String>>,
    pub change: OneOrMany<GoalChange>,
}

impl GoalFilter {
    pub const FIELDS: &'static [&'static str] = &["goal", "change"];

    pub fn templates(&self) -> Vec<&str> {
        self.goal
            .iter()
            .flat_map(OneOrMany::iter)
            .map(String::as_str)
            .collect()
    }

    pub fn resolve<E>(&self, render: &dyn Fn(&str) -> Result<String, E>) -> Result<Self, E> {
        Ok(GoalFilter {
            goal: self
                .goal
                .as_ref()
                .map(|g| g.try_map(|t| render(t)))
                .transpose()?,
            change: self.change.clone(),
        })
    }

    /// The payload the engine builds: `{ goal, change, title, status, … }`.
    pub fn hears(&self, heard: &Heard) -> bool {
        if heard.source != SignalSource::Goal {
            return false;
        }
        let said = |k: &str| heard.payload.get(k).and_then(serde_json::Value::as_str);
        word_in(&self.change, said("change"), |c| c.as_str())
            && word_in_opt(self.goal.as_ref(), said("goal"), String::clone)
    }
}

/// What moved about a workflow definition in the library.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowChange {
    /// Created or saved at a revision — by a person, or by the Workflow
    /// Agent's hand (`designed`).
    Saved,
    /// Proposed for a goal by the Workflow Agent.
    Proposed,
    /// Put away.
    Archived,
    /// Taken back out.
    Restored,
    Deleted,
}

impl WorkflowChange {
    pub const NAMES: [&'static str; 5] = ["saved", "proposed", "archived", "restored", "deleted"];

    pub fn as_str(self) -> &'static str {
        match self {
            WorkflowChange::Saved => "saved",
            WorkflowChange::Proposed => "proposed",
            WorkflowChange::Archived => "archived",
            WorkflowChange::Restored => "restored",
            WorkflowChange::Deleted => "deleted",
        }
    }
}

/// A workflow definition that moved — one, any of a list, or any of the
/// library — in one of the ways `change` names. Named by id alone, never by
/// whether it is still there: a start on `deleted` must hear the deletion.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkflowFilter {
    #[serde(
        default,
        deserialize_with = "none_when_empty",
        skip_serializing_if = "Option::is_none"
    )]
    pub workflow: Option<OneOrMany<WorkflowId>>,
    pub change: OneOrMany<WorkflowChange>,
}

impl WorkflowFilter {
    pub const FIELDS: &'static [&'static str] = &["workflow", "change"];

    /// The payload the engine builds: `{ workflow, change, name, revision?, designed?, goal? }`.
    pub fn hears(&self, heard: &Heard) -> bool {
        if heard.source != SignalSource::Workflow {
            return false;
        }
        let said = |k: &str| heard.payload.get(k).and_then(serde_json::Value::as_str);
        word_in(&self.change, said("change"), |c| c.as_str())
            && word_in_opt(self.workflow.as_ref(), said("workflow"), |w| w.to_string())
    }
}

/// What a harness session did, as an agent event hears it. Churn between
/// turns — thinking, a tool running, idle — is never an occurrence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentChange {
    /// Launched.
    Started,
    /// Stopped until a person acts.
    Waiting,
    /// Replied in a conversation.
    Replied,
    Done,
    Failed,
    Aborted,
}

impl AgentChange {
    pub const NAMES: [&'static str; 6] =
        ["started", "waiting", "replied", "done", "failed", "aborted"];

    pub fn as_str(self) -> &'static str {
        match self {
            AgentChange::Started => "started",
            AgentChange::Waiting => "waiting",
            AgentChange::Replied => "replied",
            AgentChange::Done => "done",
            AgentChange::Failed => "failed",
            AgentChange::Aborted => "aborted",
        }
    }
}

/// A harness session that moved — of one agent or any of a list (a team
/// counts its members), in one project or any of a list, on one harness or
/// any of a list — in one of the ways `change` names. A session on a goal is
/// the goal's; one with no goal is the workspace's.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentFilter {
    #[serde(
        default,
        deserialize_with = "none_when_empty",
        skip_serializing_if = "Option::is_none"
    )]
    pub agent: Option<OneOrMany<ValueRef<Assignee>>>,
    #[serde(
        default,
        deserialize_with = "none_when_empty",
        skip_serializing_if = "Option::is_none"
    )]
    pub project: Option<OneOrMany<ValueRef<ProjectId>>>,
    /// The harness id it runs on (`claude-code`, `codex`, …).
    #[serde(
        default,
        deserialize_with = "none_when_empty",
        skip_serializing_if = "Option::is_none"
    )]
    pub harness: Option<OneOrMany<String>>,
    pub change: OneOrMany<AgentChange>,
}

impl AgentFilter {
    pub const FIELDS: &'static [&'static str] = &["agent", "project", "harness", "change"];

    pub fn assignee_refs(&self) -> Vec<&ValueRef<Assignee>> {
        self.agent.iter().flat_map(OneOrMany::iter).collect()
    }

    pub fn input_refs(&self) -> Vec<(&InputName, &'static str)> {
        let mut out: Vec<(&InputName, &'static str)> = self
            .assignee_refs()
            .into_iter()
            .filter_map(|r| match r {
                ValueRef::Input { input } => Some((input, "assignee")),
                ValueRef::Fixed(_) => None,
            })
            .collect();
        out.extend(
            self.project
                .iter()
                .flat_map(OneOrMany::iter)
                .filter_map(|p| match p {
                    ValueRef::Input { input } => Some((input, "project")),
                    ValueRef::Fixed(_) => None,
                }),
        );
        out
    }

    pub fn resolve<E>(
        &self,
        assignee: &dyn Fn(&InputName) -> Result<Assignee, E>,
        project: &dyn Fn(&InputName) -> Result<ProjectId, E>,
    ) -> Result<Self, E> {
        Ok(AgentFilter {
            agent: self
                .agent
                .as_ref()
                .map(|who| {
                    who.try_map(|r| {
                        Ok(ValueRef::Fixed(match r {
                            ValueRef::Fixed(a) => a.clone(),
                            ValueRef::Input { input } => assignee(input)?,
                        }))
                    })
                })
                .transpose()?,
            project: self
                .project
                .as_ref()
                .map(|ps| {
                    ps.try_map(|r| {
                        Ok(ValueRef::Fixed(match r {
                            ValueRef::Fixed(p) => *p,
                            ValueRef::Input { input } => project(input)?,
                        }))
                    })
                })
                .transpose()?,
            harness: self.harness.clone(),
            change: self.change.clone(),
        })
    }

    /// The payload the engine builds: `{ agent?, teams, harness, kind,
    /// change, state, live_run, project?, workstream?, goal?, run?, … }`.
    pub fn hears(&self, heard: &Heard) -> bool {
        if heard.source != SignalSource::Agent {
            return false;
        }
        let p = &heard.payload;
        let said = |k: &str| p.get(k).and_then(serde_json::Value::as_str);
        if !word_in(&self.change, said("change"), |c| c.as_str()) {
            return false;
        }
        if let Some(who) = &self.agent {
            let agent = said("agent");
            let teams = p.get("teams").and_then(serde_json::Value::as_array);
            let any = who.iter().any(|r| match r {
                ValueRef::Fixed(Assignee::Agent(id)) => agent == Some(id.as_str()),
                ValueRef::Fixed(Assignee::Team(team)) => {
                    teams.is_some_and(|ts| ts.iter().any(|t| t.as_str() == Some(team.as_str())))
                }
                ValueRef::Fixed(Assignee::Human(_)) | ValueRef::Input { .. } => false,
            });
            if !any {
                return false;
            }
        }
        if let Some(projects) = &self.project {
            let project = said("project");
            let any = projects.iter().any(|r| match r {
                ValueRef::Fixed(p) => project == Some(p.to_string().as_str()),
                ValueRef::Input { .. } => false,
            });
            if !any {
                return false;
            }
        }
        word_in_opt(self.harness.as_ref(), said("harness"), String::clone)
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
            from: MessageFrom::Agents.into(),
            ..MessageFilter::default()
        };
        assert!(agents.hears(&msg("agent", "triager", "hi")));
        let named = MessageFilter {
            from: MessageFrom::Someone(ValueRef::Fixed(
                "human:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                    .parse()
                    .unwrap(),
            ))
            .into(),
            ..MessageFilter::default()
        };
        assert!(named.hears(&msg(
            "person",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "hi"
        )));
        let team = MessageFilter {
            from: MessageFrom::Someone(ValueRef::Fixed(Assignee::Team("ops".into()))).into(),
            ..MessageFilter::default()
        };
        assert!(team.hears(&msg("agent", "anyone", "hi")));
        let precise = MessageFilter {
            r#in: Some("support".into()),
            mentions: Some(ValueRef::Fixed(Assignee::Agent("triager".into())).into()),
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
            project: Some(ValueRef::Fixed(project).into()),
            change: ProjectChange::Commit.into(),
            branch: Some("main".into()),
            glob: None,
        };
        assert!(commits.hears(&change("commit", "main", json!([]))));
        assert!(!commits.hears(&change("commit", "dev", json!([]))));
        assert!(!commits.hears(&change("merge", "main", json!([]))));
        let csv = ProjectFilter {
            project: Some(ValueRef::Fixed(project).into()),
            change: ProjectChange::Files.into(),
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
            project: Some(ValueRef::Fixed(other).into()),
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
            outcome: Some(RunEnd::Failed.into()),
            ..RunFilter::default()
        };
        assert!(any_failure.hears(&ended(wf(1), "failed")));
        assert!(!any_failure.hears(&ended(wf(1), "done")));
        let of_one = RunFilter {
            workflow: Some(wf(2).into()),
            outcome: None,
            ..RunFilter::default()
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
            })
            .into(),
            mentions: None,
            contains: None,
            ..MessageFilter::default()
        };
        assert_eq!(filter.input_refs().len(), 1);
        let resolved = filter.resolve(&render, &who).unwrap();
        assert_eq!(
            resolved
                .r#in
                .as_ref()
                .and_then(OneOrMany::first)
                .map(String::as_str),
            Some("support")
        );
        assert_eq!(
            resolved.from,
            MessageFrom::Someone(ValueRef::Fixed(Assignee::Agent("sre".into()))).into()
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
            mentions: Some(
                ValueRef::Input {
                    input: input.clone(),
                }
                .into(),
            ),
            ..MessageFilter::default()
        };
        assert_eq!(filter.input_refs(), vec![(&input, "assignee")]);
        let msg = heard(
            SignalSource::Message,
            None,
            json!({"scope": "s", "author": "aa", "author_kind": "you", "mentions": [], "text": "hi"}),
        );
        let unresolved = MessageFilter {
            from: MessageFrom::Someone(ValueRef::Input { input }).into(),
            ..MessageFilter::default()
        };
        assert!(
            !unresolved.hears(&msg),
            "an unresolved sender hears nothing"
        );
        let hex = "bb".repeat(32);
        let person: Assignee = format!("human:{hex}").parse().unwrap();
        let mentions_a_person = MessageFilter {
            mentions: Some(ValueRef::Fixed(person).into()),
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
            project: Some(
                ValueRef::Input {
                    input: repo.clone(),
                }
                .into(),
            ),
            ..ProjectFilter::default()
        };
        assert_eq!(filter.input_refs(), vec![(&repo, "project")]);
        assert!(filter.project_ids().is_empty());
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
            from: MessageFrom::Someone(ValueRef::Fixed(person)).into(),
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
            from: MessageFrom::Someone(ValueRef::Fixed(Assignee::Agent("a".into()))).into(),
            mentions: Some(ValueRef::Fixed(Assignee::Agent("b".into())).into()),
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
            from: MessageFrom::Someone(ValueRef::Fixed(Assignee::Agent("dev".into()))).into(),
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
            project: Some(
                ValueRef::Fixed(ProjectId::from_ulid(ulid::Ulid::from_parts(3, 1))).into(),
            ),
            ..ProjectFilter::default()
        };
        let render = |t: &str| -> Result<String, String> { Ok(t.to_string()) };
        let input =
            |n: &InputName| -> Result<ProjectId, String> { Err(format!("{n} is not read")) };
        assert_eq!(fixed.resolve(&render, &input).unwrap(), fixed);
    }
    // added for the events programme: lists, the new words, the new filters

    #[test]
    fn one_or_many_reads_a_value_or_a_list_writes_it_as_written_and_refuses_an_empty_list() {
        let one: OneOrMany<String> = serde_json::from_value(json!("a")).unwrap();
        assert_eq!(one, OneOrMany::One("a".into()));
        assert_eq!(serde_json::to_value(&one).unwrap(), json!("a"));
        let many: OneOrMany<String> = serde_json::from_value(json!(["a", "b"])).unwrap();
        assert_eq!(many, OneOrMany::Many(vec!["a".into(), "b".into()]));
        assert_eq!(serde_json::to_value(&many).unwrap(), json!(["a", "b"]));
        let listed: OneOrMany<String> = serde_json::from_value(json!(["a"])).unwrap();
        assert_eq!(
            serde_json::to_value(&listed).unwrap(),
            json!(["a"]),
            "a one-entry list stays a list, as written"
        );
        let err = serde_json::from_value::<OneOrMany<String>>(json!([])).unwrap_err();
        assert!(err.to_string().contains("at least one"), "{err}");
        assert!(
            serde_json::from_value::<OneOrMany<ProjectChange>>(json!(["commit", "nope"])).is_err()
        );
        assert_eq!(many.len(), 2);
        assert_eq!(many.first().map(String::as_str), Some("a"));
        assert!(many.has(&"b".to_string()) && !many.has(&"c".to_string()));
        assert_eq!(many.words(String::clone), "a · b");
        assert_eq!(
            OneOrMany::<ProjectChange>::default(),
            OneOrMany::One(ProjectChange::Commit)
        );
        let mapped: Result<OneOrMany<usize>, String> = many.try_map(|s| Ok(s.len()));
        assert_eq!(mapped.unwrap(), OneOrMany::Many(vec![1, 1]));
        let failed: Result<OneOrMany<usize>, String> = many.try_map(|s| Err(s.clone()));
        assert_eq!(failed, Err("a".to_string()), "the first failure answers");
        assert!(OneOrMany::<u8>::Many(vec![]).is_empty());
        assert_eq!(OneOrMany::from(vec![1u8, 2]), OneOrMany::Many(vec![1, 2]));
    }

    #[test]
    fn an_optional_list_reads_null_and_an_empty_list_as_absent() {
        let filter: MessageFilter =
            serde_json::from_value(json!({"in": [], "mentions": null, "kind": []})).unwrap();
        assert_eq!(filter, MessageFilter::default());
        assert_eq!(
            serde_json::to_value(&filter).unwrap(),
            json!({"from": "you"})
        );
        let filter: ProjectFilter = serde_json::from_value(json!({"project": []})).unwrap();
        assert!(filter.project.is_none(), "cleared by the designer");
        let filter: RunFilter = serde_json::from_value(json!({})).unwrap();
        assert_eq!(filter.change, OneOrMany::One(RunChange::Ended));
        assert_eq!(
            serde_json::to_value(&filter).unwrap(),
            json!({}),
            "ends alone is left unwritten, as it always was"
        );
        let steps: RunFilter =
            serde_json::from_value(json!({"change": ["started", "step"], "step": "build"}))
                .unwrap();
        assert!(steps.hears_steps());
        assert_eq!(
            serde_json::to_value(&steps).unwrap(),
            json!({"change": ["started", "step"], "step": "build"})
        );
    }

    #[test]
    fn a_sender_reads_its_new_words_and_a_filter_hears_people_anyone_and_a_place_by_kind() {
        let parse = |v: serde_json::Value| serde_json::from_value::<MessageFrom>(v);
        assert_eq!(parse(json!("people")).unwrap(), MessageFrom::People);
        assert_eq!(parse(json!("anyone")).unwrap(), MessageFrom::Anyone);
        assert_eq!(MessageFrom::People.as_str(), "people");
        assert_eq!(MessageFrom::Anyone.as_str(), "anyone");
        for word in MessageFrom::WORDS {
            assert_eq!(parse(json!(word)).unwrap().as_str(), word);
        }
        let msg = |kind: &str, scope_kind: &str| {
            heard(
                SignalSource::Message,
                None,
                json!({
                    "scope": "s", "scope_kind": scope_kind, "author": "aa", "author_kind": kind,
                    "teams": [], "mentions": [], "text": "hi"
                }),
            )
        };
        let people = MessageFilter {
            from: MessageFrom::People.into(),
            ..MessageFilter::default()
        };
        assert!(people.hears(&msg("you", "channel")));
        assert!(people.hears(&msg("person", "channel")));
        assert!(
            !people.hears(&msg("agent", "channel")),
            "an agent is nobody's person"
        );
        let anyone = MessageFilter {
            from: MessageFrom::Anyone.into(),
            kind: Some(MessagePlace::Direct.into()),
            ..MessageFilter::default()
        };
        assert!(anyone.hears(&msg("agent", "direct")));
        assert!(
            !anyone.hears(&msg("agent", "channel")),
            "a standing channel is no direct one"
        );
        let several = MessageFilter {
            from: vec![MessageFrom::Agents, MessageFrom::You].into(),
            kind: Some(vec![MessagePlace::Goal, MessagePlace::Direct].into()),
            ..MessageFilter::default()
        };
        assert!(several.hears(&msg("agent", "goal")));
        assert!(several.hears(&msg("you", "direct")));
        assert!(
            !several.hears(&msg("person", "direct")),
            "a person is neither an agent nor you"
        );
        for place in MessagePlace::NAMES {
            let read: MessagePlace = serde_json::from_value(json!(place)).unwrap();
            assert_eq!(read.as_str(), place);
        }
    }

    #[test]
    fn a_message_filter_with_lists_hears_any_place_any_sender_and_any_mention() {
        let msg = |scope: &str, author: &str, mentions: serde_json::Value| {
            heard(
                SignalSource::Message,
                None,
                json!({
                    "scope": scope, "scope_kind": "channel", "author": author, "author_kind": "agent",
                    "teams": ["ops"], "mentions": mentions, "text": "refund please"
                }),
            )
        };
        let filter = MessageFilter {
            r#in: Some(vec!["support".to_string(), "ops".to_string()].into()),
            from: vec![
                MessageFrom::Someone(ValueRef::Fixed(Assignee::Agent("triager".into()))),
                MessageFrom::Someone(ValueRef::Fixed(Assignee::Team("ops".into()))),
            ]
            .into(),
            mentions: Some(
                vec![
                    ValueRef::Fixed(Assignee::Agent("a".into())),
                    ValueRef::Fixed(Assignee::Agent("b".into())),
                ]
                .into(),
            ),
            contains: Some("refund".into()),
            kind: None,
        };
        assert!(filter.hears(&msg("ops", "anyone-in-ops", json!(["b"]))));
        assert!(filter.hears(&msg("support", "triager", json!(["a", "x"]))));
        assert!(
            !filter.hears(&msg("general", "triager", json!(["a"]))),
            "elsewhere"
        );
        assert!(
            !filter.hears(&msg("support", "triager", json!(["x"]))),
            "mentions neither"
        );
        assert_eq!(filter.templates(), vec!["support", "ops", "refund"]);
        assert_eq!(filter.assignee_refs().len(), 4);
        let unresolved = MessageFilter {
            mentions: Some(
                ValueRef::Input {
                    input: InputName::new("who").unwrap(),
                }
                .into(),
            ),
            from: MessageFrom::Anyone.into(),
            ..MessageFilter::default()
        };
        assert!(
            !unresolved.hears(&msg("support", "triager", json!(["a"]))),
            "an unresolved mention hears nobody"
        );
        let render = |t: &str| -> Result<String, String> { Ok(t.to_uppercase()) };
        let input =
            |_: &InputName| -> Result<Assignee, String> { Ok(Assignee::Agent("sre".into())) };
        let resolved = unresolved.resolve(&render, &input).unwrap();
        assert_eq!(
            resolved.mentions,
            Some(ValueRef::Fixed(Assignee::Agent("sre".into())).into())
        );
        let places = MessageFilter {
            r#in: Some(vec!["{inputs.a}".to_string(), "b".to_string()].into()),
            ..MessageFilter::default()
        };
        let resolved = places.resolve(&render, &input).unwrap();
        assert_eq!(
            resolved.r#in,
            Some(vec!["{INPUTS.A}".to_string(), "B".to_string()].into()),
            "every place renders"
        );
    }

    #[test]
    fn a_project_filter_with_lists_hears_any_project_any_change_any_branch_and_any_glob() {
        let a = ProjectId::from_ulid(ulid::Ulid::from_parts(7, 1));
        let b = ProjectId::from_ulid(ulid::Ulid::from_parts(7, 2));
        let c = ProjectId::from_ulid(ulid::Ulid::from_parts(7, 3));
        let change = |p: ProjectId, c: &str, branch: &str, paths: serde_json::Value| {
            heard(
                SignalSource::Project,
                None,
                json!({"project": p.to_string(), "change": c, "branch": branch, "paths": paths}),
            )
        };
        let filter = ProjectFilter {
            project: Some(vec![ValueRef::Fixed(a), ValueRef::Fixed(b)].into()),
            change: vec![
                ProjectChange::Commit,
                ProjectChange::Branch,
                ProjectChange::Files,
            ]
            .into(),
            branch: Some(vec!["main".to_string(), "dev".to_string()].into()),
            glob: Some(vec!["*.csv".to_string(), "docs/*.md".to_string()].into()),
        };
        assert_eq!(filter.project_ids(), vec![a, b]);
        assert_eq!(filter.changes().count(), 3);
        assert!(filter.hears(&change(a, "commit", "main", json!(["x.csv"]))));
        assert!(filter.hears(&change(b, "branch", "dev", json!(["docs/a.md"]))));
        assert!(
            !filter.hears(&change(c, "commit", "main", json!(["x.csv"]))),
            "a third project"
        );
        assert!(
            !filter.hears(&change(a, "push", "main", json!(["x.csv"]))),
            "a change it lacks"
        );
        assert!(
            !filter.hears(&change(a, "commit", "feature", json!(["x.csv"]))),
            "a branch it lacks"
        );
        assert!(
            !filter.hears(&change(a, "files", "main", json!(["x.txt"]))),
            "no glob fits"
        );
        for (change, word) in [
            (ProjectChange::Branch, "branch"),
            (ProjectChange::Pull, "pull"),
            (ProjectChange::Workstream, "workstream"),
        ] {
            assert_eq!(change.as_str(), word);
            assert_eq!(serde_json::to_value(change).unwrap(), json!(word));
        }
        assert_eq!(ProjectChange::NAMES.len(), 8);
        let one_input = ProjectFilter {
            project: Some(
                vec![
                    ValueRef::Fixed(a),
                    ValueRef::Input {
                        input: InputName::new("repo").unwrap(),
                    },
                ]
                .into(),
            ),
            ..ProjectFilter::default()
        };
        assert_eq!(one_input.input_refs().len(), 1);
        assert_eq!(
            one_input.project_ids(),
            vec![a],
            "an unresolved entry names nothing"
        );
        let render = |t: &str| -> Result<String, String> { Ok(t.to_string()) };
        let input = |_: &InputName| -> Result<ProjectId, String> { Ok(b) };
        assert_eq!(
            one_input.resolve(&render, &input).unwrap().project_ids(),
            vec![a, b]
        );
    }

    #[test]
    fn a_run_filter_hears_what_the_run_did_and_an_occurrence_with_no_change_is_an_end() {
        let moved = |payload: serde_json::Value| heard(SignalSource::Run, None, payload);
        let w = wf(3);
        let g = goal(4);
        let ends = RunFilter::default();
        assert!(
            ends.hears(&moved(
                json!({"run": "r", "workflow": w.to_string(), "outcome": "done"})
            )),
            "written before `change` existed"
        );
        assert!(ends.hears(&moved(
            json!({"run": "r", "workflow": w.to_string(), "change": "ended", "outcome": "failed"})
        )));
        assert!(
            !ends.hears(&moved(
                json!({"run": "r", "workflow": w.to_string(), "change": "started"})
            )),
            "ends alone"
        );
        let steps = RunFilter {
            workflow: Some(vec![w, wf(5)].into()),
            change: vec![RunChange::Step, RunChange::Started].into(),
            step: Some(vec![StepId::new("build").unwrap()].into()),
            state: Some(vec!["failed".to_string(), "done".to_string()].into()),
            goal: Some(g.to_string().into()),
            outcome: None,
        };
        let step = |state: &str, step: &str, goal: GoalId| {
            moved(json!({
                "run": "r", "workflow": w.to_string(), "change": "step", "step": step,
                "state": state, "kind": "check", "goal": goal.to_string()
            }))
        };
        assert!(steps.hears(&step("failed", "build", g)));
        assert!(!steps.hears(&step("failed", "test", g)), "another step");
        assert!(
            !steps.hears(&step("running", "build", g)),
            "a state it lacks"
        );
        assert!(
            !steps.hears(&step("failed", "build", goal(9))),
            "another goal"
        );
        assert!(
            steps.hears(&moved(json!({"run": "r", "workflow": wf(5).to_string(), "change": "started", "goal": g.to_string()}))),
            "a start carries no step, and the step narrowing reads nothing of it"
        );
        let outcomes = RunFilter {
            outcome: Some(vec![RunEnd::Failed, RunEnd::Cancelled].into()),
            ..RunFilter::default()
        };
        assert!(outcomes.hears(&moved(
            json!({"run": "r", "workflow": "w", "outcome": "cancelled"})
        )));
        assert!(!outcomes.hears(&moved(
            json!({"run": "r", "workflow": "w", "outcome": "done"})
        )));
        assert_eq!(steps.templates(), vec![g.to_string().as_str()]);
        let render = |t: &str| -> Result<String, String> { Ok(format!("{t}!")) };
        assert_eq!(
            steps.resolve(&render).unwrap().goal,
            Some(format!("{g}!").into())
        );
        for (change, word) in [
            (RunChange::Queued, "queued"),
            (RunChange::Started, "started"),
            (RunChange::Ended, "ended"),
            (RunChange::Step, "step"),
        ] {
            assert_eq!(change.as_str(), word);
        }
        assert_eq!(RunChange::NAMES.len(), 4);
    }

    #[test]
    fn a_platform_filter_hears_any_topic_of_its_list() {
        let bus = |topic: &str| {
            heard(
                SignalSource::Platform,
                Some(topic),
                json!({"event": topic, "fields": {"state": "failed"}}),
            )
        };
        let filter = PlatformFilter {
            topic: vec!["step.changed".to_string(), "run.finished".to_string()].into(),
            fields: BTreeMap::from([("state".to_string(), "failed".to_string())]),
        };
        assert!(filter.hears(&bus("step.changed")));
        assert!(filter.hears(&bus("run.finished")));
        assert!(!filter.hears(&bus("goal.closed")));
        assert_eq!(
            filter.topics().collect::<Vec<_>>(),
            vec!["step.changed", "run.finished"]
        );
    }

    #[test]
    fn a_goal_filter_hears_a_goals_change_narrowed_to_the_goals_it_names() {
        let g = goal(1);
        let other = goal(2);
        let moved = |goal: GoalId, change: &str| {
            heard(
                SignalSource::Goal,
                None,
                json!({"goal": goal.to_string(), "change": change, "title": "Ship it", "status": "open"}),
            )
        };
        let any = GoalFilter {
            goal: None,
            change: vec![GoalChange::Closed, GoalChange::Decided].into(),
        };
        assert!(any.hears(&moved(g, "closed")));
        assert!(any.hears(&moved(other, "decided")));
        assert!(!any.hears(&moved(g, "updated")));
        let named = GoalFilter {
            goal: Some(vec![g.to_string(), "{inputs.goal}".to_string()].into()),
            change: GoalChange::Closed.into(),
        };
        assert!(named.hears(&moved(g, "closed")));
        assert!(!named.hears(&moved(other, "closed")), "another goal");
        assert!(
            !named.hears(&heard(
                SignalSource::Run,
                None,
                json!({"goal": g.to_string(), "change": "closed"})
            )),
            "goals alone"
        );
        assert_eq!(
            named.templates(),
            vec![g.to_string().as_str(), "{inputs.goal}"]
        );
        let render = |t: &str| -> Result<String, String> {
            Ok(t.replace("{inputs.goal}", &other.to_string()))
        };
        let resolved = named.resolve(&render).unwrap();
        assert!(
            resolved.hears(&moved(other, "closed")),
            "the input read names the other goal"
        );
        for word in GoalChange::NAMES {
            let read: GoalChange = serde_json::from_value(json!(word)).unwrap();
            assert_eq!(read.as_str(), word);
        }
        assert_eq!(GoalChange::NAMES.len(), 11);
        let json = serde_json::to_value(&any).unwrap();
        assert_eq!(json, json!({"change": ["closed", "decided"]}));
        assert_eq!(serde_json::from_value::<GoalFilter>(json).unwrap(), any);
        assert!(
            serde_json::from_value::<GoalFilter>(json!({"goal": "x"})).is_err(),
            "the change is the filter's one required word"
        );
    }

    #[test]
    fn a_workflow_filter_hears_a_definitions_change_narrowed_to_the_ones_it_names() {
        let moved = |workflow: WorkflowId, change: &str| {
            heard(
                SignalSource::Workflow,
                None,
                json!({"workflow": workflow.to_string(), "change": change, "name": "Weekly review"}),
            )
        };
        let any = WorkflowFilter {
            workflow: None,
            change: WorkflowChange::Saved.into(),
        };
        assert!(any.hears(&moved(wf(1), "saved")));
        assert!(!any.hears(&moved(wf(1), "deleted")));
        let named = WorkflowFilter {
            workflow: Some(vec![wf(1), wf(2)].into()),
            change: vec![WorkflowChange::Deleted, WorkflowChange::Archived].into(),
        };
        assert!(named.hears(&moved(wf(2), "deleted")));
        assert!(named.hears(&moved(wf(1), "archived")));
        assert!(!named.hears(&moved(wf(3), "deleted")));
        assert!(
            !named.hears(&heard(
                SignalSource::Goal,
                None,
                json!({"change": "deleted"})
            )),
            "workflows alone"
        );
        for word in WorkflowChange::NAMES {
            let read: WorkflowChange = serde_json::from_value(json!(word)).unwrap();
            assert_eq!(read.as_str(), word);
        }
        assert_eq!(WorkflowChange::NAMES.len(), 5);
    }

    #[test]
    fn an_agent_filter_hears_a_session_by_agent_team_project_harness_and_change() {
        let p = ProjectId::from_ulid(ulid::Ulid::from_parts(8, 1));
        let q = ProjectId::from_ulid(ulid::Ulid::from_parts(8, 2));
        let session = |agent: &str, project: Option<ProjectId>, harness: &str, change: &str| {
            let teams: Vec<&str> = if agent == "anyone-in-ops" {
                vec!["ops"]
            } else {
                vec![]
            };
            heard(
                SignalSource::Agent,
                None,
                json!({
                    "agent": agent, "teams": teams, "harness": harness, "kind": "worker",
                    "change": change, "state": change, "live_run": "lr",
                    "project": project.map(|p| p.to_string())
                }),
            )
        };
        let filter = AgentFilter {
            agent: Some(
                vec![
                    ValueRef::Fixed(Assignee::Agent("dev".into())),
                    ValueRef::Fixed(Assignee::Team("ops".into())),
                ]
                .into(),
            ),
            project: Some(vec![ValueRef::Fixed(p)].into()),
            harness: Some(vec!["claude-code".to_string(), "codex".to_string()].into()),
            change: vec![AgentChange::Done, AgentChange::Failed].into(),
        };
        assert!(filter.hears(&session("dev", Some(p), "codex", "done")));
        assert!(
            filter.hears(&session("anyone-in-ops", Some(p), "claude-code", "failed")),
            "a team counts its members"
        );
        assert!(
            !filter.hears(&session("dev", Some(q), "codex", "done")),
            "another project"
        );
        assert!(
            !filter.hears(&session("dev", None, "codex", "done")),
            "a session in a scratch folder names no project"
        );
        assert!(
            !filter.hears(&session("dev", Some(p), "gemini", "done")),
            "another harness"
        );
        assert!(
            !filter.hears(&session("dev", Some(p), "codex", "started")),
            "a change it lacks"
        );
        assert!(
            !filter.hears(&session("other", Some(p), "codex", "done")),
            "another agent, in no team of the list"
        );
        let any = AgentFilter {
            agent: None,
            project: None,
            harness: None,
            change: AgentChange::Replied.into(),
        };
        assert!(any.hears(&session("x", None, "pi", "replied")));
        assert!(
            !any.hears(&heard(
                SignalSource::Message,
                None,
                json!({"change": "replied"})
            )),
            "agents alone"
        );
        let human = AgentFilter {
            agent: Some(
                ValueRef::Fixed(
                    format!("human:{}", "ab".repeat(32))
                        .parse::<Assignee>()
                        .unwrap(),
                )
                .into(),
            ),
            ..any.clone()
        };
        assert!(
            !human.hears(&session("x", None, "pi", "replied")),
            "a person runs no session of an agent's"
        );
        let with_inputs = AgentFilter {
            agent: Some(
                ValueRef::Input {
                    input: InputName::new("who").unwrap(),
                }
                .into(),
            ),
            project: Some(
                ValueRef::Input {
                    input: InputName::new("repo").unwrap(),
                }
                .into(),
            ),
            harness: None,
            change: AgentChange::Done.into(),
        };
        assert_eq!(with_inputs.input_refs().len(), 2);
        assert_eq!(with_inputs.assignee_refs().len(), 1);
        assert!(
            !with_inputs.hears(&session("dev", Some(p), "codex", "done")),
            "unresolved, it hears nobody"
        );
        let who = |_: &InputName| -> Result<Assignee, String> { Ok(Assignee::Agent("dev".into())) };
        let which = |_: &InputName| -> Result<ProjectId, String> { Ok(p) };
        let resolved = with_inputs.resolve(&who, &which).unwrap();
        assert!(resolved.hears(&session("dev", Some(p), "codex", "done")));
        for word in AgentChange::NAMES {
            let read: AgentChange = serde_json::from_value(json!(word)).unwrap();
            assert_eq!(read.as_str(), word);
        }
        assert_eq!(AgentChange::NAMES.len(), 6);
    }

    #[test]
    fn an_agent_filter_resolves_a_fixed_project_as_itself_and_an_unresolved_one_hears_nobody() {
        let p = ProjectId::from_ulid(ulid::Ulid::from_parts(9, 1));
        let fixed = AgentFilter {
            agent: Some(
                ValueRef::Input {
                    input: InputName::new("who").unwrap(),
                }
                .into(),
            ),
            project: Some(ValueRef::Fixed(p).into()),
            harness: None,
            change: AgentChange::Started.into(),
        };
        let who = |_: &InputName| -> Result<Assignee, String> { Ok(Assignee::Agent("dev".into())) };
        let which =
            |n: &InputName| -> Result<ProjectId, String> { Err(format!("{n} is not read")) };
        let resolved = fixed.resolve(&who, &which).unwrap();
        assert_eq!(resolved.project, Some(ValueRef::Fixed(p).into()));
        assert_eq!(
            resolved.agent,
            Some(ValueRef::Fixed(Assignee::Agent("dev".into())).into())
        );
        let unresolved = AgentFilter {
            agent: None,
            project: Some(
                ValueRef::Input {
                    input: InputName::new("repo").unwrap(),
                }
                .into(),
            ),
            harness: None,
            change: AgentChange::Started.into(),
        };
        let session = heard(
            SignalSource::Agent,
            None,
            json!({"agent": "dev", "teams": [], "harness": "codex", "change": "started", "project": p.to_string()}),
        );
        assert!(
            !unresolved.hears(&session),
            "a project nobody read names nothing"
        );
        assert!(resolved.hears(&session));
    }

    #[test]
    fn the_new_sources_have_their_words() {
        assert_eq!(SignalSource::NAMES.len(), 13);
        for (source, word) in [
            (SignalSource::Goal, "goal"),
            (SignalSource::Workflow, "workflow"),
            (SignalSource::Agent, "agent"),
        ] {
            assert_eq!(source.as_str(), word);
            assert_eq!(SignalSource::parse(word), Some(source));
            assert_eq!(serde_json::to_value(source).unwrap(), json!(word));
        }
    }
}
