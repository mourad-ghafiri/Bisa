//! Start events: the ways a run of a workflow may begin.
//!
//! A `start` step names one way in — by hand, on a schedule, when called, when
//! a message arrives, when a signal is raised, when a project changes, when a
//! run finishes, when the platform says so, when an outside platform lists
//! something new, when a check starts failing — and a workflow may carry
//! several. A run enters the start it began at; the others are skipped.
//!
//! Everything here is a definition and a pure rule over it: what each event
//! carries ([`StartOn`], [`Schedule`]), how its fields resolve against the
//! inputs a host listens with ([`StartOn::resolve`]), how one occurrence maps
//! onto a run's typed inputs ([`map_event`]), what the guard lets through
//! ([`Guard::admit`]) and when a check start fires ([`FireOn::fires`]). The
//! engine observes, enqueues and dispatches; it decides nothing these do.

use crate::assignee::Assignee;
use crate::id::{AccountId, ConnectorId, OperationId, ProjectId};
use crate::listen::{
    MessageFilter, PlatformFilter, ProjectFilter, RunFilter, SignalFilter, SignalSource,
};
use crate::template::{self, Grammar, RenderContext, TemplateCtx, TemplateError};
use crate::workflow::{refuse_unknown_keys, InputDef, InputKind, InputName, ValueRef};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::num::NonZeroU32;

/// What begins a run at a `start` step. Its fields are fixed values,
/// `{inputs.<name>}` templates or `{ input = … }` references over the inputs
/// the host listens with — resolved when the start is armed
/// ([`StartOn::resolve`]), so one template serves every workspace.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "event")]
pub enum StartOn {
    /// A person, by hand: *Run…*, a goal's start, `bisa workflow run`,
    /// `POST /workflows/{id}/runs`. Maps nothing and has no guard.
    Manual,
    /// On a cadence: `every` so many seconds, or at each `cron` occurrence in
    /// `tz`.
    Schedule {
        #[serde(flatten)]
        schedule: Schedule,
    },
    /// A call to the node: local under the control-plane token, and public —
    /// secret-authenticated from outside — only when `public` is set and the
    /// machine allows public hooks (`events.public_hooks`).
    Hook {
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        public: bool,
    },
    /// A message posted into a conversation.
    Message {
        #[serde(flatten)]
        filter: MessageFilter,
    },
    /// A named signal, raised by an `emit` step, a session, a person or an
    /// A2A task.
    Signal {
        #[serde(flatten)]
        filter: SignalFilter,
    },
    /// A change in one project: a commit, a push or fetch, a pull request, a
    /// merge, files.
    Project {
        #[serde(flatten)]
        filter: ProjectFilter,
    },
    /// A run that ended — of one workflow or of any.
    Run {
        #[serde(flatten)]
        filter: RunFilter,
    },
    /// One of the engine's own bus events, by topic — the advanced door.
    Platform {
        #[serde(flatten)]
        filter: PlatformFilter,
    },
    /// An outside platform, polled through one of a connector's **read**
    /// operations: every item its answer lists that an earlier poll did not
    /// is one occurrence. `key` is the dotted path, inside one item, of the
    /// field that tells items apart; the first poll only learns what is there.
    /// The connector and the operation are absent while the designer has not
    /// chosen them — never an empty string.
    Connector {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        connector: Option<ConnectorId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        operation: Option<OperationId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        account: Option<ValueRef<AccountId>>,
        /// The operation's parameters, as templates over the listening inputs.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        params: BTreeMap<String, String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
        #[serde(flatten)]
        schedule: Schedule,
    },
    /// A shell command on a cadence — `sh -c {inputs.command}` — judged by the
    /// command guard like a `check` step, run in its project's tree or in its
    /// own scratch folder; `fire_on` says which results begin a run.
    Check {
        command: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project: Option<ValueRef<ProjectId>>,
        #[serde(default)]
        fire_on: FireOn,
        #[serde(flatten)]
        schedule: Schedule,
    },
}

