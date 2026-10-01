//! `Agent`: a definition, not a process. A prompt bound to a harness, a model
//! plan, skills and MCP servers, with its own attested keypair.
//!
//! Three core agents are the platform's own, always here and never removed:
//! the General Agent staffs and delegates, the Workflow Agent designs and
//! repairs, the Decision-Making Agent judges. The first two hold a record —
//! `general-agent` and `workflow-agent` ([`crate::id::AgentId::CORE`]) —
//! ensured at every workspace open, never disableable, their names fixed, only
//! their harness, their model plan and their decision-making switch editable.
//! The third holds none: a decision point asks it a typed question and gets
//! numbers back, and who answers for it is the `decisions.*` settings. Its id,
//! [`crate::id::AgentId::DECISION_MAKING`], is reserved and no agent record may
//! take it. The rules live here, as pure functions, so they can be unit-tested
//! without a workspace on disk.

use crate::id::{AgentId, McpId, PrincipalId, SkillId};
use crate::model_plan::ModelPlan;
use crate::origin::AgentOrigin;
use crate::tags::Tags;
use serde::{Deserialize, Serialize};

/// Who an agent will act for.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RespondPolicy {
    /// Acts only on input from its attested owner (fail-closed default).
    #[default]
    OwnerOnly,
    /// Acts on input from any workspace member.
    Members,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Agent {
    /// The id is the slug.
    pub id: AgentId,
    pub name: String,
    /// The agent's picture — an attachment this machine holds, scaled by the
    /// desktop, served by `GET /attachments/{sha256}?as=image` (ide/14
    /// §Photos). Display only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub photo: Option<crate::attachment::AttachmentRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub system_prompt: String,
    /// Harness id this agent runs on.
    pub harness: String,
    /// Ordered model candidates plus a strategy. Empty means "whatever the
    /// harness runs by default".
    #[serde(default)]
    pub models: ModelPlan,
    /// Skill library ids, resolved at launch. A reference that no longer
    /// resolves costs that skill and nothing else.
    #[serde(default)]
    pub skills: Vec<SkillId>,
    /// MCP registry ids, resolved at launch. Local; never synced.
    #[serde(default)]
    pub mcps: Vec<McpId>,
    #[serde(default, skip_serializing_if = "Tags::is_empty")]
    pub tags: Tags,
    #[serde(default)]
    pub respond: RespondPolicy,
    /// Whether the Decision-Making Agent stands in at the decision points
    /// this agent reaches — the pick of a pool it is in, the triage that may
    /// wake it, its own `decide` tool — when the workspace has not switched it
    /// on for all.
    #[serde(default)]
    pub decision_making: bool,
    /// The agent's own pubkey; the secret lives in the keystore.
    pub pubkey: PrincipalId,
    /// Where this definition came from. No serde default: provenance that can
    /// be omitted is provenance that can be laundered.
    pub origin: AgentOrigin,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub created_at: u64,
}

fn default_true() -> bool {
    true
}

/// The display name of the platform's general agent, and its mention token.
pub const GENERAL_AGENT_NAME: &str = "General Agent";
/// The display name of the platform's workflow designer, and its mention token.
pub const WORKFLOW_AGENT_NAME: &str = "Workflow Agent";
/// The display name of the platform's judge: what a person reads where the
/// other two core agents read theirs. It holds no record, so it has no mention
/// token.
pub const DECISION_MAKING_AGENT_NAME: &str = "Decision-Making Agent";

impl Agent {
    pub fn is_core(&self) -> bool {
        self.origin.is_core()
    }

