//! The one template language, and its one renderer.
//!
//! A step's instructions, a question's prompt, a notification, a check's
//! command and a start step's input mapping are all written once and
//! rendered on every run, so they share one small grammar: `{inputs.<name>}`,
//! `{steps.<id>.output}` (optionally `.<dotted.path>`), `{steps.<id>.answer}`,
//! `{goal.statement}` and `{goal.title}` — the last two a goal's run's alone:
//! a run of the workspace has no goal to read, so it refuses a definition
//! that reads one before it starts (`Workflow::scope_problems`). The event
//! that began a run is read in one place only: `{event.<dotted.path>}` in its
//! start step's input mapping ([`Grammar::StartMapping`]), which turns it into
//! typed inputs every step then reads; a start event's own fields read the
//! listening inputs alone ([`Grammar::StartEvent`]). That is the whole
//! language. There is no expression evaluator on purpose: a template runs
//! unreviewed on every event, and an evaluator would make it a scripting
//! surface with none of the review a work item gets.
//!
//! Braces in prose. A `{` opens a placeholder unless it is doubled: `{{` is
//! a literal `{` and `}}` a literal `}`, so a JSON example in a step's
//! instructions is written `{{ "plan": "…" }}`. A `}` outside a placeholder
//! is literal whether or not it is doubled. One scanner ([`segments`]) reads a
//! template for both readers below, so what validation accepts is exactly
//! what a run renders.
//!
//! Two moments, two failures. [`placeholders`] parses without a context — an
//! unknown root or a malformed step segment is a *validation* problem the
//! designer shows before anything runs. [`render`] fills a parsed template
//! from a run — a value that is absent at that moment (no event began this
//! run, an upstream step produced no output at that path, an unanswered step)
//! is `Unresolved`, never left verbatim and never blanked: a plausible-looking
//! wrong instruction is worse than a step that fails and says why. And it
//! does say why ([`Absence`]): which step, whether it has not run, is still
//! running, failed, yielded nothing, or yielded other fields — so the step's
//! error, the goal's row and the Workflow Agent's repair name the cause and
//! not just the reference.
//!
//! Two contexts, one rule. Prose renders values as they are
//! ([`RenderContext::Text`]). A `check` command is handed to `sh -c`, and
//! there every **substituted value** is single-quoted
//! ([`RenderContext::ShellCommand`], [`shell_quote`]) so a run input or a
//! hook's payload that reads `a; touch marker` is one argument, never a
//! second command. The literal text of the template is the author's and is
//! not touched: `sh -c {inputs.command}` renders as `sh -c 'npm test'`.

use crate::run::StepRecord;
use crate::workflow::{InputName, StepId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One `{…}` in a template, parsed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Placeholder {
    Input(InputName),
    StepOutput {
        step: StepId,
        path: Option<String>,
    },
    StepAnswer {
        step: StepId,
    },
    /// `{event.<path>}` — the event that began the run, in its start step's
    /// input mapping only.
    Event {
        path: String,
    },
    GoalStatement,
    GoalTitle,
    /// `{params.<name>}` — an operation's parameter, in a connector
    /// definition only.
    Param(String),
    /// `{account.<name>}` — an account-level parameter of a connector, in a
    /// connector definition only.
    Account(String),
}

/// Which roots a template may use. A workflow step's strings read the run
/// ([`Grammar::Run`]); a connector definition's strings read the operation's
/// parameters and the account's ([`Grammar::Connector`]); a start step's
/// input mapping reads the event that began the run ([`Grammar::StartMapping`])
/// and its event's fields read the listening inputs ([`Grammar::StartEvent`]).
/// The sets are disjoint on purpose: a step cannot smuggle a `{params.x}` or
/// an `{event.x}` past validation, and a definition cannot read a run it is
/// not part of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grammar {
    Run,
    Connector,
    StartMapping,
    StartEvent,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TemplateError {
    #[error("unbalanced brace at byte {at}: every `{{` needs its `}}`")]
    Unbalanced { at: usize },
    #[error(
        "unknown placeholder {{{0}}}: the roots are inputs.<name>, steps.<id>.output[.<path>], \
         steps.<id>.answer, goal.statement and goal.title; \
         to write a brace literally, double it ({{{{ and }}}})"
    )]
    Unknown(String),
    #[error(
        "{{{0}}} reads the event that began the run, which only a start step's input mapping \
         reads: map it onto an input there and read {{inputs.<name>}} here"
    )]
    EventOutsideMapping(String),
    #[error(
        "unknown placeholder {{{0}}}: a start step's input mapping reads event.<path> only; \
         to write a brace literally, double it ({{{{ and }}}})"
    )]
    UnknownInMapping(String),
    #[error(
        "unknown placeholder {{{0}}}: a start event's fields read inputs.<name> only — the \
         inputs given when it was turned on; to write a brace literally, double it ({{{{ and }}}})"
    )]
    UnknownInStartEvent(String),
    #[error(
        "unknown placeholder {{{0}}}: a connector definition reads params.<name> and \
         account.<name> only; to write a brace literally, double it ({{{{ and }}}})"
    )]
    UnknownInConnector(String),
    #[error("{{{key}}} has no value in this run: {why}")]
    Unresolved { key: String, why: Absence },
}

/// Why a placeholder has no value in this run — the tail of
/// [`TemplateError::Unresolved`]'s sentence. A step variant names the step;
/// a field variant names the hop that missed and what was there instead.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, tag = "why", rename_all = "snake_case")]
pub enum Absence {
    /// `{inputs.x}`: the run binds no input of that name.
    NoInput,
    /// `{steps.x…}`: the run has no step of that id.
    NoSuchStep { step: String },
    /// The step has not run: pending, skipped or cancelled.
    StepNotRun { step: String, state: String },
    /// The step is still running or waiting.
    StepRunning { step: String, state: String },
    /// The step failed, with its error when it recorded one.
    StepFailed { step: String, error: Option<String> },
    /// The step is done and yielded no output — a kind that never does, or
    /// a result without one.
    NoOutput { step: String },
    /// The output is there but the path misses at `at`; `has` names what the
    /// container at that hop holds instead — its keys, or its length.
    NoField {
        step: String,
        at: String,
        has: Vec<String>,
    },
    /// A `human` step done with no answer on record.
    NotAnswered { step: String },
    /// The step was diverted by one of its boundary events before it
    /// produced anything.
    StepDiverted { step: String, by: String },
    /// `{event…}`: no event began this run — a person started it.
    NoEvent,
    /// The event is there but the path misses at `at`.
    NoEventField { at: String, has: Vec<String> },
    /// `{goal.title}`: the goal has no title.
    NoTitle,
    /// `{goal.…}`: the run is the workspace's, with no goal to read.
    NoGoal,
    /// `{params.x}`: the operation binds no parameter of that name.
    NoParam,
    /// `{account.x}`: the account holds no parameter of that name.
    NoAccount,
}