impl StartOn {
    /// The `event` tag, as the wire, the index and the designer spell it.
    pub fn as_str(&self) -> &'static str {
        match self {
            StartOn::Manual => "manual",
            StartOn::Schedule { .. } => "schedule",
            StartOn::Hook { .. } => "hook",
            StartOn::Message { .. } => "message",
            StartOn::Signal { .. } => "signal",
            StartOn::Project { .. } => "project",
            StartOn::Run { .. } => "run",
            StartOn::Platform { .. } => "platform",
            StartOn::Connector { .. } => "connector",
            StartOn::Check { .. } => "check",
        }
    }

    /// Every event's tag, in the designer's order.
    pub const NAMES: [&'static str; 10] = [
        "manual",
        "schedule",
        "hook",
        "message",
        "signal",
        "project",
        "run",
        "platform",
        "connector",
        "check",
    ];

    /// The fields an event carries beside its `event` tag, by tag; `None` for
    /// a tag that is not an event. A test serializes every event and checks
    /// this table.
    pub fn fields_of(tag: &str) -> Option<&'static [&'static str]> {
        Some(match tag {
            "manual" => &[],
            "schedule" => Schedule::FIELDS,
            "hook" => &["public"],
            "message" => MessageFilter::FIELDS,
            "signal" => SignalFilter::FIELDS,
            "project" => ProjectFilter::FIELDS,
            "run" => RunFilter::FIELDS,
            "platform" => PlatformFilter::FIELDS,
            "connector" => &[
                "connector",
                "operation",
                "account",
                "params",
                "key",
                "every",
                "cron",
                "tz",
            ],
            "check" => &["command", "project", "fire_on", "every", "cron", "tz"],
            _ => return None,
        })
    }

    /// Refuse a key of a raw `on` table that no event carries — the check a
    /// step's own keys get, one level down, so `on = { event = "schedule",
    /// evry = 60 }` is an error the author sees.
    pub(crate) fn refuse_unknown<E: serde::de::Error>(raw: &serde_json::Value) -> Result<(), E> {
        let Some(map) = raw.as_object() else {
            return Ok(()); // the derived shape says what it wants instead
        };
        let tag = map
            .get("event")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| E::missing_field("event"))?;
        let own =
            StartOn::fields_of(tag).ok_or_else(|| E::unknown_variant(tag, &StartOn::NAMES))?;
        refuse_unknown_keys::<E>(map, "start event", &["event"], own)
    }

    pub fn is_manual(&self) -> bool {
        matches!(self, StartOn::Manual)
    }

    /// What a signal of this event records itself as; a manual start raises
    /// none.
    pub fn source(&self) -> Option<SignalSource> {
        Some(match self {
            StartOn::Manual => return None,
            StartOn::Schedule { .. } => SignalSource::Schedule,
            StartOn::Hook { .. } => SignalSource::Hook,
            StartOn::Message { .. } => SignalSource::Message,
            StartOn::Signal { .. } => SignalSource::Signal,
            StartOn::Project { .. } => SignalSource::Project,
            StartOn::Run { .. } => SignalSource::Run,
            StartOn::Platform { .. } => SignalSource::Platform,
            StartOn::Connector { .. } => SignalSource::Connector,
            StartOn::Check { .. } => SignalSource::Check,
        })
    }

    /// Whether auto adoption may arm this start with nobody looking: a time,
    /// a signal, a run, a platform topic or a message only reacts to what the
    /// workspace already does. A hook opens a door to the outside, a check
    /// runs a command, a poll spends an account, a project start reads a
    /// repository — each a person arms.
    pub fn arms_unattended(&self) -> bool {
        match self {
            StartOn::Manual
            | StartOn::Schedule { .. }
            | StartOn::Signal { .. }
            | StartOn::Run { .. }
            | StartOn::Platform { .. }
            | StartOn::Message { .. } => true,
            StartOn::Hook { .. }
            | StartOn::Check { .. }
            | StartOn::Connector { .. }
            | StartOn::Project { .. } => false,
        }
    }

    /// Whether this start's occurrences count against `events.fires_per_minute`.
    /// A schedule paces itself, a person's start is a person's, and a hook
    /// has its own rate buckets at the door.
    pub fn rate_limited(&self) -> bool {
        !matches!(
            self,
            StartOn::Manual | StartOn::Schedule { .. } | StartOn::Hook { .. }
        )
    }

    /// The cadence a polled or timed start keeps.
    pub fn schedule(&self) -> Option<&Schedule> {
        match self {
            StartOn::Schedule { schedule }
            | StartOn::Connector { schedule, .. }
            | StartOn::Check { schedule, .. } => Some(schedule),
            _ => None,
        }
    }

    /// Every template this event's fields carry, read under
    /// [`Grammar::StartEvent`] — the listening inputs and nothing else.
    pub fn templates(&self) -> Vec<&str> {
        match self {
            StartOn::Manual
            | StartOn::Schedule { .. }
            | StartOn::Hook { .. }
            | StartOn::Run { .. } => vec![],
            StartOn::Message { filter } => filter.templates(),
            StartOn::Signal { filter } => filter.templates(),
            StartOn::Project { filter } => filter.templates(),
            StartOn::Platform { filter } => filter.templates(),
            StartOn::Connector { params, .. } => params.values().map(String::as_str).collect(),
            StartOn::Check { command, .. } => vec![command.as_str()],
        }
    }

    /// Every input this event reads through a [`ValueRef::Input`], with the
    /// input kind the reference wants.
    pub fn input_refs(&self) -> Vec<(&InputName, &'static str)> {
        let mut out = Vec::new();
        if let Some(schedule) = self.schedule() {
            out.extend(schedule.input_refs());
        }
        match self {
            StartOn::Message { filter } => out.extend(filter.input_refs()),
            StartOn::Project { filter } => out.extend(filter.input_refs()),
            StartOn::Connector {
                account: Some(ValueRef::Input { input }),
                ..
            } => out.push((input, "account")),
            StartOn::Check {
                project: Some(ValueRef::Input { input }),
                ..
            } => out.push((input, "project")),
            _ => {}
        }
        out
    }

    /// The assignees this event names.
    pub fn assignee_refs(&self) -> Vec<&ValueRef<Assignee>> {
        match self {
            StartOn::Message { filter } => filter.assignee_refs(),
            _ => vec![],
        }
    }

    /// This event with every template rendered and every input read against
    /// the inputs its host listens with — what the engine arms. A value that
    /// cannot be resolved is an error naming it; the listener is reported
    /// and skipped, never armed on a guess.
    pub fn resolve(
        &self,
        inputs: &BTreeMap<String, serde_json::Value>,
    ) -> Result<StartOn, ResolveError> {
        let render = |tmpl: &str| render_start_field(tmpl, inputs, RenderContext::Text);
        Ok(match self {
            StartOn::Manual => StartOn::Manual,
            StartOn::Hook { public } => StartOn::Hook { public: *public },
            StartOn::Schedule { schedule } => StartOn::Schedule {
                schedule: schedule.resolve(inputs)?,
            },
            StartOn::Message { filter } => StartOn::Message {
                filter: filter.resolve(&render, &|name: &InputName| {
                    read(
                        &ValueRef::Input {
                            input: name.clone(),
                        },
                        inputs,
                        "an assignee",
                        |v| v.as_str()?.parse::<Assignee>().ok(),
                    )
                })?,
            },
            StartOn::Signal { filter } => StartOn::Signal {
                filter: filter.resolve(&render)?,
            },
            StartOn::Project { filter } => StartOn::Project {
                filter: filter.resolve(&render, &|name: &InputName| {
                    read(
                        &ValueRef::Input {
                            input: name.clone(),
                        },
                        inputs,
                        "a project id",
                        |v| v.as_str()?.parse::<ProjectId>().ok(),
                    )
                })?,
            },
            StartOn::Run { filter } => StartOn::Run {
                filter: filter.clone(),
            },
            StartOn::Platform { filter } => StartOn::Platform {
                filter: filter.resolve(&render)?,
            },
            StartOn::Connector {
                connector,
                operation,
                account,
                params,
                key,
                schedule,
            } => StartOn::Connector {
                connector: connector.clone(),
                operation: operation.clone(),
                account: account
                    .as_ref()
                    .map(|a| {
                        read(a, inputs, "an account id", |v| {
                            v.as_str()?.parse::<AccountId>().ok()
                        })
                        .map(ValueRef::Fixed)
                    })
                    .transpose()?,
                params: params
                    .iter()
                    .map(|(k, v)| Ok((k.clone(), render(v)?)))
                    .collect::<Result<_, ResolveError>>()?,
                key: key.clone(),
                schedule: schedule.resolve(inputs)?,
            },
            StartOn::Check {
                command,
                project,
                fire_on,
                schedule,
            } => StartOn::Check {
                command: render_start_field(command, inputs, RenderContext::ShellCommand)?,
                project: project
                    .as_ref()
                    .map(|p| {
                        read(p, inputs, "a project id", |v| {
                            v.as_str()?.parse::<ProjectId>().ok()
                        })
                        .map(ValueRef::Fixed)
                    })
                    .transpose()?,
                fire_on: *fire_on,
                schedule: schedule.resolve(inputs)?,
            },
        })
    }
}

