//! The engine's facts into the activity feed (the Pulse): **one concept per
//! payload**, decided here because the engine owns [`EnginePayload`], and
//! recorded through the store **before** the frame goes out, so a client
//! that reads the feed again on the frame finds the row.
//!
//! A payload with no concept is not activity — a per-frame or per-token
//! signal (`Session`, `SessionState`, `FileChanged`, `Lsp`, …) whose home is
//! the roster or the watcher, never a feed. The match is exhaustive: a
//! variant added to the enum stops this compiling until it is placed.

use crate::events::{EngineEvent, EnginePayload};
use crate::Inner;
use bisa_core::{ActivityConcept, ActivityFact, ActivitySource, ActivitySourceKind, ListenerHost};
use std::time::{SystemTime, UNIX_EPOCH};

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The concept an engine payload belongs to, or `None` for one that is not
/// activity.
pub fn concept_of(payload: &EnginePayload) -> Option<ActivityConcept> {
    use ActivityConcept as C;
    Some(match payload {
        EnginePayload::Session { .. }
        | EnginePayload::SessionState { .. }
        | EnginePayload::SessionGone { .. }
        | EnginePayload::AgentThinking { .. }
        | EnginePayload::AgentStreamed { .. }
        | EnginePayload::FileChanged { .. }
        | EnginePayload::BrowserRequest { .. }
        | EnginePayload::DrawingRequest { .. }
        | EnginePayload::MobileDevelopmentChanged { .. }
        | EnginePayload::McpProbed { .. }
        | EnginePayload::ChangesMoved { .. }
        | EnginePayload::AskOpened { .. }
        | EnginePayload::AskSettled { .. }
        | EnginePayload::Lsp { .. } => return None,
        EnginePayload::GoalCreated { .. }
        | EnginePayload::NoteChanged { .. }
        | EnginePayload::DrawingChanged { .. } => C::Workspace,
        EnginePayload::GateOpened { .. }
        | EnginePayload::QuestionAsked { .. }
        | EnginePayload::GateDecided { .. }
        | EnginePayload::ResultAccepted
        | EnginePayload::RunStarted { .. }
        | EnginePayload::RunQueued { .. }
        | EnginePayload::RunFinished { .. }
        | EnginePayload::RunCancelled { .. }
        | EnginePayload::StepChanged { .. }
        | EnginePayload::DocumentAdded { .. }
        | EnginePayload::GoalArchived { .. }
        | EnginePayload::GoalDeleted { .. }
        | EnginePayload::GoalClosed { .. }
        | EnginePayload::BoundaryFired { .. } => C::Goals,
        EnginePayload::Guided { .. }
        | EnginePayload::WorkflowProposed { .. }
        | EnginePayload::WorkflowChanged { .. }
        | EnginePayload::WorkflowDeleted { .. }
        | EnginePayload::WorkflowArchived { .. } => C::Workflows,
        EnginePayload::ProjectCreated { .. }
        | EnginePayload::ProjectChanged { .. }
        | EnginePayload::CommitterNeeded { .. }
        | EnginePayload::CommitterSet { .. }
        | EnginePayload::WorkstreamOpened { .. }
        | EnginePayload::WorkstreamEdited { .. }
        | EnginePayload::WorkstreamChanged { .. }
        | EnginePayload::WorkstreamCommitted { .. }
        | EnginePayload::WorkstreamScriptRan { .. }
        | EnginePayload::WorkstreamPublishFailed { .. }
        | EnginePayload::ServerChanged { .. }
        | EnginePayload::ProjectArchived { .. }
        | EnginePayload::ProjectDeleted { .. }
        | EnginePayload::ChangesSettled { .. }
        | EnginePayload::AttachmentChanged { .. } => C::Projects,
        EnginePayload::Scheduled { .. }
        | EnginePayload::ExecutionEnded { .. }
        | EnginePayload::AgentReplied { .. }
        | EnginePayload::ModelSwitched { .. }
        | EnginePayload::GuardDecided { .. }
        | EnginePayload::Judged { .. }
        | EnginePayload::ContentScreened { .. }
        | EnginePayload::Redacted { .. } => C::Agents,
        // What a listener heard and did is its host's story: a library
        // workflow's, or a goal's; a named signal no listener was named for
        // is the workflows'.
        EnginePayload::SignalReceived { listener, .. } => match listener {
            Some(key) => host_concept(&key.host),
            None => C::Workflows,
        },
        EnginePayload::ListenerFired { listener, .. }
        | EnginePayload::ListenerFailed { listener, .. } => host_concept(&listener.host),
        EnginePayload::ListeningChanged { host, .. } => host_concept(host),
        EnginePayload::ConversationCreated { .. } | EnginePayload::ConversationChanged { .. } => {
            C::Channels
        }
        EnginePayload::PeopleChanged { .. }
        | EnginePayload::InviteChanged { .. }
        | EnginePayload::MessageHeld { .. }
        | EnginePayload::MessageReleased { .. }
        | EnginePayload::HostedChanged { .. } => C::Channels,
        EnginePayload::SettingsChanged { .. }
        | EnginePayload::GitSetupChanged { .. }
        | EnginePayload::ConnectorsChanged { .. }
        | EnginePayload::AddonsChanged { .. }
        | EnginePayload::RelaysChanged
        | EnginePayload::Paused
        | EnginePayload::Resumed => C::Node,
    })
}