impl std::fmt::Display for Absence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Absence::NoInput => write!(f, "the run binds no input of that name"),
            Absence::NoSuchStep { step } => write!(f, "the run has no step `{step}`"),
            Absence::StepNotRun { step, state } => {
                write!(f, "step `{step}` has not run yet ({state})")
            }
            Absence::StepRunning { step, state } => {
                write!(f, "step `{step}` is still {state}")
            }
            Absence::StepFailed {
                step,
                error: Some(error),
            } => write!(f, "step `{step}` failed — {error}"),
            Absence::StepFailed { step, error: None } => write!(f, "step `{step}` failed"),
            Absence::NoOutput { step } => {
                write!(f, "step `{step}` is done and yielded no output")
            }
            Absence::NoField { step, at, has } => {
                write!(f, "step `{step}` yielded {} and no `{at}`", holds(has))
            }
            Absence::NotAnswered { step } => write!(f, "step `{step}` has no answer on record"),
            Absence::StepDiverted { step, by } => {
                write!(f, "step `{step}` was diverted by its boundary event `{by}`")
            }
            Absence::NoEvent => write!(f, "no event began this run"),
            Absence::NoEventField { at, has } => {
                write!(f, "the event carries {} and no `{at}`", holds(has))
            }
            Absence::NoTitle => write!(f, "the goal has no title"),
            Absence::NoGoal => write!(f, "this run is the workspace's and has no goal"),
            Absence::NoParam => write!(f, "the operation binds no parameter of that name"),
            Absence::NoAccount => write!(f, "the account holds no parameter of that name"),
        }
    }
}