/// Render one start field against the listening inputs.
fn render_start_field(
    tmpl: &str,
    inputs: &BTreeMap<String, serde_json::Value>,
    context: RenderContext,
) -> Result<String, ResolveError> {
    let steps = BTreeMap::new();
    let ctx = TemplateCtx {
        inputs,
        steps: &steps,
        event: None,
        goal: None,
        params: template::no_values(),
        account: template::no_values(),
    };
    Ok(template::render_with(
        tmpl,
        &ctx,
        context,
        Grammar::StartEvent,
    )?)
}

/// A reference's value: the fixed one, or the input's, parsed to what the
/// field wants.
fn read<T: Clone>(
    r: &ValueRef<T>,
    inputs: &BTreeMap<String, serde_json::Value>,
    want: &'static str,
    parse: impl Fn(&serde_json::Value) -> Option<T>,
) -> Result<T, ResolveError> {
    match r {
        ValueRef::Fixed(v) => Ok(v.clone()),
        ValueRef::Input { input } => {
            let value = inputs.get(input.as_str());
            value.and_then(&parse).ok_or_else(|| ResolveError::Input {
                input: input.to_string(),
                want,
                got: value.map_or_else(|| "nothing".to_string(), |v| v.to_string()),
            })
        }
    }
}

/// Why a start event, a schedule or a mapping could not be resolved.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ResolveError {
    #[error(transparent)]
    Template(#[from] TemplateError),
    #[error("input `{input}` should hold {want}; it holds {got}")]
    Input {
        input: String,
        want: &'static str,
        got: String,
    },
    /// A schedule that is neither `every` nor `cron`, or both.
    #[error("a schedule is `every` so many seconds or a `cron` expression — one of the two")]
    Schedule,
    /// `every = 0` would spin.
    #[error("a schedule's `every` must be at least one second")]
    ZeroInterval,
}

