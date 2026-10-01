//! Gates: the decisions a person is asked to sign.
//!
//! Three, and each is a different question. `Approval` is a workflow's
//! `approval` step — and the adoption or amendment of a workflow itself —
//! decided under the workspace's `approval` policy. `Escalation` is a question
//! raised mid-run: a `human` step, a permission, a budget top-up. `Publish`
//! guards an outward action that cannot be taken back — pushing a branch,
//! opening or merging a pull request — and alone never defers to an
//! assignment, because approving it spends the owner's credentials against a
//! remote the owner is accountable for.
//!
//! There is no gate on *moving* a goal any more: a goal has no lifecycle to
//! move through, only a run whose steps say when a person is needed.

use serde::{Deserialize, Serialize};

/// Any gate a person may be asked to decide. The union the journal, the inbox
/// and the governance policy are keyed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Gate {
    /// An `approval` step, an adoption or an amendment of a workflow.
    Approval,
    /// A question raised from inside a run.
    Escalation,
    /// An outward action that cannot be taken back.
    Publish,
}

impl Gate {
    pub const ALL: [Gate; 3] = [Gate::Approval, Gate::Escalation, Gate::Publish];

    pub fn as_str(self) -> &'static str {
        match self {
            Gate::Approval => "approval",
            Gate::Escalation => "escalation",
            Gate::Publish => "publish",
        }
    }

    /// `Publish` alone never defers to an assignment: approving it spends the
    /// owner's credentials against a remote the owner is accountable for.
    pub fn defers_to_assignment(self) -> bool {
        !matches!(self, Gate::Publish)
    }
}

impl std::str::FromStr for Gate {
    type Err = crate::CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Gate::ALL
            .into_iter()
            .find(|g| g.as_str() == s)
            .ok_or_else(|| crate::CoreError::UnknownGate(s.to_string()))
    }
}

impl std::fmt::Display for Gate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The journal event id of the signed `kind:3401` decision that authorises a
/// gated move. Carried on the [`crate::run::RunEvent::Decided`] that an
/// `approval` step completes with, so the run and the decision that moved it
/// are joined by an id and not by trust.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct ApprovalId(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gates_round_trip_and_only_publish_never_defers() {
        for g in Gate::ALL {
            assert_eq!(g.as_str().parse::<Gate>().unwrap(), g);
            assert_eq!(
                serde_json::to_string(&g).unwrap(),
                format!("\"{}\"", g.as_str())
            );
            assert_eq!(g.defers_to_assignment(), g != Gate::Publish);
        }
        assert!(
            "commit".parse::<Gate>().is_err(),
            "commit is not a gate any more"
        );
        assert!("acceptance".parse::<Gate>().is_err());
        assert_eq!(Gate::ALL.len(), 3);
    }
}
