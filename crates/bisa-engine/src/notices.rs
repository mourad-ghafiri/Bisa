//! What concerns a person, decided once: a **notice** is a fact from the
//! activity index about a thing the person is tied to — a run that finished
//! or failed, a step that blocked, a budget spent, a design that stalled, a
//! listener that could not start its run, a committer wanted, a script that
//! failed, a pull request opened or merged, a command the guard refused, a
//! folder a step made, a workflow the Workflow Agent designed, proposed or
//! that was put away. The Inbox reads them beside what is owed; the Pulse keeps every
//! fact. Nothing is stored for this: a notice is the activity record the
//! feed already keeps, classified here.
//!
//! Two questions, one module: [`notice_of`] says whether a stored record is
//! a notice and which kind; [`target_of`] says which Inbox row a live event
//! moves — the same answer [`row_of`] gives for the stored form, so a frame
//! and a reload land on one row. A row is a thing, never an event:
//! [`InboxKind`] names the nine kinds of thing. A listener's trouble lands on
//! its host's row — the workflow's, or the goal's — never a row of its own.

use crate::events::{EngineEvent, EnginePayload, ExecutionOutcome};
use bisa_core::event::{GuardVerdict, GuidanceStatus};
use bisa_core::{ListenerHost, ListenerKey, ProjectOrigin, RunOutcome, WorkstreamState};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The kinds of thing an Inbox row is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InboxKind {
    Goal,
    Channel,
    /// A direct channel.
    Dm,
    /// A workstream: its scripts, its lifecycle on the code host.
    Workstream,
    /// A project: what was made for it and who commits in it.
    Project,
    /// A conversation: its thread named you or moved unread.
    Conversation,
    /// A workflow: what the Workflow Agent designed or proposed, its being
    /// put away, and a listener of it that could not start its run. A
    /// person's own save is no news to them. It has no conversation.
    Workflow,
    /// A person on another node (14-collaboration): asked to join, joined,
    /// left, or said something the classifier held. Keyed by pubkey.
    People,
    /// A harness a person opened in the IDE's terminal, waiting on them at
    /// its own prompt (ide/06 §Reporting). Keyed by the session; a row only
    /// while it waits.
    Session,
}

impl InboxKind {
    pub const ALL: &'static [InboxKind] = &[
        InboxKind::Goal,
        InboxKind::Channel,
        InboxKind::Dm,
        InboxKind::Workstream,
        InboxKind::Project,
        InboxKind::Conversation,
        InboxKind::Workflow,
        InboxKind::People,
        InboxKind::Session,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            InboxKind::Goal => "goal",
            InboxKind::Channel => "channel",
            InboxKind::Dm => "dm",
            InboxKind::Workstream => "workstream",
            InboxKind::Project => "project",
            InboxKind::Conversation => "conversation",
            InboxKind::Workflow => "workflow",
            InboxKind::People => "people",
            InboxKind::Session => "session",
        }
    }
}

/// The row a notice or a frame belongs to.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
pub struct InboxTarget {
    pub kind: InboxKind,
    pub id: String,
}

/// What kind of thing happened — the short, named list the Inbox reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NoticeKind {
    RunDone,
    RunFailed,
    /// A step failed or blocked.
    StepBlocked,
    /// A work item spent its budget or its wall clock.
    BudgetExhausted,
    /// The Workflow Agent stalled or could not start.
    DesignStalled,
    /// A listener could not be armed, or a signal of it could not start its
    /// run — on its host's row.
    ListenerFailed,
    CommitterNeeded,
    ScriptFailed,
    /// What a person approved at the Publish gate did not go out.
    PublishFailed,
    PrOpened,
    Merged,
    GuardRefused,
    /// A workflow step made a project.
    ProjectMade,
    /// Somebody claimed an invite under `collab.join = ask` and waits.
    JoinRequested,
    /// A person on another node was admitted.
    MemberJoined,
    /// A person on another node left or was removed.
    MemberLeft,
    /// A person's message is held from agents by the classifier.
    MessageHeld,
    /// The Workflow Agent wrote a workflow — a proposal recorded, an
    /// amendment applied alone. A person's own save is none.
    WorkflowDesigned,
    /// The Workflow Agent proposed a revision for a goal. The Adopt gate is
    /// the goal's ask; this is the workflow's story.
    WorkflowProposed,
    /// A workflow was put away — out of the library and the pickers. Taking
    /// it back out is none.
    WorkflowArchived,
}