    /// The fixed display name of a core id, if it is one.
    pub fn core_name(id: &AgentId) -> Option<&'static str> {
        if id.is_general() {
            Some(GENERAL_AGENT_NAME)
        } else if id.is_workflow() {
            Some(WORKFLOW_AGENT_NAME)
        } else {
            None
        }
    }

    /// The invariants a definition must hold to be stored.
    pub fn validate(&self) -> Result<(), AgentError> {
        if self.name.trim().is_empty() {
            return Err(AgentError::EmptyName);
        }
        if self.harness.trim().is_empty() {
            return Err(AgentError::EmptyHarness);
        }
        if self.id.is_decision_making() {
            return Err(AgentError::DecisionMakingAgentIdReserved);
        }
        match (&self.origin, self.id.is_core_id()) {
            (AgentOrigin::Core, false) => return Err(AgentError::CoreIdReserved),
            (o, true) if !o.is_core() => return Err(AgentError::CoreIdReserved),
            _ => {}
        }
        if self.is_core() {
            if !self.enabled {
                return Err(AgentError::CoreCannotBeDisabled);
            }
            if Self::core_name(&self.id) != Some(self.name.as_str()) {
                return Err(AgentError::CoreNameFixed);
            }
        }
        Ok(())
    }

    /// What an update to this agent may change.
    ///
    /// Any agent may change anything but its id, its pubkey, its origin and its
    /// `created_at`. A core agent may change **only** its harness, its model
    /// plan and its decision-making switch; every other differing field is
    /// refused **by name** rather than silently restored, because a silent
    /// restore is how a UI ends up reporting a save the store threw away. A
    /// field that is equal was not changed, so a client that PATCHes the whole
    /// object back still succeeds.
    pub fn check_update(&self, next: &Agent) -> Result<(), AgentError> {
        if next.id != self.id {
            return Err(AgentError::Immutable("id"));
        }
        if next.pubkey != self.pubkey {
            return Err(AgentError::Immutable("pubkey"));
        }
        if next.origin != self.origin {
            return Err(AgentError::Immutable("origin"));
        }
        if next.created_at != self.created_at {
            return Err(AgentError::Immutable("created_at"));
        }
        if !self.is_core() {
            return Ok(());
        }
        if !next.enabled {
            return Err(AgentError::CoreCannotBeDisabled);
        }
        let changed: &[(&str, bool)] = &[
            ("name", next.name != self.name),
            ("photo", next.photo != self.photo),
            ("description", next.description != self.description),
            ("system_prompt", next.system_prompt != self.system_prompt),
            ("skills", next.skills != self.skills),
            ("mcps", next.mcps != self.mcps),
            ("tags", next.tags != self.tags),
            ("respond", next.respond != self.respond),
        ];
        if let Some((field, _)) = changed.iter().find(|(_, c)| *c) {
            return Err(AgentError::CoreFieldFixed(field));
        }
        Ok(())
    }

    /// Whether this agent may be removed at all — before anything asks what
    /// points at it.
    pub fn check_remove(&self) -> Result<(), AgentError> {
        if self.is_core() {
            return Err(AgentError::CoreCannotBeRemoved);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AgentError {
    #[error("an agent needs a name")]
    EmptyName,
    #[error("an agent needs a harness")]
    EmptyHarness,
    #[error(
        "the core ids (`general-agent`, `workflow-agent`) and the core origin belong to \
         each other and to nothing else"
    )]
    CoreIdReserved,
    #[error(
        "`decision-making-agent` is the Decision-Making Agent's id, and no agent record may \
         take it"
    )]
    DecisionMakingAgentIdReserved,
    #[error("a platform agent (`general-agent`, `workflow-agent`) cannot be disabled")]
    CoreCannotBeDisabled,
    #[error("a platform agent (`general-agent`, `workflow-agent`) cannot be removed")]
    CoreCannotBeRemoved,
    #[error("a platform agent's name is fixed: `General Agent` and `Workflow Agent`")]
    CoreNameFixed,
    #[error(
        "only its harness, its model plan and its decision-making switch are editable, and this \
         update changes `{0}`"
    )]
    CoreFieldFixed(&'static str),
    #[error("`{0}` is immutable")]
    Immutable(&'static str),
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn agent(id: &str, enabled: bool) -> Agent {
        let id = AgentId::new(id).unwrap();
        let core = id.is_core_id();
        Agent {
            name: Agent::core_name(&id)
                .map(str::to_string)
                .unwrap_or_else(|| id.to_string()),
            id,
            photo: None,
            description: None,
            system_prompt: "You are helpful.".into(),
            harness: "claude-code".into(),
            models: ModelPlan::default(),
            skills: vec![],
            mcps: vec![],
            tags: Tags::default(),
            respond: RespondPolicy::OwnerOnly,
            decision_making: false,
            pubkey: PrincipalId::new("ab".repeat(32)).unwrap(),
            origin: if core {
                AgentOrigin::Core
            } else {
                AgentOrigin::Local
            },
            enabled,
            created_at: 0,
        }
    }

    #[test]
    fn the_core_agents_keep_their_ids_and_their_origin_together() {
        for id in AgentId::CORE {
            assert!(agent(id, true).validate().is_ok(), "{id}");
            let mut g = agent(id, true);
            g.origin = AgentOrigin::Local;
            assert_eq!(g.validate(), Err(AgentError::CoreIdReserved), "{id}");
            assert_eq!(
                agent(id, false).validate(),
                Err(AgentError::CoreCannotBeDisabled),
                "{id}"
            );
        }
        let mut a = agent("developer", true);
        a.origin = AgentOrigin::Core;
        assert_eq!(a.validate(), Err(AgentError::CoreIdReserved));
    }

    #[test]
    fn a_core_agents_name_is_fixed() {
        assert_eq!(
            Agent::core_name(&AgentId::general()),
            Some(GENERAL_AGENT_NAME)
        );
        assert_eq!(
            Agent::core_name(&AgentId::workflow()),
            Some(WORKFLOW_AGENT_NAME)
        );
        assert_eq!(Agent::core_name(&AgentId::new("developer").unwrap()), None);
        let mut w = agent(AgentId::WORKFLOW, true);
        w.name = "Flow Bot".into();
        assert_eq!(w.validate(), Err(AgentError::CoreNameFixed));
    }

    #[test]
    fn a_core_agent_takes_only_a_harness_or_a_model_plan() {
        for id in AgentId::CORE {
            let g = agent(id, true);
            let mut next = g.clone();
            next.harness = "codex".into();
            next.models = ModelPlan::pinned("opus");
            next.decision_making = true;
            assert!(g.check_update(&next).is_ok(), "{id}");

            // Its effort lives in the plan, so it is editable with it.
            let mut next = g.clone();
            next.models = ModelPlan::fallback(["opus", "sonnet"]).at(crate::EffortChoice::Auto);
            next.models.models[1].effort = Some(crate::EffortChoice::Max);
            assert!(g.check_update(&next).is_ok(), "{id}");

            let mut next = g.clone();
            next.system_prompt = "be evil".into();
            assert_eq!(
                g.check_update(&next),
                Err(AgentError::CoreFieldFixed("system_prompt")),
                "{id}"
            );

            let mut next = g.clone();
            next.enabled = false;
            assert_eq!(
                g.check_update(&next),
                Err(AgentError::CoreCannotBeDisabled),
                "{id}"
            );

            assert_eq!(
                g.check_remove(),
                Err(AgentError::CoreCannotBeRemoved),
                "{id}"
            );
        }
        assert!(agent("developer", true).check_remove().is_ok());
    }

    #[test]
    fn no_agent_takes_the_decision_making_agents_id() {
        let taken = agent(AgentId::DECISION_MAKING, true);
        assert_eq!(
            taken.validate(),
            Err(AgentError::DecisionMakingAgentIdReserved)
        );
        assert!(
            !taken.id.is_core_id(),
            "it is reserved, and it holds no record"
        );
    }

    #[test]
    fn the_reserved_id_is_refused_whatever_origin_the_record_claims() {
        for origin in [
            AgentOrigin::Local,
            AgentOrigin::Core,
            AgentOrigin::Catalog {
                slug: AgentId::DECISION_MAKING.into(),
            },
        ] {
            let mut taken = agent(AgentId::DECISION_MAKING, true);
            taken.name = DECISION_MAKING_AGENT_NAME.into();
            taken.origin = origin.clone();
            assert_eq!(
                taken.validate(),
                Err(AgentError::DecisionMakingAgentIdReserved),
                "{origin:?}"
            );
        }
    }

    #[test]
    fn the_decision_making_agent_has_a_name_and_no_record_to_carry_it() {
        assert_eq!(DECISION_MAKING_AGENT_NAME, "Decision-Making Agent");
        assert_eq!(Agent::core_name(&AgentId::decision_making()), None);
    }

    #[test]
    fn provenance_and_keys_are_immutable_for_everyone() {
        let a = agent("developer", true);
        let mut next = a.clone();
        next.origin = AgentOrigin::Catalog {
            slug: "developer".into(),
        };
        assert_eq!(a.check_update(&next), Err(AgentError::Immutable("origin")));
        let mut next = a.clone();
        next.pubkey = PrincipalId::new("cd".repeat(32)).unwrap();
        assert_eq!(a.check_update(&next), Err(AgentError::Immutable("pubkey")));
        let mut next = a.clone();
        next.system_prompt = "new role".into();
        assert!(
            a.check_update(&next).is_ok(),
            "an ordinary agent may change its prompt"
        );
    }

    #[test]
    fn json_roundtrip_and_no_default_origin() {
        let a = agent("developer", true);
        let json = serde_json::to_string(&a).unwrap();
        assert_eq!(serde_json::from_str::<Agent>(&json).unwrap(), a);
        let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
        v.as_object_mut().unwrap().remove("origin");
        assert!(
            serde_json::from_value::<Agent>(v).is_err(),
            "origin has no default"
        );
    }
}
