//! The Decision-Making Agent, `decision-making-agent`: the third core agent,
//! and the one that holds no record.
//!
//! There are three core agents, always here and never removed: the General
//! Agent staffs and delegates, the Workflow Agent designs and repairs, the
//! Decision-Making Agent judges. The first two hold a record — a key, a
//! prompt, a conversation, a place in every room — which
//! [`crate::core_agents`] ensures at every open. This one holds none: a
//! decision point asks it a typed question and gets numbers back.
//!
//! Its definition is `library/core/decision-making-agent.toml`, compiled in
//! like theirs, in a shape of its own: it has no prompt to carry, so the
//! catalog's agent parser refuses it. There is nothing to ensure on disk: it
//! has no key, no truth file and no row — who answers for it is the
//! `decisions.*` settings, whose registry defaults are what the definition
//! says, and a test here holds the two together. What this module gives a
//! surface is the agent as it is listed: its reserved id, its fixed name, what
//! it is, and what it is out of the box.

use crate::error::StoreError;
use bisa_core::{AgentId, DecisionProviderKind, Effort, DECISION_MAKING_AGENT_NAME};
use serde::{Deserialize, Serialize};

pub(crate) const DECISION_MAKING_AGENT_TOML: &str =
    include_str!("../../../library/core/decision-making-agent.toml");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentFile {
    agent: AgentBody,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentBody {
    name: String,
    description: String,
    provider: DecisionProviderKind,
    harness: String,
    model: String,
    effort: Effort,
}

/// The Decision-Making Agent as a surface lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct DecisionMakingAgent {
    /// The reserved id, `decision-making-agent`.
    pub id: String,
    pub name: String,
    pub description: String,
    /// What answers out of the box, before any setting is written.
    pub default_provider: DecisionProviderKind,
    pub default_harness: String,
    pub default_model: String,
    /// How hard that model works on a judgement: a level, never `auto`.
    pub default_effort: Effort,
}

/// The Decision-Making Agent's definition, read from the compiled-in file.
pub fn decision_making_agent() -> Result<DecisionMakingAgent, StoreError> {
    let file: AgentFile = toml::from_str(DECISION_MAKING_AGENT_TOML).map_err(|e| {
        // LCOV_EXCL_START: the compiled-in file parses and names the agent: the_definition_and_the_settings_registry_say_the_same holds it
        StoreError::Invalid(bisa_core::text!(
            "error-store-invalid-library-core-decision-making-agent-toml",
            e = e.to_string()
        ))
    })?;
    // LCOV_EXCL_STOP
    if file.agent.name != DECISION_MAKING_AGENT_NAME {
        // LCOV_EXCL_START: the compiled-in file parses and names the agent: the_definition_and_the_settings_registry_say_the_same holds it
        return Err(StoreError::Invalid(bisa_core::text!(
            "error-store-invalid-decision-making-agent-s-name-fixed",
            name = (DECISION_MAKING_AGENT_NAME).to_string()
        )));
        // LCOV_EXCL_STOP
    }
    Ok(DecisionMakingAgent {
        id: AgentId::DECISION_MAKING.to_string(),
        name: file.agent.name,
        description: file.agent.description,
        default_provider: file.agent.provider,
        default_harness: file.agent.harness,
        default_model: file.agent.model,
        default_effort: file.agent.effort,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_core::settings::SettingDef;
    use serde_json::json;

    #[test]
    fn the_definition_and_the_settings_registry_say_the_same() {
        let agent = decision_making_agent().unwrap();
        assert_eq!(agent.id, "decision-making-agent");
        assert_eq!(agent.name, "Decision-Making Agent");
        assert!(!agent.description.trim().is_empty());
        let default = |key: &str| SettingDef::lookup(key).unwrap().default.clone();
        assert_eq!(
            default("decisions.provider"),
            json!(agent.default_provider.as_str())
        );
        assert_eq!(
            default("decisions.harness.id"),
            json!(agent.default_harness)
        );
        assert_eq!(
            default("decisions.harness.model"),
            json!(agent.default_model)
        );
        assert_eq!(
            default("decisions.harness.effort"),
            json!(agent.default_effort.as_str())
        );
        // Claude Code with Claude Sonnet 5.5 at `high`, and off until
        // somebody says so.
        assert_eq!(agent.default_provider, DecisionProviderKind::Harness);
        assert_eq!(agent.default_harness, "claude-code");
        assert_eq!(agent.default_model, "claude-sonnet-5-5[1m]");
        assert_eq!(agent.default_effort, Effort::High);
        assert_eq!(default("decisions.enabled"), json!(false));
        // The classifier, when a harness reads for it, runs the same.
        assert_eq!(
            default("security.classifier.model"),
            json!(agent.default_model)
        );
        assert_eq!(
            default("security.classifier.effort"),
            json!(agent.default_effort.as_str())
        );
    }

    #[test]
    fn its_effort_is_a_level_and_never_the_judges_own_to_name() {
        let asking_itself =
            DECISION_MAKING_AGENT_TOML.replace("effort = \"high\"", "effort = \"auto\"");
        assert_ne!(asking_itself, DECISION_MAKING_AGENT_TOML);
        assert!(toml::from_str::<AgentFile>(&asking_itself).is_err());
        let without = DECISION_MAKING_AGENT_TOML.replace("effort = \"high\"\n", "");
        assert!(
            toml::from_str::<AgentFile>(&without).is_err(),
            "the definition says its effort"
        );
        assert_eq!(
            serde_json::to_value(decision_making_agent().unwrap()).unwrap()["default_effort"],
            json!("high")
        );
    }

    #[test]
    fn its_id_is_no_agent_records_to_take() {
        let id = AgentId::decision_making();
        assert!(id.is_decision_making());
        assert!(
            !id.is_core_id(),
            "it is reserved, and it holds no record to ensure"
        );
    }

    /// Its file is a shape of its own — who answers out of the box, and no
    /// prompt — so the parser the other two core files share has nothing to
    /// make an agent record from.
    #[test]
    fn the_catalogs_agent_parser_refuses_its_file_for_it_is_no_catalog_agent() {
        let refused =
            crate::catalog::parse_agent(AgentId::DECISION_MAKING, DECISION_MAKING_AGENT_TOML);
        assert!(matches!(refused, Err(StoreError::Invalid(_))));
    }

    #[test]
    fn its_file_takes_no_field_it_does_not_know() {
        let with_a_prompt = format!("{DECISION_MAKING_AGENT_TOML}system_prompt = \"Judge.\"\n");
        assert!(toml::from_str::<AgentFile>(&with_a_prompt).is_err());
        let with_a_table = format!("{DECISION_MAKING_AGENT_TOML}\n[model]\nname = \"x\"\n");
        assert!(toml::from_str::<AgentFile>(&with_a_table).is_err());
    }
}
