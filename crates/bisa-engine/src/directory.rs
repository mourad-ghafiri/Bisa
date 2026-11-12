//! Who exists and what they carry: agent definitions, teams, skills and MCP
//! servers, edited by a person.
//!
//! Thin on purpose. The store validates and refuses (a slug that is taken, a
//! skill still carried by an agent, the core agent's protections); this module
//! is the engine's door for those writes so the node never reaches the store
//! (`docs/architecture/07-layering.md`, rule 2). Enablement is not here — it
//! produces membership events and lives in [`crate::membership`].

use crate::{EngineError, Inner};
use bisa_core::{
    Agent, AgentId, Assignee, AttachmentRef, McpId, McpServer, ModelPlan, RespondPolicy, Skill,
    SkillId, Tags, Team, TeamId,
};
use bisa_store::{NewAgent, NewMcp, NewSkill};
use std::sync::Arc;

// --- agents -----------------------------------------------------------------

pub fn add_agent(inner: &Arc<Inner>, new: NewAgent) -> Result<Agent, EngineError> {
    Ok(inner.ws.add_agent(new)?)
}

/// Replace an agent's record. What the edit did to its enablement is
/// announced in every channel it belongs to ([`crate::membership`]).
pub fn update_agent(inner: &Arc<Inner>, agent: Agent) -> Result<Agent, EngineError> {
    crate::membership::update_agent(inner, agent)
}

/// What a person changes of an agent's record. A field left out stays as it
/// is; `photo` and `description` clear with an inner `None`; the lists and
/// the tags replace what was there.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AgentEdit {
    pub enabled: Option<bool>,
    pub name: Option<String>,
    pub photo: Option<Option<AttachmentRef>>,
    pub description: Option<Option<String>>,
    pub system_prompt: Option<String>,
    pub harness: Option<String>,
    pub models: Option<ModelPlan>,
    pub skills: Option<Vec<SkillId>>,
    pub mcps: Option<Vec<McpId>>,
    pub tags: Option<Tags>,
    pub respond: Option<RespondPolicy>,
    pub decision_making: Option<bool>,
}

impl AgentEdit {
    fn apply(self, agent: &mut Agent) {
        fn set<T>(slot: &mut T, value: Option<T>) {
            if let Some(value) = value {
                *slot = value;
            }
        }
        set(&mut agent.enabled, self.enabled);
        set(&mut agent.name, self.name);
        set(&mut agent.photo, self.photo);
        set(&mut agent.description, self.description);
        set(&mut agent.system_prompt, self.system_prompt);
        set(&mut agent.harness, self.harness);
        set(&mut agent.models, self.models);
        set(&mut agent.skills, self.skills);
        set(&mut agent.mcps, self.mcps);
        set(&mut agent.tags, self.tags);
        set(&mut agent.respond, self.respond);
        set(&mut agent.decision_making, self.decision_making);
    }
}

/// Change the fields an edit names on the agent's record **as it stands**,
/// in one write under the roster's one writer. Two edits of different
/// fields both stand, whoever made them and however close together: neither
/// carries the other's field back as it read it. Enablement, when the edit
/// flips it, is announced as [`crate::membership`] announces any flip.
pub fn edit_agent(inner: &Arc<Inner>, id: &AgentId, edit: AgentEdit) -> Result<Agent, EngineError> {
    crate::membership::edit_agent(inner, id, |agent| {
        edit.apply(agent);
        Ok(())
    })
}

/// Remove an agent. One something still points at is refused by the store
/// — before anything is stopped; then every turn it is running in a
/// conversation is ended, as a channel's are on its deletion, so no session
/// goes on under an agent whose key and folder are gone. What it said stays
/// where it said it: a direct channel with it is history.
///
/// A work item it holds is a holder the store already refuses on; a turn in
/// a conversation is held by nothing durable, and is ended here.
pub async fn remove_agent(inner: &Arc<Inner>, id: &AgentId) -> Result<(), EngineError> {
    match inner.ws.check_agent_removal(id) {
        // A record nobody can read has no turn to end; the store removes it
        // by its id, or says why not.
        Ok(()) | Err(bisa_store::StoreError::Unreadable { .. }) => {}
        Err(e) => return Err(e.into()),
    }
    crate::conversation::end_agent_sessions(inner, id.as_str()).await;
    Ok(inner.ws.remove_agent(id)?)
}

