//! Why a session exists.
//!
//! Every live harness session the engine knows — a step's worker, the
//! Workflow Agent's wake, a conversation's turn, a harness a person opened in
//! a terminal, one bounded question put to a model — carries one
//! [`SessionOrigin`] on its roster row: what woke it and for what. It is the
//! fact a person reads to answer *why is this running*, beside where the
//! session stands and what it is doing; the engine says it once at
//! registration and again when it changes. Pure data: the engine's presence
//! fold fills it, the node serves it as it is, the desktop puts it into words.

use crate::decision::DecisionPoint;
use crate::event::GuidancePhase;
use crate::id::PrincipalId;
use crate::workflow::StepId;
use serde::{Deserialize, Serialize};

/// What woke a session, and for what.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "origin", rename_all = "snake_case")]
pub enum SessionOrigin {
    /// A run's `agent` step: the step by id and by its name in the workflow,
    /// and whether this session resumed a work item a restart cut short. The
    /// id and the name are absent only for a work item made outside a run's
    /// step — a spawned sub-item.
    Step {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step: Option<StepId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default)]
        resumed: bool,
    },
    /// The Workflow Agent, woken for a goal: designing its workflow, or
    /// repairing it after a failed run.
    Design { phase: GuidancePhase },
    /// One turn of a conversation. `scope` is the conversation's scope as the
    /// bus spells it in `agent_thinking`, so a screen pairs the two;
    /// `on_behalf_of` is the person on another node whose message woke the
    /// turn, absent for the owner's own.
    Turn {
        scope: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        on_behalf_of: Option<PrincipalId>,
    },
    /// A harness a person opened in a desktop terminal.
    Terminal,
    /// One bounded question put to a model and nothing else — no tools, a
    /// read-only ceiling, a deadline.
    Ask { purpose: AskPurpose },
}

/// What a one-shot ask is for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AskPurpose {
    /// The Tool & Commands Guard's classifier reading a call, a message or
    /// content an agent is about to read.
    Classifier,
    /// The Decision-Making Agent judging at one of its points — or at none,
    /// for a *Try it* from Settings.
    Decision {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        point: Option<DecisionPoint>,
    },
    /// A commit message suggested for a workstream's changes.
    CommitMessage,
    /// A pull request's title and body suggested for a branch.
    PullRequestMessage,
}

impl SessionOrigin {
    /// The tag the wire spells: `step`, `design`, `turn`, `terminal`, `ask`.
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionOrigin::Step { .. } => "step",
            SessionOrigin::Design { .. } => "design",
            SessionOrigin::Turn { .. } => "turn",
            SessionOrigin::Terminal => "terminal",
            SessionOrigin::Ask { .. } => "ask",
        }
    }
}

impl AskPurpose {
    /// The word the wire spells: `classifier`, `decision`, `commit_message`,
    /// `pull_request_message`.
    pub fn as_str(&self) -> &'static str {
        match self {
            AskPurpose::Classifier => "classifier",
            AskPurpose::Decision { .. } => "decision",
            AskPurpose::CommitMessage => "commit_message",
            AskPurpose::PullRequestMessage => "pull_request_message",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_origin() -> Vec<SessionOrigin> {
        vec![
            SessionOrigin::Step {
                step: Some(StepId::new("build").unwrap()),
                name: Some("Build".into()),
                resumed: true,
            },
            SessionOrigin::Step {
                step: None,
                name: None,
                resumed: false,
            },
            SessionOrigin::Design {
                phase: GuidancePhase::Repair,
            },
            SessionOrigin::Turn {
                scope: "channel:general".into(),
                on_behalf_of: None,
            },
            SessionOrigin::Terminal,
            SessionOrigin::Ask {
                purpose: AskPurpose::Decision {
                    point: Some(DecisionPoint::AssignPick),
                },
            },
            SessionOrigin::Ask {
                purpose: AskPurpose::Classifier,
            },
        ]
    }

    #[test]
    fn every_origin_round_trips_and_spells_its_tag() {
        for origin in every_origin() {
            let wire = serde_json::to_value(&origin).unwrap();
            assert_eq!(wire["origin"], serde_json::json!(origin.as_str()), "{wire}");
            let back: SessionOrigin = serde_json::from_value(wire).unwrap();
            assert_eq!(back, origin);
        }
    }

    #[test]
    fn a_step_with_nothing_to_say_writes_only_its_tag_and_reads_back_with_defaults() {
        let wire = serde_json::to_value(SessionOrigin::Step {
            step: None,
            name: None,
            resumed: false,
        })
        .unwrap();
        assert_eq!(
            wire,
            serde_json::json!({ "origin": "step", "resumed": false })
        );
        let back: SessionOrigin =
            serde_json::from_value(serde_json::json!({ "origin": "step" })).unwrap();
        assert!(matches!(
            back,
            SessionOrigin::Step {
                step: None,
                name: None,
                resumed: false
            }
        ));
    }

    #[test]
    fn an_asks_purpose_carries_the_decision_point_and_its_word() {
        let wire = serde_json::to_value(SessionOrigin::Ask {
            purpose: AskPurpose::Decision {
                point: Some(DecisionPoint::SecurityTool),
            },
        })
        .unwrap();
        assert_eq!(wire["purpose"]["kind"], serde_json::json!("decision"));
        assert_eq!(wire["purpose"]["point"], serde_json::json!("security.tool"));
        let tried = serde_json::to_value(AskPurpose::Decision { point: None }).unwrap();
        assert_eq!(
            tried,
            serde_json::json!({ "kind": "decision" }),
            "a Try it names no point"
        );
        assert_eq!(
            AskPurpose::PullRequestMessage.as_str(),
            "pull_request_message"
        );
    }
}
