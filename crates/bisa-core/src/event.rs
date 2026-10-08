//! Journal events: the append-only facts that constitute a goal's history —
//! or, for a run of the workspace, the run's own ([`Home`]).
//!
//! Each `JournalEvent` maps 1:1 onto a signed Nostr event of the kind named by
//! [`GepEventKind`]; the store crate owns signing/envelope concerns, this
//! module owns the domain payloads.

use crate::ask::{Answer, AskKind};
use crate::attachment::AttachmentRef;
use crate::gate::{ApprovalId, Gate};
use crate::home::Home;
use crate::id::{PrincipalId, ProjectId, RunId, SessionId, WorkItemId, WorkflowId};
use crate::listen::{ListenerKey, SignalSource};
use crate::run::CancelCause;
use crate::workflow::{Branch, RunOutcome, StepId};
use serde::{Deserialize, Serialize};

/// Discriminant tying a journal payload to its wire kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GepEventKind {
    GoalNote,
    Decision,
    Claim,
    Progress,
    Result,
    /// A step or a run changed (kind:3411).
    Step,
    TurnMetrics,
    Signal,
}

impl GepEventKind {
    pub fn wire_kind(self) -> u16 {
        match self {
            GepEventKind::GoalNote => crate::kind::KIND_GOAL_NOTE,
            GepEventKind::Decision => crate::kind::KIND_DECISION,
            GepEventKind::Claim => crate::kind::KIND_CLAIM,
            GepEventKind::Progress => crate::kind::KIND_PROGRESS,
            GepEventKind::Result => crate::kind::KIND_RESULT,
            GepEventKind::Step => crate::kind::KIND_STEP,
            GepEventKind::TurnMetrics => crate::kind::KIND_TURN_METRICS,
            GepEventKind::Signal => crate::kind::KIND_SIGNAL,
        }
    }
}

/// A domain fact appended to a journal: a goal's, or a run of the
/// workspace's. `author` is the signer; agent authors additionally carry an
/// owner attestation at the envelope layer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct JournalEvent {
    /// Whose journal the fact is in — read back from the event's `a` tag.
    pub home: Home,
    pub author: PrincipalId,
    /// Unix seconds, assigned at the store edge.
    pub at: u64,
    pub payload: JournalPayload,
}

/// Derives `JsonSchema` because the Pulse ships journal payloads to the desktop
/// verbatim: a payload the wire types describe is a payload the desktop's
/// `switch` must handle to compile.
/// Which of the Workflow Agent's two jobs a guidance fact is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GuidancePhase {
    /// Propose the workflow for a goal that has none.
    Design,
    /// A run failed; propose the fix.
    Repair,
}

impl GuidancePhase {
    pub fn as_str(self) -> &'static str {
        match self {
            GuidancePhase::Design => "design",
            GuidancePhase::Repair => "repair",
        }
    }
}

/// Where the Workflow Agent stands on a guided goal. Every transition is one
/// journal fact and one bus event, written by a single funnel in the engine,
/// so a screen — and a restarted node — can tell *working* from *dead*.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GuidanceStatus {
    /// A wake is queued; it may sit here while the engine is paused.
    Scheduled,
    /// A session launched and is working; `session` names it.
    Working,
    /// It asked the person something and waits for the answer.
    Asking,
    /// A proposal landed and its gate opened.
    Proposed,
    /// It stopped without proposing: timed out, ended its turn, or was
    /// interrupted by a restart. `detail` says which.
    Stalled,
    /// It could not start — the agent could not be resolved or launched, or
    /// refused the prompt. `detail` says why.
    Failed,
    /// Designing is off on this node; nobody will design.
    Off,
}

