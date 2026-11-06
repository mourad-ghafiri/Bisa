//! The core agents that hold a record: `general-agent` and
//! `workflow-agent`, the two a fresh workspace has on disk.
//!
//! Everything else is installed from the [catalog](crate::catalog) because
//! somebody chose it. These two are not a choice: always present, always
//! enabled, never removable, their names fixed, only their harness and model
//! plan editable. The rules live in [`bisa_core::Agent`]; this module
//! ensures the objects exist and carry the shipped words.
//!
//! **They are created in exactly one place**: [`Workspace::open_with_keystore`]
//! calls [`Workspace::ensure_core_agents`], and nothing else does. A workspace
//! that is open has both, by construction — and both say what this release's
//! definition says: the description, the prompt and the respond policy are
//! fixed to a person, so they are the release's to restate at every open,
//! while the harness, the model plan, the decision-making switch, the key and
//! the attestation stay exactly as the workspace has them.
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
    /// Create each core agent this workspace does not have, and restate the
    /// ones it has to the shipped definition: a core agent's description,
    /// prompt and respond policy are the platform's — fixed to a person
    /// ([`bisa_core::Agent::check_update`]), so an older workspace would
    /// otherwise keep an older method forever — while its harness, model
    /// plan, decision-making switch, photo, key, origin, birth and
    /// attestation are kept exactly (`Workspace::restate_core_agent`); a
    /// harness swap survives every open, and a second open writes nothing.
    ///
    /// Presence is decided on the truth file rather than on a successful
    /// parse: a file that failed to parse must not be written over with a
    /// fresh keypair, must not fail a workspace open either, and is not
    /// restated — it is named in the log and left for a person to repair.
    pub fn ensure_core_agents(&self) -> Result<(), StoreError> {
        for (id, name, toml_str) in CORE_AGENTS {
            let id = AgentId::new(id)?;
            let body = parse_agent(id.as_str(), toml_str)?;
            if self.agent_file_exists(&id) {
                let restated = self.restate_core_agent(
                    &id,
                    body.description.clone(),
                    body.system_prompt.clone(),
                    body.respond,
                )?;
                if restated {
                    tracing::info!(
                        target: "bisa_store::core_agents",
                        "the core agent {id} now carries the shipped definition"
                    );
                }
                continue;
            }
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

    /// A workspace on disk whose keys persist across opens.
    fn open(dir: &tempfile::TempDir) -> Workspace {
        let identity = crate::paths::Paths::new(dir.path()).identity_dir();
        Workspace::open_with_keystore(
            dir.path(),
            Box::new(crate::identity::FileKeyStore::new(identity)),
        )
        .unwrap()
    }

    fn file_json(path: &std::path::Path) -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }

    /// An older workspace's core agent takes the shipped description, prompt
    /// and respond policy at the next open, and keeps the harness, plan and
    /// switch a person chose, its key, its birth and its attestation; a
    /// second open writes nothing.
    #[test]
    fn an_older_workspace_takes_the_shipped_prompt_and_keeps_its_harness_plan_switch_and_key() {
        let dir = tempfile::tempdir().unwrap();
        let ws = open(&dir);
        let id = AgentId::workflow();
        let mut chosen = ws.get_agent(&id).unwrap();
        chosen.harness = "codex".into();
        chosen.models = bisa_core::ModelPlan::pinned("gpt-5");
        chosen.decision_making = true;
        ws.update_agent(chosen).unwrap();
        // An older release's words, as its file would hold them.
        let path = ws.paths().agent_file(&id);
        let mut file = file_json(&path);
        file["system_prompt"] = serde_json::Value::String("an older method".into());
        file["description"] = serde_json::Value::String("an older description".into());
        file["respond"] = serde_json::json!("owner_only");
        std::fs::write(&path, serde_json::to_vec_pretty(&file).unwrap()).unwrap();
        let before = file_json(&path);
        drop(ws);

        let ws = open(&dir);
        let shipped = parse_agent(AgentId::WORKFLOW, WORKFLOW_AGENT_TOML).unwrap();
        let agent = ws.get_agent(&id).unwrap();
        assert_eq!(agent.system_prompt, shipped.system_prompt);
        assert_eq!(agent.description, shipped.description);
        assert_eq!(agent.respond, shipped.respond);
        assert_eq!(agent.harness, "codex", "the person's harness is kept");
        assert_eq!(agent.models, bisa_core::ModelPlan::pinned("gpt-5"));
        assert!(agent.decision_making, "the person's switch is kept");
        let after = file_json(&path);
        for kept in [
            "pubkey",
            "created_at",
            "attestation",
            "origin",
            "enabled",
            "name",
        ] {
            assert_eq!(after[kept], before[kept], "{kept} is kept");
        }
        drop(ws);

        let bytes = std::fs::read(&path).unwrap();
        let ws = open(&dir);
        assert_eq!(
            std::fs::read(&path).unwrap(),
            bytes,
            "a second open writes nothing"
        );
        drop(ws);
    }

    /// A core agent's truth file that does not parse is neither written over
    /// nor a failed open: it is left as it is for a person to repair.
    #[test]
    fn a_core_agent_file_that_does_not_parse_is_left_alone_at_the_next_open() {
        let dir = tempfile::tempdir().unwrap();
        let ws = open(&dir);
        let path = ws.paths().agent_file(&AgentId::general());
        drop(ws);
        std::fs::write(&path, "this is not json").unwrap();
        let ws = open(&dir);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "this is not json");
        assert!(ws.get_agent(&AgentId::general()).is_err());
    }

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

    /// The Workflow Agent's method names every start event and every catch
    /// the core knows, every change word of the events that have them, the
    /// sender words and the kinds of place a message lands in: the prose
    /// moves with the enums, or this says so.
    #[test]
    fn the_workflow_agent_names_every_event_its_changes_and_the_lists() {
        let prompt = parse_agent(AgentId::WORKFLOW, WORKFLOW_AGENT_TOML)
            .unwrap()
            .system_prompt;
        let said = |word: &str| {
            prompt.contains(&format!("`{word}`")) || prompt.contains(&format!("\"{word}\""))
        };
        for name in bisa_core::StartOn::NAMES {
            assert!(said(name), "start `{name}`: {prompt}");
        }
        for name in bisa_core::WaitFor::NAMES {
            assert!(said(name), "catch `{name}`: {prompt}");
        }
        for name in bisa_core::ProjectChange::NAMES
            .iter()
            .chain(bisa_core::RunChange::NAMES.iter())
            .chain(bisa_core::GoalChange::NAMES.iter())
            .chain(bisa_core::WorkflowChange::NAMES.iter())
            .chain(bisa_core::AgentChange::NAMES.iter())
            .chain(bisa_core::MessagePlace::NAMES.iter())
            .chain(bisa_core::MessageFrom::WORDS.iter())
        {
            assert!(said(name), "change or word `{name}`: {prompt}");
        }
        for rule in [
            "takes a list instead",
            "any of the list matches",
            "schedule, message, signal, run, goal, workflow, agent or platform",
            "hook, check, connector or project start waits",
        ] {
            assert!(prompt.contains(rule), "{rule}: {prompt}");
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
