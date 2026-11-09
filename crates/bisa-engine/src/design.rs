//! The UI/UX design policy (06 — Agents and teams § Skills): who the
//! workspace holds to the UI/UX Design skill — `design.agents` — and how a
//! session is told.
//!
//! - **everyone** (the default): a session of an agent that does not carry
//!   the skill reads one sentence pointing at `skill_read` — or, on a harness
//!   without the platform's tools, the whole skill appended — and an agent
//!   that carries it has it as any skill, with nothing said twice.
//! - **assigned**: carriers alone; nobody else is told.
//! - **nobody**: the skill is dropped from every session's skills and
//!   nothing is said.
//!
//! Nothing is said until the skill is installed: a sentence about a skill
//! `skill_read` cannot find would send an agent to a door that is not there.

use crate::browser::AgentsPolicy;
use crate::Inner;
pub use bisa_core::design::DESIGN_SKILL;
use bisa_core::{ProjectId, Skill, SkillId};
use bisa_harness::SkillPayload;

/// The policy, read once per prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Access {
    pub agents: AgentsPolicy,
}

/// `design.agents`, for the checkout's project where there is one; a value
/// the registry cannot give falls to the default the registry declares.
pub fn access(inner: &Inner, project: Option<ProjectId>) -> Access {
    let word = inner
        .ws
        .setting("design.agents", project)
        .ok()
        .and_then(|r| r.value.as_str().map(str::to_string));
    let agents = match word.as_deref() {
        Some("assigned") => AgentsPolicy::Assigned,
        Some("nobody") => AgentsPolicy::Nobody,
        _ => AgentsPolicy::Everyone,
    };
    Access { agents }
}

/// The skill as installed in this workspace, or none.
pub fn installed(inner: &Inner) -> Option<Skill> {
    SkillId::new(DESIGN_SKILL)
        .ok()
        .and_then(|id| inner.ws.get_skill(&id).ok())
}

/// Whether an agent's skills name the design skill.
pub fn carries(skills: &[SkillId]) -> bool {
    skills.iter().any(|s| s.as_str() == DESIGN_SKILL)
}

/// Under *nobody* the skill leaves every session's skills, carriers included.
pub fn filter_skills(inner: &Inner, project: Option<ProjectId>, skills: &mut Vec<SkillPayload>) {
    if access(inner, project).agents == AgentsPolicy::Nobody {
        skills.retain(|s| s.id != DESIGN_SKILL);
    }
}

/// What a session's prompt says about the skill.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Framing {
    /// Nothing: the policy is not *everyone*, the agent carries the skill
    /// already, or the skill is not installed.
    Silent,
    /// One sentence pointing at `skill_read`.
    ReadWithTool,
    /// The whole skill, appended: the harness has no tool to read it with.
    Appended(SkillPayload),
}

/// The framing for one session: the policy, then whether this agent carries
/// the skill, then whether the skill is installed, then what the harness can
/// do with it.
pub fn framing(
    inner: &Inner,
    project: Option<ProjectId>,
    agent_skills: &[SkillId],
    platform_tools: bool,
) -> Framing {
    if access(inner, project).agents != AgentsPolicy::Everyone || carries(agent_skills) {
        return Framing::Silent;
    }
    let Some(skill) = installed(inner) else {
        return Framing::Silent;
    };
    if platform_tools {
        Framing::ReadWithTool
    } else {
        Framing::Appended(SkillPayload {
            id: skill.id.to_string(),
            name: skill.name,
            description: skill.description,
            markdown: skill.markdown,
        })
    }
}
