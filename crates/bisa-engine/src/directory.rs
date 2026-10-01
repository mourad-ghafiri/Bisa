//! Who exists and what they carry: agent definitions, teams, skills and MCP
//! servers, edited by a person.
//!
//! Thin on purpose. The store validates and refuses (a slug that is taken, a
//! skill still carried by an agent, the core agent's protections); this module
//! is the engine's door for those writes so the node never reaches the store
//! (`docs/architecture/07-layering.md`, rule 2). Enablement is not here — it
//! produces membership events and lives in [`crate::membership`].

use crate::{EngineError, Inner};
use bisa_core::{Agent, AgentId, Assignee, McpId, McpServer, Skill, SkillId, Tags, Team, TeamId};
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

pub fn remove_agent(inner: &Arc<Inner>, id: &AgentId) -> Result<(), EngineError> {
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
