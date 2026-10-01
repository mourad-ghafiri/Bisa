//! `Home`: where a run's truth is filed.
//!
//! A goal's run is filed with its goal: the goal's journal holds the run's
//! facts, the goal's `state/` its snapshot and its work items, the goal's
//! ledger its spend, the goal's `scratch/` what a step with no project
//! leaves. A run of the workspace has no goal to be filed with, so it is its
//! own home — a folder of the same shape under `workflows/runs/<RunId>/`.
//! Every fact a run, its work items, its gates and its sessions write names
//! the home it lands in; the goal's own facts — notes, documents,
//! attachments, guidance, an adoption — are always a goal's.

use crate::id::{GoalId, RunId};
use serde::{Deserialize, Serialize};

/// The record whose folder files a run's truth: its goal, or — for a run of
/// the workspace — the run itself. Recorded nowhere as a choice: a run's
/// home follows from its scope ([`crate::WorkflowRun::home`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "home")]
pub enum Home {
    Goal { goal: GoalId },
    Run { run: RunId },
}

impl Home {
    /// Every kind — the words the wire and the index share.
    pub const KINDS: [&'static str; 2] = ["goal", "run"];

    pub fn kind(&self) -> &'static str {
        match self {
            Home::Goal { .. } => "goal",
            Home::Run { .. } => "run",
        }
    }

    /// The goal, when the home is one.
    pub fn goal(&self) -> Option<GoalId> {
        match self {
            Home::Goal { goal } => Some(*goal),
            Home::Run { .. } => None,
        }
    }

    /// The run, when the home is a run of the workspace.
    pub fn run(&self) -> Option<RunId> {
        match self {
            Home::Goal { .. } => None,
            Home::Run { run } => Some(*run),
        }
    }

    /// The record's id as text: the goal's ULID, or the run's.
    pub fn id(&self) -> String {
        match self {
            Home::Goal { goal } => goal.to_string(),
            Home::Run { run } => run.to_string(),
        }
    }
}

impl From<GoalId> for Home {
    fn from(goal: GoalId) -> Self {
        Home::Goal { goal }
    }
}

/// `goal:<id>` or `run:<id>` — how a log line and a remembered answer name
/// a home.
impl std::fmt::Display for Home {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.kind(), self.id())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goal() -> GoalId {
        GoalId::from_ulid(ulid::Ulid::from_parts(3, 1))
    }

    fn run() -> RunId {
        RunId::from_ulid(ulid::Ulid::from_parts(3, 2))
    }

    #[test]
    fn a_home_names_its_record_and_round_trips() {
        let homes = [Home::Goal { goal: goal() }, Home::Run { run: run() }];
        for (home, kind) in homes.iter().zip(Home::KINDS) {
            assert_eq!(home.kind(), kind);
            let wire = serde_json::to_value(home).unwrap();
            assert_eq!(wire["home"], kind, "tagged by its kind");
            assert_eq!(serde_json::from_value::<Home>(wire).unwrap(), *home);
        }
        assert_eq!(homes[0].goal(), Some(goal()));
        assert_eq!(homes[0].run(), None);
        assert_eq!(homes[1].goal(), None, "a run of the workspace has no goal");
        assert_eq!(homes[1].run(), Some(run()));
        assert_eq!(homes[1].id(), run().to_string());
        assert_eq!(Home::from(goal()), homes[0]);
        assert_eq!(homes[0].to_string(), format!("goal:{}", goal()));
        assert_eq!(homes[1].to_string(), format!("run:{}", run()));
    }
}