// ---------------------------------------------------------------------------
// Schedules
// ---------------------------------------------------------------------------

/// A cadence as written: `every` so many seconds, or a `cron` expression in
/// a time zone (`tz`, UTC when absent) — exactly one of the two, which
/// validation holds it to (`ProblemKind::BadTimer`). Flat on the start's
/// table: `on = { event = "schedule", cron = "0 9 * * 1", tz = "Europe/Paris" }`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Schedule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub every: Option<ValueRef<u64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cron: Option<ValueRef<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tz: Option<String>,
}

impl Schedule {
    pub const FIELDS: &'static [&'static str] = &["every", "cron", "tz"];

    pub fn every(secs: u64) -> Self {
        Schedule {
            every: Some(ValueRef::Fixed(secs)),
            ..Schedule::default()
        }
    }

    pub fn cron(expr: impl Into<String>, tz: Option<String>) -> Self {
        Schedule {
            every: None,
            cron: Some(ValueRef::Fixed(expr.into())),
            tz,
        }
    }

    /// Exactly one of `every` and `cron`.
    pub fn is_well_formed(&self) -> bool {
        self.every.is_some() != self.cron.is_some()
    }

    pub fn input_refs(&self) -> Vec<(&InputName, &'static str)> {
        let mut out = Vec::new();
        if let Some(ValueRef::Input { input }) = &self.every {
            out.push((input, "number"));
        }
        if let Some(ValueRef::Input { input }) = &self.cron {
            out.push((input, "text"));
        }
        out
    }

    /// This schedule with its inputs read: both references fixed.
    pub fn resolve(
        &self,
        inputs: &BTreeMap<String, serde_json::Value>,
    ) -> Result<Schedule, ResolveError> {
        Ok(Schedule {
            every: self
                .every
                .as_ref()
                .map(|r| read(r, inputs, "a whole number of seconds", |v| v.as_u64()))
                .transpose()?
                .map(ValueRef::Fixed),
            cron: self
                .cron
                .as_ref()
                .map(|r| {
                    read(r, inputs, "a cron expression", |v| {
                        v.as_str().map(str::to_string)
                    })
                })
                .transpose()?
                .map(ValueRef::Fixed),
            tz: self.tz.clone(),
        })
    }

    /// The cadence of a resolved schedule: what the engine computes due
    /// times from.
    pub fn cadence(&self) -> Result<Cadence, ResolveError> {
        match (&self.every, &self.cron) {
            (Some(ValueRef::Fixed(0)), None) => Err(ResolveError::ZeroInterval),
            (Some(ValueRef::Fixed(secs)), None) => Ok(Cadence::Every { secs: *secs }),
            (None, Some(ValueRef::Fixed(expr))) => Ok(Cadence::Cron {
                expr: expr.clone(),
                tz: self.tz.clone(),
            }),
            (Some(ValueRef::Input { input }), None) | (None, Some(ValueRef::Input { input })) => {
                Err(ResolveError::Input {
                    input: input.to_string(),
                    want: "a resolved value",
                    got: "an unresolved reference".to_string(),
                })
            }
            _ => Err(ResolveError::Schedule),
        }
    }
}