/// The concept a host's listening files under.
fn host_concept(host: &ListenerHost) -> ActivityConcept {
    match host {
        ListenerHost::Workspace { .. } => ActivityConcept::Workflows,
        ListenerHost::Goal { .. } => ActivityConcept::Goals,
    }
}

/// The record a host's listening opens on.
fn host_source(host: &ListenerHost) -> ActivitySource {
    match host {
        ListenerHost::Workspace { workflow } => {
            ActivitySource::new(ActivitySourceKind::Workflow, workflow.to_string())
        }
        ListenerHost::Goal { goal } => {
            ActivitySource::new(ActivitySourceKind::Goal, goal.to_string())
        }
    }
}

/// What an event is about: the record a row opens on. The envelope's goal
/// when the payload names nothing narrower, else the workflow of the run of
/// the workspace behind it; this node when nothing at all.
pub fn source_of(event: &EngineEvent) -> ActivitySource {
    use ActivitySourceKind as K;
    let goal = || {
        event
            .goal
            .map(|g| ActivitySource::new(K::Goal, g.to_string()))
            .or_else(|| {
                event
                    .workflow
                    .map(|w| ActivitySource::new(K::Workflow, w.to_string()))
            })
            .unwrap_or_else(ActivitySource::node)
    };
    match &event.payload {
        EnginePayload::GoalCreated { goal, .. }
        | EnginePayload::DocumentAdded { goal, .. }
        | EnginePayload::GoalArchived { goal, .. }
        | EnginePayload::GoalDeleted { goal } => ActivitySource::new(K::Goal, goal.to_string()),
        EnginePayload::SignalReceived {
            listener: Some(key),
            ..
        }
        | EnginePayload::ListenerFired { listener: key, .. }
        | EnginePayload::ListenerFailed { listener: key, .. } => host_source(&key.host),
        EnginePayload::ListeningChanged { host, .. } => host_source(host),
        EnginePayload::WorkflowProposed { workflow, .. }
        | EnginePayload::WorkflowChanged { workflow, .. }
        | EnginePayload::WorkflowDeleted { workflow }
        | EnginePayload::WorkflowArchived { workflow, .. } => {
            ActivitySource::new(K::Workflow, workflow.to_string())
        }
        EnginePayload::ProjectCreated { project, .. }
        | EnginePayload::ProjectChanged { project }
        | EnginePayload::CommitterNeeded { project, .. }
        | EnginePayload::CommitterSet { project, .. }
        | EnginePayload::ProjectArchived { project, .. }
        | EnginePayload::ProjectDeleted { project }
        | EnginePayload::AttachmentChanged { project, .. } => {
            ActivitySource::new(K::Project, project.to_string())
        }
        EnginePayload::WorkstreamOpened { workstream, .. }
        | EnginePayload::WorkstreamEdited { workstream, .. }
        | EnginePayload::WorkstreamChanged { workstream, .. }
        | EnginePayload::WorkstreamCommitted { workstream, .. }
        | EnginePayload::WorkstreamScriptRan { workstream, .. }
        | EnginePayload::WorkstreamPublishFailed { workstream, .. }
        | EnginePayload::ServerChanged {
            workstream: Some(workstream),
        } => ActivitySource::new(K::Workstream, workstream.to_string()),
        // An artifact's page is nobody's checkout.
        EnginePayload::ServerChanged { workstream: None } => {
            ActivitySource::new(K::Workspace, "servers".to_string())
        }
        EnginePayload::AgentReplied { agent, .. }
        | EnginePayload::ContentScreened { agent, .. } => {
            ActivitySource::new(K::Agent, agent.clone())
        }
        EnginePayload::NoteChanged { note, .. } => ActivitySource::new(K::Workspace, note.clone()),
        EnginePayload::DrawingChanged { drawing, .. } => {
            ActivitySource::new(K::Workspace, drawing.clone())
        }
        EnginePayload::ConversationCreated { id, .. }
        | EnginePayload::ConversationChanged { id, .. } => {
            ActivitySource::new(K::Conversation, id.to_string())
        }
        EnginePayload::PeopleChanged { pubkey, .. }
        | EnginePayload::MessageHeld { author: pubkey, .. } => {
            ActivitySource::new(K::Workspace, pubkey.as_hex().to_string())
        }
        EnginePayload::InviteChanged { invite } => {
            ActivitySource::new(K::Workspace, invite.id.to_string())
        }
        EnginePayload::MessageReleased { scope, .. } => {
            ActivitySource::new(K::Conversation, scope.clone())
        }
        EnginePayload::HostedChanged { host, .. } => {
            ActivitySource::new(K::Workspace, host.as_hex().to_string())
        }
        EnginePayload::SettingsChanged { .. }
        | EnginePayload::GitSetupChanged { .. }
        | EnginePayload::ConnectorsChanged { .. }
        | EnginePayload::AddonsChanged { .. }
        | EnginePayload::RelaysChanged
        | EnginePayload::Paused
        | EnginePayload::Resumed
        | EnginePayload::Redacted { .. } => ActivitySource::node(),
        _ => goal(),
    }
}