/// What a container held where a path missed, as words: the keys in
/// backticks, a length, or *nothing* for a value with no fields at all.
fn holds(has: &[String]) -> String {
    if has.is_empty() {
        return "nothing there".to_string();
    }
    has.iter()
        .map(|k| format!("`{k}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// One piece of a template as the scanner reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Segment<'a> {
    /// Literal text — including a `{` or `}` that was written doubled.
    Text(&'a str),
    /// The key between one `{` and its `}`, and the byte offset of that `{`.
    Placeholder { key: &'a str, at: usize },
}

/// The one scanner. `{{` and `}}` are a brace each; `{key}` is a placeholder;
/// a `{` with no `}` after it, or with another `{` before its `}`, is
/// unbalanced; a `}` on its own is text.
fn segments(tmpl: &str) -> impl Iterator<Item = Result<Segment<'_>, TemplateError>> {
    let mut pos = 0usize;
    let mut done = false;
    std::iter::from_fn(move || {
        if done || pos >= tmpl.len() {
            return None;
        }
        let rest = &tmpl[pos..];
        let Some(brace) = rest.find(['{', '}']) else {
            pos = tmpl.len();
            return Some(Ok(Segment::Text(rest)));
        };
        if brace > 0 {
            pos += brace;
            return Some(Ok(Segment::Text(&rest[..brace])));
        }
        let at = pos;
        let item = if rest.starts_with("{{") {
            pos += 2;
            Ok(Segment::Text("{"))
        } else if rest.starts_with("}}") {
            pos += 2;
            Ok(Segment::Text("}"))
        } else if rest.starts_with('}') {
            pos += 1;
            Ok(Segment::Text("}"))
        } else {
            let after = &rest[1..];
            match after.find('}') {
                Some(close) if !after[..close].contains('{') => {
                    pos += 1 + close + 1;
                    Ok(Segment::Placeholder {
                        key: &after[..close],
                        at,
                    })
                }
                _ => {
                    done = true;
                    Err(TemplateError::Unbalanced { at })
                }
            }
        };
        Some(item)
    })
}

/// Parse every placeholder, in order. Text between them is not returned —
/// validation needs the references, not the prose.
pub fn placeholders(tmpl: &str) -> Result<Vec<Placeholder>, TemplateError> {
    placeholders_in(tmpl, Grammar::Run)
}

/// [`placeholders`] under a chosen grammar.
pub fn placeholders_in(tmpl: &str, grammar: Grammar) -> Result<Vec<Placeholder>, TemplateError> {
    segments(tmpl)
        .filter_map(|segment| match segment {
            Ok(Segment::Placeholder { key, .. }) => Some(parse_key(key, grammar)),
            Ok(Segment::Text(_)) => None,
            Err(e) => Some(Err(e)),
        })
        .collect()
}

fn parse_key(key: &str, grammar: Grammar) -> Result<Placeholder, TemplateError> {
    match grammar {
        Grammar::Run => parse_run_key(key),
        Grammar::Connector => parse_connector_key(key),
        Grammar::StartMapping => parse_mapping_key(key),
        Grammar::StartEvent => parse_start_event_key(key),
    }
}

/// A start step's input mapping reads the event and nothing else: the run
/// has not begun, so there are no inputs, steps or goal to read yet.
fn parse_mapping_key(key: &str) -> Result<Placeholder, TemplateError> {
    match key.split_once('.') {
        Some(("event", path)) if !path.is_empty() && !path.ends_with('.') => {
            Ok(Placeholder::Event {
                path: path.to_string(),
            })
        }
        _ => Err(TemplateError::UnknownInMapping(key.to_string())),
    }
}

/// A start event's fields read the inputs the host listens with — resolved
/// when the event is armed, before any run exists.
fn parse_start_event_key(key: &str) -> Result<Placeholder, TemplateError> {
    match key.split_once('.') {
        Some(("inputs", name)) => InputName::new(name)
            .map(Placeholder::Input)
            .map_err(|_| TemplateError::UnknownInStartEvent(key.to_string())),
        _ => Err(TemplateError::UnknownInStartEvent(key.to_string())),
    }
}

/// A connector definition's two roots. A name is an input-name word, so a
/// definition and the step that calls it spell a parameter the same way.
fn parse_connector_key(key: &str) -> Result<Placeholder, TemplateError> {
    let unknown = || TemplateError::UnknownInConnector(key.to_string());
    match key.split_once('.') {
        Some(("params", name)) => InputName::new(name)
            .map(|n| Placeholder::Param(n.to_string()))
            .map_err(|_| unknown()),
        Some(("account", name)) => InputName::new(name)
            .map(|n| Placeholder::Account(n.to_string()))
            .map_err(|_| unknown()),
        _ => Err(unknown()),
    }
}

fn parse_run_key(key: &str) -> Result<Placeholder, TemplateError> {
    let unknown = || TemplateError::Unknown(key.to_string());
    let (root, tail) = match key.split_once('.') {
        Some((root, tail)) => (root, Some(tail)),
        None => (key, None),
    };
    match (root, tail) {
        ("inputs", Some(name)) => InputName::new(name)
            .map(Placeholder::Input)
            .map_err(|_| unknown()),
        ("steps", Some(tail)) => {
            let (step, what) = tail.split_once('.').ok_or_else(unknown)?;
            let step = StepId::new(step).map_err(|_| unknown())?;
            match what.split_once('.') {
                None if what == "output" => Ok(Placeholder::StepOutput { step, path: None }),
                None if what == "answer" => Ok(Placeholder::StepAnswer { step }),
                Some(("output", path)) if !path.is_empty() && !path.ends_with('.') => {
                    Ok(Placeholder::StepOutput {
                        step,
                        path: Some(path.to_string()),
                    })
                }
                _ => Err(unknown()),
            }
        }
        // The event is its start's mapping's to read, and nobody else's.
        ("event", _) => Err(TemplateError::EventOutsideMapping(key.to_string())),
        ("goal", Some("statement")) => Ok(Placeholder::GoalStatement),
        ("goal", Some("title")) => Ok(Placeholder::GoalTitle),
        _ => Err(unknown()),
    }
}

/// What `{goal.statement}` and `{goal.title}` read: the goal a run is for.
#[derive(Clone, Copy, Debug)]
pub struct GoalText<'a> {
    pub statement: &'a str,
    pub title: Option<&'a str>,
}

/// Everything a template may read. Assembled by the caller so rendering stays
/// pure and testable with zero I/O.
#[derive(Clone, Copy, Debug)]
pub struct TemplateCtx<'a> {
    pub inputs: &'a BTreeMap<String, serde_json::Value>,
    pub steps: &'a BTreeMap<StepId, StepRecord>,
    /// The event that began the run — its whole `Signal` as JSON — which only
    /// a start step's input mapping reads ([`Grammar::StartMapping`]).
    pub event: Option<&'a serde_json::Value>,
    /// The goal a goal's run is for; `None` in a run of the workspace, and
    /// inside the run machine, which reads no goal.
    pub goal: Option<GoalText<'a>>,
    /// An operation's bound parameters — what a connector definition's
    /// `{params.<name>}` reads. Empty for a run's own strings.
    pub params: &'a BTreeMap<String, serde_json::Value>,
    /// A connector account's parameters — what `{account.<name>}` reads.
    /// Empty for a run's own strings.
    pub account: &'a BTreeMap<String, serde_json::Value>,
}

/// The map a context reads when it has nothing of that kind — a run has no
/// connector parameters, a connector definition no run.
pub fn no_values() -> &'static BTreeMap<String, serde_json::Value> {
    static EMPTY: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    &EMPTY
}

impl<'a> TemplateCtx<'a> {
    /// A context with nothing in it but an event — what a start step's input
    /// mapping renders against, before the run exists.
    pub fn event_only(
        empty_inputs: &'a BTreeMap<String, serde_json::Value>,
        empty_steps: &'a BTreeMap<StepId, StepRecord>,
        event: &'a serde_json::Value,
    ) -> Self {
        Self {
            inputs: empty_inputs,
            steps: empty_steps,
            event: Some(event),
            goal: None,
            params: no_values(),
            account: no_values(),
        }
    }
}

/// Where a rendered template goes, which decides how a substituted value is
/// written into it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderContext {
    /// Prose: instructions, prompts, notifications, wait fields. Values are
    /// substituted as they are.
    Text,
    /// A line handed to `sh -c`. Every substituted value is single-quoted so
    /// it is one word to the shell whatever characters it holds; the
    /// template's own text is not quoted.
    ShellCommand,
}

impl RenderContext {
    fn write(self, value: &str) -> String {
        match self {
            RenderContext::Text => value.to_string(),
            RenderContext::ShellCommand => shell_quote(value),
        }
    }
}