impl GuidanceStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            GuidanceStatus::Scheduled => "scheduled",
            GuidanceStatus::Working => "working",
            GuidanceStatus::Asking => "asking",
            GuidanceStatus::Proposed => "proposed",
            GuidanceStatus::Stalled => "stalled",
            GuidanceStatus::Failed => "failed",
            GuidanceStatus::Off => "off",
        }
    }

    /// A status that claims somebody is at work right now — the ones a
    /// restart turns into `Stalled`, because the work died with the process.
    pub fn is_live(self) -> bool {
        matches!(self, GuidanceStatus::Scheduled | GuidanceStatus::Working)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum JournalPayload {
    /// Clarification or revision rationale (kind:3400).
    Note { text: String },
    /// The Workflow Agent's standing on a guided goal (kind:3400): which job
    /// it is on and where it stands, so the design of a goal's workflow is
    /// never a silence — a screen reads the last of these, and a restarted
    /// node knows what was in flight.
    Guidance {
        phase: GuidancePhase,
        status: GuidanceStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session: Option<SessionId>,
    },
    /// The Tool & Commands Guard judged a tool call in this journal's goal or
    /// run of the workspace (kind:3400):
    /// what was asked, what was decided and by whom — a rule, the classifier
    /// or a person. The subject is the **redacted** command or input; a
    /// journal never carries a secret.
    Guard {
        tool: String,
        subject: String,
        verdict: GuardVerdict,
        by: GuardJudge,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rule: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// The Decision-Making Agent judged something in this journal's goal or
    /// run of the workspace (kind:3400): which decision point asked, which
    /// provider and model answered, the questions as they were asked
    /// (redacted), the answers and what became of them.
    /// Not a [`JournalPayload::Decision`]: a judgement never signs a gate.
    Judgement {
        judgement: crate::decision::Judgement,
        /// The run and the step it was asked for, when it was asked for one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step: Option<StepId>,
    },
    /// Signed gate decision (kind:3401).
    Decision {
        gate: Gate,
        approve: bool,
        /// What is being decided: `approval:<run>/<step>`, `adopt:<workflow>@<rev>`,
        /// `amend:<run>@<rev>`, a result event id, an escalation id.
        subject: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rationale: Option<String>,
        /// What the human said, when the gate was a question.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        answer: Option<Answer>,
    },
    /// A question raised for a human, awaiting a decision or an answer.
    /// `expects` carries the options the asker offered, so the journal — not a
    /// live gate — is what a restarted daemon re-renders the question from.
    Question {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        work_item: Option<WorkItemId>,
        gate: String,
        text: String,
        expects: AskKind,
    },
    /// A question taken back before anyone answered — withdrawn by the
    /// process that asked, or found by a restart with the asker dead. The
    /// subject is the question's; a restarted daemon rebuilds no ask that a
    /// later `Withdrawn` names.
    Withdrawn { subject: String, reason: String },
    /// Work-item claim (kind:3402).
    Claim {
        work_item: WorkItemId,
        harness: String,
        session: SessionId,
    },
    /// Coarse progress (kind:3403) — the activity triple.
    Progress {
        work_item: WorkItemId,
        verb: String,
        object: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        outcome: Option<String>,
    },
    /// Structured result (kind:3404), already validated against the work
    /// item's schema.
    Result {
        work_item: WorkItemId,
        output: serde_json::Value,
        #[serde(default)]
        artifacts: Vec<String>,
    },
    /// One step of a run changed (kind:3411). The run snapshot is the state;
    /// this is the rationale-bearing fact.
    Step {
        run: RunId,
        step: StepId,
        /// Nested rather than flattened: a flattened tagged enum inside a
        /// tagged enum is a schema the desktop's type generator cannot read
        /// (it keeps the inner union and drops `type`, `run` and `step`), so
        /// the fact travels as `event: { fact: "done", … }`.
        event: StepFact,
    },
    /// The run itself was queued, started, amended, finished or cancelled
    /// (kind:3411).
    Run {
        run: RunId,
        /// Nested for the same reason as [`Self::Step`]'s.
        event: RunFact,
    },
    /// A project was attached to (or detached from) this goal. The whole of
    /// the Goal ⇄ Project relation is this fact and its author: it syncs with
    /// the journal and the `goal_projects` cache is rebuilt from it.
    Attachment { project: ProjectId, attached: bool },
    /// A file a person gave the goal as context — a brief, a spec, a
    /// screenshot — kept under the goal's `documents/` folder. The fact
    /// carries the attachment descriptor: the bytes are content-addressed
    /// and reach a peer by hash on demand, and the folder is materialised
    /// from the facts wherever the bytes are held (kind:3400).
    Document { file: AttachmentRef },
    /// The occurrence that started a run (kind:3410), on the run's home: the
    /// signal's id, the listener it was raised for, what kind of event it
    /// was, its name when it had one, and what it carried.
    Signal {
        signal: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        listener: Option<ListenerKey>,
        source: SignalSource,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default)]
        payload: serde_json::Value,
    },
    /// Per-turn cost accounting (kind:3406, encrypted at the envelope layer).
    TurnMetrics {
        session: SessionId,
        input_tokens: u64,
        output_tokens: u64,
        usd_cents: u64,
    },
}