/// The fact an event is, when it is one. A run's story with no goal behind
/// it — a run of the workspace's — is the Workflows' rather than the Goals'.
pub fn fact_of(event: &EngineEvent) -> Option<ActivityFact> {
    let concept = match concept_of(&event.payload)? {
        ActivityConcept::Goals if event.goal.is_none() && event.workflow.is_some() => {
            ActivityConcept::Workflows
        }
        concept => concept,
    };
    let payload = serde_json::to_value(&event.payload).ok()?;
    Some(ActivityFact {
        at: now_secs(),
        concept,
        kind: bisa_core::tag_of(&payload),
        source: source_of(event),
        author: None,
        event: payload,
    })
}

/// Record an event into the feed — the log and the index — before it is
/// published. A store that refuses is logged and never blocks the bus: the
/// frame still reaches every subscriber.
pub fn record(inner: &Inner, event: &EngineEvent) {
    let Some(fact) = fact_of(event) else {
        // A per-frame signal — a session's tokens, a file the watcher saw —
        // is not a fact of the feed, and reaches the log only at debug.
        tracing::debug!(
            target: "bisa_engine::activity",
            kind = event.payload.topic(),
            "engine event"
        );
        return;
    };
    // The platform's own timeline, one line per fact, so a log read at
    // `info` carries what happened around an error.
    tracing::info!(
        target: "bisa_engine::activity",
        kind = %fact.kind,
        concept = fact.concept.as_str(),
        source = %fact.source.id,
        source_kind = fact.source.kind.as_str(),
        "activity"
    );
    if let Err(e) = inner.ws.record_activity(&fact, true) {
        // The same fields as the line above: what is missing from the feed,
        // and from where.
        tracing::warn!(
            target: "bisa_engine::activity",
            kind = %fact.kind,
            concept = fact.concept.as_str(),
            source = %fact.source.id,
            source_kind = fact.source.kind.as_str(),
            "activity not recorded: {e}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_that_is_not_a_fact_has_no_concept_and_the_rest_are_placed() {
        assert_eq!(
            concept_of(&EnginePayload::Paused),
            Some(ActivityConcept::Node)
        );
        assert_eq!(
            concept_of(&EnginePayload::ResultAccepted),
            Some(ActivityConcept::Goals)
        );
        assert_eq!(
            concept_of(&EnginePayload::AgentThinking {
                scope: "s".into(),
                agent: "a".into()
            }),
            None,
            "a per-turn signal is not the feed's"
        );
        assert_eq!(
            concept_of(&EnginePayload::Redacted {
                count: 1,
                kinds: vec![],
                at: "prompt".into()
            }),
            Some(ActivityConcept::Agents)
        );
        let workflow: bisa_core::WorkflowId = "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap();
        let goal: bisa_core::GoalId = "01DX5ZZKBKACTAV9WEVGEMMVRZ".parse().unwrap();
        let library: bisa_core::ListenerKey =
            format!("workspace:{workflow}/nightly").parse().unwrap();
        let of_goal: bisa_core::ListenerKey = format!("goal:{goal}/ticket").parse().unwrap();
        assert_eq!(
            concept_of(&EnginePayload::SignalReceived {
                signal: "s".into(),
                listener: Some(library.clone()),
                source: bisa_core::SignalSource::Schedule,
            }),
            Some(ActivityConcept::Workflows)
        );
        assert_eq!(
            concept_of(&EnginePayload::ListenerFailed {
                listener: of_goal,
                signal: None,
                error: "e".into(),
            }),
            Some(ActivityConcept::Goals),
            "a goal's listener is the goal's story"
        );
        assert_eq!(
            concept_of(&EnginePayload::ListeningChanged {
                host: library.host,
                on: true,
            }),
            Some(ActivityConcept::Workflows)
        );
        assert!(!ActivityConcept::ALL
            .iter()
            .any(|c| c.as_str() == "triggers"));
    }

    #[test]
    fn a_fact_names_what_it_is_about_and_keeps_its_payload() {
        let workflow: bisa_core::WorkflowId = "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap();
        let listener: bisa_core::ListenerKey =
            format!("workspace:{workflow}/nightly").parse().unwrap();
        let event = EngineEvent::global(EnginePayload::SignalReceived {
            signal: "s1".into(),
            listener: Some(listener.clone()),
            source: bisa_core::SignalSource::Hook,
        });
        let fact = fact_of(&event).unwrap();
        assert_eq!(fact.kind, "signal_received");
        assert_eq!(
            fact.source,
            ActivitySource::new(ActivitySourceKind::Workflow, workflow.to_string())
        );
        assert_eq!(
            fact.event["listener"],
            listener.to_string(),
            "the payload rides verbatim"
        );
        assert_eq!(
            fact_of(&EngineEvent::global(EnginePayload::Resumed))
                .unwrap()
                .source,
            ActivitySource::node()
        );
        assert_eq!(
            fact_of(&EngineEvent::global(EnginePayload::AgentReplied {
                scope: "c".into(),
                agent: "dev".into(),
                posted: true,
                message: None,
            }))
            .unwrap()
            .source,
            ActivitySource::new(ActivitySourceKind::Agent, "dev")
        );
        assert!(fact_of(&EngineEvent::global(EnginePayload::AgentThinking {
            scope: "s".into(),
            agent: "a".into()
        }))
        .is_none());
    }

    /// A run of the workspace has no goal: its story files under its
    /// workflow, in the Workflows' concept — a goal's run stays the goal's.
    #[test]
    fn a_workspace_runs_facts_file_under_its_workflow() {
        let workflow: bisa_core::WorkflowId = "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap();
        let goal: bisa_core::GoalId = "01DX5ZZKBKACTAV9WEVGEMMVRZ".parse().unwrap();
        let run: bisa_core::RunId = "01DX5ZZKBKACTAV9WEVGEMMVS0".parse().unwrap();
        let finished = EnginePayload::RunFinished {
            run,
            workflow,
            outcome: bisa_core::RunOutcome::Done,
        };
        let workspace = crate::events::EventScope {
            goal: None,
            workflow: Some(workflow),
            run: Some(run),
        };
        let fact = fact_of(&workspace.event(None, finished.clone())).unwrap();
        assert_eq!(fact.concept, ActivityConcept::Workflows);
        assert_eq!(
            fact.source,
            ActivitySource::new(ActivitySourceKind::Workflow, workflow.to_string())
        );
        let on_goal = crate::events::EventScope {
            goal: Some(goal),
            workflow: Some(workflow),
            run: Some(run),
        };
        let fact = fact_of(&on_goal.event(None, finished)).unwrap();
        assert_eq!(fact.concept, ActivityConcept::Goals);
        assert_eq!(
            fact.source,
            ActivitySource::new(ActivitySourceKind::Goal, goal.to_string())
        );
    }
}
