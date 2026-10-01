//! The core agents that hold a record: `general-agent` and
//! `workflow-agent`, the two a fresh workspace has on disk.
//!
//! Everything else is installed from the [catalog](crate::catalog) because
//! somebody chose it. These two are not a choice: always present, always
//! enabled, never removable, their names fixed, only their harness and model
//! plan editable. The rules live in [`bisa_core::Agent`]; this module
//! only ensures the objects exist.
//!
//! **They are created in exactly one place**: [`Workspace::open_with_keystore`]
//! calls [`Workspace::ensure_core_agents`], and nothing else does. A workspace
//! that is open has both, by construction.
//!
//! Their definitions are `library/core/general-agent.toml` and
//! `library/core/workflow-agent.toml`, compiled in — the catalog's
//! own agent shape minus `skills` and `tags`, so they parse with the catalog's
//! parser rather than a second one that could drift from it.
//!
//! The third core agent is not made here. The Decision-Making Agent holds no
//! record, so there is nothing to ensure, and its file
//! (`library/core/decision-making-agent.toml`) has a shape of its own that the
//! catalog's parser refuses: see [`crate::decision_making_agent`].

use crate::catalog::parse_agent;
use crate::error::StoreError;
use crate::workspace::Workspace;
use bisa_core::{AgentId, AgentOrigin, GENERAL_AGENT_NAME, WORKFLOW_AGENT_NAME};

/// The general agent's definition, compiled into the binary like every
/// catalog entry.
pub(crate) const GENERAL_AGENT_TOML: &str =
    include_str!("../../../library/core/general-agent.toml");

/// The workflow agent's definition.
pub(crate) const WORKFLOW_AGENT_TOML: &str =
    include_str!("../../../library/core/workflow-agent.toml");

/// `(id, fixed name, definition)` — the order is [`AgentId::CORE`]'s, which is
/// also the order the addressing directory lists them in.
pub(crate) const CORE_AGENTS: [(&str, &str, &str); 2] = [
    (AgentId::GENERAL, GENERAL_AGENT_NAME, GENERAL_AGENT_TOML),
    (AgentId::WORKFLOW, WORKFLOW_AGENT_NAME, WORKFLOW_AGENT_TOML),
];

impl Workspace {
    /// Create each core agent this workspace does not have, and leave the
    /// ones it has completely alone — a harness swap must survive every
    /// subsequent open.
    ///
    /// Presence is decided on the truth file rather than on a successful
    /// parse: a file that failed to parse must not be written over with a
    /// fresh keypair, and must not fail a workspace open either.
    pub fn ensure_core_agents(&self) -> Result<(), StoreError> {
        for (id, name, toml_str) in CORE_AGENTS {
            let id = AgentId::new(id)?;
            if self.agent_file_exists(&id) {
                continue;
            }
            let body = parse_agent(id.as_str(), toml_str)?;
            let mut new = body.into_new_agent();
            new.name = name.to_string();
            self.add_agent_with_id(&id, new, AgentOrigin::Core)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Workflow Agent's method is stated once, in its definition: where
    /// a step's outcome lands, that a project is made only by
    /// `create_project`, that a finished run is repaired by a new proposal
    /// — and no ritual of fetching what its wake already carries.
    #[test]
    fn the_workflow_agent_is_told_the_placement_rule_and_no_fetch_ritual() {
        let prompt = parse_agent(AgentId::WORKFLOW, WORKFLOW_AGENT_TOML)
            .unwrap()
            .system_prompt;
        for said in [
            "names no project and runs in",
            "create_project",
            "a finished run is not amended",
            "Nothing the goal did not ask for",
            "TEMPLATES",
        ] {
            assert!(prompt.contains(said), "{said}: {prompt}");
        }
        for gone in [
            "workspace_overview first",
            "the engine makes",
            "born of that step",
        ] {
            assert!(!prompt.contains(gone), "{gone} is back: {prompt}");
        }
    }

    /// The General Agent makes a project only for files, never for an answer.
    #[test]
    fn the_general_agent_never_makes_a_project_for_an_answer() {
        let prompt = parse_agent(AgentId::GENERAL, GENERAL_AGENT_TOML)
            .unwrap()
            .system_prompt;
        assert!(prompt.contains("only when it needs one"), "{prompt}");
        assert!(
            prompt.contains("the only way a project is made"),
            "{prompt}"
        );
        assert!(!prompt.contains("the engine creates"), "{prompt}");
    }

    #[test]
    fn every_core_definition_parses_and_carries_no_skills_or_tags() {
        for (id, _, toml_str) in CORE_AGENTS {
            let body = parse_agent(id, toml_str).unwrap();
            assert!(!body.name.trim().is_empty(), "{id}");
            assert!(!body.system_prompt.trim().is_empty(), "{id}");
            assert!(
                body.skills.is_empty(),
                "{id}: a core agent carries no skills"
            );
            assert!(body.tags.is_empty(), "{id}: a core agent carries no tags");
        }
    }

    #[test]
    fn every_core_definition_has_a_primary_and_a_fallback_model() {
        for (id, _, toml_str) in CORE_AGENTS {
            let body = parse_agent(id, toml_str).unwrap();
            let plan = body.into_new_agent().models;
            let order = plan.order(&bisa_core::AllHealthy, 0, None);
            assert!(
                order.len() >= 2,
                "{id}: needs a fallback model, got {order:?}"
            );
        }
    }

    #[test]
    fn both_core_agents_respond_to_members() {
        for (id, _, toml_str) in CORE_AGENTS {
            let body = parse_agent(id, toml_str).unwrap();
            assert_eq!(
                body.respond,
                bisa_core::RespondPolicy::Members,
                "{id}: a platform agent answers every member"
            );
        }
    }

    #[test]
    fn core_agent_names_match_the_core_constants() {
        assert_eq!(CORE_AGENTS.len(), AgentId::CORE.len());
        for ((id, name, toml_str), core) in CORE_AGENTS.iter().zip(AgentId::CORE) {
            assert_eq!(*id, core);
            let aid = AgentId::new(*id).unwrap();
            assert_eq!(bisa_core::Agent::core_name(&aid), Some(*name));
            assert_eq!(parse_agent(id, toml_str).unwrap().name, *name);
        }
    }
}
