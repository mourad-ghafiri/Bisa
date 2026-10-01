//! `Team`: a named set of agents and humans, expanded one level at assignment
//! time. A team cannot contain another team.
//!
//! Teams have `enabled`: a disabled team is out of the addressing
//! directory, out of every roster and not expanded by the assignment union —
//! but not deleted, keeps its members, and comes back unchanged.

use crate::assignee::Assignee;
use crate::id::TeamId;
use crate::origin::Origin;
use crate::tags::Tags;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Team {
    pub id: TeamId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    /// The team's picture — an attachment this machine holds (ide/14 §Photos). Display only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub photo: Option<crate::attachment::AttachmentRef>,
    /// Agents and humans. Never a team.
    #[serde(default)]
    pub members: Vec<Assignee>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Tags::is_empty")]
    pub tags: Tags,
    /// Provenance, not a lock: an installed team is yours to edit.
    pub origin: Origin,
    pub created_at: u64,
}

fn default_true() -> bool {
    true
}

impl Team {
    /// A team of teams is refused; one level is the whole model, so no cycle
    /// detection is needed. A core agent is dropped rather than refused: it is
    /// a participant of every team already, and storing it would put it in a
    /// list an edit could remove it from.
    pub fn normalise_members(members: Vec<Assignee>) -> Result<Vec<Assignee>, TeamError> {
        let mut out: Vec<Assignee> = Vec::with_capacity(members.len());
        for m in members {
            match &m {
                Assignee::Team(id) => return Err(TeamError::NestedTeam(id.clone())),
                Assignee::Agent(id) if crate::id::AgentId::is_core_str(id) => continue,
                _ => {}
            }
            if !out.contains(&m) {
                out.push(m);
            }
        }
        Ok(out)
    }

    pub fn validate(&self) -> Result<(), TeamError> {
        if self.name.trim().is_empty() {
            return Err(TeamError::EmptyName);
        }
        for m in &self.members {
            if let Assignee::Team(id) = m {
                return Err(TeamError::NestedTeam(id.clone()));
            }
            if let Assignee::Agent(id) = m {
                if crate::id::AgentId::is_core_str(id) {
                    return Err(TeamError::CoreAgentStored);
                }
            }
        }
        Ok(())
    }

    /// The agents in the work-routing pool.
    pub fn agent_ids(&self) -> impl Iterator<Item = &str> {
        self.members.iter().filter_map(|m| m.as_agent())
    }

    /// The humans who may decide gates.
    pub fn humans(&self) -> impl Iterator<Item = &crate::id::PrincipalId> {
        self.members.iter().filter_map(|m| m.as_human())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TeamError {
    #[error("a team needs a name")]
    EmptyName,
    #[error("a team may not contain another team (`team:{0}`)")]
    NestedTeam(String),
    #[error("a platform agent is a participant of every team and is never stored in one")]
    CoreAgentStored,
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn team(id: &str, enabled: bool) -> Team {
        Team {
            id: TeamId::new(id).unwrap(),
            name: id.into(),
            purpose: None,
            photo: None,
            members: vec![Assignee::Agent("developer".into())],
            enabled,
            tags: Tags::default(),
            origin: Origin::Local,
            created_at: 0,
        }
    }

    #[test]
    fn nesting_is_refused_and_the_core_agents_are_dropped() {
        assert_eq!(
            Team::normalise_members(vec![Assignee::Team("x".into())]),
            Err(TeamError::NestedTeam("x".into()))
        );
        let got = Team::normalise_members(vec![
            Assignee::Agent("general-agent".into()),
            Assignee::Agent("workflow-agent".into()),
            Assignee::Agent("developer".into()),
            Assignee::Agent("developer".into()),
        ])
        .unwrap();
        assert_eq!(got, vec![Assignee::Agent("developer".into())]);
        for core in crate::id::AgentId::CORE {
            let mut t = team("eng", true);
            t.members.push(Assignee::Agent(core.into()));
            assert_eq!(t.validate(), Err(TeamError::CoreAgentStored), "{core}");
        }
    }

    #[test]
    fn enabled_defaults_true_and_roundtrips() {
        let t = team("eng", true);
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(serde_json::from_str::<Team>(&json).unwrap(), t);
        let v: Team = serde_json::from_str(
            r#"{"id":"x","name":"X","members":[],"origin":"local","created_at":0}"#,
        )
        .unwrap();
        assert!(v.enabled);
        assert_eq!(t.agent_ids().collect::<Vec<_>>(), vec!["developer"]);
    }

    #[test]
    fn a_team_needs_a_name_and_splits_its_members_into_agents_and_humans() {
        let human = crate::id::PrincipalId::new("ab".repeat(32)).unwrap();
        let mut t = team("eng", true);
        t.members = Team::normalise_members(vec![
            Assignee::Agent("developer".into()),
            Assignee::Human(human.clone()),
            Assignee::Human(human.clone()),
            Assignee::Agent("reviewer".into()),
        ])
        .unwrap();
        assert_eq!(t.members.len(), 3, "a human listed twice is one member");
        assert_eq!(
            t.agent_ids().collect::<Vec<_>>(),
            vec!["developer", "reviewer"]
        );
        assert_eq!(t.humans().collect::<Vec<_>>(), vec![&human]);
        assert_eq!(t.validate(), Ok(()));
        t.name = "   ".into();
        assert_eq!(t.validate(), Err(TeamError::EmptyName));
        t.name = "Eng".into();
        t.members.push(Assignee::Team("ops".into()));
        assert_eq!(t.validate(), Err(TeamError::NestedTeam("ops".into())));
        assert_eq!(
            Team::normalise_members(vec![]).unwrap(),
            vec![],
            "an empty team is a team"
        );
    }
}