/// A resolved cadence: whole seconds, or a cron expression and its zone.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Cadence {
    Every { secs: u64 },
    Cron { expr: String, tz: Option<String> },
}

// ---------------------------------------------------------------------------
// Check starts
// ---------------------------------------------------------------------------

/// Which results of a check start begin a run.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum FireOn {
    /// The first failing result after a passing one — or after turning on:
    /// the alert, once per outage.
    #[default]
    StartsFailing,
    /// Every failing result.
    Failing,
    /// Every passing result.
    Passing,
    /// Every result.
    Always,
}

impl FireOn {
    pub fn as_str(self) -> &'static str {
        match self {
            FireOn::StartsFailing => "starts_failing",
            FireOn::Failing => "failing",
            FireOn::Passing => "passing",
            FireOn::Always => "always",
        }
    }

    /// Whether this result begins a run. `passed` is exit 0 — a command that
    /// timed out or could not be spawned did not pass — and `before` is the
    /// previous result, `None` when there was none since the start was armed.
    pub fn fires(self, passed: bool, before: Option<bool>) -> bool {
        match self {
            FireOn::Always => true,
            FireOn::Failing => !passed,
            FireOn::Passing => passed,
            FireOn::StartsFailing => !passed && before != Some(false),
        }
    }
}

// ---------------------------------------------------------------------------
// The guard
// ---------------------------------------------------------------------------

/// Rate and overlap controls, applied by the runtime before a run starts, not
/// by the workflow.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct Guard {
    /// Drop an occurrence within this many seconds of the last run this
    /// listener started.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub debounce_secs: u64,
    #[serde(default, skip_serializing_if = "Overlap::is_queue")]
    pub overlap: Overlap,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

/// What an occurrence does while runs of its listener are still live —
/// counted over live runs, never over dispatches.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Overlap {
    /// One run at a time; later occurrences wait in the durable backlog.
    #[default]
    Queue,
    /// Dropped while the listener's last run is unfinished — and it says so.
    Skip,
    /// Up to this many at once; the rest wait. `{ parallel = 4 }`.
    Parallel(NonZeroU32),
}

impl Overlap {
    pub fn as_str(self) -> &'static str {
        match self {
            Overlap::Queue => "queue",
            Overlap::Skip => "skip",
            Overlap::Parallel(_) => "parallel",
        }
    }

    pub fn is_queue(&self) -> bool {
        matches!(self, Overlap::Queue)
    }
}

/// What one occurrence meets at the guard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    /// Start a run now.
    Start,
    /// Keep it in the backlog until a run of the listener ends.
    Hold,
    /// Drop it, saying why.
    Drop(Dropped),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dropped {
    /// Within `debounce_secs` of the last run the listener started.
    Debounced { secs_left: u64 },
    /// `skip`: the listener's last run is unfinished.
    Busy,
}

impl Guard {
    pub fn is_default(&self) -> bool {
        *self == Guard::default()
    }

    /// An arriving occurrence: `live` runs of the listener are unfinished and
    /// the last one it started began `since_last_start` seconds ago.
    pub fn admit(&self, live: usize, since_last_start: Option<u64>) -> Admission {
        if let Some(since) = since_last_start {
            if since < self.debounce_secs {
                return Admission::Drop(Dropped::Debounced {
                    secs_left: self.debounce_secs - since,
                });
            }
        }
        match self.overlap {
            Overlap::Skip if live > 0 => Admission::Drop(Dropped::Busy),
            _ if self.admit_held(live) => Admission::Start,
            _ => Admission::Hold,
        }
    }

    /// A held occurrence, asked again when a run of the listener ended:
    /// whether it starts now. The debounce was judged when it arrived.
    pub fn admit_held(&self, live: usize) -> bool {
        match self.overlap {
            Overlap::Queue | Overlap::Skip => live == 0,
            Overlap::Parallel(max) => live < max.get() as usize,
        }
    }
}

// ---------------------------------------------------------------------------
// The mapping
// ---------------------------------------------------------------------------

/// Why one occurrence could not be mapped onto a run's inputs.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MapError {
    #[error("the start maps `{input}`, which the workflow does not declare")]
    UnknownInput { input: String },
    #[error("input `{input}`: {source}")]
    Template {
        input: String,
        #[source]
        source: TemplateError,
    },
    #[error("input `{input}` wants {want}; the event gave {got}")]
    Kind {
        input: String,
        want: &'static str,
        got: String,
    },
}

