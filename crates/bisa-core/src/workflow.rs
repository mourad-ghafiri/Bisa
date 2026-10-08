//! `Workflow`: a named graph of steps with typed inputs — the shape a goal's
//! work takes, chosen per goal rather than forced by the platform.
//!
//! Three families of step, each kind one thing. **Events** are things that
//! happen: a `start` is one way a run may begin — by hand, on a schedule, when
//! called, when a message, a signal, a project's change, a run's end or a
//! platform topic arrives, when an outside platform lists something new, when
//! a check starts failing ([`crate::start`]); a `wait` holds the run for the
//! world; an `emit` raises a named signal; an `end` ends a path or the run.
//! **Gateways** route: `decide` (the first rule that holds, or every one),
//! `if`, `switch`, `judge`, `parallel`, and the loops `for_each` and `while`.
//! **Tasks** do work: an agent's, a person's answer or approval, a check, a
//! connector call, a post, a spawned goal. A step whose work can be stopped
//! may carry **boundary events** — a timeout that diverts it, a reminder posted
//! beside it, a message or a signal that ends it ([`crate::boundary`]).
//! Sequence is a flow; parallelism is several flows out of one step; a join is
//! how several flows into one step meet; a loop is a flow back to an earlier
//! step, bounded by `max_visits`.
//!
//! Everything here is a definition. Execution state lives in one place, the
//! [`crate::run::WorkflowRun`], which holds a frozen copy of the definition it
//! runs. [`Workflow::validate`] is pure and exhaustive: it reports every
//! problem, never the first, so the designer can show them all and *Adopt*
//! and *Start* can refuse while any remains.

use crate::ask::AskOption;
use crate::assignee::Assignee;
use crate::boundary::{may_carry_boundaries, Boundary, BoundaryAct, BoundaryOn};
use crate::caps::ToolTier;
use crate::connector::{Connector, ConnectorAccount, Operation, ParamKind};
use crate::effort::EffortChoice;
use crate::id::{
    AccountId, ChannelId, ConnectorId, GoalId, OperationId, PrincipalId, ProjectId, WorkflowId,
    WorkstreamId,
};
use crate::listen::{
    valid_signal_name, MessageFilter, PlatformFilter, ProjectFilter, RunEnd, RunFilter,
    SignalFilter,
};
use crate::origin::WorkflowOrigin;
use crate::run::{RunScope, StepRecord};
use crate::start::{Guard, Schedule, StartOn};
use crate::tags::Tags;
use crate::template::{self, Grammar, Placeholder, TemplateError};
use serde::de::Error as _;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::str::FromStr;

// ---------------------------------------------------------------------------
// Names
// ---------------------------------------------------------------------------

/// A step id or an input name: 1–32 chars of `a-z`, `0-9`, `-`, `_`, starting
/// with a letter. Narrower than a slug id on purpose — `.` and `:` are the
/// placeholder grammar's separators (`{steps.<id>.output.<path>}`), so a name
/// that carried them could not be referenced.
fn validate_word(what: &str, s: &str) -> Result<(), crate::CoreError> {
    let invalid = || crate::CoreError::InvalidWord {
        what: what.to_string(),
        value: s.to_string(),
    };
    if s.is_empty() || s.len() > 32 {
        return Err(invalid());
    }
    if !s.as_bytes()[0].is_ascii_lowercase() {
        return Err(invalid());
    }
    if !s
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err(invalid());
    }
    Ok(())
}

macro_rules! word_id {
    ($(#[$doc:meta])* $name:ident, $what:literal) => {
        $(#[$doc])*
        #[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(s: impl Into<String>) -> Result<Self, crate::CoreError> {
                let s = s.into();
                validate_word($what, &s)?;
                Ok(Self(s))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!(stringify!($name), "({:?})"), self.0)
            }
        }

        impl FromStr for $name {
            type Err = crate::CoreError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::new(s)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl PartialEq<str> for $name {
            fn eq(&self, other: &str) -> bool {
                self.0 == other
            }
        }

        impl PartialEq<&str> for $name {
            fn eq(&self, other: &&str) -> bool {
                self.0 == *other
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = String::deserialize(d)?;
                Self::new(s).map_err(serde::de::Error::custom)
            }
        }

        impl schemars::JsonSchema for $name {
            fn schema_name() -> std::borrow::Cow<'static, str> {
                std::borrow::Cow::Borrowed(stringify!($name))
            }
            fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
                schemars::json_schema!({
                    "type": "string",
                    "description": concat!("A ", $what, ": 1-32 chars of a-z, 0-9, '-', '_', starting with a letter"),
                    "pattern": "^[a-z][a-z0-9_-]{0,31}$"
                })
            }
        }
    };
}

word_id!(
    /// A step's id, unique within its workflow. Referenced by flows,
    /// conditions and placeholders, so it is a word and not a label.
    StepId,
    "step id"
);
word_id!(
    /// An input's name, unique within its workflow. Referenced by
    /// `{inputs.<name>}` and by value references.
    InputName,
    "input name"
);

/// A branch label on a `decide` step and on the flows leaving it. Compared
/// byte-exact.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct Branch(String);

impl Branch {
    pub fn new(s: impl Into<String>) -> Result<Self, crate::CoreError> {
        let s = s.into();
        if s.trim().is_empty() || s.len() > 64 {
            return Err(crate::CoreError::InvalidBranch(s));
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Branch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for Branch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Branch({:?})", self.0)
    }
}

impl FromStr for Branch {
    type Err = crate::CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl<'de> Deserialize<'de> for Branch {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::new(s).map_err(serde::de::Error::custom)
    }
}

// ---------------------------------------------------------------------------
// The definition
// ---------------------------------------------------------------------------

/// A workflow: inputs, steps, and the flows between them. The addressable
/// `kind:33412` snapshot; `revision` is the snapshot authority.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Workflow {
    pub id: WorkflowId,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub inputs: Vec<InputDef>,
    /// Display order. The graph is in each step's `then`.
    pub steps: Vec<Step>,
    /// Drawn for the library, installed from the catalog, or designed for
    /// one goal — where the library, the pickers and the goal's own tab each
    /// show it follows from this.
    pub origin: WorkflowOrigin,
    pub author: PrincipalId,
    #[serde(default, skip_serializing_if = "Tags::is_empty")]
    pub tags: Tags,
    /// Put away: out of the library and the pickers, refused for a goal or a
    /// run; the runs that hold a copy are untouched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived: Option<crate::archive::Archived>,
    /// Whether the Decision-Making Agent stands in at the decision points a
    /// run of this workflow reaches — who takes a step's work item, whether an
    /// auto goal adopts it alone — when the workspace has not switched it on
    /// for all. A `judge` step asks it either way.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub decision_making: bool,
    /// Monotonic; every save is a revision. A run holds the copy it started
    /// with, so a later revision never touches it.
    pub revision: u64,
    pub created_at: u64,
}

impl Workflow {
    pub fn is_archived(&self) -> bool {
        self.archived.is_some()
    }
}

/// A typed input a run is started with. A template cannot know a project's
/// ULID or a workspace's people, so a placement or an assignee that varies
/// per goal is an input, and a step names the input ([`ValueRef::Input`]).
///
/// Deserialization refuses a key it does not know — a misspelled `defualt`
/// is an error the author sees, not a default that silently never applies.
/// The kind sits flattened beside the other fields, which is why the check
/// is written by hand ([`refuse_unknown_keys`]) rather than derived.
#[derive(Clone, Debug, PartialEq, Serialize, schemars::JsonSchema)]
pub struct InputDef {
    pub name: InputName,
    pub label: String,
    #[serde(flatten)]
    pub kind: InputKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(default)]
    pub required: bool,
}

/// The keys an input carries beside its kind's own.
const INPUT_FIELDS: &[&str] = &["name", "label", "kind", "default", "required"];

impl<'de> Deserialize<'de> for InputDef {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Shape {
            name: InputName,
            label: String,
            #[serde(flatten)]
            kind: InputKind,
            #[serde(default)]
            default: Option<serde_json::Value>,
            #[serde(default)]
            required: bool,
        }
        let raw = serde_json::Map::<String, serde_json::Value>::deserialize(d)?;
        let tag = raw
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| D::Error::missing_field("kind"))?;
        let own = InputKind::fields_of(tag)
            .ok_or_else(|| D::Error::unknown_variant(tag, &InputKind::NAMES))?;
        refuse_unknown_keys::<D::Error>(&raw, "input", INPUT_FIELDS, own)?;
        let shape: Shape =
            serde_json::from_value(serde_json::Value::Object(raw)).map_err(D::Error::custom)?;
        Ok(InputDef {
            name: shape.name,
            label: shape.label,
            kind: shape.kind,
            default: shape.default,
            required: shape.required,
        })
    }
}

/// Refuse any key of `raw` that is neither one of `base` nor one of `own`
/// (the flattened kind's fields). The message lists what is allowed, so the
/// author can see the misspelling.
pub(crate) fn refuse_unknown_keys<E: serde::de::Error>(
    raw: &serde_json::Map<String, serde_json::Value>,
    what: &str,
    base: &[&str],
    own: &[&str],
) -> Result<(), E> {
    for key in raw.keys() {
        if base.contains(&key.as_str()) || own.contains(&key.as_str()) {
            continue;
        }
        let mut allowed: Vec<&str> = base.iter().chain(own).copied().collect();
        allowed.sort_unstable();
        return Err(E::custom(format!(
            "unknown field `{key}` on a {what}; the fields are {}",
            allowed.join(", ")
        )));
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "kind")]
pub enum InputKind {
    Text,
    Number,
    Bool,
    Choice {
        options: Vec<String>,
    },
    /// `agent:<id>`, `team:<id>` or `human:<hex>`, in wire form.
    Assignee,
    /// A project id.
    Project,
    /// An account of one connector, by id — this machine's way into an
    /// outside platform, which a template cannot know. The connector is
    /// absent while the designer has not chosen one: a problem
    /// (`ProblemKind::Unfilled`), never a refusal.
    Account {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        connector: Option<ConnectorId>,
    },
}

impl InputKind {
    /// Every kind's tag.
    pub const NAMES: [&'static str; 7] = [
        "text", "number", "bool", "choice", "assignee", "project", "account",
    ];

    /// The fields a kind carries beside its `kind` tag, by tag; `None` for a
    /// tag that is not a kind. Kept beside the enum so the two move together
    /// (a test serializes every kind and checks the table).
    pub fn fields_of(tag: &str) -> Option<&'static [&'static str]> {
        Some(match tag {
            "text" | "number" | "bool" | "assignee" | "project" => &[],
            "choice" => &["options"],
            "account" => &["connector"],
            _ => return None,
        })
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            InputKind::Text => "text",
            InputKind::Number => "number",
            InputKind::Bool => "bool",
            InputKind::Choice { .. } => "choice",
            InputKind::Assignee => "assignee",
            InputKind::Project => "project",
            InputKind::Account { .. } => "account",
        }
    }

    /// What a person typed for an input of this kind, as the value a run is
    /// given. A word is the word it is wherever a word is asked — `2.4`,
    /// `true` and `null` are text for a text input, never a number, a yes
    /// or nothing — and a number or a yes/no is read as one. What cannot be
    /// read as its kind stays the word it is, for [`Self::accepts`] to
    /// refuse by name.
    pub fn read(&self, typed: &str) -> serde_json::Value {
        use serde_json::Value as V;
        let word = || V::String(typed.to_string());
        match self {
            InputKind::Number => match serde_json::from_str::<V>(typed.trim()) {
                Ok(number @ V::Number(_)) => number,
                _ => word(),
            },
            InputKind::Bool => match typed.trim() {
                "true" => V::Bool(true),
                "false" => V::Bool(false),
                _ => word(),
            },
            InputKind::Text
            | InputKind::Choice { .. }
            | InputKind::Assignee
            | InputKind::Project
            | InputKind::Account { .. } => word(),
        }
    }

    /// Does a supplied value fit this kind? Pure; used when a run starts.
    pub fn accepts(&self, value: &serde_json::Value) -> bool {
        use serde_json::Value as V;
        match (self, value) {
            (InputKind::Text, V::String(_)) => true,
            (InputKind::Number, V::Number(_)) => true,
            (InputKind::Bool, V::Bool(_)) => true,
            (InputKind::Choice { options }, V::String(s)) => options.iter().any(|o| o == s),
            (InputKind::Assignee, V::String(s)) => s.parse::<Assignee>().is_ok(),
            (InputKind::Project, V::String(s)) => s.parse::<ProjectId>().is_ok(),
            (InputKind::Account { .. }, V::String(s)) => s.parse::<AccountId>().is_ok(),
            _ => false,
        }
    }
}

/// One node of the graph.
///
/// Deserialization refuses a key it does not know, for the step's own fields
/// and for the kind's ([`StepKind::fields_of`]): `retires = 2` is an error,
/// not a step that never retries. The kind is flattened beside `id` and
/// `name`, which is why the check is written by hand.
#[derive(Clone, Debug, PartialEq, Serialize, schemars::JsonSchema)]
pub struct Step {
    pub id: StepId,
    pub name: String,
    /// The kind and its fields sit beside `id` and `name` on the wire
    /// (`"kind": "agent", "instructions": …`), which is what a palette and a
    /// TOML template both want.
    #[serde(flatten)]
    pub kind: StepKind,
    /// What follows. Several flows fan out in parallel; a flow labelled with
    /// a branch is taken only when a gateway chose that branch, or when a
    /// boundary event of that name diverted the step.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub then: Vec<Flow>,
    /// Events heard while this step is live — a timeout that diverts it, a
    /// reminder posted beside it, a message or a signal that ends it — on the
    /// kinds whose work can be stopped ([`may_carry_boundaries`]). A divert's
    /// flows are the ones labelled with its name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boundaries: Vec<Boundary>,
    /// How several incoming flows meet. Irrelevant with one. A loop edge —
    /// a flow back into this step from a step it leads to — never holds an
    /// `all` or a `one` join: the step enters on its forward flows, and the
    /// loop edge re-enters it when taken.
    #[serde(default)]
    pub join: Join,
    /// What a failure of this step does once its retries are spent.
    #[serde(default)]
    pub on_fail: OnFail,
    /// How many times a failed attempt is retried before `on_fail` applies.
    #[serde(default)]
    pub retries: u8,
    /// How many times this step may be entered in one run. A loop is legal;
    /// an unbounded one is not.
    #[serde(default = "default_max_visits")]
    pub max_visits: u8,
    /// Where the card stands on the designer's canvas, in canvas pixels.
    /// Absent, the layered layout places it — the same way on every machine
    /// — until a person moves it or edits the canvas, which writes every
    /// card's place. The run machine and validation never read it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<Point>,
}

/// A place on the designer's canvas: whole pixels, as the canvas snaps them,
/// so the same picture is the same numbers wherever it is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

pub const DEFAULT_MAX_VISITS: u8 = 3;

fn default_max_visits() -> u8 {
    DEFAULT_MAX_VISITS
}

/// The keys a step carries beside its kind's own.
const STEP_FIELDS: &[&str] = &[
    "id",
    "name",
    "kind",
    "then",
    "boundaries",
    "join",
    "on_fail",
    "retries",
    "max_visits",
    "position",
];

impl<'de> Deserialize<'de> for Step {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Shape {
            id: StepId,
            name: String,
            #[serde(flatten)]
            kind: StepKind,
            #[serde(default)]
            then: Vec<Flow>,
            #[serde(default)]
            boundaries: Vec<Boundary>,
            #[serde(default)]
            join: Join,
            #[serde(default)]
            on_fail: OnFail,
            #[serde(default)]
            retries: u8,
            #[serde(default = "default_max_visits")]
            max_visits: u8,
            #[serde(default)]
            position: Option<Point>,
        }
        let raw = serde_json::Map::<String, serde_json::Value>::deserialize(d)?;
        let tag = raw
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| D::Error::missing_field("kind"))?;
        let own = StepKind::fields_of(tag)
            .ok_or_else(|| D::Error::unknown_variant(tag, &StepKind::NAMES))?;
        refuse_unknown_keys::<D::Error>(&raw, "step", STEP_FIELDS, own)?;
        // One level down, the same rule: the tables a step nests — a start's
        // event, a wait's catch, each boundary event and its own event — are
        // flattened shapes serde cannot hold to their keys, so they are
        // checked here, before anything is read.
        match (tag, raw.get("on"), raw.get("until")) {
            ("start", Some(on), _) => StartOn::refuse_unknown::<D::Error>(on)?,
            ("wait", _, Some(until)) => WaitFor::refuse_unknown::<D::Error>(until)?,
            _ => {}
        }
        if let Some(serde_json::Value::Array(boundaries)) = raw.get("boundaries") {
            for b in boundaries {
                Boundary::refuse_unknown::<D::Error>(b)?;
            }
        }
        let shape: Shape =
            serde_json::from_value(serde_json::Value::Object(raw)).map_err(D::Error::custom)?;
        Ok(Step {
            id: shape.id,
            name: shape.name,
            kind: shape.kind,
            then: shape.then,
            boundaries: shape.boundaries,
            join: shape.join,
            on_fail: shape.on_fail,
            retries: shape.retries,
            max_visits: shape.max_visits,
            position: shape.position,
        })
    }
}

/// A directed edge. Deserializes from a bare `"step-id"` as well as from the
/// object, so a template can write `then = ["build"]`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct Flow {
    pub to: StepId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<Branch>,
}

impl Flow {
    pub fn to(to: StepId) -> Self {
        Self { to, branch: None }
    }

    pub fn branch(to: StepId, branch: Branch) -> Self {
        Self {
            to,
            branch: Some(branch),
        }
    }
}

impl<'de> Deserialize<'de> for Flow {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        /// The object form, with no room for a misspelled key.
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Object {
            to: StepId,
            #[serde(default)]
            branch: Option<Branch>,
        }
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire {
            Bare(StepId),
            Full(Object),
        }
        Ok(match Wire::deserialize(d)? {
            Wire::Bare(to) => Flow { to, branch: None },
            Wire::Full(Object { to, branch }) => Flow { to, branch },
        })
    }
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Join {
    /// Every incoming flow that can still arrive has arrived or died.
    #[default]
    All,
    /// The first arrival is enough.
    Any,
    /// Exactly one incoming flow arrives once every one that could has
    /// settled; two is a failure of the step (an exclusive-or at fan-in).
    One,
}

impl Join {
    pub fn as_str(self) -> &'static str {
        match self {
            Join::All => "all",
            Join::Any => "any",
            Join::One => "one",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "on_fail")]
pub enum OnFail {
    /// The run fails.
    #[default]
    Fail,
    /// Continue along the unlabelled flows as if the step were done.
    Skip,
    /// Continue at one named step and nowhere else.
    Then { step: StepId },
}

impl OnFail {
    pub fn as_str(&self) -> &'static str {
        match self {
            OnFail::Fail => "fail",
            OnFail::Skip => "skip",
            OnFail::Then { .. } => "then",
        }
    }
}

/// Either a fixed value or the name of an input that carries it.
///
/// The schema is named after what it carries (`AssigneeOrInput`,
/// `uint64OrInput`): one name for every instantiation would leave the
/// generator to number them, and a number moves when the schema's order does.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
#[schemars(rename = "{T}OrInput")]
pub enum ValueRef<T> {
    Input { input: InputName },
    Fixed(T),
}

/// What a step is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "kind")]
pub enum StepKind {
    /// One way a run begins: by hand, or on an event ([`StartOn`]). Done the
    /// moment the run enters it. Several are several ways in: a run enters
    /// the one it began at and the others are skipped. `inputs` maps the
    /// event onto the run's inputs — templates reading `{event.<path>}` and
    /// nothing else, the one place the event is read — and `guard` says what
    /// an occurrence does while runs it started are still live. A `manual`
    /// start maps nothing and has no guard.
    Start {
        on: StartOn,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        inputs: BTreeMap<String, String>,
        #[serde(default, skip_serializing_if = "Guard::is_default")]
        guard: Guard,
    },
    /// A work item run by an agent: the existing executor, placement and
    /// harness walk. Completes when the item yields a result.
    Agent {
        instructions: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        assignee: Option<ValueRef<Assignee>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project: Option<ValueRef<ProjectId>>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        harness: Vec<String>,
        /// A model pin for this step's sessions. **Never substituted**: an
        /// unavailable pin fails the step rather than running something the
        /// author did not ask for. Absent means the agent's own plan.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        /// An effort pin for this step's sessions: a level, or `auto` for
        /// the Decision-Making Agent to name one. It wins over the model's
        /// own and the agent's plan, and is then clamped to what the harness
        /// and the model take. Absent means the agent's own.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        effort: Option<EffortChoice>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_schema: Option<serde_json::Value>,
        #[serde(default = "default_tier")]
        tier_ceiling: ToolTier,
    },
    /// A person answers — from the options offered, in free text, or "not
    /// sure" — or does something by hand and marks the step done.
    Human {
        prompt: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        options: Vec<AskOption>,
        #[serde(default)]
        multi: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        assignee: Option<ValueRef<Assignee>>,
    },
    /// A signed decision under the workspace's `approval` policy. Declined is
    /// a failure of the step.
    Approval { prompt: String },
    /// Pass or fail, judged by a command or by a schema over an upstream
    /// output. A check that does not pass is a failure of the step.
    Check { check: CheckKind },
    /// `pick = "first"` (the default): the first rule whose condition holds
    /// names the branch. `pick = "every"`: every rule that holds does, and
    /// their flows are all taken at once. `otherwise` when none holds. Only
    /// the chosen branches' flows are taken.
    Decide {
        rules: Vec<Rule>,
        otherwise: Branch,
        #[serde(default, skip_serializing_if = "Pick::is_first")]
        pick: Pick,
    },
    /// `yes` when `when` holds, else `no`. A decide with one question.
    If { when: Condition },
    /// `on` rendered as text against the run, compared byte-exact to each
    /// case's value; `otherwise` when none matches.
    Switch {
        on: String,
        cases: Vec<Case>,
        otherwise: Branch,
    },
    /// The Decision-Making Agent reads `state` rendered against the run and
    /// picks one of `options` by what each means; `otherwise` when it is not
    /// sure enough, or gives no answer that holds to the decision contract.
    /// Naming the step *is* switching the Decision-Making Agent on for it.
    Judge {
        state: String,
        instructions: String,
        options: Vec<JudgeOption>,
        otherwise: Branch,
        /// How sure the pick must be to be taken; the node's
        /// `decisions.confidence.act` when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min_confidence: Option<f64>,
    },
    /// Every flow out of it at once: done on entry, each unlabelled flow
    /// taken. Where the branches meet, the step they flow into joins them.
    Parallel,
    /// `items` renders to a JSON array; one `each` per item, the body flows
    /// back into this step, then `done`.
    ForEach {
        items: String,
        #[serde(default = "default_max_iterations")]
        max_iterations: u16,
    },
    /// `loop` while `when` holds — tested on every entry, the first included
    /// — then `done`.
    While {
        when: Condition,
        #[serde(default = "default_max_iterations")]
        max_iterations: u16,
    },
    /// Call one operation of an outside platform through a connector
    /// installed here. The connector is named by slug and the operation by
    /// id, so the step reads the same on every node; the account — this
    /// machine's way in — is the connector's default, a fixed id, or an
    /// input of kind `account`. Every parameter is a template over the run,
    /// rendered as text; the operation's own parameter kinds type it. The
    /// output is what the operation selects from the answer, checked against
    /// `output_schema` when one is declared.
    /// The connector and the operation are absent while the designer has
    /// not chosen them — a reference is a real id or absent, never an empty
    /// string — and the validator says so (`ProblemKind::Unfilled`).
    Connector {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        connector: Option<ConnectorId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        operation: Option<OperationId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        account: Option<ValueRef<AccountId>>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        params: BTreeMap<String, String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_schema: Option<serde_json::Value>,
        /// The person's word that this write runs with nobody's approval
        /// before it. A step calling an operation that `writes` needs an
        /// `approval` or `human` step upstream, or this — never silence
        /// (`ProblemKind::UngatedWrite`). Meaningless on a read.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        unattended: bool,
    },
    /// Hold until the world moves: a delay, a moment, a schedule, a named
    /// signal, a message, a project's change, a run's end, a platform topic,
    /// or a person releasing it.
    Wait { until: WaitFor },
    /// Raise a named signal — heard by `signal` starts, waits and boundary
    /// events — with a payload of templates rendered against the run. Its
    /// output is `{ signal }`.
    Emit {
        signal: String,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        payload: BTreeMap<String, String>,
    },
    /// Post into a conversation as an agent — `author`, or the Workflow
    /// Agent when none is named. Mentions wake the addressed agents; none
    /// wakes nobody.
    Notify {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        template: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        mentions: Vec<ValueRef<Assignee>>,
        /// Whose message it is. Must resolve to an agent — never a person
        /// or a team; absent, the Workflow Agent speaks.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        author: Option<ValueRef<Assignee>>,
    },
    /// A sub-goal with its own workflow, `refines`-linked to this one.
    Spawn {
        statement_template: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workflow: Option<WorkflowId>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        assignees: Vec<ValueRef<Assignee>>,
        /// What the child's run is given: an input of its workflow to the
        /// template that says it, rendered against this run and read by the
        /// kind the child's input declares. A spawn is a start by hand made
        /// by a step, so what the child's start asks for, the step gives —
        /// every input it requires and no default fills.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        inputs: BTreeMap<String, String>,
        /// Wait for the child's run to finish before this step completes.
        #[serde(default = "default_true")]
        wait: bool,
    },
    /// An end event. `finish = "path"` (the default) ends this path — the
    /// run is done once every path has drained, as after a step with no
    /// `then`; `"done"` finishes the run now, cancelling what is still live;
    /// `"failed"` fails this step and the run now.
    End {
        #[serde(default, skip_serializing_if = "Finish::is_path")]
        finish: Finish,
    },
}

/// One option of a `judge`: the branch it takes, and what choosing it means —
/// the sentence the Decision-Making Agent reads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JudgeOption {
    pub branch: Branch,
    pub meaning: String,
}

/// One case of a `switch`: the value the rendered subject must equal, and
/// the branch that takes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub value: String,
    pub branch: Branch,
}

/// The bound a loop kind carries on its own iterations; a `for_each` with
/// more items, or a `while` that gets this far, fails the step.
pub const DEFAULT_MAX_ITERATIONS: u16 = 100;

fn default_max_iterations() -> u16 {
    DEFAULT_MAX_ITERATIONS
}

/// The branch words the branching kinds choose between, fixed so a canvas
/// and a template read the same flows: `if` takes `yes`/`no`, `for_each`
/// takes `each`/`done`, `while` takes `loop`/`done`.
pub mod branch {
    pub const YES: &str = "yes";
    pub const NO: &str = "no";
    pub const EACH: &str = "each";
    pub const DONE: &str = "done";
    pub const LOOP: &str = "loop";

    /// A fixed word as a [`super::Branch`]. The words above are all valid —
    /// `every_fixed_branch_word_is_a_valid_branch` holds each of them to
    /// `Branch::new` — so nothing is checked, and nothing can panic, here.
    pub fn of(word: &'static str) -> super::Branch {
        super::Branch(word.to_string())
    }
}

impl<T: std::fmt::Display> ValueRef<T> {
    /// The value's word for a person: the fixed value, or `{inputs.name}`.
    pub fn word(&self) -> String {
        match self {
            ValueRef::Fixed(v) => v.to_string(),
            ValueRef::Input { input } => format!("{{inputs.{input}}}"),
        }
    }
}

/// The first sentence of `text`, cut at `max` characters with an ellipsis —
/// what a summary row shows of an instruction meant for a session.
pub fn first_sentence(text: &str, max: usize) -> String {
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let end = one_line
        .char_indices()
        .find(|(i, c)| {
            matches!(c, '.' | '!' | '?') && one_line[i + c.len_utf8()..].starts_with(' ')
        })
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(one_line.len());
    let sentence = &one_line[..end];
    if sentence.chars().count() <= max {
        return sentence.to_string();
    }
    let mut cut: String = sentence.chars().take(max.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

impl Step {
    /// One line a person can judge the step by, in the proposal card and the
    /// summary of a design: what an agent is told to do, what a
    /// person is asked, what a check runs, which branches a decision has,
    /// what a wait waits for, where a notification goes, what a child goal
    /// says, how a run ends. Never longer than a line; never the whole
    /// instruction.
    pub fn summary(&self) -> crate::text::Text {
        use crate::text;
        const MAX: usize = SUMMARY_MAX;
        match &self.kind {
            StepKind::Start { on, .. } => on.summary(),
            StepKind::Parallel => text!(
                "step-summary-parallel",
                n = self.then.iter().filter(|f| f.branch.is_none()).count()
            ),
            StepKind::Emit { signal, .. } => {
                text!("step-summary-emit", signal = signal.to_string())
            }
            StepKind::Agent {
                instructions,
                assignee,
                ..
            } => {
                let what = first_sentence(instructions, MAX);
                match assignee {
                    Some(a) => text!("step-summary-agent-assigned", what = what, who = a.word()),
                    None => text!("step-summary-agent", what = what),
                }
            }
            StepKind::Human {
                prompt, options, ..
            } => {
                let what = first_sentence(prompt, MAX);
                if options.is_empty() {
                    text!("step-summary-human", what = what)
                } else {
                    text!(
                        "step-summary-human-options",
                        what = what,
                        options = options
                            .iter()
                            .map(|o| o.label.as_str())
                            .collect::<Vec<_>>()
                            .join(" / ")
                    )
                }
            }
            StepKind::Approval { prompt } => {
                text!("step-summary-approval", what = first_sentence(prompt, MAX))
            }
            StepKind::Check { check } => match check {
                CheckKind::Command { command } => text!(
                    "step-summary-check-command",
                    command = first_sentence(command, MAX)
                ),
                CheckKind::Schema { of: Some(of), .. } => {
                    text!("step-summary-check-schema", of = of.to_string())
                }
                CheckKind::Schema { of: None, .. } => text!("step-summary-check-schema-unchosen"),
            },
            StepKind::Decide {
                rules,
                otherwise,
                pick,
            } => {
                let n = rules.len();
                let branches = rules
                    .iter()
                    .map(|r| r.branch.to_string())
                    .collect::<Vec<_>>()
                    .join(" · ");
                let otherwise = otherwise.to_string();
                match pick {
                    Pick::First => text!(
                        "step-summary-decide",
                        n = n,
                        branches = branches,
                        otherwise = otherwise
                    ),
                    Pick::Every => text!(
                        "step-summary-decide-every",
                        n = n,
                        branches = branches,
                        otherwise = otherwise
                    ),
                }
            }
            StepKind::If { when } => text!(
                "step-summary-if",
                when = when.as_str(),
                yes = branch::YES,
                no = branch::NO
            ),
            StepKind::Switch {
                on,
                cases,
                otherwise,
            } => text!(
                "step-summary-switch",
                on = first_sentence(on, MAX),
                n = cases.len(),
                values = cases
                    .iter()
                    .map(|c| c.value.clone())
                    .collect::<Vec<_>>()
                    .join(" · "),
                otherwise = otherwise.to_string()
            ),
            StepKind::Judge {
                instructions,
                options,
                otherwise,
                ..
            } => text!(
                "step-summary-judge",
                what = first_sentence(instructions, MAX),
                n = options.len(),
                branches = options
                    .iter()
                    .map(|o| o.branch.to_string())
                    .collect::<Vec<_>>()
                    .join(" · "),
                otherwise = otherwise.to_string()
            ),
            StepKind::ForEach {
                items,
                max_iterations,
            } => text!(
                "step-summary-for-each",
                items = first_sentence(items, MAX),
                max = *max_iterations
            ),
            StepKind::While {
                when,
                max_iterations,
            } => text!(
                "step-summary-while",
                when = when.as_str(),
                max = *max_iterations
            ),
            StepKind::Connector {
                connector,
                operation,
                params,
                ..
            } => {
                let params = params.keys().cloned().collect::<Vec<_>>().join(", ");
                match (connector, operation, params.is_empty()) {
                    (Some(c), Some(o), true) => {
                        text!("step-summary-connector", call = format!("{c}.{o}"))
                    }
                    (Some(c), Some(o), false) => text!(
                        "step-summary-connector-params",
                        call = format!("{c}.{o}"),
                        params = params
                    ),
                    (Some(c), None, true) => text!(
                        "step-summary-connector-operation-unchosen",
                        connector = c.to_string()
                    ),
                    (Some(c), None, false) => text!(
                        "step-summary-connector-operation-unchosen-params",
                        connector = c.to_string(),
                        params = params
                    ),
                    (None, _, true) => text!("step-summary-connector-unchosen"),
                    (None, _, false) => {
                        text!("step-summary-connector-unchosen-params", params = params)
                    }
                }
            }
            StepKind::Wait { until } => match until {
                WaitFor::Delay { secs } => text!("step-summary-wait-delay", secs = secs.word()),
                WaitFor::Time { at } => {
                    text!("step-summary-wait-time", at = first_sentence(at, MAX))
                }
                WaitFor::Schedule { cron, .. } => {
                    text!("step-summary-wait-schedule", cron = cron.word())
                }
                WaitFor::Signal { filter } => {
                    text!("step-summary-wait-signal", name = filter.name.clone())
                }
                WaitFor::Message { filter } => match &filter.r#in {
                    Some(scope) => text!("step-summary-wait-message-in", scope = scope.clone()),
                    None => text!("step-summary-wait-message"),
                },
                WaitFor::Project { filter } => match &filter.project {
                    Some(project) => text!(
                        "step-summary-wait-project",
                        change = filter.change.as_str(),
                        project = project.word()
                    ),
                    None => text!("step-summary-wait-project-unchosen"),
                },
                WaitFor::Run { filter } => {
                    let outcome = filter.outcome.map_or("any", RunEnd::as_str);
                    match &filter.workflow {
                        Some(w) => text!(
                            "step-summary-wait-run-of",
                            workflow = w.to_string(),
                            outcome = outcome
                        ),
                        None => text!("step-summary-wait-run", outcome = outcome),
                    }
                }
                WaitFor::Platform { filter } => {
                    text!("step-summary-wait-platform", topic = filter.topic.clone())
                }
                WaitFor::Release => text!("step-summary-wait-release"),
            },
            StepKind::Notify {
                scope,
                template,
                author,
                ..
            } => {
                let what = first_sentence(template, MAX);
                match (scope, author) {
                    (Some(s), Some(a)) => text!(
                        "step-summary-notify-scope-as",
                        scope = s.to_string(),
                        what = what,
                        who = a.word()
                    ),
                    (Some(s), None) => {
                        text!(
                            "step-summary-notify-scope",
                            scope = s.to_string(),
                            what = what
                        )
                    }
                    (None, Some(a)) => text!("step-summary-notify-as", what = what, who = a.word()),
                    (None, None) => text!("step-summary-notify", what = what),
                }
            }
            StepKind::Spawn {
                statement_template,
                wait,
                ..
            } => {
                let what = first_sentence(statement_template, MAX);
                if *wait {
                    text!("step-summary-spawn", what = what)
                } else {
                    text!("step-summary-spawn-no-wait", what = what)
                }
            }
            StepKind::End { finish } => match finish {
                Finish::Path => text!("step-summary-end-path"),
                Finish::Done => text!("step-summary-end-done"),
                Finish::Failed => text!("step-summary-end-failed"),
            },
        }
    }
}

/// The longest a summary quotes of what a person wrote — an instruction, a
/// question, a command.
const SUMMARY_MAX: usize = 140;

impl StartOn {
    /// What begins a run on this event, in words: a start step's summary,
    /// and a listener's — whose event is the step's as it was armed, every
    /// input read.
    pub fn summary(&self) -> crate::text::Text {
        start_summary(self, SUMMARY_MAX)
    }
}

/// A start step's line: what begins a run there, in words.
fn start_summary(on: &StartOn, max: usize) -> crate::text::Text {
    use crate::text;
    match on {
        StartOn::Manual => text!("step-summary-start-manual"),
        StartOn::Schedule { schedule } => schedule_summary(schedule),
        StartOn::Hook { public: false } => text!("step-summary-start-hook"),
        StartOn::Hook { public: true } => text!("step-summary-start-hook-public"),
        StartOn::Message { filter } => match &filter.r#in {
            Some(scope) => text!("step-summary-start-message-in", scope = scope.clone()),
            None => text!("step-summary-start-message"),
        },
        StartOn::Signal { filter } => {
            text!("step-summary-start-signal", name = filter.name.clone())
        }
        StartOn::Project { filter } => match &filter.project {
            Some(project) => text!(
                "step-summary-start-project",
                change = filter.change.as_str(),
                project = project.word()
            ),
            None => text!("step-summary-start-project-unchosen"),
        },
        StartOn::Run { filter } => {
            let outcome = filter.outcome.map_or("any", RunEnd::as_str);
            match &filter.workflow {
                Some(w) => text!(
                    "step-summary-start-run-of",
                    workflow = w.to_string(),
                    outcome = outcome
                ),
                None => text!("step-summary-start-run", outcome = outcome),
            }
        }
        StartOn::Platform { filter } => {
            text!("step-summary-start-platform", topic = filter.topic.clone())
        }
        StartOn::Connector {
            connector: Some(c),
            operation: Some(o),
            ..
        } => text!("step-summary-start-connector", call = format!("{c}.{o}")),
        StartOn::Connector { .. } => text!("step-summary-start-connector-unchosen"),
        StartOn::Check {
            command, fire_on, ..
        } => text!(
            "step-summary-start-check",
            command = first_sentence(command, max),
            fire_on = fire_on.as_str()
        ),
    }
}

fn schedule_summary(schedule: &Schedule) -> crate::text::Text {
    use crate::text;
    match (&schedule.every, &schedule.cron) {
        (Some(secs), None) => text!("step-summary-start-every", secs = secs.word()),
        (None, Some(cron)) => text!("step-summary-start-cron", cron = cron.word()),
        _ => text!("step-summary-start-schedule-unset"),
    }
}

impl Step {
    /// Every boundary event's name, in declaration order.
    pub fn boundary_names(&self) -> Vec<&Branch> {
        self.boundaries.iter().map(|b| &b.name).collect()
    }

    /// The names of the boundaries that divert — the labels of the step's
    /// divert flows.
    pub fn divert_names(&self) -> Vec<&Branch> {
        self.boundaries
            .iter()
            .filter(|b| b.diverts())
            .map(|b| &b.name)
            .collect()
    }

    pub fn has_diverts(&self) -> bool {
        self.boundaries.iter().any(Boundary::diverts)
    }

    pub fn boundary(&self, name: &Branch) -> Option<&Boundary> {
        self.boundaries.iter().find(|b| &b.name == name)
    }

    /// Whether `flow` is taken only when one of the step's boundary events
    /// diverts it.
    pub fn is_divert_flow(&self, flow: &Flow) -> bool {
        flow.branch
            .as_ref()
            .is_some_and(|label| self.divert_names().contains(&label))
    }

    /// The event a `start` step begins on.
    pub fn start_on(&self) -> Option<&StartOn> {
        match &self.kind {
            StepKind::Start { on, .. } => Some(on),
            _ => None,
        }
    }

    /// Every template the step renders against the run: its kind's and its
    /// boundary events'.
    pub fn templates(&self) -> Vec<&str> {
        let mut out = self.kind.templates();
        for b in &self.boundaries {
            out.extend(b.templates());
        }
        out
    }

    /// Every input the step reads through a reference: its kind's and its
    /// boundary events'.
    pub fn input_refs(&self) -> Vec<(&InputName, &'static str)> {
        let mut out = self.kind.input_refs();
        for b in &self.boundaries {
            out.extend(b.input_refs());
        }
        out
    }

    /// Every assignee the step names: its kind's and its boundary events'.
    pub fn assignee_refs(&self) -> Vec<&ValueRef<Assignee>> {
        let mut out = self.kind.assignee_refs();
        for b in &self.boundaries {
            out.extend(b.assignee_refs());
        }
        out
    }
}

fn default_tier() -> ToolTier {
    ToolTier::Write
}

fn default_true() -> bool {
    true
}

impl StepKind {
    /// The `kind` tag, as the index column and the palette spell it.
    pub fn as_str(&self) -> &'static str {
        match self {
            StepKind::Start { .. } => "start",
            StepKind::Agent { .. } => "agent",
            StepKind::Human { .. } => "human",
            StepKind::Approval { .. } => "approval",
            StepKind::Check { .. } => "check",
            StepKind::Decide { .. } => "decide",
            StepKind::If { .. } => "if",
            StepKind::Switch { .. } => "switch",
            StepKind::Judge { .. } => "judge",
            StepKind::Parallel => "parallel",
            StepKind::ForEach { .. } => "for_each",
            StepKind::While { .. } => "while",
            StepKind::Connector { .. } => "connector",
            StepKind::Wait { .. } => "wait",
            StepKind::Emit { .. } => "emit",
            StepKind::Notify { .. } => "notify",
            StepKind::Spawn { .. } => "spawn",
            StepKind::End { .. } => "end",
        }
    }

    /// Every kind's tag, in palette order: events, gateways, loops, tasks.
    pub const NAMES: [&'static str; 18] = [
        "start",
        "wait",
        "emit",
        "end",
        "decide",
        "if",
        "switch",
        "judge",
        "parallel",
        "for_each",
        "while",
        "agent",
        "human",
        "approval",
        "check",
        "connector",
        "notify",
        "spawn",
    ];

    /// Which family a kind belongs to — what the palette groups it under and
    /// the canvas draws it as.
    pub fn family(&self) -> Family {
        match self {
            StepKind::Start { .. }
            | StepKind::Wait { .. }
            | StepKind::Emit { .. }
            | StepKind::End { .. } => Family::Event,
            StepKind::Decide { .. }
            | StepKind::If { .. }
            | StepKind::Switch { .. }
            | StepKind::Judge { .. }
            | StepKind::Parallel => Family::Gateway,
            StepKind::ForEach { .. } | StepKind::While { .. } => Family::Loop,
            StepKind::Agent { .. }
            | StepKind::Human { .. }
            | StepKind::Approval { .. }
            | StepKind::Check { .. }
            | StepKind::Connector { .. }
            | StepKind::Notify { .. }
            | StepKind::Spawn { .. } => Family::Task,
        }
    }

    /// The branches a branching kind can choose between — a `decide`'s rules
    /// and `otherwise`, an `if`'s `yes`/`no`, a `switch`'s cases and
    /// `otherwise`, a loop's body branch and `done` — and `None` for a kind
    /// whose flows are unlabelled. Duplicates are kept: validation names
    /// them.
    pub fn branches(&self) -> Option<Vec<Branch>> {
        Some(match self {
            StepKind::Decide {
                rules, otherwise, ..
            } => rules
                .iter()
                .map(|r| r.branch.clone())
                .chain(std::iter::once(otherwise.clone()))
                .collect(),
            StepKind::If { .. } => vec![branch::of(branch::YES), branch::of(branch::NO)],
            StepKind::Switch {
                cases, otherwise, ..
            } => cases
                .iter()
                .map(|c| c.branch.clone())
                .chain(std::iter::once(otherwise.clone()))
                .collect(),
            StepKind::Judge {
                options, otherwise, ..
            } => options
                .iter()
                .map(|o| o.branch.clone())
                .chain(std::iter::once(otherwise.clone()))
                .collect(),
            StepKind::ForEach { .. } | StepKind::While { .. } => {
                let (body, exit) = self.loop_branches()?;
                vec![body, exit]
            }
            _ => return None,
        })
    }

    /// `(body branch, exit branch)` for a loop kind; `None` for the rest.
    pub fn loop_branches(&self) -> Option<(Branch, Branch)> {
        match self {
            StepKind::ForEach { .. } => Some((branch::of(branch::EACH), branch::of(branch::DONE))),
            StepKind::While { .. } => Some((branch::of(branch::LOOP), branch::of(branch::DONE))),
            _ => None,
        }
    }

    /// A kind that re-enters its body per iteration and carries a cursor.
    pub fn is_loop(&self) -> bool {
        matches!(self, StepKind::ForEach { .. } | StepKind::While { .. })
    }

    /// Every top-level condition this kind carries: a `decide`'s rules, an
    /// `if`'s and a `while`'s `when`. Validation walks their leaves.
    pub fn conditions(&self) -> Vec<&Condition> {
        match self {
            StepKind::Decide { rules, .. } => rules.iter().map(|r| &r.when).collect(),
            StepKind::If { when } | StepKind::While { when, .. } => vec![when],
            _ => vec![],
        }
    }

    /// What a kind's output promises to a later step — what
    /// `{steps.<id>.output.<field>}` and an `output_equals` path may read.
    /// The run machine and the engine write these shapes
    /// (`run.rs` `enter_for_each`/`enter_while`/`Switch`, `effects.rs`
    /// `run_check`/`notify`/`spawn`); an agent's or a connector's result is
    /// judged by its `output_schema`, so its promise is the schema's
    /// `required` and nothing without one.
    pub fn promised(&self) -> Promised<'_> {
        match self {
            StepKind::Agent { output_schema, .. } | StepKind::Connector { output_schema, .. } => {
                Promised::Schema(output_schema.as_ref())
            }
            StepKind::Human { .. } => Promised::Answer,
            StepKind::Check { .. } => Promised::Keys(&["evidence"]),
            StepKind::Notify { .. } => Promised::Keys(&["message"]),
            StepKind::Emit { .. } => Promised::Keys(&["signal"]),
            StepKind::Spawn { wait: true, .. } => Promised::Keys(&["child", "outcome"]),
            StepKind::Spawn { wait: false, .. } => Promised::Keys(&["child"]),
            StepKind::Switch { .. } => Promised::Keys(&["value"]),
            StepKind::Judge { .. } => Promised::Keys(&["choice", "confidence", "judged"]),
            StepKind::ForEach { .. } => Promised::Keys(&["item", "index", "count"]),
            StepKind::While { .. } => Promised::Keys(&["index"]),
            StepKind::Wait { until } if until.hears_the_world() => Promised::Payload,
            StepKind::Wait { .. }
            | StepKind::Start { .. }
            | StepKind::Approval { .. }
            | StepKind::Decide { .. }
            | StepKind::If { .. }
            | StepKind::Parallel
            | StepKind::End { .. } => Promised::Nothing,
        }
    }

    /// The fields a kind carries beside its `kind` tag, by tag; `None` for a
    /// tag that is not a kind. The wire's closed vocabulary, used to refuse a
    /// misspelled key; a test serializes every kind and checks this table.
    pub fn fields_of(tag: &str) -> Option<&'static [&'static str]> {
        Some(match tag {
            "start" => &["on", "inputs", "guard"],
            "agent" => &[
                "instructions",
                "assignee",
                "project",
                "harness",
                "model",
                "effort",
                "output_schema",
                "tier_ceiling",
            ],
            "human" => &["prompt", "options", "multi", "assignee"],
            "approval" => &["prompt"],
            "check" => &["check"],
            "decide" => &["rules", "otherwise", "pick"],
            "if" => &["when"],
            "switch" => &["on", "cases", "otherwise"],
            "judge" => &[
                "state",
                "instructions",
                "options",
                "otherwise",
                "min_confidence",
            ],
            "parallel" => &[],
            "for_each" => &["items", "max_iterations"],
            "while" => &["when", "max_iterations"],
            "connector" => &[
                "connector",
                "operation",
                "account",
                "params",
                "output_schema",
                "unattended",
            ],
            "wait" => &["until"],
            "emit" => &["signal", "payload"],
            "notify" => &["scope", "template", "mentions", "author"],
            "spawn" => &[
                "statement_template",
                "workflow",
                "assignees",
                "inputs",
                "wait",
            ],
            "end" => &["finish"],
            _ => return None,
        })
    }

    /// Every template string this kind renders against the run, for
    /// validation and rendering. A start's strings are read under their own
    /// grammars ([`StartOn::templates`], its input mapping), never the run's.
    pub fn templates(&self) -> Vec<&str> {
        match self {
            StepKind::Start { .. } | StepKind::Parallel => vec![],
            StepKind::Emit { signal, payload } => std::iter::once(signal.as_str())
                .chain(payload.values().map(String::as_str))
                .collect(),
            StepKind::Agent { instructions, .. } => vec![instructions.as_str()],
            StepKind::Human { prompt, .. } | StepKind::Approval { prompt } => {
                vec![prompt.as_str()]
            }
            StepKind::Check {
                check: CheckKind::Command { command },
            } => vec![command.as_str()],
            StepKind::Check { .. }
            | StepKind::Decide { .. }
            | StepKind::If { .. }
            | StepKind::While { .. }
            | StepKind::End { .. } => vec![],
            StepKind::Switch { on, .. } => vec![on.as_str()],
            StepKind::Judge {
                state,
                instructions,
                ..
            } => vec![state.as_str(), instructions.as_str()],
            StepKind::ForEach { items, .. } => vec![items.as_str()],
            StepKind::Connector { params, .. } => params.values().map(String::as_str).collect(),
            StepKind::Wait { until } => until.templates(),
            StepKind::Notify {
                scope, template, ..
            } => {
                let mut v = vec![template.as_str()];
                if let Some(s) = scope {
                    v.push(s.as_str());
                }
                v
            }
            StepKind::Spawn {
                statement_template,
                inputs,
                ..
            } => std::iter::once(statement_template.as_str())
                .chain(inputs.values().map(String::as_str))
                .collect(),
        }
    }

    /// The assignee references this kind carries.
    pub fn assignee_refs(&self) -> Vec<&ValueRef<Assignee>> {
        match self {
            StepKind::Start { on, .. } => on.assignee_refs(),
            StepKind::Wait { until } => until.assignee_refs(),
            StepKind::Agent { assignee, .. } | StepKind::Human { assignee, .. } => {
                assignee.iter().collect()
            }
            StepKind::Notify {
                mentions, author, ..
            } => mentions.iter().chain(author.iter()).collect(),
            StepKind::Spawn { assignees, .. } => assignees.iter().collect(),
            _ => vec![],
        }
    }

    /// Every input this kind reads through a [`ValueRef::Input`], with the
    /// input kind the reference wants — the one list validation, the
    /// unused-input check and the designer consult.
    pub fn input_refs(&self) -> Vec<(&InputName, &'static str)> {
        match self {
            // A start's and a wait's filters say which of their references
            // are assignees and which are something else.
            StepKind::Start { on, .. } => return on.input_refs(),
            StepKind::Wait { until } => return until.input_refs(),
            _ => {}
        }
        let mut out: Vec<(&InputName, &'static str)> = Vec::new();
        for r in self.assignee_refs() {
            if let ValueRef::Input { input } = r {
                out.push((input, "assignee"));
            }
        }
        match self {
            StepKind::Agent {
                project: Some(ValueRef::Input { input }),
                ..
            } => out.push((input, "project")),
            StepKind::Connector {
                account: Some(ValueRef::Input { input }),
                ..
            } => out.push((input, "account")),
            _ => {}
        }
        out
    }
}

/// The palette's groups: what happens, what routes, what repeats, what works.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    Event,
    Gateway,
    Loop,
    Task,
}

impl Family {
    pub fn as_str(self) -> &'static str {
        match self {
            Family::Event => "event",
            Family::Gateway => "gateway",
            Family::Loop => "loop",
            Family::Task => "task",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "check")]
pub enum CheckKind {
    /// Exit 0 in the goal's placement is a pass.
    Command { command: String },
    /// An upstream step's output must satisfy this JSON Schema. The step is
    /// absent while the designer has not chosen one (`ProblemKind::Unfilled`).
    Schema {
        schema: serde_json::Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        of: Option<StepId>,
    },
}

/// One rule of a `decide` step.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub when: Condition,
    pub branch: Branch,
}

/// What a `wait` holds for — a catch event. Its filters are the start
/// events' types ([`crate::listen`]), their fields rendered against the run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "until")]
pub enum WaitFor {
    /// Seconds from the moment the step is entered — fixed, or read from a
    /// `number` input so one template serves many holds.
    Delay { secs: ValueRef<u64> },
    /// A moment: `at` renders against the run to Unix seconds or an RFC 3339
    /// timestamp — a deadline an input or an upstream step names.
    Time { at: String },
    /// The next occurrence of a cron expression — fixed, or read from a
    /// `text` input.
    Schedule {
        cron: ValueRef<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tz: Option<String>,
    },
    /// A named signal whose payload carries every field, exactly.
    Signal {
        #[serde(flatten)]
        filter: SignalFilter,
    },
    /// A message posted into a conversation.
    Message {
        #[serde(flatten)]
        filter: MessageFilter,
    },
    /// A change in one project.
    Project {
        #[serde(flatten)]
        filter: ProjectFilter,
    },
    /// A run that ended.
    Run {
        #[serde(flatten)]
        filter: RunFilter,
    },
    /// One of the engine's own bus events.
    Platform {
        #[serde(flatten)]
        filter: PlatformFilter,
    },
    /// A person releases it — or a call does (`POST …/release`), with a
    /// payload that becomes the step's output.
    Release,
}

impl WaitFor {
    pub fn as_str(&self) -> &'static str {
        match self {
            WaitFor::Delay { .. } => "delay",
            WaitFor::Time { .. } => "time",
            WaitFor::Schedule { .. } => "schedule",
            WaitFor::Signal { .. } => "signal",
            WaitFor::Message { .. } => "message",
            WaitFor::Project { .. } => "project",
            WaitFor::Run { .. } => "run",
            WaitFor::Platform { .. } => "platform",
            WaitFor::Release => "release",
        }
    }

    pub const NAMES: [&'static str; 9] = [
        "delay", "time", "schedule", "signal", "message", "project", "run", "platform", "release",
    ];

    /// The fields a catch carries beside its `until` tag, by tag.
    pub fn fields_of(tag: &str) -> Option<&'static [&'static str]> {
        Some(match tag {
            "delay" => &["secs"],
            "time" => &["at"],
            "schedule" => &["cron", "tz"],
            "signal" => SignalFilter::FIELDS,
            "message" => MessageFilter::FIELDS,
            "project" => ProjectFilter::FIELDS,
            "run" => RunFilter::FIELDS,
            "platform" => PlatformFilter::FIELDS,
            "release" => &[],
            _ => return None,
        })
    }

    pub(crate) fn refuse_unknown<E: serde::de::Error>(raw: &serde_json::Value) -> Result<(), E> {
        let Some(map) = raw.as_object() else {
            return Ok(());
        };
        let tag = map
            .get("until")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| E::missing_field("until"))?;
        let own =
            WaitFor::fields_of(tag).ok_or_else(|| E::unknown_variant(tag, &WaitFor::NAMES))?;
        refuse_unknown_keys::<E>(map, "wait", &["until"], own)
    }

    /// Held by something the world does rather than by a clock: the step's
    /// output is what was heard ([`RunEvent::Heard`](crate::run::RunEvent)),
    /// or what a release carried.
    pub fn hears_the_world(&self) -> bool {
        match self {
            WaitFor::Signal { .. }
            | WaitFor::Message { .. }
            | WaitFor::Project { .. }
            | WaitFor::Run { .. }
            | WaitFor::Platform { .. }
            | WaitFor::Release => true,
            WaitFor::Delay { .. } | WaitFor::Time { .. } | WaitFor::Schedule { .. } => false,
        }
    }

    /// Driven by the clock: `Elapsed` completes it.
    pub fn is_timer(&self) -> bool {
        matches!(
            self,
            WaitFor::Delay { .. } | WaitFor::Time { .. } | WaitFor::Schedule { .. }
        )
    }

    /// Completed by an event heard: `Heard` completes it.
    pub fn is_catch(&self) -> bool {
        !self.is_timer() && !matches!(self, WaitFor::Release)
    }

    pub fn templates(&self) -> Vec<&str> {
        match self {
            WaitFor::Delay { .. }
            | WaitFor::Schedule { .. }
            | WaitFor::Run { .. }
            | WaitFor::Release => vec![],
            WaitFor::Time { at } => vec![at.as_str()],
            WaitFor::Signal { filter } => filter.templates(),
            WaitFor::Message { filter } => filter.templates(),
            WaitFor::Project { filter } => filter.templates(),
            WaitFor::Platform { filter } => filter.templates(),
        }
    }

    pub fn input_refs(&self) -> Vec<(&InputName, &'static str)> {
        match self {
            WaitFor::Delay {
                secs: ValueRef::Input { input },
            } => vec![(input, "number")],
            WaitFor::Schedule {
                cron: ValueRef::Input { input },
                ..
            } => vec![(input, "text")],
            WaitFor::Message { filter } => filter.input_refs(),
            WaitFor::Project { filter } => filter.input_refs(),
            _ => vec![],
        }
    }

    pub fn assignee_refs(&self) -> Vec<&ValueRef<Assignee>> {
        match self {
            WaitFor::Message { filter } => filter.assignee_refs(),
            _ => vec![],
        }
    }
}

/// Which of a `decide`'s rules choose.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Pick {
    /// The first rule that holds: one branch.
    #[default]
    First,
    /// Every rule that holds: as many branches as hold, all at once.
    Every,
}

impl Pick {
    pub fn as_str(self) -> &'static str {
        match self {
            Pick::First => "first",
            Pick::Every => "every",
        }
    }

    pub fn is_first(&self) -> bool {
        matches!(self, Pick::First)
    }
}

/// What an `end` step ends.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Finish {
    /// This path: the run is done once every path has drained.
    #[default]
    Path,
    /// The run, now: done, and whatever is still live is cancelled.
    Done,
    /// The run, now: this step fails, and the run with it.
    Failed,
}

impl Finish {
    pub fn as_str(self) -> &'static str {
        match self {
            Finish::Path => "path",
            Finish::Done => "done",
            Finish::Failed => "failed",
        }
    }

    pub fn is_path(&self) -> bool {
        matches!(self, Finish::Path)
    }
}

/// How a run ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunOutcome {
    Done,
    Failed,
}

impl RunOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            RunOutcome::Done => "done",
            RunOutcome::Failed => "failed",
        }
    }
}

// ---------------------------------------------------------------------------
// Conditions
// ---------------------------------------------------------------------------

/// A rule's test. A **closed, typed set**, deliberately not an expression
/// language: an evaluator would make every workflow a scripting surface
/// running unreviewed on the node's own privileges, and it would be untestable
/// without I/O. Six leaves and four combinators (`all`, `any`, `one`,
/// `not`) that nest to [`Condition::MAX_DEPTH`]; another case is a code
/// change with a test, which is the point. No leaf reads the event that
/// began the run: a start's input mapping turns it into typed inputs, and a
/// rule reads those.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "condition")]
pub enum Condition {
    InputEquals {
        input: InputName,
        value: serde_json::Value,
    },
    OutputEquals {
        step: StepId,
        path: String,
        value: serde_json::Value,
    },
    OutputMatches {
        step: StepId,
        path: String,
        contains: String,
    },
    /// A `human` step's answer selected this option.
    Answered { step: StepId, option: String },
    /// A `check` or `approval` step passed (`Done`) or did not (`Failed`,
    /// or diverted by one of its boundary events).
    Outcome { step: StepId, passed: bool },
    /// Hour-of-day window, inclusive, in UTC. `from_hour > to_hour` wraps
    /// midnight (22..6 is "overnight").
    Between { from_hour: u8, to_hour: u8 },
    /// Every condition in `of` holds. Empty holds.
    All { of: Vec<Condition> },
    /// At least one condition in `of` holds. Empty does not.
    Any { of: Vec<Condition> },
    /// Exactly one condition in `of` holds. Empty does not.
    One { of: Vec<Condition> },
    /// The condition does not hold.
    Not { of: Box<Condition> },
}

/// Everything a [`Condition`] may read. Assembled by the run so evaluation
/// stays pure.
#[derive(Clone, Copy, Debug)]
pub struct ConditionCtx<'a> {
    pub inputs: &'a BTreeMap<String, serde_json::Value>,
    pub steps: &'a BTreeMap<StepId, StepRecord>,
    /// UTC hour of day, 0..=23.
    pub hour: u8,
}

impl Condition {
    pub fn as_str(&self) -> &'static str {
        match self {
            Condition::InputEquals { .. } => "input_equals",
            Condition::OutputEquals { .. } => "output_equals",
            Condition::OutputMatches { .. } => "output_matches",
            Condition::Answered { .. } => "answered",
            Condition::Outcome { .. } => "outcome",
            Condition::Between { .. } => "between",
            Condition::All { .. } => "all",
            Condition::Any { .. } => "any",
            Condition::One { .. } => "one",
            Condition::Not { .. } => "not",
        }
    }

    /// How deep a condition may nest. A leaf is depth 1; deeper than this is
    /// a definition nobody can read on a card, and a stack nobody should
    /// trust from the wire.
    pub const MAX_DEPTH: usize = 8;

    /// The combinator's children, or none for a leaf.
    fn children(&self) -> &[Condition] {
        match self {
            Condition::All { of } | Condition::Any { of } | Condition::One { of } => of,
            Condition::Not { of } => std::slice::from_ref(of),
            _ => &[],
        }
    }

    fn is_leaf(&self) -> bool {
        !matches!(
            self,
            Condition::All { .. }
                | Condition::Any { .. }
                | Condition::One { .. }
                | Condition::Not { .. }
        )
    }

    /// Every leaf under this condition, itself included when it is one, in
    /// order. Validation judges leaves; combinators only shape them.
    pub fn leaves(&self) -> Vec<&Condition> {
        if self.is_leaf() {
            return vec![self];
        }
        let mut out = Vec::new();
        let mut stack: Vec<&Condition> = self.children().iter().rev().collect();
        while let Some(c) = stack.pop() {
            if c.is_leaf() {
                out.push(c);
            } else {
                stack.extend(c.children().iter().rev());
            }
        }
        out
    }

    /// The steps this condition reads, through every combinator, in order.
    pub fn reads_steps(&self) -> Vec<&StepId> {
        self.leaves()
            .into_iter()
            .filter_map(|c| match c {
                Condition::OutputEquals { step, .. }
                | Condition::OutputMatches { step, .. }
                | Condition::Answered { step, .. }
                | Condition::Outcome { step, .. } => Some(step),
                _ => None,
            })
            .collect()
    }

    /// The inputs this condition reads, through every combinator, in order.
    pub fn reads_inputs(&self) -> Vec<&InputName> {
        self.leaves()
            .into_iter()
            .filter_map(|c| match c {
                Condition::InputEquals { input, .. } => Some(input),
                _ => None,
            })
            .collect()
    }

    /// Nesting depth: a leaf is 1, a combinator one more than its deepest
    /// child (an empty one is 1).
    pub fn depth(&self) -> usize {
        // Iterative: the depth is bounded only by what validation refuses,
        // and a hostile definition must not overflow the stack here.
        let mut deepest = 1usize;
        let mut stack: Vec<(&Condition, usize)> = vec![(self, 1)];
        while let Some((c, d)) = stack.pop() {
            deepest = deepest.max(d);
            for child in c.children() {
                stack.push((child, d + 1));
            }
        }
        deepest
    }

    /// Whether any `all`/`any`/`one` under this condition has nothing in it —
    /// a test that always answers the same way, which is never what was meant.
    pub fn empty_combinator(&self) -> bool {
        let mut stack: Vec<&Condition> = vec![self];
        while let Some(c) = stack.pop() {
            match c {
                Condition::All { of } | Condition::Any { of } | Condition::One { of }
                    if of.is_empty() =>
                {
                    return true;
                }
                _ => stack.extend(c.children()),
            }
        }
        false
    }

    pub fn holds(&self, ctx: &ConditionCtx<'_>) -> bool {
        use crate::run::StepState;
        let output = |step: &StepId, path: &str| -> Option<&serde_json::Value> {
            let out = ctx.steps.get(step)?.output.as_ref()?;
            if path.is_empty() {
                Some(out)
            } else {
                template::json_path(out, path)
            }
        };
        match self {
            Condition::InputEquals { input, value } => {
                ctx.inputs.get(input.as_str()) == Some(value)
            }
            Condition::OutputEquals { step, path, value } => output(step, path) == Some(value),
            Condition::OutputMatches {
                step,
                path,
                contains,
            } => contains_str(output(step, path), contains),
            Condition::Answered { step, option } => ctx
                .steps
                .get(step)
                .and_then(|r| r.answer.as_ref())
                .is_some_and(|a| a.selected.iter().any(|s| s == option)),
            Condition::Outcome { step, passed } => match ctx.steps.get(step).map(|r| &r.state) {
                Some(StepState::Done { .. }) => *passed,
                Some(StepState::Failed | StepState::Diverted { .. }) => !*passed,
                _ => false,
            },
            Condition::Between { from_hour, to_hour } => {
                let h = ctx.hour;
                if from_hour <= to_hour {
                    h >= *from_hour && h <= *to_hour
                } else {
                    h >= *from_hour || h <= *to_hour
                }
            }
            Condition::All { of } => of.iter().all(|c| c.holds(ctx)),
            Condition::Any { of } => of.iter().any(|c| c.holds(ctx)),
            Condition::One { of } => of.iter().filter(|c| c.holds(ctx)).count() == 1,
            Condition::Not { of } => !of.holds(ctx),
        }
    }
}

fn contains_str(value: Option<&serde_json::Value>, needle: &str) -> bool {
    match value {
        Some(serde_json::Value::String(s)) => s.contains(needle),
        Some(other) => other.to_string().contains(needle),
        None => false,
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// The two syntaxes a definition embeds that the core does not parse itself:
/// a cron expression and a JSON Schema. The store implements this with the
/// crates the engine already schedules and checks with; the core stays free
/// of both and of any I/O. [`NoSyntaxChecks`] accepts everything.
pub trait SyntaxChecks {
    /// Why `expr` is not a cron expression this platform can schedule, or
    /// `None` when it is one.
    fn cron_error(&self, expr: &str) -> Option<String>;
    /// Why `schema` is not a JSON Schema, or `None` when it is one.
    fn schema_error(&self, schema: &serde_json::Value) -> Option<String>;
}

/// The checks a caller without a scheduler or a schema compiler passes:
/// every expression and every schema is taken as written.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoSyntaxChecks;

impl SyntaxChecks for NoSyntaxChecks {
    fn cron_error(&self, _expr: &str) -> Option<String> {
        None
    }
    fn schema_error(&self, _schema: &serde_json::Value) -> Option<String> {
        None
    }
}

static NO_SYNTAX_CHECKS: NoSyntaxChecks = NoSyntaxChecks;

/// What the workspace knows and a definition cannot: who exists, which
/// workflows and projects exist, which harnesses and models the runtime can
/// launch, which workflows spawn which, and how to read a cron expression or
/// a schema.
#[derive(Clone, Copy)]
pub struct ValidationCtx<'a> {
    pub assignees: &'a [Assignee],
    pub workflows: &'a [WorkflowId],
    pub projects: &'a [ProjectId],
    /// The harness ids the runtime can launch. An agent step naming another
    /// is a problem. Empty means no runtime has described itself (an offline
    /// tool with no engine), and the check is skipped.
    pub harnesses: &'a [String],
    /// Per harness, the models it lists. A harness absent here has an unknown
    /// list and its pins are not checked — the one soft check, because a
    /// model list is fetched from the harness and may not be cached yet.
    pub models: &'a [(String, Vec<String>)],
    /// The harnesses that take an effort (`HarnessCaps::EFFORT`). An agent
    /// step that pins an effort and names only harnesses outside this list
    /// is a problem. Read only when `harnesses` is not empty: with no runtime
    /// described there is nobody to say which harness has the control.
    pub effort_harnesses: &'a [String],
    /// For every workflow the workspace holds, the workflows its `spawn`
    /// steps name. A `spawn` whose target leads back here is a cycle.
    pub spawns: &'a [(WorkflowId, Vec<WorkflowId>)],
    /// The connectors installed here, whole: a `connector` step is checked
    /// against the operation it names and that operation's parameters.
    pub connectors: &'a [Connector],
    /// This machine's connector accounts. A fixed account a step names must
    /// be one of them; a connector that needs one must have one, named or
    /// default, before a run starts.
    pub accounts: &'a [ConnectorAccount],
    /// The topics the engine's bus emits. A `platform` start or wait naming
    /// another is a problem; empty means the caller has no engine to ask, and
    /// the check is skipped — the one soft check besides model lists.
    pub topics: &'a [&'a str],
    /// The workflows only events start — no manual start, so nothing can run
    /// them by hand. A `spawn` naming one is a problem.
    pub event_only: &'a [WorkflowId],
    /// What every workflow the workspace holds asks of whoever starts it: its
    /// inputs. A `spawn` is held to what its workflow asks.
    pub asks: &'a [(WorkflowId, Vec<InputDef>)],
    pub checks: &'a dyn SyntaxChecks,
}

impl Default for ValidationCtx<'_> {
    fn default() -> Self {
        Self {
            assignees: &[],
            workflows: &[],
            projects: &[],
            harnesses: &[],
            models: &[],
            effort_harnesses: &[],
            spawns: &[],
            connectors: &[],
            accounts: &[],
            topics: &[],
            event_only: &[],
            asks: &[],
            checks: &NO_SYNTAX_CHECKS,
        }
    }
}

impl fmt::Debug for ValidationCtx<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ValidationCtx")
            .field("assignees", &self.assignees)
            .field("workflows", &self.workflows)
            .field("projects", &self.projects)
            .field("harnesses", &self.harnesses)
            .field("models", &self.models)
            .field("effort_harnesses", &self.effort_harnesses)
            .field("spawns", &self.spawns)
            .field("topics", &self.topics)
            .field("event_only", &self.event_only)
            .field("asks", &self.asks)
            .finish_non_exhaustive()
    }
}

/// One thing wrong with a definition. The designer renders problems by kind
/// and selects the step; `Adopt` and `Start` refuse while any exists.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Problem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<StepId>,
    pub kind: ProblemKind,
    /// The sentence, as data — a message of `locales/en/problems.ftl`
    /// (`problem-<kind>-…`) with the step and input names as arguments.
    pub text: crate::text::Text,
}

/// The enum and its `ALL` list from one spelling of the variants, so a kind
/// added here is in `ALL` by construction (a test checks the count against
/// an exhaustive match). The enum is written out in full inside the macro so
/// the desktop's source-reading test still finds `pub enum ProblemKind`.
macro_rules! problem_kinds {
    ($(#[$m:meta])* $vis:vis enum $name:ident { $($(#[$vm:meta])* $v:ident),* $(,)? }) => {
        $(#[$m])*
        $vis enum $name {
            $($(#[$vm])* $v,)*
        }

        impl $name {
            /// Every kind, in declaration order.
            pub const ALL: &'static [$name] = &[$($name::$v,)*];
        }
    };
}

problem_kinds! {
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ProblemKind {
    EmptyName,
    DuplicateStepId,
    DuplicateInput,
    NoStart,
    ManyStarts,
    UnknownStep,
    Unreachable,
    SelfFlow,
    BranchWithoutRule,
    RuleWithoutFlow,
    LabelledFlowOnPlainStep,
    UnlabelledFlowOnDecide,
    DuplicateBranch,
    NotUpstream,
    UnknownInput,
    InputKindMismatch,
    UnknownPlaceholder,
    BadTemplate,
    ZeroVisits,
    UnknownAssignee,
    UnknownWorkflow,
    UnknownProject,
    EndWithSuccessors,
    BadQuestion,
    UnusedInput,
    EmptyRules,
    UnknownHarness,
    UnknownModel,
    BadCron,
    BadSchema,
    SpawnCycle,
    NotifyScopeUnknown,
    /// A condition nests deeper than [`Condition::MAX_DEPTH`], or an
    /// `all`/`any`/`one` in it has nothing in it.
    BadCondition,
    /// Nothing on a loop's body side flows back into it.
    LoopWithoutReturn,
    /// A loop's exit flow leads to a step that flows back into the loop.
    LoopExitReturns,
    /// A loop allows zero iterations.
    ZeroIterations,
    /// A connector step names a connector that is not installed.
    UnknownConnector,
    /// A connector step names an operation its connector does not have.
    UnknownOperation,
    /// A connector step leaves out a parameter the operation requires.
    MissingConnectorParam,
    /// A connector step names an account this node does not hold.
    UnknownAccount,
    /// A `connector` step calls an operation that writes with no `approval`
    /// or `human` step upstream and no `unattended: true` of its own — a
    /// write nobody gated and nobody said they meant to leave ungated.
    UngatedWrite,
    /// A step string reads `{params.…}` or `{account.…}`, which only a
    /// connector definition may.
    ParamOnlyPlaceholder,
    /// A choice not made yet: a connector step's connector or operation, a
    /// schema check's step, an account input's connector. The designer
    /// births these absent and fills them in the inspector; the checks that
    /// depend on the choice wait for it.
    Unfilled,
    /// A `notify` step's `author` is a person or a team; only an agent speaks
    /// for a workflow.
    NotifyAuthorNotAnAgent,
    /// A `{steps.x…}` placeholder, a condition or a schema check names an
    /// ancestor that is not sure to have run when the step is entered: it
    /// runs after the step on a loop's first pass, a branch or an `any`
    /// join reaches the step without it, or it may have failed and been
    /// passed over (`on_fail` skip or then).
    NotAssured,
    /// Reading the output of a kind that yields none, a `human`'s output
    /// rather than its answer, or the answer of a step that is not `human`.
    NoSuchOutput,
    /// `{steps.x.output.<field>}` or a rule's path whose first segment the
    /// producer does not promise: not in its `output_schema`'s `required`,
    /// nothing at all without a schema, or a key a fixed-shape kind never
    /// writes.
    UnpromisedOutput,
    /// A step reads `{goal.statement}` or `{goal.title}` in a run of the
    /// workspace, which has no goal to read — never a problem of the
    /// definition, only of where it is started ([`Workflow::scope_problems`]).
    NeedsGoal,
    /// A `start` step has a flow or a fail route into it: a start begins a
    /// run and nothing inside one leads back to it.
    StartHasIncoming,
    /// More than one `manual` start: a run by hand begins at exactly one.
    ManyManualStarts,
    /// A `manual` start carries an input mapping or a guard — what only an
    /// event start reads.
    ManualStartConfigured,
    /// The event read anywhere but a start's input mapping, or a start's
    /// mapping or event fields reading a root that is not theirs.
    StartPlaceholder,
    /// Boundary events on a step whose work cannot be stopped while it is
    /// live ([`may_carry_boundaries`]).
    BoundaryOnInstantStep,
    /// A reminder (`every`) that diverts: it would stop the step at its first
    /// tick.
    ReminderInterrupts,
    /// A clock that cannot run: zero seconds, a reminder that fires no
    /// times, a schedule that is both `every` and `cron`, or neither.
    BadTimer,
    /// A signal's name that is not dotted lowercase words.
    BadSignalName,
    /// A connector start that cannot poll: an operation that writes, one that
    /// takes a file, or no `key` to tell items apart.
    BadPoll,
    /// A `platform` start or wait naming no topic, or one the engine never
    /// emits.
    UnknownTopic,
    /// A `spawn` naming a workflow only events start: nothing can run it by
    /// hand.
    SpawnNeedsManualEntry,
    /// An agent step pins an effort and none of the harnesses it names has
    /// the effort control: the pin would be sent to nobody.
    UnsupportedEffort,
    /// A `spawn` that gives its workflow an input it does not declare, or
    /// leaves out one it requires and no default fills: the child's run
    /// could never start.
    SpawnInput,
}
}

/// Why a run's inputs do not fit a definition. Every message reaches a
/// person, so each names the input.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum InputError {
    #[error("input `{input}` wants {want}, got {got}")]
    WrongKind {
        input: String,
        want: String,
        got: String,
    },
    #[error("input `{input}` is required and has no default")]
    Missing { input: String },
    #[error("the workflow declares no input named {}", .inputs.iter().map(|k| format!("`{k}`")).collect::<Vec<_>>().join(", "))]
    Unknown { inputs: Vec<String> },
}

impl Workflow {
    pub fn step(&self, id: &StepId) -> Option<&Step> {
        self.steps.iter().find(|s| &s.id == id)
    }

    pub fn input(&self, name: &InputName) -> Option<&InputDef> {
        self.inputs.iter().find(|i| &i.name == name)
    }

    /// What was given, and for an input nobody gave, its default: what a
    /// host listens with. An event's own fields read an input as a run's
    /// steps do — a schedule's `cron`, a check's command — and a default is
    /// the definition's word for what nobody said. Nothing is refused here:
    /// what an occurrence supplies is bound when its run begins
    /// ([`Self::bind_inputs`]).
    pub fn with_defaults(
        &self,
        given: &BTreeMap<String, serde_json::Value>,
    ) -> BTreeMap<String, serde_json::Value> {
        let mut all = given.clone();
        for def in &self.inputs {
            if let Some(default) = &def.default {
                all.entry(def.name.to_string())
                    .or_insert_with(|| default.clone());
            }
        }
        all
    }

    /// The input contract, in one place: a supplied value must fit its
    /// declared kind, a missing one takes its default, a required one with no
    /// default is refused, and a name the definition does not declare is
    /// refused. Used when a run starts and again when it is amended, so an
    /// amendment can never leave a run holding inputs its definition cannot
    /// read.
    pub fn bind_inputs(
        &self,
        mut given: BTreeMap<String, serde_json::Value>,
    ) -> Result<BTreeMap<String, serde_json::Value>, InputError> {
        for def in &self.inputs {
            match given.get(def.name.as_str()) {
                Some(v) => {
                    if !def.kind.accepts(v) {
                        let want = match &def.kind {
                            InputKind::Choice { options } => {
                                format!("choice ({})", options.join(", "))
                            }
                            other => other.as_str().to_string(),
                        };
                        return Err(InputError::WrongKind {
                            input: def.name.to_string(),
                            want,
                            got: v.to_string(),
                        });
                    }
                }
                None => match &def.default {
                    Some(d) => {
                        given.insert(def.name.to_string(), d.clone());
                    }
                    None if def.required => {
                        return Err(InputError::Missing {
                            input: def.name.to_string(),
                        });
                    }
                    None => {}
                },
            }
        }
        let unknown: Vec<String> = given
            .keys()
            .filter(|k| !self.inputs.iter().any(|d| d.name.as_str() == k.as_str()))
            .cloned()
            .collect();
        if !unknown.is_empty() {
            return Err(InputError::Unknown { inputs: unknown });
        }
        Ok(given)
    }

    /// The step ids `step` can flow into: its `then` targets, then its
    /// `on_fail: then` target, each once. An id that names no step is kept —
    /// validation reports it; a walk just finds nothing there.
    pub fn successors(step: &Step) -> Vec<&StepId> {
        let mut out: Vec<&StepId> = Vec::new();
        for f in &step.then {
            if !out.contains(&&f.to) {
                out.push(&f.to);
            }
        }
        if let OnFail::Then { step: target } = &step.on_fail {
            if !out.contains(&target) {
                out.push(target);
            }
        }
        out
    }

    /// The ways a run may begin: the `start` steps, in display order — or, in
    /// a workflow that names none, its root: the step no flow and no
    /// `on_fail: then` route leads into, else (the first step is the target
    /// of a loop) the first step. Empty only for an empty workflow.
    pub fn start_steps(&self) -> Vec<&Step> {
        start_steps_in(&self.steps)
    }

    /// Whether the definition names its starts.
    pub fn has_starts(&self) -> bool {
        self.steps
            .iter()
            .any(|s| matches!(s.kind, StepKind::Start { .. }))
    }

    /// Where a run by hand begins: the `manual` start, else the one root of a
    /// workflow that names no start. `None` for a workflow only events begin,
    /// and for one whose roots are ambiguous (`ManyStarts`).
    pub fn manual_entry(&self) -> Option<&Step> {
        if self.has_starts() {
            return self.steps.iter().find(|s| {
                matches!(
                    &s.kind,
                    StepKind::Start {
                        on: StartOn::Manual,
                        ..
                    }
                )
            });
        }
        match start_steps_in(&self.steps).as_slice() {
            [one] => Some(one),
            _ => None,
        }
    }

    /// Only events begin it: it names starts and none is by hand.
    pub fn is_event_only(&self) -> bool {
        self.has_starts() && self.manual_entry().is_none()
    }

    /// Whether a run may begin at `id`: one of its starts.
    pub fn is_entry(&self, id: &StepId) -> bool {
        self.start_steps().iter().any(|s| &s.id == id)
    }

    /// Every start that begins on an event, with its event.
    pub fn event_starts(&self) -> Vec<(&Step, &StartOn)> {
        self.steps
            .iter()
            .filter_map(|s| match &s.kind {
                StepKind::Start { on, .. } if !on.is_manual() => Some((s, on)),
                _ => None,
            })
            .collect()
    }

    /// What a host must give when it begins listening: for every event
    /// start, the required inputs its mapping does not supply, and the inputs
    /// its event's own fields read — each only when no default fills it. The
    /// union over the starts, since any of them may begin a run.
    pub fn listening_needs(&self) -> BTreeSet<InputName> {
        let mut out = BTreeSet::new();
        let unfilled = |name: &InputName| self.input(name).is_some_and(|d| d.default.is_none());
        for (step, on) in self.event_starts() {
            let StepKind::Start {
                inputs: mapping, ..
            } = &step.kind
            else {
                continue; // LCOV_EXCL_LINE: `event_starts` yields start steps alone; the pattern restates it
            };
            for def in &self.inputs {
                if def.required && def.default.is_none() && !mapping.contains_key(def.name.as_str())
                {
                    out.insert(def.name.clone());
                }
            }
            for (name, _) in on.input_refs() {
                if unfilled(name) {
                    out.insert(name.clone());
                }
            }
            for tmpl in on.templates() {
                for p in template::placeholders_in(tmpl, Grammar::StartEvent).unwrap_or_default() {
                    if let Placeholder::Input(name) = p {
                        if unfilled(&name) {
                            out.insert(name);
                        }
                    } // LCOV_EXCL_LINE: the start-event grammar yields input placeholders alone (template.rs parse_start_event_key)
                }
            }
        }
        out
    }

    /// The edges that close a cycle — `(from, to)` over `then` flows and
    /// `on_fail: then` routes — as a depth-first walk from the start (then
    /// from any step not yet reached, in display order) meets them: an edge
    /// into a step still on the path. Every other edge is *forward*, and the
    /// forward edges always form a graph with no cycle from the start. The
    /// run machine lets a loop edge re-enter a step but never lets one hold a
    /// join ([`Step::join`]); the canvas draws the same edges upward.
    /// Iterative, so no graph can overflow the stack.
    pub fn loop_edges(&self) -> BTreeSet<(StepId, StepId)> {
        #[derive(Clone, Copy, PartialEq, Eq)]
        enum Mark {
            Open,
            Done,
        }
        let mut marks: BTreeMap<&StepId, Mark> = BTreeMap::new();
        let mut out: BTreeSet<(StepId, StepId)> = BTreeSet::new();
        let successors_of = |id: &StepId| -> Vec<&StepId> {
            self.step(id).map(Self::successors).unwrap_or_default()
        };
        let roots: Vec<&StepId> = self
            .start_steps()
            .into_iter()
            .chain(self.steps.iter())
            .map(|s| &s.id)
            .collect();
        for root in roots {
            if marks.contains_key(root) {
                continue;
            }
            marks.insert(root, Mark::Open);
            let mut stack: Vec<(&StepId, Vec<&StepId>, usize)> =
                vec![(root, successors_of(root), 0)];
            while let Some(top) = stack.last_mut() {
                if top.2 < top.1.len() {
                    let next = top.1[top.2];
                    top.2 += 1;
                    match marks.get(next) {
                        Some(Mark::Open) => {
                            out.insert((top.0.clone(), next.clone()));
                        }
                        Some(Mark::Done) => {}
                        None => {
                            if self.step(next).is_some() {
                                marks.insert(next, Mark::Open);
                                let succ = successors_of(next);
                                stack.push((next, succ, 0));
                            }
                        }
                    }
                } else {
                    marks.insert(top.0, Mark::Done);
                    stack.pop();
                }
            }
        }
        out
    }

    /// Is `from → to` a loop edge? Convenience over [`Workflow::loop_edges`];
    /// a caller asking about every edge computes the set once.
    pub fn is_loop_edge(&self, from: &StepId, to: &StepId) -> bool {
        self.loop_edges().contains(&(from.clone(), to.clone()))
    }

    /// The flows into `id`: `(from, branch)`, over `then` only.
    pub fn incoming(&self, id: &StepId) -> Vec<(&Step, &Flow)> {
        self.steps
            .iter()
            .flat_map(|s| s.then.iter().filter(|f| &f.to == id).map(move |f| (s, f)))
            .collect()
    }

    /// Steps whose `on_fail` routes to `id`.
    pub fn fail_routes_into(&self, id: &StepId) -> Vec<&Step> {
        self.steps
            .iter()
            .filter(|s| matches!(&s.on_fail, OnFail::Then { step } if step == id))
            .collect()
    }

    /// The steps a loop kind re-runs per iteration: reachable from its
    /// body-branch flows over `then` and `on_fail: then` (never through the
    /// loop step itself) *and* able to reach the loop step again the same
    /// way. Empty for a plain step, or for a loop whose body never returns.
    pub fn loop_body(&self, id: &StepId) -> BTreeSet<StepId> {
        let Some(step) = self.step(id) else {
            return BTreeSet::new();
        };
        let Some((body, _)) = step.kind.loop_branches() else {
            return BTreeSet::new();
        };
        let heads: Vec<&StepId> = step
            .then
            .iter()
            .filter(|f| f.branch.as_ref() == Some(&body))
            .map(|f| &f.to)
            .collect();
        // Forward from the body flows, never through the loop step.
        let mut forward: BTreeSet<StepId> = BTreeSet::new();
        let mut queue: VecDeque<&StepId> = heads.into_iter().collect();
        while let Some(cur) = queue.pop_front() {
            if cur == id || !forward.insert(cur.clone()) {
                continue;
            }
            if let Some(s) = self.step(cur) {
                queue.extend(Self::successors(s));
            }
        }
        forward
            .into_iter()
            .filter(|s| self.returns_to(s, id))
            .collect()
    }

    /// Does `from` reach `target` over `then` and `on_fail: then`, without
    /// passing through `target` on the way? A visited set keeps a cycle
    /// elsewhere from spinning it.
    fn returns_to(&self, from: &StepId, target: &StepId) -> bool {
        let mut seen: BTreeSet<&StepId> = BTreeSet::new();
        let mut queue: VecDeque<&StepId> = VecDeque::from([from]);
        while let Some(cur) = queue.pop_front() {
            if !seen.insert(cur) {
                continue;
            }
            let Some(s) = self.step(cur) else {
                continue;
            };
            for next in Self::successors(s) {
                if next == target {
                    return true;
                }
                queue.push_back(next);
            }
        }
        false
    }

    /// Every step from which `id` is reachable over `then` flows, transitively.
    /// Cycles are fine: the walk carries a visited set.
    pub fn ancestors(&self, id: &StepId) -> BTreeSet<StepId> {
        Self::ancestors_in(&self.ways_in(), id)
    }

    /// Every way into each step, read once: the `then` flows and the fail
    /// routes — a fail route is a way in too: the step that failed runs
    /// before its remediation, and the remediation may read its outcome.
    /// A caller asking about every step builds this once; asked per step,
    /// the scan of every flow made validation cubic in the step count.
    fn ways_in(&self) -> BTreeMap<&StepId, Vec<&StepId>> {
        let mut out: BTreeMap<&StepId, Vec<&StepId>> = BTreeMap::new();
        for from in &self.steps {
            for flow in &from.then {
                out.entry(&flow.to).or_default().push(&from.id);
            }
            if let OnFail::Then { step: target } = &from.on_fail {
                out.entry(target).or_default().push(&from.id);
            }
        }
        out
    }

    /// [`Workflow::ancestors`] over an index already built ([`Workflow::ways_in`]).
    fn ancestors_in(ways_in: &BTreeMap<&StepId, Vec<&StepId>>, id: &StepId) -> BTreeSet<StepId> {
        let mut seen: BTreeSet<StepId> = BTreeSet::new();
        let mut queue: VecDeque<&StepId> = VecDeque::from([id]);
        while let Some(cur) = queue.pop_front() {
            for from in ways_in.get(cur).into_iter().flatten() {
                if seen.insert((*from).clone()) {
                    queue.push_back(from);
                }
            }
        }
        seen
    }

    /// The forward edges into each step — `then` flows and `on_fail: then`
    /// routes, minus the loop edges — the graph the run machine enters steps
    /// along on a first pass. Acyclic from the start by construction.
    fn forward_incoming(&self) -> BTreeMap<StepId, Vec<(StepId, FwdEdge)>> {
        let loops = self.loop_edges();
        let mut out: BTreeMap<StepId, Vec<(StepId, FwdEdge)>> = BTreeMap::new();
        for from in &self.steps {
            for f in &from.then {
                if loops.contains(&(from.id.clone(), f.to.clone())) {
                    continue;
                }
                let edge = if from.is_divert_flow(f) {
                    FwdEdge::Divert
                } else {
                    FwdEdge::Then {
                        labelled: f.branch.is_some(),
                    }
                };
                out.entry(f.to.clone())
                    .or_default()
                    .push((from.id.clone(), edge));
            }
            if let OnFail::Then { step: target } = &from.on_fail {
                if !loops.contains(&(from.id.clone(), target.clone())) {
                    out.entry(target.clone())
                        .or_default()
                        .push((from.id.clone(), FwdEdge::Fail));
                }
            }
        }
        out
    }

    /// Every step from which `id` is reached over forward edges alone — the
    /// ancestors a first pass has run through; a step reached only back
    /// along a loop is not among them.
    fn forward_ancestors(
        forward_in: &BTreeMap<StepId, Vec<(StepId, FwdEdge)>>,
        id: &StepId,
    ) -> BTreeSet<StepId> {
        let mut seen: BTreeSet<StepId> = BTreeSet::new();
        let mut queue: VecDeque<&StepId> = VecDeque::from([id]);
        while let Some(cur) = queue.pop_front() {
            for (from, _) in forward_in.get(cur).into_iter().flatten() {
                if seen.insert(from.clone()) {
                    queue.push_back(from);
                }
            }
        }
        seen
    }

    /// What every reachable step can count on at entry ([`Assurance`]), one
    /// pass in topological order over the forward edges:
    ///
    /// - an edge `u → s` contributes `u`'s own assurance plus `u` itself —
    ///   except to `present` when the edge may be taken after `u` failed
    ///   (`u`'s fail route; an unlabelled flow from a `skip`; any flow into
    ///   the `then` target), since a failed step has no output;
    /// - `s` counts on the **intersection** over its forward edges — what
    ///   every way in brings;
    /// - a step that joins with `all` or `one` waits for every forward edge,
    ///   so it also counts on each arm that is **sure** to arrive: an
    ///   unlabelled flow from a source that is entered whenever some step
    ///   already settled at `s` is — certainty relative to the join, which
    ///   is what lets a fan-in behind a `decide` read every arm. An `any`
    ///   join keeps the intersection: it enters on the first arrival.
    ///
    /// Every start is a root with nothing to count on. Empty only for an
    /// empty workflow; a step nothing reaches is absent (that problem is
    /// reported instead).
    pub fn assurance(&self) -> BTreeMap<StepId, Assurance> {
        let mut out: BTreeMap<StepId, Assurance> = BTreeMap::new();
        let starts: Vec<StepId> = self.start_steps().iter().map(|s| s.id.clone()).collect();
        if starts.is_empty() {
            return out;
        }
        let forward_in = self.forward_incoming();
        // The loop edges once: asked per edge, each answer walked the graph again.
        let loops = self.loop_edges();
        let is_loop = |from: &StepId, to: &StepId| loops.contains(&(from.clone(), to.clone()));
        // Kahn's walk from the start over the forward edges.
        let mut order: Vec<StepId> = Vec::new();
        let mut pending: BTreeMap<StepId, usize> = BTreeMap::new();
        {
            let mut seen: BTreeSet<StepId> = BTreeSet::new();
            let mut queue: VecDeque<StepId> = starts.into_iter().collect();
            while let Some(cur) = queue.pop_front() {
                if !seen.insert(cur.clone()) {
                    continue;
                }
                if let Some(step) = self.step(&cur) {
                    for next in Self::successors(step) {
                        if self.step(next).is_some() && !is_loop(&cur, next) {
                            queue.push_back(next.clone());
                        }
                    }
                } // LCOV_EXCL_LINE: the walk queues the ids of steps it found, so every id it pops is a step of the workflow
            }
            for id in &seen {
                let n = forward_in
                    .get(id)
                    .map(|edges| edges.iter().filter(|(from, _)| seen.contains(from)).count())
                    .unwrap_or(0);
                pending.insert(id.clone(), n);
            }
            let mut ready: VecDeque<StepId> = pending
                .iter()
                .filter(|(_, n)| **n == 0)
                .map(|(id, _)| id.clone())
                .collect();
            while let Some(cur) = ready.pop_front() {
                order.push(cur.clone());
                if let Some(step) = self.step(&cur) {
                    // Once per forward edge, as `pending` counted them: a step
                    // one source feeds twice — a flow and a divert, a flow and
                    // the fail route — is released by the second, not never.
                    let fail_route = match &step.on_fail {
                        OnFail::Then { step: target } => Some(target),
                        _ => None,
                    };
                    for next in step.then.iter().map(|f| &f.to).chain(fail_route) {
                        if !seen.contains(next) || is_loop(&cur, next) {
                            continue;
                        }
                        if let Some(n) = pending.get_mut(next) {
                            *n = n.saturating_sub(1);
                            if *n == 0 {
                                ready.push_back(next.clone());
                            }
                        } // LCOV_EXCL_LINE: `pending` counts every step the walk saw, so a successor it saw is in it
                    }
                } // LCOV_EXCL_LINE: the walk queues the ids of steps it found, so every id it pops is a step of the workflow
            }
        }
        let mut sure_cache: BTreeMap<StepId, BTreeSet<StepId>> = BTreeMap::new();
        for id in order {
            let Some(step) = self.step(&id) else {
                continue; // LCOV_EXCL_LINE: the walk queues the ids of steps it found, so every id it pops is a step of the workflow
            };
            let edges: Vec<(StepId, FwdEdge)> = forward_in
                .get(&id)
                .into_iter()
                .flatten()
                .filter(|(from, _)| out.contains_key(from))
                .cloned()
                .collect();
            if edges.is_empty() {
                out.insert(id, Assurance::default());
                continue;
            }
            let contrib_present = |from: &StepId, edge: FwdEdge| -> BTreeSet<StepId> {
                let mut set = out[from].present.clone();
                if !self.may_fail_in(from, edge, &id) {
                    set.insert(from.clone());
                }
                set
            };
            let contrib_settled = |from: &StepId| -> BTreeSet<StepId> {
                let mut set = out[from].settled.clone();
                set.insert(from.clone());
                set
            };
            let mut present: Option<BTreeSet<StepId>> = None;
            let mut settled: Option<BTreeSet<StepId>> = None;
            for (from, edge) in &edges {
                let p = contrib_present(from, *edge);
                let t = contrib_settled(from);
                present = Some(match present {
                    None => p,
                    Some(acc) => acc.intersection(&p).cloned().collect(),
                });
                settled = Some(match settled {
                    None => t,
                    Some(acc) => acc.intersection(&t).cloned().collect(),
                });
            }
            let mut present = present.unwrap_or_default();
            let mut settled = settled.unwrap_or_default();
            if matches!(step.join, Join::All | Join::One) {
                let basis = settled.clone();
                for (from, edge) in &edges {
                    if !self.unconditional(from, *edge, &id) {
                        continue;
                    }
                    let sure = self.sure_ancestors(&forward_in, from, &mut sure_cache);
                    if sure.intersection(&basis).next().is_some() {
                        present.extend(contrib_present(from, *edge));
                        settled.extend(contrib_settled(from));
                    }
                }
            }
            out.insert(id, Assurance { present, settled });
        }
        out
    }

    /// May the edge `from → to` be taken with `from` holding no output? It
    /// may after `from` failed — the edge is `from`'s fail route, an
    /// unlabelled flow out of a step that skips on failure, or any flow into
    /// the step `from` routes to on failure (`run.rs` `edge`) — and a divert
    /// flow is taken only when a boundary event stopped `from` short.
    fn may_fail_in(&self, from: &StepId, edge: FwdEdge, to: &StepId) -> bool {
        let Some(u) = self.step(from) else {
            return true;
        };
        match edge {
            FwdEdge::Fail | FwdEdge::Divert => true,
            FwdEdge::Then { labelled } => match &u.on_fail {
                OnFail::Fail => false,
                OnFail::Skip => !labelled,
                OnFail::Then { step } => step == to,
            },
        }
    }

    /// Is the edge `from → to` taken whenever `from` is entered? An
    /// unlabelled flow, out of a step that routes nowhere else on failure and
    /// that no boundary event can divert.
    fn unconditional(&self, from: &StepId, edge: FwdEdge, to: &StepId) -> bool {
        matches!(edge, FwdEdge::Then { labelled: false })
            && !self.step(from).is_some_and(|u| {
                u.has_diverts() || matches!(&u.on_fail, OnFail::Then { step: t } if t != to)
            })
    }

    /// Whether an `approval` or a `human` step stands before `id` on its
    /// normal flows: a gate whose divert flow leads to `id` did not gate it —
    /// a timeout is nobody's approval.
    fn gated(&self, id: &StepId) -> bool {
        self.steps
            .iter()
            .filter(|g| matches!(g.kind, StepKind::Approval { .. } | StepKind::Human { .. }))
            .any(|g| {
                let mut seen: BTreeSet<&StepId> = BTreeSet::new();
                let mut queue: VecDeque<&StepId> = g
                    .then
                    .iter()
                    .filter(|f| !g.is_divert_flow(f))
                    .map(|f| &f.to)
                    .collect();
                while let Some(cur) = queue.pop_front() {
                    if cur == id {
                        return true;
                    }
                    if !seen.insert(cur) {
                        continue;
                    }
                    if let Some(s) = self.step(cur) {
                        queue.extend(Self::successors(s));
                    }
                }
                false
            })
    }

    /// `id` and every step from which `id` is reached over unconditional
    /// forward edges — unlabelled flows whose source does not route
    /// elsewhere on failure: entered whenever the far end is.
    fn sure_ancestors(
        &self,
        forward_in: &BTreeMap<StepId, Vec<(StepId, FwdEdge)>>,
        id: &StepId,
        cache: &mut BTreeMap<StepId, BTreeSet<StepId>>,
    ) -> BTreeSet<StepId> {
        if let Some(found) = cache.get(id) {
            return found.clone();
        }
        let mut seen: BTreeSet<StepId> = BTreeSet::from([id.clone()]);
        let mut queue: VecDeque<StepId> = VecDeque::from([id.clone()]);
        while let Some(cur) = queue.pop_front() {
            for (from, edge) in forward_in.get(&cur).into_iter().flatten() {
                if self.unconditional(from, *edge, &cur) && seen.insert(from.clone()) {
                    queue.push_back(from.clone());
                }
            }
        }
        cache.insert(id.clone(), seen.clone());
        seen
    }

    /// The two sides of a loop kind: the steps its body flows reach without
    /// passing through the loop step (where `{item}` is written), and the
    /// steps its exit flows reach. A plain step has neither.
    pub fn loop_sides(&self, id: &StepId) -> (BTreeSet<StepId>, BTreeSet<StepId>) {
        let Some(step) = self.step(id) else {
            return (BTreeSet::new(), BTreeSet::new());
        };
        let Some((body, exit)) = step.kind.loop_branches() else {
            return (BTreeSet::new(), BTreeSet::new());
        };
        let reach = |branch: &Branch| -> BTreeSet<StepId> {
            let mut seen: BTreeSet<StepId> = BTreeSet::new();
            let mut queue: VecDeque<&StepId> = step
                .then
                .iter()
                .filter(|f| f.branch.as_ref() == Some(branch))
                .map(|f| &f.to)
                .collect();
            while let Some(cur) = queue.pop_front() {
                if cur == id || !seen.insert(cur.clone()) {
                    continue;
                }
                if let Some(s) = self.step(cur) {
                    queue.extend(Self::successors(s));
                }
            }
            seen
        };
        (reach(&body), reach(&exit))
    }

    /// The problems one reference to another step's output, answer or
    /// outcome has, first only: the reading step, the step read, and what is
    /// read. In order — a step it is not downstream of, a kind that has no
    /// such thing, a step not sure to have run by then, a field the producer
    /// does not promise.
    fn reference_problems(
        &self,
        reader: &Step,
        target: &StepId,
        read: Reference<'_>,
        graph: &ReferenceGraph<'_>,
    ) -> Option<Problem> {
        let problem = |kind: ProblemKind, text: crate::text::Text| {
            Some(Problem {
                step: Some(reader.id.clone()),
                kind,
                text,
            })
        };
        let Some(producer) = self.step(target) else {
            return None; // UnknownStep is reported where the reference is parsed
        };
        let what = read.what();
        if target == &reader.id || !graph.ancestors.contains(target) {
            return problem(
                ProblemKind::NotUpstream,
                crate::text!(
                    "problem-not-upstream-step-which-does-not-run-before",
                    a0 = (reader.id).to_string(),
                    what = what.to_string(),
                    target = target.to_string()
                ),
            );
        }
        let kind = producer.kind.as_str();
        let promised = producer.kind.promised();
        match read {
            Reference::AnsweredRule => {
                if !matches!(producer.kind, StepKind::Human { .. }) {
                    return problem(
                        ProblemKind::NotUpstream,
                        crate::text!(
                            "problem-not-upstream-step-which-not-one-human",
                            a0 = (reader.id).to_string(),
                            what = what.to_string(),
                            target = target.to_string(),
                            kind = kind.to_string()
                        ),
                    );
                }
            }
            Reference::OutcomeRule => {
                if !matches!(
                    producer.kind,
                    StepKind::Check { .. } | StepKind::Approval { .. }
                ) {
                    return problem(
                        ProblemKind::NotUpstream,
                        crate::text!(
                            "problem-not-upstream-step-which-not-one-check-approval",
                            a0 = (reader.id).to_string(),
                            what = what.to_string(),
                            target = target.to_string(),
                            kind = kind.to_string()
                        ),
                    );
                }
            }
            Reference::AnswerText => {
                if !matches!(producer.kind, StepKind::Human { .. }) {
                    return problem(
                        ProblemKind::NoSuchOutput,
                        crate::text!(
                            "problem-no-such-output-step-reads-answer-which-only-human",
                            a0 = (reader.id).to_string(),
                            target = target.to_string(),
                            kind = kind.to_string()
                        ),
                    );
                }
            }
            Reference::OutputText { .. }
            | Reference::OutputRule { .. }
            | Reference::SchemaCheck => match promised {
                Promised::Nothing => {
                    return problem(
                        ProblemKind::NoSuchOutput,
                        crate::text!(
                            "problem-no-such-output-step-which-yields-none",
                            a0 = (reader.id).to_string(),
                            what = what.to_string(),
                            target = target.to_string(),
                            kind = kind.to_string()
                        ),
                    )
                }
                Promised::Answer => {
                    return problem(
                        ProblemKind::NoSuchOutput,
                        crate::text!(
                            "problem-no-such-output-step-which-human-yields-answer-not",
                            a0 = (reader.id).to_string(),
                            what = what.to_string(),
                            target = target.to_string()
                        ),
                    )
                }
                _ => {}
            },
        }
        let assured = graph.assurance.get(&reader.id);
        let sure = !read.needs_present() || assured.is_some_and(|a| a.present.contains(target));
        if !sure {
            let why = if !graph.forward_ancestors.contains(target) {
                format!(
                    "which only runs after it: on the first pass of the loop `{target}` has nothing yet — give the second pass its own step, or read `{target}` where it is sure to have run"
                )
            } else if assured.is_some_and(|a| a.settled.contains(target)) {
                "which may have failed and been passed over by then (`on_fail` skip or then); a failed step has no output".to_string()
            } else {
                format!(
                    "which not every way in has run: a branch, an `any` join or a fail route reaches `{}` without it",
                    reader.id
                )
            };
            return problem(
                ProblemKind::NotAssured,
                crate::text!(
                    "problem-not-assured-step",
                    a0 = (reader.id).to_string(),
                    what = what.to_string(),
                    target = target.to_string(),
                    why = why.to_string()
                ),
            );
        }
        let path = read.path()?;
        let field = path.split('.').next().unwrap_or_default();
        if field.is_empty() {
            return None; // LCOV_EXCL_LINE: `Reference::path` is `None` for an empty rule path, and the template grammar refuses a dotted path that ends in nothing
        }
        let promised_fields = match promised {
            Promised::Keys(_) if matches!(producer.kind, StepKind::ForEach { .. }) => {
                let (body, _) = self.loop_sides(target);
                if body.contains(&reader.id) {
                    Some(vec![
                        "item".to_string(),
                        "index".to_string(),
                        "count".to_string(),
                    ])
                } else {
                    Some(vec!["index".to_string(), "count".to_string()])
                }
            }
            other => other.fields(),
        };
        let Some(fields) = promised_fields else {
            return None; // a payload: any path
        };
        if fields.iter().any(|f| f == field) {
            return None;
        }
        let text = match promised {
            Promised::Schema(None) => crate::text!(
                "problem-unpromised-output-step-reads-s-output-but-declares",
                a0 = (reader.id).to_string(),
                target = target.to_string(),
                field = field.to_string()
            ),
            Promised::Schema(Some(_)) => crate::text!(
                "problem-unpromised-output-step-reads-s-output-which-s",
                a0 = (reader.id).to_string(),
                target = target.to_string(),
                field = field.to_string()
            ),
            _ => crate::text!(
                "problem-unpromised-output-step-reads-s-output-step-s",
                a0 = (reader.id).to_string(),
                target = target.to_string(),
                field = field.to_string(),
                kind = kind.to_string(),
                a1 = (fields
                    .iter()
                    .map(|f| format!("`{f}`"))
                    .collect::<Vec<_>>()
                    .join(", "))
                .to_string()
            ),
        };
        problem(ProblemKind::UnpromisedOutput, text)
    }

    /// Every problem, never the first.
    pub fn validate(&self, ctx: &ValidationCtx<'_>) -> Vec<Problem> {
        let mut problems: Vec<Problem> = Vec::new();
        let mut push = |step: Option<&StepId>, kind: ProblemKind, text: crate::text::Text| {
            problems.push(Problem {
                step: step.cloned(),
                kind,
                text,
            });
        };

        if self.name.trim().is_empty() {
            push(
                None,
                ProblemKind::EmptyName,
                crate::text!("problem-empty-name-workflow-needs-name"),
            );
        }

        // Inputs: unique names, and an account input names its connector.
        let mut seen_inputs: BTreeSet<&InputName> = BTreeSet::new();
        for input in &self.inputs {
            if !seen_inputs.insert(&input.name) {
                push(
                    None,
                    ProblemKind::DuplicateInput,
                    crate::text!(
                        "problem-duplicate-input-input-declared-twice",
                        a0 = (input.name).to_string()
                    ),
                );
            }
            if let InputKind::Account { connector: None } = input.kind {
                push(
                    None,
                    ProblemKind::Unfilled,
                    crate::text!(
                        "problem-unfilled-input-account-but-which-connector-not",
                        a0 = (input.name).to_string()
                    ),
                );
            }
        }

        // Steps: unique ids.
        let mut seen_steps: BTreeSet<&StepId> = BTreeSet::new();
        for step in &self.steps {
            if !seen_steps.insert(&step.id) {
                push(
                    Some(&step.id),
                    ProblemKind::DuplicateStepId,
                    crate::text!(
                        "problem-duplicate-step-id-step-declared-twice",
                        a0 = (step.id).to_string()
                    ),
                );
            }
            if step.name.trim().is_empty() {
                push(
                    Some(&step.id),
                    ProblemKind::EmptyName,
                    crate::text!(
                        "problem-empty-name-step-needs-name",
                        a0 = (step.id).to_string()
                    ),
                );
            }
        }
        let known = |id: &StepId| seen_steps.contains(id);

        if self.steps.is_empty() {
            push(
                None,
                ProblemKind::NoStart,
                crate::text!("problem-no-start-workflow-needs-least-one-step"),
            );
            return problems;
        }

        // References resolve; no self flow.
        for step in &self.steps {
            for flow in &step.then {
                if flow.to == step.id {
                    push(
                        Some(&step.id),
                        ProblemKind::SelfFlow,
                        crate::text!(
                            "problem-self-flow-step-flows-itself",
                            a0 = (step.id).to_string()
                        ),
                    );
                } else if !known(&flow.to) {
                    push(
                        Some(&step.id),
                        ProblemKind::UnknownStep,
                        crate::text!(
                            "problem-unknown-step-step-flows-which-does-not-exist",
                            a0 = (step.id).to_string(),
                            a1 = (flow.to).to_string()
                        ),
                    );
                }
            }
            if let OnFail::Then { step: target } = &step.on_fail {
                if target == &step.id {
                    push(
                        Some(&step.id),
                        ProblemKind::SelfFlow,
                        crate::text!(
                            "problem-self-flow-step-routes-own-failure-itself",
                            a0 = (step.id).to_string()
                        ),
                    );
                } else if !known(target) {
                    push(
                        Some(&step.id),
                        ProblemKind::UnknownStep,
                        crate::text!(
                            "problem-unknown-step-step-routes-failure-which-does-not",
                            a0 = (step.id).to_string(),
                            a1 = (target).to_string()
                        ),
                    );
                }
            }
            if let StepKind::Check {
                check: CheckKind::Schema { of: Some(of), .. },
            } = &step.kind
            {
                if !known(of) {
                    push(
                        Some(&step.id),
                        ProblemKind::UnknownStep,
                        crate::text!(
                            "problem-unknown-step-step-checks-output-which-does-not",
                            a0 = (step.id).to_string(),
                            of = of.to_string()
                        ),
                    );
                }
            }
            for c in step.kind.conditions() {
                for s in c.reads_steps() {
                    if !known(s) {
                        push(
                            Some(&step.id),
                            ProblemKind::UnknownStep,
                            crate::text!(
                                "problem-unknown-step-step-reads-which-does-not-exist",
                                a0 = (step.id).to_string(),
                                s = s.to_string()
                            ),
                        );
                    }
                }
            }
        }

        // The ways in. Named starts are each a root, and at most one begins
        // by hand. A workflow that names none begins at its one root, as it
        // always did: a loop back to the first step is not a missing start —
        // `start_steps` names the first step then — so no step count above
        // zero reaches the `NoStart` arm; it stays for the type's sake.
        let starts = self.start_steps();
        if self.has_starts() {
            let manual: Vec<&&Step> = starts
                .iter()
                .filter(|s| s.start_on().is_some_and(StartOn::is_manual))
                .collect();
            if manual.len() > 1 {
                for s in &manual {
                    push(
                        Some(&s.id),
                        ProblemKind::ManyManualStarts,
                        crate::text!(
                            "problem-many-manual-starts",
                            a0 = (s.id).to_string(),
                            n = manual.len()
                        ),
                    );
                }
            }
            for s in &starts {
                let into = self
                    .incoming(&s.id)
                    .into_iter()
                    .map(|(from, _)| &from.id)
                    .chain(self.fail_routes_into(&s.id).into_iter().map(|f| &f.id))
                    .next();
                if let Some(from) = into {
                    push(
                        Some(&s.id),
                        ProblemKind::StartHasIncoming,
                        crate::text!(
                            "problem-start-has-incoming",
                            a0 = (s.id).to_string(),
                            from = from.to_string()
                        ),
                    );
                }
            }
        } else {
            match starts.len() {
                // LCOV_EXCL_START: `start_steps` names the first step when no step is a start, so the count is never zero; the arm stays for the match's sake
                0 => push(
                    None,
                    ProblemKind::NoStart,
                    crate::text!("problem-no-start-workflow-needs-least-one-step"),
                ),
                // LCOV_EXCL_STOP
                1 => {}
                _ => {
                    for s in &starts {
                        push(
                            Some(&s.id),
                            ProblemKind::ManyStarts,
                            crate::text!(
                                "problem-many-starts-step-has-no-incoming-flow-so",
                                a0 = (s.id).to_string()
                            ),
                        );
                    }
                }
            }
        }

        // Reachability from the start(s), over `then` and `on_fail: then`.
        {
            let mut reached: BTreeSet<&StepId> = BTreeSet::new();
            let mut queue: VecDeque<&Step> = starts.iter().copied().collect();
            for s in &queue {
                reached.insert(&s.id);
            }
            while let Some(cur) = queue.pop_front() {
                let mut nexts: Vec<&StepId> = cur.then.iter().map(|f| &f.to).collect();
                if let OnFail::Then { step } = &cur.on_fail {
                    nexts.push(step);
                }
                for id in nexts {
                    if let Some(next) = self.step(id) {
                        if reached.insert(&next.id) {
                            queue.push_back(next);
                        }
                    }
                }
            }
            for step in &self.steps {
                if !reached.contains(&step.id) {
                    push(
                        Some(&step.id),
                        ProblemKind::Unreachable,
                        crate::text!(
                            "problem-unreachable-step-cannot-be-reached-from-start",
                            a0 = (step.id).to_string()
                        ),
                    );
                }
            }
        }

        // Every input something reads, so a declared one nothing reads can
        // be reported at the end.
        let mut used_inputs: BTreeSet<InputName> = BTreeSet::new();

        // Per-step rules.
        let assurance = self.assurance();
        let forward_in = self.forward_incoming();
        let ways_in = self.ways_in();
        for step in &self.steps {
            // A divert's flows carry its name; every other label is a
            // gateway's branch.
            let diverts = step.divert_names();
            let labelled: Vec<&Branch> = step
                .then
                .iter()
                .filter_map(|f| f.branch.as_ref())
                .filter(|b| !diverts.contains(b))
                .collect();
            let unlabelled = step.then.iter().filter(|f| f.branch.is_none()).count();
            // A branching kind's flows and the branches it can choose are the
            // same set, one to one; a plain kind's flows carry no label.
            let kind_word = step.kind.as_str();
            match step.kind.branches() {
                Some(list) => {
                    if unlabelled > 0 {
                        push(Some(&step.id), ProblemKind::UnlabelledFlowOnDecide, crate::text!("problem-unlabelled-flow-on-decide-every-flow-out-step-names-branch", kind_word = kind_word.to_string(), a0 = (step.id).to_string()));
                    }
                    let mut branches: BTreeSet<&Branch> = BTreeSet::new();
                    for b in &list {
                        if !branches.insert(b) {
                            push(
                                Some(&step.id),
                                ProblemKind::DuplicateBranch,
                                crate::text!(
                                    "problem-duplicate-branch-step-chooses-twice",
                                    kind_word = kind_word.to_string(),
                                    a0 = (step.id).to_string(),
                                    b = b.to_string()
                                ),
                            );
                        }
                    }
                    if let StepKind::Switch { cases, .. } = &step.kind {
                        let mut values: BTreeSet<&str> = BTreeSet::new();
                        for c in cases {
                            if !values.insert(c.value.as_str()) {
                                push(
                                    Some(&step.id),
                                    ProblemKind::DuplicateBranch,
                                    crate::text!(
                                        "problem-duplicate-branch-switch-step-has-two-cases-value",
                                        a0 = (step.id).to_string(),
                                        a1 = (c.value).to_string()
                                    ),
                                );
                            }
                        }
                    }
                    for b in &branches {
                        if !labelled.contains(b) {
                            push(
                                Some(&step.id),
                                ProblemKind::RuleWithoutFlow,
                                crate::text!(
                                    "problem-rule-without-flow-step-can-choose-but-no-flow",
                                    kind_word = kind_word.to_string(),
                                    a0 = (step.id).to_string(),
                                    b = b.to_string()
                                ),
                            );
                        }
                    }
                    for b in &labelled {
                        if !branches.contains(b) {
                            push(Some(&step.id), ProblemKind::BranchWithoutRule, crate::text!("problem-branch-without-rule-flow-out-step-labelled-which-never", kind_word = kind_word.to_string(), a0 = (step.id).to_string(), b = b.to_string()));
                        }
                    }
                }
                None => {
                    if !labelled.is_empty() {
                        push(Some(&step.id), ProblemKind::LabelledFlowOnPlainStep, crate::text!("problem-labelled-flow-on-plain-step-only-branching-step-decide-if-switch", a0 = (step.id).to_string(), kind_word = kind_word.to_string()));
                    }
                }
            }
            for p in self.boundary_problems(step) {
                push(p.step.as_ref(), p.kind, p.text);
            }
            if let StepKind::Start {
                on,
                inputs: mapping,
                guard,
            } = &step.kind
            {
                for p in self.start_problems(step, on, mapping, guard, ctx, &mut used_inputs) {
                    push(p.step.as_ref(), p.kind, p.text);
                }
            }
            // A loop's body must come back, and its exit must not.
            if let Some((body, exit)) = step.kind.loop_branches() {
                let has_body_flow = step.then.iter().any(|f| f.branch.as_ref() == Some(&body));
                if has_body_flow && self.loop_body(&step.id).is_empty() {
                    push(
                        Some(&step.id),
                        ProblemKind::LoopWithoutReturn,
                        crate::text!(
                            "problem-loop-without-return-step-nothing-side-flows-back-into",
                            kind_word = kind_word.to_string(),
                            a0 = (step.id).to_string(),
                            body = body.to_string()
                        ),
                    );
                }
                for f in step
                    .then
                    .iter()
                    .filter(|f| f.branch.as_ref() == Some(&exit))
                {
                    if known(&f.to) && self.returns_to(&f.to, &step.id) {
                        push(
                            Some(&step.id),
                            ProblemKind::LoopExitReturns,
                            crate::text!(
                                "problem-loop-exit-returns-step-flow-leads-which-flows-back",
                                kind_word = kind_word.to_string(),
                                a0 = (step.id).to_string(),
                                exit = exit.to_string(),
                                a1 = (f.to).to_string()
                            ),
                        );
                    }
                }
                let max_iterations = match &step.kind {
                    StepKind::ForEach { max_iterations, .. }
                    | StepKind::While { max_iterations, .. } => *max_iterations,
                    _ => 1, // LCOV_EXCL_LINE: `loop_branches` is `Some` for `for_each` and `loop` alone; the arm keeps the match total
                };
                if max_iterations == 0 {
                    push(
                        Some(&step.id),
                        ProblemKind::ZeroIterations,
                        crate::text!(
                            "problem-zero-iterations-step-allows-zero-iterations-least-one",
                            kind_word = kind_word.to_string(),
                            a0 = (step.id).to_string()
                        ),
                    );
                }
            }
            // A condition reads on a card and runs on a bounded stack.
            for c in step.kind.conditions() {
                if c.empty_combinator() {
                    push(
                        Some(&step.id),
                        ProblemKind::BadCondition,
                        crate::text!(
                            "problem-bad-condition-step-all-any-one-condition-needs",
                            a0 = (step.id).to_string()
                        ),
                    );
                }
                let depth = c.depth();
                if depth > Condition::MAX_DEPTH {
                    push(
                        Some(&step.id),
                        ProblemKind::BadCondition,
                        crate::text!(
                            "problem-bad-condition-step-conditions-nest-deep-most-allowed",
                            a0 = (step.id).to_string(),
                            depth = depth.to_string(),
                            a1 = (Condition::MAX_DEPTH).to_string()
                        ),
                    );
                }
            }
            if matches!(step.kind, StepKind::End { .. }) && !step.then.is_empty() {
                push(
                    Some(&step.id),
                    ProblemKind::EndWithSuccessors,
                    crate::text!(
                        "problem-end-with-successors-end-step-has-flows-out-end",
                        a0 = (step.id).to_string()
                    ),
                );
            }
            if step.max_visits == 0 {
                push(
                    Some(&step.id),
                    ProblemKind::ZeroVisits,
                    crate::text!(
                        "problem-zero-visits-step-allows-zero-visits-least-one",
                        a0 = (step.id).to_string()
                    ),
                );
            }
            if let StepKind::Human { options, multi, .. } = &step.kind {
                let kind = crate::ask::AskKind::Answer {
                    options: options.clone(),
                    multi: *multi,
                };
                if let Err(e) = kind.validate() {
                    push(
                        Some(&step.id),
                        ProblemKind::BadQuestion,
                        crate::text!(
                            "problem-bad-question-step",
                            a0 = (step.id).to_string(),
                            e = e.to_string()
                        ),
                    );
                }
            }

            // Upstream references: a condition, a schema check or a placeholder
            // reads only what is sure to be there when this step is entered.
            let ancestors = Self::ancestors_in(&ways_in, &step.id);
            let forward_ancestors = Self::forward_ancestors(&forward_in, &step.id);
            let graph = ReferenceGraph {
                assurance: &assurance,
                ancestors: &ancestors,
                forward_ancestors: &forward_ancestors,
            };
            let mut upstream_problems: Vec<Problem> = Vec::new();
            match &step.kind {
                StepKind::Check {
                    check: CheckKind::Schema { of: Some(of), .. },
                } => upstream_problems.extend(self.reference_problems(
                    step,
                    of,
                    Reference::SchemaCheck,
                    &graph,
                )),
                StepKind::Check {
                    check: CheckKind::Schema { of: None, .. },
                } => push(
                    Some(&step.id),
                    ProblemKind::Unfilled,
                    crate::text!(
                        "problem-unfilled-step-checks-no-step-s-output",
                        a0 = (step.id).to_string()
                    ),
                ),
                _ => {}
            }
            for c in step.kind.conditions() {
                for leaf in c.leaves() {
                    let found = match leaf {
                        Condition::Answered { step: s, .. } => {
                            self.reference_problems(step, s, Reference::AnsweredRule, &graph)
                        }
                        Condition::Outcome { step: s, .. } => {
                            self.reference_problems(step, s, Reference::OutcomeRule, &graph)
                        }
                        Condition::OutputEquals { step: s, path, .. }
                        | Condition::OutputMatches { step: s, path, .. } => {
                            self.reference_problems(step, s, Reference::OutputRule { path }, &graph)
                        }
                        _ => None,
                    };
                    upstream_problems.extend(found);
                }
            }
            for p in upstream_problems {
                push(p.step.as_ref(), p.kind, p.text);
            }

            // Templates: the kind's and its boundary events', all rendered
            // against the run.
            for tmpl in step.templates() {
                match template::placeholders(tmpl) {
                    // The event is read by a start's mapping alone; the
                    // refusal says where to read it.
                    Err(TemplateError::EventOutsideMapping(key)) => push(Some(&step.id), ProblemKind::StartPlaceholder, crate::text!("problem-start-placeholder-event-outside-mapping", a0 = (step.id).to_string(), key = key.to_string())),
                    // The step grammar has no `params.`/`account.` root: those
                    // are a connector definition's, and the refusal says so.
                    Err(TemplateError::Unknown(key))
                        if key.starts_with("params.") || key.starts_with("account.") =>
                    {
                        push(Some(&step.id), ProblemKind::ParamOnlyPlaceholder, crate::text!("problem-param-only-placeholder-step-connector-definition-s-step-reads", a0 = (step.id).to_string()))
                    }
                    Err(TemplateError::Unknown(key)) => push(Some(&step.id), ProblemKind::UnknownPlaceholder, crate::text!("problem-unknown-placeholder-step-not-placeholder-platform-knows-write", a0 = (step.id).to_string(), key = key.to_string())),
                    Err(e) => push(Some(&step.id), ProblemKind::BadTemplate, crate::text!("problem-bad-question-step", a0 = (step.id).to_string(), e = e.to_string())),
                    Ok(found) => {
                        for p in found {
                            match p {
                                Placeholder::Input(name) => {
                                    used_inputs.insert(name.clone());
                                    if self.input(&name).is_none() {
                                        push(Some(&step.id), ProblemKind::UnknownInput, crate::text!("problem-unknown-input-step-uses-input-which-not-declared", a0 = (step.id).to_string(), name = name.to_string()));
                                    }
                                }
                                Placeholder::StepOutput { step: s, path } => {
                                    if !known(&s) {
                                        push(Some(&step.id), ProblemKind::UnknownStep, crate::text!("problem-unknown-step-step-names-template-which-does-not", a0 = (step.id).to_string(), s = s.to_string()));
                                    } else if let Some(p) = self.reference_problems(
                                        step,
                                        &s,
                                        Reference::OutputText { path: path.as_deref() },
                                        &graph,
                                    ) {
                                        push(p.step.as_ref(), p.kind, p.text);
                                    }
                                }
                                Placeholder::StepAnswer { step: s } => {
                                    if !known(&s) {
                                        push(Some(&step.id), ProblemKind::UnknownStep, crate::text!("problem-unknown-step-step-names-template-which-does-not", a0 = (step.id).to_string(), s = s.to_string()));
                                    } else if let Some(p) =
                                        self.reference_problems(step, &s, Reference::AnswerText, &graph)
                                    {
                                        push(p.step.as_ref(), p.kind, p.text);
                                    }
                                }
                                // A switch subject or a loop's items render
                                // inside the run machine, which reads no goal.
                                Placeholder::GoalStatement | Placeholder::GoalTitle
                                    if matches!(
                                        step.kind,
                                        StepKind::Switch { .. } | StepKind::ForEach { .. }
                                    ) =>
                                {
                                    push(Some(&step.id), ProblemKind::BadTemplate, crate::text!("problem-bad-template-step-cannot-drive-machine-reads-no", a0 = (step.id).to_string(), kind_word = kind_word.to_string()));
                                }
                                // The run grammar never yields the event.
                                Placeholder::Event { .. }
                                | Placeholder::GoalStatement
                                | Placeholder::GoalTitle => {}
                                Placeholder::Param(_) | Placeholder::Account(_) => push(Some(&step.id), ProblemKind::ParamOnlyPlaceholder, crate::text!("problem-param-only-placeholder-step-connector-definition-s-step-reads", a0 = (step.id).to_string())), // LCOV_EXCL_LINE: the run grammar refuses `params.` and `account.` before it yields a placeholder (the `Err` arm above says so)
                            }
                        }
                    }
                }
            }

            // Value references and the things they name.
            let check_input_ref = |name: &InputName, want: &str| -> Option<Problem> {
                match self.input(name) {
                    None => Some(Problem {
                        step: Some(step.id.clone()),
                        kind: ProblemKind::UnknownInput,
                        text: crate::text!(
                            "problem-unknown-input-step-reads-input-which-not-declared",
                            a0 = (step.id).to_string(),
                            name = name.to_string()
                        ),
                    }),
                    Some(def) if def.kind.as_str() != want => Some(Problem {
                        step: Some(step.id.clone()),
                        kind: ProblemKind::InputKindMismatch,
                        text: crate::text!(
                            "problem-input-kind-mismatch-step-reads-input-as-but",
                            a0 = (step.id).to_string(),
                            name = name.to_string(),
                            want = want.to_string(),
                            a1 = (def.kind.as_str()).to_string()
                        ),
                    }),
                    Some(_) => None,
                }
            };
            for (name, want) in step.input_refs() {
                used_inputs.insert(name.clone());
                if let Some(p) = check_input_ref(name, want) {
                    push(p.step.as_ref(), p.kind, p.text);
                }
            }
            for r in step.assignee_refs() {
                if let ValueRef::Fixed(a) = r {
                    if !ctx.assignees.contains(a) {
                        push(
                            Some(&step.id),
                            ProblemKind::UnknownAssignee,
                            crate::text!(
                                "problem-unknown-assignee-step-names-who-not-workspace",
                                a0 = (step.id).to_string(),
                                a = a.to_string()
                            ),
                        );
                    }
                }
            }
            if let StepKind::Agent {
                project: Some(ValueRef::Fixed(id)),
                ..
            } = &step.kind
            {
                if !ctx.projects.contains(id) {
                    push(
                        Some(&step.id),
                        ProblemKind::UnknownProject,
                        crate::text!(
                            "problem-unknown-project-step-names-project-which-not-workspace",
                            a0 = (step.id).to_string(),
                            id = id.to_string()
                        ),
                    );
                }
            }
            for c in step.kind.conditions() {
                for name in c.reads_inputs() {
                    used_inputs.insert(name.clone());
                    if self.input(name).is_none() {
                        push(
                            Some(&step.id),
                            ProblemKind::UnknownInput,
                            crate::text!(
                                "problem-unknown-input-step-tests-input-which-not-declared",
                                a0 = (step.id).to_string(),
                                name = name.to_string()
                            ),
                        );
                    }
                }
            }
            if let StepKind::Spawn {
                workflow: Some(w), ..
            } = &step.kind
            {
                if !ctx.workflows.contains(w) {
                    push(
                        Some(&step.id),
                        ProblemKind::UnknownWorkflow,
                        crate::text!(
                            "problem-unknown-workflow-step-spawns-workflow-which-not-workspace",
                            a0 = (step.id).to_string(),
                            w = w.to_string()
                        ),
                    );
                } else if spawn_reaches(ctx.spawns, *w, self.id) {
                    push(
                        Some(&step.id),
                        ProblemKind::SpawnCycle,
                        crate::text!(
                            "problem-spawn-cycle-step-spawns-workflow-which-spawns-workflow",
                            a0 = (step.id).to_string(),
                            w = w.to_string()
                        ),
                    );
                } else if ctx.event_only.contains(w) {
                    push(
                        Some(&step.id),
                        ProblemKind::SpawnNeedsManualEntry,
                        crate::text!(
                            "problem-spawn-needs-manual-entry",
                            a0 = (step.id).to_string(),
                            w = w.to_string()
                        ),
                    );
                }
            }
            // A spawn is a start by hand made by a step: it is held to what
            // its workflow asks, as a person's start is.
            if let StepKind::Spawn {
                workflow: Some(w),
                inputs: given,
                ..
            } = &step.kind
            {
                if let Some((_, asked)) = ctx.asks.iter().find(|(id, _)| id == w) {
                    for name in given.keys() {
                        if !asked.iter().any(|def| def.name.as_str() == name) {
                            push(
                                Some(&step.id),
                                ProblemKind::SpawnInput,
                                crate::text!(
                                    "problem-spawn-input-unknown",
                                    a0 = (step.id).to_string(),
                                    name = name.to_string(),
                                    w = w.to_string()
                                ),
                            );
                        }
                    }
                    for def in asked {
                        let filled = def.default.is_some() || given.contains_key(def.name.as_str());
                        if def.required && !filled {
                            push(
                                Some(&step.id),
                                ProblemKind::SpawnInput,
                                crate::text!(
                                    "problem-spawn-input-missing",
                                    a0 = (step.id).to_string(),
                                    name = def.name.to_string(),
                                    w = w.to_string()
                                ),
                            );
                        }
                    }
                }
            }

            // The kind's own syntax and the runtime it names.
            match &step.kind {
                StepKind::Decide { rules, .. } if rules.is_empty() => push(
                    Some(&step.id),
                    ProblemKind::EmptyRules,
                    crate::text!(
                        "problem-empty-rules-decide-step-has-no-rules-so",
                        a0 = (step.id).to_string()
                    ),
                ),
                StepKind::Switch { cases, .. } if cases.is_empty() => push(
                    Some(&step.id),
                    ProblemKind::EmptyRules,
                    crate::text!(
                        "problem-empty-rules-switch-step-has-no-cases-so",
                        a0 = (step.id).to_string()
                    ),
                ),
                StepKind::Judge { options, .. } if options.len() < 2 => push(
                    Some(&step.id),
                    ProblemKind::EmptyRules,
                    crate::text!(
                        "problem-empty-rules-judge-step-offers-option-s-judgement",
                        a0 = (step.id).to_string(),
                        a1 = (options.len()).to_string()
                    ),
                ),
                StepKind::Judge {
                    min_confidence: Some(c),
                    ..
                } if !(0.0..=1.0).contains(c) => push(
                    Some(&step.id),
                    ProblemKind::BadQuestion,
                    crate::text!(
                        "problem-bad-question-judge-step-asks-confidence-number-from",
                        a0 = (step.id).to_string(),
                        c = c.to_string()
                    ),
                ),
                StepKind::Agent {
                    harness,
                    model,
                    effort,
                    output_schema,
                    ..
                } => {
                    if !ctx.harnesses.is_empty() {
                        for h in harness {
                            if !ctx.harnesses.contains(h) {
                                push(Some(&step.id), ProblemKind::UnknownHarness, crate::text!("problem-unknown-harness-step-names-harness-which-runtime-cannot", a0 = (step.id).to_string(), h = h.to_string(), a1 = (ctx.harnesses.join(", ")).to_string()));
                            }
                        }
                    }
                    if let Some(m) = model {
                        for h in harness {
                            let Some((_, listed)) = ctx.models.iter().find(|(name, _)| name == h)
                            else {
                                continue; // no list cached for this harness: not checked
                            };
                            if !listed.iter().any(|l| l == m) {
                                push(
                                    Some(&step.id),
                                    ProblemKind::UnknownModel,
                                    crate::text!(
                                        "problem-unknown-model-step-pins-model-which-harness-does",
                                        a0 = (step.id).to_string(),
                                        m = m.to_string(),
                                        h = h.to_string()
                                    ),
                                );
                            }
                        }
                    }
                    if let Some(pin) = effort {
                        // Judged against the harnesses the runtime knows: one
                        // it does not is `UnknownHarness` above, and with no
                        // runtime described, or no harness named, there is
                        // nothing to check against.
                        let known: Vec<&str> = harness
                            .iter()
                            .map(String::as_str)
                            .filter(|h| ctx.harnesses.iter().any(|k| k == h))
                            .collect();
                        let takes_effort = |h: &&str| ctx.effort_harnesses.iter().any(|e| e == h);
                        if !known.is_empty() && !known.iter().any(takes_effort) {
                            push(
                                Some(&step.id),
                                ProblemKind::UnsupportedEffort,
                                crate::text!(
                                    "problem-unsupported-effort",
                                    a0 = (step.id).to_string(),
                                    effort = pin.to_string(),
                                    harnesses = known.join(", ")
                                ),
                            );
                        }
                    }
                    if let Some(schema) = output_schema {
                        if let Some(why) = ctx.checks.schema_error(schema) {
                            push(
                                Some(&step.id),
                                ProblemKind::BadSchema,
                                crate::text!(
                                    "problem-bad-schema-step-output-schema-not-json-schema",
                                    a0 = (step.id).to_string(),
                                    why = why.to_string()
                                ),
                            );
                        }
                    }
                }
                StepKind::Connector {
                    connector,
                    operation,
                    account,
                    params,
                    output_schema,
                    unattended,
                } => {
                    if let Some(schema) = output_schema {
                        if let Some(why) = ctx.checks.schema_error(schema) {
                            push(
                                Some(&step.id),
                                ProblemKind::BadSchema,
                                crate::text!(
                                    "problem-bad-schema-step-output-schema-not-json-schema",
                                    a0 = (step.id).to_string(),
                                    why = why.to_string()
                                ),
                            );
                        }
                    }
                    let call = Call {
                        site: CallSite::Step,
                        connector: connector.as_ref(),
                        operation: operation.as_ref(),
                        params,
                    };
                    let (found, resolved) = self.operation_problems(&step.id, &call, ctx);
                    for p in found {
                        push(p.step.as_ref(), p.kind, p.text);
                    }
                    if let Some((def, op)) = resolved {
                        // A write is gated or owned: an approval or a human
                        // step upstream on its normal flows, or the person's
                        // own `unattended`. Silence is the one thing refused
                        // — the Workflow Agent's prompt asks for the gate;
                        // this is what holds it to it.
                        if op.writes && !*unattended && !self.gated(&step.id) {
                            push(
                                Some(&step.id),
                                ProblemKind::UngatedWrite,
                                crate::text!(
                                    "problem-ungated-write-step-calls-which-writes-platform-with",
                                    a0 = (step.id).to_string(),
                                    connector = def.id.to_string(),
                                    operation = op.id.to_string()
                                ),
                            );
                        }
                        for p in self.account_problems(&step.id, def, account.as_ref(), ctx) {
                            push(p.step.as_ref(), p.kind, p.text);
                        }
                    }
                }
                StepKind::Emit { signal, .. } => {
                    if let Some(p) = signal_name_problem(&step.id, signal, Grammar::Run) {
                        push(p.step.as_ref(), p.kind, p.text);
                    }
                }
                StepKind::Wait { until } => {
                    for p in self.wait_problems(step, until, ctx) {
                        push(p.step.as_ref(), p.kind, p.text);
                    }
                }
                StepKind::Check {
                    check: CheckKind::Schema { schema, .. },
                } => {
                    if let Some(why) = ctx.checks.schema_error(schema) {
                        push(
                            Some(&step.id),
                            ProblemKind::BadSchema,
                            crate::text!(
                                "problem-bad-schema-step-schema-not-json-schema",
                                a0 = (step.id).to_string(),
                                why = why.to_string()
                            ),
                        );
                    }
                }
                StepKind::Notify {
                    scope: Some(scope), ..
                } => {
                    let literal =
                        matches!(template::placeholders(scope), Ok(found) if found.is_empty());
                    if literal && !is_conversation_id(scope) {
                        push(
                            Some(&step.id),
                            ProblemKind::NotifyScopeUnknown,
                            crate::text!(
                                "problem-notify-scope-unknown-step-posts-into-which-not-channel",
                                a0 = (step.id).to_string(),
                                scope = scope.to_string()
                            ),
                        );
                    }
                }
                _ => {}
            }
            // Only an agent speaks for a workflow: a fixed author that is a
            // person or a team is refused here; an author read from an input
            // is judged when the input is bound (`validate_bound`); an agent
            // nobody installed is `UnknownAssignee`'s, above.
            if let StepKind::Notify {
                author: Some(ValueRef::Fixed(author)),
                ..
            } = &step.kind
            {
                if author.as_agent().is_none() {
                    push(
                        Some(&step.id),
                        ProblemKind::NotifyAuthorNotAnAgent,
                        crate::text!(
                            "problem-notify-author-not-an-agent-step-speaks-as-who-not-agent",
                            a0 = (step.id).to_string(),
                            author = author.to_string()
                        ),
                    );
                }
            }
        }

        for input in &self.inputs {
            if !used_inputs.contains(&input.name) {
                push(
                    None,
                    ProblemKind::UnusedInput,
                    crate::text!(
                        "problem-unused-input-input-declared-but-no-step-reads",
                        a0 = (input.name).to_string(),
                        a1 = (input.name).to_string()
                    ),
                );
            }
        }

        problems
    }
}

/// Where a connector operation is called from: a `connector` step, or a
/// `connector` start polling it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CallSite {
    Step,
    Poll,
}

/// One connector call as validation reads it.
struct Call<'a> {
    site: CallSite,
    connector: Option<&'a ConnectorId>,
    operation: Option<&'a OperationId>,
    params: &'a BTreeMap<String, String>,
}

/// Whether a template has no placeholder under `grammar` — a literal the
/// definition can be judged by now, rather than a value only a run renders.
fn literal(tmpl: &str, grammar: Grammar) -> bool {
    matches!(template::placeholders_in(tmpl, grammar), Ok(found) if found.is_empty())
}

/// A literal signal name that is not dotted lowercase words.
fn signal_name_problem(step: &StepId, name: &str, grammar: Grammar) -> Option<Problem> {
    (literal(name, grammar) && !valid_signal_name(name)).then(|| Problem {
        step: Some(step.clone()),
        kind: ProblemKind::BadSignalName,
        text: crate::text!(
            "problem-bad-signal-name",
            a0 = step.to_string(),
            name = name.to_string()
        ),
    })
}

/// A literal conversation a message filter listens in that no conversation
/// could be.
fn listens_in_problem(step: &StepId, filter: &MessageFilter, grammar: Grammar) -> Option<Problem> {
    let place = filter.r#in.as_deref()?;
    (literal(place, grammar) && !is_conversation_id(place)).then(|| Problem {
        step: Some(step.clone()),
        kind: ProblemKind::NotifyScopeUnknown,
        text: crate::text!(
            "problem-notify-scope-unknown-message-in",
            a0 = step.to_string(),
            scope = place.to_string()
        ),
    })
}

/// A platform topic that is empty, or one the engine never emits.
fn topic_problem(
    step: &StepId,
    filter: &PlatformFilter,
    ctx: &ValidationCtx<'_>,
) -> Option<Problem> {
    let problem = |text| {
        Some(Problem {
            step: Some(step.clone()),
            kind: ProblemKind::UnknownTopic,
            text,
        })
    };
    if filter.topic.trim().is_empty() {
        return problem(crate::text!(
            "problem-unknown-topic-empty",
            a0 = step.to_string()
        ));
    }
    if !ctx.topics.is_empty() && !ctx.topics.contains(&filter.topic.as_str()) {
        return problem(crate::text!(
            "problem-unknown-topic",
            a0 = step.to_string(),
            topic = filter.topic.clone()
        ));
    }
    None
}

impl Workflow {
    /// What calling one connector operation with these parameters has wrong,
    /// at a `connector` step or a `connector` start — and the connector and
    /// the operation, when the call names both and this node has them. A
    /// choice not made yet stops the checks that depend on it.
    fn operation_problems<'c>(
        &self,
        step: &StepId,
        call: &Call<'_>,
        ctx: &ValidationCtx<'c>,
    ) -> (Vec<Problem>, Option<(&'c Connector, &'c Operation)>) {
        let mut out = Vec::new();
        let mut push = |kind: ProblemKind, text: crate::text::Text| {
            out.push(Problem {
                step: Some(step.clone()),
                kind,
                text,
            });
        };
        let a0 = step.to_string();
        let Some(connector) = call.connector else {
            push(
                ProblemKind::Unfilled,
                match call.site {
                    CallSite::Step => {
                        crate::text!("problem-unfilled-step-calls-no-connector-yet-pick", a0 = a0)
                    }
                    CallSite::Poll => {
                        crate::text!("problem-unfilled-start-polls-no-connector", a0 = a0)
                    }
                },
            );
            return (out, None);
        };
        let Some(operation) = call.operation else {
            push(
                ProblemKind::Unfilled,
                match call.site {
                    CallSite::Step => crate::text!(
                        "problem-unfilled-step-calls-but-names-no-operation",
                        a0 = a0,
                        connector = connector.to_string()
                    ),
                    CallSite::Poll => crate::text!(
                        "problem-unfilled-start-polls-no-operation",
                        a0 = a0,
                        connector = connector.to_string()
                    ),
                },
            );
            return (out, None);
        };
        let Some(def) = ctx.connectors.iter().find(|c| &c.id == connector) else {
            push(
                ProblemKind::UnknownConnector,
                crate::text!(
                    "problem-unknown-connector-step-calls-connector-which-not-installed",
                    a0 = a0,
                    connector = connector.to_string()
                ),
            );
            return (out, None);
        };
        let Some(op) = def.operation(operation) else {
            push(
                ProblemKind::UnknownOperation,
                crate::text!(
                    "problem-unknown-operation-step-calls-which-connector-does-not",
                    a0 = a0,
                    connector = connector.to_string(),
                    operation = operation.to_string(),
                    a1 = (def
                        .operations
                        .iter()
                        .map(|o| o.id.to_string())
                        .collect::<Vec<_>>()
                        .join(", "))
                    .to_string()
                ),
            );
            return (out, None);
        };
        for name in call.params.keys() {
            if !op.params.iter().any(|p| p.name.as_str() == name) {
                push(
                    ProblemKind::MissingConnectorParam,
                    crate::text!(
                        "problem-missing-connector-param-step-sets-which-not-parameter",
                        a0 = a0.clone(),
                        name = name.to_string(),
                        connector = connector.to_string(),
                        operation = operation.to_string()
                    ),
                );
            }
        }
        for p in op.params.iter().filter(|p| p.required) {
            let given = call
                .params
                .get(p.name.as_str())
                .is_some_and(|v| !v.trim().is_empty());
            if !given {
                push(
                    ProblemKind::MissingConnectorParam,
                    crate::text!(
                        "problem-missing-connector-param-step-leaves-unset-which-requires",
                        a0 = a0.clone(),
                        a1 = (p.name).to_string(),
                        connector = connector.to_string(),
                        operation = operation.to_string()
                    ),
                );
            }
        }
        (out, Some((def, op)))
    }

    /// What running a call as its account has wrong: a fixed account this
    /// node does not hold, an input that holds another connector's, or none
    /// named where the connector needs one and has no default.
    fn account_problems(
        &self,
        step: &StepId,
        def: &Connector,
        account: Option<&ValueRef<AccountId>>,
        ctx: &ValidationCtx<'_>,
    ) -> Vec<Problem> {
        let mut out = Vec::new();
        let mut push = |kind: ProblemKind, text: crate::text::Text| {
            out.push(Problem {
                step: Some(step.clone()),
                kind,
                text,
            });
        };
        let connector = &def.id;
        let mine = || ctx.accounts.iter().filter(|a| &a.connector == connector);
        match account {
            Some(ValueRef::Fixed(id)) => {
                if !mine().any(|a| a.id == *id) {
                    push(
                        ProblemKind::UnknownAccount,
                        crate::text!(
                            "problem-unknown-account-step-runs-as-account-which-not",
                            a0 = step.to_string(),
                            id = id.to_string(),
                            connector = connector.to_string()
                        ),
                    );
                }
            }
            Some(ValueRef::Input { input }) => {
                if let Some(InputDef {
                    kind:
                        InputKind::Account {
                            connector: Some(wanted),
                        },
                    ..
                }) = self.input(input)
                {
                    if wanted != connector {
                        push(
                            ProblemKind::InputKindMismatch,
                            crate::text!(
                                "problem-input-kind-mismatch-step-reads-input-as-account-but",
                                a0 = step.to_string(),
                                input = input.to_string(),
                                connector = connector.to_string(),
                                wanted = wanted.to_string()
                            ),
                        );
                    }
                }
            }
            None => {
                if def.auth.needs_account() && !mine().any(|a| a.default) && mine().count() != 1 {
                    push(
                        ProblemKind::UnknownAccount,
                        crate::text!(
                            "problem-unknown-account-step-names-no-account-has-no",
                            a0 = step.to_string(),
                            connector = connector.to_string()
                        ),
                    );
                }
            }
        }
        out
    }

    /// What a step's boundary events have wrong: carried by a kind that cannot
    /// be stopped, a name that clashes with a branch or another boundary, a
    /// reminder that diverts, a clock that cannot run, a divert with no flow,
    /// and what their filters and acts name.
    fn boundary_problems(&self, step: &Step) -> Vec<Problem> {
        let mut out = Vec::new();
        if step.boundaries.is_empty() {
            return out;
        }
        let a0 = step.id.to_string();
        let mut push = |kind: ProblemKind, text: crate::text::Text| {
            out.push(Problem {
                step: Some(step.id.clone()),
                kind,
                text,
            });
        };
        if !may_carry_boundaries(&step.kind) {
            push(
                ProblemKind::BoundaryOnInstantStep,
                crate::text!(
                    "problem-boundary-on-instant-step",
                    a0 = a0.clone(),
                    kind_word = step.kind.as_str()
                ),
            );
        }
        let kind_branches = step.kind.branches().unwrap_or_default();
        let mut names: BTreeSet<&Branch> = kind_branches.iter().collect();
        for b in &step.boundaries {
            let name = b.name.to_string();
            if !names.insert(&b.name) {
                push(
                    ProblemKind::DuplicateBranch,
                    crate::text!(
                        "problem-duplicate-branch-boundary",
                        a0 = a0.clone(),
                        name = name.clone()
                    ),
                );
            }
            if matches!(
                (&b.on, &b.act),
                (BoundaryOn::Every { .. }, BoundaryAct::Divert)
            ) {
                push(
                    ProblemKind::ReminderInterrupts,
                    crate::text!(
                        "problem-reminder-interrupts",
                        a0 = a0.clone(),
                        name = name.clone()
                    ),
                );
            }
            if matches!(b.on.secs(), Some(ValueRef::Fixed(0))) {
                push(
                    ProblemKind::BadTimer,
                    crate::text!(
                        "problem-bad-timer-zero",
                        a0 = a0.clone(),
                        name = name.clone()
                    ),
                );
            }
            if matches!(b.on, BoundaryOn::Every { max: 0, .. }) {
                push(
                    ProblemKind::BadTimer,
                    crate::text!(
                        "problem-bad-timer-no-fires",
                        a0 = a0.clone(),
                        name = name.clone()
                    ),
                );
            }
            if b.diverts() && !step.then.iter().any(|f| f.branch.as_ref() == Some(&b.name)) {
                push(
                    ProblemKind::RuleWithoutFlow,
                    crate::text!(
                        "problem-rule-without-flow-divert",
                        a0 = a0.clone(),
                        name = name.clone()
                    ),
                );
            }
            let found = match &b.on {
                BoundaryOn::Signal { filter } => {
                    signal_name_problem(&step.id, &filter.name, Grammar::Run)
                }
                BoundaryOn::Message { filter } => {
                    listens_in_problem(&step.id, filter, Grammar::Run)
                }
                BoundaryOn::After { .. } | BoundaryOn::Every { .. } => None,
            };
            if let Some(p) = found {
                push(p.kind, p.text);
            }
            match &b.act {
                BoundaryAct::Divert => {}
                BoundaryAct::Emit { signal, .. } => {
                    if let Some(p) = signal_name_problem(&step.id, signal, Grammar::Run) {
                        push(p.kind, p.text);
                    }
                }
                BoundaryAct::Notify { scope, author, .. } => {
                    if let Some(scope) = scope {
                        if literal(scope, Grammar::Run) && !is_conversation_id(scope) {
                            push(
                                ProblemKind::NotifyScopeUnknown,
                                crate::text!(
                                    "problem-notify-scope-unknown-step-posts-into-which-not-channel",
                                    a0 = a0.clone(),
                                    scope = scope.to_string()
                                ),
                            );
                        }
                    }
                    if let Some(ValueRef::Fixed(author)) = author {
                        if author.as_agent().is_none() {
                            push(
                                ProblemKind::NotifyAuthorNotAnAgent,
                                crate::text!(
                                    "problem-notify-author-not-an-agent-step-speaks-as-who-not-agent",
                                    a0 = a0.clone(),
                                    author = author.to_string()
                                ),
                            );
                        }
                    }
                }
            }
        }
        out
    }

    /// What a `start` step has wrong: a manual start that maps or guards, a
    /// mapping that reads anything but the event or names an undeclared input,
    /// event fields that read anything but the listening inputs, and what its
    /// event names.
    fn start_problems(
        &self,
        step: &Step,
        on: &StartOn,
        mapping: &BTreeMap<String, String>,
        guard: &Guard,
        ctx: &ValidationCtx<'_>,
        used_inputs: &mut BTreeSet<InputName>,
    ) -> Vec<Problem> {
        let mut out = Vec::new();
        let a0 = step.id.to_string();
        let mut push = |kind: ProblemKind, text: crate::text::Text| {
            out.push(Problem {
                step: Some(step.id.clone()),
                kind,
                text,
            });
        };
        if on.is_manual() {
            if !mapping.is_empty() {
                push(
                    ProblemKind::ManualStartConfigured,
                    crate::text!("problem-manual-start-configured-inputs", a0 = a0.clone()),
                );
            }
            if !guard.is_default() {
                push(
                    ProblemKind::ManualStartConfigured,
                    crate::text!("problem-manual-start-configured-guard", a0 = a0.clone()),
                );
            }
        }
        // The mapping: declared inputs, filled from the event and nothing else.
        for (name, tmpl) in mapping {
            match InputName::new(name.as_str()) {
                Ok(input) if self.input(&input).is_some() => {
                    used_inputs.insert(input);
                }
                _ => push(
                    ProblemKind::UnknownInput,
                    crate::text!(
                        "problem-unknown-input-start-maps",
                        a0 = a0.clone(),
                        name = name.to_string()
                    ),
                ),
            }
            match template::placeholders_in(tmpl, Grammar::StartMapping) {
                Ok(_) => {}
                Err(TemplateError::UnknownInMapping(key)) => push(
                    ProblemKind::StartPlaceholder,
                    crate::text!(
                        "problem-start-placeholder-mapping",
                        a0 = a0.clone(),
                        key = key.to_string()
                    ),
                ),
                Err(e) => push(
                    ProblemKind::BadTemplate,
                    crate::text!(
                        "problem-bad-question-step",
                        a0 = a0.clone(),
                        e = e.to_string()
                    ),
                ),
            }
        }
        // The event's own fields: the inputs the host listens with, and
        // nothing else.
        for tmpl in on.templates() {
            match template::placeholders_in(tmpl, Grammar::StartEvent) {
                Ok(found) => {
                    for p in found {
                        if let Placeholder::Input(name) = p {
                            if self.input(&name).is_none() {
                                push(
                                    ProblemKind::UnknownInput,
                                    crate::text!(
                                        "problem-unknown-input-step-uses-input-which-not-declared",
                                        a0 = a0.clone(),
                                        name = name.to_string()
                                    ),
                                );
                            }
                            used_inputs.insert(name);
                        } // LCOV_EXCL_LINE: the start-event grammar yields input placeholders alone (template.rs parse_start_event_key)
                    }
                }
                Err(TemplateError::UnknownInStartEvent(key)) => push(
                    ProblemKind::StartPlaceholder,
                    crate::text!(
                        "problem-start-placeholder-event-field",
                        a0 = a0.clone(),
                        key = key.to_string()
                    ),
                ),
                Err(e) => push(
                    ProblemKind::BadTemplate,
                    crate::text!(
                        "problem-bad-question-step",
                        a0 = a0.clone(),
                        e = e.to_string()
                    ),
                ),
            }
        }
        if let Some(schedule) = on.schedule() {
            for p in schedule_problems(&step.id, schedule, ctx) {
                push(p.kind, p.text);
            }
        }
        match on {
            StartOn::Manual | StartOn::Schedule { .. } | StartOn::Hook { .. } => {}
            StartOn::Message { filter } => {
                if let Some(p) = listens_in_problem(&step.id, filter, Grammar::StartEvent) {
                    push(p.kind, p.text);
                }
            }
            StartOn::Signal { filter } => {
                if let Some(p) = signal_name_problem(&step.id, &filter.name, Grammar::StartEvent) {
                    push(p.kind, p.text);
                }
            }
            StartOn::Project { filter } => {
                for p in project_problems(&step.id, filter, ctx) {
                    push(p.kind, p.text);
                }
            }
            StartOn::Run { filter } => {
                if let Some(p) = run_workflow_problem(&step.id, filter, ctx) {
                    push(p.kind, p.text);
                }
            }
            StartOn::Platform { filter } => {
                if let Some(p) = topic_problem(&step.id, filter, ctx) {
                    push(p.kind, p.text);
                }
            }
            StartOn::Connector {
                connector,
                operation,
                account,
                params,
                key,
                ..
            } => {
                let call = Call {
                    site: CallSite::Poll,
                    connector: connector.as_ref(),
                    operation: operation.as_ref(),
                    params,
                };
                let (found, resolved) = self.operation_problems(&step.id, &call, ctx);
                for p in found {
                    push(p.kind, p.text);
                }
                if let Some((def, op)) = resolved {
                    if op.writes {
                        push(
                            ProblemKind::BadPoll,
                            crate::text!(
                                "problem-bad-poll-writes",
                                a0 = a0.clone(),
                                connector = def.id.to_string(),
                                operation = op.id.to_string()
                            ),
                        );
                    }
                    for param in op.params.iter().filter(|p| p.kind == ParamKind::File) {
                        push(
                            ProblemKind::BadPoll,
                            crate::text!(
                                "problem-bad-poll-file",
                                a0 = a0.clone(),
                                connector = def.id.to_string(),
                                operation = op.id.to_string(),
                                param = param.name.to_string()
                            ),
                        );
                    }
                    for p in self.account_problems(&step.id, def, account.as_ref(), ctx) {
                        push(p.kind, p.text);
                    }
                }
                if key.as_deref().is_none_or(|k| k.trim().is_empty()) {
                    push(
                        ProblemKind::BadPoll,
                        crate::text!("problem-bad-poll-key", a0 = a0.clone()),
                    );
                }
            }
            StartOn::Check { project, .. } => {
                if let Some(ValueRef::Fixed(id)) = project {
                    if !ctx.projects.contains(id) {
                        push(
                            ProblemKind::UnknownProject,
                            crate::text!(
                                "problem-unknown-project-step-names-project-which-not-workspace",
                                a0 = a0.clone(),
                                id = id.to_string()
                            ),
                        );
                    }
                }
            }
        }
        out
    }

    /// What a `wait`'s catch has wrong: a cron that does not parse, a signal
    /// name, a conversation, a project, a workflow or a topic no event could
    /// come from.
    fn wait_problems(&self, step: &Step, until: &WaitFor, ctx: &ValidationCtx<'_>) -> Vec<Problem> {
        match until {
            WaitFor::Schedule {
                cron: ValueRef::Fixed(expr),
                ..
            } => ctx
                .checks
                .cron_error(expr)
                .map(|why| Problem {
                    step: Some(step.id.clone()),
                    kind: ProblemKind::BadCron,
                    text: crate::text!(
                        "problem-bad-cron-step",
                        a0 = (step.id).to_string(),
                        why = why.to_string()
                    ),
                })
                .into_iter()
                .collect(),
            WaitFor::Signal { filter } => signal_name_problem(&step.id, &filter.name, Grammar::Run)
                .into_iter()
                .collect(),
            WaitFor::Message { filter } => listens_in_problem(&step.id, filter, Grammar::Run)
                .into_iter()
                .collect(),
            WaitFor::Project { filter } => project_problems(&step.id, filter, ctx),
            WaitFor::Run { filter } => run_workflow_problem(&step.id, filter, ctx)
                .into_iter()
                .collect(),
            WaitFor::Platform { filter } => {
                topic_problem(&step.id, filter, ctx).into_iter().collect()
            }
            WaitFor::Delay { .. }
            | WaitFor::Time { .. }
            | WaitFor::Schedule { .. }
            | WaitFor::Release => {
                vec![]
            }
        }
    }
}

/// A schedule that is not one cadence, counts zero seconds, or names a cron
/// that does not parse.
fn schedule_problems(step: &StepId, schedule: &Schedule, ctx: &ValidationCtx<'_>) -> Vec<Problem> {
    let problem = |kind: ProblemKind, text: crate::text::Text| Problem {
        step: Some(step.clone()),
        kind,
        text,
    };
    let a0 = step.to_string();
    let mut out = Vec::new();
    if !schedule.is_well_formed() {
        out.push(problem(
            ProblemKind::BadTimer,
            crate::text!("problem-bad-timer-schedule", a0 = a0.clone()),
        ));
    }
    if matches!(schedule.every, Some(ValueRef::Fixed(0))) {
        out.push(problem(
            ProblemKind::BadTimer,
            crate::text!("problem-bad-timer-schedule-zero", a0 = a0.clone()),
        ));
    }
    if let Some(ValueRef::Fixed(expr)) = &schedule.cron {
        if let Some(why) = ctx.checks.cron_error(expr) {
            out.push(problem(
                ProblemKind::BadCron,
                crate::text!(
                    "problem-bad-cron-step",
                    a0 = a0.clone(),
                    why = why.to_string()
                ),
            ));
        }
    }
    out
}

/// A project filter that names no project yet, or one the workspace lacks.
fn project_problems(
    step: &StepId,
    filter: &ProjectFilter,
    ctx: &ValidationCtx<'_>,
) -> Vec<Problem> {
    let problem = |kind: ProblemKind, text: crate::text::Text| Problem {
        step: Some(step.clone()),
        kind,
        text,
    };
    match &filter.project {
        None => vec![problem(
            ProblemKind::Unfilled,
            crate::text!("problem-unfilled-project-change", a0 = step.to_string()),
        )],
        Some(ValueRef::Fixed(id)) if !ctx.projects.contains(id) => vec![problem(
            ProblemKind::UnknownProject,
            crate::text!(
                "problem-unknown-project-step-names-project-which-not-workspace",
                a0 = step.to_string(),
                id = id.to_string()
            ),
        )],
        Some(_) => vec![],
    }
}

/// A run filter naming a workflow the workspace lacks.
fn run_workflow_problem(
    step: &StepId,
    filter: &RunFilter,
    ctx: &ValidationCtx<'_>,
) -> Option<Problem> {
    let w = filter.workflow.as_ref()?;
    (!ctx.workflows.contains(w)).then(|| Problem {
        step: Some(step.clone()),
        kind: ProblemKind::UnknownWorkflow,
        text: crate::text!(
            "problem-unknown-workflow-run-event",
            a0 = step.to_string(),
            w = w.to_string()
        ),
    })
}

/// Does following `spawns` from `from` reach `target`? A depth-first walk
/// with a visited set, so a cycle elsewhere in the library cannot hang it.
fn spawn_reaches(
    spawns: &[(WorkflowId, Vec<WorkflowId>)],
    from: WorkflowId,
    target: WorkflowId,
) -> bool {
    if from == target {
        return true;
    }
    let mut seen: BTreeSet<WorkflowId> = BTreeSet::new();
    let mut stack: Vec<WorkflowId> = vec![from];
    while let Some(cur) = stack.pop() {
        if !seen.insert(cur) {
            continue;
        }
        if let Some((_, nexts)) = spawns.iter().find(|(id, _)| *id == cur) {
            for n in nexts {
                if *n == target {
                    return true;
                }
                stack.push(*n);
            }
        }
    }
    false
}

/// What a step's output promises a reader — see [`StepKind::promised`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Promised<'a> {
    /// The kind never writes an output.
    Nothing,
    /// A `human`: an answer, read as `{steps.<id>.answer}`.
    Answer,
    /// A result judged by an `output_schema`: the fields it requires;
    /// nothing at all without a schema.
    Schema(Option<&'a serde_json::Value>),
    /// A shape the machine writes, these keys and no other.
    Keys(&'static [&'static str]),
    /// What the world sent — a heard event's payload, a release's — any
    /// path.
    Payload,
}

impl Promised<'_> {
    /// The top-level fields a reader may count on, or `None` when any path
    /// is allowed (a payload).
    pub fn fields(&self) -> Option<Vec<String>> {
        match self {
            Promised::Nothing | Promised::Answer => Some(vec![]),
            Promised::Schema(None) => Some(vec![]),
            Promised::Schema(Some(schema)) => Some(
                schema
                    .get("required")
                    .and_then(|r| r.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect(),
            ),
            Promised::Keys(keys) => Some(keys.iter().map(|k| k.to_string()).collect()),
            Promised::Payload => None,
        }
    }
}

/// What a step can count on at every entry: the steps whose output — or a
/// `human`'s answer — is **present**, and the steps that have **settled**
/// (done, or failed and continued) whenever this step is entered.
/// [`Workflow::validate`] refuses a placeholder or a schema check outside
/// `present`, so a step reads only what is sure to be there
/// ([`ProblemKind::NotAssured`]); `settled` is what tells a step that may
/// have failed from one a first pass has not reached, and what a fan-in's
/// sure arms are judged against.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Assurance {
    pub present: BTreeSet<StepId>,
    pub settled: BTreeSet<StepId>,
}

/// One forward edge into a step: a `then` flow, labelled or not, a flow a
/// boundary event of the source diverts along, or the source's `on_fail:
/// then` route.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FwdEdge {
    Then { labelled: bool },
    Divert,
    Fail,
}

/// One reference from a step to another step's output, answer or outcome,
/// as [`Workflow::validate`] checks it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reference<'a> {
    /// `{steps.x.output}` or `{steps.x.output.<path>}` in a template: the
    /// output must be present.
    OutputText { path: Option<&'a str> },
    /// `{steps.x.answer}` in a template: the answer must be present.
    AnswerText,
    /// An `output_equals`/`output_matches` rule. An absent value reads
    /// `false`, so a rule may read a step that has not run — a `while`
    /// testing its own body, a `decide` after a step that failed and was
    /// passed over — and needs only an ancestor of the right shape.
    OutputRule { path: &'a str },
    /// An `answered` rule: a human, an ancestor.
    AnsweredRule,
    /// An `outcome` rule: a check or an approval, an ancestor.
    OutcomeRule,
    /// A schema `check` of the step's output: present.
    SchemaCheck,
}

impl<'a> Reference<'a> {
    fn what(self) -> &'static str {
        match self {
            Reference::OutputText { .. } | Reference::OutputRule { .. } => "reads the output of",
            Reference::AnswerText | Reference::AnsweredRule => "reads the answer of",
            Reference::OutcomeRule => "reads the outcome of",
            Reference::SchemaCheck => "checks the output of",
        }
    }

    /// Needs the value there when the step is entered — a rule reads an
    /// absent value as `false` and is not held to that.
    fn needs_present(self) -> bool {
        matches!(
            self,
            Reference::OutputText { .. } | Reference::AnswerText | Reference::SchemaCheck
        )
    }

    fn path(self) -> Option<&'a str> {
        match self {
            Reference::OutputText { path } => path,
            Reference::OutputRule { path } if !path.is_empty() => Some(path),
            _ => None,
        }
    }
}

/// What the reference checks read about the graph, computed once per
/// workflow and once per reading step.
struct ReferenceGraph<'a> {
    assurance: &'a BTreeMap<StepId, Assurance>,
    /// Every ancestor of the reading step, loop edges included.
    ancestors: &'a BTreeSet<StepId>,
    /// The ancestors a first pass has run through.
    forward_ancestors: &'a BTreeSet<StepId>,
}

/// The start steps of a definition that is not yet a [`Workflow`] — a
/// catalog template's body, say. The rule is [`Workflow::start_steps`]'s: the
/// `start` steps when there are any, else the root.
pub fn start_steps_in(steps: &[Step]) -> Vec<&Step> {
    let named: Vec<&Step> = steps
        .iter()
        .filter(|s| matches!(s.kind, StepKind::Start { .. }))
        .collect();
    if !named.is_empty() {
        return named;
    }
    let targets: BTreeSet<&StepId> = steps.iter().flat_map(Workflow::successors).collect();
    let roots: Vec<&Step> = steps.iter().filter(|s| !targets.contains(&s.id)).collect();
    if roots.is_empty() {
        steps.first().into_iter().collect()
    } else {
        roots
    }
}

impl Workflow {
    /// What the scope a run is started in refuses of this definition. A run
    /// of the workspace has no goal, so a step whose strings read
    /// `{goal.statement}` or `{goal.title}` cannot render there — one
    /// `NeedsGoal` per such step, asked before anything is written rather
    /// than when the step is entered deep into the run. A goal's run reads
    /// its goal freely.
    pub fn scope_problems(&self, scope: &RunScope) -> Vec<Problem> {
        if !scope.is_workspace() {
            return Vec::new();
        }
        self.steps
            .iter()
            .filter(|step| {
                step.templates().into_iter().any(|tmpl| {
                    template::placeholders(tmpl).is_ok_and(|found| {
                        found.iter().any(|p| {
                            matches!(p, Placeholder::GoalStatement | Placeholder::GoalTitle)
                        })
                    })
                })
            })
            .map(|step| Problem {
                step: Some(step.id.clone()),
                kind: ProblemKind::NeedsGoal,
                text: crate::text!(
                    "problem-needs-goal-step-reads-goal-run-of-workspace",
                    a0 = (step.id).to_string()
                ),
            })
            .collect()
    }

    /// What only bound inputs can answer, asked when they bind rather than
    /// when a step is armed deep into a run — the inputs a run starts with, or
    /// the inputs a host begins listening with: a cron read from an input
    /// parses, seconds read from an input are whole, a conversation read from
    /// the inputs is one — for a wait, a notify step, a boundary event and a
    /// start event alike — and what the run's scope refuses
    /// ([`Self::scope_problems`]). A value that reads a step's output or the
    /// goal is the run's to judge. The problems are [`Self::validate`]'s
    /// kinds, so every surface that refuses with problems refuses the same
    /// way.
    pub fn validate_bound(
        &self,
        scope: &RunScope,
        inputs: &BTreeMap<String, serde_json::Value>,
        accounts: &[ConnectorAccount],
        checks: &dyn SyntaxChecks,
    ) -> Vec<Problem> {
        let mut out = self.scope_problems(scope);
        let no_steps = BTreeMap::new();
        let ctx = template::TemplateCtx {
            inputs,
            steps: &no_steps,
            event: None,
            goal: None,
            params: template::no_values(),
            account: template::no_values(),
        };
        // Whether a template renders from the inputs alone, and what to.
        let from_inputs = |tmpl: &str, grammar: Grammar| -> Option<String> {
            let found = template::placeholders_in(tmpl, grammar).ok()?;
            let bound_now =
                !found.is_empty() && found.iter().all(|p| matches!(p, Placeholder::Input(_)));
            if !bound_now {
                return None; // a literal is validate()'s; anything else the run's
            }
            template::render_with(tmpl, &ctx, template::RenderContext::Text, grammar).ok()
        };
        let whole_secs = |step: &StepId, input: &InputName| -> Option<Problem> {
            inputs
                .get(input.as_str())
                .and_then(|v| v.as_u64())
                .is_none()
                .then(|| Problem {
                    step: Some(step.clone()),
                    kind: ProblemKind::InputKindMismatch,
                    text: crate::text!(
                        "problem-input-kind-mismatch-step-input-should-hold-whole-number",
                        a0 = step.to_string(),
                        input = input.to_string(),
                        a1 = (inputs
                            .get(input.as_str())
                            .map_or("nothing".to_string(), |v| v.to_string()))
                        .to_string()
                    ),
                })
        };
        let cron_from = |step: &StepId, input: &InputName| -> Option<Problem> {
            match inputs.get(input.as_str()).and_then(|v| v.as_str()) {
                None => Some(Problem {
                    step: Some(step.clone()),
                    kind: ProblemKind::BadCron,
                    text: crate::text!(
                        "problem-bad-cron-step-input-should-hold-cron-expression",
                        a0 = step.to_string(),
                        input = input.to_string(),
                        a1 = (inputs
                            .get(input.as_str())
                            .map_or("nothing".to_string(), |v| v.to_string()))
                        .to_string()
                    ),
                }),
                Some(expr) => checks.cron_error(expr).map(|why| Problem {
                    step: Some(step.clone()),
                    kind: ProblemKind::BadCron,
                    text: crate::text!(
                        "problem-bad-cron-step",
                        a0 = step.to_string(),
                        why = why.to_string()
                    ),
                }),
            }
        };
        for step in &self.steps {
            // A start's cadence and conversation, read from the inputs it
            // listens with.
            if let Some(on) = step.start_on() {
                if let Some(schedule) = on.schedule() {
                    if let Some(ValueRef::Input { input }) = &schedule.every {
                        out.extend(whole_secs(&step.id, input));
                    }
                    if let Some(ValueRef::Input { input }) = &schedule.cron {
                        out.extend(cron_from(&step.id, input));
                    }
                }
                if let StartOn::Message { filter } = on {
                    if let Some(place) = filter
                        .r#in
                        .as_deref()
                        .and_then(|t| from_inputs(t, Grammar::StartEvent))
                    {
                        if !is_conversation_id(&place) {
                            out.push(Problem {
                                step: Some(step.id.clone()),
                                kind: ProblemKind::NotifyScopeUnknown,
                                text: crate::text!(
                                    "problem-notify-scope-unknown-message-in",
                                    a0 = (step.id).to_string(),
                                    scope = place
                                ),
                            });
                        }
                    }
                }
            }
            // A boundary's clock and post, read from the run's inputs.
            for b in &step.boundaries {
                if let Some(ValueRef::Input { input }) = b.on.secs() {
                    out.extend(whole_secs(&step.id, input));
                }
                let BoundaryAct::Notify { scope, author, .. } = &b.act else {
                    continue;
                };
                if let Some(rendered) = scope.as_deref().and_then(|t| from_inputs(t, Grammar::Run))
                {
                    if !is_conversation_id(&rendered) {
                        out.push(Problem {
                            step: Some(step.id.clone()),
                            kind: ProblemKind::NotifyScopeUnknown,
                            text: crate::text!(
                                "problem-notify-scope-unknown-step-posts-into-which-not-channel-2",
                                a0 = (step.id).to_string(),
                                rendered = rendered.to_string()
                            ),
                        });
                    }
                }
                if let Some(ValueRef::Input { input }) = author {
                    let named = inputs
                        .get(input.as_str())
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse::<Assignee>().ok());
                    if let Some(who) = named.filter(|a| a.as_agent().is_none()) {
                        out.push(Problem {
                            step: Some(step.id.clone()),
                            kind: ProblemKind::NotifyAuthorNotAnAgent,
                            text: crate::text!(
                                "problem-notify-author-not-an-agent-step-input-names-who-not-agent",
                                a0 = (step.id).to_string(),
                                input = input.to_string(),
                                who = who.to_string()
                            ),
                        });
                    }
                }
            }
        }
        for step in &self.steps {
            let mut push = |kind: ProblemKind, text: crate::text::Text| {
                out.push(Problem {
                    step: Some(step.id.clone()),
                    kind,
                    text,
                });
            };
            match &step.kind {
                StepKind::Wait {
                    until:
                        WaitFor::Schedule {
                            cron: ValueRef::Input { input },
                            ..
                        },
                } => {
                    if let Some(p) = cron_from(&step.id, input) {
                        push(p.kind, p.text);
                    }
                }
                StepKind::Wait {
                    until:
                        WaitFor::Delay {
                            secs: ValueRef::Input { input },
                        },
                } => {
                    if let Some(p) = whole_secs(&step.id, input) {
                        push(p.kind, p.text);
                    }
                }
                StepKind::Notify {
                    scope: Some(scope), ..
                } => {
                    if let Some(rendered) = from_inputs(scope, Grammar::Run) {
                        if !is_conversation_id(&rendered) {
                            push(ProblemKind::NotifyScopeUnknown, crate::text!("problem-notify-scope-unknown-step-posts-into-which-not-channel-2", a0 = (step.id).to_string(), rendered = rendered.to_string()));
                        }
                    }
                }
                StepKind::Notify {
                    author: Some(ValueRef::Input { input }),
                    ..
                } => {
                    // A missing or mistyped value is `InputKindMismatch`'s;
                    // here the value is an assignee, and it must be an agent.
                    let named = inputs
                        .get(input.as_str())
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse::<Assignee>().ok());
                    if let Some(who) = named.filter(|a| a.as_agent().is_none()) {
                        push(
                            ProblemKind::NotifyAuthorNotAnAgent,
                            crate::text!(
                                "problem-notify-author-not-an-agent-step-input-names-who-not-agent",
                                a0 = (step.id).to_string(),
                                input = input.to_string(),
                                who = who.to_string()
                            ),
                        );
                    }
                }
                StepKind::Connector {
                    connector: Some(connector),
                    account: Some(ValueRef::Input { input }),
                    ..
                } => {
                    let named = inputs
                        .get(input.as_str())
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse::<AccountId>().ok());
                    match named {
                        Some(id)
                            if accounts
                                .iter()
                                .any(|a| a.id == id && &a.connector == connector) => {}
                        Some(id) => push(
                            ProblemKind::UnknownAccount,
                            crate::text!(
                                "problem-unknown-account-step-input-names-account-which-not",
                                a0 = (step.id).to_string(),
                                input = input.to_string(),
                                id = id.to_string(),
                                connector = connector.to_string()
                            ),
                        ),
                        None => push(
                            ProblemKind::UnknownAccount,
                            crate::text!(
                                "problem-unknown-account-step-input-should-hold-account-id",
                                a0 = (step.id).to_string(),
                                input = input.to_string(),
                                a1 = (inputs
                                    .get(input.as_str())
                                    .map_or("nothing".to_string(), |v| v.to_string()))
                                .to_string()
                            ),
                        ),
                    }
                }
                _ => {}
            }
        }
        out
    }
}

/// A literal notify scope must be something a conversation can be posted
/// into: a channel id, a goal id or a workstream id. Syntactic — whether it
/// exists is the run's business.
fn is_conversation_id(scope: &str) -> bool {
    ChannelId::new(scope).is_ok()
        || scope.parse::<GoalId>().is_ok()
        || scope.parse::<WorkstreamId>().is_ok()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::run::StepState;
    use serde_json::json;

    pub(crate) fn sid(s: &str) -> StepId {
        StepId::new(s).unwrap()
    }

    /// A goal's run: its scope refuses nothing of a definition.
    pub(crate) fn goal_scope() -> RunScope {
        RunScope::Goal {
            goal: GoalId::from_ulid(ulid::Ulid::from_parts(1, 1)),
        }
    }

    pub(crate) fn pk() -> PrincipalId {
        PrincipalId::new("ab".repeat(32)).unwrap()
    }

    pub(crate) fn agent(id: &str, then: &[&str]) -> Step {
        Step {
            id: sid(id),
            name: id.to_uppercase(),
            kind: StepKind::Agent {
                instructions: format!("do {id}"),
                assignee: Some(ValueRef::Fixed(Assignee::Agent("developer".into()))),
                project: None,
                harness: vec![],
                model: None,
                effort: None,
                output_schema: None,
                tier_ceiling: ToolTier::Write,
            },
            then: then.iter().map(|t| Flow::to(sid(t))).collect(),
            boundaries: vec![],
            join: Join::All,
            on_fail: OnFail::Fail,
            retries: 0,
            max_visits: DEFAULT_MAX_VISITS,
            position: None,
        }
    }

    pub(crate) fn step(id: &str, kind: StepKind, then: Vec<Flow>) -> Step {
        Step {
            id: sid(id),
            name: id.to_uppercase(),
            kind,
            then,
            boundaries: vec![],
            join: Join::All,
            on_fail: OnFail::Fail,
            retries: 0,
            max_visits: DEFAULT_MAX_VISITS,
            position: None,
        }
    }

    pub(crate) fn workflow(steps: Vec<Step>) -> Workflow {
        Workflow {
            id: WorkflowId::from_ulid(ulid::Ulid::from_parts(9, 1)),
            name: "Test".into(),
            description: String::new(),
            inputs: vec![],
            steps,
            origin: WorkflowOrigin::Workspace,
            author: pk(),
            tags: Tags::default(),
            revision: 1,
            archived: None,
            decision_making: false,
            created_at: 0,
        }
    }

    pub(crate) fn ctx() -> ValidationCtx<'static> {
        static ASSIGNEES: std::sync::LazyLock<Vec<Assignee>> = std::sync::LazyLock::new(|| {
            vec![
                Assignee::Agent("developer".into()),
                Assignee::Agent("reviewer".into()),
                Assignee::Team("engineering".into()),
            ]
        });
        ValidationCtx {
            assignees: &ASSIGNEES,
            ..ValidationCtx::default()
        }
    }

    fn kinds(problems: &[Problem]) -> Vec<ProblemKind> {
        problems.iter().map(|p| p.kind).collect()
    }

    /// An agent or connector step promising `fields`: its output schema
    /// requires each, so a later step may read them.
    pub(crate) fn promising(mut s: Step, fields: &[&str]) -> Step {
        let schema = Some(json!({ "type": "object", "required": fields }));
        match &mut s.kind {
            StepKind::Agent { output_schema, .. } | StepKind::Connector { output_schema, .. } => {
                *output_schema = schema;
            }
            other => panic!("{} promises nothing", other.as_str()),
        }
        s
    }

    #[test]
    fn a_valid_workflow_has_no_problems() {
        let wf = workflow(vec![
            promising(agent("a", &["b", "c"]), &["summary"]),
            agent("b", &["d"]),
            agent("c", &["d"]),
            step(
                "d",
                StepKind::Approval {
                    prompt: "Ship {steps.a.output.summary} for {goal.statement}?".into(),
                },
                vec![Flow::to(sid("e"))],
            ),
            step(
                "e",
                StepKind::End {
                    finish: Finish::Done,
                },
                vec![],
            ),
        ]);
        assert_eq!(wf.validate(&ctx()), vec![]);
        assert_eq!(wf.manual_entry().map(|s| s.id.as_str()), Some("a"));
        assert!(wf.ancestors(&sid("d")).contains(&sid("a")));
        assert!(!wf.ancestors(&sid("a")).contains(&sid("d")));
    }

    #[test]
    fn names_and_ids_are_checked() {
        let mut wf = workflow(vec![agent("a", &[]), agent("a", &[])]);
        wf.name = "  ".into();
        wf.steps[1].name = String::new();
        wf.inputs = vec![
            InputDef {
                name: InputName::new("x").unwrap(),
                label: "X".into(),
                kind: InputKind::Text,
                default: None,
                required: false,
            },
            InputDef {
                name: InputName::new("x").unwrap(),
                label: "X again".into(),
                kind: InputKind::Bool,
                default: None,
                required: false,
            },
        ];
        let k = kinds(&wf.validate(&ctx()));
        assert!(k.contains(&ProblemKind::EmptyName));
        assert!(k.contains(&ProblemKind::DuplicateInput));
        assert!(k.contains(&ProblemKind::DuplicateStepId));
        assert!(StepId::new("Build").is_err());
        assert!(StepId::new("a.b").is_err());
        assert!(StepId::new("1a").is_err());
        assert!(StepId::new("a".repeat(33).as_str()).is_err());
        assert!(StepId::new("build-2_x").is_ok());
        assert!(Branch::new(" ").is_err());
        assert!(Branch::new("yes").is_ok());
    }

    /// Each id rule says its own sentence: a step id or an input name is a
    /// word, a branch is a label, and neither borrows the slug id's rule.
    #[test]
    fn a_word_id_and_a_branch_each_refuse_with_their_own_sentence() {
        let word = StepId::new("a.b").unwrap_err().to_string();
        assert!(
            word.starts_with("invalid step id \"a.b\": expected 1-32 chars of a-z, 0-9, '-', '_', starting with a letter"),
            "{word}"
        );
        assert!(!word.contains("1-64"), "not the slug sentence: {word}");
        let name = InputName::new("Who").unwrap_err().to_string();
        assert!(name.starts_with("invalid input name \"Who\""), "{name}");
        let branch = Branch::new(" ").unwrap_err().to_string();
        assert!(branch.starts_with("invalid branch \" \""), "{branch}");
        assert!(branch.contains("not blank"), "{branch}");
        assert!(matches!(
            Branch::new("x".repeat(65)),
            Err(crate::CoreError::InvalidBranch(_))
        ));
        assert!(matches!(
            StepId::new(""),
            Err(crate::CoreError::InvalidWord { .. })
        ));
    }

    #[test]
    fn flows_must_resolve_and_never_point_home() {
        let wf = workflow(vec![agent("a", &["a", "zzz"])]);
        let k = kinds(&wf.validate(&ctx()));
        assert!(k.contains(&ProblemKind::SelfFlow));
        assert!(k.contains(&ProblemKind::UnknownStep));
    }

    #[test]
    fn a_failure_routed_to_its_own_step_is_a_self_flow_with_its_own_sentence() {
        let mut a = agent("a", &["b"]);
        a.on_fail = OnFail::Then { step: sid("a") };
        let mut b = agent("b", &[]);
        b.on_fail = OnFail::Then { step: sid("gone") };
        let problems = workflow(vec![a, b]).validate(&ctx());
        let own = problems
            .iter()
            .find(|p| p.kind == ProblemKind::SelfFlow)
            .expect("a failure that routes home is refused");
        assert_eq!(own.step, Some(sid("a")));
        assert!(
            own.text
                .to_string()
                .contains("routes its own failure to itself"),
            "{}",
            own.text
        );
        let missing = problems
            .iter()
            .find(|p| p.kind == ProblemKind::UnknownStep)
            .expect("a failure routed to nothing is refused");
        assert!(
            missing
                .text
                .to_string()
                .contains("routes its failure to `gone`"),
            "{}",
            missing.text
        );
    }

    #[test]
    fn a_long_workflow_validates_whole_and_a_fault_at_its_far_end_is_found() {
        // Five hundred steps in a line: what a generated or imported
        // definition can be. Sound, it has no problem — the walks are
        // iterative, so length is not depth — and quickly.
        let ids: Vec<String> = (0..500).map(|i| format!("s{i}")).collect();
        let line = |broken: bool| -> Workflow {
            workflow(
                ids.iter()
                    .enumerate()
                    .map(|(i, id)| match ids.get(i + 1) {
                        Some(next) => agent(id, &[next.as_str()]),
                        None if broken => agent(id, &["nowhere"]),
                        None => agent(id, &[]),
                    })
                    .collect(),
            )
        };
        // About a second in a debug build on its own; the walks it guards
        // against (per step and per edge — cubic in the step count) took
        // minutes. The bound is wide, because the whole workspace's tests
        // run beside this one and a loaded machine once measured nine
        // seconds: it is the shape that is held, not the machine.
        let began = std::time::Instant::now();
        assert_eq!(line(false).validate(&ctx()), vec![], "a long line is sound");
        let problems = line(true).validate(&ctx());
        assert!(
            began.elapsed() < std::time::Duration::from_secs(30),
            "validation is not quadratic in anything that matters: {:?}",
            began.elapsed()
        );
        assert_eq!(kinds(&problems), vec![ProblemKind::UnknownStep]);
        assert_eq!(problems[0].step, Some(sid("s499")));
    }

    #[test]
    fn a_ring_nobody_declared_is_walked_once_and_bounded_by_its_visits() {
        // a → b → c → a, with no loop step: cycles are fine by design — the
        // first step is the start, every walk of the graph carries a visited
        // set and ends, and what bounds the run is each step's `max_visits`.
        let ring = workflow(vec![
            agent("a", &["b"]),
            agent("b", &["c"]),
            agent("c", &["a"]),
        ]);
        assert_eq!(ring.validate(&ctx()), vec![]);
        assert!(ring.steps.iter().all(|s| s.max_visits > 0), "a ring ends");
        assert_eq!(ring.manual_entry().map(|s| s.id.clone()), Some(sid("a")));
        assert!(
            ring.is_loop_edge(&sid("c"), &sid("a")),
            "the edge home is the loop's"
        );
        assert_eq!(
            ring.ancestors(&sid("a")),
            BTreeSet::from([sid("a"), sid("b"), sid("c")]),
            "round the ring and home again, each step once"
        );
        // A ring with a way in and a way out.
        let wf = workflow(vec![
            agent("start", &["a"]),
            agent("a", &["b"]),
            agent("b", &["a", "end"]),
            step(
                "end",
                StepKind::End {
                    finish: Finish::Done,
                },
                vec![],
            ),
        ]);
        assert_eq!(
            wf.validate(&ctx()),
            wf.validate(&ctx()),
            "validation is a pure function of the definition"
        );
        assert_eq!(
            wf.ancestors(&sid("end")),
            BTreeSet::from([sid("start"), sid("a"), sid("b")])
        );
        // A failure's route is a way in: its remediation has the failed step upstream.
        let mut risky = agent("risky", &["end2"]);
        risky.on_fail = OnFail::Then { step: sid("mend") };
        let routed = workflow(vec![
            risky,
            agent("mend", &["end2"]),
            step(
                "end2",
                StepKind::End {
                    finish: Finish::Done,
                },
                vec![],
            ),
        ]);
        assert_eq!(
            routed.ancestors(&sid("mend")),
            BTreeSet::from([sid("risky")])
        );
    }

    #[test]
    fn exactly_one_start() {
        // Two roots.
        let two = workflow(vec![
            agent("a", &["c"]),
            agent("b", &["c"]),
            agent("c", &[]),
        ]);
        assert_eq!(
            kinds(&two.validate(&ctx()))
                .iter()
                .filter(|k| **k == ProblemKind::ManyStarts)
                .count(),
            2
        );
        // A pure cycle: every step has an incoming flow, so the first step is
        // the start and the way back is the loop edge.
        let cycle = workflow(vec![agent("a", &["b"]), agent("b", &["a"])]);
        assert_eq!(cycle.validate(&ctx()), vec![]);
        assert_eq!(cycle.manual_entry().map(|s| s.id.as_str()), Some("a"));
        assert_eq!(cycle.loop_edges(), BTreeSet::from([(sid("b"), sid("a"))]));
        // Only an empty workflow has no start.
        assert!(kinds(&workflow(vec![]).validate(&ctx())).contains(&ProblemKind::NoStart));
    }

    #[test]
    fn unreachable_steps_are_named() {
        let wf = workflow(vec![
            agent("a", &[]),
            agent("b", &["c"]),
            agent("c", &["b"]),
        ]);
        let k = kinds(&wf.validate(&ctx()));
        // `a` and the b/c cycle: two roots? No — b and c point at each other,
        // so only `a` is a root, and b/c are unreachable.
        assert_eq!(
            k.iter().filter(|k| **k == ProblemKind::Unreachable).count(),
            2
        );
        assert!(!k.contains(&ProblemKind::ManyStarts));
    }

    fn decide(rules: Vec<(&str, Condition)>, otherwise: &str, then: Vec<Flow>) -> Step {
        step(
            "d",
            StepKind::Decide {
                rules: rules
                    .into_iter()
                    .map(|(b, when)| Rule {
                        when,
                        branch: Branch::new(b).unwrap(),
                    })
                    .collect(),
                otherwise: Branch::new(otherwise).unwrap(),
                pick: Pick::First,
            },
            then,
        )
    }

    fn outcome_of(step: &str, passed: bool) -> Condition {
        Condition::Outcome {
            step: sid(step),
            passed,
        }
    }

    #[test]
    fn decide_branches_and_flows_agree() {
        let check = step(
            "c",
            StepKind::Check {
                check: CheckKind::Command {
                    command: "true".into(),
                },
            },
            vec![Flow::to(sid("d"))],
        );
        // Well-formed.
        let ok = workflow(vec![
            check.clone(),
            decide(
                vec![("yes", outcome_of("c", true))],
                "no",
                vec![
                    Flow::branch(sid("y"), Branch::new("yes").unwrap()),
                    Flow::branch(sid("n"), Branch::new("no").unwrap()),
                ],
            ),
            agent("y", &[]),
            agent("n", &[]),
        ]);
        assert_eq!(ok.validate(&ctx()), vec![]);

        // Unlabelled flow, a branch no rule chooses, a rule no flow carries,
        // a duplicated branch.
        let bad = workflow(vec![
            check.clone(),
            decide(
                vec![
                    ("yes", outcome_of("c", true)),
                    ("yes", outcome_of("c", false)),
                ],
                "no",
                vec![
                    Flow::to(sid("y")),
                    Flow::branch(sid("n"), Branch::new("maybe").unwrap()),
                ],
            ),
            agent("y", &[]),
            agent("n", &[]),
        ]);
        let k = kinds(&bad.validate(&ctx()));
        assert!(k.contains(&ProblemKind::UnlabelledFlowOnDecide));
        assert!(k.contains(&ProblemKind::BranchWithoutRule));
        assert!(k.contains(&ProblemKind::RuleWithoutFlow));
        assert!(k.contains(&ProblemKind::DuplicateBranch));

        // A labelled flow on a plain step.
        let plain = workflow(vec![
            step(
                "a",
                StepKind::Notify {
                    scope: None,
                    template: "hi".into(),
                    mentions: vec![],
                    author: None,
                },
                vec![Flow::branch(sid("b"), Branch::new("x").unwrap())],
            ),
            agent("b", &[]),
        ]);
        assert!(kinds(&plain.validate(&ctx())).contains(&ProblemKind::LabelledFlowOnPlainStep));
    }

    #[test]
    fn references_must_be_upstream_and_of_the_right_kind() {
        let wf = workflow(vec![
            agent("a", &["d"]),
            decide(
                vec![
                    ("x", outcome_of("later", true)), // downstream
                    ("y", outcome_of("a", true)),     // upstream but an agent, not a check
                    (
                        "z",
                        Condition::Answered {
                            step: sid("a"),
                            option: "o".into(),
                        },
                    ),
                ],
                "w",
                vec![
                    Flow::branch(sid("later"), Branch::new("x").unwrap()),
                    Flow::branch(sid("later"), Branch::new("y").unwrap()),
                    Flow::branch(sid("later"), Branch::new("z").unwrap()),
                    Flow::branch(sid("later"), Branch::new("w").unwrap()),
                ],
            ),
            step(
                "later",
                StepKind::Check {
                    check: CheckKind::Schema {
                        schema: json!({"type": "object"}),
                        of: Some(sid("later2")),
                    },
                },
                vec![Flow::to(sid("later2"))],
            ),
            agent("later2", &[]),
        ]);
        let k = kinds(&wf.validate(&ctx()));
        assert_eq!(
            k.iter().filter(|k| **k == ProblemKind::NotUpstream).count(),
            4,
            "{k:?}"
        );
    }

    #[test]
    fn templates_are_parsed_and_their_references_checked() {
        let mut wf = workflow(vec![agent("a", &["b"]), agent("b", &[])]);
        wf.steps[1].kind = StepKind::Agent {
            instructions: "{inputs.nope} {steps.a.output.x} {steps.zzz.answer} {steps.b.output} {mystery} {unclosed".into(),
            assignee: None,
            project: None,
            harness: vec![],
            model: None,
            effort: None,
            output_schema: None,
            tier_ceiling: ToolTier::Write,
        };
        let k = kinds(&wf.validate(&ctx()));
        // The parse stops at the first error, so this template reports the
        // unknown root; the unbalanced brace is the next save's problem.
        assert!(k.contains(&ProblemKind::UnknownPlaceholder), "{k:?}");

        wf.steps[1].kind = StepKind::Agent {
            instructions: "{inputs.nope} {steps.a.output.x} {steps.zzz.answer} {steps.b.output}"
                .into(),
            assignee: None,
            project: None,
            harness: vec![],
            model: None,
            effort: None,
            output_schema: None,
            tier_ceiling: ToolTier::Write,
        };
        let k = kinds(&wf.validate(&ctx()));
        assert!(k.contains(&ProblemKind::UnknownInput), "{k:?}");
        assert!(k.contains(&ProblemKind::UnknownStep), "{k:?}");
        assert!(
            k.contains(&ProblemKind::NotUpstream),
            "b names itself: {k:?}"
        );

        wf.steps[1].kind = StepKind::Agent {
            instructions: "{unclosed".into(),
            assignee: None,
            project: None,
            harness: vec![],
            model: None,
            effort: None,
            output_schema: None,
            tier_ceiling: ToolTier::Write,
        };
        assert!(kinds(&wf.validate(&ctx())).contains(&ProblemKind::BadTemplate));
    }

    #[test]
    fn value_references_name_inputs_of_the_right_kind_and_known_things() {
        let mut wf = workflow(vec![agent("a", &[])]);
        wf.inputs = vec![InputDef {
            name: InputName::new("who").unwrap(),
            label: "Who".into(),
            kind: InputKind::Text,
            default: None,
            required: true,
        }];
        wf.steps[0].kind = StepKind::Agent {
            instructions: "x".into(),
            assignee: Some(ValueRef::Input {
                input: InputName::new("who").unwrap(),
            }),
            project: Some(ValueRef::Input {
                input: InputName::new("where").unwrap(),
            }),
            harness: vec![],
            model: None,
            effort: None,
            output_schema: None,
            tier_ceiling: ToolTier::Write,
        };
        let k = kinds(&wf.validate(&ctx()));
        assert!(k.contains(&ProblemKind::InputKindMismatch), "{k:?}");
        assert!(k.contains(&ProblemKind::UnknownInput), "{k:?}");

        let wf = workflow(vec![
            step(
                "a",
                StepKind::Notify {
                    scope: None,
                    template: "hi".into(),
                    mentions: vec![ValueRef::Fixed(Assignee::Agent("nobody".into()))],
                    author: None,
                },
                vec![Flow::to(sid("b"))],
            ),
            step(
                "b",
                StepKind::Spawn {
                    statement_template: "child".into(),
                    workflow: Some(WorkflowId::from_ulid(ulid::Ulid::from_parts(5, 5))),
                    assignees: vec![],
                    inputs: Default::default(),
                    wait: true,
                },
                vec![Flow::to(sid("c"))],
            ),
            step(
                "c",
                StepKind::Agent {
                    instructions: "x".into(),
                    assignee: None,
                    project: Some(ValueRef::Fixed(ProjectId::from_ulid(
                        ulid::Ulid::from_parts(7, 7),
                    ))),
                    harness: vec![],
                    model: None,
                    effort: None,
                    output_schema: None,
                    tier_ceiling: ToolTier::Write,
                },
                vec![],
            ),
        ]);
        let k = kinds(&wf.validate(&ctx()));
        assert!(k.contains(&ProblemKind::UnknownAssignee), "{k:?}");
        assert!(k.contains(&ProblemKind::UnknownWorkflow), "{k:?}");
        assert!(k.contains(&ProblemKind::UnknownProject), "{k:?}");
    }

    #[test]
    fn ends_visits_and_questions_are_checked() {
        let mut wf = workflow(vec![
            step(
                "e",
                StepKind::End {
                    finish: Finish::Done,
                },
                vec![Flow::to(sid("q"))],
            ),
            step(
                "q",
                StepKind::Human {
                    prompt: "?".into(),
                    options: vec![AskOption::new("a", "A"), AskOption::new("a", "A2")],
                    multi: false,
                    assignee: None,
                },
                vec![],
            ),
        ]);
        wf.steps[1].max_visits = 0;
        let k = kinds(&wf.validate(&ctx()));
        assert!(k.contains(&ProblemKind::EndWithSuccessors));
        assert!(k.contains(&ProblemKind::ZeroVisits));
        assert!(k.contains(&ProblemKind::BadQuestion));
    }

    #[test]
    fn cycles_are_legal_when_bounded() {
        let wf = workflow(vec![
            agent("a", &["c"]),
            step(
                "c",
                StepKind::Check {
                    check: CheckKind::Command {
                        command: "test".into(),
                    },
                },
                vec![Flow::to(sid("d"))],
            ),
            decide(
                vec![("ok", outcome_of("c", true))],
                "again",
                vec![
                    Flow::branch(sid("e"), Branch::new("ok").unwrap()),
                    Flow::branch(sid("a"), Branch::new("again").unwrap()),
                ],
            ),
            step(
                "e",
                StepKind::End {
                    finish: Finish::Done,
                },
                vec![],
            ),
        ]);
        // `a` has an incoming flow from `d`, but every step has one: the
        // first step in display order is the start, and `d → a` is the loop.
        assert_eq!(wf.validate(&ctx()), vec![]);
        assert_eq!(wf.manual_entry().map(|s| s.id.as_str()), Some("a"));
        assert_eq!(wf.loop_edges(), BTreeSet::from([(sid("d"), sid("a"))]));
        assert!(wf.is_loop_edge(&sid("d"), &sid("a")));
        assert!(!wf.is_loop_edge(&sid("a"), &sid("c")));
        // A step before the loop makes that step the start and the same edge the loop.
        let mut wf = wf;
        wf.steps.insert(0, agent("start", &["a"]));
        assert_eq!(wf.validate(&ctx()), vec![]);
        assert_eq!(wf.manual_entry().map(|s| s.id.as_str()), Some("start"));
        assert_eq!(wf.loop_edges(), BTreeSet::from([(sid("d"), sid("a"))]));
    }

    #[test]
    fn loop_edges_are_the_back_edges_from_the_start() {
        // A diamond whose join loops back to the arms' source: only the edge
        // that closes the cycle is a loop edge; the arms into the join are
        // forward, so the join still waits for both.
        let wf = workflow(vec![
            agent("s", &["a"]),
            agent("a", &["b", "c"]),
            agent("b", &["d"]),
            agent("c", &["d"]),
            decide(
                vec![("again", outcome_of("b", true))],
                "on",
                vec![
                    Flow::branch(sid("a"), Branch::new("again").unwrap()),
                    Flow::branch(sid("e"), Branch::new("on").unwrap()),
                ],
            ),
            step(
                "e",
                StepKind::End {
                    finish: Finish::Done,
                },
                vec![],
            ),
        ]);
        assert_eq!(wf.loop_edges(), BTreeSet::from([(sid("d"), sid("a"))]));
        assert!(!wf.is_loop_edge(&sid("b"), &sid("d")));
        assert!(!wf.is_loop_edge(&sid("c"), &sid("d")));

        // A fail route back is a loop edge too.
        let mut verify = step(
            "verify",
            StepKind::Check {
                check: CheckKind::Command {
                    command: "test".into(),
                },
            },
            vec![Flow::to(sid("ship"))],
        );
        verify.on_fail = OnFail::Then { step: sid("fix") };
        let wf = workflow(vec![agent("fix", &["verify"]), verify, agent("ship", &[])]);
        assert_eq!(
            wf.loop_edges(),
            BTreeSet::from([(sid("verify"), sid("fix"))])
        );
        assert_eq!(wf.validate(&ctx()), vec![]);
    }

    #[test]
    fn a_fail_route_target_is_not_a_second_start() {
        // probe → healthy; probe's failure routes to diagnose → done. Only
        // `probe` starts: `diagnose` is reached, by the fail route.
        let mut probe = step(
            "probe",
            StepKind::Check {
                check: CheckKind::Command {
                    command: "test".into(),
                },
            },
            vec![Flow::to(sid("healthy"))],
        );
        probe.on_fail = OnFail::Then {
            step: sid("diagnose"),
        };
        let wf = workflow(vec![
            probe,
            step(
                "healthy",
                StepKind::End {
                    finish: Finish::Done,
                },
                vec![],
            ),
            agent("diagnose", &["done"]),
            step(
                "done",
                StepKind::End {
                    finish: Finish::Done,
                },
                vec![],
            ),
        ]);
        assert_eq!(
            wf.start_steps()
                .iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>(),
            vec!["probe"]
        );
        assert_eq!(wf.validate(&ctx()), vec![]);
        assert!(wf.loop_edges().is_empty());
    }

    #[test]
    fn a_first_step_loop_target_is_the_start() {
        // survey → sound(decide: ok → write, again → survey) → write.
        let wf = workflow(vec![
            agent("survey", &["d"]),
            decide(
                vec![("ok", outcome_of("survey", true))],
                "again",
                vec![
                    Flow::branch(sid("write"), Branch::new("ok").unwrap()),
                    Flow::branch(sid("survey"), Branch::new("again").unwrap()),
                ],
            ),
            agent("write", &[]),
        ]);
        assert_eq!(
            wf.start_steps()
                .iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>(),
            vec!["survey"]
        );
        assert_eq!(wf.loop_edges(), BTreeSet::from([(sid("d"), sid("survey"))]));
        let k = kinds(&wf.validate(&ctx()));
        assert!(!k.contains(&ProblemKind::NoStart), "{k:?}");
        assert!(!k.contains(&ProblemKind::Unreachable), "{k:?}");
    }

    #[test]
    fn successors_lists_then_targets_then_the_fail_route_once() {
        let mut s = agent("a", &["b", "c", "b"]);
        s.on_fail = OnFail::Then { step: sid("c") };
        assert_eq!(
            Workflow::successors(&s),
            vec![&sid("b"), &sid("c")],
            "a duplicate flow and a fail route to a then-target count once"
        );
        let mut t = agent("t", &[]);
        t.on_fail = OnFail::Then { step: sid("fix") };
        assert_eq!(Workflow::successors(&t), vec![&sid("fix")]);
        assert!(Workflow::successors(&agent("u", &[])).is_empty());
    }

    #[test]
    fn a_diverted_step_did_not_pass() {
        let inputs = BTreeMap::new();
        let steps = BTreeMap::from([(
            sid("review"),
            StepRecord {
                state: StepState::Diverted {
                    by: Branch::new("late").unwrap(),
                },
                ..StepRecord::default()
            },
        )]);
        let ctx = ConditionCtx {
            inputs: &inputs,
            steps: &steps,
            hour: 0,
        };
        assert!(outcome_of("review", false).holds(&ctx));
        assert!(!outcome_of("review", true).holds(&ctx));
    }

    #[test]
    fn the_payload_leaves_are_gone_and_never_read() {
        for gone in ["payload_equals", "payload_matches"] {
            let raw = json!({"condition": gone, "path": "x", "value": 1});
            assert!(
                serde_json::from_value::<Condition>(raw).is_err(),
                "{gone} is not a condition: a start's mapping turns the event into inputs"
            );
        }
    }

    #[test]
    fn step_and_input_conditions() {
        let inputs = BTreeMap::from([("env".to_string(), json!("prod"))]);
        let mut steps = BTreeMap::new();
        steps.insert(
            sid("c"),
            StepRecord {
                state: StepState::Failed,
                ..StepRecord::default()
            },
        );
        steps.insert(
            sid("r"),
            StepRecord {
                state: StepState::Done { branches: vec![] },
                output: Some(json!({"verdict": "approve", "n": 2})),
                answer: Some(crate::ask::Answer::selecting(["ship"])),
                ..StepRecord::default()
            },
        );
        let ctx = ConditionCtx {
            inputs: &inputs,
            steps: &steps,
            hour: 12,
        };
        assert!(Condition::InputEquals {
            input: InputName::new("env").unwrap(),
            value: json!("prod")
        }
        .holds(&ctx));
        assert!(!outcome_of("c", true).holds(&ctx));
        assert!(outcome_of("c", false).holds(&ctx));
        assert!(outcome_of("r", true).holds(&ctx));
        assert!(
            !outcome_of("zzz", false).holds(&ctx),
            "an unknown step is neither"
        );
        assert!(Condition::OutputEquals {
            step: sid("r"),
            path: "verdict".into(),
            value: json!("approve")
        }
        .holds(&ctx));
        assert!(Condition::OutputMatches {
            step: sid("r"),
            path: "".into(),
            contains: "\"n\":2".into()
        }
        .holds(&ctx));
        assert!(Condition::Answered {
            step: sid("r"),
            option: "ship".into()
        }
        .holds(&ctx));
        assert!(!Condition::Answered {
            step: sid("r"),
            option: "hold".into()
        }
        .holds(&ctx));
    }

    #[test]
    fn between_windows_including_the_wrap() {
        let inputs = BTreeMap::new();
        let steps = BTreeMap::new();
        let at = |hour: u8| ConditionCtx {
            inputs: &inputs,
            steps: &steps,
            hour,
        };
        let day = Condition::Between {
            from_hour: 9,
            to_hour: 17,
        };
        assert!(day.holds(&at(9)));
        assert!(day.holds(&at(17)));
        assert!(!day.holds(&at(8)));
        assert!(!day.holds(&at(18)));
        let night = Condition::Between {
            from_hour: 22,
            to_hour: 6,
        };
        assert!(night.holds(&at(23)));
        assert!(night.holds(&at(0)));
        assert!(night.holds(&at(6)));
        assert!(!night.holds(&at(7)));
        assert!(!night.holds(&at(21)));
    }

    #[test]
    fn the_wire_shape_is_flat_and_flows_take_a_bare_id() {
        let wf = workflow(vec![agent("a", &["b"]), agent("b", &[])]);
        let json = serde_json::to_value(&wf).unwrap();
        assert_eq!(json["steps"][0]["kind"], "agent");
        assert_eq!(json["steps"][0]["instructions"], "do a");
        assert_eq!(json["steps"][0]["then"][0]["to"], "b");
        assert_eq!(serde_json::from_value::<Workflow>(json).unwrap(), wf);

        let from_toml: Step = toml::from_str(
            r#"
            id = "verify"
            name = "Run the tests"
            kind = "check"
            check = { check = "command", command = "cargo test" }
            then = ["ship", { to = "fix", branch = "no" }]
            on_fail = { on_fail = "then", step = "fix" }
            retries = 1
            "#,
        )
        .unwrap();
        assert_eq!(from_toml.then[0], Flow::to(sid("ship")));
        assert_eq!(
            from_toml.then[1],
            Flow::branch(sid("fix"), Branch::new("no").unwrap())
        );
        assert_eq!(from_toml.on_fail, OnFail::Then { step: sid("fix") });
        assert_eq!(from_toml.max_visits, DEFAULT_MAX_VISITS);
        assert_eq!(from_toml.retries, 1);

        // An old shape is refused, never half-read.
        assert!(serde_json::from_value::<Workflow>(json!({
            "id": "01ARZ3NDEKTSV4RRFFQ69G5FAV", "name": "x", "steps": [],
            "origin": "local", "author": "ab".repeat(32), "revision": 1, "created_at": 0,
            "criteria": []
        }))
        .is_err());
    }

    #[test]
    fn a_workflow_carrying_the_retired_switch_is_refused() {
        let mut wf = workflow(vec![agent("a", &[])]);
        wf.decision_making = true;
        let json = serde_json::to_value(&wf).unwrap();
        assert_eq!(json["decision_making"], true);
        assert_eq!(
            serde_json::from_value::<Workflow>(json.clone()).unwrap(),
            wf
        );

        // The definition denies unknown fields, so the retired switch is a
        // refusal and never a switch silently read as off.
        let mut retired = json;
        let fields = retired.as_object_mut().unwrap();
        fields.remove("decision_making");
        fields.insert("decision_maker".into(), json!(true)); // terminology-lint-ignore: decision-maker - proves the retired word is refused
        let refused = serde_json::from_value::<Workflow>(retired).unwrap_err();
        assert!(refused.to_string().contains("unknown field"), "{refused}");
    }

    #[test]
    fn value_refs_and_input_kinds() {
        let fixed: ValueRef<Assignee> =
            serde_json::from_value(json!({"agent": "developer"})).unwrap();
        assert_eq!(fixed, ValueRef::Fixed(Assignee::Agent("developer".into())));
        let by_input: ValueRef<Assignee> = serde_json::from_value(json!({"input": "who"})).unwrap();
        assert_eq!(
            by_input,
            ValueRef::Input {
                input: InputName::new("who").unwrap()
            }
        );
        let project: ValueRef<ProjectId> =
            serde_json::from_value(json!("01ARZ3NDEKTSV4RRFFQ69G5FAV")).unwrap();
        assert!(matches!(project, ValueRef::Fixed(_)));

        assert!(InputKind::Text.accepts(&json!("x")));
        assert!(!InputKind::Text.accepts(&json!(1)));
        assert!(InputKind::Number.accepts(&json!(1.5)));
        assert!(InputKind::Bool.accepts(&json!(false)));
        assert!(InputKind::Choice {
            options: vec!["a".into(), "b".into()]
        }
        .accepts(&json!("b")));
        assert!(!InputKind::Choice {
            options: vec!["a".into()]
        }
        .accepts(&json!("b")));
        assert!(InputKind::Assignee.accepts(&json!("agent:developer")));
        assert!(!InputKind::Assignee.accepts(&json!("developer")));
        assert!(InputKind::Project.accepts(&json!("01ARZ3NDEKTSV4RRFFQ69G5FAV")));
        assert!(!InputKind::Project.accepts(&json!("web-app")));
        assert_eq!(StepKind::NAMES.len(), 18);
        assert_eq!(ProblemKind::ALL.len(), 61);
    }

    fn input(name: &str, kind: InputKind) -> InputDef {
        InputDef {
            name: InputName::new(name).unwrap(),
            label: name.to_uppercase(),
            kind,
            default: None,
            required: false,
        }
    }

    fn agent_reading(id: &str, instructions: &str, then: &[&str]) -> Step {
        let mut s = agent(id, then);
        s.kind = StepKind::Agent {
            instructions: instructions.into(),
            assignee: Some(ValueRef::Fixed(Assignee::Agent("developer".into()))),
            project: None,
            harness: vec![],
            model: None,
            effort: None,
            output_schema: None,
            tier_ceiling: ToolTier::Write,
        };
        s
    }

    #[test]
    fn a_steps_position_round_trips_and_is_absent_until_written() {
        let mut s = step(
            "a",
            StepKind::End {
                finish: Finish::Done,
            },
            vec![],
        );
        let bare = serde_json::to_value(&s).unwrap();
        assert!(bare.get("position").is_none(), "absent, not null: {bare}");
        s.position = Some(Point { x: 40, y: 120 });
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["position"], json!({ "x": 40, "y": 120 }));
        assert_eq!(serde_json::from_value::<Step>(json).unwrap(), s);
        // A template may place a step by hand.
        let from_toml: Step = toml::from_str(
            "id = \"a\"\nname = \"A\"\nkind = \"end\"\nfinish = \"done\"\nposition = { x = 8, y = 16 }\n",
        )
        .unwrap();
        assert_eq!(from_toml.position, Some(Point { x: 8, y: 16 }));
        // A misspelled position is a key like any other: refused by name.
        let e = serde_json::from_value::<Step>(json!({
            "id": "a", "name": "A", "kind": "end", "finish": "done", "positon": { "x": 1, "y": 2 }
        }))
        .unwrap_err()
        .to_string();
        assert!(e.contains("positon") && e.contains("position"), "{e}");
        // A point with a stray field is refused too.
        assert!(serde_json::from_value::<Step>(json!({
            "id": "a", "name": "A", "kind": "end", "finish": "done", "position": { "x": 1, "y": 2, "z": 3 }
        }))
        .is_err());
    }

    #[test]
    fn a_misspelled_key_is_refused() {
        // A step's own field.
        let e = serde_json::from_value::<Step>(json!({
            "id": "a", "name": "A", "kind": "agent", "instructions": "x", "retires": 2
        }))
        .unwrap_err()
        .to_string();
        assert!(e.contains("retires"), "{e}");
        assert!(e.contains("retries"), "the message lists the fields: {e}");
        // A kind's field spelled for another kind.
        let e = serde_json::from_value::<Step>(json!({
            "id": "a", "name": "A", "kind": "approval", "prompt": "?", "instructions": "x"
        }))
        .unwrap_err()
        .to_string();
        assert!(e.contains("instructions"), "{e}");
        // A kind that is not one.
        let e = serde_json::from_value::<Step>(json!({ "id": "a", "name": "A", "kind": "agnt" }))
            .unwrap_err()
            .to_string();
        assert!(e.contains("agnt"), "{e}");
        // No kind at all.
        assert!(serde_json::from_value::<Step>(json!({ "id": "a", "name": "A" })).is_err());
        // An input.
        let e = serde_json::from_value::<InputDef>(json!({
            "name": "x", "label": "X", "kind": "text", "defualt": "v"
        }))
        .unwrap_err()
        .to_string();
        assert!(e.contains("defualt"), "{e}");
        assert!(serde_json::from_value::<InputDef>(json!({
            "name": "x", "label": "X", "kind": "choice", "options": ["a"]
        }))
        .is_ok());
        assert!(serde_json::from_value::<InputDef>(json!({
            "name": "x", "label": "X", "kind": "text", "options": ["a"]
        }))
        .is_err());
        // A flow object, a rule, a problem.
        assert!(serde_json::from_value::<Flow>(json!({ "too": "a" })).is_err());
        assert!(serde_json::from_value::<Flow>(json!({ "to": "a", "branch": "b" })).is_ok());
        assert!(serde_json::from_value::<Flow>(json!("a")).is_ok());
        assert!(serde_json::from_value::<Rule>(json!({
            "when": { "condition": "between", "from_hour": 1, "to_hour": 2 },
            "branch": "b", "extra": 1
        }))
        .is_err());
        assert!(serde_json::from_value::<Problem>(json!({
            "kind": "no_start", "message": "m", "severity": "high"
        }))
        .is_err());
        // The well-formed step still reads, from JSON and from TOML.
        let s: Step = serde_json::from_value(json!({
            "id": "a", "name": "A", "kind": "check",
            "check": { "check": "command", "command": "echo ok" }, "retries": 1
        }))
        .unwrap();
        assert_eq!(s.retries, 1);
        let s: Step = toml::from_str(
            "id = \"w\"\nname = \"W\"\nkind = \"wait\"\nuntil = { until = \"delay\", secs = 30 }\n",
        )
        .unwrap();
        assert_eq!(
            s.kind,
            StepKind::Wait {
                until: WaitFor::Delay {
                    secs: ValueRef::Fixed(30)
                }
            }
        );
        let s: Step = toml::from_str(
            "id = \"w\"\nname = \"W\"\nkind = \"wait\"\nuntil = { until = \"delay\", secs = { input = \"hold\" } }\n",
        )
        .unwrap();
        assert_eq!(
            s.kind,
            StepKind::Wait {
                until: WaitFor::Delay {
                    secs: ValueRef::Input {
                        input: InputName::new("hold").unwrap()
                    }
                }
            }
        );
    }

    #[test]
    fn step_kind_field_table_matches_the_wire() {
        // Every kind, fully populated, serialized: the keys beside the step's
        // own must be exactly what `fields_of` says, both ways.
        let who = ValueRef::Fixed(Assignee::Agent("developer".into()));
        let samples: Vec<StepKind> = vec![
            StepKind::Start {
                on: StartOn::Schedule {
                    schedule: Schedule::cron("0 9 * * 1", Some("UTC".into())),
                },
                inputs: BTreeMap::from([("x".to_string(), "{event.payload.x}".to_string())]),
                guard: Guard {
                    debounce_secs: 30,
                    overlap: crate::start::Overlap::Skip,
                },
            },
            StepKind::Parallel,
            StepKind::Emit {
                signal: "report.ready".into(),
                payload: BTreeMap::from([("url".to_string(), "{inputs.x}".to_string())]),
            },
            StepKind::Agent {
                instructions: "x".into(),
                assignee: Some(who.clone()),
                project: Some(ValueRef::Fixed(ProjectId::from_ulid(
                    ulid::Ulid::from_parts(1, 1),
                ))),
                harness: vec!["h".into()],
                model: Some("m".into()),
                effort: Some(EffortChoice::Xhigh),
                output_schema: Some(json!({})),
                tier_ceiling: ToolTier::Write,
            },
            StepKind::Human {
                prompt: "?".into(),
                options: vec![AskOption::new("a", "A")],
                multi: true,
                assignee: Some(who.clone()),
            },
            StepKind::Approval { prompt: "?".into() },
            StepKind::Check {
                check: CheckKind::Command {
                    command: "echo ok".into(),
                },
            },
            StepKind::Decide {
                rules: vec![],
                otherwise: Branch::new("b").unwrap(),
                pick: Pick::Every,
            },
            StepKind::If {
                when: Condition::Between {
                    from_hour: 0,
                    to_hour: 23,
                },
            },
            StepKind::Switch {
                on: "{inputs.env}".into(),
                cases: vec![Case {
                    value: "prod".into(),
                    branch: Branch::new("prod").unwrap(),
                }],
                otherwise: Branch::new("other").unwrap(),
            },
            StepKind::Judge {
                state: "{steps.a.output.report}".into(),
                instructions: "Is the report good news?".into(),
                options: vec![JudgeOption {
                    branch: Branch::new("good").unwrap(),
                    meaning: "nothing needs a person".into(),
                }],
                otherwise: Branch::new("other").unwrap(),
                min_confidence: Some(0.8),
            },
            StepKind::ForEach {
                items: "{steps.a.output.items}".into(),
                max_iterations: 5,
            },
            StepKind::While {
                when: Condition::Between {
                    from_hour: 0,
                    to_hour: 23,
                },
                max_iterations: 5,
            },
            StepKind::Connector {
                connector: Some(ConnectorId::new("slack").unwrap()),
                operation: Some(OperationId::new("post_message").unwrap()),
                account: Some(ValueRef::Fixed(AccountId::from_ulid(
                    ulid::Ulid::from_parts(3, 3),
                ))),
                params: BTreeMap::from([("text".to_string(), "hello {inputs.x}".to_string())]),
                output_schema: Some(json!({ "type": "object" })),
                // Every optional field is *present* in a sample: a field the
                // wire skips at its default (`unattended: false`) would never
                // reach the table check below, and a kind's table missing it
                // would refuse every saved step that carries it.
                unattended: true,
            },
            StepKind::Wait {
                until: WaitFor::Release,
            },
            StepKind::Notify {
                scope: Some("general".into()),
                template: "t".into(),
                mentions: vec![who.clone()],
                author: Some(who.clone()),
            },
            StepKind::Spawn {
                statement_template: "s".into(),
                workflow: Some(WorkflowId::from_ulid(ulid::Ulid::from_parts(1, 2))),
                assignees: vec![who],
                inputs: BTreeMap::from([("report".to_string(), "{inputs.env}".to_string())]),
                wait: true,
            },
            StepKind::End {
                finish: Finish::Done,
            },
        ];
        assert_eq!(samples.len(), StepKind::NAMES.len());
        for kind in samples {
            let tag = kind.as_str();
            let s = step("s", kind, vec![Flow::to(sid("t"))]);
            let value = serde_json::to_value(&s).unwrap();
            let keys: BTreeSet<&str> = value
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .filter(|k| !STEP_FIELDS.contains(k))
                .collect();
            let table: BTreeSet<&str> = StepKind::fields_of(tag).unwrap().iter().copied().collect();
            assert_eq!(keys, table, "kind `{tag}`");
            // And the wire reads back through the checking deserializer.
            let back: Step = serde_json::from_value(value).unwrap();
            assert_eq!(back, s);
        }
        assert!(StepKind::fields_of("nope").is_none());
        for tag in StepKind::NAMES {
            assert!(StepKind::fields_of(tag).is_some(), "{tag}");
        }
        // The same for inputs.
        let choice = input(
            "c",
            InputKind::Choice {
                options: vec!["a".into()],
            },
        );
        let value = serde_json::to_value(&choice).unwrap();
        let keys: BTreeSet<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .filter(|k| !INPUT_FIELDS.contains(k))
            .collect();
        assert_eq!(keys, BTreeSet::from(["options"]));
        for tag in InputKind::NAMES {
            assert!(InputKind::fields_of(tag).is_some(), "{tag}");
        }
    }

    #[test]
    fn a_judge_offers_two_options_and_a_confidence_from_zero_to_one() {
        let option = |b: &str| JudgeOption {
            branch: Branch::new(b).unwrap(),
            meaning: format!("what {b} means"),
        };
        let judge = |options: Vec<JudgeOption>, min_confidence| StepKind::Judge {
            state: "the report".into(),
            instructions: "Which way?".into(),
            options,
            otherwise: Branch::new("unsure").unwrap(),
            min_confidence,
        };
        let kind = judge(vec![option("left"), option("right")], Some(0.8));
        assert_eq!(kind.as_str(), "judge");
        assert_eq!(
            kind.branches().unwrap(),
            ["left", "right", "unsure"].map(|b| Branch::new(b).unwrap())
        );
        assert_eq!(kind.templates(), vec!["the report", "Which way?"]);
        assert!(kind.conditions().is_empty());
        assert!(step("j", kind.clone(), vec![])
            .summary()
            .to_string()
            .starts_with("judges Which way?"));

        let flows = |branches: &[&str]| -> Vec<Flow> {
            branches
                .iter()
                .map(|b| Flow {
                    branch: Some(Branch::new(*b).unwrap()),
                    ..Flow::to(sid("end"))
                })
                .collect()
        };
        let problems_of = |kind: StepKind, branches: &[&str]| -> Vec<ProblemKind> {
            let wf = workflow(vec![
                step("j", kind, flows(branches)),
                step(
                    "end",
                    StepKind::End {
                        finish: Finish::Done,
                    },
                    vec![],
                ),
            ]);
            wf.validate(&ValidationCtx::default())
                .into_iter()
                .map(|p| p.kind)
                .collect()
        };
        assert!(problems_of(kind, &["left", "right", "unsure"]).is_empty());
        assert!(
            problems_of(judge(vec![option("left")], None), &["left", "unsure"])
                .contains(&ProblemKind::EmptyRules)
        );
        assert!(problems_of(
            judge(vec![option("left"), option("right")], Some(1.5)),
            &["left", "right", "unsure"]
        )
        .contains(&ProblemKind::BadQuestion));
        assert!(problems_of(
            judge(vec![option("left"), option("right")], None),
            &["left", "unsure"]
        )
        .contains(&ProblemKind::RuleWithoutFlow));
    }

    #[test]
    fn every_fixed_branch_word_is_a_valid_branch() {
        // `branch::of` builds these without checking, so the check lives here.
        for word in [
            branch::YES,
            branch::NO,
            branch::EACH,
            branch::DONE,
            branch::LOOP,
        ] {
            assert_eq!(Branch::new(word).unwrap(), branch::of(word), "{word}");
        }
    }

    // ----- what a step can count on at entry ------------------------------

    fn present(wf: &Workflow, id: &str) -> BTreeSet<StepId> {
        wf.assurance()
            .get(&sid(id))
            .map(|a| a.present.clone())
            .unwrap_or_default()
    }

    fn settled(wf: &Workflow, id: &str) -> BTreeSet<StepId> {
        wf.assurance()
            .get(&sid(id))
            .map(|a| a.settled.clone())
            .unwrap_or_default()
    }

    fn ids(list: &[&str]) -> BTreeSet<StepId> {
        list.iter().map(|s| sid(s)).collect()
    }

    fn human_step(id: &str, then: &[&str]) -> Step {
        step(
            id,
            StepKind::Human {
                prompt: "?".into(),
                options: vec![AskOption::new("yes", "Yes"), AskOption::new("no", "No")],
                multi: false,
                assignee: None,
            },
            then.iter().map(|t| Flow::to(sid(t))).collect(),
        )
    }

    fn check_step(id: &str, then: &[&str]) -> Step {
        step(
            id,
            StepKind::Check {
                check: CheckKind::Command {
                    command: "true".into(),
                },
            },
            then.iter().map(|t| Flow::to(sid(t))).collect(),
        )
    }

    fn decide_step(
        id: &str,
        rules: Vec<(&str, Condition)>,
        otherwise: &str,
        then: Vec<Flow>,
    ) -> Step {
        let mut d = decide(rules, otherwise, then);
        d.id = sid(id);
        d
    }

    fn always() -> Condition {
        Condition::Between {
            from_hour: 0,
            to_hour: 23,
        }
    }

    fn labelled(to: &str, branch: &str) -> Flow {
        Flow::branch(sid(to), Branch::new(branch).unwrap())
    }

    #[test]
    fn assured_is_the_forward_chain_on_a_straight_line() {
        let wf = workflow(vec![
            agent("a", &["b"]),
            agent("b", &["c"]),
            agent("c", &[]),
        ]);
        assert_eq!(present(&wf, "a"), ids(&[]));
        assert_eq!(present(&wf, "b"), ids(&["a"]));
        assert_eq!(present(&wf, "c"), ids(&["a", "b"]));
        assert_eq!(settled(&wf, "c"), ids(&["a", "b"]));
    }

    /// The reported bug's shape: the head of a rework loop cannot count on
    /// what the loop's later steps produce — on the first pass they have
    /// not run — while the steps after it count on it.
    #[test]
    fn assured_stops_at_a_loop_edge() {
        let wf = workflow(vec![
            agent("implement", &["review"]),
            promising(agent("review", &["verdict"]), &["verdict", "findings"]),
            decide_step(
                "verdict",
                vec![("approve", always())],
                "changes",
                vec![
                    labelled("ship", "approve"),
                    labelled("implement", "changes"),
                ],
            ),
            agent("ship", &[]),
        ]);
        assert!(
            wf.ancestors(&sid("implement")).contains(&sid("review")),
            "reachable back along the loop"
        );
        assert_eq!(
            present(&wf, "implement"),
            ids(&[]),
            "but not on the first pass"
        );
        assert_eq!(present(&wf, "review"), ids(&["implement"]));
        assert_eq!(
            present(&wf, "ship"),
            ids(&["implement", "review", "verdict"])
        );
    }

    #[test]
    fn assured_after_a_decide_is_the_common_prefix() {
        let wf = workflow(vec![
            agent("s", &["d"]),
            decide_step(
                "d",
                vec![("yes", always())],
                "no",
                vec![labelled("a", "yes"), labelled("b", "no")],
            ),
            agent("a", &["c"]),
            agent("b", &["c"]),
            agent("c", &[]),
        ]);
        assert_eq!(present(&wf, "a"), ids(&["s", "d"]));
        assert_eq!(
            present(&wf, "c"),
            ids(&["s", "d"]),
            "neither arm is sure at the fan-in"
        );
    }

    #[test]
    fn assured_through_a_plain_fan_in_is_the_union() {
        let mut wf = workflow(vec![
            agent("s", &["a", "b"]),
            agent("a", &["c"]),
            agent("b", &["c"]),
            agent("c", &[]),
        ]);
        assert_eq!(
            present(&wf, "c"),
            ids(&["s", "a", "b"]),
            "an all join waits for both arms"
        );
        wf.steps[3].join = Join::Any;
        assert_eq!(
            present(&wf, "c"),
            ids(&["s"]),
            "an any join enters on the first"
        );
        wf.steps[3].join = Join::One;
        assert_eq!(
            present(&wf, "c"),
            ids(&["s", "a", "b"]),
            "a one join waits like all"
        );
    }

    /// product-launch's shape: a fan-in behind a decide still counts on every
    /// arm, because the arms are sure once the step the decide led to is.
    #[test]
    fn assured_fan_in_behind_a_decide_is_still_a_union() {
        let wf = workflow(vec![
            agent("s", &["d"]),
            decide_step(
                "d",
                vec![("go", always())],
                "back",
                vec![labelled("r", "go"), labelled("s", "back")],
            ),
            agent("r", &["a", "b"]),
            agent("a", &["j"]),
            agent("b", &["j"]),
            agent("j", &[]),
        ]);
        assert_eq!(present(&wf, "j"), ids(&["s", "d", "r", "a", "b"]));
    }

    #[test]
    fn assured_mixed_join_adds_only_the_sure_arms() {
        let wf = workflow(vec![
            agent("s", &["a", "d"]),
            agent("a", &["j"]),
            decide_step(
                "d",
                vec![("yes", always())],
                "no",
                vec![labelled("b", "yes"), labelled("e", "no")],
            ),
            agent("b", &["j"]),
            agent("e", &[]),
            agent("j", &[]),
        ]);
        assert_eq!(
            present(&wf, "j"),
            ids(&["s", "a"]),
            "b may never arrive; a always does"
        );
    }

    #[test]
    fn assured_excludes_a_step_that_may_fail_and_skip() {
        let mut wf = workflow(vec![agent("a", &["b"]), agent("b", &[])]);
        wf.steps[0].on_fail = OnFail::Skip;
        assert_eq!(
            present(&wf, "b"),
            ids(&[]),
            "a may have failed with no output"
        );
        assert_eq!(
            settled(&wf, "b"),
            ids(&["a"]),
            "but it has settled either way"
        );
    }

    #[test]
    fn assured_excludes_the_failed_step_on_its_fail_route() {
        let mut probe = check_step("probe", &["healthy"]);
        probe.on_fail = OnFail::Then {
            step: sid("diagnose"),
        };
        let wf = workflow(vec![probe, agent("healthy", &[]), agent("diagnose", &[])]);
        assert_eq!(present(&wf, "diagnose"), ids(&[]));
        assert_eq!(settled(&wf, "diagnose"), ids(&["probe"]));
        assert_eq!(present(&wf, "healthy"), ids(&["probe"]));
    }

    #[test]
    fn assured_of_a_loop_body_includes_the_loop_step() {
        let wf = for_each_wf("{steps.list.output.items}", 3, &["each"]);
        assert_eq!(present(&wf, "body"), ids(&["list", "each"]));
        assert_eq!(present(&wf, "done"), ids(&["list", "each"]));
        let (body, exit) = wf.loop_sides(&sid("each"));
        assert_eq!(body, ids(&["body"]));
        assert_eq!(exit, ids(&["done"]));
        assert_eq!(
            wf.loop_sides(&sid("list")),
            (ids(&[]), ids(&[])),
            "a plain step has no sides"
        );
    }

    // ----- what validation refuses -----------------------------------------

    fn problems_of(wf: &Workflow, kind: ProblemKind) -> Vec<String> {
        wf.validate(&ctx())
            .into_iter()
            .filter(|p| p.kind == kind)
            .map(|p| p.text.to_string())
            .collect()
    }

    #[test]
    fn a_loop_head_cannot_read_what_runs_after_it() {
        let wf = workflow(vec![
            agent_reading(
                "implement",
                "Address {steps.review.output.findings}",
                &["review"],
            ),
            promising(agent("review", &["verdict"]), &["verdict", "findings"]),
            decide_step(
                "verdict",
                vec![("approve", always())],
                "changes",
                vec![
                    labelled("ship", "approve"),
                    labelled("implement", "changes"),
                ],
            ),
            agent("ship", &[]),
        ]);
        let found = problems_of(&wf, ProblemKind::NotAssured);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("first pass of the loop"), "{}", found[0]);
        assert!(
            found[0].contains("give the second pass its own step"),
            "{}",
            found[0]
        );
        assert!(
            !kinds(&wf.validate(&ctx())).contains(&ProblemKind::NotUpstream),
            "it is an ancestor"
        );
        // Read where it is sure to have run: clean.
        let mut wf = wf;
        wf.steps[0] = agent("implement", &["review"]);
        wf.steps[3] = agent_reading("ship", "Ship with {steps.review.output.findings}", &[]);
        assert_eq!(wf.validate(&ctx()), vec![]);
    }

    #[test]
    fn a_join_after_a_decide_cannot_read_one_arm() {
        let wf = workflow(vec![
            agent("s", &["d"]),
            decide_step(
                "d",
                vec![("yes", always())],
                "no",
                vec![labelled("a", "yes"), labelled("b", "no")],
            ),
            promising(agent("a", &["c"]), &["x"]),
            agent("b", &["c"]),
            agent_reading("c", "{steps.a.output.x}", &[]),
        ]);
        let found = problems_of(&wf, ProblemKind::NotAssured);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].contains("not every way in has run"),
            "{}",
            found[0]
        );
    }

    #[test]
    fn an_any_join_cannot_read_an_arm_but_an_all_join_can() {
        let mut wf = workflow(vec![
            agent("s", &["venue", "comms"]),
            human_step("venue", &["ready"]),
            agent("comms", &["ready"]),
            agent_reading("ready", "Venue: {steps.venue.answer}", &[]),
        ]);
        assert_eq!(
            wf.validate(&ctx()),
            vec![],
            "an all join waits for the venue's answer"
        );
        wf.steps[3].join = Join::Any;
        let found = problems_of(&wf, ProblemKind::NotAssured);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("`any` join"), "{}", found[0]);
    }

    #[test]
    fn a_fail_route_target_cannot_read_the_failed_step_but_may_read_its_outcome() {
        let mut probe = check_step("probe", &["healthy"]);
        probe.on_fail = OnFail::Then {
            step: sid("diagnose"),
        };
        let wf = workflow(vec![
            probe,
            agent("healthy", &[]),
            agent_reading(
                "diagnose",
                "It said {steps.probe.output.evidence}",
                &["route"],
            ),
            decide_step(
                "route",
                vec![("failed", outcome_of("probe", false))],
                "fine",
                vec![labelled("fix", "failed"), labelled("fine", "fine")],
            ),
            agent("fix", &[]),
            agent("fine", &[]),
        ]);
        let found = problems_of(&wf, ProblemKind::NotAssured);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].contains("may have failed and been passed over"),
            "{}",
            found[0]
        );
        assert!(
            !kinds(&wf.validate(&ctx())).contains(&ProblemKind::NotUpstream),
            "the outcome rule is fine: {:?}",
            wf.validate(&ctx())
        );
    }

    #[test]
    fn a_skipped_failure_blocks_a_placeholder_but_not_a_condition() {
        let mut a = promising(agent("a", &["b", "d"]), &["ok"]);
        a.on_fail = OnFail::Skip;
        let wf = workflow(vec![
            a,
            agent_reading("b", "{steps.a.output}", &[]),
            decide_step(
                "d",
                vec![(
                    "yes",
                    Condition::OutputEquals {
                        step: sid("a"),
                        path: "ok".into(),
                        value: json!(true),
                    },
                )],
                "no",
                vec![labelled("y", "yes"), labelled("n", "no")],
            ),
            agent("y", &[]),
            agent("n", &[]),
        ]);
        let problems = wf.validate(&ctx());
        assert_eq!(
            kinds(&problems),
            vec![ProblemKind::NotAssured],
            "{problems:?}"
        );
        assert_eq!(problems[0].step.as_ref().map(|s| s.as_str()), Some("b"));
    }

    #[test]
    fn a_while_condition_may_read_its_own_body() {
        // "loop until the body says done": the body has not run on the first
        // entry, the rule reads false, and the loop goes round — a rule is
        // not held to a step's presence.
        let mut hold = step(
            "again",
            StepKind::While {
                when: Condition::Not {
                    of: Box::new(Condition::OutputEquals {
                        step: sid("body"),
                        path: "done".into(),
                        value: json!(true),
                    }),
                },
                max_iterations: 5,
            },
            vec![labelled("body", "loop"), labelled("after", "done")],
        );
        hold.join = Join::All;
        let wf = workflow(vec![
            hold,
            promising(agent("body", &["again"]), &["done"]),
            agent("after", &[]),
        ]);
        assert_eq!(wf.validate(&ctx()), vec![]);
    }

    #[test]
    fn a_step_reading_itself_in_a_loop_is_not_upstream() {
        let wf = workflow(vec![
            agent_reading("a", "{steps.a.output}", &["d"]),
            decide_step(
                "d",
                vec![("again", always())],
                "stop",
                vec![labelled("a", "again"), labelled("e", "stop")],
            ),
            agent("e", &[]),
        ]);
        let k = kinds(&wf.validate(&ctx()));
        assert!(k.contains(&ProblemKind::NotUpstream), "{k:?}");
        assert!(!k.contains(&ProblemKind::NotAssured), "{k:?}");
    }

    #[test]
    fn reading_the_output_of_a_kind_that_yields_none() {
        let mut approval = step(
            "ok",
            StepKind::Approval { prompt: "?".into() },
            vec![Flow::to(sid("r"))],
        );
        approval.on_fail = OnFail::Fail;
        for (producer, tmpl, words) in [
            (
                decide_step(
                    "p",
                    vec![("yes", always())],
                    "no",
                    vec![labelled("r", "yes"), labelled("r", "no")],
                ),
                "{steps.p.output}",
                "is decide and yields none",
            ),
            (
                approval,
                "{steps.ok.output.x}",
                "is approval and yields none",
            ),
            (
                step(
                    "p",
                    StepKind::Wait {
                        until: WaitFor::Delay {
                            secs: ValueRef::Fixed(1),
                        },
                    },
                    vec![Flow::to(sid("r"))],
                ),
                "{steps.p.output}",
                "is wait and yields none",
            ),
            (
                human_step("p", &["r"]),
                "{steps.p.output.findings}",
                "yields an answer, not an output; read {steps.p.answer}",
            ),
            (
                agent("p", &["r"]),
                "{steps.p.answer}",
                "only a human step answers",
            ),
        ] {
            let wf = workflow(vec![producer, agent_reading("r", tmpl, &[])]);
            let found = problems_of(&wf, ProblemKind::NoSuchOutput);
            assert_eq!(found.len(), 1, "{tmpl}: {:?}", wf.validate(&ctx()));
            assert!(found[0].contains(words), "{tmpl}: {}", found[0]);
        }
        // A wait on a signal yields its payload, any path.
        let wf = workflow(vec![
            step(
                "hook",
                StepKind::Wait {
                    until: WaitFor::Signal {
                        filter: SignalFilter {
                            name: "deploy".into(),
                            fields: BTreeMap::new(),
                        },
                    },
                },
                vec![Flow::to(sid("r"))],
            ),
            agent_reading("r", "{steps.hook.output.payload.pr}", &[]),
        ]);
        assert_eq!(wf.validate(&ctx()), vec![]);
    }

    #[test]
    fn an_output_path_must_be_a_required_field() {
        let reader = |tmpl: &str| agent_reading("r", tmpl, &[]);
        let optional = {
            let mut a = agent("a", &["r"]);
            if let StepKind::Agent { output_schema, .. } = &mut a.kind {
                *output_schema = Some(json!({
                    "type": "object",
                    "required": ["verdict"],
                    "properties": { "verdict": { "type": "string" }, "findings": { "type": "array" } }
                }));
            }
            a
        };
        let wf = workflow(vec![optional.clone(), reader("{steps.a.output.findings}")]);
        let found = problems_of(&wf, ProblemKind::UnpromisedOutput);
        assert_eq!(found.len(), 1, "{:?}", wf.validate(&ctx()));
        assert!(found[0].contains("does not require"), "{}", found[0]);
        assert!(
            found[0].contains("add `findings` to `required`"),
            "{}",
            found[0]
        );
        let wf = workflow(vec![optional.clone(), reader("{steps.a.output.verdict}")]);
        assert_eq!(wf.validate(&ctx()), vec![], "a required field");
        let wf = workflow(vec![optional, reader("{steps.a.output}")]);
        assert_eq!(
            wf.validate(&ctx()),
            vec![],
            "the whole output needs no promise"
        );
        let wf = workflow(vec![
            agent("a", &["r"]),
            reader("{steps.a.output.findings}"),
        ]);
        let found = problems_of(&wf, ProblemKind::UnpromisedOutput);
        assert_eq!(found.len(), 1, "{:?}", wf.validate(&ctx()));
        assert!(
            found[0].contains("declares no output_schema"),
            "{}",
            found[0]
        );
        // A rule's path is held to the same promise.
        let wf = workflow(vec![
            agent("a", &["d"]),
            decide_step(
                "d",
                vec![(
                    "yes",
                    Condition::OutputEquals {
                        step: sid("a"),
                        path: "ok".into(),
                        value: json!(true),
                    },
                )],
                "no",
                vec![labelled("y", "yes"), labelled("n", "no")],
            ),
            agent("y", &[]),
            agent("n", &[]),
        ]);
        assert_eq!(
            kinds(&wf.validate(&ctx())),
            vec![ProblemKind::UnpromisedOutput]
        );
    }

    #[test]
    fn fixed_shape_kinds_promise_their_keys() {
        let reads = |producer: Step, tmpl: &str| {
            let mut producer = producer;
            producer.then = vec![Flow::to(sid("r"))];
            workflow(vec![producer, agent_reading("r", tmpl, &[])]).validate(&ctx())
        };
        let notify = step(
            "n",
            StepKind::Notify {
                scope: None,
                template: "t".into(),
                mentions: vec![],
                author: None,
            },
            vec![],
        );
        let spawn = |wait: bool| {
            step(
                "sp",
                StepKind::Spawn {
                    statement_template: "child".into(),
                    workflow: None,
                    assignees: vec![],
                    inputs: Default::default(),
                    wait,
                },
                vec![],
            )
        };
        assert_eq!(
            reads(check_step("c", &[]), "{steps.c.output.evidence}"),
            vec![]
        );
        assert_eq!(
            kinds(&reads(check_step("c", &[]), "{steps.c.output.passed}")),
            vec![ProblemKind::UnpromisedOutput]
        );
        assert_eq!(reads(notify.clone(), "{steps.n.output.message}"), vec![]);
        assert_eq!(reads(spawn(true), "{steps.sp.output.outcome}"), vec![]);
        assert_eq!(
            kinds(&reads(spawn(false), "{steps.sp.output.outcome}")),
            vec![ProblemKind::UnpromisedOutput]
        );
        assert_eq!(reads(spawn(false), "{steps.sp.output.child}"), vec![]);
        let found = reads(notify, "{steps.n.output.text}");
        assert_eq!(found.len(), 1);
        assert!(
            found[0]
                .text
                .to_string()
                .contains("a notify step's output has only `message`"),
            "{}",
            found[0].text
        );
        // A for_each promises the item on its body side and not past it.
        let wf = for_each_wf("{steps.list.output.items}", 3, &["each"]);
        let with_body = |tmpl: &str| {
            let mut wf = wf.clone();
            wf.steps[2] = agent_reading("body", tmpl, &["each"]);
            wf.steps[2].max_visits = 1;
            wf.validate(&ctx())
        };
        let with_done = |tmpl: &str| {
            let mut wf = wf.clone();
            wf.steps[3] = agent_reading("done", tmpl, &[]);
            wf.validate(&ctx())
        };
        assert_eq!(with_body("{steps.each.output.item.key}"), vec![]);
        assert_eq!(with_body("{steps.each.output.index}"), vec![]);
        assert_eq!(with_done("{steps.each.output.count}"), vec![]);
        assert_eq!(
            kinds(&with_done("{steps.each.output.item}")),
            vec![ProblemKind::UnpromisedOutput]
        );
        assert_eq!(
            kinds(&with_body("{steps.each.output.items}")),
            vec![ProblemKind::UnpromisedOutput]
        );
    }

    #[test]
    fn problem_kind_all_is_exhaustive() {
        // The match is exhaustive, so a kind added to the enum without a row
        // here fails to compile; `ALL` is built by the same macro that
        // declares the enum, so the two cannot drift.
        let ordinal = |k: ProblemKind| -> usize {
            match k {
                ProblemKind::EmptyName => 0,
                ProblemKind::DuplicateStepId => 1,
                ProblemKind::DuplicateInput => 2,
                ProblemKind::NoStart => 3,
                ProblemKind::ManyStarts => 4,
                ProblemKind::UnknownStep => 5,
                ProblemKind::Unreachable => 6,
                ProblemKind::SelfFlow => 7,
                ProblemKind::BranchWithoutRule => 8,
                ProblemKind::RuleWithoutFlow => 9,
                ProblemKind::LabelledFlowOnPlainStep => 10,
                ProblemKind::UnlabelledFlowOnDecide => 11,
                ProblemKind::DuplicateBranch => 12,
                ProblemKind::NotUpstream => 13,
                ProblemKind::UnknownInput => 14,
                ProblemKind::InputKindMismatch => 15,
                ProblemKind::UnknownPlaceholder => 16,
                ProblemKind::BadTemplate => 17,
                ProblemKind::ZeroVisits => 18,
                ProblemKind::UnknownAssignee => 19,
                ProblemKind::UnknownWorkflow => 20,
                ProblemKind::UnknownProject => 21,
                ProblemKind::EndWithSuccessors => 22,
                ProblemKind::BadQuestion => 23,
                ProblemKind::UnusedInput => 24,
                ProblemKind::EmptyRules => 25,
                ProblemKind::UnknownHarness => 26,
                ProblemKind::UnknownModel => 27,
                ProblemKind::BadCron => 28,
                ProblemKind::BadSchema => 29,
                ProblemKind::SpawnCycle => 30,
                ProblemKind::NotifyScopeUnknown => 31,
                ProblemKind::BadCondition => 32,
                ProblemKind::LoopWithoutReturn => 33,
                ProblemKind::LoopExitReturns => 34,
                ProblemKind::ZeroIterations => 35,
                ProblemKind::UnknownConnector => 36,
                ProblemKind::UnknownOperation => 37,
                ProblemKind::MissingConnectorParam => 38,
                ProblemKind::UnknownAccount => 39,
                ProblemKind::ParamOnlyPlaceholder => 40,
                ProblemKind::Unfilled => 41,
                ProblemKind::NotifyAuthorNotAnAgent => 42,
                ProblemKind::NotAssured => 43,
                ProblemKind::NoSuchOutput => 44,
                ProblemKind::UnpromisedOutput => 45,
                ProblemKind::UngatedWrite => 46,
                ProblemKind::NeedsGoal => 47,
                ProblemKind::StartHasIncoming => 48,
                ProblemKind::ManyManualStarts => 49,
                ProblemKind::ManualStartConfigured => 50,
                ProblemKind::StartPlaceholder => 51,
                ProblemKind::BoundaryOnInstantStep => 52,
                ProblemKind::ReminderInterrupts => 53,
                ProblemKind::BadTimer => 54,
                ProblemKind::BadSignalName => 55,
                ProblemKind::BadPoll => 56,
                ProblemKind::UnknownTopic => 57,
                ProblemKind::SpawnNeedsManualEntry => 58,
                ProblemKind::UnsupportedEffort => 59,
                ProblemKind::SpawnInput => 60,
            }
        };
        let seen: BTreeSet<usize> = ProblemKind::ALL.iter().map(|k| ordinal(*k)).collect();
        assert_eq!(seen.len(), ProblemKind::ALL.len());
        assert_eq!(seen, (0..ProblemKind::ALL.len()).collect());
        // The wire spelling is snake_case, as the desktop reads it.
        assert_eq!(
            serde_json::to_value(ProblemKind::NotifyScopeUnknown).unwrap(),
            json!("notify_scope_unknown")
        );
        assert_eq!(
            serde_json::to_value(ProblemKind::NeedsGoal).unwrap(),
            json!("needs_goal")
        );
        // The newest kind is the last one, under its own word.
        assert_eq!(ProblemKind::ALL.last(), Some(&ProblemKind::SpawnInput));
        assert_eq!(
            serde_json::to_value(ProblemKind::SpawnInput).unwrap(),
            json!("spawn_input")
        );
    }

    /// A run of the workspace refuses, by step, every string that reads the
    /// goal it does not have; a goal's run refuses nothing of the kind, and
    /// neither does validation, since the definition itself is sound.
    #[test]
    fn a_workspace_run_refuses_every_step_that_reads_its_goal() {
        let reads = |text: &str| -> StepKind {
            StepKind::Agent {
                instructions: text.into(),
                assignee: None,
                project: None,
                harness: vec![],
                model: None,
                effort: None,
                output_schema: None,
                tier_ceiling: ToolTier::Write,
            }
        };
        let wf = workflow(vec![
            step(
                "brief",
                reads("Goal: {goal.statement}"),
                vec![Flow::to(sid("plain"))],
            ),
            step(
                "plain",
                reads("Do the thing."),
                vec![Flow::to(sid("titled"))],
            ),
            step(
                "titled",
                StepKind::Notify {
                    scope: None,
                    template: "Released {goal.title}".into(),
                    mentions: vec![],
                    author: None,
                },
                vec![],
            ),
        ]);
        let workspace = RunScope::Workspace {
            budget: crate::goal::Budget::default(),
        };
        let problems = wf.scope_problems(&workspace);
        assert_eq!(
            problems.iter().map(|p| p.kind).collect::<Vec<_>>(),
            vec![ProblemKind::NeedsGoal, ProblemKind::NeedsGoal]
        );
        assert_eq!(
            problems
                .iter()
                .filter_map(|p| p.step.as_ref().map(|s| s.to_string()))
                .collect::<Vec<_>>(),
            vec!["brief".to_string(), "titled".to_string()],
            "one problem per step that reads the goal, and none for the one that does not"
        );
        assert!(
            wf.scope_problems(&goal_scope()).is_empty(),
            "a goal's run reads its goal"
        );
        assert!(
            !kinds(&wf.validate(&ValidationCtx::default())).contains(&ProblemKind::NeedsGoal),
            "the definition is sound: only where it starts can refuse it"
        );
        // Binding asks the same question, so a run's start and turning the
        // workflow On refuse alike.
        assert_eq!(
            kinds(&wf.validate_bound(&workspace, &BTreeMap::new(), &[], &Picky)),
            vec![ProblemKind::NeedsGoal, ProblemKind::NeedsGoal]
        );
    }

    #[test]
    fn branching_kinds_list_their_branches_and_summaries() {
        let b = |s: &str| Branch::new(s).unwrap();
        let when = Condition::Outcome {
            step: sid("c"),
            passed: true,
        };
        let if_ = StepKind::If { when: when.clone() };
        assert_eq!(if_.branches(), Some(vec![b("yes"), b("no")]));
        assert_eq!(if_.loop_branches(), None);
        assert!(!if_.is_loop());
        let switch = StepKind::Switch {
            on: "{inputs.env}".into(),
            cases: vec![
                Case {
                    value: "prod".into(),
                    branch: b("live"),
                },
                Case {
                    value: "qa".into(),
                    branch: b("test"),
                },
            ],
            otherwise: b("other"),
        };
        assert_eq!(
            switch.branches(),
            Some(vec![b("live"), b("test"), b("other")])
        );
        let each = StepKind::ForEach {
            items: "{steps.list.output.items}".into(),
            max_iterations: 7,
        };
        assert_eq!(each.branches(), Some(vec![b("each"), b("done")]));
        assert_eq!(each.loop_branches(), Some((b("each"), b("done"))));
        assert!(each.is_loop());
        let while_ = StepKind::While {
            when: when.clone(),
            max_iterations: 2,
        };
        assert_eq!(while_.branches(), Some(vec![b("loop"), b("done")]));
        assert_eq!(while_.loop_branches(), Some((b("loop"), b("done"))));
        assert_eq!(
            StepKind::Approval { prompt: "?".into() }.branches(),
            None,
            "a plain kind labels nothing"
        );
        assert_eq!(if_.conditions(), vec![&when]);
        assert_eq!(while_.conditions(), vec![&when]);
        assert!(switch.conditions().is_empty());
        assert_eq!(each.templates(), vec!["{steps.list.output.items}"]);
        assert_eq!(switch.templates(), vec!["{inputs.env}"]);

        let mk = |kind: StepKind| step("s", kind, vec![]);
        assert_eq!(mk(if_).summary().to_string(), "if outcome: yes · no");
        assert_eq!(
            mk(switch).summary().to_string(),
            "switches on {inputs.env}: prod · qa · other (otherwise)"
        );
        assert_eq!(
            mk(each).summary().to_string(),
            "for each of {steps.list.output.items}, at most 7 times"
        );
        assert_eq!(
            mk(while_).summary().to_string(),
            "while outcome, at most 2 times"
        );
    }

    #[test]
    fn condition_combinators_hold_and_recurse() {
        let inputs = BTreeMap::from([("x".to_string(), json!("a"))]);
        let steps = BTreeMap::new();
        let ctx = ConditionCtx {
            inputs: &inputs,
            steps: &steps,
            hour: 12,
        };
        let yes = || Condition::InputEquals {
            input: InputName::new("x").unwrap(),
            value: json!("a"),
        };
        let no = || Condition::InputEquals {
            input: InputName::new("x").unwrap(),
            value: json!("b"),
        };
        assert!(Condition::All {
            of: vec![yes(), yes()]
        }
        .holds(&ctx));
        assert!(!Condition::All {
            of: vec![yes(), no()]
        }
        .holds(&ctx));
        assert!(Condition::Any {
            of: vec![no(), yes()]
        }
        .holds(&ctx));
        assert!(!Condition::Any {
            of: vec![no(), no()]
        }
        .holds(&ctx));
        assert!(Condition::One {
            of: vec![no(), yes()]
        }
        .holds(&ctx));
        assert!(
            !Condition::One {
                of: vec![yes(), yes()]
            }
            .holds(&ctx),
            "two is not one"
        );
        assert!(!Condition::One {
            of: vec![no(), no()]
        }
        .holds(&ctx));
        assert!(Condition::Not { of: Box::new(no()) }.holds(&ctx));
        assert!(!Condition::Not {
            of: Box::new(yes())
        }
        .holds(&ctx));
        // Empty combinators total: all is vacuously true, any and one are not.
        assert!(Condition::All { of: vec![] }.holds(&ctx));
        assert!(!Condition::Any { of: vec![] }.holds(&ctx));
        assert!(!Condition::One { of: vec![] }.holds(&ctx));
        // Nested.
        let nested = Condition::Not {
            of: Box::new(Condition::All {
                of: vec![
                    yes(),
                    Condition::Any {
                        of: vec![no(), no()],
                    },
                ],
            }),
        };
        assert!(nested.holds(&ctx));
    }

    #[test]
    fn condition_combinators_report_their_reads_and_names() {
        let tree = Condition::Not {
            of: Box::new(Condition::All {
                of: vec![
                    Condition::Outcome {
                        step: sid("c"),
                        passed: true,
                    },
                    Condition::Any {
                        of: vec![
                            Condition::InputEquals {
                                input: InputName::new("x").unwrap(),
                                value: json!(1),
                            },
                            Condition::Answered {
                                step: sid("q"),
                                option: "a".into(),
                            },
                        ],
                    },
                ],
            }),
        };
        assert_eq!(
            tree.leaves().iter().map(|c| c.as_str()).collect::<Vec<_>>(),
            vec!["outcome", "input_equals", "answered"]
        );
        assert_eq!(tree.reads_steps(), vec![&sid("c"), &sid("q")]);
        assert_eq!(tree.reads_inputs(), vec![&InputName::new("x").unwrap()]);
        assert_eq!(
            tree.depth(),
            4,
            "not > all > any > leaf: a leaf is 1, each combinator one more"
        );
        assert!(!tree.empty_combinator());
        assert!(Condition::Not {
            of: Box::new(Condition::Any { of: vec![] })
        }
        .empty_combinator());
        assert_eq!(tree.as_str(), "not");
        assert_eq!(Condition::One { of: vec![] }.as_str(), "one");
        // The wire: nested, tagged, and the same back through JSON and TOML.
        let json = serde_json::to_value(&tree).unwrap();
        assert_eq!(json["condition"], "not");
        assert_eq!(json["of"]["condition"], "all");
        assert_eq!(json["of"]["of"][1]["condition"], "any");
        assert_eq!(serde_json::from_value::<Condition>(json).unwrap(), tree);
        let toml = toml::to_string(&tree).unwrap();
        assert_eq!(toml::from_str::<Condition>(&toml).unwrap(), tree);
    }

    #[test]
    fn nested_condition_refs_are_validated() {
        // A leaf inside a combinator is judged as a rule's own leaf is: an
        // unknown step, a step that runs later, a wrong kind of step.
        let later = Condition::Outcome {
            step: sid("later"),
            passed: true,
        };
        let wrong_kind = Condition::Answered {
            step: sid("a"),
            option: "x".into(),
        };
        let mut wf = workflow(vec![
            agent("a", &["i"]),
            step(
                "i",
                StepKind::If {
                    when: Condition::All {
                        of: vec![later, wrong_kind],
                    },
                },
                vec![
                    Flow::branch(sid("later"), Branch::new("yes").unwrap()),
                    Flow::branch(sid("n"), Branch::new("no").unwrap()),
                ],
            ),
            step(
                "later",
                StepKind::Check {
                    check: CheckKind::Command {
                        command: "true".into(),
                    },
                },
                vec![],
            ),
            agent("n", &[]),
        ]);
        let k = kinds(&wf.validate(&ctx()));
        assert_eq!(
            k.iter().filter(|k| **k == ProblemKind::NotUpstream).count(),
            2,
            "{k:?}"
        );
        // A step nobody declared, read through a `not`.
        wf.steps[1].kind = StepKind::If {
            when: Condition::Not {
                of: Box::new(Condition::Outcome {
                    step: sid("zzz"),
                    passed: true,
                }),
            },
        };
        assert!(kinds(&wf.validate(&ctx())).contains(&ProblemKind::UnknownStep));
        // An input read through a combinator counts as read.
        wf.inputs = vec![input("x", InputKind::Text)];
        wf.steps[1].kind = StepKind::If {
            when: Condition::Any {
                of: vec![Condition::InputEquals {
                    input: InputName::new("x").unwrap(),
                    value: json!("a"),
                }],
            },
        };
        assert_eq!(wf.validate(&ctx()), vec![]);
    }

    #[test]
    fn a_condition_too_deep_or_empty_is_refused() {
        let if_step = |when: Condition| {
            workflow(vec![
                agent("a", &["i"]),
                step(
                    "i",
                    StepKind::If { when },
                    vec![
                        Flow::branch(sid("y"), Branch::new("yes").unwrap()),
                        Flow::branch(sid("n"), Branch::new("no").unwrap()),
                    ],
                ),
                agent("y", &[]),
                agent("n", &[]),
            ])
        };
        assert_eq!(
            kinds(&if_step(Condition::All { of: vec![] }).validate(&ctx())),
            vec![ProblemKind::BadCondition]
        );
        let between = || Condition::Between {
            from_hour: 0,
            to_hour: 23,
        };
        let mut deep = between();
        for _ in 0..8 {
            deep = Condition::Not { of: Box::new(deep) };
        }
        assert_eq!(deep.depth(), 9);
        assert_eq!(
            kinds(&if_step(deep).validate(&ctx())),
            vec![ProblemKind::BadCondition]
        );
        let mut fine = between();
        for _ in 0..7 {
            fine = Condition::Not { of: Box::new(fine) };
        }
        assert_eq!(fine.depth(), 8);
        assert_eq!(if_step(fine).validate(&ctx()), vec![]);
    }

    #[test]
    fn if_labels_exactly_yes_and_no() {
        let when = || Condition::Between {
            from_hour: 0,
            to_hour: 23,
        };
        let with = |then: Vec<Flow>| {
            workflow(vec![
                agent("a", &["i"]),
                step("i", StepKind::If { when: when() }, then),
                agent("y", &[]),
                agent("n", &[]),
            ])
        };
        let yes = Flow::branch(sid("y"), Branch::new("yes").unwrap());
        let no = Flow::branch(sid("n"), Branch::new("no").unwrap());
        assert_eq!(with(vec![yes.clone(), no.clone()]).validate(&ctx()), vec![]);
        // `n` is now unreachable too; the branch rule is what we look for.
        assert!(kinds(&with(vec![yes.clone()]).validate(&ctx()))
            .contains(&ProblemKind::RuleWithoutFlow));
        let extra = Flow::branch(sid("n"), Branch::new("maybe").unwrap());
        let k = kinds(&with(vec![yes.clone(), no.clone(), extra]).validate(&ctx()));
        assert!(k.contains(&ProblemKind::BranchWithoutRule), "{k:?}");
        let k = kinds(&with(vec![Flow::to(sid("y")), no]).validate(&ctx()));
        assert!(k.contains(&ProblemKind::UnlabelledFlowOnDecide), "{k:?}");
    }

    #[test]
    fn switch_cases_and_flows_agree() {
        let case = |value: &str, branch: &str| Case {
            value: value.into(),
            branch: Branch::new(branch).unwrap(),
        };
        let with = |on: &str, cases: Vec<Case>| {
            workflow(vec![
                promising(agent("a", &["s"]), &["env"]),
                step(
                    "s",
                    StepKind::Switch {
                        on: on.into(),
                        cases,
                        otherwise: Branch::new("other").unwrap(),
                    },
                    vec![
                        Flow::branch(sid("p"), Branch::new("prod").unwrap()),
                        Flow::branch(sid("o"), Branch::new("other").unwrap()),
                    ],
                ),
                agent("p", &[]),
                agent("o", &[]),
            ])
        };
        assert_eq!(
            with("{steps.a.output.env}", vec![case("prod", "prod")]).validate(&ctx()),
            vec![]
        );
        assert_eq!(
            kinds(&with("{steps.a.output.env}", vec![]).validate(&ctx())),
            vec![ProblemKind::BranchWithoutRule, ProblemKind::EmptyRules],
            "no case: the flow labelled `prod` is a branch the switch never chooses, and the switch is empty"
        );
        let k = kinds(
            &with(
                "{steps.a.output.env}",
                vec![case("prod", "prod"), case("prod", "prod")],
            )
            .validate(&ctx()),
        );
        assert_eq!(
            k.iter()
                .filter(|k| **k == ProblemKind::DuplicateBranch)
                .count(),
            2,
            "a duplicate branch and a duplicate value: {k:?}"
        );
        assert!(
            kinds(&with("{goal.statement}", vec![case("prod", "prod")]).validate(&ctx()))
                .contains(&ProblemKind::BadTemplate)
        );
        assert!(
            kinds(&with("{steps.p.output.env}", vec![case("prod", "prod")]).validate(&ctx()))
                .contains(&ProblemKind::NotUpstream)
        );
    }

    /// A loop: `list → each(for_each) → body → each`, `each → done`.
    fn for_each_wf(items: &str, max_iterations: u16, body_then: &[&str]) -> Workflow {
        let mut wf = workflow(vec![
            promising(agent("list", &["each"]), &["items"]),
            step(
                "each",
                StepKind::ForEach {
                    items: items.into(),
                    max_iterations,
                },
                vec![
                    Flow::branch(sid("body"), Branch::new("each").unwrap()),
                    Flow::branch(sid("done"), Branch::new("done").unwrap()),
                ],
            ),
            agent("body", body_then),
            agent("done", &[]),
        ]);
        wf.steps[2].max_visits = 1;
        wf
    }

    #[test]
    fn for_each_needs_a_body_that_returns_and_an_exit_that_does_not() {
        let ok = for_each_wf("{steps.list.output.items}", 3, &["each"]);
        assert_eq!(ok.validate(&ctx()), vec![]);
        assert_eq!(
            ok.loop_edges(),
            BTreeSet::from([(sid("body"), sid("each"))])
        );
        assert_eq!(
            kinds(&for_each_wf("{steps.list.output.items}", 3, &[]).validate(&ctx())),
            vec![ProblemKind::LoopWithoutReturn]
        );
        let mut exit_returns = for_each_wf("{steps.list.output.items}", 3, &["each"]);
        exit_returns.steps[3].then = vec![Flow::to(sid("body"))];
        assert!(kinds(&exit_returns.validate(&ctx())).contains(&ProblemKind::LoopExitReturns));
        assert_eq!(
            kinds(&for_each_wf("{steps.list.output.items}", 0, &["each"]).validate(&ctx())),
            vec![ProblemKind::ZeroIterations]
        );
        assert_eq!(
            kinds(&for_each_wf("{goal.title}", 3, &["each"]).validate(&ctx())),
            vec![ProblemKind::BadTemplate]
        );
    }

    #[test]
    fn while_labels_loop_and_done_and_needs_a_return() {
        let with = |then: Vec<Flow>, body_then: &[&str]| {
            workflow(vec![
                agent("a", &["w"]),
                step(
                    "w",
                    StepKind::While {
                        when: Condition::Between {
                            from_hour: 0,
                            to_hour: 23,
                        },
                        max_iterations: 3,
                    },
                    then,
                ),
                agent("body", body_then),
                agent("done", &[]),
            ])
        };
        let loop_ = Flow::branch(sid("body"), Branch::new("loop").unwrap());
        let done = Flow::branch(sid("done"), Branch::new("done").unwrap());
        assert_eq!(
            with(vec![loop_.clone(), done.clone()], &["w"]).validate(&ctx()),
            vec![]
        );
        assert_eq!(
            kinds(&with(vec![loop_.clone(), done.clone()], &[]).validate(&ctx())),
            vec![ProblemKind::LoopWithoutReturn]
        );
        let k = kinds(
            &with(
                vec![
                    loop_,
                    Flow::branch(sid("done"), Branch::new("exit").unwrap()),
                ],
                &["w"],
            )
            .validate(&ctx()),
        );
        assert!(
            k.contains(&ProblemKind::RuleWithoutFlow)
                && k.contains(&ProblemKind::BranchWithoutRule),
            "{k:?}"
        );
    }

    #[test]
    fn loop_body_is_the_steps_between_the_body_flow_and_the_return() {
        // A diamond body: each → b1, b2 → join → each; the exit side is out.
        let mut wf = for_each_wf("{steps.list.output.items}", 3, &[]);
        wf.steps[2].then = vec![Flow::to(sid("b1")), Flow::to(sid("b2"))];
        wf.steps.push(agent("b1", &["join"]));
        wf.steps.push(agent("b2", &["join"]));
        wf.steps.push(agent("join", &["each"]));
        assert_eq!(wf.validate(&ctx()), vec![]);
        assert_eq!(
            wf.loop_body(&sid("each")),
            BTreeSet::from([sid("body"), sid("b1"), sid("b2"), sid("join")])
        );
        assert!(
            wf.loop_body(&sid("list")).is_empty(),
            "a plain step has no body"
        );
        assert!(wf.loop_body(&sid("done")).is_empty());
    }

    #[test]
    fn unused_input_is_a_problem() {
        let mut wf = workflow(vec![agent("a", &[])]);
        wf.inputs = vec![input("x", InputKind::Text)];
        let problems = wf.validate(&ctx());
        assert_eq!(kinds(&problems), vec![ProblemKind::UnusedInput]);
        assert_eq!(problems[0].step, None);
        assert!(problems[0].text.to_string().contains("`x`"));

        // Read by a template: fine.
        let mut wf = workflow(vec![agent_reading("a", "do {inputs.x}", &[])]);
        wf.inputs = vec![input("x", InputKind::Text)];
        assert_eq!(wf.validate(&ctx()), vec![]);

        // Read by a decide rule: fine.
        let mut wf = workflow(vec![
            step(
                "d",
                StepKind::Decide {
                    rules: vec![Rule {
                        when: Condition::InputEquals {
                            input: InputName::new("x").unwrap(),
                            value: json!("yes"),
                        },
                        branch: Branch::new("go").unwrap(),
                    }],
                    otherwise: Branch::new("stop").unwrap(),
                    pick: Pick::First,
                },
                vec![
                    Flow::branch(sid("a"), Branch::new("go").unwrap()),
                    Flow::branch(sid("b"), Branch::new("stop").unwrap()),
                ],
            ),
            agent("a", &[]),
            agent("b", &[]),
        ]);
        wf.inputs = vec![input("x", InputKind::Text)];
        assert_eq!(wf.validate(&ctx()), vec![]);

        // Read by a value reference: fine.
        let mut wf = workflow(vec![agent("a", &[])]);
        wf.inputs = vec![input("who", InputKind::Assignee)];
        wf.steps[0].kind = StepKind::Agent {
            instructions: "x".into(),
            assignee: Some(ValueRef::Input {
                input: InputName::new("who").unwrap(),
            }),
            project: None,
            harness: vec![],
            model: None,
            effort: None,
            output_schema: None,
            tier_ceiling: ToolTier::Write,
        };
        assert_eq!(wf.validate(&ctx()), vec![]);
    }

    #[test]
    fn empty_rules_is_a_problem() {
        let wf = workflow(vec![
            step(
                "d",
                StepKind::Decide {
                    rules: vec![],
                    otherwise: Branch::new("only").unwrap(),
                    pick: Pick::First,
                },
                vec![Flow::branch(sid("a"), Branch::new("only").unwrap())],
            ),
            agent("a", &[]),
        ]);
        let problems = wf.validate(&ctx());
        assert_eq!(kinds(&problems), vec![ProblemKind::EmptyRules]);
        assert_eq!(problems[0].step, Some(sid("d")));
    }

    fn runtime_ctx<'a>(
        harnesses: &'a [String],
        models: &'a [(String, Vec<String>)],
    ) -> ValidationCtx<'a> {
        ValidationCtx {
            harnesses,
            models,
            ..ctx()
        }
    }

    fn pinned(harness: &[&str], model: Option<&str>) -> Workflow {
        let mut wf = workflow(vec![agent("a", &[])]);
        wf.steps[0].kind = StepKind::Agent {
            instructions: "x".into(),
            assignee: Some(ValueRef::Fixed(Assignee::Agent("developer".into()))),
            project: None,
            harness: harness.iter().map(|h| h.to_string()).collect(),
            model: model.map(str::to_string),
            effort: None,
            output_schema: None,
            tier_ceiling: ToolTier::Write,
        };
        wf
    }

    #[test]
    fn unknown_harness_and_model() {
        let harnesses = vec!["claude-code".to_string()];
        let models = vec![("claude-code".to_string(), vec!["m1".to_string()])];
        let rt = runtime_ctx(&harnesses, &models);

        let k = kinds(&pinned(&["codex"], None).validate(&rt));
        assert_eq!(k, vec![ProblemKind::UnknownHarness]);

        let k = kinds(&pinned(&["claude-code"], Some("zzz")).validate(&rt));
        assert_eq!(k, vec![ProblemKind::UnknownModel]);

        assert_eq!(pinned(&["claude-code"], Some("m1")).validate(&rt), vec![]);
        assert_eq!(pinned(&[], None).validate(&rt), vec![]);

        // With no runtime described at all, harness names are not checked.
        assert_eq!(pinned(&["codex"], None).validate(&ctx()), vec![]);
    }

    fn effort_ctx<'a>(
        harnesses: &'a [String],
        effort_harnesses: &'a [String],
    ) -> ValidationCtx<'a> {
        ValidationCtx {
            harnesses,
            effort_harnesses,
            ..ctx()
        }
    }

    fn pinned_effort(harness: &[&str], effort: Option<EffortChoice>) -> Workflow {
        let mut wf = pinned(harness, None);
        match &mut wf.steps[0].kind {
            StepKind::Agent { effort: pin, .. } => *pin = effort,
            other => panic!("{} pins no effort", other.as_str()),
        }
        wf
    }

    #[test]
    fn an_effort_pinned_where_no_harness_can_set_it_is_a_problem() {
        let harnesses = vec![
            "claude-code".to_string(),
            "codex".to_string(),
            "preset:goose".to_string(),
            "custom".to_string(),
        ];
        let takes = vec!["claude-code".to_string(), "codex".to_string()];
        let rt = effort_ctx(&harnesses, &takes);
        let high = Some(EffortChoice::High);

        // The one harness named has no control.
        let problems = pinned_effort(&["preset:goose"], high).validate(&rt);
        assert_eq!(kinds(&problems), vec![ProblemKind::UnsupportedEffort]);
        assert_eq!(problems[0].step, Some(sid("a")));
        assert_eq!(problems[0].text.id, "problem-unsupported-effort");
        assert_eq!(
            problems[0].text.get("effort"),
            Some(&crate::text::Arg::Str("high".into()))
        );
        assert_eq!(
            problems[0].text.get("harnesses"),
            Some(&crate::text::Arg::Str("preset:goose".into()))
        );
        // Neither of two has it: one problem, naming both.
        let problems = pinned_effort(&["preset:goose", "custom"], high).validate(&rt);
        assert_eq!(kinds(&problems), vec![ProblemKind::UnsupportedEffort]);
        assert_eq!(
            problems[0].text.get("harnesses"),
            Some(&crate::text::Arg::Str("preset:goose, custom".into()))
        );
        // `auto` is a pin like any level: nobody could run what the judge names.
        assert_eq!(
            kinds(&pinned_effort(&["custom"], Some(EffortChoice::Auto)).validate(&rt)),
            vec![ProblemKind::UnsupportedEffort]
        );

        // One that has the control is enough: the walk may land on it.
        assert_eq!(
            pinned_effort(&["preset:goose", "codex"], high).validate(&rt),
            vec![]
        );
        assert_eq!(pinned_effort(&["claude-code"], high).validate(&rt), vec![]);
        // No pin is nothing to check.
        assert_eq!(pinned_effort(&["preset:goose"], None).validate(&rt), vec![]);
        // No harness named: the agent's own, which a definition does not know.
        assert_eq!(pinned_effort(&[], high).validate(&rt), vec![]);
    }

    #[test]
    fn an_effort_pin_is_not_checked_against_a_runtime_nobody_described() {
        let high = Some(EffortChoice::High);
        // No runtime at all: an offline tool.
        assert_eq!(
            pinned_effort(&["preset:goose"], high).validate(&ctx()),
            vec![]
        );
        // The list of harnesses with the control, and no harnesses: still
        // nobody described the runtime.
        let takes = vec!["claude-code".to_string()];
        assert_eq!(
            pinned_effort(&["preset:goose"], high).validate(&effort_ctx(&[], &takes)),
            vec![]
        );
        // A harness the runtime cannot launch is that problem, once, and not
        // this one beside it.
        let harnesses = vec!["claude-code".to_string()];
        let rt = effort_ctx(&harnesses, &takes);
        assert_eq!(
            kinds(&pinned_effort(&["nowhere"], high).validate(&rt)),
            vec![ProblemKind::UnknownHarness]
        );
        // Among a known one without the control and an unknown one, the known
        // one is judged.
        let harnesses = vec!["claude-code".to_string(), "custom".to_string()];
        let rt = effort_ctx(&harnesses, &takes);
        assert_eq!(
            kinds(&pinned_effort(&["nowhere", "custom"], high).validate(&rt)),
            vec![ProblemKind::UnknownHarness, ProblemKind::UnsupportedEffort]
        );
    }

    #[test]
    fn a_steps_effort_reads_from_the_wire_in_its_own_word() {
        let wire = json!({
            "id": "a",
            "name": "A",
            "kind": "agent",
            "instructions": "x",
            "effort": "xhigh",
        });
        let step: Step = serde_json::from_value(wire).unwrap();
        assert!(matches!(
            step.kind,
            StepKind::Agent {
                effort: Some(EffortChoice::Xhigh),
                ..
            }
        ));
        assert_eq!(serde_json::to_value(&step).unwrap()["effort"], "xhigh");
        // Absent when nobody pinned one.
        let bare = pinned(&[], None);
        assert!(serde_json::to_value(&bare.steps[0])
            .unwrap()
            .get("effort")
            .is_none());
        // `auto` is a choice; a word that is no effort is refused, and so is
        // the key on a kind that runs no model.
        let auto = json!({ "id": "a", "name": "A", "kind": "agent", "instructions": "x", "effort": "auto" });
        assert!(serde_json::from_value::<Step>(auto).is_ok());
        let odd = json!({ "id": "a", "name": "A", "kind": "agent", "instructions": "x", "effort": "ultra" });
        assert!(serde_json::from_value::<Step>(odd).is_err());
        let human =
            json!({ "id": "h", "name": "H", "kind": "human", "prompt": "?", "effort": "high" });
        assert!(serde_json::from_value::<Step>(human).is_err());
    }

    #[test]
    fn model_check_skipped_when_harness_models_unknown() {
        let harnesses = vec!["claude-code".to_string()];
        let rt = runtime_ctx(&harnesses, &[]);
        assert_eq!(pinned(&["claude-code"], Some("zzz")).validate(&rt), vec![]);
        // A pin with no harness named has nothing to check against.
        assert_eq!(pinned(&[], Some("zzz")).validate(&rt), vec![]);
    }

    struct Picky;

    impl SyntaxChecks for Picky {
        fn cron_error(&self, expr: &str) -> Option<String> {
            (expr == "bad").then(|| format!("{expr:?} is not a cron expression"))
        }
        fn schema_error(&self, schema: &serde_json::Value) -> Option<String> {
            schema
                .get("bogus")
                .map(|_| "unknown keyword `bogus`".to_string())
        }
    }

    #[test]
    fn bad_cron_and_bad_schema_come_from_checks() {
        let picky = ValidationCtx {
            checks: &Picky,
            ..ctx()
        };
        let wf = workflow(vec![step(
            "w",
            StepKind::Wait {
                until: WaitFor::Schedule {
                    cron: ValueRef::Fixed("bad".into()),
                    tz: None,
                },
            },
            vec![],
        )]);
        assert_eq!(kinds(&wf.validate(&picky)), vec![ProblemKind::BadCron]);
        assert_eq!(
            wf.validate(&ctx()),
            vec![],
            "the default checks accept everything"
        );

        let wf = workflow(vec![
            agent("a", &["c"]),
            step(
                "c",
                StepKind::Check {
                    check: CheckKind::Schema {
                        schema: json!({ "bogus": 1 }),
                        of: Some(sid("a")),
                    },
                },
                vec![],
            ),
        ]);
        assert_eq!(kinds(&wf.validate(&picky)), vec![ProblemKind::BadSchema]);

        let mut wf = workflow(vec![agent("a", &[])]);
        wf.steps[0].kind = StepKind::Agent {
            instructions: "x".into(),
            assignee: Some(ValueRef::Fixed(Assignee::Agent("developer".into()))),
            project: None,
            harness: vec![],
            model: None,
            effort: None,
            output_schema: Some(json!({ "bogus": 1 })),
            tier_ceiling: ToolTier::Write,
        };
        assert_eq!(kinds(&wf.validate(&picky)), vec![ProblemKind::BadSchema]);

        // A cron read from an input is checked when the run resolves it.
        let mut wf = workflow(vec![step(
            "w",
            StepKind::Wait {
                until: WaitFor::Schedule {
                    cron: ValueRef::Input {
                        input: InputName::new("when").unwrap(),
                    },
                    tz: None,
                },
            },
            vec![],
        )]);
        wf.inputs = vec![input("when", InputKind::Text)];
        assert_eq!(wf.validate(&picky), vec![]);
    }

    #[test]
    fn spawn_cycle_through_two_workflows() {
        let a = workflow(vec![]).id;
        let b = WorkflowId::from_ulid(ulid::Ulid::from_parts(9, 2));
        let c = WorkflowId::from_ulid(ulid::Ulid::from_parts(9, 3));
        let wf = workflow(vec![step(
            "s",
            StepKind::Spawn {
                statement_template: "child".into(),
                workflow: Some(b),
                assignees: vec![],
                inputs: Default::default(),
                wait: true,
            },
            vec![],
        )]);
        let workflows = vec![a, b, c];
        let cyclic = vec![(b, vec![c]), (c, vec![a])];
        let k = kinds(&wf.validate(&ValidationCtx {
            workflows: &workflows,
            spawns: &cyclic,
            ..ctx()
        }));
        assert_eq!(k, vec![ProblemKind::SpawnCycle]);

        let acyclic = vec![(b, vec![c]), (c, vec![])];
        assert_eq!(
            wf.validate(&ValidationCtx {
                workflows: &workflows,
                spawns: &acyclic,
                ..ctx()
            }),
            vec![]
        );

        // A workflow spawning itself is the shortest cycle.
        let selfish = workflow(vec![step(
            "s",
            StepKind::Spawn {
                statement_template: "child".into(),
                workflow: Some(a),
                assignees: vec![],
                inputs: Default::default(),
                wait: true,
            },
            vec![],
        )]);
        let k = kinds(&selfish.validate(&ValidationCtx {
            workflows: &workflows,
            ..ctx()
        }));
        assert_eq!(k, vec![ProblemKind::SpawnCycle]);
    }

    /// A connector definition as validation sees it: one operation with a
    /// required `text` parameter, bearer auth, no account unless the test
    /// adds one.
    fn slack() -> Connector {
        use crate::connector::{HttpMethod, Operation, OutputSpec, ParamDef, ParamKind};
        Connector {
            id: ConnectorId::new("slack").unwrap(),
            name: "Slack".into(),
            description: "Slack.".into(),
            tags: Tags::default(),
            origin: crate::origin::Origin::Local,
            base_url: "https://slack.com".into(),
            hosts: vec!["slack.com".into()],
            insecure_tls: false,
            auth: crate::connector::AuthScheme::Bearer,
            params: vec![],
            operations: vec![Operation {
                id: OperationId::new("post_message").unwrap(),
                name: "Post a message".into(),
                description: "Posts.".into(),
                method: HttpMethod::Post,
                path: "/api/chat.postMessage".into(),
                query: BTreeMap::new(),
                headers: BTreeMap::new(),
                body: None,
                params: vec![
                    ParamDef {
                        name: InputName::new("text").unwrap(),
                        label: "Text".into(),
                        kind: ParamKind::Text,
                        required: true,
                        doc: "The message.".into(),
                    },
                    ParamDef {
                        name: InputName::new("thread").unwrap(),
                        label: "Thread".into(),
                        kind: ParamKind::Text,
                        required: false,
                        doc: "A thread.".into(),
                    },
                ],
                output: OutputSpec::default(),
                writes: true,
                timeout_secs: None,
                idempotency: None,
                page: None,
            }],
            check: None,
            revision: 0,
            created_at: 0,
        }
    }

    fn account(connector: &str, default: bool) -> ConnectorAccount {
        ConnectorAccount {
            id: AccountId::from_ulid(ulid::Ulid::from_parts(7, 7)),
            connector: ConnectorId::new(connector).unwrap(),
            label: "work".into(),
            params: BTreeMap::new(),
            default,
            auth: Default::default(),
            created_at: 0,
        }
    }

    fn connector_step(
        id: &str,
        params: &[(&str, &str)],
        account: Option<ValueRef<AccountId>>,
    ) -> Step {
        step(
            id,
            StepKind::Connector {
                connector: Some(ConnectorId::new("slack").unwrap()),
                operation: Some(OperationId::new("post_message").unwrap()),
                account,
                params: params
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
                output_schema: None,
                // The fixture's write is the person's word here; the gate rule
                // has its own tests below.
                unattended: true,
            },
            vec![],
        )
    }

    /// A write is gated or owned: an `approval` or `human` step upstream, or
    /// the step's own `unattended`. Silence is refused.
    #[test]
    fn a_write_with_no_gate_upstream_and_no_unattended_word_is_refused() {
        let connectors = vec![slack()];
        let accounts = vec![account("slack", true)];
        let vctx = ValidationCtx {
            connectors: &connectors,
            accounts: &accounts,
            ..ctx()
        };
        let mut post = connector_step("post", &[("text", "hi")], None);
        if let StepKind::Connector { unattended, .. } = &mut post.kind {
            *unattended = false;
        }
        // Alone: refused, by name.
        let wf = workflow(vec![post.clone()]);
        let problems = wf.validate(&vctx);
        assert_eq!(kinds(&problems), vec![ProblemKind::UngatedWrite]);
        assert!(
            problems[0].text.to_string().contains("slack.post_message"),
            "{}",
            problems[0].text
        );
        assert!(
            problems[0].text.to_string().contains("unattended: true"),
            "{}",
            problems[0].text
        );
        assert_eq!(problems[0].step.as_ref(), Some(&sid("post")));

        // An approval upstream gates it.
        let gated = workflow(vec![
            step(
                "ok",
                StepKind::Approval {
                    prompt: "Post it?".into(),
                },
                vec![Flow::to(sid("post"))],
            ),
            post.clone(),
        ]);
        assert_eq!(gated.validate(&vctx), vec![]);

        // So does a human step two steps back.
        let far = workflow(vec![
            step(
                "ask",
                StepKind::Human {
                    prompt: "Which channel?".into(),
                    options: vec![],
                    multi: false,
                    assignee: None,
                },
                vec![Flow::to(sid("gap"))],
            ),
            agent("gap", &["post"]),
            post.clone(),
        ]);
        assert_eq!(far.validate(&vctx), vec![]);

        // The person's own word does too, and travels on the wire only when said.
        let mut owned = post.clone();
        if let StepKind::Connector { unattended, .. } = &mut owned.kind {
            *unattended = true;
        }
        assert_eq!(workflow(vec![owned.clone()]).validate(&vctx), vec![]);
        let wire = serde_json::to_value(&owned).unwrap();
        assert_eq!(wire["unattended"], json!(true));
        let wire = serde_json::to_value(&post).unwrap();
        assert!(wire.get("unattended").is_none(), "false is absent: {wire}");

        // A read needs no gate.
        let mut read_def = slack();
        read_def.operations[0].writes = false;
        let reads = vec![read_def];
        let rctx = ValidationCtx {
            connectors: &reads,
            accounts: &accounts,
            ..ctx()
        };
        assert_eq!(workflow(vec![post]).validate(&rctx), vec![]);
    }

    /// A connector step is checked against the definition installed here:
    /// the connector, the operation, its parameters, and the account it
    /// runs as — all before a run starts.
    #[test]
    fn a_connector_step_is_checked_against_the_definition() {
        let connectors = vec![slack()];
        let accounts = vec![account("slack", true)];
        let vctx = ValidationCtx {
            connectors: &connectors,
            accounts: &accounts,
            ..ctx()
        };
        let wf = workflow(vec![connector_step(
            "post",
            &[("text", "hi {goal.statement}")],
            None,
        )]);
        assert_eq!(wf.validate(&vctx), vec![], "the default account carries it");

        // Nothing installed: the connector is unknown, and nothing else is judged.
        let bare = ValidationCtx { ..ctx() };
        assert_eq!(
            kinds(&wf.validate(&bare)),
            vec![ProblemKind::UnknownConnector]
        );

        // An operation the connector does not have.
        let mut other = wf.clone();
        if let StepKind::Connector { operation, .. } = &mut other.steps[0].kind {
            *operation = Some(OperationId::new("nope").unwrap());
        }
        assert_eq!(
            kinds(&other.validate(&vctx)),
            vec![ProblemKind::UnknownOperation]
        );

        // A parameter the operation does not declare, and a required one left unset.
        let wrong = workflow(vec![connector_step("post", &[("colour", "red")], None)]);
        assert_eq!(
            kinds(&wrong.validate(&vctx)),
            vec![
                ProblemKind::MissingConnectorParam,
                ProblemKind::MissingConnectorParam
            ]
        );

        // A fixed account that is not here.
        let stranger = AccountId::from_ulid(ulid::Ulid::from_parts(9, 9));
        let named = workflow(vec![connector_step(
            "post",
            &[("text", "hi")],
            Some(ValueRef::Fixed(stranger)),
        )]);
        assert_eq!(
            kinds(&named.validate(&vctx)),
            vec![ProblemKind::UnknownAccount]
        );

        // No account named and none marked default: the step cannot run.
        let undefaulted = vec![account("slack", false), {
            let mut a = account("slack", false);
            a.id = AccountId::from_ulid(ulid::Ulid::from_parts(8, 8));
            a
        }];
        let two = ValidationCtx {
            connectors: &connectors,
            accounts: &undefaulted,
            ..ctx()
        };
        assert_eq!(kinds(&wf.validate(&two)), vec![ProblemKind::UnknownAccount]);
        let one = ValidationCtx {
            connectors: &connectors,
            accounts: &undefaulted[..1],
            ..ctx()
        };
        assert_eq!(wf.validate(&one), vec![], "the only account is the default");

        // A `{params.x}` root belongs to a definition, never to a step.
        let leaked = workflow(vec![connector_step(
            "post",
            &[("text", "{params.text}")],
            None,
        )]);
        assert_eq!(
            kinds(&leaked.validate(&vctx)),
            vec![ProblemKind::ParamOnlyPlaceholder]
        );

        // An account read from an input: the input must be an account of this connector.
        let mut by_input = workflow(vec![connector_step(
            "post",
            &[("text", "hi")],
            Some(ValueRef::Input {
                input: InputName::new("who").unwrap(),
            }),
        )]);
        by_input.inputs = vec![input(
            "who",
            InputKind::Account {
                connector: Some(ConnectorId::new("slack").unwrap()),
            },
        )];
        assert_eq!(by_input.validate(&vctx), vec![]);
        by_input.inputs = vec![input(
            "who",
            InputKind::Account {
                connector: Some(ConnectorId::new("jira").unwrap()),
            },
        )];
        assert_eq!(
            kinds(&by_input.validate(&vctx)),
            vec![ProblemKind::InputKindMismatch]
        );
        by_input.inputs = vec![input("who", InputKind::Text)];
        assert_eq!(
            kinds(&by_input.validate(&vctx)),
            vec![ProblemKind::InputKindMismatch]
        );

        // Bound: the account the input holds must be one of this connector's.
        by_input.inputs = vec![input(
            "who",
            InputKind::Account {
                connector: Some(ConnectorId::new("slack").unwrap()),
            },
        )];
        let mine = accounts[0].id.to_string();
        let bound = BTreeMap::from([("who".to_string(), json!(mine))]);
        assert_eq!(
            by_input.validate_bound(&goal_scope(), &bound, &accounts, &Picky),
            vec![]
        );
        let theirs = BTreeMap::from([("who".to_string(), json!(stranger.to_string()))]);
        assert_eq!(
            kinds(&by_input.validate_bound(&goal_scope(), &theirs, &accounts, &Picky)),
            vec![ProblemKind::UnknownAccount]
        );
        assert_eq!(
            kinds(&by_input.validate_bound(
                &goal_scope(),
                &BTreeMap::from([("who".to_string(), json!("x"))]),
                &accounts,
                &Picky
            )),
            vec![ProblemKind::UnknownAccount]
        );
        // The summary names the call.
        assert_eq!(
            wf.steps[0].summary().to_string(),
            "calls slack.post_message with text"
        );
    }

    /// A reference is a real id or absent, never an empty string. A choice
    /// the designer has not made yet is one `Unfilled` problem — and the
    /// checks that depend on it wait rather than pile on.
    #[test]
    fn a_choice_not_made_yet_is_unfilled_and_nothing_else() {
        let connectors = vec![slack()];
        let accounts = vec![account("slack", true)];
        let vctx = ValidationCtx {
            connectors: &connectors,
            accounts: &accounts,
            ..ctx()
        };
        // A connector step fresh from the palette: no connector, no operation.
        let mut wf = workflow(vec![step(
            "call",
            StepKind::Connector {
                connector: None,
                operation: None,
                account: None,
                params: BTreeMap::new(),
                output_schema: None,
                // The fixture's write is the person's word here; the gate rule
                // has its own tests below.
                unattended: true,
            },
            vec![],
        )]);
        let problems = wf.validate(&vctx);
        assert_eq!(kinds(&problems), vec![ProblemKind::Unfilled]);
        assert_eq!(problems[0].step, Some(sid("call")));
        assert!(
            problems[0].text.to_string().contains("no connector yet"),
            "{}",
            problems[0].text
        );
        assert_eq!(
            wf.steps[0].summary().to_string(),
            "calls a connector (not chosen yet)"
        );
        // The connector chosen, the operation not yet: still one problem, and
        // the required parameters are not judged before the operation is known.
        if let StepKind::Connector { connector, .. } = &mut wf.steps[0].kind {
            *connector = Some(ConnectorId::new("slack").unwrap());
        }
        let problems = wf.validate(&vctx);
        assert_eq!(kinds(&problems), vec![ProblemKind::Unfilled]);
        assert!(
            problems[0].text.to_string().contains("no operation yet"),
            "{}",
            problems[0].text
        );
        assert_eq!(
            wf.steps[0].summary().to_string(),
            "calls slack (operation not chosen yet)"
        );
        // Both chosen: the ordinary checks take over.
        if let StepKind::Connector { operation, .. } = &mut wf.steps[0].kind {
            *operation = Some(OperationId::new("post_message").unwrap());
        }
        assert_eq!(
            kinds(&wf.validate(&vctx)),
            vec![ProblemKind::MissingConnectorParam]
        );

        // A schema check whose step is not chosen.
        let check = workflow(vec![
            agent("a", &["shape"]),
            step(
                "shape",
                StepKind::Check {
                    check: CheckKind::Schema {
                        schema: json!({ "type": "object" }),
                        of: None,
                    },
                },
                vec![],
            ),
        ]);
        let problems = check.validate(&ctx());
        assert_eq!(kinds(&problems), vec![ProblemKind::Unfilled]);
        assert_eq!(problems[0].step, Some(sid("shape")));
        assert_eq!(
            check.steps[1].summary().to_string(),
            "checks the output of a step (not chosen yet) against a schema"
        );

        // An account input whose connector is not chosen: a problem on the
        // workflow, not on a step.
        let mut by_input = workflow(vec![agent("a", &[])]);
        by_input.steps[0].kind = StepKind::Agent {
            instructions: "hi {inputs.who}".into(),
            assignee: None,
            project: None,
            harness: vec![],
            model: None,
            effort: None,
            output_schema: None,
            tier_ceiling: ToolTier::Write,
        };
        by_input.inputs = vec![input("who", InputKind::Account { connector: None })];
        let problems = by_input.validate(&ctx());
        assert_eq!(kinds(&problems), vec![ProblemKind::Unfilled]);
        assert_eq!(problems[0].step, None);
        assert!(
            problems[0].text.to_string().contains("input `who`"),
            "{}",
            problems[0].text
        );
    }

    /// On the wire an absent reference is an absent key: it reads back as
    /// `None`, it is written as nothing, and `null` is accepted too.
    #[test]
    fn an_absent_reference_round_trips_as_an_absent_key() {
        let step: Step = serde_json::from_value(json!({
            "id": "call", "name": "Call", "kind": "connector",
            "connector": null, "operation": null, "params": {}
        }))
        .unwrap();
        assert!(matches!(
            &step.kind,
            StepKind::Connector {
                connector: None,
                operation: None,
                ..
            }
        ));
        let wire = serde_json::to_value(&step).unwrap();
        assert!(wire.get("connector").is_none(), "{wire}");
        assert!(wire.get("operation").is_none(), "{wire}");
        let again: Step = serde_json::from_value(wire).unwrap();
        assert_eq!(again, step);

        let check: Step = serde_json::from_value(json!({
            "id": "shape", "name": "Shape", "kind": "check",
            "check": { "check": "schema", "schema": { "type": "object" } }
        }))
        .unwrap();
        assert!(matches!(
            &check.kind,
            StepKind::Check {
                check: CheckKind::Schema { of: None, .. }
            }
        ));
        let input: InputDef = serde_json::from_value(json!({
            "name": "who", "label": "Who", "kind": "account"
        }))
        .unwrap();
        assert_eq!(input.kind, InputKind::Account { connector: None });

        // An empty string is still not an id: the designer never sends one.
        let e = serde_json::from_value::<Step>(json!({
            "id": "call", "name": "Call", "kind": "connector", "connector": ""
        }))
        .unwrap_err()
        .to_string();
        assert!(e.contains("invalid connector id"), "{e}");
    }

    #[test]
    fn an_account_input_accepts_an_account_id() {
        let kind = InputKind::Account {
            connector: Some(ConnectorId::new("slack").unwrap()),
        };
        assert!(kind.accepts(&json!(
            AccountId::from_ulid(ulid::Ulid::from_parts(1, 1)).to_string()
        )));
        assert!(!kind.accepts(&json!("work")));
        assert!(!kind.accepts(&json!(3)));
        assert_eq!(kind.as_str(), "account");
        let wire = serde_json::to_value(&kind).unwrap();
        assert_eq!(wire, json!({ "kind": "account", "connector": "slack" }));
        assert_eq!(serde_json::from_value::<InputKind>(wire).unwrap(), kind);
    }

    #[test]
    fn wait_delay_from_number_input_validates_kind() {
        let mut wf = workflow(vec![step(
            "w",
            StepKind::Wait {
                until: WaitFor::Delay {
                    secs: ValueRef::Input {
                        input: InputName::new("hold").unwrap(),
                    },
                },
            },
            vec![],
        )]);
        wf.inputs = vec![input("hold", InputKind::Text)];
        assert_eq!(
            kinds(&wf.validate(&ctx())),
            vec![ProblemKind::InputKindMismatch]
        );
        wf.inputs = vec![input("hold", InputKind::Number)];
        assert_eq!(wf.validate(&ctx()), vec![]);
        wf.inputs = vec![];
        assert_eq!(kinds(&wf.validate(&ctx())), vec![ProblemKind::UnknownInput]);
    }

    #[test]
    fn a_signal_name_is_a_template_and_a_literal_one_is_dotted_words() {
        let wait_for = |name: &str| {
            workflow(vec![step(
                "w",
                StepKind::Wait {
                    until: WaitFor::Signal {
                        filter: SignalFilter {
                            name: name.into(),
                            fields: BTreeMap::new(),
                        },
                    },
                },
                vec![],
            )])
        };
        let mut wf = wait_for("{inputs.topic}");
        wf.inputs = vec![input("topic", InputKind::Text)];
        assert_eq!(wf.validate(&ctx()), vec![]);
        wf.inputs = vec![];
        assert_eq!(kinds(&wf.validate(&ctx())), vec![ProblemKind::UnknownInput]);
        assert_eq!(wait_for("report.ready").validate(&ctx()), vec![]);
        assert_eq!(
            kinds(&wait_for("Report Ready").validate(&ctx())),
            vec![ProblemKind::BadSignalName]
        );
    }

    #[test]
    fn notify_literal_scope_must_be_a_conversation_id() {
        let notify = |scope: &str| {
            workflow(vec![step(
                "n",
                StepKind::Notify {
                    scope: Some(scope.into()),
                    template: "t".into(),
                    mentions: vec![],
                    author: None,
                },
                vec![],
            )])
        };
        assert_eq!(notify("general").validate(&ctx()), vec![]);
        assert_eq!(
            notify("01ARZ3NDEKTSV4RRFFQ69G5FAV").validate(&ctx()),
            vec![],
            "a goal or workstream id"
        );
        assert_eq!(
            kinds(&notify("Not A Scope!").validate(&ctx())),
            vec![ProblemKind::NotifyScopeUnknown]
        );
        let mut templated = notify("{inputs.channel}");
        templated.inputs = vec![input("channel", InputKind::Text)];
        assert_eq!(templated.validate(&ctx()), vec![]);
    }

    /// A cron read from an input passes `validate` (nothing to read yet) and
    /// is judged when the run's inputs bind.
    #[test]
    fn a_cron_read_from_an_input_is_checked_when_bound() {
        let mut wf = workflow(vec![step(
            "clock",
            StepKind::Wait {
                until: WaitFor::Schedule {
                    cron: ValueRef::Input {
                        input: InputName::new("when").unwrap(),
                    },
                    tz: None,
                },
            },
            vec![],
        )]);
        wf.inputs = vec![input("when", InputKind::Text)];
        assert_eq!(
            wf.validate(&ValidationCtx {
                checks: &Picky,
                ..ctx()
            }),
            vec![]
        );
        let bound = |v: serde_json::Value| BTreeMap::from([("when".to_string(), v)]);
        assert_eq!(
            wf.validate_bound(&goal_scope(), &bound(json!("* * * * *")), &[], &Picky),
            vec![]
        );
        let problems = wf.validate_bound(&goal_scope(), &bound(json!("bad")), &[], &Picky);
        assert_eq!(kinds(&problems), vec![ProblemKind::BadCron]);
        assert_eq!(problems[0].step, Some(sid("clock")));
        assert_eq!(
            kinds(&wf.validate_bound(&goal_scope(), &bound(json!(5)), &[], &Picky)),
            vec![ProblemKind::BadCron],
            "a number is not a cron expression"
        );
    }

    /// Only an agent speaks for a workflow: a fixed person or team is refused
    /// outright, an agent nobody installed is an unknown assignee, and no
    /// author at all is the Workflow Agent's turn to speak.
    #[test]
    fn notify_author_must_be_an_agent() {
        let notify = |author: Option<Assignee>| {
            workflow(vec![step(
                "n",
                StepKind::Notify {
                    scope: None,
                    template: "t".into(),
                    mentions: vec![],
                    author: author.map(ValueRef::Fixed),
                },
                vec![],
            )])
        };
        assert_eq!(notify(None).validate(&ctx()), vec![]);
        assert_eq!(
            notify(Some(Assignee::Agent("developer".into()))).validate(&ctx()),
            vec![]
        );
        assert_eq!(
            kinds(&notify(Some(Assignee::Agent("nobody".into()))).validate(&ctx())),
            vec![ProblemKind::UnknownAssignee]
        );
        let team = notify(Some(Assignee::Team("engineering".into()))).validate(&ctx());
        assert_eq!(kinds(&team), vec![ProblemKind::NotifyAuthorNotAnAgent]);
        assert!(
            team[0]
                .text
                .to_string()
                .contains("step `n` speaks as `team:engineering`"),
            "{team:?}"
        );
        let person = notify(Some(Assignee::Human(pk()))).validate(&ctx());
        assert_eq!(
            kinds(&person),
            vec![
                ProblemKind::UnknownAssignee,
                ProblemKind::NotifyAuthorNotAnAgent
            ],
            "a person is neither staff nor a voice"
        );
    }

    /// An author read from an input passes `validate` (nothing to read yet)
    /// and is judged when the run's inputs bind: an agent speaks, anyone
    /// else is refused by the step's name.
    #[test]
    fn a_notify_author_read_from_an_input_is_checked_when_bound() {
        let mut wf = workflow(vec![step(
            "n",
            StepKind::Notify {
                scope: None,
                template: "t".into(),
                mentions: vec![],
                author: Some(ValueRef::Input {
                    input: InputName::new("voice").unwrap(),
                }),
            },
            vec![],
        )]);
        wf.inputs = vec![input("voice", InputKind::Assignee)];
        assert_eq!(wf.validate(&ctx()), vec![]);
        let bound = |v: &str| BTreeMap::from([("voice".to_string(), json!(v))]);
        assert_eq!(
            wf.validate_bound(&goal_scope(), &bound("agent:developer"), &[], &Picky),
            vec![]
        );
        let problems = wf.validate_bound(&goal_scope(), &bound("team:engineering"), &[], &Picky);
        assert_eq!(kinds(&problems), vec![ProblemKind::NotifyAuthorNotAnAgent]);
        assert!(
            problems[0]
                .text
                .to_string()
                .contains("step `n`: input `voice`"),
            "{problems:?}"
        );
    }

    #[test]
    fn a_notify_scope_read_from_an_input_is_checked_when_bound() {
        let notify = |scope: &str| {
            let mut wf = workflow(vec![step(
                "n",
                StepKind::Notify {
                    scope: Some(scope.into()),
                    template: "t".into(),
                    mentions: vec![],
                    author: None,
                },
                vec![],
            )]);
            wf.inputs = vec![input("channel", InputKind::Text)];
            wf
        };
        let bound = |v: &str| BTreeMap::from([("channel".to_string(), json!(v))]);
        let wf = notify("{inputs.channel}");
        assert_eq!(
            wf.validate_bound(&goal_scope(), &bound("general"), &[], &Picky),
            vec![]
        );
        assert_eq!(
            wf.validate_bound(
                &goal_scope(),
                &bound("01ARZ3NDEKTSV4RRFFQ69G5FAV"),
                &[],
                &Picky
            ),
            vec![]
        );
        let problems = wf.validate_bound(&goal_scope(), &bound("no such channel!"), &[], &Picky);
        assert_eq!(kinds(&problems), vec![ProblemKind::NotifyScopeUnknown]);
        assert!(
            problems[0].text.to_string().contains("no such channel!"),
            "{}",
            problems[0].text
        );
        // A scope that reads a step's output is the run's to judge.
        let mut later = notify("{steps.a.output.where}");
        later
            .steps
            .insert(0, promising(agent("a", &["n"]), &["where"]));
        assert_eq!(
            later.validate_bound(&goal_scope(), &bound("x"), &[], &Picky),
            vec![]
        );
        // A step never reads the event that began the run: a scope that
        // tries is refused by the definition, not judged by the binding.
        let eventful = notify("{event.payload.channel}");
        // Nothing reads `channel` here, and an input nothing reads is a
        // problem of its own: the definition is judged without it.
        let mut defined = eventful.clone();
        defined.inputs.clear();
        assert_eq!(
            kinds(&defined.validate(&ValidationCtx::default())),
            vec![ProblemKind::StartPlaceholder]
        );
        assert_eq!(
            eventful.validate_bound(&goal_scope(), &bound("x"), &[], &Picky),
            vec![]
        );
    }

    #[test]
    fn a_delay_read_from_an_input_must_be_whole_seconds() {
        let mut wf = workflow(vec![step(
            "w",
            StepKind::Wait {
                until: WaitFor::Delay {
                    secs: ValueRef::Input {
                        input: InputName::new("hold").unwrap(),
                    },
                },
            },
            vec![],
        )]);
        wf.inputs = vec![input("hold", InputKind::Number)];
        let bound = |v: serde_json::Value| BTreeMap::from([("hold".to_string(), v)]);
        assert_eq!(
            wf.validate_bound(&goal_scope(), &bound(json!(30)), &[], &Picky),
            vec![]
        );
        assert_eq!(
            kinds(&wf.validate_bound(&goal_scope(), &bound(json!(2.5)), &[], &Picky)),
            vec![ProblemKind::InputKindMismatch]
        );
        assert_eq!(
            kinds(&wf.validate_bound(&goal_scope(), &bound(json!(-1)), &[], &Picky)),
            vec![ProblemKind::InputKindMismatch]
        );
    }

    #[test]
    fn bind_inputs_fills_defaults_refuses_wrong_kind_and_unknown() {
        let mut wf = workflow(vec![agent("a", &[])]);
        wf.inputs = vec![
            InputDef {
                name: InputName::new("n").unwrap(),
                label: "N".into(),
                kind: InputKind::Number,
                default: Some(json!(3)),
                required: false,
            },
            InputDef {
                name: InputName::new("c").unwrap(),
                label: "C".into(),
                kind: InputKind::Choice {
                    options: vec!["x".into(), "y".into()],
                },
                default: None,
                required: true,
            },
            input("opt", InputKind::Text),
        ];
        let bound = wf
            .bind_inputs(BTreeMap::from([("c".to_string(), json!("x"))]))
            .unwrap();
        assert_eq!(bound.get("n"), Some(&json!(3)), "the default is filled");
        assert_eq!(bound.get("c"), Some(&json!("x")));
        assert!(
            !bound.contains_key("opt"),
            "an optional input with no default stays absent"
        );

        assert_eq!(
            wf.bind_inputs(BTreeMap::new()),
            Err(InputError::Missing { input: "c".into() })
        );
        assert_eq!(
            wf.bind_inputs(BTreeMap::from([("c".to_string(), json!("z"))])),
            Err(InputError::WrongKind {
                input: "c".into(),
                want: "choice (x, y)".into(),
                got: "\"z\"".into(),
            })
        );
        assert_eq!(
            wf.bind_inputs(BTreeMap::from([
                ("c".to_string(), json!("x")),
                ("zzz".to_string(), json!(1)),
            ])),
            Err(InputError::Unknown {
                inputs: vec!["zzz".into()]
            })
        );
        let e = InputError::Unknown {
            inputs: vec!["p".into(), "q".into()],
        };
        assert_eq!(
            e.to_string(),
            "the workflow declares no input named `p`, `q`"
        );
    }

    #[test]
    fn a_default_fills_what_nobody_gave_and_never_what_was_given() {
        let mut wf = workflow(vec![agent("a", &[])]);
        let mut when = input("when", InputKind::Text);
        when.default = Some(json!("0 9 * * MON"));
        wf.inputs = vec![when, input("who", InputKind::Text)];
        assert_eq!(
            wf.with_defaults(&BTreeMap::new()),
            BTreeMap::from([("when".to_string(), json!("0 9 * * MON"))]),
            "an input with no default stays absent"
        );
        let given = BTreeMap::from([
            ("when".to_string(), json!("30 8 * * *")),
            ("who".to_string(), json!("the team")),
        ]);
        assert_eq!(wf.with_defaults(&given), given);
    }

    #[test]
    fn a_typed_word_is_read_by_the_kind_that_asks_for_it() {
        let choice = InputKind::Choice {
            options: vec!["true".into(), "12".into()],
        };
        // A word stays the word it is wherever a word is asked.
        for typed in [
            "true",
            "false",
            "null",
            "12",
            "2.4",
            "[1]",
            "\"quoted\"",
            " spaced ",
        ] {
            for kind in [
                InputKind::Text,
                choice.clone(),
                InputKind::Assignee,
                InputKind::Project,
                InputKind::Account { connector: None },
            ] {
                assert_eq!(
                    kind.read(typed),
                    json!(typed),
                    "{typed} as {}",
                    kind.as_str()
                );
            }
        }
        assert!(InputKind::Text.accepts(&InputKind::Text.read("true")));
        assert!(choice.accepts(&choice.read("12")));

        // A number and a yes/no are read as one.
        assert_eq!(InputKind::Number.read("12"), json!(12));
        assert_eq!(InputKind::Number.read(" 2.5 "), json!(2.5));
        assert_eq!(InputKind::Number.read("-3"), json!(-3));
        assert_eq!(InputKind::Bool.read("true"), json!(true));
        assert_eq!(InputKind::Bool.read("false"), json!(false));

        // What is not its kind stays a word, and is refused as one.
        for (kind, typed) in [
            (InputKind::Number, "twelve"),
            (InputKind::Number, "true"),
            (InputKind::Number, ""),
            (InputKind::Bool, "yes"),
            (InputKind::Bool, "1"),
        ] {
            let read = kind.read(typed);
            assert_eq!(read, json!(typed), "{typed} as {}", kind.as_str());
            assert!(!kind.accepts(&read), "{typed} as {}", kind.as_str());
        }
    }

    // -------------------------------------------------------------------
    // Events and gateways
    // -------------------------------------------------------------------

    fn start(id: &str, on: StartOn, then: &[&str]) -> Step {
        step(
            id,
            StepKind::Start {
                on,
                inputs: BTreeMap::new(),
                guard: Guard::default(),
            },
            then.iter().map(|t| Flow::to(sid(t))).collect(),
        )
    }

    fn manual(id: &str, then: &[&str]) -> Step {
        start(id, StartOn::Manual, then)
    }

    fn weekly(id: &str, then: &[&str]) -> Step {
        start(
            id,
            StartOn::Schedule {
                schedule: Schedule::cron("0 9 * * 1", None),
            },
            then,
        )
    }

    fn mapping(mut s: Step, pairs: &[(&str, &str)]) -> Step {
        if let StepKind::Start { inputs, .. } = &mut s.kind {
            for (k, v) in pairs {
                inputs.insert(k.to_string(), v.to_string());
            }
        }
        s
    }

    fn approval(id: &str, then: Vec<Flow>) -> Step {
        step(
            id,
            StepKind::Approval {
                prompt: "Ship it?".into(),
            },
            then,
        )
    }

    fn timeout(name: &str, secs: u64) -> Boundary {
        Boundary {
            name: Branch::new(name).unwrap(),
            on: BoundaryOn::After {
                secs: ValueRef::Fixed(secs),
            },
            act: BoundaryAct::Divert,
        }
    }

    fn reminder(name: &str) -> Boundary {
        Boundary {
            name: Branch::new(name).unwrap(),
            on: BoundaryOn::Every {
                secs: ValueRef::Fixed(86_400),
                max: 3,
            },
            act: BoundaryAct::Notify {
                scope: None,
                template: "Still waiting.".into(),
                mentions: vec![],
                author: None,
            },
        }
    }

    #[test]
    fn named_starts_are_the_ways_in_and_the_manual_one_is_the_run_by_hand() {
        let wf = workflow(vec![
            manual("by_hand", &["a"]),
            weekly("weekly", &["a"]),
            agent("a", &[]),
        ]);
        assert_eq!(wf.validate(&ctx()), vec![]);
        let ids: Vec<&str> = wf.start_steps().iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["by_hand", "weekly"]);
        assert_eq!(wf.manual_entry().map(|s| s.id.as_str()), Some("by_hand"));
        assert!(wf.has_starts() && !wf.is_event_only());
        assert!(wf.is_entry(&sid("weekly")) && !wf.is_entry(&sid("a")));
        assert_eq!(wf.event_starts().len(), 1);

        let events_only = workflow(vec![weekly("weekly", &["a"]), agent("a", &[])]);
        assert!(events_only.is_event_only());
        assert_eq!(events_only.manual_entry(), None);

        // A workflow that names none begins at its root, as it always did.
        let plain = workflow(vec![agent("a", &["b"]), agent("b", &[])]);
        assert!(!plain.has_starts() && !plain.is_event_only());
        assert_eq!(plain.manual_entry().map(|s| s.id.as_str()), Some("a"));
        assert_eq!(
            start_steps_in(&plain.steps)
                .iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>(),
            ["a"]
        );
    }

    #[test]
    fn a_start_has_nothing_before_it_and_one_start_is_by_hand() {
        let looped = workflow(vec![manual("s", &["a"]), agent("a", &["s"])]);
        assert!(kinds(&looped.validate(&ctx())).contains(&ProblemKind::StartHasIncoming));

        let twice = workflow(vec![
            manual("s1", &["a"]),
            manual("s2", &["a"]),
            agent("a", &[]),
        ]);
        assert_eq!(
            kinds(&twice.validate(&ctx())),
            vec![ProblemKind::ManyManualStarts, ProblemKind::ManyManualStarts]
        );

        let mut configured = mapping(manual("s", &["a"]), &[("x", "{event.at}")]);
        if let StepKind::Start { guard, .. } = &mut configured.kind {
            guard.debounce_secs = 5;
        }
        let mut wf = workflow(vec![configured, agent_reading("a", "{inputs.x}", &[])]);
        wf.inputs = vec![input("x", InputKind::Text)];
        assert_eq!(
            kinds(&wf.validate(&ctx())),
            vec![
                ProblemKind::ManualStartConfigured,
                ProblemKind::ManualStartConfigured
            ]
        );

        // Beside named starts, a plain root is reached by nothing.
        let orphan = workflow(vec![manual("s", &["a"]), agent("a", &[]), agent("b", &[])]);
        assert_eq!(
            kinds(&orphan.validate(&ctx())),
            vec![ProblemKind::Unreachable]
        );
    }

    #[test]
    fn the_event_is_read_by_a_starts_mapping_and_nowhere_else() {
        let hook = mapping(
            start("hook", StartOn::Hook { public: false }, &["a"]),
            &[("ticket", "{event.payload.body}")],
        );
        let with = |first: Step, reader: &str| {
            let mut wf = workflow(vec![first, agent_reading("a", reader, &[])]);
            wf.inputs = vec![input("ticket", InputKind::Text)];
            wf
        };
        assert_eq!(
            with(hook.clone(), "Triage {inputs.ticket}").validate(&ctx()),
            vec![]
        );
        assert_eq!(
            kinds(&with(hook.clone(), "Triage {event.payload.body}").validate(&ctx())),
            vec![ProblemKind::StartPlaceholder]
        );
        let reads_inputs = mapping(hook.clone(), &[("ticket", "{inputs.ticket}")]);
        assert_eq!(
            kinds(&with(reads_inputs, "Triage {inputs.ticket}").validate(&ctx())),
            vec![ProblemKind::StartPlaceholder]
        );
        let stray = mapping(hook, &[("nope", "{event.at}")]);
        assert_eq!(
            kinds(&with(stray, "Triage {inputs.ticket}").validate(&ctx())),
            vec![ProblemKind::UnknownInput]
        );
        let reads_a_step = workflow(vec![
            start(
                "m",
                StartOn::Message {
                    filter: MessageFilter {
                        contains: Some("{steps.a.output}".into()),
                        ..MessageFilter::default()
                    },
                },
                &["a"],
            ),
            agent("a", &[]),
        ]);
        assert_eq!(
            kinds(&reads_a_step.validate(&ctx())),
            vec![ProblemKind::StartPlaceholder]
        );
    }

    #[test]
    fn a_timeout_diverts_along_its_own_flows_beside_the_normal_ones() {
        let mut gate = approval(
            "review",
            vec![Flow::to(sid("ship")), labelled("escalate", "late")],
        );
        gate.boundaries = vec![timeout("late", 172_800), reminder("nudge")];
        let wf = |g: Step| {
            workflow(vec![
                manual("s", &["review"]),
                g,
                agent("ship", &[]),
                agent("escalate", &[]),
            ])
        };
        assert_eq!(wf(gate.clone()).validate(&ctx()), vec![]);
        assert_eq!(gate.divert_names(), vec![&Branch::new("late").unwrap()]);
        assert_eq!(gate.boundary_names().len(), 2);
        assert!(gate.is_divert_flow(&gate.then[1]) && !gate.is_divert_flow(&gate.then[0]));
        assert!(gate.has_diverts());

        let mut lonely = gate.clone();
        lonely.then.truncate(1);
        let wf2 = workflow(vec![manual("s", &["review"]), lonely, agent("ship", &[])]);
        assert_eq!(
            kinds(&wf2.validate(&ctx())),
            vec![ProblemKind::RuleWithoutFlow]
        );

        let mut stray = gate.clone();
        stray.then.push(labelled("ship", "typo"));
        assert_eq!(
            kinds(&wf(stray).validate(&ctx())),
            vec![ProblemKind::LabelledFlowOnPlainStep]
        );

        let mut twice = gate;
        twice.boundaries.push(timeout("late", 60));
        assert_eq!(
            kinds(&wf(twice).validate(&ctx())),
            vec![ProblemKind::DuplicateBranch]
        );
    }

    #[test]
    fn boundaries_sit_on_stoppable_steps_and_a_reminder_never_diverts() {
        let mut check = step(
            "c",
            StepKind::Check {
                check: CheckKind::Command {
                    command: "true".into(),
                },
            },
            vec![Flow::to(sid("a")), labelled("a", "late")],
        );
        check.boundaries = vec![timeout("late", 60)];
        let wf = workflow(vec![manual("s", &["c"]), check, agent("a", &[])]);
        assert_eq!(
            kinds(&wf.validate(&ctx())),
            vec![ProblemKind::BoundaryOnInstantStep]
        );

        let mut nag = approval("g", vec![Flow::to(sid("a")), labelled("a", "nag")]);
        nag.boundaries = vec![Boundary {
            act: BoundaryAct::Divert,
            ..reminder("nag")
        }];
        let wf = workflow(vec![manual("s", &["g"]), nag, agent("a", &[])]);
        assert_eq!(
            kinds(&wf.validate(&ctx())),
            vec![ProblemKind::ReminderInterrupts]
        );

        let mut spent = approval("g", vec![Flow::to(sid("a"))]);
        spent.boundaries = vec![Boundary {
            on: BoundaryOn::Every {
                secs: ValueRef::Fixed(0),
                max: 0,
            },
            ..reminder("nag")
        }];
        let wf = workflow(vec![manual("s", &["g"]), spent, agent("a", &[])]);
        assert_eq!(
            kinds(&wf.validate(&ctx())),
            vec![ProblemKind::BadTimer, ProblemKind::BadTimer]
        );
    }

    #[test]
    fn a_write_reached_only_by_a_gates_timeout_is_not_gated() {
        let connectors = [slack()];
        let accounts = [account("slack", true)];
        let rt = ValidationCtx {
            connectors: &connectors,
            accounts: &accounts,
            ..ctx()
        };
        let mut post = connector_step("post", &[("text", "hi")], None);
        if let StepKind::Connector { unattended, .. } = &mut post.kind {
            *unattended = false;
        }
        let gated_by = |normal: &str, late: &str| {
            let mut gate = approval("g", vec![Flow::to(sid(normal)), labelled(late, "late")]);
            gate.boundaries = vec![timeout("late", 60)];
            workflow(vec![
                manual("s", &["g"]),
                gate,
                agent("ship", &[]),
                post.clone(),
            ])
        };
        assert_eq!(
            kinds(&gated_by("ship", "post").validate(&rt)),
            vec![ProblemKind::UngatedWrite],
            "a timeout is nobody's approval"
        );
        assert_eq!(gated_by("post", "ship").validate(&rt), vec![]);
    }

    #[test]
    fn a_diverted_step_holds_nothing_on_its_divert_path() {
        let mut work = promising(agent("work", &["use"]), &["report"]);
        work.then.push(labelled("late", "slow"));
        work.boundaries = vec![timeout("slow", 3_600)];
        let wf = workflow(vec![
            manual("s", &["work"]),
            work,
            agent_reading("use", "{steps.work.output.report}", &[]),
            agent_reading("late", "{steps.work.output.report}", &[]),
        ]);
        assert_eq!(kinds(&wf.validate(&ctx())), vec![ProblemKind::NotAssured]);
        let a = wf.assurance();
        assert!(a[&sid("use")].present.contains(&sid("work")));
        assert!(!a[&sid("late")].present.contains(&sid("work")));
        assert!(a[&sid("late")].settled.contains(&sid("work")));
    }

    /// A step one source feeds twice — its normal flow and a divert, or its
    /// flow and its fail route, into the same step — is still walked: it
    /// counts on what came before the source, and on the source only where
    /// every way in brings its output.
    #[test]
    fn a_step_fed_twice_by_one_source_is_still_assured() {
        let mut ask = promising(agent("ask", &["record"]), &["answer"]);
        ask.then.push(labelled("record", "late"));
        ask.boundaries = vec![timeout("late", 60)];
        let wf = workflow(vec![
            manual("s", &["brief"]),
            promising(agent("brief", &["ask"]), &["text"]),
            ask,
            agent_reading("record", "{steps.brief.output.text}", &[]),
        ]);
        assert_eq!(wf.validate(&ctx()), vec![]);
        let a = wf.assurance();
        assert!(a[&sid("record")].present.contains(&sid("brief")));
        assert!(
            !a[&sid("record")].present.contains(&sid("ask")),
            "the divert brings no answer"
        );
        assert!(a[&sid("record")].settled.contains(&sid("ask")));

        let mut risky = agent("risky", &["after"]);
        risky.on_fail = OnFail::Then { step: sid("after") };
        let wf = workflow(vec![manual("s", &["risky"]), risky, agent("after", &[])]);
        assert!(wf.assurance().contains_key(&sid("after")));
    }

    #[test]
    fn decide_every_takes_every_rule_that_holds_and_a_join_counts_on_none() {
        let always = || Condition::Between {
            from_hour: 0,
            to_hour: 23,
        };
        let fan = |reader: &str| {
            workflow(vec![
                manual("s", &["d"]),
                step(
                    "d",
                    StepKind::Decide {
                        rules: vec![
                            Rule {
                                when: always(),
                                branch: Branch::new("mail").unwrap(),
                            },
                            Rule {
                                when: always(),
                                branch: Branch::new("chat").unwrap(),
                            },
                        ],
                        otherwise: Branch::new("none").unwrap(),
                        pick: Pick::Every,
                    },
                    vec![
                        labelled("mail", "mail"),
                        labelled("chat", "chat"),
                        labelled("join", "none"),
                    ],
                ),
                promising(agent("mail", &["join"]), &["sent"]),
                promising(agent("chat", &["join"]), &["sent"]),
                agent_reading("join", reader, &[]),
            ])
        };
        assert_eq!(fan("Summarise").validate(&ctx()), vec![]);
        assert_eq!(
            kinds(&fan("Summarise {steps.mail.output.sent}").validate(&ctx())),
            vec![ProblemKind::NotAssured]
        );
    }

    #[test]
    fn listening_needs_what_no_event_supplies_and_no_default_fills() {
        let hook = mapping(
            start("hook", StartOn::Hook { public: false }, &["a"]),
            &[("ticket", "{event.payload.body}")],
        );
        let health = start(
            "health",
            StartOn::Check {
                command: "sh -c {inputs.command}".into(),
                project: None,
                fire_on: crate::start::FireOn::StartsFailing,
                schedule: Schedule {
                    every: Some(ValueRef::Input {
                        input: InputName::new("interval").unwrap(),
                    }),
                    ..Schedule::default()
                },
            },
            &["a"],
        );
        let mut wf = workflow(vec![
            manual("by_hand", &["a"]),
            hook,
            health,
            agent_reading("a", "{inputs.ticket} for {inputs.team}", &[]),
        ]);
        let mut interval = input("interval", InputKind::Number);
        interval.default = Some(json!(300));
        let mut ticket = input("ticket", InputKind::Text);
        ticket.required = true;
        let mut team = input("team", InputKind::Text);
        team.required = true;
        wf.inputs = vec![input("command", InputKind::Text), interval, ticket, team];
        assert_eq!(wf.validate(&ctx()), vec![]);
        let needs: Vec<String> = wf.listening_needs().iter().map(|n| n.to_string()).collect();
        // `team` no event maps; `ticket` the check start does not map; the
        // check reads `command`; `interval` has its default.
        assert_eq!(needs, ["command", "team", "ticket"]);
    }

    #[test]
    fn a_start_event_names_only_what_exists() {
        let one = |on: StartOn| workflow(vec![start("s", on, &["a"]), agent("a", &[])]);
        let problems = |on: StartOn, rt: &ValidationCtx<'_>| kinds(&one(on).validate(rt));
        let ctx = ctx();
        assert_eq!(
            problems(
                StartOn::Signal {
                    filter: SignalFilter {
                        name: "Report Ready".into(),
                        fields: BTreeMap::new(),
                    },
                },
                &ctx
            ),
            vec![ProblemKind::BadSignalName]
        );
        assert_eq!(
            problems(
                StartOn::Project {
                    filter: ProjectFilter::default(),
                },
                &ctx
            ),
            vec![ProblemKind::Unfilled]
        );
        let ghost = ProjectId::from_ulid(ulid::Ulid::from_parts(8, 8));
        assert_eq!(
            problems(
                StartOn::Project {
                    filter: ProjectFilter {
                        project: Some(ValueRef::Fixed(ghost)),
                        ..ProjectFilter::default()
                    },
                },
                &ctx
            ),
            vec![ProblemKind::UnknownProject]
        );
        let elsewhere = WorkflowId::from_ulid(ulid::Ulid::from_parts(8, 9));
        assert_eq!(
            problems(
                StartOn::Run {
                    filter: RunFilter {
                        workflow: Some(elsewhere),
                        outcome: None,
                    },
                },
                &ctx
            ),
            vec![ProblemKind::UnknownWorkflow]
        );
        assert_eq!(
            problems(
                StartOn::Platform {
                    filter: PlatformFilter::default(),
                },
                &ctx
            ),
            vec![ProblemKind::UnknownTopic]
        );
        let topics = ["goal.closed"];
        let rt = ValidationCtx {
            topics: &topics,
            ..ctx
        };
        let platform = |topic: &str| StartOn::Platform {
            filter: PlatformFilter {
                topic: topic.into(),
                fields: BTreeMap::new(),
            },
        };
        assert_eq!(problems(platform("goal.closed"), &rt), vec![]);
        assert_eq!(
            problems(platform("goal.exploded"), &rt),
            vec![ProblemKind::UnknownTopic]
        );
        assert_eq!(
            problems(
                StartOn::Schedule {
                    schedule: Schedule::default(),
                },
                &ctx
            ),
            vec![ProblemKind::BadTimer]
        );
        assert_eq!(
            problems(
                StartOn::Schedule {
                    schedule: Schedule::every(0),
                },
                &ctx
            ),
            vec![ProblemKind::BadTimer]
        );
        let picky = ValidationCtx {
            checks: &Picky,
            ..ctx
        };
        assert_eq!(
            problems(
                StartOn::Schedule {
                    schedule: Schedule::cron("bad", None),
                },
                &picky
            ),
            vec![ProblemKind::BadCron]
        );
        assert_eq!(
            problems(
                StartOn::Message {
                    filter: MessageFilter {
                        r#in: Some("no such place!".into()),
                        ..MessageFilter::default()
                    },
                },
                &ctx
            ),
            vec![ProblemKind::NotifyScopeUnknown]
        );
        assert_eq!(
            problems(
                StartOn::Message {
                    filter: MessageFilter {
                        from: crate::listen::MessageFrom::Someone(ValueRef::Fixed(
                            Assignee::Agent("nobody".into())
                        )),
                        ..MessageFilter::default()
                    },
                },
                &ctx
            ),
            vec![ProblemKind::UnknownAssignee]
        );
    }

    #[test]
    fn a_poll_reads_and_tells_its_items_apart() {
        let mut read = slack();
        read.operations[0].writes = false;
        let reads = [read];
        let writes = [slack()];
        let accounts = [account("slack", true)];
        let poll = |key: Option<&str>| {
            workflow(vec![
                start(
                    "s",
                    StartOn::Connector {
                        connector: Some(ConnectorId::new("slack").unwrap()),
                        operation: Some(OperationId::new("post_message").unwrap()),
                        account: None,
                        params: BTreeMap::from([("text".to_string(), "hi".to_string())]),
                        key: key.map(str::to_string),
                        schedule: Schedule::every(300),
                    },
                    &["a"],
                ),
                agent("a", &[]),
            ])
        };
        let over = |connectors| ValidationCtx {
            connectors,
            accounts: &accounts,
            ..ctx()
        };
        assert_eq!(poll(Some("ts")).validate(&over(&reads)), vec![]);
        assert_eq!(
            kinds(&poll(None).validate(&over(&reads))),
            vec![ProblemKind::BadPoll]
        );
        assert_eq!(
            kinds(&poll(Some("ts")).validate(&over(&writes))),
            vec![ProblemKind::BadPoll],
            "a start event only reads"
        );
        let unchosen = workflow(vec![
            start(
                "s",
                StartOn::Connector {
                    connector: None,
                    operation: None,
                    account: None,
                    params: BTreeMap::new(),
                    key: None,
                    schedule: Schedule::every(300),
                },
                &["a"],
            ),
            agent("a", &[]),
        ]);
        assert_eq!(
            kinds(&unchosen.validate(&ctx())),
            vec![ProblemKind::Unfilled, ProblemKind::BadPoll]
        );
    }

    #[test]
    fn an_emit_names_its_signal_and_a_spawn_needs_a_way_in_by_hand() {
        let emit = |name: &str| {
            let mut wf = workflow(vec![
                manual("s", &["e"]),
                step(
                    "e",
                    StepKind::Emit {
                        signal: name.into(),
                        payload: BTreeMap::from([("url".to_string(), "{inputs.url}".to_string())]),
                    },
                    vec![],
                ),
            ]);
            wf.inputs = vec![input("url", InputKind::Text)];
            wf
        };
        assert_eq!(emit("report.ready").validate(&ctx()), vec![]);
        assert_eq!(
            kinds(&emit("Report!").validate(&ctx())),
            vec![ProblemKind::BadSignalName]
        );
        let target = WorkflowId::from_ulid(ulid::Ulid::from_parts(5, 5));
        let spawner = workflow(vec![
            manual("s", &["sp"]),
            step(
                "sp",
                StepKind::Spawn {
                    statement_template: "do it".into(),
                    workflow: Some(target),
                    assignees: vec![],
                    inputs: Default::default(),
                    wait: true,
                },
                vec![],
            ),
        ]);
        let known = [target];
        let rt = ValidationCtx {
            workflows: &known,
            ..ctx()
        };
        assert_eq!(spawner.validate(&rt), vec![]);
        let rt = ValidationCtx {
            event_only: &known,
            ..rt
        };
        assert_eq!(
            kinds(&spawner.validate(&rt)),
            vec![ProblemKind::SpawnNeedsManualEntry]
        );
    }

    /// **A spawn gives its workflow what it asks.** A step that opened a
    /// goal on a workflow could give its run nothing: the catalog's own
    /// incident response opened its follow-up on *Bug fix*, which requires a
    /// report and a project, and failed there every time it got that far.
    /// The step says what the child is given, and is held to what the child
    /// asks before anything runs.
    #[test]
    fn a_spawn_is_held_to_what_its_workflow_asks() {
        let target = WorkflowId::from_ulid(ulid::Ulid::from_parts(5, 5));
        let spawner = |given: &[(&str, &str)]| {
            let mut wf = workflow(vec![
                manual("s", &["sp"]),
                step(
                    "sp",
                    StepKind::Spawn {
                        statement_template: "fix {inputs.what}".into(),
                        workflow: Some(target),
                        assignees: vec![],
                        inputs: given
                            .iter()
                            .map(|(name, said)| (name.to_string(), said.to_string()))
                            .collect(),
                        wait: false,
                    },
                    vec![],
                ),
            ]);
            wf.inputs = vec![input("what", InputKind::Text)];
            wf
        };
        let mut rounds = input("rounds", InputKind::Number);
        rounds.default = Some(json!(3));
        let mut report = input("report", InputKind::Text);
        report.required = true;
        let mut project = input("project", InputKind::Project);
        project.required = true;
        let asked = [(
            target,
            vec![report, project, rounds, input("notes", InputKind::Text)],
        )];
        let known = [target];
        let rt = ValidationCtx {
            workflows: &known,
            asks: &asked,
            ..ctx()
        };

        // What it requires is given; what a default fills, or nobody
        // requires, may be left out.
        let whole = spawner(&[("report", "{inputs.what}"), ("project", "{inputs.what}")]);
        assert_eq!(whole.validate(&rt), vec![]);
        assert_eq!(
            spawner(&[
                ("report", "r"),
                ("project", "p"),
                ("rounds", "5"),
                ("notes", "n")
            ])
            .validate(&rt),
            vec![]
        );

        // One required input left out is one problem, by name.
        let problems = spawner(&[("report", "r")]).validate(&rt);
        assert_eq!(kinds(&problems), vec![ProblemKind::SpawnInput]);
        assert_eq!(problems[0].step.as_ref().map(StepId::as_str), Some("sp"));
        assert!(
            format!("{:?}", problems[0].text).contains("project"),
            "{:?}",
            problems[0].text
        );
        assert_eq!(
            kinds(&spawner(&[]).validate(&rt)),
            vec![ProblemKind::SpawnInput, ProblemKind::SpawnInput]
        );

        // An input the workflow does not declare is one too.
        let problems = spawner(&[("report", "r"), ("project", "p"), ("reprot", "r")]).validate(&rt);
        assert_eq!(kinds(&problems), vec![ProblemKind::SpawnInput]);
        assert!(
            format!("{:?}", problems[0].text).contains("reprot"),
            "{:?}",
            problems[0].text
        );

        // What it says is a template of this run, read as every other is.
        let reads_nobody = spawner(&[("report", "{inputs.nobody}"), ("project", "p")]);
        assert_eq!(
            kinds(&reads_nobody.validate(&rt)),
            vec![ProblemKind::UnknownInput]
        );

        // A workflow nobody knows is said once, as that.
        assert_eq!(
            kinds(&spawner(&[]).validate(&ctx())),
            vec![ProblemKind::UnknownWorkflow]
        );

        // On the wire the inputs sit beside the step's other fields, and a
        // spawn that gives nothing says nothing.
        let said = serde_json::to_value(&whole.steps[1]).unwrap();
        assert_eq!(
            said["inputs"],
            json!({ "report": "{inputs.what}", "project": "{inputs.what}" })
        );
        assert_eq!(
            serde_json::from_value::<Step>(said).unwrap(),
            whole.steps[1]
        );
        let nothing = serde_json::to_value(&spawner(&[]).steps[1]).unwrap();
        assert_eq!(nothing.get("inputs"), None, "{nothing}");
    }

    #[test]
    fn a_step_refuses_a_misspelled_key_in_what_it_nests() {
        let good = json!({"id": "s", "name": "S", "kind": "start", "on": {"event": "schedule", "every": 60}, "then": ["a"]});
        assert!(serde_json::from_value::<Step>(good).is_ok());
        let bad_on = json!({"id": "s", "name": "S", "kind": "start", "on": {"event": "schedule", "evry": 60}});
        let err = serde_json::from_value::<Step>(bad_on)
            .unwrap_err()
            .to_string();
        assert!(err.contains("evry"), "{err}");
        let bad_until = json!({"id": "w", "name": "W", "kind": "wait", "until": {"until": "signal", "topic": "x"}});
        let err = serde_json::from_value::<Step>(bad_until)
            .unwrap_err()
            .to_string();
        assert!(err.contains("topic"), "{err}");
        let bad_boundary = json!({
            "id": "g", "name": "G", "kind": "approval", "prompt": "?",
            "boundaries": [{"name": "late", "on": {"event": "after", "secs": 5}, "act": "divert", "to": "x"}]
        });
        assert!(serde_json::from_value::<Step>(bad_boundary).is_err());

        let mut gate = approval("g", vec![Flow::to(sid("a")), labelled("b", "late")]);
        gate.boundaries = vec![timeout("late", 60), reminder("nudge")];
        let value = serde_json::to_value(&gate).unwrap();
        assert_eq!(serde_json::from_value::<Step>(value).unwrap(), gate);

        let parallel = json!({"id": "p", "name": "P", "kind": "parallel", "then": ["a", "b"]});
        assert_eq!(
            serde_json::from_value::<Step>(parallel).unwrap().kind,
            StepKind::Parallel
        );
        let end = json!({"id": "e", "name": "E", "kind": "end"});
        assert_eq!(
            serde_json::from_value::<Step>(end).unwrap().kind,
            StepKind::End {
                finish: Finish::Path
            }
        );
    }

    #[test]
    fn a_starts_cadence_and_conversation_read_from_inputs_are_judged_when_they_bind() {
        let mut timed = workflow(vec![
            start(
                "s",
                StartOn::Schedule {
                    schedule: Schedule {
                        every: Some(ValueRef::Input {
                            input: InputName::new("interval").unwrap(),
                        }),
                        ..Schedule::default()
                    },
                },
                &["a"],
            ),
            agent("a", &[]),
        ]);
        timed.inputs = vec![input("interval", InputKind::Number)];
        let every = |v: serde_json::Value| BTreeMap::from([("interval".to_string(), v)]);
        assert_eq!(
            timed.validate_bound(&goal_scope(), &every(json!(60)), &[], &Picky),
            vec![]
        );
        assert_eq!(
            kinds(&timed.validate_bound(&goal_scope(), &every(json!("soon")), &[], &Picky)),
            vec![ProblemKind::InputKindMismatch]
        );
        let mut listens = workflow(vec![
            start(
                "s",
                StartOn::Message {
                    filter: MessageFilter {
                        r#in: Some("{inputs.channel}".into()),
                        ..MessageFilter::default()
                    },
                },
                &["a"],
            ),
            agent("a", &[]),
        ]);
        listens.inputs = vec![input("channel", InputKind::Text)];
        let at = |c: &str| BTreeMap::from([("channel".to_string(), json!(c))]);
        assert_eq!(
            listens.validate_bound(&goal_scope(), &at("support"), &[], &Picky),
            vec![]
        );
        assert_eq!(
            kinds(&listens.validate_bound(&goal_scope(), &at("no place!"), &[], &Picky)),
            vec![ProblemKind::NotifyScopeUnknown]
        );
    }

    #[test]
    fn every_kind_has_a_family() {
        let families: Vec<(&str, Family)> = [
            StepKind::Parallel,
            StepKind::End {
                finish: Finish::Path,
            },
            StepKind::Approval { prompt: "?".into() },
        ]
        .iter()
        .map(|k| (k.as_str(), k.family()))
        .collect();
        assert_eq!(
            families,
            [
                ("parallel", Family::Gateway),
                ("end", Family::Event),
                ("approval", Family::Task)
            ]
        );
    }
    // added by the coverage pass: workflow.rs

    #[test]
    fn every_family_pick_finish_join_on_fail_condition_and_input_kind_has_its_wire_word() {
        assert_eq!(
            [Family::Event, Family::Gateway, Family::Loop, Family::Task].map(Family::as_str),
            ["event", "gateway", "loop", "task"]
        );
        assert_eq!(
            (Pick::First.as_str(), Pick::Every.as_str()),
            ("first", "every")
        );
        assert_eq!(
            (
                Finish::Path.as_str(),
                Finish::Done.as_str(),
                Finish::Failed.as_str()
            ),
            ("path", "done", "failed")
        );
        assert_eq!(
            (Join::All.as_str(), Join::Any.as_str(), Join::One.as_str()),
            ("all", "any", "one")
        );
        assert_eq!(
            (
                OnFail::Fail.as_str(),
                OnFail::Skip.as_str(),
                OnFail::Then { step: sid("x") }.as_str()
            ),
            ("fail", "skip", "then")
        );
        let conditions = [
            (
                Condition::OutputEquals {
                    step: sid("a"),
                    path: "p".into(),
                    value: json!(1),
                },
                "output_equals",
            ),
            (
                Condition::OutputMatches {
                    step: sid("a"),
                    path: "p".into(),
                    contains: "x".into(),
                },
                "output_matches",
            ),
            (
                Condition::Between {
                    from_hour: 1,
                    to_hour: 2,
                },
                "between",
            ),
            (Condition::All { of: vec![] }, "all"),
            (Condition::Any { of: vec![] }, "any"),
        ];
        for (condition, word) in &conditions {
            assert_eq!(condition.as_str(), *word);
        }
        assert_eq!(
            (
                InputKind::Bool.as_str(),
                InputKind::Choice { options: vec![] }.as_str()
            ),
            ("bool", "choice")
        );
        assert_eq!(InputKind::fields_of("nope"), None);
        assert_eq!(WaitFor::fields_of("nope"), None);
        assert!(WaitFor::refuse_unknown::<serde_json::Error>(&json!("delay")).is_ok());
        assert!(!contains_str(None, "x"));
        assert!(format!("{:?}", ctx()).starts_with("ValidationCtx"));
        assert_eq!(StepKind::Parallel.promised().fields(), Some(vec![]));
        assert_eq!(
            StepKind::Human {
                prompt: "p".into(),
                options: vec![],
                multi: false,
                assignee: None,
            }
            .promised()
            .fields(),
            Some(vec![])
        );
        assert_eq!(
            StepKind::ForEach {
                items: "x".into(),
                max_iterations: 1,
            }
            .family(),
            Family::Loop
        );
        let otherwise = Branch::new("o").unwrap();
        assert!(matches!(
            StepKind::Switch { on: "x".into(), cases: vec![], otherwise: otherwise.clone() }.promised(),
            Promised::Keys(keys) if keys == ["value"]
        ));
        assert!(matches!(
            StepKind::Judge {
                state: "s".into(),
                instructions: "i".into(),
                options: vec![],
                otherwise,
                min_confidence: None,
            }
            .promised(),
            Promised::Keys(keys) if keys == ["choice", "confidence", "judged"]
        ));
        assert!(matches!(
            StepKind::While { when: Condition::All { of: vec![] }, max_iterations: 1 }.promised(),
            Promised::Keys(keys) if keys == ["index"]
        ));
    }

    #[test]
    fn every_step_kind_and_every_start_summarises_itself_in_one_line() {
        let s = |kind: StepKind| step("s", kind, vec![]).summary().id.to_string();
        assert_eq!(
            s(StepKind::Approval {
                prompt: "Ship it?".into()
            }),
            "step-summary-approval"
        );
        assert_eq!(
            s(StepKind::Check {
                check: CheckKind::Schema {
                    schema: json!({}),
                    of: Some(sid("a")),
                },
            }),
            "step-summary-check-schema"
        );
        let conn = |c: Option<&str>, o: Option<&str>, params: &[&str]| StepKind::Connector {
            connector: c.map(|c| ConnectorId::new(c).unwrap()),
            operation: o.map(|o| OperationId::new(o).unwrap()),
            account: None,
            params: params
                .iter()
                .map(|p| (p.to_string(), "{inputs.x}".to_string()))
                .collect(),
            output_schema: None,
            unattended: false,
        };
        assert_eq!(
            s(conn(Some("slack"), Some("post"), &[])),
            "step-summary-connector"
        );
        assert_eq!(
            s(conn(Some("slack"), None, &["text"])),
            "step-summary-connector-operation-unchosen-params"
        );
        assert_eq!(
            s(conn(None, None, &["text"])),
            "step-summary-connector-unchosen-params"
        );
        let project = ProjectId::from_ulid(ulid::Ulid::from_parts(1, 1));
        let workflow_id = WorkflowId::from_ulid(ulid::Ulid::from_parts(1, 1));
        let waits = [
            (
                WaitFor::Delay {
                    secs: ValueRef::Fixed(5),
                },
                "step-summary-wait-delay",
            ),
            (
                WaitFor::Time {
                    at: "tomorrow".into(),
                },
                "step-summary-wait-time",
            ),
            (
                WaitFor::Schedule {
                    cron: ValueRef::Fixed("0 9 * * *".into()),
                    tz: None,
                },
                "step-summary-wait-schedule",
            ),
            (
                WaitFor::Signal {
                    filter: SignalFilter {
                        name: "x.ready".into(),
                        fields: BTreeMap::new(),
                    },
                },
                "step-summary-wait-signal",
            ),
            (
                WaitFor::Message {
                    filter: MessageFilter {
                        r#in: Some("support".into()),
                        ..MessageFilter::default()
                    },
                },
                "step-summary-wait-message-in",
            ),
            (
                WaitFor::Message {
                    filter: MessageFilter::default(),
                },
                "step-summary-wait-message",
            ),
            (
                WaitFor::Project {
                    filter: ProjectFilter {
                        project: Some(ValueRef::Fixed(project)),
                        ..ProjectFilter::default()
                    },
                },
                "step-summary-wait-project",
            ),
            (
                WaitFor::Project {
                    filter: ProjectFilter::default(),
                },
                "step-summary-wait-project-unchosen",
            ),
            (
                WaitFor::Run {
                    filter: RunFilter {
                        workflow: Some(workflow_id),
                        outcome: None,
                    },
                },
                "step-summary-wait-run-of",
            ),
            (
                WaitFor::Run {
                    filter: RunFilter::default(),
                },
                "step-summary-wait-run",
            ),
            (
                WaitFor::Platform {
                    filter: PlatformFilter {
                        topic: "goal.closed".into(),
                        fields: BTreeMap::new(),
                    },
                },
                "step-summary-wait-platform",
            ),
        ];
        for (until, id) in waits {
            assert_eq!(s(StepKind::Wait { until }), id);
        }
        let agent_author = Some(ValueRef::Fixed(Assignee::Agent("dev".into())));
        let notify = |scope: Option<&str>, author: Option<ValueRef<Assignee>>| StepKind::Notify {
            scope: scope.map(str::to_string),
            template: "done".into(),
            mentions: vec![],
            author,
        };
        assert_eq!(
            s(notify(Some("general"), agent_author.clone())),
            "step-summary-notify-scope-as"
        );
        assert_eq!(
            s(notify(Some("general"), None)),
            "step-summary-notify-scope"
        );
        assert_eq!(s(notify(None, agent_author)), "step-summary-notify-as");
        assert_eq!(s(notify(None, None)), "step-summary-notify");
        let spawn = |wait: bool| StepKind::Spawn {
            statement_template: "child".into(),
            workflow: None,
            assignees: vec![],
            inputs: BTreeMap::new(),
            wait,
        };
        assert_eq!(s(spawn(true)), "step-summary-spawn");
        assert_eq!(s(spawn(false)), "step-summary-spawn-no-wait");
        assert_eq!(
            s(StepKind::End {
                finish: Finish::Failed
            }),
            "step-summary-end-failed"
        );
        let start = |on: StartOn| on.summary().id.to_string();
        assert_eq!(
            start(StartOn::Message {
                filter: MessageFilter::default()
            }),
            "step-summary-start-message"
        );
        assert_eq!(
            start(StartOn::Project {
                filter: ProjectFilter {
                    project: Some(ValueRef::Fixed(project)),
                    ..ProjectFilter::default()
                }
            }),
            "step-summary-start-project"
        );
        assert_eq!(
            start(StartOn::Project {
                filter: ProjectFilter::default()
            }),
            "step-summary-start-project-unchosen"
        );
        assert_eq!(
            start(StartOn::Run {
                filter: RunFilter {
                    workflow: Some(workflow_id),
                    outcome: None,
                }
            }),
            "step-summary-start-run-of"
        );
        assert_eq!(
            start(StartOn::Platform {
                filter: PlatformFilter {
                    topic: "goal.closed".into(),
                    fields: BTreeMap::new(),
                }
            }),
            "step-summary-start-platform"
        );
        assert_eq!(
            start(StartOn::Connector {
                connector: None,
                operation: None,
                account: None,
                params: BTreeMap::new(),
                key: None,
                schedule: Schedule::every(60),
            }),
            "step-summary-start-connector-unchosen"
        );
        assert_eq!(
            start(StartOn::Schedule {
                schedule: Schedule::default()
            }),
            "step-summary-start-schedule-unset"
        );
    }

    #[test]
    fn no_way_in_a_check_of_a_step_that_is_not_there_and_a_rule_on_an_input_nobody_declared_are_named(
    ) {
        // A loop back to the first step is not a missing start: the first
        // step is the way in. Only a definition with no step at all has none.
        let ring = workflow(vec![agent("a", &["b"]), agent("b", &["a"])]);
        assert!(!kinds(&ring.validate(&ctx())).contains(&ProblemKind::NoStart));
        assert!(kinds(&workflow(vec![]).validate(&ctx())).contains(&ProblemKind::NoStart));
        let checks = workflow(vec![step(
            "a",
            StepKind::Check {
                check: CheckKind::Schema {
                    schema: json!({"type": "object"}),
                    of: Some(sid("ghost")),
                },
            },
            vec![],
        )]);
        assert!(kinds(&checks.validate(&ctx())).contains(&ProblemKind::UnknownStep));
        let branch = |b: &str| Flow {
            to: sid("c"),
            branch: Some(Branch::new(b).unwrap()),
        };
        let tests = workflow(vec![
            agent("a", &["b"]),
            decide(
                vec![(
                    "go",
                    Condition::InputEquals {
                        input: InputName::new("nope").unwrap(),
                        value: json!(1),
                    },
                )],
                "stop",
                vec![branch("go"), branch("stop")],
            ),
            agent("c", &[]),
        ]);
        assert!(kinds(&tests.validate(&ctx())).contains(&ProblemKind::UnknownInput));
    }

    #[test]
    fn a_template_that_names_a_step_that_is_not_there_or_a_connectors_own_roots_is_named() {
        let wf = workflow(vec![
            agent_reading("a", "read {steps.ghost.output}", &["b"]),
            agent_reading("b", "read {params.x}", &[]),
        ]);
        let k = kinds(&wf.validate(&ctx()));
        assert!(
            k.contains(&ProblemKind::UnknownStep) && k.contains(&ProblemKind::ParamOnlyPlaceholder),
            "{k:?}"
        );
    }

    #[test]
    fn a_start_whose_mapping_does_not_parse_whose_event_reads_an_undeclared_input_or_names_an_unknown_project_is_named(
    ) {
        let start = |on: StartOn, inputs: BTreeMap<String, String>| {
            step(
                "s",
                StepKind::Start {
                    on,
                    inputs,
                    guard: Guard::default(),
                },
                vec![Flow::to(sid("a"))],
            )
        };
        let mapping = workflow(vec![
            start(
                StartOn::Hook { public: false },
                BTreeMap::from([("x".to_string(), "{event.payload".to_string())]),
            ),
            agent("a", &[]),
        ]);
        assert!(kinds(&mapping.validate(&ctx())).contains(&ProblemKind::BadTemplate));
        let fields = |value: &str| {
            workflow(vec![
                start(
                    StartOn::Signal {
                        filter: SignalFilter {
                            name: "x.ready".into(),
                            fields: BTreeMap::from([("env".to_string(), value.to_string())]),
                        },
                    },
                    BTreeMap::new(),
                ),
                agent("a", &[]),
            ])
        };
        assert!(
            kinds(&fields("{inputs.nope}").validate(&ctx())).contains(&ProblemKind::UnknownInput)
        );
        assert!(kinds(&fields("{").validate(&ctx())).contains(&ProblemKind::BadTemplate));
        let check = workflow(vec![
            start(
                StartOn::Check {
                    command: "true".into(),
                    project: Some(ValueRef::Fixed(ProjectId::from_ulid(
                        ulid::Ulid::from_parts(5, 5),
                    ))),
                    fire_on: crate::start::FireOn::StartsFailing,
                    schedule: Schedule::every(60),
                },
                BTreeMap::new(),
            ),
            agent("a", &[]),
        ]);
        assert!(kinds(&check.validate(&ctx())).contains(&ProblemKind::UnknownProject));
    }

    #[test]
    fn a_boundary_that_posts_into_no_channel_speaks_as_no_agent_or_names_a_signal_badly_is_named() {
        use crate::boundary::{Boundary, BoundaryAct, BoundaryOn};
        let mut a = agent("a", &[]);
        let timer = |name: &str, act: BoundaryAct| Boundary {
            name: Branch::new(name).unwrap(),
            on: BoundaryOn::After {
                secs: ValueRef::Fixed(60),
            },
            act,
        };
        a.boundaries = vec![
            timer(
                "late",
                BoundaryAct::Notify {
                    scope: Some("not a channel!".into()),
                    template: "late".into(),
                    mentions: vec![],
                    author: Some(ValueRef::Fixed(Assignee::Human(pk()))),
                },
            ),
            timer(
                "loud",
                BoundaryAct::Emit {
                    signal: "Bad Signal".into(),
                    payload: BTreeMap::new(),
                },
            ),
            Boundary {
                name: Branch::new("heard").unwrap(),
                on: BoundaryOn::Signal {
                    filter: SignalFilter {
                        name: "Bad Signal".into(),
                        fields: BTreeMap::new(),
                    },
                },
                act: BoundaryAct::Divert,
            },
        ];
        let problems = workflow(vec![a]).validate(&ctx());
        let k = kinds(&problems);
        assert!(
            k.contains(&ProblemKind::NotifyScopeUnknown)
                && k.contains(&ProblemKind::NotifyAuthorNotAnAgent),
            "{k:?}"
        );
        assert!(
            problems.len() >= 4,
            "the two signal names are refused too: {problems:?}"
        );
    }

    #[test]
    fn a_boundary_whose_scope_or_author_is_read_from_the_inputs_is_judged_once_they_are_bound() {
        use crate::boundary::{Boundary, BoundaryAct, BoundaryOn};
        let mut wf = workflow(vec![agent("a", &[])]);
        wf.inputs = vec![
            input("place", InputKind::Text),
            input("who", InputKind::Assignee),
        ];
        wf.steps[0].boundaries = vec![Boundary {
            name: Branch::new("late").unwrap(),
            on: BoundaryOn::After {
                secs: ValueRef::Fixed(60),
            },
            act: BoundaryAct::Notify {
                scope: Some("{inputs.place}".into()),
                template: "late".into(),
                mentions: vec![],
                author: Some(ValueRef::Input {
                    input: InputName::new("who").unwrap(),
                }),
            },
        }];
        let inputs = BTreeMap::from([
            ("place".to_string(), json!("not a channel!")),
            (
                "who".to_string(),
                json!(format!("human:{}", "ab".repeat(32))),
            ),
        ]);
        let k = kinds(&wf.validate_bound(&goal_scope(), &inputs, &[], &NoSyntaxChecks));
        assert!(
            k.contains(&ProblemKind::NotifyScopeUnknown)
                && k.contains(&ProblemKind::NotifyAuthorNotAnAgent),
            "{k:?}"
        );
    }

    #[test]
    fn a_poll_names_its_missing_operation_a_file_parameter_it_cannot_carry_and_an_account_nobody_holds(
    ) {
        let mut slack = slack();
        let mut upload = slack.operations[0].clone();
        upload.id = OperationId::new("upload").unwrap();
        upload.params.push(crate::connector::ParamDef {
            name: InputName::new("doc").unwrap(),
            label: "Doc".into(),
            kind: crate::connector::ParamKind::File,
            required: true,
            doc: String::new(),
        });
        slack.operations.push(upload);
        let connectors = vec![slack];
        let accounts = vec![account("slack", true)];
        let vctx = ValidationCtx {
            connectors: &connectors,
            accounts: &accounts,
            checks: &Picky,
            ..ctx()
        };
        let poll = |operation: Option<&str>, acct: Option<ValueRef<AccountId>>| {
            workflow(vec![
                step(
                    "s",
                    StepKind::Start {
                        on: StartOn::Connector {
                            connector: Some(ConnectorId::new("slack").unwrap()),
                            operation: operation.map(|o| OperationId::new(o).unwrap()),
                            account: acct,
                            params: BTreeMap::new(),
                            key: Some("id".into()),
                            schedule: Schedule::every(60),
                        },
                        inputs: BTreeMap::new(),
                        guard: Guard::default(),
                    },
                    vec![Flow::to(sid("a"))],
                ),
                agent("a", &[]),
            ])
        };
        assert!(kinds(&poll(None, None).validate(&vctx)).contains(&ProblemKind::Unfilled));
        let stranger = AccountId::from_ulid(ulid::Ulid::from_parts(9, 9));
        let k = kinds(&poll(Some("upload"), Some(ValueRef::Fixed(stranger))).validate(&vctx));
        assert!(
            k.contains(&ProblemKind::BadPoll) && k.contains(&ProblemKind::UnknownAccount),
            "{k:?}"
        );
        // A connector step whose output schema the checker refuses.
        let bad_schema = workflow(vec![step(
            "call",
            StepKind::Connector {
                connector: Some(ConnectorId::new("slack").unwrap()),
                operation: Some(connectors[0].operations[0].id.clone()),
                account: None,
                params: BTreeMap::new(),
                output_schema: Some(json!({"bogus": true})),
                unattended: true,
            },
            vec![],
        )]);
        assert!(kinds(&bad_schema.validate(&vctx)).contains(&ProblemKind::BadSchema));
    }

    #[test]
    fn a_loops_body_and_sides_are_empty_for_a_step_that_is_not_there_and_an_empty_workflow_assures_nothing(
    ) {
        let wf = workflow(vec![agent("a", &[])]);
        assert!(wf.loop_body(&sid("ghost")).is_empty());
        assert_eq!(
            wf.loop_sides(&sid("ghost")),
            (BTreeSet::new(), BTreeSet::new())
        );
        assert!(workflow(vec![]).assurance().is_empty());
        assert!(wf.may_fail_in(&sid("ghost"), FwdEdge::Fail, &sid("a")));
    }
    // added by the coverage pass: b4-workflow.rs
    #[test]
    fn the_remaining_wire_words_a_wait_a_run_outcome_a_value_an_account_kind_a_branch_and_a_step_id(
    ) {
        let every_wait = [
            WaitFor::Delay {
                secs: ValueRef::Fixed(1),
            },
            WaitFor::Time { at: "x".into() },
            WaitFor::Schedule {
                cron: ValueRef::Fixed("x".into()),
                tz: None,
            },
            WaitFor::Signal {
                filter: SignalFilter {
                    name: "x".into(),
                    fields: BTreeMap::new(),
                },
            },
            WaitFor::Message {
                filter: MessageFilter::default(),
            },
            WaitFor::Project {
                filter: ProjectFilter::default(),
            },
            WaitFor::Run {
                filter: RunFilter::default(),
            },
            WaitFor::Platform {
                filter: PlatformFilter {
                    topic: "t".into(),
                    fields: BTreeMap::new(),
                },
            },
            WaitFor::Release,
        ];
        for (until, name) in every_wait.iter().zip(WaitFor::NAMES) {
            assert_eq!(until.as_str(), name);
        }
        assert_eq!(
            (RunOutcome::Done.as_str(), RunOutcome::Failed.as_str()),
            ("done", "failed")
        );
        assert_eq!(
            ValueRef::<u64>::Input {
                input: InputName::new("n").unwrap()
            }
            .word(),
            "{inputs.n}"
        );
        assert_eq!(InputKind::Account { connector: None }.as_str(), "account");
        assert!(format!("{:?}", Branch::new("yes").unwrap()).contains("yes"));
        assert_eq!(
            "yes".parse::<Branch>().unwrap(),
            Branch::new("yes").unwrap()
        );
        let id = sid("a");
        let text: &str = id.as_ref();
        assert_eq!(text, "a");
        assert!(id == *"a");
        assert!(id == "a");
        assert!(!workflow(vec![]).is_archived());
        let schema = serde_json::to_value(schemars::schema_for!(StepId)).unwrap();
        assert_eq!(schema["type"], "string", "{schema}");
    }

    #[test]
    fn a_wait_names_its_templates_its_inputs_and_its_assignees_by_its_kind() {
        let n = InputName::new("n").unwrap();
        assert_eq!(
            WaitFor::Delay {
                secs: ValueRef::Input { input: n.clone() }
            }
            .input_refs(),
            vec![(&n, "number")]
        );
        assert_eq!(
            WaitFor::Schedule {
                cron: ValueRef::Input { input: n.clone() },
                tz: None,
            }
            .input_refs(),
            vec![(&n, "text")]
        );
        let project = WaitFor::Project {
            filter: ProjectFilter {
                project: Some(ValueRef::Input { input: n.clone() }),
                branch: Some("{inputs.b}".into()),
                ..ProjectFilter::default()
            },
        };
        assert_eq!(project.input_refs(), vec![(&n, "project")]);
        assert_eq!(project.templates(), vec!["{inputs.b}"]);
        assert_eq!(
            WaitFor::Time {
                at: "{inputs.at}".into()
            }
            .templates(),
            vec!["{inputs.at}"]
        );
        assert!(WaitFor::Signal {
            filter: SignalFilter {
                name: "x.{inputs.n}".into(),
                fields: BTreeMap::new(),
            },
        }
        .templates()
        .contains(&"x.{inputs.n}"));
        assert!(WaitFor::Platform {
            filter: PlatformFilter {
                topic: "t".into(),
                fields: BTreeMap::from([("k".to_string(), "{inputs.v}".to_string())]),
            },
        }
        .templates()
        .contains(&"{inputs.v}"));
        let who = ValueRef::Fixed(Assignee::Agent("a".into()));
        let message = WaitFor::Message {
            filter: MessageFilter {
                mentions: Some(who.clone()),
                ..MessageFilter::default()
            },
        };
        assert_eq!(message.assignee_refs(), vec![&who]);
        assert!(WaitFor::Release.assignee_refs().is_empty());
        assert!(WaitFor::Release.templates().is_empty());
        assert!(WaitFor::Release.input_refs().is_empty());
    }

    #[test]
    fn a_condition_names_the_steps_it_reads_a_value_is_searched_as_text_and_a_number_given_text_is_refused(
    ) {
        let reads = Condition::All {
            of: vec![
                Condition::Answered {
                    step: sid("a"),
                    option: "x".into(),
                },
                Condition::Outcome {
                    step: sid("b"),
                    passed: true,
                },
                Condition::Between {
                    from_hour: 1,
                    to_hour: 2,
                },
            ],
        };
        assert_eq!(reads.reads_steps(), vec![&sid("a"), &sid("b")]);
        assert!(contains_str(Some(&json!(42)), "4"));
        let mut wf = workflow(vec![agent("a", &[])]);
        wf.inputs = vec![input("n", InputKind::Number)];
        assert!(matches!(
            wf.bind_inputs(BTreeMap::from([("n".to_string(), json!("three"))])),
            Err(InputError::WrongKind { want, .. }) if want == "number"
        ));
    }

    #[test]
    fn the_summaries_of_the_plainer_starts_and_steps_and_the_defaults_a_file_may_leave_out() {
        let start = |on: StartOn| on.summary().id.to_string();
        assert_eq!(start(StartOn::Manual), "step-summary-start-manual");
        assert_eq!(
            start(StartOn::Hook { public: false }),
            "step-summary-start-hook"
        );
        assert_eq!(
            start(StartOn::Hook { public: true }),
            "step-summary-start-hook-public"
        );
        assert_eq!(
            start(StartOn::Schedule {
                schedule: Schedule::every(60)
            }),
            "step-summary-start-every"
        );
        assert_eq!(
            start(StartOn::Schedule {
                schedule: Schedule::cron("0 9 * * 1", None)
            }),
            "step-summary-start-cron"
        );
        assert_eq!(
            start(StartOn::Message {
                filter: MessageFilter {
                    r#in: Some("support".into()),
                    ..MessageFilter::default()
                }
            }),
            "step-summary-start-message-in"
        );
        assert_eq!(
            start(StartOn::Message {
                filter: MessageFilter::default()
            }),
            "step-summary-start-message"
        );
        assert_eq!(
            start(StartOn::Run {
                filter: RunFilter::default()
            }),
            "step-summary-start-run"
        );
        assert_eq!(
            start(StartOn::Connector {
                connector: Some(ConnectorId::new("c").unwrap()),
                operation: Some(OperationId::new("o").unwrap()),
                account: None,
                params: BTreeMap::new(),
                key: None,
                schedule: Schedule::every(1),
            }),
            "step-summary-start-connector"
        );
        assert_eq!(
            start(StartOn::Connector {
                connector: None,
                operation: None,
                account: None,
                params: BTreeMap::new(),
                key: None,
                schedule: Schedule::every(1),
            }),
            "step-summary-start-connector-unchosen"
        );
        assert_eq!(
            start(StartOn::Check {
                command: "true".into(),
                project: None,
                fire_on: crate::start::FireOn::StartsFailing,
                schedule: Schedule::every(1),
            }),
            "step-summary-start-check"
        );
        let s = |kind: StepKind| step("s", kind, vec![]).summary().id.to_string();
        assert_eq!(
            s(StepKind::Human {
                prompt: "Ready?".into(),
                options: vec![],
                multi: false,
                assignee: None,
            }),
            "step-summary-human"
        );
        assert_eq!(
            s(StepKind::Wait {
                until: WaitFor::Release
            }),
            "step-summary-wait-release"
        );
        let mut a = agent("a", &[]);
        assert_eq!(a.summary().id, "step-summary-agent-assigned");
        if let StepKind::Agent { assignee, .. } = &mut a.kind {
            assignee.take();
        }
        assert_eq!(a.summary().id, "step-summary-agent");
        let for_each: StepKind =
            serde_json::from_value(json!({"kind": "for_each", "items": "{inputs.xs}"})).unwrap();
        assert!(matches!(
            for_each,
            StepKind::ForEach {
                max_iterations: DEFAULT_MAX_ITERATIONS,
                ..
            }
        ));
        let spawn: StepKind =
            serde_json::from_value(json!({"kind": "spawn", "statement_template": "x"})).unwrap();
        assert!(matches!(spawn, StepKind::Spawn { wait: true, .. }));
    }
    // added by the coverage pass: b5-workflow.rs
    #[test]
    fn the_words_and_readers_the_earlier_pins_left_out() {
        assert!(format!("{:?}", InputName::new("n").unwrap()).contains("n"));
        assert_eq!(
            "n".parse::<InputName>().unwrap(),
            InputName::new("n").unwrap()
        );
        assert_eq!(InputKind::Project.as_str(), "project");
        assert_eq!(
            StartOn::Signal {
                filter: SignalFilter {
                    name: "x".into(),
                    fields: BTreeMap::new(),
                },
            }
            .summary()
            .id,
            "step-summary-start-signal"
        );
        assert!(matches!(
            StepKind::Emit {
                signal: "s".into(),
                payload: BTreeMap::new(),
            }
            .promised(),
            Promised::Keys(keys) if keys == ["signal"]
        ));
        let m = InputName::new("m").unwrap();
        let message = WaitFor::Message {
            filter: MessageFilter {
                r#in: Some("{inputs.room}".into()),
                mentions: Some(ValueRef::Input { input: m.clone() }),
                ..MessageFilter::default()
            },
        };
        assert!(message.templates().contains(&"{inputs.room}"));
        assert_eq!(message.input_refs(), vec![(&m, "assignee")]);
        assert_eq!(
            Condition::OutputMatches {
                step: sid("a"),
                path: "x".into(),
                contains: "y".into(),
            }
            .reads_steps(),
            vec![&sid("a")]
        );
        assert!(contains_str(Some(&json!("abc")), "b"));
    }

    #[test]
    fn a_listening_workflow_needs_the_input_its_schedule_reads_when_nothing_fills_it() {
        let interval = InputName::new("interval").unwrap();
        let mut wf = workflow(vec![
            step(
                "s",
                StepKind::Start {
                    on: StartOn::Schedule {
                        schedule: Schedule {
                            every: Some(ValueRef::Input {
                                input: interval.clone(),
                            }),
                            cron: None,
                            tz: None,
                        },
                    },
                    inputs: BTreeMap::new(),
                    guard: Guard::default(),
                },
                vec![Flow::to(sid("a"))],
            ),
            agent("a", &[]),
        ]);
        wf.inputs = vec![input("interval", InputKind::Number)];
        assert_eq!(wf.listening_needs(), BTreeSet::from([interval]));
    }

    #[test]
    fn a_workflow_with_no_steps_has_no_start() {
        assert!(kinds(&workflow(vec![]).validate(&ctx())).contains(&ProblemKind::NoStart));
    }

    #[test]
    fn the_walks_visit_a_step_once_and_step_over_a_flow_to_nowhere() {
        // A diamond: `w` is reached twice; `ghost` is nobody's step.
        let wf = workflow(vec![
            agent("x", &["y", "z"]),
            agent("y", &["w"]),
            agent("z", &["w"]),
            agent("w", &["ghost"]),
        ]);
        assert!(!wf.returns_to(&sid("x"), &sid("t")));
        assert!(wf.returns_to(&sid("x"), &sid("w")));
        // A gate before a diamond: `c` is queued twice on the way to `d`.
        let gated = workflow(vec![
            human_step("g", &["a", "b"]),
            agent("a", &["c"]),
            agent("b", &["c"]),
            agent("c", &["d"]),
            agent("d", &[]),
        ]);
        assert!(gated.gated(&sid("d")));
        assert!(!gated.gated(&sid("g")));
        // Spawns that meet: `d` is reached from `b` and from `c`.
        let w = |n: u64| WorkflowId::from_ulid(ulid::Ulid::from_parts(n, 1));
        let spawns = vec![
            (w(1), vec![w(2), w(3)]),
            (w(2), vec![w(4)]),
            (w(3), vec![w(4)]),
            (w(4), vec![]),
        ];
        assert!(!spawn_reaches(&spawns, w(1), w(5)));
        assert!(spawn_reaches(&spawns, w(1), w(4)));
    }

    #[test]
    fn a_rule_may_read_an_answer_of_a_question_upstream_and_a_match_on_an_output_upstream() {
        let labelled = |to: &str, branch: &str| Flow {
            to: sid(to),
            branch: Some(Branch::new(branch).unwrap()),
        };
        let answered = workflow(vec![
            human_step("h", &["d"]),
            decide(
                vec![(
                    "yes",
                    Condition::Answered {
                        step: sid("h"),
                        option: "yes".into(),
                    },
                )],
                "no",
                vec![labelled("a", "yes"), labelled("b", "no")],
            ),
            agent("a", &[]),
            agent("b", &[]),
        ]);
        let found = kinds(&answered.validate(&ctx()));
        assert!(!found.contains(&ProblemKind::NotUpstream), "{found:?}");
        let matched = workflow(vec![
            agent("p", &["d"]),
            decide(
                vec![(
                    "yes",
                    Condition::OutputMatches {
                        step: sid("p"),
                        path: "summary".into(),
                        contains: "ok".into(),
                    },
                )],
                "no",
                vec![labelled("a", "yes"), labelled("b", "no")],
            ),
            agent("a", &[]),
            agent("b", &[]),
        ]);
        let found = kinds(&matched.validate(&ctx()));
        assert!(!found.contains(&ProblemKind::NotUpstream), "{found:?}");
    }

    #[test]
    fn a_message_boundary_a_goal_placeholder_and_the_outside_waits_are_validated() {
        let mut watched = agent("a", &["b"]);
        watched.then.push(Flow {
            to: sid("c"),
            branch: Some(Branch::new("cancelled").unwrap()),
        });
        watched.boundaries = vec![Boundary {
            name: Branch::new("cancelled").unwrap(),
            on: BoundaryOn::Message {
                filter: MessageFilter {
                    contains: Some("cancel".into()),
                    ..MessageFilter::default()
                },
            },
            act: BoundaryAct::Divert,
        }];
        let wf = workflow(vec![watched, agent("b", &[]), agent("c", &[])]);
        let found = kinds(&wf.validate(&ctx()));
        assert!(
            !found.contains(&ProblemKind::NotifyScopeUnknown),
            "{found:?}"
        );
        let reading = workflow(vec![agent_reading("a", "do it for {goal.title}", &[])]);
        let found = kinds(&reading.validate(&ctx()));
        assert!(!found.contains(&ProblemKind::UnknownInput), "{found:?}");
        let wait = |id: &str, until: WaitFor, then: &[&str]| {
            step(
                id,
                StepKind::Wait { until },
                then.iter().map(|t| Flow::to(sid(t))).collect(),
            )
        };
        let waits = workflow(vec![
            agent("a", &["m"]),
            wait(
                "m",
                WaitFor::Message {
                    filter: MessageFilter::default(),
                },
                &["p"],
            ),
            wait(
                "p",
                WaitFor::Project {
                    filter: ProjectFilter::default(),
                },
                &["r"],
            ),
            wait(
                "r",
                WaitFor::Run {
                    filter: RunFilter::default(),
                },
                &["t"],
            ),
            wait(
                "t",
                WaitFor::Platform {
                    filter: PlatformFilter {
                        topic: "goal.closed".into(),
                        fields: BTreeMap::new(),
                    },
                },
                &[],
            ),
        ]);
        let found = kinds(&waits.validate(&ctx()));
        assert!(found.contains(&ProblemKind::Unfilled), "{found:?}");
        let p = InputName::new("p").unwrap();
        let mut by_input = workflow(vec![
            agent("a", &["w"]),
            wait(
                "w",
                WaitFor::Project {
                    filter: ProjectFilter {
                        project: Some(ValueRef::Input { input: p }),
                        ..ProjectFilter::default()
                    },
                },
                &[],
            ),
        ]);
        by_input.inputs = vec![input("p", InputKind::Project)];
        let found = kinds(&by_input.validate(&ctx()));
        assert!(
            !found.contains(&ProblemKind::Unfilled)
                && !found.contains(&ProblemKind::UnknownProject),
            "{found:?}"
        );
    }
    // added by the coverage pass: b7-workflow.rs
    #[test]
    fn a_bound_run_reads_its_clocks_its_cron_and_its_posts_from_the_inputs_it_was_given() {
        let labelled = |to: &str, branch: &str| Flow {
            to: sid(to),
            branch: Some(Branch::new(branch).unwrap()),
        };
        let mut watched = agent("a", &["b"]);
        watched.then.push(labelled("c", "late"));
        watched.boundaries = vec![
            Boundary {
                name: Branch::new("late").unwrap(),
                on: BoundaryOn::After {
                    secs: ValueRef::Input {
                        input: InputName::new("remind").unwrap(),
                    },
                },
                act: BoundaryAct::Divert,
            },
            Boundary {
                name: Branch::new("nudge").unwrap(),
                on: BoundaryOn::Every {
                    secs: ValueRef::Fixed(60),
                    max: 2,
                },
                act: BoundaryAct::Notify {
                    scope: None,
                    template: "Still here.".into(),
                    mentions: vec![],
                    author: None,
                },
            },
        ];
        let mut wf = workflow(vec![
            step(
                "s",
                StepKind::Start {
                    on: StartOn::Schedule {
                        schedule: Schedule {
                            every: None,
                            cron: Some(ValueRef::Input {
                                input: InputName::new("when").unwrap(),
                            }),
                            tz: None,
                        },
                    },
                    inputs: BTreeMap::new(),
                    guard: Guard::default(),
                },
                vec![Flow::to(sid("a"))],
            ),
            watched,
            agent("b", &[]),
            agent("c", &[]),
        ]);
        wf.inputs = vec![
            input("remind", InputKind::Number),
            input("when", InputKind::Text),
        ];
        let bound = |remind: serde_json::Value, when: serde_json::Value| {
            BTreeMap::from([("remind".to_string(), remind), ("when".to_string(), when)])
        };
        let found = kinds(&wf.validate_bound(
            &goal_scope(),
            &bound(json!(5), json!("0 9 * * 1")),
            &[],
            &Picky,
        ));
        assert!(
            !found.contains(&ProblemKind::InputKindMismatch)
                && !found.contains(&ProblemKind::BadCron),
            "{found:?}"
        );
        let found =
            kinds(&wf.validate_bound(&goal_scope(), &bound(json!("soon"), json!(9)), &[], &Picky));
        assert!(
            found.contains(&ProblemKind::InputKindMismatch)
                && found.contains(&ProblemKind::BadCron),
            "{found:?}"
        );
        // A message start whose conversation is a literal is validate()'s to
        // judge, not the binding's.
        let literal = workflow(vec![
            step(
                "s",
                StepKind::Start {
                    on: StartOn::Message {
                        filter: MessageFilter {
                            r#in: Some("support".into()),
                            ..MessageFilter::default()
                        },
                    },
                    inputs: BTreeMap::new(),
                    guard: Guard::default(),
                },
                vec![Flow::to(sid("a"))],
            ),
            agent("a", &[]),
        ]);
        let found = kinds(&literal.validate_bound(&goal_scope(), &BTreeMap::new(), &[], &Picky));
        assert!(
            !found.contains(&ProblemKind::NotifyScopeUnknown),
            "{found:?}"
        );
    }

    #[test]
    fn an_output_rule_with_no_path_names_no_field() {
        let labelled = |to: &str, branch: &str| Flow {
            to: sid(to),
            branch: Some(Branch::new(branch).unwrap()),
        };
        let wf = workflow(vec![
            agent("p", &["d"]),
            decide(
                vec![(
                    "yes",
                    Condition::OutputEquals {
                        step: sid("p"),
                        path: String::new(),
                        value: json!(1),
                    },
                )],
                "no",
                vec![labelled("a", "yes"), labelled("b", "no")],
            ),
            agent("a", &[]),
            agent("b", &[]),
        ]);
        let found = kinds(&wf.validate(&ctx()));
        assert!(!found.contains(&ProblemKind::NotUpstream), "{found:?}");
    }
}

#[cfg(test)]
mod summary_tests {
    use super::tests::sid;
    use super::*;

    #[test]
    fn a_first_sentence_is_one_line_cut_at_the_limit() {
        assert_eq!(
            first_sentence("Build the site.\nThen deploy it.", 140),
            "Build the site."
        );
        assert_eq!(
            first_sentence("No terminator here", 140),
            "No terminator here"
        );
        assert_eq!(
            first_sentence("Version 1.2 ships. Then more.", 140),
            "Version 1.2 ships.",
            "a dot inside a number is not an end"
        );
        let long = "a".repeat(200);
        let cut = first_sentence(&long, 10);
        assert_eq!(cut.chars().count(), 10);
        assert!(cut.ends_with('…'));
    }

    #[test]
    fn every_kind_summarises_in_one_line_a_person_can_judge() {
        let step = |kind: StepKind| Step {
            id: StepId::new("s").unwrap(),
            name: "S".into(),
            kind,
            then: vec![],
            boundaries: vec![],
            join: Join::All,
            on_fail: OnFail::Fail,
            retries: 0,
            max_visits: 3,
            position: None,
        };
        let agent = step(StepKind::Agent {
            instructions: "Write the parser. Cover every edge case.".into(),
            assignee: Some(ValueRef::Fixed(Assignee::Agent("dev".into()))),
            project: None,
            harness: vec![],
            model: None,
            effort: None,
            output_schema: None,
            tier_ceiling: ToolTier::Write,
        });
        let s = agent.summary().to_string();
        assert!(
            s.starts_with("Write the parser.") && s.contains("dev"),
            "{s}"
        );
        assert!(!s.contains("edge case"), "one sentence only: {s}");
        let human = step(StepKind::Human {
            prompt: "Which tone?".into(),
            options: vec![
                AskOption::new("warm", "Warm"),
                AskOption::new("formal", "Formal"),
            ],
            multi: false,
            assignee: None,
        });
        assert_eq!(human.summary().to_string(), "Which tone? (Warm / Formal)");
        let check = step(StepKind::Check {
            check: CheckKind::Command {
                command: "cargo test".into(),
            },
        });
        assert_eq!(
            check.summary().to_string(),
            "passes when `cargo test` exits 0"
        );
        let decide = step(StepKind::Decide {
            rules: vec![],
            otherwise: Branch::new("ship").unwrap(),
            pick: Pick::First,
        });
        assert_eq!(decide.summary().to_string(), "branches: ship (otherwise)");
        let every = step(StepKind::Decide {
            rules: vec![Rule {
                when: Condition::Between {
                    from_hour: 9,
                    to_hour: 17,
                },
                branch: Branch::new("notify").unwrap(),
            }],
            otherwise: Branch::new("skip").unwrap(),
            pick: Pick::Every,
        });
        assert_eq!(
            every.summary().to_string(),
            "every rule that holds: notify · skip (otherwise)"
        );
        let end = step(StepKind::End {
            finish: Finish::Done,
        });
        assert_eq!(end.summary().to_string(), "finishes the run");
        let path = step(StepKind::End {
            finish: Finish::Path,
        });
        assert_eq!(path.summary().to_string(), "ends this path");
        let weekly = step(StepKind::Start {
            on: StartOn::Schedule {
                schedule: Schedule::cron("0 9 * * 1", None),
            },
            inputs: BTreeMap::new(),
            guard: Guard::default(),
        });
        assert_eq!(
            weekly.summary().to_string(),
            "begins on the schedule `0 9 * * 1`"
        );
        let by_hand = step(StepKind::Start {
            on: StartOn::Manual,
            inputs: BTreeMap::new(),
            guard: Guard::default(),
        });
        assert_eq!(by_hand.summary().to_string(), "begins by hand");
        let failing = step(StepKind::Start {
            on: StartOn::Check {
                command: "make health".into(),
                project: None,
                fire_on: crate::start::FireOn::StartsFailing,
                schedule: Schedule::every(300),
            },
            inputs: BTreeMap::new(),
            guard: Guard::default(),
        });
        assert_eq!(
            failing.summary().to_string(),
            "begins when `make health` starts failing"
        );
        let emit = step(StepKind::Emit {
            signal: "report.ready".into(),
            payload: BTreeMap::new(),
        });
        assert_eq!(
            emit.summary().to_string(),
            "raises the signal `report.ready`"
        );
        let mut fan = step(StepKind::Parallel);
        fan.then = vec![Flow::to(sid("a")), Flow::to(sid("b"))];
        assert_eq!(fan.summary().to_string(), "takes 2 paths at once");
        for s in [
            &agent, &human, &check, &decide, &every, &end, &path, &weekly, &by_hand, &failing,
            &emit, &fan,
        ] {
            assert!(!s.summary().to_string().contains('\n'));
        }
    }
}