impl NoticeKind {
    pub const ALL: &'static [NoticeKind] = &[
        NoticeKind::RunDone,
        NoticeKind::RunFailed,
        NoticeKind::StepBlocked,
        NoticeKind::BudgetExhausted,
        NoticeKind::DesignStalled,
        NoticeKind::ListenerFailed,
        NoticeKind::CommitterNeeded,
        NoticeKind::ScriptFailed,
        NoticeKind::PublishFailed,
        NoticeKind::PrOpened,
        NoticeKind::Merged,
        NoticeKind::GuardRefused,
        NoticeKind::ProjectMade,
        NoticeKind::JoinRequested,
        NoticeKind::MemberJoined,
        NoticeKind::MemberLeft,
        NoticeKind::MessageHeld,
        NoticeKind::WorkflowDesigned,
        NoticeKind::WorkflowProposed,
        NoticeKind::WorkflowArchived,
    ];

    /// A notice that reads as trouble — the tone a screen gives it.
    pub fn is_trouble(self) -> bool {
        matches!(
            self,
            NoticeKind::RunFailed
                | NoticeKind::StepBlocked
                | NoticeKind::BudgetExhausted
                | NoticeKind::DesignStalled
                | NoticeKind::ListenerFailed
                | NoticeKind::ScriptFailed
                | NoticeKind::PublishFailed
                | NoticeKind::GuardRefused
                | NoticeKind::MessageHeld
        )
    }
}

/// The activity tags a notice can be under — what the index is asked for.
/// A tag here is not a notice yet: [`notice_of`] reads the payload.
pub const NOTICE_TAGS: &[&str] = &[
    "run_finished",
    "step_changed",
    "execution_ended",
    "guided",
    "listener_failed",
    "committer_needed",
    "workstream_script_ran",
    "workstream_publish_failed",
    "workstream_changed",
    "guard_decided",
    "project_created",
    "people_changed",
    "invite_changed",
    "message_held",
    "workflow_changed",
    "workflow_proposed",
    "workflow_archived",
];

fn str_at<'a>(event: &'a Value, key: &str) -> Option<&'a str> {
    event.get(key).and_then(Value::as_str)
}

/// Whether a stored activity record is a notice, and which kind. `tag` is
/// the record's `kind` column; `event` the payload as it was recorded.
pub fn notice_of(tag: &str, event: &Value) -> Option<NoticeKind> {
    match tag {
        "run_finished" => match str_at(event, "outcome")? {
            "done" => Some(NoticeKind::RunDone),
            "failed" => Some(NoticeKind::RunFailed),
            _ => None,
        },
        "step_changed" => match str_at(event, "state")? {
            "failed" | "blocked" => Some(NoticeKind::StepBlocked),
            _ => None,
        },
        "execution_ended" => match event.get("outcome").and_then(|o| str_at(o, "outcome"))? {
            "budget_exhausted" | "wall_clock_exceeded" => Some(NoticeKind::BudgetExhausted),
            _ => None,
        },
        "guided" => match str_at(event, "status")? {
            "stalled" | "failed" => Some(NoticeKind::DesignStalled),
            _ => None,
        },
        "listener_failed" => Some(NoticeKind::ListenerFailed),
        "committer_needed" => Some(NoticeKind::CommitterNeeded),
        "workstream_script_ran" => match event.get("ok").and_then(Value::as_bool)? {
            false => Some(NoticeKind::ScriptFailed),
            true => None,
        },
        "workstream_publish_failed" => Some(NoticeKind::PublishFailed),
        "workstream_changed" => match event.get("state").and_then(|s| str_at(s, "state"))? {
            "pr_open" => Some(NoticeKind::PrOpened),
            "merged" => Some(NoticeKind::Merged),
            _ => None,
        },
        "guard_decided" => match str_at(event, "verdict")? {
            "denied" => Some(NoticeKind::GuardRefused),
            _ => None,
        },
        // A step's project: the goal's own design names its step, a library
        // workflow's step is the `step` origin itself (its ref flattened).
        "project_created" => {
            let origin = event.get("origin")?;
            let by_step = str_at(origin, "origin") == Some("step")
                || origin.get("step").is_some_and(|s| !s.is_null());
            by_step.then_some(NoticeKind::ProjectMade)
        }
        "people_changed" => match event.get("change").and_then(|c| str_at(c, "change"))? {
            "joined" => Some(NoticeKind::MemberJoined),
            "left" => Some(NoticeKind::MemberLeft),
            _ => None,
        },
        "invite_changed" => match event
            .get("invite")
            .and_then(|i| i.get("state"))
            .and_then(|st| str_at(st, "state"))?
        {
            "requested" => Some(NoticeKind::JoinRequested),
            _ => None,
        },
        "message_held" => match event.get("reason").and_then(|r| str_at(r, "reason"))? {
            "harmful" | "no_verdict" => Some(NoticeKind::MessageHeld),
            _ => None,
        },
        // The Workflow Agent's hand is news; a person's own save is not.
        "workflow_changed" => event
            .get("designed")
            .and_then(Value::as_bool)?
            .then_some(NoticeKind::WorkflowDesigned),
        "workflow_proposed" => Some(NoticeKind::WorkflowProposed),
        "workflow_archived" => event
            .get("archived")
            .and_then(Value::as_bool)?
            .then_some(NoticeKind::WorkflowArchived),
        _ => None,
    }
}

