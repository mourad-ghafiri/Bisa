//! Activity rendering: verb / object → outcome lines. Spine events (runs, steps, gates, results) are
//! bold; ambient progress is dim; failures rise.

use bisa_core::event::{JournalEvent, JournalPayload, RunFact, StepFact};
use bisa_core::workitem::WorkItemState;
use bisa_core::{
    Answer, AskKind, Boundary, BoundaryAct, BoundaryOn, MessageFilter, ProjectFilter, RunEnd,
    RunFilter, SignalFilter, StepKind, ValueRef, WaitFor,
};
use bisa_engine::{EngineEvent, EnginePayload, FiredOutcome};
use bisa_harness::{LifecycleEvent, Outcome, ProgressEvent, SessionEvent};
use std::io::IsTerminal;

pub struct Style {
    tty: bool,
}

impl Style {
    pub fn stderr() -> Self {
        Self {
            tty: std::io::stderr().is_terminal(),
        }
    }
    /// No colour, whatever the terminal: a line that is kept, not watched.
    pub fn plain() -> Self {
        Self { tty: false }
    }
    pub fn bold(&self, s: &str) -> String {
        if self.tty {
            format!("\x1b[1m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }
    pub fn dim(&self, s: &str) -> String {
        if self.tty {
            format!("\x1b[2m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }
    pub fn red(&self, s: &str) -> String {
        if self.tty {
            format!("\x1b[31m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }
}

pub fn workitem_state_label(state: &WorkItemState) -> String {
    match state {
        WorkItemState::Open => "open".into(),
        WorkItemState::Claimed { .. } => "claimed".into(),
        WorkItemState::InProgress { .. } => {
            bisa_i18n::say(&bisa_core::text!("cli-activity-progress"))
        }
        WorkItemState::Blocked { reason, .. } => format!("blocked: {reason}"),
        WorkItemState::Review { .. } => "review".into(),
        WorkItemState::Accepted => "accepted".into(),
        WorkItemState::Rejected { .. } => "rejected".into(),
        WorkItemState::Cancelled => "cancelled".into(),
    }
}

/// A step kind's one word, as the wire tags it — events, gateways, loops,
/// then tasks.
pub fn step_kind_label(kind: &StepKind) -> String {
    let word: &'static str = match kind {
        StepKind::Start { .. } => "start",
        StepKind::Wait { .. } => "wait",
        StepKind::Emit { .. } => "emit",
        StepKind::End { .. } => "end",
        StepKind::Decide { .. } => "decide",
        StepKind::If { .. } => "if",
        StepKind::Switch { .. } => "switch",
        StepKind::Judge { .. } => "judge",
        StepKind::Parallel => "parallel",
        StepKind::ForEach { .. } => "for_each",
        StepKind::While { .. } => "while",
        StepKind::Agent { .. } => "agent",
        StepKind::Human { .. } => "human",
        StepKind::Approval { .. } => "approval",
        StepKind::Check { .. } => "check",
        StepKind::Connector { .. } => "connector",
        StepKind::Notify { .. } => "notify",
        StepKind::Spawn { .. } => "spawn",
    };
    if word == "for_each" {
        return bisa_i18n::say(&bisa_core::text!("cli-activity-kind-for-each"));
    }
    word.to_string()
}

/// One judgement in a line: the point, who answered, what it chose and how
/// sure, and what became of it.
pub fn judgement_line(j: &bisa_core::Judgement) -> String {
    let answers: Vec<String> = j
        .answers
        .values()
        .map(|a| match a {
            bisa_core::DecisionAnswer::Noul { noul } => format!("{noul:.2}"),
            bisa_core::DecisionAnswer::Choice {
                choice, confidence, ..
            } => format!("{choice} ({confidence:.2})"),
            bisa_core::DecisionAnswer::Score {
                score, confidence, ..
            } => format!("{score:.1} ({confidence:.2})"),
        })
        .collect();
    bisa_i18n::say(&bisa_core::text!(
        "cli-activity-judgement",
        a0 = (j.point).to_string(),
        a1 = (j.model).to_string(),
        a2 = (j.outcome.as_str()).to_string(),
        a3 = (if answers.is_empty() {
            bisa_i18n::say(&bisa_core::text!("cli-activity-no-answer"))
        } else {
            answers.join(", ")
        })
        .to_string(),
        a4 = (j
            .reason
            .as_deref()
            .map(|r| format!(" — {}", short(r, 80)))
            .unwrap_or_default())
        .to_string()
    ))
}

/// What a `wait` step waits for, in a few words: one line per catch.
pub fn wait_label(until: &WaitFor) -> String {
    match until {
        WaitFor::Delay { secs } => seconds_label(secs),
        WaitFor::Time { at } => bisa_i18n::say(&bisa_core::text!(
            "cli-activity-wait-time",
            at = short(at, 60)
        )),
        WaitFor::Schedule {
            cron: ValueRef::Fixed(cron),
            ..
        } => bisa_i18n::say(&bisa_core::text!(
            "cli-activity-cron",
            cron = cron.to_string()
        )),
        WaitFor::Schedule {
            cron: ValueRef::Input { input },
            ..
        } => bisa_i18n::say(&bisa_core::text!(
            "cli-activity-cron-from",
            input = input.to_string()
        )),
        WaitFor::Signal { filter } => signal_label(filter),
        WaitFor::Message { filter } => message_label(filter),
        WaitFor::Project { filter } => project_label(filter),
        WaitFor::Run { filter } => run_label(filter),
        WaitFor::Platform { filter } => bisa_i18n::say(&bisa_core::text!(
            "cli-activity-wait-platform",
            topic = filter.topic.to_string()
        )),
        WaitFor::Release => bisa_i18n::say(&bisa_core::text!("cli-activity-person-release")),
    }
}

/// So many seconds to pass — fixed, or read from an input.
fn seconds_label(secs: &ValueRef<u64>) -> String {
    match secs {
        ValueRef::Fixed(secs) => bisa_i18n::say(&bisa_core::text!(
            "cli-activity-s-pass",
            secs = secs.to_string()
        )),
        ValueRef::Input { input } => bisa_i18n::say(&bisa_core::text!(
            "cli-activity-input-seconds-pass",
            input = input.to_string()
        )),
    }
}

/// A named signal, as a wait and a boundary event hold for it.
fn signal_label(filter: &SignalFilter) -> String {
    bisa_i18n::say(&bisa_core::text!(
        "cli-activity-wait-signal",
        name = filter.name.to_string()
    ))
}

/// A message, and where it has to land when the filter says.
fn message_label(filter: &MessageFilter) -> String {
    match &filter.r#in {
        Some(scope) => bisa_i18n::say(&bisa_core::text!(
            "cli-activity-wait-message-in",
            scope = scope.to_string()
        )),
        None => bisa_i18n::say(&bisa_core::text!("cli-activity-wait-message")),
    }
}

/// A project's change: which change (the core's word, which the message
/// selects on), in which project.
fn project_label(filter: &ProjectFilter) -> String {
    let change = filter.change.as_str();
    match &filter.project {
        Some(project) => bisa_i18n::say(&bisa_core::text!(
            "cli-activity-wait-project",
            change = change,
            project = project.word()
        )),
        None => bisa_i18n::say(&bisa_core::text!(
            "cli-activity-wait-project-unchosen",
            change = change
        )),
    }
}

/// A run's end: of which workflow, and how it has to end — `any` when the
/// filter names no outcome.
fn run_label(filter: &RunFilter) -> String {
    let outcome = filter.outcome.map_or("any", RunEnd::as_str);
    match &filter.workflow {
        Some(workflow) => bisa_i18n::say(&bisa_core::text!(
            "cli-activity-wait-run-of",
            workflow = workflow.to_string(),
            outcome = outcome
        )),
        None => bisa_i18n::say(&bisa_core::text!(
            "cli-activity-wait-run",
            outcome = outcome
        )),
    }
}

/// One boundary event of a live step, in a line: what it listens for and
/// what it does when heard. `fired` is how many times it fired this visit.
pub fn boundary_label(boundary: &Boundary, fired: u32) -> String {
    let on = match &boundary.on {
        BoundaryOn::After { secs } => bisa_i18n::say(&bisa_core::text!(
            "cli-activity-boundary-after",
            secs = secs.word()
        )),
        BoundaryOn::Every { secs, max } => bisa_i18n::say(&bisa_core::text!(
            "cli-activity-boundary-every",
            secs = secs.word(),
            fired = fired.to_string(),
            max = max.to_string()
        )),
        BoundaryOn::Message { filter } => bisa_i18n::say(&bisa_core::text!(
            "cli-activity-boundary-on",
            what = message_label(filter)
        )),
        BoundaryOn::Signal { filter } => bisa_i18n::say(&bisa_core::text!(
            "cli-activity-boundary-on",
            what = signal_label(filter)
        )),
    };
    let act = match &boundary.act {
        BoundaryAct::Divert => bisa_i18n::say(&bisa_core::text!("cli-activity-boundary-diverts")),
        BoundaryAct::Notify { .. } => {
            bisa_i18n::say(&bisa_core::text!("cli-activity-boundary-posts"))
        }
        BoundaryAct::Emit { signal, .. } => bisa_i18n::say(&bisa_core::text!(
            "cli-activity-boundary-emits",
            signal = signal.to_string()
        )),
    };
    bisa_i18n::say(&bisa_core::text!(
        "cli-activity-boundary",
        name = boundary.name.to_string(),
        act = act,
        on = on
    ))
}

fn short(s: &str, n: usize) -> String {
    let s = s.replace('\n', " ");
    if s.chars().count() > n {
        let mut t: String = s.chars().take(n).collect();
        t.push('…');
        t
    } else {
        s
    }
}

/// One journal event → one activity line.
pub fn journal_line(e: &JournalEvent) -> String {
    let who = &e.author.to_string()[..8];
    format!("{:>10}  {who}  {}", e.at, payload_line(&e.payload))
}

/// The payload half of a journal line, with no envelope around it.
///
/// Split out because `bisa pulse` reads the node's `/pulse`, and that
/// route now ships the payload rather than a rendered sentence — so the CLI
/// needs the same words without an author or a timestamp it would print
/// twice.
pub fn payload_line(payload: &JournalPayload) -> String {
    match payload {
        JournalPayload::Note { text } => bisa_i18n::say(&bisa_core::text!(
            "cli-activity-noted",
            a0 = (short(text, 70)).to_string()
        )),
        JournalPayload::Guidance {
            phase,
            status,
            detail,
            ..
        } => bisa_i18n::say(&bisa_core::text!(
            "cli-activity-workflow-agent",
            a0 = (phase.as_str()).to_string(),
            a1 = (status.as_str()).to_string(),
            a2 = (detail
                .as_deref()
                .map(|d| format!(" — {}", short(d, 60)))
                .unwrap_or_default())
            .to_string()
        )),
        JournalPayload::Judgement { judgement, .. } => judgement_line(judgement),
        JournalPayload::Guard {
            tool,
            subject,
            verdict,
            by,
            rule,
            reason,
        } => bisa_i18n::say(&bisa_core::text!(
            "cli-activity-guard",
            a0 = (verdict.as_str()).to_string(),
            tool = tool.to_string(),
            a1 = (by.as_str()).to_string(),
            a2 = (rule
                .as_deref()
                .map(|r| format!(" ({r})"))
                .unwrap_or_default())
            .to_string(),
            a3 = (if subject.is_empty() {
                String::new()
            } else {
                format!(": {}", short(subject, 60))
            })
            .to_string(),
            a4 = (reason
                .as_deref()
                .map(|r| format!(" — {}", short(r, 60)))
                .unwrap_or_default())
            .to_string()
        )),
        JournalPayload::Step { step, event, .. } => match event {
            StepFact::Started { work_item: Some(w) } => bisa_i18n::say(&bisa_core::text!(
                "cli-activity-step-started-item",
                step = step.to_string(),
                w = w.to_string()
            )),
            StepFact::Started { work_item: None } => bisa_i18n::say(&bisa_core::text!(
                "cli-activity-step-started",
                step = step.to_string()
            )),
            StepFact::Waiting => bisa_i18n::say(&bisa_core::text!(
                "cli-activity-step-waiting",
                step = step.to_string()
            )),
            // A gateway names the branches it chose — one, or every one that
            // held; a task, an event and a `parallel` name none.
            StepFact::Done { branches } if branches.is_empty() => bisa_i18n::say(
                &bisa_core::text!("cli-activity-step-done-2", step = step.to_string()),
            ),
            StepFact::Done { branches } => bisa_i18n::say(&bisa_core::text!(
                "cli-activity-step-done",
                step = step.to_string(),
                b = branches
                    .iter()
                    .map(|b| b.to_string())
                    .collect::<Vec<_>>()
                    .join(" · ")
            )),
            StepFact::Failed { error } => bisa_i18n::say(&bisa_core::text!(
                "cli-activity-step-failed",
                step = step.to_string(),
                a0 = (short(error, 50)).to_string()
            )),
            StepFact::Diverted { by } => bisa_i18n::say(&bisa_core::text!(
                "cli-activity-step-diverted",
                step = step.to_string(),
                by = by.to_string()
            )),
            StepFact::Boundary { boundary } => bisa_i18n::say(&bisa_core::text!(
                "cli-activity-step-boundary",
                step = step.to_string(),
                boundary = boundary.to_string()
            )),
            StepFact::Answered { answer } => bisa_i18n::say(&bisa_core::text!(
                "cli-activity-step-answered",
                step = step.to_string(),
                a0 = (answer_line(answer)).to_string()
            )),
            StepFact::Decided { approve, .. } => format!(
                "step {step} {}",
                if *approve { "approved" } else { "declined" }
            ),
            StepFact::Skipped => bisa_i18n::say(&bisa_core::text!(
                "cli-activity-step-skipped",
                step = step.to_string()
            )),
            StepFact::Cancelled => bisa_i18n::say(&bisa_core::text!(
                "cli-activity-step-cancelled",
                step = step.to_string()
            )),
        },
        JournalPayload::Run { run, event } => match event {
            RunFact::Queued { workflow, revision } => bisa_i18n::say(&bisa_core::text!(
                "cli-activity-run-queued-workflow-rev",
                run = run.to_string(),
                workflow = workflow.to_string(),
                revision = revision.to_string()
            )),
            RunFact::Started {
                workflow,
                revision,
                start,
                signal,
            } => bisa_i18n::say(&bisa_core::text!(
                "cli-activity-run-started-workflow-rev",
                run = run.to_string(),
                workflow = workflow.to_string(),
                revision = revision.to_string(),
                entry = entry_words(start.as_ref().map(|s| s.as_str()), signal.as_deref())
            )),
            RunFact::Amended { revision } => bisa_i18n::say(&bisa_core::text!(
                "cli-activity-run-amended-revision",
                run = run.to_string(),
                revision = revision.to_string()
            )),
            RunFact::Finished { outcome } => bisa_i18n::say(&bisa_core::text!(
                "cli-activity-run-finished",
                run = run.to_string(),
                a0 = (outcome.as_str()).to_string()
            )),
            RunFact::Cancelled { cause } => format!("run {run} {}", cause.as_str()),
        },
        JournalPayload::Decision {
            gate,
            approve,
            subject,
            answer,
            ..
        } => match answer {
            Some(a) => format!("answered: {}", answer_line(a)),
            None => bisa_i18n::say(&bisa_core::text!(
                "cli-activity-gate",
                a0 = (if *approve { "approved" } else { "rejected" }).to_string(),
                a1 = (gate.as_str()).to_string(),
                a2 = (short(subject, 26)).to_string()
            )),
        },
        JournalPayload::Question { text, expects, .. } => match expects {
            AskKind::Answer { options, .. } if options.is_empty() => bisa_i18n::say(
                &bisa_core::text!("cli-activity-asked-you", a0 = (short(text, 60)).to_string()),
            ),
            AskKind::Answer { options, .. } => bisa_i18n::say(&bisa_core::text!(
                "cli-activity-asked-you-option-s",
                a0 = (short(text, 46)).to_string(),
                a1 = (options.len()).to_string()
            )),
            AskKind::Decision => bisa_i18n::say(&bisa_core::text!(
                "cli-activity-asked-decision",
                a0 = (short(text, 55)).to_string()
            )),
        },
        JournalPayload::Withdrawn { subject, reason } => bisa_i18n::say(&bisa_core::text!(
            "cli-activity-withdrew",
            a0 = (short(subject, 40)).to_string(),
            a1 = (short(reason, 50)).to_string()
        )),
        JournalPayload::Claim {
            work_item, harness, ..
        } => bisa_i18n::say(&bisa_core::text!(
            "cli-activity-claimed-item-via",
            work_item = work_item.to_string(),
            harness = harness.to_string()
        )),
        JournalPayload::Progress {
            verb,
            object,
            outcome,
            ..
        } => match outcome {
            Some(o) => format!("{verb} {} → {}", short(object, 40), short(o, 30)),
            None => format!("{verb} {}", short(object, 50)),
        },
        JournalPayload::Result {
            work_item,
            artifacts,
            ..
        } => bisa_i18n::say(&bisa_core::text!(
            "cli-activity-submitted-result-artifact-s",
            work_item = work_item.to_string(),
            a0 = (artifacts.len()).to_string()
        )),
        JournalPayload::Attachment { project, attached } => format!(
            "{} project {project}",
            if *attached { "attached" } else { "detached" }
        ),
        JournalPayload::Document { file } => bisa_i18n::say(&bisa_core::text!(
            "cli-activity-gave-document-bytes",
            a0 = (file.name).to_string(),
            a1 = (file.mime).to_string(),
            a2 = (file.size).to_string()
        )),
        JournalPayload::Signal {
            signal,
            listener,
            source,
            name,
            payload,
        } => {
            let what = signal_words(source.as_str(), name.as_deref());
            let carried = carried_words(payload);
            match listener {
                Some(listener) => bisa_i18n::say(&bisa_core::text!(
                    "cli-activity-signal-heard",
                    signal = signal.to_string(),
                    what = what,
                    listener = listener.to_string(),
                    carried = carried
                )),
                None => bisa_i18n::say(&bisa_core::text!(
                    "cli-activity-signal-raised",
                    signal = signal.to_string(),
                    what = what,
                    carried = carried
                )),
            }
        }
        JournalPayload::TurnMetrics {
            input_tokens,
            output_tokens,
            usd_cents,
            ..
        } => bisa_i18n::say(&bisa_core::text!(
            "cli-activity-turn-tokens",
            input_tokens = input_tokens.to_string(),
            output_tokens = output_tokens.to_string(),
            usd_cents = usd_cents.to_string()
        )),
    }
}

/// What a human said, on one line.
///
/// Selection and text are both shown when both are present: a person who picks
/// an option *and* qualifies it has said two things, and an activity view that renders
/// only one of them reads as if they said less than they did.
fn answer_line(a: &Answer) -> String {
    if a.unsure {
        return match a.text.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            Some(t) => format!("“not sure” — {}", short(t, 44)),
            None => "“I'm not sure”".to_string(),
        };
    }
    let picked = a.selected.join(", ");
    match (picked.is_empty(), a.text.as_deref()) {
        (true, Some(t)) => format!("“{}”", short(t, 60)),
        (false, Some(t)) => format!("{} — “{}”", short(&picked, 24), short(t, 32)),
        (false, None) => short(&picked, 60),
        (true, None) => "(nothing)".to_string(),
    }
}

/// Where a run began, said after its sentence: the start step it entered and
/// the signal that began it — nothing for a run by hand at a workflow's root.
fn entry_words(start: Option<&str>, signal: Option<&str>) -> String {
    let mut words = String::new();
    if let Some(start) = start {
        words.push_str(&bisa_i18n::say(&bisa_core::text!(
            "cli-activity-run-entry-start",
            start = start.to_string()
        )));
    }
    if let Some(signal) = signal {
        words.push_str(&bisa_i18n::say(&bisa_core::text!(
            "cli-activity-run-entry-signal",
            signal = signal.to_string()
        )));
    }
    words
}

/// A signal's kind in words: its source, and its name when it has one —
/// `hook`, `signal report.ready`, `platform goal.closed`.
fn signal_words(source: &str, name: Option<&str>) -> String {
    match name {
        Some(name) => format!("{source} {name}"),
        None => source.to_string(),
    }
}

/// What a signal carried, cut to a glance; nothing for an empty payload.
fn carried_words(payload: &serde_json::Value) -> String {
    let empty = match payload {
        serde_json::Value::Null => true,
        serde_json::Value::Object(map) => map.is_empty(),
        _ => false,
    };
    if empty {
        return String::new();
    }
    bisa_i18n::say(&bisa_core::text!(
        "cli-activity-signal-carried",
        payload = short(&payload.to_string(), 60)
    ))
}

/// An occurrence was written down — for a listener, or kept for the waits
/// that replay it.
fn signal_received_line(scope: &str, signal: &str, listener: Option<&str>, source: &str) -> String {
    match listener {
        Some(listener) => bisa_i18n::say(&bisa_core::text!(
            "cli-activity-signal-received",
            scope = scope.to_string(),
            signal = signal.to_string(),
            source = source.to_string(),
            listener = listener.to_string()
        )),
        None => bisa_i18n::say(&bisa_core::text!(
            "cli-activity-signal-kept",
            scope = scope.to_string(),
            signal = signal.to_string(),
            source = source.to_string()
        )),
    }
}

/// What became of a signal at its listener, as both renderers read it.
enum Fired<'a> {
    /// It started this run — on a goal, or in the workspace.
    Started { run: &'a str, goal: Option<&'a str> },
    /// It started nothing, and why.
    Skipped { reason: &'a str },
}

/// A queued signal met its listener: a run, or the reason there is none.
fn listener_fired_line(scope: &str, listener: &str, signal: &str, fired: Fired<'_>) -> String {
    match fired {
        Fired::Started {
            run,
            goal: Some(goal),
        } => bisa_i18n::say(&bisa_core::text!(
            "cli-activity-listener-started-on-goal",
            scope = scope.to_string(),
            listener = listener.to_string(),
            signal = signal.to_string(),
            run = run.to_string(),
            goal = goal.to_string()
        )),
        Fired::Started { run, goal: None } => bisa_i18n::say(&bisa_core::text!(
            "cli-activity-listener-started-in-workspace",
            scope = scope.to_string(),
            listener = listener.to_string(),
            signal = signal.to_string(),
            run = run.to_string()
        )),
        Fired::Skipped { reason } => bisa_i18n::say(&bisa_core::text!(
            "cli-activity-listener-skipped",
            scope = scope.to_string(),
            listener = listener.to_string(),
            signal = signal.to_string(),
            reason = short(reason, 80)
        )),
    }
}

/// A listener that could not be armed, or whose signal could not start its run.
fn listener_failed_line(scope: &str, listener: &str, error: &str) -> String {
    bisa_i18n::say(&bisa_core::text!(
        "cli-activity-listener-failed",
        scope = scope.to_string(),
        listener = listener.to_string(),
        error = short(error, 80)
    ))
}

/// A host began or stopped hearing its start events.
fn listening_changed_line(scope: &str, host: &str, on: bool) -> String {
    if on {
        bisa_i18n::say(&bisa_core::text!(
            "cli-activity-listening-on",
            scope = scope.to_string(),
            host = host.to_string()
        ))
    } else {
        bisa_i18n::say(&bisa_core::text!(
            "cli-activity-listening-off",
            scope = scope.to_string(),
            host = host.to_string()
        ))
    }
}

/// A boundary event of a live step fired: it diverted the step, or it acted
/// beside it.
fn boundary_fired_line(scope: &str, step: &str, boundary: &str, diverts: bool) -> String {
    if diverts {
        bisa_i18n::say(&bisa_core::text!(
            "cli-activity-boundary-diverted",
            scope = scope.to_string(),
            step = step.to_string(),
            boundary = boundary.to_string()
        ))
    } else {
        bisa_i18n::say(&bisa_core::text!(
            "cli-activity-boundary-acted",
            scope = scope.to_string(),
            step = step.to_string(),
            boundary = boundary.to_string()
        ))
    }
}

/// One engine event → an optional activity line (None = too noisy to show).
pub fn engine_line(style: &Style, e: &EngineEvent) -> Option<String> {
    let scope = e
        .work_item
        .map(|w| format!("[{}] ", &w.to_string()[20..]))
        .unwrap_or_default();
    let line = match &e.payload {
        EnginePayload::Scheduled { harness } => {
            style.bold(&bisa_i18n::say(&bisa_core::text!("cli-activity-scheduled-onto", scope = scope.to_string(), harness = harness.to_string())))
        }
        // Presence: one line when a session waits, fails or is done; the
        // streaming states are the roster's, not the activity view.'s.
        EnginePayload::SessionState { presence, .. } => {
            let who = presence
                .agent
                .as_ref()
                .map(|a| a.to_string())
                .unwrap_or_else(|| presence.harness.clone());
            match &presence.state {
                bisa_engine::SessionState::Waiting { on } => {
                    let what = match on {
                        bisa_engine::WaitingOn::Permission { tool, .. } => format!("permission: {tool}"),
                        bisa_engine::WaitingOn::Question { text, .. } => short(text, 60),
                        bisa_engine::WaitingOn::Gate { gate, .. } => format!("{gate:?} gate").to_lowercase(),
                        bisa_engine::WaitingOn::Auth { provider, .. } => bisa_i18n::say(&bisa_core::text!("cli-activity-sign", provider = provider.to_string())),
                    };
                    style.bold(&bisa_i18n::say(&bisa_core::text!("cli-activity-waiting-on-you", scope = scope.to_string(), who = who.to_string(), what = what.to_string())))
                }
                bisa_engine::SessionState::Failed { reason } => {
                    style.red(&format!("{scope}✗ {who} failed — {}", short(reason, 60)))
                }
                bisa_engine::SessionState::Aborted => style.red(&format!("{scope}✗ {who} aborted")),
                bisa_engine::SessionState::Done => style.dim(&format!("{scope}✓ {who} done")),
                bisa_engine::SessionState::Parked => style.dim(&format!("{scope}{who} parked")),
                _ => return None,
            }
        }
        EnginePayload::SessionGone { .. } => return None,
        EnginePayload::ExecutionEnded { outcome } => {
            style.bold(&bisa_i18n::say(&bisa_core::text!("cli-activity-execution-ended", scope = scope.to_string(), outcome = outcome.to_string())))
        }
        EnginePayload::GateOpened { question, .. } => {
            style.bold(&bisa_i18n::say(&bisa_core::text!("cli-activity-waiting-on-you-question", scope = scope.to_string(), question = question.to_string())))
        }
        EnginePayload::QuestionAsked { text, expects, .. } => match expects {
            AskKind::Answer { options, .. } if options.is_empty() => {
                style.bold(&bisa_i18n::say(&bisa_core::text!("cli-activity-question-reply", scope = scope.to_string(), text = text.to_string())))
            }
            AskKind::Answer { options, .. } => style.bold(&format!(
                "{scope}? {text}\n   {}\n   (reply: bisa answer <id> -o <option>, or --unsure)",
                options
                    .iter()
                    .map(|o| format!(
                        "[{}] {}{}",
                        o.id,
                        o.label,
                        if o.recommended { " (recommended)" } else { "" }
                    ))
                    .collect::<Vec<_>>()
                    .join("  ")
            )),
            AskKind::Decision => style.bold(&format!("{scope}? {text}")),
        },
        EnginePayload::Guided {
            phase,
            status,
            detail,
            ..
        } => style.dim(&bisa_i18n::say(&bisa_core::text!("cli-activity-workflow-agent", a0 = (phase.as_str()).to_string(), a1 = (status.as_str()).to_string(), a2 = (detail.as_deref().map(|d| format!(" — {d}")).unwrap_or_default()).to_string()))),
        EnginePayload::AgentThinking { agent, .. } => style.dim(&bisa_i18n::say(&bisa_core::text!("cli-activity-replying", agent = agent.to_string()))),
        // Each frame of the reply as it is written: noise in a stream of acts.
        EnginePayload::AgentStreamed { .. } => return None,
        EnginePayload::AgentReplied { agent, posted, .. } => {
            if *posted {
                style.bold(&format!("{agent} replied"))
            } else {
                style.dim(&bisa_i18n::say(&bisa_core::text!("cli-activity-finished-turn", agent = agent.to_string())))
            }
        }
        EnginePayload::GateDecided { gate, approve, .. } => style.bold(&format!(
            "{scope}{} gate {}",
            gate.as_str(),
            if *approve { "approved" } else { "rejected" }
        )),
        EnginePayload::ResultAccepted => style.bold(&bisa_i18n::say(&bisa_core::text!("cli-activity-result-accepted", scope = scope.to_string()))),
        EnginePayload::RunStarted { run, workflow } => {
            style.bold(&bisa_i18n::say(&bisa_core::text!("cli-activity-run-started-workflow", run = run.to_string(), workflow = workflow.to_string())))
        }
        EnginePayload::RunQueued {
            run,
            workflow,
            position,
        } => style.dim(&bisa_i18n::say(&bisa_core::text!("cli-activity-run-queued-workflow-position", run = run.to_string(), workflow = workflow.to_string(), position = position.to_string()))),
        EnginePayload::RunFinished { run, outcome, .. } => match outcome {
            bisa_core::RunOutcome::Done => style.bold(&bisa_i18n::say(&bisa_core::text!("cli-activity-run-finished-done", run = run.to_string()))),
            bisa_core::RunOutcome::Failed => style.red(&bisa_i18n::say(&bisa_core::text!("cli-activity-run-finished-failed", run = run.to_string()))),
        },
        EnginePayload::RunCancelled { run, cause, .. } => {
            style.bold(&format!("run {run} {}", cause.as_str()))
        }
        EnginePayload::StepChanged { step, state, kind, .. } => match state.as_str() {
            "failed" => style.red(&bisa_i18n::say(&bisa_core::text!("cli-activity-step-line-failed", scope = scope.to_string(), step = step.to_string(), kind = kind.to_string()))),
            "done" => style.bold(&bisa_i18n::say(&bisa_core::text!("cli-activity-step-line-done", scope = scope.to_string(), step = step.to_string(), kind = kind.to_string()))),
            "waiting" => style.bold(&bisa_i18n::say(&bisa_core::text!("cli-activity-step-line-waiting", scope = scope.to_string(), step = step.to_string(), kind = kind.to_string()))),
            "diverted" => style.bold(&bisa_i18n::say(&bisa_core::text!("cli-activity-step-line-diverted", scope = scope.to_string(), step = step.to_string(), kind = kind.to_string()))),
            other => style.dim(&bisa_i18n::say(&bisa_core::text!("cli-activity-step", scope = scope.to_string(), step = step.to_string(), kind = kind.to_string(), other = other.to_string()))),
        },
        EnginePayload::GoalClosed { reason } => {
            style.bold(&bisa_i18n::say(&bisa_core::text!("cli-activity-goal-closed", a0 = (reason.as_str()).to_string())))
        }
        EnginePayload::GoalArchived { goal, archived } => {
            style.dim(&format!("goal {goal} {}", if *archived { "archived" } else { "unarchived" }))
        }
        EnginePayload::GoalDeleted { goal } => style.bold(&bisa_i18n::say(&bisa_core::text!("cli-activity-goal-deleted", goal = goal.to_string()))),
        EnginePayload::WorkflowDeleted { workflow } => {
            style.bold(&bisa_i18n::say(&bisa_core::text!("cli-activity-workflow-deleted", workflow = workflow.to_string())))
        }
        EnginePayload::WorkflowArchived { workflow, archived } => style.dim(&format!(
            "workflow {workflow} {}",
            if *archived { "archived" } else { "unarchived" }
        )),
        EnginePayload::ProjectArchived { project, archived } => {
            style.dim(&format!("project {project} {}", if *archived { "archived" } else { "unarchived" }))
        }
        EnginePayload::ProjectDeleted { project } => style.bold(&bisa_i18n::say(&bisa_core::text!("cli-activity-project-deleted", project = project.to_string()))),
        EnginePayload::GoalCreated { goal, origin } => {
            style.bold(&bisa_i18n::say(&bisa_core::text!("cli-activity-goal-created", goal = goal.to_string(), a0 = (origin.as_str()).to_string())))
        }
        EnginePayload::DocumentAdded { file, .. } => {
            style.dim(&bisa_i18n::say(&bisa_core::text!("cli-activity-document-given-bytes", a0 = (file.name).to_string(), a1 = (file.mime).to_string(), a2 = (file.size).to_string())))
        }
        EnginePayload::WorkflowProposed {
            workflow, revision, ..
        } => style.bold(&format!(
            "⏸ the Workflow Agent proposed workflow {workflow} rev {revision} — adopt with: bisa approve <goal> [--input k=v]"
        )),
        EnginePayload::WorkflowChanged {
            workflow, revision, ..
        } => {
            style.dim(&bisa_i18n::say(&bisa_core::text!("cli-activity-workflow-changed-rev", workflow = workflow.to_string(), revision = revision.to_string())))
        }
        // The line a project's creation gets everywhere. Bold rather than
        // dim: a project is a folder on disk this workspace now owns, and a
        // folder that appeared without anybody reading a line about it is the
        // shape of the bug this variant was added to close.
        EnginePayload::ProjectCreated { slug, project, .. } => {
            style.bold(&bisa_i18n::say(&bisa_core::text!("cli-activity-project-created", slug = slug.to_string(), project = project.to_string())))
        }
        // Bold: a repository nobody can commit in is a question for a person,
        // wherever the project came from — a hand, a step, an agent.
        EnginePayload::CommitterNeeded { slug, reason, global, .. } => style.bold(&bisa_i18n::say(&bisa_core::text!("cli-activity-project-needs-someone-commit", slug = slug.to_string(), a0 = (reason.as_str().replace('_', " ")).to_string(), a1 = (match global { Some(g) => format!(" — global git config names {g}; pin it with `bisa project identity`"), None => bisa_i18n::say(&bisa_core::text!("cli-activity-set-one-with-bisa-project-identity")), }).to_string()))),
        EnginePayload::CommitterSet { project, identity, .. } => {
            bisa_i18n::say(&bisa_core::text!("cli-activity-project-commits", project = project.to_string(), identity = identity.to_string()))
        }
        EnginePayload::GitSetupChanged { what } => bisa_i18n::say(&bisa_core::text!("cli-activity-git-setup-changed", a0 = (what.as_str()).to_string())),
        EnginePayload::ConnectorsChanged { what } => {
            bisa_i18n::say(&bisa_core::text!("cli-activity-connectors-changed", a0 = (what.as_str()).to_string()))
        }
        EnginePayload::AddonsChanged { what } => {
            bisa_i18n::say(&bisa_core::text!("cli-activity-addons-changed", a0 = (what.as_str()).to_string(), id = what.id().to_string()))
        }
        EnginePayload::ConversationCreated { id, origin } => {
            bisa_i18n::say(&bisa_core::text!("cli-activity-conversation-started-about", id = id.to_string(), a0 = (origin.kind()).to_string()))
        }
        EnginePayload::ConversationChanged { id, change } => {
            format!("conversation {id} {change:?}").to_lowercase()
        }
        EnginePayload::ProjectChanged { project } => bisa_i18n::say(&bisa_core::text!("cli-activity-project-edited", project = project.to_string())),
        EnginePayload::WorkstreamEdited { workstream, .. } => {
            bisa_i18n::say(&bisa_core::text!("cli-activity-workstream-edited", workstream = workstream.to_string()))
        }
        // Attaching is the whole of the goal ⇄ project relation, so it is
        // news on the goal's line rather than a dim aside.
        EnginePayload::AttachmentChanged { project, attached } => style.bold(&format!(
            "{scope}project {project} {}",
            if *attached { "attached" } else { "detached" }
        )),
        // Dim, not bold: a note changing is somebody's scratchpad moving, and
        // the activity view's bold is spent on things that change what the workspace
        // is.
        EnginePayload::NoteChanged { note, .. } => style.dim(&bisa_i18n::say(&bisa_core::text!("cli-activity-note-changed", note = note.to_string()))),
        EnginePayload::DrawingChanged { drawing, .. } => style.dim(&bisa_i18n::say(&bisa_core::text!("cli-activity-drawing-changed", drawing = drawing.to_string()))),
        EnginePayload::DrawingRequest { request, .. } => style.dim(&bisa_i18n::say(&bisa_core::text!("cli-activity-agent-asked-canvas", scope = scope.to_string(), a0 = (serde_json::to_value(request.action) .ok() .and_then(|v| v.as_str().map(str::to_string)) .unwrap_or_default()).to_string()))),
        // Failover is never silent: the reader has to be able to tell which
        // model produced which half of a transcript.
        EnginePayload::ModelSwitched {
            from,
            to,
            reason,
            retry_in_secs,
            after_progress,
            ..
        } => style.bold(&bisa_i18n::say(&bisa_core::text!(
            "cli-activity-model-unavailable-retrying",
            scope = scope.to_string(),
            from = from.to_string(),
            reason = short(reason, 60),
            retry_in_secs = retry_in_secs.to_string(),
            to = to.to_string(),
            tail = if *after_progress {
                bisa_i18n::say(&bisa_core::text!("cli-activity-work-had-already-started"))
            } else {
                String::new()
            }
        ))),
        // NOTE(wave 2): these three variants arrived with the workstream work
        // in `engine/events.rs`; rendering them here is what kept the CLI
        // compiling. Wave 3B owns the workstream CLI and may want richer lines.
        EnginePayload::WorkstreamOpened {
            workstream,
            branch,
            path,
            ..
        } => style.dim(&bisa_i18n::say(&bisa_core::text!("cli-activity-workstream-opened", scope = scope.to_string(), workstream = workstream.to_string(), a0 = (branch .as_ref() .map(|b| format!(" on {b}")) .unwrap_or_default()).to_string(), path = path.to_string()))),
        EnginePayload::WorkstreamChanged { workstream, state } => {
            style.dim(&bisa_i18n::say(&bisa_core::text!("cli-activity-workstream", scope = scope.to_string(), workstream = workstream.to_string(), state = format!("{state:?}"))))
        }
        EnginePayload::WorkstreamCommitted {
            workstream,
            branch,
            commit,
        } => style.bold(&bisa_i18n::say(&bisa_core::text!("cli-activity-workstream-committed", scope = scope.to_string(), workstream = workstream.to_string(), a0 = (short(commit, 12)).to_string(), branch = branch.to_string()))),
        // A script that failed is the one nobody was looking at: red. One
        // that ran is an ambient fact.
        EnginePayload::WorkstreamScriptRan {
            workstream,
            phase,
            ok,
            output,
            ..
        } => {
            let line = bisa_i18n::say(&bisa_core::text!("cli-activity-workstream-script", scope = scope.to_string(), workstream = workstream.to_string(), phase = phase.to_string(), a0 = (short(output.lines().next().unwrap_or(""), 80)).to_string()));
            if *ok {
                style.dim(&line)
            } else {
                style.red(&line)
            }
        }
        // What a person approved and did not go out: nobody was looking.
        EnginePayload::WorkstreamPublishFailed {
            workstream,
            what,
            reason,
            ..
        } => style.red(&bisa_i18n::say(&bisa_core::text!("cli-activity-workstream-publish-failed", scope = scope.to_string(), workstream = workstream.to_string(), what = what.to_string(), reason = (short(reason.lines().next().unwrap_or(""), 120)).to_string()))),
        EnginePayload::ServerChanged { workstream: Some(workstream) } => {
            style.dim(&bisa_i18n::say(&bisa_core::text!("cli-activity-workstream-served-folders-changed", scope = scope.to_string(), workstream = workstream.to_string())))
        }
        EnginePayload::ServerChanged { workstream: None } => {
            style.dim(&bisa_i18n::say(&bisa_core::text!("cli-activity-served-artifact-pages-changed", scope = scope.to_string())))
        }
        EnginePayload::PeopleChanged { pubkey, change, label } => {
            let who = label.clone().unwrap_or_else(|| format!("{}…", &pubkey.as_hex()[..8]));
            let what = match change {
                bisa_store::PeopleChange::Joined { role } => bisa_i18n::say(&bisa_core::text!("cli-activity-joined-as", role = role.to_string())),
                bisa_store::PeopleChange::Left => "left".to_string(),
                bisa_store::PeopleChange::RoleChanged { role } => bisa_i18n::say(&bisa_core::text!("cli-activity-now", role = role.to_string())),
                bisa_store::PeopleChange::ProfileChanged => bisa_i18n::say(&bisa_core::text!("cli-activity-changed-their-profile")),
            };
            format!("{scope}{who} {what}")
        }
        EnginePayload::InviteChanged { invite } => style.dim(&bisa_i18n::say(&bisa_core::text!("cli-activity-invite", scope = scope.to_string(), a0 = (invite.id).to_string(), a1 = (invite.role).to_string(), a2 = (invite.state.as_str()).to_string()))),
        EnginePayload::MessageHeld { scope: room, author, reason, .. } => style.red(&bisa_i18n::say(&bisa_core::text!("cli-activity-message-from", scope = scope.to_string(), a0 = (author.as_hex()[..8]).to_string(), room = room.to_string(), a1 = (reason.words()).to_string()))),
        EnginePayload::MessageReleased { scope: room, .. } => {
            style.dim(&bisa_i18n::say(&bisa_core::text!("cli-activity-held-message-was-released", scope = scope.to_string(), room = room.to_string())))
        }
        EnginePayload::ContentScreened {
            agent,
            source,
            verdict,
            ..
        } => match verdict {
            bisa_engine::content::ContentVerdict::Withheld => {
                style.red(&bisa_i18n::say(&bisa_core::text!("cli-activity-content-from-was-withheld-from", scope = scope.to_string(), source = source.to_string(), agent = agent.to_string())))
            }
            other => style.dim(&bisa_i18n::say(&bisa_core::text!("cli-activity-read-content-from", scope = scope.to_string(), agent = agent.to_string(), source = source.to_string(), a0 = (other.as_str()).to_string()))),
        },
        EnginePayload::HostedChanged { host, .. } => style.dim(&bisa_i18n::say(&bisa_core::text!("cli-activity-what-node-holds-s-workspace-changed", scope = scope.to_string(), a0 = (host.as_hex()[..8]).to_string()))),
        EnginePayload::RelaysChanged => style.dim(&bisa_i18n::say(&bisa_core::text!("cli-activity-relays-changed", scope = scope.to_string()))),
        EnginePayload::BrowserRequest { request, .. } => style.dim(&bisa_i18n::say(&bisa_core::text!("cli-activity-agent-asked-embedded-browser", scope = scope.to_string(), a0 = (serde_json::to_value(request.action) .ok() .and_then(|v| v.as_str().map(str::to_string)) .unwrap_or_default()).to_string()))),
        EnginePayload::MobileDevelopmentChanged { what } => style.dim(&bisa_i18n::say(&bisa_core::text!("cli-activity-mobile-development-changed", scope = scope.to_string(), a0 = (match what { bisa_engine::mobile_development::MobileDevelopmentChange::Toolchain => "toolchain", bisa_engine::mobile_development::MobileDevelopmentChange::Devices => "devices", }).to_string()))),
        // A refusal is the line a person came to read; an allow or a question
        // is an ambient fact.
        EnginePayload::GuardDecided {
            tool,
            subject,
            verdict,
            by,
            reason,
            ..
        } => {
            let line = bisa_i18n::say(&bisa_core::text!("cli-activity-guard-2", scope = scope.to_string(), a0 = (verdict.as_str()).to_string(), tool = tool.to_string(), a1 = (by.as_str()).to_string(), a2 = (if subject.is_empty() { String::new() } else { format!(": {}", short(subject, 60)) }).to_string(), a3 = (reason.as_deref().map(|r| format!(" — {}", short(r, 60))).unwrap_or_default()).to_string()));
            match verdict {
                bisa_core::event::GuardVerdict::Denied => style.red(&line),
                _ => style.dim(&line),
            }
        }
        EnginePayload::Judged { judgement, .. } => {
            style.dim(&format!("{scope}{}", judgement_line(judgement)))
        }
        EnginePayload::Redacted { count, kinds, at } => style.dim(&bisa_i18n::say(&bisa_core::text!("cli-activity-redacted-secret-before", scope = scope.to_string(), count = count.to_string(), a0 = (if *count == 1 { "" } else { "s" }).to_string(), at = at.to_string(), a1 = (kinds.join(", ")).to_string()))),
        // A signal is queued, not done: dim, like any other ambient fact.
        // The line that matters is what its listener made of it, below.
        EnginePayload::SignalReceived {
            signal,
            listener,
            source,
        } => {
            let listener = listener.as_ref().map(|l| l.to_string());
            style.dim(&signal_received_line(
                &scope,
                signal,
                listener.as_deref(),
                source.as_str(),
            ))
        }
        EnginePayload::ListenerFired {
            listener,
            signal,
            outcome,
        } => {
            let listener = listener.to_string();
            match outcome {
                FiredOutcome::Started { run, goal } => {
                    let run = run.to_string();
                    let goal = goal.as_ref().map(|g| g.to_string());
                    let fired = Fired::Started {
                        run: &run,
                        goal: goal.as_deref(),
                    };
                    style.bold(&listener_fired_line(&scope, &listener, signal, fired))
                }
                FiredOutcome::Skipped { reason } => style.dim(&listener_fired_line(
                    &scope,
                    &listener,
                    signal,
                    Fired::Skipped {
                        reason: reason.as_str(),
                    },
                )),
            }
        }
        EnginePayload::ListenerFailed {
            listener, error, ..
        } => style.red(&listener_failed_line(&scope, &listener.to_string(), error)),
        EnginePayload::ListeningChanged { host, on } => {
            style.bold(&listening_changed_line(&scope, &host.to_string(), *on))
        }
        // A divert moved the run; an act beside a live step is ambient.
        EnginePayload::BoundaryFired {
            step,
            boundary,
            diverts,
            ..
        } => {
            let line =
                boundary_fired_line(&scope, step.as_str(), boundary.as_str(), *diverts);
            if *diverts {
                style.bold(&line)
            } else {
                style.dim(&line)
            }
        }
        EnginePayload::FileChanged { path, kind, .. } => {
            style.dim(&format!("{scope}file {path} {kind:?}"))
        }
        EnginePayload::SettingsChanged { scope, keys, .. } => {
            style.dim(&bisa_i18n::say(&bisa_core::text!("cli-activity-settings-changed", scope = scope.to_string(), a0 = (keys.join(", ")).to_string())))
        }
        EnginePayload::Lsp {
            language, method, ..
        } => style.dim(&bisa_i18n::say(&bisa_core::text!("cli-activity-language-server", scope = scope.to_string(), language = language.to_string(), method = method.to_string()))),
        // A conversation's changes to review (ide/20). The count moving is
        // the review panel's; the feed says a word landed, and an ask.
        EnginePayload::ChangesMoved { .. } => return None,
        EnginePayload::ChangesSettled {
            act,
            files,
            skipped,
            ..
        } => {
            let left = if *skipped == 0 {
                String::new()
            } else {
                format!(", {skipped} left alone")
            };
            style.dim(&bisa_i18n::say(&bisa_core::text!("cli-activity-agent-changes-file-s", scope = scope.to_string(), act = act.to_string(), files = files.to_string(), left = left.to_string())))
        }
        EnginePayload::AskOpened { ask, .. } => style.bold(&bisa_i18n::say(&bisa_core::text!("cli-activity-asks", scope = scope.to_string(), a0 = (ask.agent).to_string(), a1 = (ask.subject.words()).to_string(), a2 = (short(&ask.question, 80)).to_string()))),
        EnginePayload::AskSettled { allowed, .. } => style.dim(&format!(
            "{scope}ask {}",
            if *allowed { "allowed" } else { "refused" }
        )),
        EnginePayload::McpProbed { id, ok } => style.dim(&bisa_i18n::say(&bisa_core::text!("cli-activity-mcp-server-probed", scope = scope.to_string(), id = id.to_string(), a0 = (if *ok { "answered" } else { "failed" }).to_string()))),
        EnginePayload::Paused => style.bold(&bisa_i18n::say(&bisa_core::text!("cli-activity-engine-paused"))),
        EnginePayload::Resumed => style.bold(&bisa_i18n::say(&bisa_core::text!("cli-activity-engine-resumed"))),
        EnginePayload::Session { event } => return session_line(style, &scope, event),
    };
    Some(line)
}

fn session_line(style: &Style, scope: &str, e: &SessionEvent) -> Option<String> {
    match e {
        SessionEvent::Lifecycle(l) => Some(match l {
            LifecycleEvent::Started => style.bold(&bisa_i18n::say(&bisa_core::text!(
                "cli-activity-session-started",
                scope = scope.to_string()
            ))),
            // A process-pid announcement is internal plumbing, not activity news.
            LifecycleEvent::ProcessStarted { .. } => return None,
            LifecycleEvent::Ended {
                outcome,
                is_terminal,
            } => {
                let o = match outcome {
                    Outcome::Completed => "completed".to_string(),
                    Outcome::Aborted => "aborted".to_string(),
                    Outcome::Failed { error } => {
                        return Some(style.red(&bisa_i18n::say(&bisa_core::text!(
                            "cli-activity-session-failed",
                            scope = scope.to_string(),
                            a0 = (short(error, 70)).to_string()
                        ))))
                    }
                    Outcome::ModelUnavailable {
                        model, retry_after, ..
                    } => {
                        let when = match retry_after {
                            Some(secs) => format!(" · retry in {secs}s"),
                            None => String::new(),
                        };
                        return Some(style.dim(&bisa_i18n::say(&bisa_core::text!(
                            "cli-activity-model-unavailable-trying-next-model",
                            scope = scope.to_string(),
                            model = model.to_string(),
                            when = when.to_string()
                        ))));
                    }
                    Outcome::Suspended { reason } => bisa_i18n::say(&bisa_core::text!(
                        "cli-activity-suspended",
                        a0 = (short(reason, 40)).to_string()
                    )),
                };
                if *is_terminal {
                    style.bold(&bisa_i18n::say(&bisa_core::text!(
                        "cli-activity-session-ended",
                        scope = scope.to_string(),
                        o = o.to_string()
                    )))
                } else {
                    style.dim(&bisa_i18n::say(&bisa_core::text!(
                        "cli-activity-turn-ended",
                        scope = scope.to_string(),
                        o = o.to_string()
                    )))
                }
            }
            LifecycleEvent::Parked => style.dim(&bisa_i18n::say(&bisa_core::text!(
                "cli-activity-session-parked",
                scope = scope.to_string()
            ))),
            LifecycleEvent::Revived => style.dim(&bisa_i18n::say(&bisa_core::text!(
                "cli-activity-session-revived",
                scope = scope.to_string()
            ))),
            // Work that stopped until somebody answers — the one line here
            // that is about a person rather than the agent.
            LifecycleEvent::InputRequested { request } => {
                style.bold(&bisa_i18n::say(&bisa_core::text!(
                    "cli-activity-asked-to",
                    scope = scope.to_string(),
                    what = short(&request.summary(), 70)
                )))
            }
            LifecycleEvent::InputResolved { .. } => style.dim(&bisa_i18n::say(&bisa_core::text!(
                "cli-activity-answered-going-on",
                scope = scope.to_string()
            ))),
        }),
        SessionEvent::Progress(p) => match p {
            ProgressEvent::ToolStarted {
                name, args_summary, ..
            } => Some(style.dim(&format!("{scope}→ {name} {}", short(args_summary, 60)))),
            ProgressEvent::ToolEnded { name, ok, .. } if !ok => {
                Some(style.red(&format!("{scope}✗ {name} failed")))
            }
            ProgressEvent::SubagentStarted {
                name, description, ..
            } => Some(style.dim(&bisa_i18n::say(&bisa_core::text!(
                "cli-activity-sub-agent",
                scope = scope.to_string(),
                name = name.to_string(),
                description = short(description, 50)
            )))),
            ProgressEvent::SubagentEnded { ok: false, .. } => Some(style.red(&bisa_i18n::say(
                &bisa_core::text!("cli-activity-sub-agent-failed", scope = scope.to_string()),
            ))),
            // A sub-agent's tools are its own business; only its failures show.
            ProgressEvent::Nested { event, .. } => match event.as_ref() {
                ProgressEvent::ToolEnded {
                    name, ok: false, ..
                } => Some(style.red(&format!("{scope}↳ ✗ {name} failed"))),
                _ => None,
            },
            ProgressEvent::CostDelta {
                input_tokens,
                output_tokens,
                ..
            } => Some(style.dim(&format!("{scope}· {} tokens", input_tokens + output_tokens))),
            _ => None, // TurnStarted/TurnEnded/TextDelta/successful ToolEnded: ambient noise
        },
        SessionEvent::Raw(_) => None,
    }
}

/// The engine event an SSE frame carries: the node's envelope is
/// `{stream, payload}`, and only the `engine` stream's payload is one.
pub fn engine_event(frame: &serde_json::Value) -> Option<&serde_json::Value> {
    if frame["stream"].as_str() == Some("engine") {
        Some(&frame["payload"])
    } else {
        None
    }
}

/// One fact of the activity feed — an engine payload as `GET /pulse` ships
/// it — in words; nothing for a kind this renderer has no line for.
pub fn fact_line(payload: &serde_json::Value) -> Option<String> {
    engine_line_json(&Style::plain(), &serde_json::json!({ "payload": payload }))
}

/// One SSE frame from the daemon → an optional activity line. The stream
/// carries a tagged envelope (`{stream, payload}`); engine frames render like
/// the in-process activity, conversation frames as one-line conversation
/// notices, and the rest — an inbox delta drives a counter, a system frame
/// says the stream lagged — are silent.
pub fn sse_line(style: &Style, frame: &serde_json::Value) -> Option<String> {
    match frame["stream"].as_str() {
        Some("engine") => engine_line_json(style, &frame["payload"]),
        Some("conversation") => {
            let p = &frame["payload"];
            if p["snapshot"].as_bool().unwrap_or(false) {
                return None;
            }
            let who = p["author"].as_str().unwrap_or("");
            let who = &who[..who.len().min(8)];
            Some(style.dim(&format!(
                "✉ {who} in {}: {}",
                short(p["scope"].as_str().unwrap_or(""), 12),
                short(p["snippet"].as_str().unwrap_or(""), 60)
            )))
        }
        Some(_) => None,
        // Unenveloped frames (embedded engine events) still render.
        None => engine_line_json(style, frame),
    }
}

/// One engine event *as JSON* → an optional activity line. Mirrors
/// [`engine_line`]; the engine's event enum is serialize-only, so daemon-fed
/// rendering works off the wire shape.
fn engine_line_json(style: &Style, e: &serde_json::Value) -> Option<String> {
    let scope = e["work_item"]
        .as_str()
        .map(|w| format!("[{}] ", &w[w.len().saturating_sub(6)..]))
        .unwrap_or_default();
    let p = &e["payload"];
    let s = |key: &str| p[key].as_str().unwrap_or("").to_string();
    let line = match p["type"].as_str()? {
        "scheduled" => style.bold(&bisa_i18n::say(&bisa_core::text!(
            "cli-activity-scheduled-onto-2",
            scope = scope.to_string(),
            a0 = (s("harness")).to_string()
        ))),
        "execution_ended" => style.bold(&bisa_i18n::say(&bisa_core::text!(
            "cli-activity-execution-ended-2",
            scope = scope.to_string(),
            a0 = (s("outcome")).to_string()
        ))),
        "gate_opened" => style.bold(&bisa_i18n::say(&bisa_core::text!(
            "cli-activity-waiting-on-you-question",
            scope = scope.to_string(),
            question = s("question")
        ))),
        "question_asked" => {
            let hint = if p["expects"]["kind"].as_str() == Some("answer") {
                bisa_i18n::say(&bisa_core::text!("cli-activity-reply-bisa-answer"))
            } else {
                String::new()
            };
            style.bold(&format!("{scope}? {}{hint}", s("text")))
        }
        "guided" => style.dim(&bisa_i18n::say(&bisa_core::text!(
            "cli-activity-workflow-agent",
            a0 = (s("phase")).to_string(),
            a1 = (s("status")).to_string(),
            a2 = (p["detail"]
                .as_str()
                .map(|d| format!(" — {d}"))
                .unwrap_or_default())
            .to_string()
        ))),
        "gate_decided" => style.bold(&format!(
            "{scope}{} gate {}",
            s("gate"),
            if p["approve"].as_bool().unwrap_or(false) {
                "approved"
            } else {
                "rejected"
            }
        )),
        "result_accepted" => style.bold(&bisa_i18n::say(&bisa_core::text!(
            "cli-activity-result-accepted",
            scope = scope.to_string()
        ))),
        "run_started" => style.bold(&bisa_i18n::say(&bisa_core::text!(
            "cli-activity-run-started-workflow-2",
            a0 = (s("run")).to_string(),
            a1 = (s("workflow")).to_string()
        ))),
        "run_queued" => style.dim(&bisa_i18n::say(&bisa_core::text!(
            "cli-activity-run-queued-workflow-position-2",
            a0 = (s("run")).to_string(),
            a1 = (s("workflow")).to_string(),
            a2 = (p["position"].as_u64().unwrap_or(1)).to_string()
        ))),
        "run_finished" => match s("outcome").as_str() {
            "failed" => style.red(&bisa_i18n::say(&bisa_core::text!(
                "cli-activity-run-finished-failed-2",
                a0 = (s("run")).to_string()
            ))),
            _ => style.bold(&bisa_i18n::say(&bisa_core::text!(
                "cli-activity-run-finished-done-2",
                a0 = (s("run")).to_string()
            ))),
        },
        "run_cancelled" => style.bold(&format!(
            "run {} {}",
            s("run"),
            p["cause"]["cause"].as_str().unwrap_or("cancelled")
        )),
        "step_changed" => match s("state").as_str() {
            "failed" => style.red(&bisa_i18n::say(&bisa_core::text!(
                "cli-activity-step-line-failed",
                scope = scope.to_string(),
                step = s("step"),
                kind = s("kind")
            ))),
            "done" => style.bold(&bisa_i18n::say(&bisa_core::text!(
                "cli-activity-step-line-done",
                scope = scope.to_string(),
                step = s("step"),
                kind = s("kind")
            ))),
            "waiting" => style.bold(&bisa_i18n::say(&bisa_core::text!(
                "cli-activity-step-line-waiting",
                scope = scope.to_string(),
                step = s("step"),
                kind = s("kind")
            ))),
            "diverted" => style.bold(&bisa_i18n::say(&bisa_core::text!(
                "cli-activity-step-line-diverted",
                scope = scope.to_string(),
                step = s("step"),
                kind = s("kind")
            ))),
            other => style.dim(&bisa_i18n::say(&bisa_core::text!(
                "cli-activity-step-2",
                scope = scope.to_string(),
                a0 = (s("step")).to_string(),
                a1 = (s("kind")).to_string(),
                other = other.to_string()
            ))),
        },
        "goal_closed" => style.bold(&bisa_i18n::say(&bisa_core::text!(
            "cli-activity-goal-closed",
            a0 = (p["reason"]["reason"].as_str().unwrap_or("closed")).to_string()
        ))),
        "workflow_proposed" => style.bold(&format!(
            "⏸ the Workflow Agent proposed workflow {} — adopt with: bisa approve <goal>",
            s("workflow")
        )),
        "workflow_changed" => style.dim(&bisa_i18n::say(&bisa_core::text!(
            "cli-activity-workflow-changed",
            a0 = (s("workflow")).to_string()
        ))),
        "signal_received" => style.dim(&signal_received_line(
            &scope,
            &s("signal"),
            p["listener"].as_str(),
            &s("source"),
        )),
        "listener_fired" => match p["outcome"]["outcome"].as_str()? {
            "started" => style.bold(&listener_fired_line(
                &scope,
                &s("listener"),
                &s("signal"),
                Fired::Started {
                    run: p["outcome"]["run"].as_str().unwrap_or(""),
                    goal: p["outcome"]["goal"].as_str(),
                },
            )),
            "skipped" => style.dim(&listener_fired_line(
                &scope,
                &s("listener"),
                &s("signal"),
                Fired::Skipped {
                    reason: p["outcome"]["reason"].as_str().unwrap_or(""),
                },
            )),
            _ => return None,
        },
        "listener_failed" => style.red(&listener_failed_line(&scope, &s("listener"), &s("error"))),
        "listening_changed" => style.bold(&listening_changed_line(
            &scope,
            &s("host"),
            p["on"].as_bool().unwrap_or(false),
        )),
        "boundary_fired" => {
            let diverts = p["diverts"].as_bool().unwrap_or(false);
            let line = boundary_fired_line(&scope, &s("step"), &s("boundary"), diverts);
            if diverts {
                style.bold(&line)
            } else {
                style.dim(&line)
            }
        }
        "paused" => style.bold(&bisa_i18n::say(&bisa_core::text!(
            "cli-activity-engine-paused"
        ))),
        "resumed" => style.bold(&bisa_i18n::say(&bisa_core::text!(
            "cli-activity-engine-resumed"
        ))),
        "session" => return session_line_json(style, &scope, &p["event"]),
        _ => return None,
    };
    Some(line)
}

fn session_line_json(style: &Style, scope: &str, e: &serde_json::Value) -> Option<String> {
    // SessionEvent is round-trippable — deserialize and reuse the exact
    // in-process renderer.
    let ev: SessionEvent = serde_json::from_value(e.clone()).ok()?;
    session_line(style, scope, &ev)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_core::{Branch, PlatformFilter, PrincipalId, RunId, SignalSource, StepId, WorkflowId};
    use serde_json::json;
    use std::collections::BTreeMap;

    fn kind(body: serde_json::Value) -> StepKind {
        serde_json::from_value(body).expect("a step kind")
    }

    /// A sentence the catalog has: a miss renders as the message's id.
    fn said(line: &str) {
        assert!(
            !line.is_empty() && !line.contains("cli-"),
            "a line the catalog does not say: {line:?}"
        );
    }

    #[test]
    fn every_kind_has_its_word_the_three_new_ones_included() {
        let kinds = [
            kind(json!({"kind": "start", "on": {"event": "manual"}})),
            kind(json!({"kind": "wait", "until": {"until": "release"}})),
            kind(json!({"kind": "emit", "signal": "report.ready"})),
            kind(json!({"kind": "end", "finish": "failed"})),
            kind(json!({"kind": "decide", "rules": [], "otherwise": "rest", "pick": "every"})),
            kind(json!({"kind": "if", "when": {"condition": "all", "of": []}})),
            kind(
                json!({"kind": "switch", "on": "{inputs.tier}", "cases": [], "otherwise": "rest"}),
            ),
            kind(json!({
                "kind": "judge", "state": "s", "instructions": "pick", "options": [],
                "otherwise": "rest"
            })),
            kind(json!({"kind": "parallel"})),
            kind(json!({"kind": "for_each", "items": "[]"})),
            kind(json!({"kind": "while", "when": {"condition": "all", "of": []}})),
            kind(json!({"kind": "agent", "instructions": "do it"})),
            kind(json!({"kind": "human", "prompt": "which?"})),
            kind(json!({"kind": "approval", "prompt": "ship?"})),
            kind(json!({"kind": "check", "check": {"check": "command", "command": "true"}})),
            kind(json!({"kind": "connector"})),
            kind(json!({"kind": "notify", "template": "done"})),
            kind(json!({"kind": "spawn", "statement_template": "a child"})),
        ];
        assert_eq!(kinds.len(), StepKind::NAMES.len());
        for (kind, name) in kinds.iter().zip(StepKind::NAMES) {
            assert_eq!(kind.as_str(), name, "the palette's order");
            let word = step_kind_label(kind);
            match name {
                "for_each" => assert_eq!(word, "for each"),
                _ => assert_eq!(word, name),
            }
        }
    }

    #[test]
    fn every_catch_says_what_it_holds_for() {
        let waits = [
            WaitFor::Delay {
                secs: ValueRef::Fixed(30),
            },
            WaitFor::Time {
                at: "{inputs.deadline}".into(),
            },
            WaitFor::Schedule {
                cron: ValueRef::Fixed("0 9 * * 1".into()),
                tz: None,
            },
            WaitFor::Signal {
                filter: SignalFilter {
                    name: "report.ready".into(),
                    fields: BTreeMap::new(),
                },
            },
            WaitFor::Message {
                filter: MessageFilter {
                    r#in: Some("support".into()),
                    ..MessageFilter::default()
                },
            },
            WaitFor::Project {
                filter: ProjectFilter::default(),
            },
            WaitFor::Run {
                filter: RunFilter {
                    workflow: None,
                    outcome: Some(RunEnd::Failed),
                },
            },
            WaitFor::Platform {
                filter: PlatformFilter {
                    topic: "goal.closed".into(),
                    fields: BTreeMap::new(),
                },
            },
            WaitFor::Release,
        ];
        assert_eq!(waits.len(), WaitFor::NAMES.len());
        let labels: Vec<String> = waits
            .iter()
            .zip(WaitFor::NAMES)
            .map(|(wait, name)| {
                assert_eq!(wait.as_str(), name, "the catches' order");
                let label = wait_label(wait);
                said(&label);
                label
            })
            .collect();
        assert_eq!(labels[0], "30s to pass");
        assert!(labels[1].contains("{inputs.deadline}"), "{}", labels[1]);
        assert!(labels[3].contains("report.ready"), "{}", labels[3]);
        assert_eq!(labels[4], "a message in support");
        assert!(labels[5].contains("not chosen yet"), "{}", labels[5]);
        assert_eq!(labels[6], "a run to fail");
        assert!(labels[7].contains("goal.closed"), "{}", labels[7]);
        assert_eq!(
            wait_label(&WaitFor::Message {
                filter: MessageFilter::default()
            }),
            "a message"
        );
    }

    #[test]
    fn a_boundary_event_says_what_it_hears_and_what_it_does() {
        let boundary = |body: serde_json::Value| -> Boundary {
            serde_json::from_value(body).expect("a boundary event")
        };
        let late = boundary(json!({
            "name": "late", "on": {"event": "after", "secs": 172800}, "act": "divert"
        }));
        let line = boundary_label(&late, 0);
        said(&line);
        assert!(
            line.contains("late") && line.contains("172800") && line.contains("diverts"),
            "{line}"
        );
        let nudge = boundary(json!({
            "name": "nudge", "on": {"event": "every", "secs": 86400, "max": 5},
            "act": "notify", "template": "still waiting"
        }));
        let line = boundary_label(&nudge, 2);
        said(&line);
        assert!(line.contains("nudge") && line.contains("2 of 5"), "{line}");
        let heads_up = boundary(json!({
            "name": "heads-up", "on": {"event": "signal", "name": "deploy.started"},
            "act": "emit", "signal": "review.waiting"
        }));
        let line = boundary_label(&heads_up, 0);
        assert!(
            line.contains("deploy.started") && line.contains("review.waiting"),
            "{line}"
        );
    }

    fn step_fact(event: StepFact) -> String {
        payload_line(&JournalPayload::Step {
            run: RunId::from_ulid(ulid::Ulid::from_parts(4, 1)),
            step: StepId::new("route").expect("a step id"),
            event,
        })
    }

    #[test]
    fn a_step_s_new_facts_are_said() {
        assert_eq!(
            step_fact(StepFact::Done { branches: vec![] }),
            "step route done"
        );
        assert_eq!(
            step_fact(StepFact::Done {
                branches: vec![
                    Branch::new("mail").expect("a branch"),
                    Branch::new("chat").expect("a branch")
                ],
            }),
            "step route done → mail · chat"
        );
        assert_eq!(
            step_fact(StepFact::Diverted {
                by: Branch::new("late").expect("a branch"),
            }),
            "step route diverted by late"
        );
        let acted = step_fact(StepFact::Boundary {
            boundary: Branch::new("nudge").expect("a branch"),
        });
        said(&acted);
        assert!(
            acted.contains("route") && acted.contains("nudge"),
            "{acted}"
        );
    }

    #[test]
    fn a_run_says_where_it_began_and_a_signal_what_it_was() {
        let run = RunId::from_ulid(ulid::Ulid::from_parts(4, 1));
        let workflow = WorkflowId::from_ulid(ulid::Ulid::from_parts(9, 1));
        let started = |start: Option<&str>, signal: Option<&str>| {
            payload_line(&JournalPayload::Run {
                run,
                event: RunFact::Started {
                    workflow,
                    revision: 3,
                    start: start.map(|s| StepId::new(s).expect("a step id")),
                    signal: signal.map(str::to_string),
                },
            })
        };
        let by_hand = started(None, None);
        assert_eq!(
            by_hand,
            format!("run {run} started on workflow {workflow} rev 3")
        );
        let by_event = started(Some("weekly"), Some("01SIGNAL"));
        assert_eq!(by_event, format!("{by_hand} at weekly on signal 01SIGNAL"));

        let listener: bisa_core::ListenerKey = format!("workspace:{workflow}/ticket")
            .parse()
            .expect("a listener");
        let heard = payload_line(&JournalPayload::Signal {
            signal: "01SIGNAL".into(),
            listener: Some(listener.clone()),
            source: SignalSource::Hook,
            name: None,
            payload: json!({"subject": "help"}),
        });
        said(&heard);
        assert!(
            heard.contains("01SIGNAL")
                && heard.contains("hook")
                && heard.contains(&listener.to_string())
                && heard.contains("subject"),
            "{heard}"
        );
        let raised = payload_line(&JournalPayload::Signal {
            signal: "01NAMED".into(),
            listener: None,
            source: SignalSource::Signal,
            name: Some("report.ready".into()),
            payload: json!({}),
        });
        said(&raised);
        assert!(raised.contains("signal report.ready"), "{raised}");
        assert!(
            !raised.contains('{'),
            "an empty payload is not said: {raised}"
        );
    }

    #[test]
    fn a_journal_line_carries_its_author_and_its_moment() {
        let event = JournalEvent {
            home: bisa_core::Home::Run {
                run: RunId::from_ulid(ulid::Ulid::from_parts(4, 1)),
            },
            author: PrincipalId::new("cd".repeat(32)).expect("a principal"),
            at: 7,
            payload: JournalPayload::Step {
                run: RunId::from_ulid(ulid::Ulid::from_parts(4, 1)),
                step: StepId::new("review").expect("a step id"),
                event: StepFact::Diverted {
                    by: Branch::new("late").expect("a branch"),
                },
            },
        };
        let line = journal_line(&event);
        assert!(
            line.contains("cdcdcdcd") && line.ends_with("diverted by late"),
            "{line}"
        );
    }

    /// The daemon's frames, as `GET /events` writes them.
    fn frame(payload: serde_json::Value) -> serde_json::Value {
        json!({"stream": "engine", "payload": {"payload": payload}})
    }

    #[test]
    fn the_daemon_s_frames_are_read_through_their_envelope() {
        let style = Style::plain();
        let started = frame(json!({
            "type": "listener_fired", "listener": "workspace:01WF/hourly", "signal": "01S",
            "outcome": {"outcome": "started", "run": "01RUN"}
        }));
        let line = sse_line(&style, &started).expect("a line");
        said(&line);
        assert!(
            line.contains("workspace:01WF/hourly")
                && line.contains("01RUN")
                && line.contains("workspace"),
            "{line}"
        );
        let on_goal = frame(json!({
            "type": "listener_fired", "listener": "goal:01G/ticket", "signal": "01S",
            "outcome": {"outcome": "started", "run": "01RUN", "goal": "01G"}
        }));
        let line = sse_line(&style, &on_goal).expect("a line");
        assert!(line.contains("on goal 01G"), "{line}");
        let skipped = frame(json!({
            "type": "listener_fired", "listener": "goal:01G/ticket", "signal": "01S",
            "outcome": {"outcome": "skipped", "reason": "its last run is unfinished"}
        }));
        let line = sse_line(&style, &skipped).expect("a line");
        assert!(line.contains("its last run is unfinished"), "{line}");

        for (payload, expect) in [
            (
                json!({"type": "signal_received", "signal": "01S", "listener": null,
                       "source": "signal"}),
                "01S",
            ),
            (
                json!({"type": "signal_received", "signal": "01S",
                       "listener": "goal:01G/ticket", "source": "hook"}),
                "goal:01G/ticket",
            ),
            (
                json!({"type": "listener_failed", "listener": "goal:01G/ticket",
                       "error": "not armed: input `who` holds nothing"}),
                "not armed",
            ),
            (
                json!({"type": "listening_changed", "host": "workspace:01WF", "on": true}),
                "workspace:01WF",
            ),
            (
                json!({"type": "boundary_fired", "run": "01RUN", "workflow": "01WF",
                       "step": "review", "boundary": "late", "diverts": true}),
                "late",
            ),
            (
                json!({"type": "step_changed", "run": "01RUN", "workflow": "01WF",
                       "step": "review", "state": "diverted", "kind": "approval"}),
                "diverted",
            ),
        ] {
            let line = sse_line(&style, &frame(payload.clone()))
                .unwrap_or_else(|| panic!("no line for {payload}"));
            said(&line);
            assert!(line.contains(expect), "{line}");
        }

        // Every other stream is silent, and the engine's event is the
        // envelope's payload.
        let inbox = json!({"stream": "inbox", "payload": {"key": "01G", "unread_count": 1}});
        assert_eq!(sse_line(&style, &inbox), None);
        assert_eq!(engine_event(&inbox), None);
        assert_eq!(
            engine_event(&started).map(|e| e["payload"]["type"].clone()),
            Some(json!("listener_fired"))
        );
        // A fact of the feed is the payload alone.
        let fact =
            fact_line(&json!({"type": "listening_changed", "host": "goal:01G", "on": false}))
                .expect("a line");
        assert!(fact.contains("goal:01G"), "{fact}");
        assert_eq!(fact_line(&json!({"type": "invented_later"})), None);
    }
}
