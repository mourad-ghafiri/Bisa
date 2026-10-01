//! The activity feed's vocabulary: **one concept per fact**, and the fact
//! itself as the feed stores it.
//!
//! The Pulse shows everything that happens across the platform, and a
//! reader narrows it to one of the platform's core concepts — a workspace's
//! own shape changing, goals, workflows, projects and their workstreams,
//! channels, agents, this node. A fact belongs to exactly one
//! concept, decided here for a journal payload and in the engine for its
//! own payloads, so a filter is a column and never a guess made twice.

use crate::event::JournalPayload;
use crate::home::Home;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// The concepts a reader filters the feed by, in the order a surface lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActivityConcept {
    /// The workspace's own shape: a goal made, a note written.
    Workspace,
    /// A goal's story: its notes, questions, decisions, progress, results and runs.
    Goals,
    /// Workflows designed, proposed, changed and turned on — and what their
    /// start events heard.
    Workflows,
    /// Projects and their workstreams: made, edited, opened, committed, attached.
    Projects,
    /// Messages in a channel, a direct message or a workstream thread.
    Channels,
    /// Agents at work: claims, metrics, sessions scheduled and ended, replies, the guard.
    Agents,
    /// This node: its settings, its git setup, paused and resumed.
    Node,
}

impl ActivityConcept {
    /// Every concept, in the order the feed's tabs draw them.
    pub const ALL: &'static [ActivityConcept] = &[
        ActivityConcept::Workspace,
        ActivityConcept::Goals,
        ActivityConcept::Workflows,
        ActivityConcept::Projects,
        ActivityConcept::Channels,
        ActivityConcept::Agents,
        ActivityConcept::Node,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ActivityConcept::Workspace => "workspace",
            ActivityConcept::Goals => "goals",
            ActivityConcept::Workflows => "workflows",
            ActivityConcept::Projects => "projects",
            ActivityConcept::Channels => "channels",
            ActivityConcept::Agents => "agents",
            ActivityConcept::Node => "node",
        }
    }

    /// The concept a journal fact belongs to, by where it was written: a
    /// goal's story is the Goals', a run of the workspace's the Workflows'.
    /// Exhaustive: a payload added to the journal stops this compiling until
    /// it is placed.
    pub fn of_journal(home: &Home, payload: &JournalPayload) -> ActivityConcept {
        let story = match home {
            Home::Goal { .. } => ActivityConcept::Goals,
            Home::Run { .. } => ActivityConcept::Workflows,
        };
        match payload {
            JournalPayload::Note { .. }
            | JournalPayload::Question { .. }
            | JournalPayload::Withdrawn { .. }
            | JournalPayload::Decision { .. }
            | JournalPayload::Progress { .. }
            | JournalPayload::Result { .. }
            | JournalPayload::Attachment { .. }
            | JournalPayload::Document { .. }
            | JournalPayload::Step { .. }
            | JournalPayload::Run { .. }
            | JournalPayload::Signal { .. } => story,
            JournalPayload::Guidance { .. } => ActivityConcept::Workflows,
            JournalPayload::Claim { .. }
            | JournalPayload::TurnMetrics { .. }
            | JournalPayload::Guard { .. }
            | JournalPayload::Judgement { .. } => ActivityConcept::Agents,
        }
    }
}

impl fmt::Display for ActivityConcept {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ActivityConcept {
    type Err = crate::CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ActivityConcept::ALL
            .iter()
            .copied()
            .find(|c| c.as_str() == s)
            .ok_or_else(|| crate::CoreError::UnknownActivityConcept(s.to_string()))
    }
}

/// What a fact is about — the thing a row opens on. `Node` and `Workspace`
/// name no record; their id is the empty string.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActivitySourceKind {
    Goal,
    Channel,
    /// A conversation: its messages, whatever its origin.
    Conversation,
    Workstream,
    Project,
    Workflow,
    Agent,
    Node,
    Workspace,
}

impl ActivitySourceKind {
    pub const ALL: &'static [ActivitySourceKind] = &[
        ActivitySourceKind::Goal,
        ActivitySourceKind::Channel,
        ActivitySourceKind::Conversation,
        ActivitySourceKind::Workstream,
        ActivitySourceKind::Project,
        ActivitySourceKind::Workflow,
        ActivitySourceKind::Agent,
        ActivitySourceKind::Node,
        ActivitySourceKind::Workspace,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ActivitySourceKind::Goal => "goal",
            ActivitySourceKind::Channel => "channel",
            ActivitySourceKind::Conversation => "conversation",
            ActivitySourceKind::Workstream => "workstream",
            ActivitySourceKind::Project => "project",
            ActivitySourceKind::Workflow => "workflow",
            ActivitySourceKind::Agent => "agent",
            ActivitySourceKind::Node => "node",
            ActivitySourceKind::Workspace => "workspace",
        }
    }
}

impl FromStr for ActivitySourceKind {
    type Err = crate::CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ActivitySourceKind::ALL
            .iter()
            .copied()
            .find(|k| k.as_str() == s)
            .ok_or_else(|| crate::CoreError::UnknownActivityConcept(s.to_string()))
    }
}

/// What a row is about: the kind of thing and its id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActivitySource {
    pub kind: ActivitySourceKind,
    pub id: String,
}