/// The row a stored notice belongs to: `source_kind`/`source_id` are the
/// record's, `event` its payload. A goal's facts land on the goal — and a run
/// of the workspace's, filed under its workflow, on the workflow; a committer
/// wanted, a script and a lifecycle on the workstream; a folder a step made
/// on the project; a listener's trouble on its host — the workflow, or the
/// goal; a workflow's facts on the workflow.
pub fn row_of(tag: &str, source_kind: &str, source_id: &str, event: &Value) -> Option<InboxTarget> {
    let target = |kind, id: &str| {
        Some(InboxTarget {
            kind,
            id: id.to_string(),
        })
    };
    match tag {
        "run_finished" | "step_changed" | "execution_ended" | "guided" | "guard_decided" => {
            match source_kind {
                "goal" => target(InboxKind::Goal, source_id),
                "workflow" => target(InboxKind::Workflow, source_id),
                _ => None,
            }
        }
        "committer_needed"
        | "workstream_script_ran"
        | "workstream_publish_failed"
        | "workstream_changed" => target(InboxKind::Workstream, str_at(event, "workstream")?),
        "project_created" => target(InboxKind::Project, str_at(event, "project")?),
        "listener_failed" => host_row(str_at(event, "listener")?.parse().ok()?),
        "workflow_changed" | "workflow_proposed" | "workflow_archived" => {
            target(InboxKind::Workflow, str_at(event, "workflow")?)
        }
        "people_changed" => target(InboxKind::People, str_at(event, "pubkey")?),
        "message_held" => target(InboxKind::People, str_at(event, "author")?),
        "invite_changed" => target(
            InboxKind::People,
            event
                .get("invite")
                .and_then(|i| i.get("state"))
                .and_then(|st| str_at(st, "by"))?,
        ),
        _ => None,
    }
}