/// The inputs one occurrence gives a run: each of the start's `inputs`
/// rendered against the event (the whole signal, as JSON — `{event.payload.…}`)
/// and shaped to its input's kind: a `number` parses as a number, a `bool` as
/// `true` or `false`, every other kind takes the text. A field the event does
/// not carry leaves an optional input to its default and refuses a required
/// one — an event that lacks what the run needs never starts it half-filled.
pub fn map_event(
    mapping: &BTreeMap<String, String>,
    defs: &[InputDef],
    event: &serde_json::Value,
) -> Result<BTreeMap<String, serde_json::Value>, MapError> {
    let no_inputs = BTreeMap::new();
    let no_steps = BTreeMap::new();
    let ctx = TemplateCtx::event_only(&no_inputs, &no_steps, event);
    let mut out = BTreeMap::new();
    for (name, tmpl) in mapping {
        let def = defs
            .iter()
            .find(|d| d.name.as_str() == name)
            .ok_or_else(|| MapError::UnknownInput {
                input: name.clone(),
            })?;
        let text =
            match template::render_with(tmpl, &ctx, RenderContext::Text, Grammar::StartMapping) {
                Ok(text) => text,
                Err(TemplateError::Unresolved { .. }) if !def.required || def.default.is_some() => {
                    continue;
                }
                Err(source) => {
                    return Err(MapError::Template {
                        input: name.clone(),
                        source,
                    })
                }
            };
        let value = match &def.kind {
            InputKind::Number => match serde_json::from_str::<serde_json::Value>(text.trim()) {
                Ok(n @ serde_json::Value::Number(_)) => n,
                _ => {
                    return Err(MapError::Kind {
                        input: name.clone(),
                        want: "a number",
                        got: text,
                    })
                }
            },
            InputKind::Bool => match text.trim() {
                "true" => serde_json::Value::Bool(true),
                "false" => serde_json::Value::Bool(false),
                _ => {
                    return Err(MapError::Kind {
                        input: name.clone(),
                        want: "true or false",
                        got: text,
                    })
                }
            },
            InputKind::Text
            | InputKind::Choice { .. }
            | InputKind::Assignee
            | InputKind::Project
            | InputKind::Account { .. } => serde_json::Value::String(text),
        };
        out.insert(name.clone(), value);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn every_event() -> Vec<StartOn> {
        vec![
            StartOn::Manual,
            StartOn::Schedule {
                schedule: Schedule::cron("0 9 * * 1", Some("Europe/Paris".into())),
            },
            StartOn::Hook { public: true },
            StartOn::Message {
                filter: MessageFilter {
                    r#in: Some("support".into()),
                    from: crate::listen::MessageFrom::Agents,
                    mentions: Some(ValueRef::Fixed(Assignee::Agent("triager".into()))),
                    contains: Some("refund".into()),
                },
            },
            StartOn::Signal {
                filter: SignalFilter {
                    name: "report.ready".into(),
                    fields: BTreeMap::from([("env".into(), "prod".into())]),
                },
            },
            StartOn::Project {
                filter: ProjectFilter {
                    project: Some(ValueRef::Input {
                        input: InputName::new("repo").unwrap(),
                    }),
                    change: crate::listen::ProjectChange::Files,
                    branch: Some("main".into()),
                    glob: Some("*.csv".into()),
                },
            },
            StartOn::Run {
                filter: RunFilter {
                    workflow: None,
                    outcome: Some(crate::listen::RunEnd::Failed),
                },
            },
            StartOn::Platform {
                filter: PlatformFilter {
                    topic: "goal.closed".into(),
                    fields: BTreeMap::from([("reason".into(), "done".into())]),
                },
            },
            StartOn::Connector {
                connector: Some(ConnectorId::new("github").unwrap()),
                operation: Some(OperationId::new("list-issues").unwrap()),
                account: None,
                params: BTreeMap::from([("repo".into(), "{inputs.repo}".into())]),
                key: Some("id".into()),
                schedule: Schedule::every(300),
            },
            StartOn::Check {
                command: "sh -c {inputs.command}".into(),
                project: None,
                fire_on: FireOn::StartsFailing,
                schedule: Schedule {
                    every: Some(ValueRef::Input {
                        input: InputName::new("interval").unwrap(),
                    }),
                    ..Schedule::default()
                },
            },
        ]
    }

    #[test]
    fn every_event_round_trips_and_its_keys_are_the_table() {
        let events = every_event();
        assert_eq!(events.len(), StartOn::NAMES.len());
        for (on, name) in events.iter().zip(StartOn::NAMES) {
            assert_eq!(on.as_str(), name);
            let json = serde_json::to_value(on).unwrap();
            assert_eq!(
                serde_json::from_value::<StartOn>(json.clone()).unwrap(),
                *on
            );
            let own = StartOn::fields_of(name).expect("a tag");
            for key in json.as_object().unwrap().keys() {
                assert!(
                    key == "event" || own.contains(&key.as_str()),
                    "{name} writes `{key}`, which its field table does not list"
                );
            }
            StartOn::refuse_unknown::<serde_json::Error>(&json).unwrap();
        }
        assert!(StartOn::fields_of("webhook").is_none());
    }

    #[test]
    fn a_misspelled_key_on_a_start_event_is_refused() {
        let raw = json!({"event": "schedule", "evry": 60});
        let err = StartOn::refuse_unknown::<serde_json::Error>(&raw).unwrap_err();
        assert!(err.to_string().contains("evry"), "{err}");
        assert!(
            StartOn::refuse_unknown::<serde_json::Error>(&json!({"event": "webhook"})).is_err()
        );
    }

    #[test]
    fn who_may_arm_a_start_and_what_counts_against_the_rate() {
        let unattended: Vec<&str> = every_event()
            .iter()
            .filter(|on| on.arms_unattended())
            .map(|on| on.as_str())
            .collect();
        assert_eq!(
            unattended,
            ["manual", "schedule", "message", "signal", "run", "platform"]
        );
        let limited: Vec<&str> = every_event()
            .iter()
            .filter(|on| on.rate_limited())
            .map(|on| on.as_str())
            .collect();
        assert_eq!(
            limited,
            [
                "message",
                "signal",
                "project",
                "run",
                "platform",
                "connector",
                "check"
            ]
        );
        assert_eq!(StartOn::Manual.source(), None);
        assert_eq!(
            StartOn::Hook { public: false }.source(),
            Some(SignalSource::Hook)
        );
    }

    #[test]
    fn a_start_resolves_against_the_listening_inputs() {
        let inputs = BTreeMap::from([
            (
                "command".to_string(),
                json!("curl -f https://example.invalid/health"),
            ),
            ("interval".to_string(), json!(120)),
            (
                "repo".to_string(),
                json!(ProjectId::from_ulid(ulid::Ulid::from_parts(4, 4)).to_string()),
            ),
        ]);
        let check = every_event().pop().unwrap();
        let StartOn::Check {
            command, schedule, ..
        } = check.resolve(&inputs).unwrap()
        else {
            panic!("a check stays a check")
        };
        assert_eq!(command, "sh -c 'curl -f https://example.invalid/health'");
        assert_eq!(schedule.cadence().unwrap(), Cadence::Every { secs: 120 });

        let project = &every_event()[5];
        let StartOn::Project { filter } = project.resolve(&inputs).unwrap() else {
            panic!("a project start stays one")
        };
        assert!(filter.project_id().is_some());

        let missing = BTreeMap::new();
        let err = check.resolve(&missing).unwrap_err();
        assert!(matches!(err, ResolveError::Template(_)), "{err}");
        let wrong = BTreeMap::from([
            ("command".to_string(), json!("true")),
            ("interval".to_string(), json!("soon")),
        ]);
        assert!(matches!(
            check.resolve(&wrong).unwrap_err(),
            ResolveError::Input { .. }
        ));
    }

    #[test]
    fn a_schedule_is_one_cadence_never_both_or_none() {
        assert!(Schedule::every(60).is_well_formed());
        assert!(Schedule::cron("0 9 * * *", None).is_well_formed());
        assert!(!Schedule::default().is_well_formed());
        let both = Schedule {
            every: Some(ValueRef::Fixed(60)),
            cron: Some(ValueRef::Fixed("0 9 * * *".into())),
            tz: None,
        };
        assert!(!both.is_well_formed());
        assert_eq!(both.cadence(), Err(ResolveError::Schedule));
        assert_eq!(
            Schedule::every(0).cadence(),
            Err(ResolveError::ZeroInterval)
        );
        assert_eq!(
            Schedule::cron("0 9 * * *", Some("UTC".into())).cadence(),
            Ok(Cadence::Cron {
                expr: "0 9 * * *".into(),
                tz: Some("UTC".into())
            })
        );
    }

    #[test]
    fn a_check_start_fires_once_per_outage_by_default() {
        let f = FireOn::StartsFailing;
        assert!(f.fires(false, None), "failing when turned on is an outage");
        assert!(f.fires(false, Some(true)), "pass → fail");
        assert!(!f.fires(false, Some(false)), "still failing: already said");
        assert!(!f.fires(true, Some(false)));
        assert!(FireOn::Failing.fires(false, Some(false)));
        assert!(!FireOn::Failing.fires(true, None));
        assert!(FireOn::Passing.fires(true, Some(true)));
        assert!(FireOn::Always.fires(true, None) && FireOn::Always.fires(false, None));
        assert_eq!(
            serde_json::to_value(FireOn::default()).unwrap(),
            json!("starts_failing")
        );
    }

    #[test]
    fn the_guard_counts_live_runs_and_debounces_arrivals_only() {
        let queue = Guard::default();
        assert!(queue.is_default());
        assert_eq!(queue.admit(0, None), Admission::Start);
        assert_eq!(queue.admit(1, None), Admission::Hold);
        assert!(queue.admit_held(0));
        let skip = Guard {
            overlap: Overlap::Skip,
            ..Guard::default()
        };
        assert_eq!(skip.admit(1, None), Admission::Drop(Dropped::Busy));
        let two = Guard {
            overlap: Overlap::Parallel(NonZeroU32::new(2).unwrap()),
            ..Guard::default()
        };
        assert_eq!(two.admit(1, None), Admission::Start);
        assert_eq!(two.admit(2, None), Admission::Hold);
        let calm = Guard {
            debounce_secs: 60,
            ..Guard::default()
        };
        assert_eq!(
            calm.admit(0, Some(20)),
            Admission::Drop(Dropped::Debounced { secs_left: 40 })
        );
        assert_eq!(calm.admit(0, Some(60)), Admission::Start);
        assert!(
            calm.admit_held(0),
            "a held occurrence is not debounced again"
        );
    }

    #[test]
    fn the_guard_reads_and_writes_its_words() {
        let g: Guard = serde_json::from_value(json!({"overlap": {"parallel": 4}})).unwrap();
        assert_eq!(g.overlap, Overlap::Parallel(NonZeroU32::new(4).unwrap()));
        assert_eq!(
            serde_json::to_value(g).unwrap(),
            json!({"overlap": {"parallel": 4}})
        );
        assert_eq!(serde_json::to_value(Guard::default()).unwrap(), json!({}));
        assert!(serde_json::from_value::<Guard>(json!({"overlap": {"parallel": 0}})).is_err());
        assert!(serde_json::from_value::<Guard>(json!({"max_concurrent": 1})).is_err());
        let skip: Guard = serde_json::from_value(json!({"overlap": "skip"})).unwrap();
        assert_eq!(skip.overlap.as_str(), "skip");
    }

    fn input(name: &str, kind: InputKind, required: bool) -> InputDef {
        InputDef {
            name: InputName::new(name).unwrap(),
            label: name.to_string(),
            kind,
            default: None,
            required,
        }
    }

    #[test]
    fn an_occurrence_maps_onto_typed_inputs() {
        let defs = vec![
            input("ticket", InputKind::Text, true),
            input("count", InputKind::Number, false),
            input("urgent", InputKind::Bool, false),
            input("customer", InputKind::Text, false),
        ];
        let mapping = BTreeMap::from([
            ("ticket".to_string(), "{event.payload.body}".to_string()),
            ("count".to_string(), "{event.payload.n}".to_string()),
            ("urgent".to_string(), "{event.payload.urgent}".to_string()),
            (
                "customer".to_string(),
                "{event.payload.customer}".to_string(),
            ),
        ]);
        let event = json!({"payload": {"body": {"text": "help"}, "n": 3, "urgent": true}});
        let got = map_event(&mapping, &defs, &event).unwrap();
        assert_eq!(got["ticket"], json!("{\"text\":\"help\"}"));
        assert_eq!(got["count"], json!(3));
        assert_eq!(got["urgent"], json!(true));
        assert!(
            !got.contains_key("customer"),
            "an optional input the event lacks keeps its default"
        );

        let lacking = json!({"payload": {"n": 3}});
        assert!(matches!(
            map_event(&mapping, &defs, &lacking),
            Err(MapError::Template { .. })
        ));
        let not_a_number = json!({"payload": {"body": "x", "n": "three"}});
        assert!(matches!(
            map_event(&mapping, &defs, &not_a_number),
            Err(MapError::Kind { .. })
        ));
        let unknown = BTreeMap::from([("nope".to_string(), "{event.at}".to_string())]);
        assert!(matches!(
            map_event(&unknown, &defs, &event),
            Err(MapError::UnknownInput { .. })
        ));
        let reads_inputs = BTreeMap::from([("ticket".to_string(), "{inputs.ticket}".to_string())]);
        assert!(matches!(
            map_event(&reads_inputs, &defs, &event),
            Err(MapError::Template { .. })
        ));
    }
}
