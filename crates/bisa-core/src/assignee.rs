//! `Assignee`: one word for "who can be given work".
//!
//! Goals, projects and workflow steps all need to name a worker or a
//! decider. Modelling agents, humans and teams as one sum type is what lets
//! assignment resolve in a single place (`workers()` / `approvers()`, Wave 2)
//! instead of every surface re-implementing the union.
//!
//! The compact wire form (`agent:<id>`, `human:<hex>`, `team:<id>`) exists
//! because two callers need assignees as flat strings: the CLI, and
//! governance's `Listed` policy, which has always written `team:<id>`. Keeping
//! one grammar means those two never drift apart.

use crate::id::PrincipalId;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// A principal-shaped target for work or approval.
///
/// Serialization matches the store's `TeamMember` exactly (`{"agent": "id"}`,
/// `{"human": "<hex>"}`), so a team's member list and an assignee list are the
/// same bytes on the wire.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Assignee {
    /// An `AgentDef` id. Takes work.
    Agent(String),
    /// A human principal. Decides gates and answers questions; a human-only
    /// assignment never blocks execution.
    Human(PrincipalId),
    /// A `TeamDef` id, expanded to its members at resolution time. Nesting is
    /// not modelled: one level needs no cycle detection.
    Team(String),
}

impl Assignee {
    pub fn as_agent(&self) -> Option<&str> {
        match self {
            Assignee::Agent(id) => Some(id),
            _ => None,
        }
    }

    pub fn as_human(&self) -> Option<&PrincipalId> {
        match self {
            Assignee::Human(pk) => Some(pk),
            _ => None,
        }
    }
}

impl fmt::Display for Assignee {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Assignee::Agent(id) => write!(f, "agent:{id}"),
            Assignee::Human(pk) => write!(f, "human:{pk}"),
            Assignee::Team(id) => write!(f, "team:{id}"),
        }
    }
}

impl FromStr for Assignee {
    type Err = crate::CoreError;

    /// Accepts exactly `agent:<id>`, `human:<64 hex>` and `team:<id>`.
    /// Anything else is rejected: a bare id would be ambiguous between the
    /// three kinds, and silently guessing is how assignment surfaces drift.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || crate::CoreError::InvalidAssignee(s.to_string());
        let (prefix, rest) = s.split_once(':').ok_or_else(invalid)?;
        if rest.is_empty() {
            return Err(invalid());
        }
        match prefix {
            "agent" => Ok(Assignee::Agent(rest.to_string())),
            "team" => Ok(Assignee::Team(rest.to_string())),
            "human" => Ok(Assignee::Human(
                PrincipalId::new(rest).map_err(|_| invalid())?,
            )),
            _ => Err(invalid()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex() -> String {
        "ab".repeat(32)
    }

    #[test]
    fn wire_form_roundtrips() {
        let cases = [
            Assignee::Agent("developer".into()),
            Assignee::Team("01TEAM".into()),
            Assignee::Human(PrincipalId::new(hex()).unwrap()),
        ];
        for a in cases {
            let s = a.to_string();
            assert_eq!(s.parse::<Assignee>().unwrap(), a, "{s}");
        }
    }

    #[test]
    fn wire_form_is_the_governance_grammar() {
        // governance's `Listed` policy has always spelled a team this way.
        assert_eq!(Assignee::Team("01T".into()).to_string(), "team:01T");
    }

    #[test]
    fn rejects_anything_outside_the_grammar() {
        for bad in [
            "",
            "developer",     // no prefix: ambiguous
            "agent",         // no separator
            "agent:",        // empty id
            "human:",        // empty id
            "robot:x",       // unknown prefix
            "Agent:x",       // case-sensitive
            " agent:x",      // no trimming
            "human:not-hex", // bad principal
            "human:ABCDEF",  // too short and uppercase
        ] {
            assert!(
                bad.parse::<Assignee>().is_err(),
                "{bad:?} should be refused"
            );
        }
        // Uppercase hex is refused by PrincipalId, not silently lowercased.
        assert!(format!("human:{}", "AB".repeat(32))
            .parse::<Assignee>()
            .is_err());
    }

    #[test]
    fn colons_in_the_id_survive() {
        // Only the first colon separates; ids are otherwise opaque.
        let a: Assignee = "agent:acp:goose".parse().unwrap();
        assert_eq!(a, Assignee::Agent("acp:goose".into()));
        assert_eq!(a.to_string(), "agent:acp:goose");
    }

    #[test]
    fn json_matches_team_member_shape() {
        let a = Assignee::Agent("x".into());
        assert_eq!(serde_json::to_string(&a).unwrap(), r#"{"agent":"x"}"#);
        let h = Assignee::Human(PrincipalId::new(hex()).unwrap());
        assert_eq!(
            serde_json::to_string(&h).unwrap(),
            format!(r#"{{"human":"{}"}}"#, hex())
        );
    }
}
