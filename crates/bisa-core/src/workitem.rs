//! Work items: what an `agent` step becomes when it runs — the unit a harness
//! session actually executes — and a state machine of their own.
//!
//! The work item had eight states and one method (`is_terminal`), and fifteen
//! sites assigned its state directly. `WorkItemTransition` closes that: the
//! only way a work item's state changes is [`WorkItemState::apply`].

use crate::assignee::Assignee;
use crate::caps::ToolTier;
use crate::effort::EffortChoice;
use crate::goal::Budget;
use crate::home::Home;
use crate::id::{PrincipalId, ProjectId, RunId, WorkItemId};
use crate::workflow::StepId;
use serde::{Deserialize, Serialize};

/// The harness work runs on when nobody named one: neither the step, nor an
/// agent that took it.
pub const DEFAULT_HARNESS: &str = "claude-code";

/// `deny_unknown_fields`: an item written by the plan-shaped code carried
/// `plan` and `criteria`, and this build refuses it rather than half-reading it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkItemSpec {
    pub id: WorkItemId,
    /// Where the item is filed — its run's home: the goal the run is for, or
    /// a run of the workspace itself.
    pub home: Home,
    /// The run and the step this item was created for. An item outside a run
    /// is refused at preflight: nothing schedules work a workflow did not ask
    /// for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<RunId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<StepId>,
    /// What to do, self-contained (a fresh session must be able to act on it).
    pub instructions: String,
    pub state: WorkItemState,

    // --- placement ---
    /// The project this item works in. `None` means the work names no project
    /// and the item runs in its home's own `scratch/` directory. There is
    /// deliberately no `cwd`: a spec that could name a directory could name
    /// the workspace root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectId>,

    // --- executor ---
    /// The harnesses the item runs on, in order; the engine walks this chain
    /// on `Unavailable`. What its step named — the author's word — and, for
    /// a step that named none, empty until the item is taken: it then runs
    /// on the harness of the agent that took it, else on
    /// [`DEFAULT_HARNESS`], and says so here.
    pub harness_candidates: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The step's effort pin — a level, or `auto` — the first link of the
    /// chain. Absent means the model's own, the agent's plan, the setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<EffortChoice>,

    // --- contract ---
    /// JSON Schema the structured result must satisfy (via `yield_result`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,

    // --- budget ---
    #[serde(default)]
    pub budget: Budget,

    // --- assignment ---
    /// Who this item is *for*: the request. Joins the assignment union at the
    /// nearest position and, when non-empty, wins alone.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assignees: Vec<Assignee>,

    // --- capabilities ---
    #[serde(default = "default_tier")]
    pub tier_ceiling: ToolTier,
    /// The agent that actually ran this item — **written by the engine when it
    /// picks, never accepted from a caller.**
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// Agent ids this item may spawn as sub-work ("*" = any, empty = none).
    #[serde(default)]
    pub spawn_allowlist: Vec<String>,
    /// Remaining spawn depth (0 = leaf).
    #[serde(default)]
    pub depth_budget: u8,

    // --- the bounds, on the item so a restart neither refunds nor charges them ---
    /// Results submitted that did not fit the contract; the last miss blocks
    /// the item (`max_result_attempts`).
    #[serde(default)]
    pub result_attempts: u8,
    /// Sessions on this item a restart cut short; the next session is told.
    #[serde(default)]
    pub interruptions: u8,
}

fn default_tier() -> ToolTier {
    ToolTier::Write
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "state")]
pub enum WorkItemState {
    Open,
    /// Claimed by a principal via a signed `kind:3402`; first valid claim wins.
    Claimed {
        by: PrincipalId,
    },
    InProgress {
        by: PrincipalId,
    },
    /// Waiting on a gate, a human or a dependency.
    Blocked {
        by: PrincipalId,
        reason: String,
    },
    /// Result submitted, awaiting verification.
    Review {
        by: PrincipalId,
    },
    Accepted,
    /// Rejected with the verification evidence; re-runs as steering input.
    Rejected {
        evidence: Vec<String>,
    },
    Cancelled,
}