impl ActivitySource {
    pub fn new(kind: ActivitySourceKind, id: impl Into<String>) -> Self {
        Self {
            kind,
            id: id.into(),
        }
    }

    /// This node, which no record names.
    pub fn node() -> Self {
        Self::new(ActivitySourceKind::Node, "")
    }

    /// The workspace itself.
    pub fn workspace() -> Self {
        Self::new(ActivitySourceKind::Workspace, "")
    }
}

/// One fact as the feed stores and serves it: when, which concept, the
/// payload's tag, what it is about, who signed it, and the payload
/// **verbatim** — the feed never restates a fact's fields, so nothing added
/// to a payload can reach a reader as less than itself.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActivityFact {
    pub at: u64,
    pub concept: ActivityConcept,
    /// The payload's own tag — `note`, `message`, `workstream_opened`.
    pub kind: String,
    pub source: ActivitySource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub event: serde_json::Value,
}

/// The tag a serialised payload carries — every payload the feed stores is
/// internally tagged on `type`.
pub fn tag_of(event: &serde_json::Value) -> String {
    event
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("unknown")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_concept_parses_its_seven_names_and_nothing_else() {
        for c in ActivityConcept::ALL {
            assert_eq!(c.as_str().parse::<ActivityConcept>().unwrap(), *c);
            assert_eq!(
                serde_json::to_value(c).unwrap(),
                serde_json::json!(c.as_str())
            );
        }
        assert_eq!(ActivityConcept::ALL.len(), 7);
        assert!("everything".parse::<ActivityConcept>().is_err());
        assert!("triggers".parse::<ActivityConcept>().is_err());
        assert!("trigger".parse::<ActivitySourceKind>().is_err());
        assert!("goal".parse::<ActivitySourceKind>().is_ok());
        assert!("peer".parse::<ActivitySourceKind>().is_err());
    }

    #[test]
    fn every_journal_fact_has_one_concept() {
        use crate::event::{RunFact, StepFact};
        let on_goal = Home::Goal {
            goal: crate::GoalId::from_ulid(ulid::Ulid::nil()),
        };
        let on_run = Home::Run {
            run: crate::RunId::from_ulid(ulid::Ulid::nil()),
        };
        let goal = JournalPayload::Note { text: "x".into() };
        assert_eq!(
            ActivityConcept::of_journal(&on_goal, &goal),
            ActivityConcept::Goals
        );
        let run = JournalPayload::Run {
            run: crate::RunId::from_ulid(ulid::Ulid::nil()),
            event: RunFact::Finished {
                outcome: crate::workflow::RunOutcome::Done,
            },
        };
        assert_eq!(
            ActivityConcept::of_journal(&on_goal, &run),
            ActivityConcept::Goals,
            "a goal's run is the goal's story"
        );
        assert_eq!(
            ActivityConcept::of_journal(&on_run, &run),
            ActivityConcept::Workflows,
            "a run of the workspace is its workflow's story"
        );
        let step = JournalPayload::Step {
            run: crate::RunId::from_ulid(ulid::Ulid::nil()),
            step: crate::StepId::new("build").unwrap(),
            event: StepFact::Waiting,
        };
        assert_eq!(
            ActivityConcept::of_journal(&on_goal, &step),
            ActivityConcept::Goals
        );
        assert_eq!(
            ActivityConcept::of_journal(&on_run, &step),
            ActivityConcept::Workflows
        );
        let signal = JournalPayload::Signal {
            signal: "s".into(),
            listener: None,
            source: crate::listen::SignalSource::Project,
            name: None,
            payload: serde_json::json!({}),
        };
        assert_eq!(
            ActivityConcept::of_journal(&on_goal, &signal),
            ActivityConcept::Goals,
            "the occurrence that started a goal's run is the goal's story"
        );
        assert_eq!(
            ActivityConcept::of_journal(&on_run, &signal),
            ActivityConcept::Workflows
        );
        let metrics = JournalPayload::TurnMetrics {
            session: crate::SessionId::from_ulid(ulid::Ulid::nil()),
            input_tokens: 1,
            output_tokens: 1,
            usd_cents: 1,
        };
        for home in [&on_goal, &on_run] {
            assert_eq!(
                ActivityConcept::of_journal(home, &metrics),
                ActivityConcept::Agents,
                "an agent's facts are the Agents' wherever they land"
            );
        }
    }

    #[test]
    fn a_fact_keeps_its_payload_verbatim_and_names_its_tag() {
        let event = serde_json::json!({"type": "workstream_opened", "workstream": "w1", "extra": {"deep": true}});
        assert_eq!(tag_of(&event), "workstream_opened");
        assert_eq!(tag_of(&serde_json::json!({})), "unknown");
        let fact = ActivityFact {
            at: 5,
            concept: ActivityConcept::Projects,
            kind: tag_of(&event),
            source: ActivitySource::new(ActivitySourceKind::Workstream, "w1"),
            author: None,
            event: event.clone(),
        };
        let back: ActivityFact =
            serde_json::from_str(&serde_json::to_string(&fact).unwrap()).unwrap();
        assert_eq!(back.event, event);
        assert_eq!(back.source.kind.as_str(), "workstream");
        assert_eq!(ActivitySource::node().id, "");
    }
}