/// How the guard's judgement went.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GuardVerdict {
    /// It ran (or may run) — with a restored input when a placeholder was in it.
    Allowed,
    /// Refused; the agent heard the reason.
    Denied,
    /// Put to a person, in the Inbox.
    Asked,
}

impl GuardVerdict {
    pub fn as_str(self) -> &'static str {
        match self {
            GuardVerdict::Allowed => "allowed",
            GuardVerdict::Denied => "denied",
            GuardVerdict::Asked => "asked",
        }
    }
}

/// Who decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GuardJudge {
    /// A guard rule matched — a built-in or a person's.
    Rule,
    /// The classifier read the redacted call.
    Classifier,
    /// A person answered in the Inbox.
    Person,
    /// The content screen read what an agent was about to read from outside
    /// — a page, a review — through the classifier.
    Content,
}

impl GuardJudge {
    pub fn as_str(self) -> &'static str {
        match self {
            GuardJudge::Rule => "rule",
            GuardJudge::Classifier => "classifier",
            GuardJudge::Person => "person",
            GuardJudge::Content => "content",
        }
    }
}

/// What happened to a step.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "fact")]
pub enum StepFact {
    Started {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        work_item: Option<WorkItemId>,
    },
    Waiting,
    /// Finished, naming the branches a gateway chose.
    Done {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        branches: Vec<Branch>,
    },
    Failed {
        error: String,
    },
    /// Stopped by a boundary event that diverts.
    Diverted {
        by: Branch,
    },
    /// A boundary event that does not divert acted beside the live step: a
    /// post, or a signal.
    Boundary {
        boundary: Branch,
    },
    Answered {
        answer: Answer,
    },
    Decided {
        approve: bool,
        approval: ApprovalId,
    },
    Skipped,
    Cancelled,
}

impl StepFact {
    pub fn as_str(&self) -> &'static str {
        match self {
            StepFact::Started { .. } => "started",
            StepFact::Waiting => "waiting",
            StepFact::Done { .. } => "done",
            StepFact::Failed { .. } => "failed",
            StepFact::Diverted { .. } => "diverted",
            StepFact::Boundary { .. } => "boundary",
            StepFact::Answered { .. } => "answered",
            StepFact::Decided { .. } => "decided",
            StepFact::Skipped => "skipped",
            StepFact::Cancelled => "cancelled",
        }
    }
}

/// What happened to a run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "fact")]
pub enum RunFact {
    /// Made behind its goal's live run; it starts when its turn comes.
    Queued {
        workflow: WorkflowId,
        revision: u64,
    },
    /// Started — at once when made on an idle goal or in the workspace, or
    /// from a goal's queue — at `start`, on the signal `signal` when an event
    /// began it.
    Started {
        workflow: WorkflowId,
        revision: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start: Option<StepId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signal: Option<String>,
    },
    Amended {
        revision: u64,
    },
    Finished {
        outcome: RunOutcome,
    },
    Cancelled {
        cause: CancelCause,
    },
}