/// The moves. Constructing one is the proof the move exists; `apply` decides
/// whether it is legal from here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "transition")]
pub enum WorkItemTransition {
    /// Open | Rejected -> Claimed. First valid claim wins; the store makes it
    /// atomic.
    Claim { by: PrincipalId },
    /// Claimed -> InProgress.
    Start,
    /// Claimed | InProgress -> Open. The claimer let go (a launch that never
    /// started, a session that died before it produced anything).
    Release,
    /// InProgress -> Blocked.
    Block { reason: String },
    /// Blocked -> InProgress.
    Unblock,
    /// Blocked -> Open. "Go again": an explicit run resets blocked items.
    Reset,
    /// InProgress -> Review.
    Submit,
    /// Review -> Accepted.
    Accept,
    /// Review -> Rejected.
    Reject { evidence: Vec<String> },
    /// Any non-terminal -> Cancelled.
    Cancel,
}

impl WorkItemTransition {
    pub fn name(&self) -> &'static str {
        match self {
            WorkItemTransition::Claim { .. } => "claim",
            WorkItemTransition::Start => "start",
            WorkItemTransition::Release => "release",
            WorkItemTransition::Block { .. } => "block",
            WorkItemTransition::Unblock => "unblock",
            WorkItemTransition::Reset => "reset",
            WorkItemTransition::Submit => "submit",
            WorkItemTransition::Accept => "accept",
            WorkItemTransition::Reject { .. } => "reject",
            WorkItemTransition::Cancel => "cancel",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WorkItemError {
    #[error("cannot {transition} a work item that is {from}")]
    Illegal {
        from: &'static str,
        transition: &'static str,
    },
    #[error("the work item is {0}; nothing moves")]
    Terminal(&'static str),
}

impl WorkItemState {
    pub fn is_terminal(&self) -> bool {
        matches!(self, WorkItemState::Accepted | WorkItemState::Cancelled)
    }

    /// Unsettled: claimed by somebody and not yet accepted, rejected or
    /// cancelled. The goal's `Submit` guard counts these.
    pub fn is_unsettled(&self) -> bool {
        matches!(
            self,
            WorkItemState::Claimed { .. }
                | WorkItemState::InProgress { .. }
                | WorkItemState::Blocked { .. }
                | WorkItemState::Review { .. }
        )
    }

    /// The index column's spelling.
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkItemState::Open => "open",
            WorkItemState::Claimed { .. } => "claimed",
            WorkItemState::InProgress { .. } => "in_progress",
            WorkItemState::Blocked { .. } => "blocked",
            WorkItemState::Review { .. } => "review",
            WorkItemState::Accepted => "accepted",
            WorkItemState::Rejected { .. } => "rejected",
            WorkItemState::Cancelled => "cancelled",
        }
    }

    /// The one exhaustive match. Nothing else writes a work item's state.
    pub fn apply(self, t: &WorkItemTransition) -> Result<WorkItemState, WorkItemError> {
        use WorkItemState::*;
        use WorkItemTransition as T;
        if self.is_terminal() {
            return Err(WorkItemError::Terminal(self.as_str()));
        }
        let illegal = || WorkItemError::Illegal {
            from: self.as_str(),
            transition: t.name(),
        };
        match (&self, t) {
            (Open, T::Claim { by }) | (Rejected { .. }, T::Claim { by }) => {
                Ok(Claimed { by: by.clone() })
            }
            (Claimed { by }, T::Start) => Ok(InProgress { by: by.clone() }),
            (Claimed { .. }, T::Release) | (InProgress { .. }, T::Release) => Ok(Open),
            (InProgress { by }, T::Block { reason }) => Ok(Blocked {
                by: by.clone(),
                reason: reason.clone(),
            }),
            (Blocked { by, .. }, T::Unblock) => Ok(InProgress { by: by.clone() }),
            (Blocked { .. }, T::Reset) => Ok(Open),
            (InProgress { by }, T::Submit) => Ok(Review { by: by.clone() }),
            (Review { .. }, T::Accept) => Ok(Accepted),
            (Review { .. }, T::Reject { evidence }) => Ok(Rejected {
                evidence: evidence.clone(),
            }),
            (_, T::Cancel) => Ok(Cancelled),
            _ => Err(illegal()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pk() -> PrincipalId {
        PrincipalId::new("ab".repeat(32)).unwrap()
    }

    fn spec() -> WorkItemSpec {
        WorkItemSpec {
            id: WorkItemId::from_ulid(ulid::Ulid::from_parts(2, 1)),
            home: Home::Goal {
                goal: crate::GoalId::from_ulid(ulid::Ulid::from_parts(1, 1)),
            },
            run: Some(RunId::from_ulid(ulid::Ulid::from_parts(4, 1))),
            step: Some(StepId::new("implement").unwrap()),
            instructions: "implement the parser".into(),
            state: WorkItemState::Open,
            project: Some(ProjectId::from_ulid(ulid::Ulid::from_parts(3, 1))),
            harness_candidates: vec!["claude-code".into()],
            model: None,
            effort: None,
            output_schema: None,
            budget: Budget::default(),
            assignees: vec![],
            tier_ceiling: ToolTier::Write,
            agent: None,
            spawn_allowlist: vec![],
            depth_budget: 0,
            result_attempts: 0,
            interruptions: 0,
        }
    }

    #[test]
    fn spec_roundtrip_with_defaults() {
        let s = spec();
        let json = serde_json::to_value(&s).unwrap();
        assert!(json.get("assignees").is_none());
        assert!(json.get("agent").is_none());
        assert!(json.get("effort").is_none(), "no pin, no key");
        assert_eq!(json["step"], "implement");
        assert_eq!(json["home"]["home"], "goal", "filed under its home");
        assert_eq!(
            serde_json::from_value::<WorkItemSpec>(json.clone()).unwrap(),
            s
        );
        // An item of a run of the workspace is filed under the run.
        let run = RunId::from_ulid(ulid::Ulid::from_parts(4, 1));
        let of_workspace_run = WorkItemSpec {
            home: Home::Run { run },
            ..spec()
        };
        let wire = serde_json::to_value(&of_workspace_run).unwrap();
        assert_eq!(wire["home"]["run"], run.to_string());
        assert_eq!(
            serde_json::from_value::<WorkItemSpec>(wire).unwrap(),
            of_workspace_run
        );
        // A pin travels in its own word and reads back; a word that is no
        // effort is refused.
        let pinned = WorkItemSpec {
            effort: Some(EffortChoice::Xhigh),
            ..spec()
        };
        let wire = serde_json::to_value(&pinned).unwrap();
        assert_eq!(wire["effort"], "xhigh");
        assert_eq!(
            serde_json::from_value::<WorkItemSpec>(wire.clone()).unwrap(),
            pinned
        );
        let mut odd = wire;
        odd["effort"] = serde_json::json!("ultra");
        assert!(serde_json::from_value::<WorkItemSpec>(odd).is_err());
        // The shape that named its goal at the top level is refused.
        let mut goal_keyed = serde_json::to_value(&s).unwrap();
        let goal = goal_keyed["home"]["goal"].clone();
        goal_keyed.as_object_mut().unwrap().remove("home");
        goal_keyed["goal"] = goal;
        assert!(serde_json::from_value::<WorkItemSpec>(goal_keyed).is_err());
        // The plan-shaped item is refused, never half-read.
        let mut old = json;
        old["plan"] = serde_json::json!(null);
        old["criteria"] = serde_json::json!([]);
        assert!(serde_json::from_value::<WorkItemSpec>(old).is_err());
    }

    #[test]
    fn the_ordinary_life_of_an_item() {
        use WorkItemState::*;
        use WorkItemTransition as T;
        let s = Open.apply(&T::Claim { by: pk() }).unwrap();
        let s = s.apply(&T::Start).unwrap();
        assert_eq!(s, InProgress { by: pk() });
        let s = s
            .apply(&T::Block {
                reason: "gate".into(),
            })
            .unwrap();
        assert!(s.is_unsettled());
        let s = s.apply(&T::Unblock).unwrap();
        let s = s.apply(&T::Submit).unwrap();
        assert_eq!(s, Review { by: pk() });
        let s = s.apply(&T::Accept).unwrap();
        assert!(s.is_terminal());
        assert_eq!(
            s.apply(&T::Cancel),
            Err(WorkItemError::Terminal("accepted"))
        );
    }

    #[test]
    fn rejected_items_can_be_claimed_again_and_blocked_items_reset() {
        use WorkItemState::*;
        use WorkItemTransition as T;
        let rejected = Rejected {
            evidence: vec!["nope".into()],
        };
        assert!(rejected.apply(&T::Claim { by: pk() }).is_ok());
        let blocked = Blocked {
            by: pk(),
            reason: "x".into(),
        };
        assert_eq!(blocked.apply(&T::Reset), Ok(Open));
    }

    #[test]
    fn illegal_moves_are_named_not_performed() {
        use WorkItemState::*;
        use WorkItemTransition as T;
        assert_eq!(
            Open.apply(&T::Accept),
            Err(WorkItemError::Illegal {
                from: "open",
                transition: "accept"
            })
        );
        assert!(Open.apply(&T::Start).is_err());
        assert!(Cancelled.apply(&T::Claim { by: pk() }).is_err());
    }

    #[test]
    fn cancel_is_legal_from_every_live_state() {
        use WorkItemState::*;
        use WorkItemTransition as T;
        for s in [
            Open,
            Claimed { by: pk() },
            InProgress { by: pk() },
            Blocked {
                by: pk(),
                reason: "r".into(),
            },
            Review { by: pk() },
            Rejected { evidence: vec![] },
        ] {
            assert_eq!(s.apply(&T::Cancel), Ok(Cancelled));
        }
    }

    // added by the coverage pass: workitem.rs

    #[test]
    fn every_transition_and_state_has_its_wire_word_and_a_release_and_a_rejection_move_as_the_table_says(
    ) {
        let words = [
            (WorkItemTransition::Claim { by: pk() }, "claim"),
            (WorkItemTransition::Start, "start"),
            (WorkItemTransition::Release, "release"),
            (WorkItemTransition::Block { reason: "r".into() }, "block"),
            (WorkItemTransition::Unblock, "unblock"),
            (WorkItemTransition::Reset, "reset"),
            (WorkItemTransition::Submit, "submit"),
            (WorkItemTransition::Accept, "accept"),
            (WorkItemTransition::Reject { evidence: vec![] }, "reject"),
            (WorkItemTransition::Cancel, "cancel"),
        ];
        for (t, word) in &words {
            assert_eq!(t.name(), *word);
        }
        assert_eq!(
            WorkItemState::Rejected { evidence: vec![] }.as_str(),
            "rejected"
        );
        let claimed = WorkItemState::Claimed { by: pk() };
        assert_eq!(
            claimed.apply(&WorkItemTransition::Release).unwrap(),
            WorkItemState::Open
        );
        let working = WorkItemState::InProgress { by: pk() };
        assert_eq!(
            working.apply(&WorkItemTransition::Release).unwrap(),
            WorkItemState::Open
        );
        let review = WorkItemState::Review { by: pk() };
        assert_eq!(
            review
                .apply(&WorkItemTransition::Reject {
                    evidence: vec!["flaky".into()]
                })
                .unwrap(),
            WorkItemState::Rejected {
                evidence: vec!["flaky".into()]
            }
        );
    }

    // added by the coverage pass: b5-workitem.rs
    #[test]
    fn a_spec_without_a_tier_writes_and_the_unsettled_states_say_their_words() {
        let mut json = serde_json::to_value(spec()).unwrap();
        json.as_object_mut().unwrap().remove("tier_ceiling");
        let read: WorkItemSpec = serde_json::from_value(json).unwrap();
        assert_eq!(read.tier_ceiling, ToolTier::Write);
        let by = pk();
        let unsettled = [
            WorkItemState::Claimed { by: by.clone() },
            WorkItemState::InProgress { by: by.clone() },
            WorkItemState::Blocked {
                by: by.clone(),
                reason: "a gate".into(),
            },
            WorkItemState::Review { by },
        ];
        for state in &unsettled {
            assert!(state.is_unsettled(), "{}", state.as_str());
        }
        assert_eq!(
            unsettled
                .iter()
                .map(WorkItemState::as_str)
                .collect::<Vec<_>>(),
            ["claimed", "in_progress", "blocked", "review"]
        );
        assert!(!WorkItemState::Open.is_unsettled());
        assert!(!WorkItemState::Accepted.is_unsettled());
    }
}