/// `raw` as one POSIX shell word: wrapped in single quotes, with every
/// single quote inside written as `'\''` (close, escaped quote, reopen).
/// Nothing else is special inside single quotes, so this is complete.
pub fn shell_quote(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 2);
    out.push('\'');
    for ch in raw.chars() {
        if ch == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

/// Render a template against a run as prose ([`RenderContext::Text`]). A
/// string value renders bare; any other JSON value renders compact; an answer
/// renders as its selected option ids joined by `, `, then its text.
pub fn render(tmpl: &str, ctx: &TemplateCtx<'_>) -> Result<String, TemplateError> {
    render_in(tmpl, ctx, RenderContext::Text)
}

/// Render a template against a run for one destination. See
/// [`RenderContext`] for what changes between the two.
pub fn render_in(
    tmpl: &str,
    ctx: &TemplateCtx<'_>,
    context: RenderContext,
) -> Result<String, TemplateError> {
    render_with(tmpl, ctx, context, Grammar::Run)
}

/// [`render_in`] under a chosen grammar — a connector definition's strings
/// render against the operation's parameters and the account's.
pub fn render_with(
    tmpl: &str,
    ctx: &TemplateCtx<'_>,
    context: RenderContext,
    grammar: Grammar,
) -> Result<String, TemplateError> {
    let mut out = String::with_capacity(tmpl.len());
    for segment in segments(tmpl) {
        match segment? {
            // The author's text, written as it is: a literal brace included,
            // and never quoted, whatever the destination.
            Segment::Text(text) => out.push_str(text),
            Segment::Placeholder { key, .. } => {
                let placeholder = parse_key(key, grammar)?;
                out.push_str(&context.write(&resolve(key, &placeholder, ctx)?));
            }
        }
    }
    Ok(out)
}

fn resolve(
    key: &str,
    placeholder: &Placeholder,
    ctx: &TemplateCtx<'_>,
) -> Result<String, TemplateError> {
    let unresolved = |why: Absence| TemplateError::Unresolved {
        key: key.to_string(),
        why,
    };
    match placeholder {
        Placeholder::Input(name) => ctx
            .inputs
            .get(name.as_str())
            .map(render_json)
            .ok_or_else(|| unresolved(Absence::NoInput)),
        Placeholder::StepOutput { step, path } => {
            let record = ctx.steps.get(step).ok_or_else(|| {
                unresolved(Absence::NoSuchStep {
                    step: step.to_string(),
                })
            })?;
            let output = record.output.as_ref().ok_or_else(|| {
                unresolved(step_absence(
                    step,
                    record,
                    Absence::NoOutput {
                        step: step.to_string(),
                    },
                ))
            })?;
            match path {
                None => Ok(render_json(output)),
                Some(p) => json_path(output, p).map(render_json).ok_or_else(|| {
                    let (at, has) = missing_hop(output, p);
                    unresolved(Absence::NoField {
                        step: step.to_string(),
                        at,
                        has,
                    })
                }),
            }
        }
        Placeholder::StepAnswer { step } => {
            let record = ctx.steps.get(step).ok_or_else(|| {
                unresolved(Absence::NoSuchStep {
                    step: step.to_string(),
                })
            })?;
            let answer = record.answer.as_ref().ok_or_else(|| {
                unresolved(step_absence(
                    step,
                    record,
                    Absence::NotAnswered {
                        step: step.to_string(),
                    },
                ))
            })?;
            let mut parts: Vec<String> = Vec::new();
            if !answer.selected.is_empty() {
                parts.push(answer.selected.join(", "));
            }
            if let Some(text) = answer.text.as_deref().map(str::trim) {
                if !text.is_empty() {
                    parts.push(text.to_string());
                }
            }
            if answer.unsure {
                parts.push("not sure".to_string());
            }
            Ok(parts.join(" — "))
        }
        Placeholder::Event { path } => {
            let event = ctx.event.ok_or_else(|| unresolved(Absence::NoEvent))?;
            json_path(event, path).map(render_json).ok_or_else(|| {
                let (at, has) = missing_hop(event, path);
                unresolved(Absence::NoEventField { at, has })
            })
        }
        Placeholder::GoalStatement => ctx
            .goal
            .map(|g| g.statement.to_string())
            .ok_or_else(|| unresolved(Absence::NoGoal)),
        Placeholder::GoalTitle => {
            let goal = ctx.goal.ok_or_else(|| unresolved(Absence::NoGoal))?;
            goal.title
                .map(str::to_string)
                .ok_or_else(|| unresolved(Absence::NoTitle))
        }
        Placeholder::Param(name) => ctx
            .params
            .get(name)
            .map(render_json)
            .ok_or_else(|| unresolved(Absence::NoParam)),
        Placeholder::Account(name) => ctx
            .account
            .get(name)
            .map(render_json)
            .ok_or_else(|| unresolved(Absence::NoAccount)),
    }
}

/// Why a step's output or answer is absent, from the record: not run yet,
/// still going, failed — or `done`, the caller's word for a finished step
/// that holds nothing of the kind asked.
fn step_absence(step: &StepId, record: &StepRecord, done: Absence) -> Absence {
    use crate::run::StepState;
    let step = step.to_string();
    match &record.state {
        StepState::Pending | StepState::Skipped | StepState::Cancelled => Absence::StepNotRun {
            step,
            state: record.state.as_str().to_string(),
        },
        StepState::Running | StepState::Waiting => Absence::StepRunning {
            step,
            state: record.state.as_str().to_string(),
        },
        StepState::Failed => Absence::StepFailed {
            step,
            error: record.error.clone(),
        },
        StepState::Diverted { by } => Absence::StepDiverted {
            step,
            by: by.to_string(),
        },
        StepState::Done { .. } => done,
    }
}

/// Where a dotted path first misses in `value`, and what the container at
/// that hop holds instead: `(the path up to and including the missing part,
/// the keys there — or its length for an array, nothing for a scalar)`.
/// Called only after [`json_path`] answered `None`, so a part misses.
fn missing_hop(value: &serde_json::Value, path: &str) -> (String, Vec<String>) {
    let mut cur = value;
    let mut walked: Vec<&str> = Vec::new();
    for part in path.split('.') {
        walked.push(part);
        let next = match cur {
            serde_json::Value::Object(map) if !part.is_empty() => map.get(part),
            serde_json::Value::Array(items) => {
                part.parse::<usize>().ok().and_then(|i| items.get(i))
            }
            _ => None,
        };
        match next {
            Some(v) => cur = v,
            None => {
                let has = match cur {
                    serde_json::Value::Object(map) => map.keys().cloned().collect(),
                    serde_json::Value::Array(items) => vec![format!("{} items", items.len())],
                    _ => vec![],
                };
                return (walked.join("."), has);
            }
        }
    }
    (path.to_string(), vec![])
}

/// Look up a dotted path in a JSON value. Numeric components index arrays,
/// so `items.0.name` works; anything missing yields `None` rather than null,
/// which is what lets the renderer tell "absent" from "present and null".
pub fn json_path<'a>(value: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut cur = value;
    for part in path.split('.') {
        if part.is_empty() {
            return None;
        }
        cur = match cur {
            serde_json::Value::Object(map) => map.get(part)?,
            serde_json::Value::Array(items) => items.get(part.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(cur)
}

/// A string renders as itself (no quotes); anything else as compact JSON.
fn render_json(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ask::Answer;
    use crate::run::StepState;
    use serde_json::json;

    fn step(id: &str) -> StepId {
        StepId::new(id).unwrap()
    }

    fn steps() -> BTreeMap<StepId, StepRecord> {
        let mut m = BTreeMap::new();
        m.insert(
            step("build"),
            StepRecord {
                state: StepState::Done { branches: vec![] },
                output: Some(json!({"summary": "ok", "files": ["a.rs", "b.rs"], "n": 3})),
                ..StepRecord::default()
            },
        );
        m.insert(
            step("review"),
            StepRecord {
                state: StepState::Done { branches: vec![] },
                answer: Some(Answer::selecting(["ship"]).with_text("looks good")),
                ..StepRecord::default()
            },
        );
        m.insert(step("later"), StepRecord::default());
        m
    }

    fn inputs() -> BTreeMap<String, serde_json::Value> {
        BTreeMap::from([
            ("feature".to_string(), json!("dark mode")),
            ("count".to_string(), json!(2)),
        ])
    }

    #[test]
    fn renders_every_root() {
        let inputs = inputs();
        let steps = steps();
        let ctx = TemplateCtx {
            inputs: &inputs,
            steps: &steps,
            event: None,
            goal: Some(GoalText {
                statement: "Ship dark mode",
                title: Some("Dark mode"),
            }),
            params: no_values(),
            account: no_values(),
        };
        let out = render(
            "{inputs.feature} x{inputs.count}: {steps.build.output.summary} / {steps.review.answer} \
             for {goal.title}: {goal.statement}",
            &ctx,
        )
        .unwrap();
        assert_eq!(
            out,
            "dark mode x2: ok / ship — looks good for Dark mode: Ship dark mode"
        );
    }

    /// The event is read by a start step's input mapping and nowhere else:
    /// the mapping reads `{event.…}` alone, a step never does, and a start
    /// event's own fields read the listening inputs alone.
    #[test]
    fn the_event_is_read_by_its_start_mapping_alone() {
        let empty_inputs = BTreeMap::new();
        let empty_steps = BTreeMap::new();
        let event = json!({"id": "01SIG", "at": 7, "payload": {"pr": {"number": 12}}});
        let ctx = TemplateCtx::event_only(&empty_inputs, &empty_steps, &event);
        assert_eq!(
            render_with(
                "pr {event.payload.pr.number} ({event.id} at {event.at})",
                &ctx,
                RenderContext::Text,
                Grammar::StartMapping
            )
            .unwrap(),
            "pr 12 (01SIG at 7)"
        );
        assert!(matches!(
            placeholders_in("{inputs.x}", Grammar::StartMapping),
            Err(TemplateError::UnknownInMapping(k)) if k == "inputs.x"
        ));
        assert!(matches!(
            placeholders("{event.payload.x}"),
            Err(TemplateError::EventOutsideMapping(k)) if k == "event.payload.x"
        ));
        assert_eq!(
            placeholders_in("every {inputs.when}", Grammar::StartEvent).unwrap(),
            vec![Placeholder::Input(InputName::new("when").unwrap())]
        );
        for bad in ["{event.payload.x}", "{steps.a.output}", "{goal.title}"] {
            assert!(
                matches!(
                    placeholders_in(bad, Grammar::StartEvent),
                    Err(TemplateError::UnknownInStartEvent(_))
                ),
                "{bad}"
            );
        }
        let said = TemplateError::EventOutsideMapping("event.x".into()).to_string();
        assert!(said.contains("input mapping"), "{said}");
    }

    #[test]
    fn nested_paths_and_array_indices() {
        let inputs = inputs();
        let steps = steps();
        let ctx = TemplateCtx {
            inputs: &inputs,
            steps: &steps,
            event: None,
            goal: Some(GoalText {
                statement: "",
                title: None,
            }),
            params: no_values(),
            account: no_values(),
        };
        assert_eq!(
            render("{steps.build.output.files.1}", &ctx).unwrap(),
            "b.rs"
        );
        // A structure renders as compact JSON in the order its producer wrote
        // it (the workspace's `serde_json` keeps insertion order), so what an
        // agent reads is what the upstream step said, not an alphabetised copy.
        assert_eq!(
            render("{steps.build.output}", &ctx).unwrap(),
            r#"{"summary":"ok","files":["a.rs","b.rs"],"n":3}"#
        );
        assert_eq!(render("{steps.build.output.n}", &ctx).unwrap(), "3");
        assert!(
            matches!(
                render("{steps.build.output.files.9}", &ctx),
                Err(TemplateError::Unresolved { .. })
            ),
            "an index past the end is unresolved, never empty text"
        );
    }

    #[test]
    fn strings_lose_their_quotes_but_structures_do_not() {
        let inputs = BTreeMap::from([
            ("s".to_string(), json!("hi")),
            ("n".to_string(), json!(3)),
            ("b".to_string(), json!(true)),
            ("o".to_string(), json!({"k": 1})),
            ("z".to_string(), json!(null)),
        ]);
        let steps = BTreeMap::new();
        let ctx = TemplateCtx {
            inputs: &inputs,
            steps: &steps,
            event: None,
            goal: Some(GoalText {
                statement: "",
                title: None,
            }),
            params: no_values(),
            account: no_values(),
        };
        assert_eq!(render("{inputs.s}", &ctx).unwrap(), "hi");
        assert_eq!(render("{inputs.n}", &ctx).unwrap(), "3");
        assert_eq!(render("{inputs.b}", &ctx).unwrap(), "true");
        assert_eq!(render("{inputs.o}", &ctx).unwrap(), r#"{"k":1}"#);
        // Present-and-null renders; absent is unresolved. That distinction is
        // the reason `json_path` returns Option rather than Value::Null.
        assert_eq!(render("{inputs.z}", &ctx).unwrap(), "null");
        assert!(matches!(
            render("{inputs.nope}", &ctx),
            Err(TemplateError::Unresolved { key, why: Absence::NoInput }) if key == "inputs.nope"
        ));
    }

    #[test]
    fn unknown_roots_are_a_parse_error_not_verbatim() {
        for bad in [
            "{mystery}",
            "{payload.a}",
            "{name}",
            "{output}",
            "{steps.build}",
            "{steps.build.nope}",
            "{steps.Build.output}",
            "{steps.build.output.}",
            "{inputs.}",
            "{inputs.Bad Name}",
            "{signal}",
            "{signal.x}",
            "{goal.nope}",
            "{}",
        ] {
            assert!(
                matches!(placeholders(bad), Err(TemplateError::Unknown(_))),
                "{bad:?} should be an unknown placeholder"
            );
        }
        assert_eq!(
            placeholders("a {inputs.x} b {steps.s.output.p.q} {steps.s.answer} {goal.statement} {goal.title}")
                .unwrap(),
            vec![
                Placeholder::Input(InputName::new("x").unwrap()),
                Placeholder::StepOutput {
                    step: step("s"),
                    path: Some("p.q".into())
                },
                Placeholder::StepAnswer { step: step("s") },
                Placeholder::GoalStatement,
                Placeholder::GoalTitle,
            ]
        );
        assert!(placeholders("no placeholders").unwrap().is_empty());
    }

    #[test]
    fn absent_values_are_unresolved() {
        let inputs = inputs();
        let steps = steps();
        let ctx = TemplateCtx {
            inputs: &inputs,
            steps: &steps,
            event: None,
            goal: Some(GoalText {
                statement: "s",
                title: None,
            }),
            params: no_values(),
            account: no_values(),
        };
        for (tmpl, key) in [
            ("{steps.later.output}", "steps.later.output"),
            ("{steps.build.answer}", "steps.build.answer"),
            ("{steps.build.output.missing}", "steps.build.output.missing"),
            ("{goal.title}", "goal.title"),
            ("{inputs.absent}", "inputs.absent"),
        ] {
            assert!(
                matches!(render(tmpl, &ctx), Err(TemplateError::Unresolved { key: k, .. }) if k == key),
                "{tmpl}"
            );
        }
    }

    /// The sentence after the key says why: which step, and whether it has
    /// not run, is still going, failed, yielded nothing, or yielded other
    /// fields — the difference between a workflow to redraw and a session
    /// that missed a field.
    #[test]
    fn an_unresolved_placeholder_says_why() {
        let inputs = inputs();
        let mut steps = steps();
        steps.insert(
            step("running"),
            StepRecord {
                state: StepState::Running,
                ..StepRecord::default()
            },
        );
        steps.insert(
            step("broken"),
            StepRecord {
                state: StepState::Failed,
                error: Some("the session ended without yielding a result".into()),
                ..StepRecord::default()
            },
        );
        steps.insert(
            step("quiet"),
            StepRecord {
                state: StepState::Done { branches: vec![] },
                ..StepRecord::default()
            },
        );
        steps.insert(
            step("waited"),
            StepRecord {
                state: StepState::Diverted {
                    by: crate::workflow::Branch::new("late").unwrap(),
                },
                ..StepRecord::default()
            },
        );
        let event = json!({"payload": {"pr": {"number": 12}}});
        let ctx = TemplateCtx {
            inputs: &inputs,
            steps: &steps,
            event: Some(&event),
            goal: Some(GoalText {
                statement: "s",
                title: None,
            }),
            params: no_values(),
            account: no_values(),
        };
        fn why(tmpl: &str, ctx: &TemplateCtx<'_>) -> Absence {
            match render(tmpl, ctx) {
                Err(TemplateError::Unresolved { why, .. }) => why,
                other => panic!("{tmpl}: {other:?}"),
            }
        }
        assert_eq!(why("{inputs.absent}", &ctx), Absence::NoInput);
        assert_eq!(
            why("{steps.nowhere.output}", &ctx),
            Absence::NoSuchStep {
                step: "nowhere".into()
            }
        );
        assert_eq!(
            why("{steps.later.output}", &ctx),
            Absence::StepNotRun {
                step: "later".into(),
                state: "pending".into()
            }
        );
        assert_eq!(
            why("{steps.running.output.x}", &ctx),
            Absence::StepRunning {
                step: "running".into(),
                state: "running".into()
            }
        );
        assert_eq!(
            why("{steps.broken.output}", &ctx),
            Absence::StepFailed {
                step: "broken".into(),
                error: Some("the session ended without yielding a result".into())
            }
        );
        assert_eq!(
            why("{steps.quiet.output}", &ctx),
            Absence::NoOutput {
                step: "quiet".into()
            }
        );
        assert_eq!(
            why("{steps.build.output.findings}", &ctx),
            Absence::NoField {
                step: "build".into(),
                at: "findings".into(),
                has: vec!["summary".into(), "files".into(), "n".into()]
            }
        );
        assert_eq!(
            why("{steps.build.output.files.9.name}", &ctx),
            Absence::NoField {
                step: "build".into(),
                at: "files.9".into(),
                has: vec!["2 items".into()]
            },
            "the hop that missed, not the whole path"
        );
        assert_eq!(
            why("{steps.build.output.summary.word}", &ctx),
            Absence::NoField {
                step: "build".into(),
                at: "summary.word".into(),
                has: vec![]
            },
            "a scalar has no fields at all"
        );
        assert_eq!(
            why("{steps.build.answer}", &ctx),
            Absence::NotAnswered {
                step: "build".into()
            },
            "done, and no answer on record"
        );
        assert_eq!(
            why("{steps.later.answer}", &ctx),
            Absence::StepNotRun {
                step: "later".into(),
                state: "pending".into()
            }
        );
        assert_eq!(
            why("{steps.waited.output}", &ctx),
            Absence::StepDiverted {
                step: "waited".into(),
                by: "late".into()
            },
            "a diverted step says which boundary took it"
        );
        fn mapped(tmpl: &str, ctx: &TemplateCtx<'_>) -> Absence {
            match render_with(tmpl, ctx, RenderContext::Text, Grammar::StartMapping) {
                Err(TemplateError::Unresolved { why, .. }) => why,
                other => panic!("{tmpl}: {other:?}"),
            }
        }
        assert_eq!(
            mapped("{event.payload.issue}", &ctx),
            Absence::NoEventField {
                at: "payload.issue".into(),
                has: vec!["pr".into()]
            }
        );
        assert_eq!(why("{goal.title}", &ctx), Absence::NoTitle);
        // The words a person reads.
        let said = render("{steps.build.output.findings}", &ctx)
            .unwrap_err()
            .to_string();
        assert_eq!(
            said,
            "{steps.build.output.findings} has no value in this run: step `build` yielded `summary`, `files`, `n` and no `findings`"
        );
        assert_eq!(
            render("{steps.broken.output}", &ctx).unwrap_err().to_string(),
            "{steps.broken.output} has no value in this run: step `broken` failed — the session ended without yielding a result"
        );
        assert_eq!(
            render("{steps.later.output}", &ctx)
                .unwrap_err()
                .to_string(),
            "{steps.later.output} has no value in this run: step `later` has not run yet (pending)"
        );
        let empty_event = json!({});
        let ctx = TemplateCtx {
            event: Some(&empty_event),
            ..ctx
        };
        assert_eq!(
            mapped("{event.x}", &ctx),
            Absence::NoEventField {
                at: "x".into(),
                has: vec![]
            }
        );
        let ctx = TemplateCtx { event: None, ..ctx };
        assert_eq!(mapped("{event.x}", &ctx), Absence::NoEvent);
        // A run of the workspace has no goal: both goal placeholders say so,
        // before the title's own absence could.
        let ctx = TemplateCtx { goal: None, ..ctx };
        assert_eq!(why("{goal.statement}", &ctx), Absence::NoGoal);
        assert_eq!(why("{goal.title}", &ctx), Absence::NoGoal);
        assert_eq!(
            render("Goal: {goal.statement}", &ctx)
                .unwrap_err()
                .to_string(),
            "{goal.statement} has no value in this run: this run is the workspace's and has no goal"
        );
    }

    #[test]
    fn malformed_braces_never_panic() {
        let inputs = BTreeMap::new();
        let steps = BTreeMap::new();
        let ctx = TemplateCtx {
            inputs: &inputs,
            steps: &steps,
            event: None,
            goal: Some(GoalText {
                statement: "",
                title: None,
            }),
            params: no_values(),
            account: no_values(),
        };
        assert_eq!(
            render("{unclosed", &ctx),
            Err(TemplateError::Unbalanced { at: 0 })
        );
        // A brace inside a key is still unbalanced: `{ {inputs.x} }` is not
        // a placeholder wrapped in braces — that is written `{{{inputs.x}}}`.
        assert_eq!(
            render("{ {inputs.x} }", &ctx),
            Err(TemplateError::Unbalanced { at: 0 })
        );
        assert_eq!(render("}{", &ctx), Err(TemplateError::Unbalanced { at: 1 }));
        assert_eq!(render("{{unclosed", &ctx).unwrap(), "{unclosed");
        assert_eq!(render("", &ctx).unwrap(), "");
        assert_eq!(render("a } b", &ctx).unwrap(), "a } b");
        assert_eq!(render("no placeholders", &ctx).unwrap(), "no placeholders");
    }

    /// `{{` and `}}` are a brace each, in both readers and both contexts, so
    /// a JSON example sits in a step's prose without becoming a placeholder.
    #[test]
    fn doubled_braces_are_literal() {
        let inputs = inputs();
        let steps = steps();
        let ctx = TemplateCtx {
            inputs: &inputs,
            steps: &steps,
            event: None,
            goal: Some(GoalText {
                statement: "Ship",
                title: None,
            }),
            params: no_values(),
            account: no_values(),
        };
        assert_eq!(
            render(r#"Call yield_result with {{ "plan": "<markdown>" }}"#, &ctx).unwrap(),
            r#"Call yield_result with { "plan": "<markdown>" }"#
        );
        assert_eq!(
            render(
                r#"{{ "options": [{{ "id": "a", "n": {inputs.count} }}], "goal": "{goal.statement}" }}"#,
                &ctx
            )
            .unwrap(),
            r#"{ "options": [{ "id": "a", "n": 2 }], "goal": "Ship" }"#,
            "nested braces double too, and placeholders inside still render"
        );
        assert_eq!(render("{{{inputs.feature}}}", &ctx).unwrap(), "{dark mode}");
        assert_eq!(render("{{{{", &ctx).unwrap(), "{{");
        assert_eq!(render("a }} b", &ctx).unwrap(), "a } b");
        assert_eq!(
            placeholders(r#"{{ "plan": "…" }}"#).unwrap(),
            vec![],
            "a doubled brace is not a placeholder"
        );
        assert_eq!(
            placeholders("{{{inputs.x}}}").unwrap(),
            vec![Placeholder::Input(InputName::new("x").unwrap())]
        );
        // Literal text is the author's and is never quoted, even for a shell.
        let shell_inputs = BTreeMap::from([("x".to_string(), json!("a b"))]);
        let empty = BTreeMap::new();
        let shell = TemplateCtx {
            inputs: &shell_inputs,
            steps: &empty,
            event: None,
            goal: Some(GoalText {
                statement: "",
                title: None,
            }),
            params: no_values(),
            account: no_values(),
        };
        assert_eq!(
            render_in("echo {{x}} {inputs.x}", &shell, RenderContext::ShellCommand).unwrap(),
            "echo {x} 'a b'"
        );
    }

    /// A connector definition reads `params.*` and `account.*`; a run's
    /// strings do not, and a definition reads nothing of a run. The two
    /// grammars are disjoint by construction.
    #[test]
    fn the_connector_grammar_has_its_own_two_roots() {
        assert_eq!(
            placeholders_in("{params.q} and {account.site}", Grammar::Connector).unwrap(),
            vec![
                Placeholder::Param("q".into()),
                Placeholder::Account("site".into())
            ]
        );
        assert!(matches!(
            placeholders("{params.q}"),
            Err(TemplateError::Unknown(k)) if k == "params.q"
        ));
        assert!(matches!(
            placeholders_in("{inputs.q}", Grammar::Connector),
            Err(TemplateError::UnknownInConnector(k)) if k == "inputs.q"
        ));
        assert!(matches!(
            placeholders_in("{params.Not A Name}", Grammar::Connector),
            Err(TemplateError::UnknownInConnector(_))
        ));
        assert_eq!(
            placeholders_in("{{ \"q\": 1 }}", Grammar::Connector).unwrap(),
            vec![]
        );

        let params = BTreeMap::from([("q".to_string(), json!("a b")), ("n".to_string(), json!(3))]);
        let account = BTreeMap::from([("site".to_string(), json!("acme"))]);
        let empty = BTreeMap::new();
        let ctx = TemplateCtx {
            inputs: &empty,
            steps: &BTreeMap::new(),
            event: None,
            goal: Some(GoalText {
                statement: "",
                title: None,
            }),
            params: &params,
            account: &account,
        };
        assert_eq!(
            render_with(
                "https://{account.site}.example.com/search?q={params.q}&n={params.n}",
                &ctx,
                RenderContext::Text,
                Grammar::Connector
            )
            .unwrap(),
            "https://acme.example.com/search?q=a b&n=3"
        );
        assert!(matches!(
            render_with("{params.zzz}", &ctx, RenderContext::Text, Grammar::Connector),
            Err(TemplateError::Unresolved { key, why: Absence::NoParam }) if key == "params.zzz"
        ));
        assert!(matches!(
            render("{params.q}", &ctx),
            Err(TemplateError::Unknown(_))
        ));
    }

    #[test]
    fn unknown_placeholder_says_how_to_write_a_brace() {
        let said = TemplateError::Unknown("plan".into()).to_string();
        assert!(said.contains("{plan}"), "{said}");
        assert!(
            said.contains("to write a brace literally, double it"),
            "{said}"
        );
    }

    /// One scanner, one verdict: what validation refuses, a run refuses, and
    /// what validation accepts, a run renders.
    #[test]
    fn both_readers_scan_alike() {
        let inputs = inputs();
        let steps = steps();
        let ctx = TemplateCtx {
            inputs: &inputs,
            steps: &steps,
            event: None,
            goal: Some(GoalText {
                statement: "",
                title: None,
            }),
            params: no_values(),
            account: no_values(),
        };
        for tmpl in [
            "{unclosed",
            "{{ok}}",
            "{inputs.feature}}",
            "{mystery}",
            "{ {inputs.feature} }",
            "}{",
            "plain",
        ] {
            assert_eq!(
                placeholders(tmpl).err(),
                render(tmpl, &ctx).err(),
                "{tmpl:?} parses one way and renders another"
            );
        }
    }

    #[test]
    fn path_lookup_edges() {
        let v = json!({"a": [1, 2], "b": {"": 5}});
        assert_eq!(json_path(&v, "a.0"), Some(&json!(1)));
        assert!(json_path(&v, "a.9").is_none());
        assert!(json_path(&v, "a.x").is_none());
        assert!(json_path(&v, "a.0.deeper").is_none());
        assert!(json_path(&v, "").is_none());
        assert!(json_path(&v, "b.").is_none());
    }

    #[test]
    fn an_answer_renders_what_the_person_said() {
        let inputs = BTreeMap::new();
        let mut steps = BTreeMap::new();
        steps.insert(
            step("q"),
            StepRecord {
                answer: Some(Answer::unsure()),
                ..StepRecord::default()
            },
        );
        let ctx = TemplateCtx {
            inputs: &inputs,
            steps: &steps,
            event: None,
            goal: Some(GoalText {
                statement: "",
                title: None,
            }),
            params: no_values(),
            account: no_values(),
        };
        assert_eq!(render("{steps.q.answer}", &ctx).unwrap(), "not sure");
    }

    fn shell_ctx<'a>(
        inputs: &'a BTreeMap<String, serde_json::Value>,
        steps: &'a BTreeMap<StepId, StepRecord>,
    ) -> TemplateCtx<'a> {
        TemplateCtx {
            inputs,
            steps,
            event: None,
            goal: Some(GoalText {
                statement: "",
                title: None,
            }),
            params: no_values(),
            account: no_values(),
        }
    }

    #[test]
    fn shell_context_quotes_every_value() {
        // The value carries a command separator and a second command. In a
        // shell context it becomes one quoted word; the template's own text
        // (`echo `) is left as the author wrote it.
        let inputs = BTreeMap::from([("x".to_string(), json!("a; touch marker"))]);
        let steps = BTreeMap::new();
        let out = render_in(
            "echo {inputs.x}",
            &shell_ctx(&inputs, &steps),
            RenderContext::ShellCommand,
        )
        .unwrap();
        assert_eq!(out, "echo 'a; touch marker'");
        // A number is quoted too: the rule has no exceptions to remember.
        let inputs = BTreeMap::from([("n".to_string(), json!(3))]);
        let out = render_in(
            "sleep {inputs.n}",
            &shell_ctx(&inputs, &steps),
            RenderContext::ShellCommand,
        )
        .unwrap();
        assert_eq!(out, "sleep '3'");
    }

    #[test]
    fn shell_context_escapes_single_quotes() {
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
        assert_eq!(shell_quote(""), "''");
        assert_eq!(shell_quote("$HOME `x` \"y\""), "'$HOME `x` \"y\"'");
        let inputs = BTreeMap::from([("x".to_string(), json!("it's"))]);
        let steps = BTreeMap::new();
        let out = render_in(
            "echo {inputs.x}",
            &shell_ctx(&inputs, &steps),
            RenderContext::ShellCommand,
        )
        .unwrap();
        assert_eq!(out, "echo 'it'\\''s'");
    }

    #[test]
    fn text_context_is_unchanged() {
        let inputs = BTreeMap::from([("x".to_string(), json!("a; touch marker"))]);
        let steps = BTreeMap::new();
        let ctx = shell_ctx(&inputs, &steps);
        assert_eq!(
            render("say {inputs.x}", &ctx).unwrap(),
            "say a; touch marker"
        );
        assert_eq!(
            render_in("say {inputs.x}", &ctx, RenderContext::Text).unwrap(),
            render("say {inputs.x}", &ctx).unwrap()
        );
    }
}