/// Attach a library skill; idempotent, and the list keeps attach order.
pub fn attach_skill(
    inner: &Arc<Inner>,
    agent: &AgentId,
    skill: &SkillId,
) -> Result<Agent, EngineError> {
    Ok(inner.ws.attach_skill(agent, skill)?)
}

pub fn detach_skill(
    inner: &Arc<Inner>,
    agent: &AgentId,
    skill: &SkillId,
) -> Result<Agent, EngineError> {
    Ok(inner.ws.detach_skill(agent, skill)?)
}

pub fn attach_mcp(inner: &Arc<Inner>, agent: &AgentId, mcp: &McpId) -> Result<Agent, EngineError> {
    Ok(inner.ws.attach_mcp(agent, mcp)?)
}

pub fn detach_mcp(inner: &Arc<Inner>, agent: &AgentId, mcp: &McpId) -> Result<Agent, EngineError> {
    Ok(inner.ws.detach_mcp(agent, mcp)?)
}

// --- teams ------------------------------------------------------------------

pub fn create_team(
    inner: &Arc<Inner>,
    name: &str,
    purpose: Option<&str>,
    members: Vec<Assignee>,
    tags: Tags,
) -> Result<Team, EngineError> {
    Ok(inner.ws.create_team(name, purpose, members, tags)?)
}

/// Replace a team's record, its enablement announced as an agent's is.
pub fn update_team(inner: &Arc<Inner>, team: Team) -> Result<Team, EngineError> {
    crate::membership::update_team(inner, team)
}

/// What a person changes of a team's record; a field left out stays as it
/// is, `purpose` and `photo` clear with an inner `None`, the members and the
/// tags replace what was there.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TeamEdit {
    pub enabled: Option<bool>,
    pub name: Option<String>,
    pub purpose: Option<Option<String>>,
    pub photo: Option<Option<AttachmentRef>>,
    pub members: Option<Vec<Assignee>>,
    pub tags: Option<Tags>,
}

impl TeamEdit {
    fn apply(self, team: &mut Team) {
        fn set<T>(slot: &mut T, value: Option<T>) {
            if let Some(value) = value {
                *slot = value;
            }
        }
        set(&mut team.enabled, self.enabled);
        set(&mut team.name, self.name);
        set(&mut team.purpose, self.purpose);
        set(&mut team.photo, self.photo);
        set(&mut team.members, self.members);
        set(&mut team.tags, self.tags);
    }
}

/// A team's edit, as [`edit_agent`] is an agent's: on the record as it
/// stands, one write, a flip announced once.
pub fn edit_team(inner: &Arc<Inner>, id: &TeamId, edit: TeamEdit) -> Result<Team, EngineError> {
    crate::membership::edit_team(inner, id, |team| {
        edit.apply(team);
        Ok(())
    })
}

pub fn remove_team(inner: &Arc<Inner>, id: &TeamId) -> Result<(), EngineError> {
    Ok(inner.ws.remove_team(id)?)
}

// --- skills -----------------------------------------------------------------

pub fn create_skill(inner: &Arc<Inner>, new: NewSkill) -> Result<Skill, EngineError> {
    Ok(inner.ws.create_skill(new)?)
}

pub fn update_skill(inner: &Arc<Inner>, skill: Skill) -> Result<Skill, EngineError> {
    Ok(inner.ws.update_skill(skill)?)
}

/// Remove a skill, or refuse while any agent still carries it.
pub fn remove_skill(inner: &Arc<Inner>, id: &SkillId) -> Result<(), EngineError> {
    Ok(inner.ws.remove_skill(id)?)
}

// --- MCP servers ------------------------------------------------------------

pub fn create_mcp(inner: &Arc<Inner>, new: NewMcp) -> Result<McpServer, EngineError> {
    Ok(inner.ws.create_mcp(new)?)
}

/// Replace a server's fields — and forget what the old shape answered when
/// probed: a health read against an edited command line would be a lie.
pub fn update_mcp(inner: &Arc<Inner>, server: McpServer) -> Result<McpServer, EngineError> {
    let saved = inner.ws.update_mcp(server)?;
    inner.mcp_health.invalidate(&saved.id);
    Ok(saved)
}

/// Remove a server, or refuse while any agent still carries it.
pub fn remove_mcp(inner: &Arc<Inner>, id: &McpId) -> Result<(), EngineError> {
    inner.ws.remove_mcp(id)?;
    inner.mcp_health.invalidate(id);
    Ok(())
}