/// The row a live event moves — the frame's key. The envelope's goal for a
/// goal's facts, and its workflow for a run of the workspace's, which no goal
/// holds; the thing a project, workstream, listener or workflow fact names;
/// and, whether or not it is a notice, the payloads that change what a goal
/// or a run owes (a gate, a question, a decision, a run's state). A proposal
/// moves the goal — its Adopt gate is what changed hands — and the
/// workflow's row learns of the notice on its next read.
pub fn target_of(event: &EngineEvent) -> Option<InboxTarget> {
    use EnginePayload as P;
    let goal = || {
        event
            .goal
            .map(|g| InboxTarget {
                kind: InboxKind::Goal,
                id: g.to_string(),
            })
            .or_else(|| {
                event.workflow.map(|w| InboxTarget {
                    kind: InboxKind::Workflow,
                    id: w.to_string(),
                })
            })
    };
    match &event.payload {
        P::GateOpened { .. }
        | P::QuestionAsked { .. }
        | P::GateDecided { .. }
        | P::RunStarted { .. }
        | P::RunQueued { .. }
        | P::RunFinished { .. }
        | P::RunCancelled { .. }
        | P::StepChanged { .. }
        | P::GoalClosed { .. }
        | P::WorkflowProposed { .. }
        | P::ExecutionEnded { .. }
        | P::Guided { .. }
        | P::GuardDecided { .. } => goal(),
        P::CommitterNeeded { workstream, .. }
        | P::WorkstreamScriptRan { workstream, .. }
        | P::WorkstreamPublishFailed { workstream, .. }
        | P::WorkstreamChanged { workstream, .. } => Some(InboxTarget {
            kind: InboxKind::Workstream,
            id: workstream.to_string(),
        }),
        P::ProjectCreated { project, .. } => Some(InboxTarget {
            kind: InboxKind::Project,
            id: project.to_string(),
        }),
        P::ListenerFailed { listener, .. } => host_row(listener.clone()),
        P::WorkflowChanged {
            workflow,
            designed: true,
            ..
        }
        | P::WorkflowArchived {
            workflow,
            archived: true,
        } => Some(InboxTarget {
            kind: InboxKind::Workflow,
            id: workflow.to_string(),
        }),
        P::PeopleChanged { pubkey, .. } | P::MessageHeld { author: pubkey, .. } => {
            Some(InboxTarget {
                kind: InboxKind::People,
                id: pubkey.as_hex().to_string(),
            })
        }
        P::InviteChanged { invite } => match &invite.state {
            bisa_core::InviteState::Requested { by, .. }
            | bisa_core::InviteState::Accepted { by, .. }
            | bisa_core::InviteState::Refused { by, .. } => Some(InboxTarget {
                kind: InboxKind::People,
                id: by.as_hex().to_string(),
            }),
            _ => None,
        },
        // A terminal harness's every state change moves its own row — the
        // row is there while it waits on the person and gone otherwise, and
        // the frame says which. An engine-driven session moves a goal's row
        // through its gate instead, never a row of its own.
        P::SessionState { live_run, presence }
            if presence.kind == crate::registry::SessionKind::Terminal =>
        {
            Some(InboxTarget {
                kind: InboxKind::Session,
                id: live_run.to_string(),
            })
        }
        _ => None,
    }
}

/// Whether a live payload is a notice — the typed twin of [`notice_of`],
/// so a frame can say `notice_count` moved without a round trip through JSON.
pub fn notice_of_payload(payload: &EnginePayload) -> Option<NoticeKind> {
    use EnginePayload as P;
    match payload {
        P::RunFinished { outcome, .. } => Some(match outcome {
            RunOutcome::Done => NoticeKind::RunDone,
            RunOutcome::Failed => NoticeKind::RunFailed,
        }),
        P::StepChanged { state, .. } if state == "failed" || state == "blocked" => {
            Some(NoticeKind::StepBlocked)
        }
        P::ExecutionEnded {
            outcome: ExecutionOutcome::BudgetExhausted | ExecutionOutcome::WallClockExceeded,
        } => Some(NoticeKind::BudgetExhausted),
        P::Guided {
            status: GuidanceStatus::Stalled | GuidanceStatus::Failed,
            ..
        } => Some(NoticeKind::DesignStalled),
        P::ListenerFailed { .. } => Some(NoticeKind::ListenerFailed),
        P::CommitterNeeded { .. } => Some(NoticeKind::CommitterNeeded),
        P::WorkstreamScriptRan { ok: false, .. } => Some(NoticeKind::ScriptFailed),
        P::WorkstreamPublishFailed { .. } => Some(NoticeKind::PublishFailed),
        P::WorkstreamChanged {
            state: WorkstreamState::PrOpen { .. },
            ..
        } => Some(NoticeKind::PrOpened),
        P::WorkstreamChanged {
            state: WorkstreamState::Merged { .. },
            ..
        } => Some(NoticeKind::Merged),
        P::GuardDecided {
            verdict: GuardVerdict::Denied,
            ..
        } => Some(NoticeKind::GuardRefused),
        P::ProjectCreated {
            origin: ProjectOrigin::Goal { step: Some(_), .. } | ProjectOrigin::Step { .. },
            ..
        } => Some(NoticeKind::ProjectMade),
        P::PeopleChanged {
            change: bisa_store::PeopleChange::Joined { .. },
            ..
        } => Some(NoticeKind::MemberJoined),
        P::PeopleChanged {
            change: bisa_store::PeopleChange::Left,
            ..
        } => Some(NoticeKind::MemberLeft),
        P::InviteChanged { invite } => match invite.state {
            bisa_core::InviteState::Requested { .. } => Some(NoticeKind::JoinRequested),
            _ => None,
        },
        P::MessageHeld {
            reason:
                bisa_store::HeldReason::Harmful { .. } | bisa_store::HeldReason::NoVerdict { .. },
            ..
        } => Some(NoticeKind::MessageHeld),
        P::WorkflowChanged { designed: true, .. } => Some(NoticeKind::WorkflowDesigned),
        P::WorkflowProposed { .. } => Some(NoticeKind::WorkflowProposed),
        P::WorkflowArchived { archived: true, .. } => Some(NoticeKind::WorkflowArchived),
        _ => None,
    }
}