impl RunFact {
    pub fn as_str(&self) -> &'static str {
        match self {
            RunFact::Queued { .. } => "queued",
            RunFact::Started { .. } => "started",
            RunFact::Amended { .. } => "amended",
            RunFact::Finished { .. } => "finished",
            RunFact::Cancelled { .. } => "cancelled",
        }
    }
}

impl JournalPayload {
    /// The wire kind this payload serializes to. `Question` and `Attachment`
    /// ride on the note kind; `Step` and `Run` share the step kind.
    pub fn event_kind(&self) -> GepEventKind {
        match self {
            JournalPayload::Note { .. }
            | JournalPayload::Guidance { .. }
            | JournalPayload::Guard { .. }
            | JournalPayload::Judgement { .. }
            | JournalPayload::Question { .. }
            | JournalPayload::Withdrawn { .. }
            | JournalPayload::Attachment { .. }
            | JournalPayload::Document { .. } => GepEventKind::GoalNote,
            JournalPayload::Decision { .. } => GepEventKind::Decision,
            JournalPayload::Claim { .. } => GepEventKind::Claim,
            JournalPayload::Progress { .. } => GepEventKind::Progress,
            JournalPayload::Result { .. } => GepEventKind::Result,
            JournalPayload::Step { .. } | JournalPayload::Run { .. } => GepEventKind::Step,
            JournalPayload::TurnMetrics { .. } => GepEventKind::TurnMetrics,
            JournalPayload::Signal { .. } => GepEventKind::Signal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_kinds_map_to_registered_wire_kinds() {
        let payloads = [
            JournalPayload::Note { text: "x".into() },
            JournalPayload::Guidance {
                phase: GuidancePhase::Design,
                status: GuidanceStatus::Working,
                detail: None,
                session: None,
            },
            JournalPayload::Decision {
                gate: Gate::Approval,
                approve: true,
                subject: "s".into(),
                rationale: None,
                answer: None,
            },
            JournalPayload::Guard {
                tool: "Bash".into(),
                subject: "sudo ls".into(),
                verdict: GuardVerdict::Denied,
                by: GuardJudge::Rule,
                rule: Some("privilege_escalation".into()),
                reason: None,
            },
            JournalPayload::Judgement {
                judgement: crate::decision::Judgement {
                    point: crate::decision::DecisionPoint::AssignPick,
                    provider: crate::decision::DecisionProviderKind::Harness,
                    model: "claude-sonnet-5".into(),
                    calibrated: false,
                    questions: Default::default(),
                    answers: Default::default(),
                    outcome: crate::decision::JudgementOutcome::Failed,
                    reason: Some("no answer".into()),
                    latency_ms: 12,
                    usage: Default::default(),
                },
                run: None,
                step: None,
            },
            JournalPayload::Progress {
                work_item: WorkItemId::from_ulid(ulid::Ulid::from_parts(1, 1)),
                verb: "edited".into(),
                object: "main.rs".into(),
                outcome: None,
            },
            JournalPayload::Step {
                run: RunId::from_ulid(ulid::Ulid::from_parts(4, 1)),
                step: StepId::new("build").unwrap(),
                event: StepFact::Done { branches: vec![] },
            },
            JournalPayload::Signal {
                signal: "01SIGNAL".into(),
                listener: None,
                source: SignalSource::Schedule,
                name: None,
                payload: serde_json::json!({"at": 1}),
            },
            JournalPayload::Run {
                run: RunId::from_ulid(ulid::Ulid::from_parts(4, 1)),
                event: RunFact::Finished {
                    outcome: RunOutcome::Done,
                },
            },
        ];
        for p in payloads {
            assert!(crate::kind::is_gep_kind(p.event_kind().wire_kind()));
        }
    }

    #[test]
    fn journal_event_roundtrip() {
        let ev = JournalEvent {
            home: Home::Run {
                run: RunId::from_ulid(ulid::Ulid::from_parts(4, 1)),
            },
            author: PrincipalId::new("cd".repeat(32)).unwrap(),
            at: 1,
            payload: JournalPayload::Step {
                run: RunId::from_ulid(ulid::Ulid::from_parts(4, 1)),
                step: StepId::new("review").unwrap(),
                event: StepFact::Decided {
                    approve: false,
                    approval: ApprovalId("dec".into()),
                },
            },
        };
        let json = serde_json::to_value(&ev).unwrap();
        assert_eq!(
            json["home"]["home"], "run",
            "a run of the workspace's own journal"
        );
        assert_eq!(json["payload"]["type"], "step");
        assert_eq!(json["payload"]["step"], "review");
        // The fact is nested under `event`, never flattened into the payload:
        // a flattened tagged enum inside a tagged enum is a schema the
        // desktop's type generator cannot read (see `JournalPayload::Step`).
        assert_eq!(json["payload"]["event"]["fact"], "decided");
        assert_eq!(json["payload"]["event"]["approve"], false);
        assert_eq!(
            json["payload"]["fact"],
            serde_json::Value::Null,
            "not flattened"
        );
        assert_eq!(serde_json::from_value::<JournalEvent>(json).unwrap(), ev);
    }

    #[test]
    fn every_fact_and_status_word_is_its_wire_tag() {
        let steps = [
            StepFact::Started { work_item: None },
            StepFact::Waiting,
            StepFact::Done { branches: vec![] },
            StepFact::Done {
                branches: vec![Branch::new("mail").unwrap(), Branch::new("chat").unwrap()],
            },
            StepFact::Failed {
                error: "red".into(),
            },
            StepFact::Diverted {
                by: Branch::new("late").unwrap(),
            },
            StepFact::Boundary {
                boundary: Branch::new("nudge").unwrap(),
            },
            StepFact::Answered {
                answer: crate::ask::Answer::text("yes"),
            },
            StepFact::Decided {
                approve: true,
                approval: ApprovalId("a".into()),
            },
            StepFact::Skipped,
            StepFact::Cancelled,
        ];
        for f in &steps {
            let json = serde_json::to_value(f).unwrap();
            assert_eq!(json["fact"], f.as_str(), "{f:?}");
            assert_eq!(serde_json::from_value::<StepFact>(json).unwrap(), *f);
        }
        let runs = [
            RunFact::Queued {
                workflow: crate::id::WorkflowId::from_ulid(ulid::Ulid::from_parts(9, 1)),
                revision: 3,
            },
            RunFact::Started {
                workflow: crate::id::WorkflowId::from_ulid(ulid::Ulid::from_parts(9, 1)),
                revision: 3,
                start: Some(StepId::new("weekly").unwrap()),
                signal: Some("01SIGNAL".into()),
            },
            RunFact::Amended { revision: 4 },
            RunFact::Finished {
                outcome: RunOutcome::Failed,
            },
            RunFact::Cancelled {
                cause: CancelCause::Stopped {
                    rationale: Some("enough".into()),
                },
            },
            RunFact::Cancelled {
                cause: CancelCause::Closed {
                    reason: crate::goal::ClosureReason::Abandoned { rationale: None },
                },
            },
        ];
        for f in &runs {
            let json = serde_json::to_value(f).unwrap();
            assert_eq!(json["fact"], f.as_str(), "{f:?}");
            assert_eq!(serde_json::from_value::<RunFact>(json).unwrap(), *f);
        }
        let json = serde_json::to_value(&runs[5]).unwrap();
        let old_signal = serde_json::json!({
            "type": "signal", "signal": "01S", "trigger": "01T", "topic": "webhook", "payload": {}
        });
        assert!(
            serde_json::from_value::<JournalPayload>(old_signal).is_err(),
            "a signal names its source, never a trigger"
        );
        assert_eq!(
            json["cause"]["cause"], "closed",
            "the cause nests under the fact"
        );
        assert_eq!(json["cause"]["reason"]["reason"], "abandoned");
        for s in [
            GuidanceStatus::Scheduled,
            GuidanceStatus::Working,
            GuidanceStatus::Asking,
            GuidanceStatus::Proposed,
            GuidanceStatus::Stalled,
            GuidanceStatus::Failed,
            GuidanceStatus::Off,
        ] {
            assert_eq!(serde_json::to_value(s).unwrap(), s.as_str());
            assert_eq!(
                s.is_live(),
                matches!(s, GuidanceStatus::Scheduled | GuidanceStatus::Working),
                "{s:?}"
            );
        }
        for p in [GuidancePhase::Design, GuidancePhase::Repair] {
            assert_eq!(serde_json::to_value(p).unwrap(), p.as_str());
        }
    }

    #[test]
    fn wire_kinds_are_distinct_and_a_step_and_a_run_share_one() {
        let kinds = [
            GepEventKind::GoalNote,
            GepEventKind::Decision,
            GepEventKind::Claim,
            GepEventKind::Progress,
            GepEventKind::Result,
            GepEventKind::Step,
            GepEventKind::TurnMetrics,
            GepEventKind::Signal,
        ];
        let mut wire: Vec<u16> = kinds.iter().map(|k| k.wire_kind()).collect();
        wire.sort_unstable();
        wire.dedup();
        assert_eq!(wire.len(), kinds.len(), "one number per kind");
        for k in kinds {
            assert!(
                crate::kind::is_gep_kind(k.wire_kind()),
                "{k:?} is a registered GEP kind"
            );
        }
        let run = JournalPayload::Run {
            run: RunId::from_ulid(ulid::Ulid::from_parts(4, 1)),
            event: RunFact::Amended { revision: 2 },
        };
        assert_eq!(run.event_kind(), GepEventKind::Step);
        assert_eq!(run.event_kind().wire_kind(), crate::kind::KIND_STEP);
    }

    // added by the coverage pass: b5-event.rs
    #[test]
    fn a_verdict_and_a_judge_each_have_their_wire_word() {
        assert_eq!(
            [
                GuardVerdict::Allowed,
                GuardVerdict::Denied,
                GuardVerdict::Asked
            ]
            .map(GuardVerdict::as_str),
            ["allowed", "denied", "asked"]
        );
        assert_eq!(
            [
                GuardJudge::Rule,
                GuardJudge::Classifier,
                GuardJudge::Person,
                GuardJudge::Content
            ]
            .map(GuardJudge::as_str),
            ["rule", "classifier", "person", "content"]
        );
    }

    // added by the coverage pass: b7-event.rs
    #[test]
    fn a_claim_a_result_and_the_turn_metrics_each_ride_their_own_kind() {
        let work_item = WorkItemId::from_ulid(ulid::Ulid::from_parts(1, 1));
        let session = SessionId::from_ulid(ulid::Ulid::from_parts(2, 1));
        assert_eq!(
            JournalPayload::Claim {
                work_item,
                harness: "claude-code".into(),
                session,
            }
            .event_kind(),
            GepEventKind::Claim
        );
        assert_eq!(
            JournalPayload::Result {
                work_item,
                output: serde_json::json!({"ok": true}),
                artifacts: vec![],
            }
            .event_kind(),
            GepEventKind::Result
        );
        assert_eq!(
            JournalPayload::TurnMetrics {
                session,
                input_tokens: 1,
                output_tokens: 2,
                usd_cents: 3,
            }
            .event_kind(),
            GepEventKind::TurnMetrics
        );
    }
}