/// A listener's row: its host's — the library workflow's, or the goal's.
fn host_row(listener: ListenerKey) -> Option<InboxTarget> {
    Some(match listener.host {
        ListenerHost::Workspace { workflow } => InboxTarget {
            kind: InboxKind::Workflow,
            id: workflow.to_string(),
        },
        ListenerHost::Goal { goal } => InboxTarget {
            kind: InboxKind::Goal,
            id: goal.to_string(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The typed and the stored classification agree: a payload recorded as
    /// JSON reads back as the same notice, or as none.
    fn agree(payload: EnginePayload, expect: Option<NoticeKind>) {
        let typed = notice_of_payload(&payload);
        let value = serde_json::to_value(&payload).unwrap();
        let tag = bisa_core::tag_of(&value);
        let stored = notice_of(&tag, &value);
        assert_eq!(typed, expect, "typed: {value}");
        assert_eq!(stored, expect, "stored: {value}");
    }

    #[test]
    fn a_notice_is_one_of_the_named_facts_and_nothing_else() {
        let run: bisa_core::RunId = "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap();
        let workflow: bisa_core::WorkflowId = "01DX5ZZKBKACTAV9WEVGEMMVRZ".parse().unwrap();
        agree(
            EnginePayload::RunFinished {
                run,
                workflow,
                outcome: RunOutcome::Done,
            },
            Some(NoticeKind::RunDone),
        );
        agree(
            EnginePayload::RunFinished {
                run,
                workflow,
                outcome: RunOutcome::Failed,
            },
            Some(NoticeKind::RunFailed),
        );
        agree(
            EnginePayload::StepChanged {
                run,
                workflow,
                step: "build".parse().unwrap(),
                state: "failed".into(),
                kind: "agent".into(),
            },
            Some(NoticeKind::StepBlocked),
        );
        agree(
            EnginePayload::StepChanged {
                run,
                workflow,
                step: "build".parse().unwrap(),
                state: "done".into(),
                kind: "agent".into(),
            },
            None,
        );
        agree(
            EnginePayload::ExecutionEnded {
                outcome: ExecutionOutcome::BudgetExhausted,
            },
            Some(NoticeKind::BudgetExhausted),
        );
        agree(
            EnginePayload::ExecutionEnded {
                outcome: ExecutionOutcome::Completed,
            },
            None,
        );
        agree(
            EnginePayload::Guided {
                phase: bisa_core::event::GuidancePhase::Design,
                status: GuidanceStatus::Stalled,
                detail: None,
                session: None,
            },
            Some(NoticeKind::DesignStalled),
        );
        agree(
            EnginePayload::Guided {
                phase: bisa_core::event::GuidancePhase::Design,
                status: GuidanceStatus::Working,
                detail: None,
                session: None,
            },
            None,
        );
        agree(
            EnginePayload::ListenerFailed {
                listener: format!("workspace:{workflow}/nightly").parse().unwrap(),
                signal: Some("s1".into()),
                error: "no".into(),
            },
            Some(NoticeKind::ListenerFailed),
        );
        agree(
            EnginePayload::ListenerFired {
                listener: format!("workspace:{workflow}/nightly").parse().unwrap(),
                signal: "s1".into(),
                outcome: crate::events::FiredOutcome::Started { run, goal: None },
            },
            None,
        );
    }

    #[test]
    fn a_workflows_facts_read_as_notices_only_from_the_agents_hand_or_a_putting_away() {
        let wf: bisa_core::WorkflowId = "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap();
        agree(
            EnginePayload::WorkflowChanged {
                workflow: wf,
                revision: 2,
                designed: true,
            },
            Some(NoticeKind::WorkflowDesigned),
        );
        agree(
            EnginePayload::WorkflowChanged {
                workflow: wf,
                revision: 2,
                designed: false,
            },
            None,
        );
        agree(
            EnginePayload::WorkflowProposed {
                workflow: wf,
                revision: 1,
                gate_id: None,
            },
            Some(NoticeKind::WorkflowProposed),
        );
        agree(
            EnginePayload::WorkflowArchived {
                workflow: wf,
                archived: true,
            },
            Some(NoticeKind::WorkflowArchived),
        );
        agree(
            EnginePayload::WorkflowArchived {
                workflow: wf,
                archived: false,
            },
            None,
        );
        agree(EnginePayload::WorkflowDeleted { workflow: wf }, None);
        assert_eq!(InboxKind::Workflow.as_str(), "workflow");
        assert!(InboxKind::ALL.contains(&InboxKind::Workflow));
        for kind in [
            NoticeKind::WorkflowDesigned,
            NoticeKind::WorkflowProposed,
            NoticeKind::WorkflowArchived,
        ] {
            assert!(NoticeKind::ALL.contains(&kind));
            assert!(!kind.is_trouble(), "{kind:?} is news, not trouble");
        }
    }

    /// What a person approved and did not go out is trouble on the
    /// workstream's own row, live and stored alike.
    #[test]
    fn an_approved_publish_that_failed_is_trouble_on_its_workstreams_row() {
        let wid: bisa_core::WorkstreamId = "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap();
        let pid: bisa_core::ProjectId = "01BX5ZZKBKACTAV9WEVGEMMVRZ".parse().unwrap();
        let goal: bisa_core::GoalId = "01CX5ZZKBKACTAV9WEVGEMMVRZ".parse().unwrap();
        let failed = EnginePayload::WorkstreamPublishFailed {
            workstream: wid,
            project: pid,
            what: "push work/checkout".into(),
            reason: "the remote refused".into(),
        };
        agree(failed.clone(), Some(NoticeKind::PublishFailed));
        assert!(NoticeKind::ALL.contains(&NoticeKind::PublishFailed));
        assert!(NoticeKind::PublishFailed.is_trouble());
        let row = Some(InboxTarget {
            kind: InboxKind::Workstream,
            id: wid.to_string(),
        });
        // Said on its goal's scope, it still lands on the workstream.
        assert_eq!(
            target_of(&EngineEvent::scoped(goal, None, failed.clone())),
            row
        );
        let stored = serde_json::to_value(&failed).unwrap();
        assert_eq!(
            row_of(
                "workstream_publish_failed",
                "workstream",
                &wid.to_string(),
                &stored
            ),
            row
        );
        assert_eq!(failed.topic(), "workstream.publish_failed");
        // And the Inbox asks the index for it: a fact classified and never
        // asked for is a notice nobody reads.
        assert!(NOTICE_TAGS.contains(&"workstream_publish_failed"));
        assert_eq!(
            notice_of("workstream_publish_failed", &stored),
            Some(NoticeKind::PublishFailed)
        );
    }

    #[test]
    fn the_projects_facts_read_as_notices_only_when_they_concern_a_person() {
        let wid: bisa_core::WorkstreamId = "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap();
        let pid: bisa_core::ProjectId = "01BX5ZZKBKACTAV9WEVGEMMVRZ".parse().unwrap();
        agree(
            EnginePayload::WorkstreamScriptRan {
                workstream: wid,
                project: pid,
                phase: crate::scripts::Phase::Clean,
                ok: false,
                output: "boom".into(),
            },
            Some(NoticeKind::ScriptFailed),
        );
        agree(
            EnginePayload::WorkstreamScriptRan {
                workstream: wid,
                project: pid,
                phase: crate::scripts::Phase::Clean,
                ok: true,
                output: String::new(),
            },
            None,
        );
        agree(
            EnginePayload::WorkstreamChanged {
                workstream: wid,
                state: WorkstreamState::Dirty,
            },
            None,
        );
        agree(
            EnginePayload::ProjectCreated {
                project: pid,
                slug: "site".into(),
                origin: ProjectOrigin::Workspace,
            },
            None,
        );
        agree(
            EnginePayload::GuardDecided {
                session: None,
                tool: "sh".into(),
                subject: "rm -rf /".into(),
                verdict: GuardVerdict::Denied,
                by: bisa_core::event::GuardJudge::Rule,
                rule: None,
                reason: None,
            },
            Some(NoticeKind::GuardRefused),
        );
        agree(
            EnginePayload::GuardDecided {
                session: None,
                tool: "sh".into(),
                subject: "ls".into(),
                verdict: GuardVerdict::Allowed,
                by: bisa_core::event::GuardJudge::Rule,
                rule: None,
                reason: None,
            },
            None,
        );
    }

    #[test]
    fn a_terminal_harness_moves_a_row_of_its_own_and_an_engine_session_none() {
        let presence = |kind: crate::registry::SessionKind| crate::presence::SessionPresence {
            id: crate::registry::LiveRunId::mint(),
            kind,
            origin: match kind {
                crate::registry::SessionKind::Terminal => bisa_core::SessionOrigin::Terminal,
                _ => bisa_core::SessionOrigin::Step {
                    step: None,
                    name: None,
                    resumed: false,
                },
            },
            state: crate::presence::SessionState::Waiting {
                on: crate::presence::WaitingOn::Permission {
                    tool: "Bash".into(),
                    gate_id: None,
                },
            },
            since: 1,
            harness: "claude-code".into(),
            model: None,
            effort: None,
            agent: None,
            session_id: None,
            work_item: None,
            conversation: None,
            goal: None,
            run: None,
            workstream: None,
            project: None,
            cwd: None,
            transcript_path: None,
            started: 1,
            pid: None,
            cost: Default::default(),
            children: vec![],
            last_activity: 1,
            revision: 1,
        };
        let terminal = presence(crate::registry::SessionKind::Terminal);
        let ev = EngineEvent::global(EnginePayload::SessionState {
            live_run: terminal.id,
            presence: Box::new(terminal.clone()),
        });
        assert_eq!(
            target_of(&ev),
            Some(InboxTarget {
                kind: InboxKind::Session,
                id: terminal.id.to_string()
            }),
            "a terminal harness's state is its own row's"
        );
        let worker = presence(crate::registry::SessionKind::Worker);
        let ev = EngineEvent::global(EnginePayload::SessionState {
            live_run: worker.id,
            presence: Box::new(worker),
        });
        assert_eq!(
            target_of(&ev),
            None,
            "an engine-driven session's wait is its goal's gate, not a row"
        );
        assert_eq!(InboxKind::Session.as_str(), "session");
        assert!(InboxKind::ALL.contains(&InboxKind::Session));
    }

    #[test]
    fn a_notice_lands_on_the_thing_it_concerns() {
        let goal: bisa_core::GoalId = "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap();
        let wid: bisa_core::WorkstreamId = "01BX5ZZKBKACTAV9WEVGEMMVRZ".parse().unwrap();
        let pid: bisa_core::ProjectId = "01CX5ZZKBKACTAV9WEVGEMMVRZ".parse().unwrap();
        let workflow: bisa_core::WorkflowId = "01DX5ZZKBKACTAV9WEVGEMMVRZ".parse().unwrap();
        let failed = EnginePayload::RunFinished {
            run: "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
            workflow,
            outcome: RunOutcome::Failed,
        };
        let on_goal = EngineEvent::scoped(goal, None, failed.clone());
        // A run of the workspace has no goal: its notice lands on its
        // workflow's row, live and stored alike.
        let in_the_workspace = crate::events::EventScope {
            goal: None,
            workflow: Some(workflow),
            run: None,
        }
        .event(None, failed);
        assert_eq!(
            target_of(&in_the_workspace),
            Some(InboxTarget {
                kind: InboxKind::Workflow,
                id: workflow.to_string()
            })
        );
        assert_eq!(
            row_of(
                "run_finished",
                "workflow",
                &workflow.to_string(),
                &json!({"outcome": "failed"})
            ),
            Some(InboxTarget {
                kind: InboxKind::Workflow,
                id: workflow.to_string()
            })
        );
        assert_eq!(
            target_of(&on_goal),
            Some(InboxTarget {
                kind: InboxKind::Goal,
                id: goal.to_string()
            })
        );
        assert_eq!(
            row_of(
                "run_finished",
                "goal",
                &goal.to_string(),
                &json!({"outcome": "failed"})
            ),
            Some(InboxTarget {
                kind: InboxKind::Goal,
                id: goal.to_string()
            })
        );
        assert_eq!(
            row_of("run_finished", "node", "", &json!({"outcome": "failed"})),
            None,
            "a run fact with no goal moves nothing"
        );
        let committer = EngineEvent::global(EnginePayload::CommitterNeeded {
            project: pid,
            slug: "site".into(),
            workstream: wid,
            reason: crate::identity::CommitterReason::Created,
            origin: ProjectOrigin::Workspace,
            global: None,
        });
        assert_eq!(
            target_of(&committer),
            Some(InboxTarget {
                kind: InboxKind::Workstream,
                id: wid.to_string()
            }),
            "the workstream where the answer is written, not the project"
        );
        assert_eq!(
            row_of(
                "committer_needed",
                "project",
                &pid.to_string(),
                &serde_json::to_value(&committer.payload).unwrap()
            )
            .map(|t| t.kind),
            Some(InboxKind::Workstream)
        );
        // A listener's trouble lands on its host: the workflow's row for the
        // workspace's listener, the goal's for a goal's — live and stored.
        let of_library: bisa_core::ListenerKey =
            format!("workspace:{workflow}/nightly").parse().unwrap();
        let failed_listener = EngineEvent::global(EnginePayload::ListenerFailed {
            listener: of_library.clone(),
            signal: None,
            error: "e".into(),
        });
        assert_eq!(
            target_of(&failed_listener),
            Some(InboxTarget {
                kind: InboxKind::Workflow,
                id: workflow.to_string()
            })
        );
        assert_eq!(
            row_of(
                "listener_failed",
                "workflow",
                &workflow.to_string(),
                &serde_json::to_value(&failed_listener.payload).unwrap()
            ),
            target_of(&failed_listener)
        );
        let of_goal: bisa_core::ListenerKey = format!("goal:{goal}/ticket").parse().unwrap();
        let goal_listener = EngineEvent::global(EnginePayload::ListenerFailed {
            listener: of_goal,
            signal: None,
            error: "e".into(),
        });
        assert_eq!(
            target_of(&goal_listener),
            Some(InboxTarget {
                kind: InboxKind::Goal,
                id: goal.to_string()
            })
        );
        assert!(!InboxKind::ALL.iter().any(|k| k.as_str() == "trigger"));
        let wf: bisa_core::WorkflowId = "01DX5ZZKBKACTAV9WEVGEMMVRZ".parse().unwrap();
        let designed = EngineEvent::global(EnginePayload::WorkflowChanged {
            workflow: wf,
            revision: 3,
            designed: true,
        });
        assert_eq!(
            target_of(&designed),
            Some(InboxTarget {
                kind: InboxKind::Workflow,
                id: wf.to_string()
            }),
            "the agent's design moves the workflow's row"
        );
        let saved = EngineEvent::global(EnginePayload::WorkflowChanged {
            workflow: wf,
            revision: 3,
            designed: false,
        });
        assert_eq!(target_of(&saved), None, "a person's own save moves no row");
        let proposed = EngineEvent::scoped(
            goal,
            None,
            EnginePayload::WorkflowProposed {
                workflow: wf,
                revision: 1,
                gate_id: Some("adopt:x".into()),
            },
        );
        assert_eq!(
            target_of(&proposed).map(|t| t.kind),
            Some(InboxKind::Goal),
            "a proposal's frame is the goal's: its Adopt gate changed hands"
        );
        assert_eq!(
            row_of(
                "workflow_proposed",
                "workflow",
                &wf.to_string(),
                &serde_json::to_value(&proposed.payload).unwrap()
            ),
            Some(InboxTarget {
                kind: InboxKind::Workflow,
                id: wf.to_string()
            }),
            "and the stored notice is the workflow's"
        );
        assert_eq!(
            row_of(
                "workflow_archived",
                "workflow",
                &wf.to_string(),
                &json!({"workflow": wf.to_string(), "archived": true})
            )
            .map(|t| t.kind),
            Some(InboxKind::Workflow)
        );
        let made = EngineEvent::global(EnginePayload::ProjectCreated {
            project: pid,
            slug: "site".into(),
            origin: ProjectOrigin::Workspace,
        });
        assert_eq!(target_of(&made).map(|t| t.kind), Some(InboxKind::Project));
        let token = EngineEvent::global(EnginePayload::AgentThinking {
            agent: "a".into(),
            scope: "s".into(),
        });
        assert_eq!(target_of(&token), None, "a streamed token moves no row");
        for kind in NoticeKind::ALL {
            let _ = kind.is_trouble();
        }
    }
}
