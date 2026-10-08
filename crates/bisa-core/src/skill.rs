//! `Skill`: a named markdown procedure an agent follows. Held once in the
//! library and referenced by id; twenty agents share one checklist instead of
//! growing twenty drifting copies.

use crate::id::SkillId;
use crate::origin::Origin;
use crate::tags::Tags;
use serde::{Deserialize, Serialize};

/// Longest skill body, in bytes. A skill is delivered into every session that
/// carries it, so an unbounded one is a tax on every launch.
pub const MAX_SKILL_BYTES: usize = 32 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Skill {
    /// Immutable: agents reference it, and it becomes a directory name inside
    /// a session's skills dir.
    pub id: SkillId,
    pub name: String,
    /// One line telling a model *when* to reach for this skill. It is the only
    /// part read before the skill is opened — so it is required.
    pub description: String,
    #[serde(default, skip_serializing_if = "Tags::is_empty")]
    pub tags: Tags,
    pub markdown: String,
    pub origin: Origin,
    pub created_at: u64,
}

impl Skill {
    pub fn validate(&self) -> Result<(), SkillError> {
        if self.name.trim().is_empty() {
            return Err(SkillError::EmptyName);
        }
        if self.description.trim().is_empty() {
            return Err(SkillError::EmptyDescription);
        }
        if self.markdown.len() > MAX_SKILL_BYTES {
            return Err(SkillError::TooLarge(self.markdown.len()));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SkillError {
    #[error("a skill needs a name")]
    EmptyName,
    #[error(
        "a skill needs a one-line description: it is the only part a model reads before opening it"
    )]
    EmptyDescription,
    #[error("skill body is {0} bytes; the cap is {MAX_SKILL_BYTES}")]
    TooLarge(usize),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill() -> Skill {
        Skill {
            id: SkillId::new("code-review-checklist").unwrap(),
            name: "Code review checklist".into(),
            description: "Reviewing a diff: what to check, in order.".into(),
            tags: Tags::default(),
            markdown: "# Checklist\n".into(),
            origin: Origin::Local,
            created_at: 0,
        }
    }

    #[test]
    fn a_description_is_required_and_the_body_is_capped() {
        assert!(skill().validate().is_ok());
        let mut s = skill();
        s.description = " ".into();
        assert_eq!(s.validate(), Err(SkillError::EmptyDescription));
        let mut s = skill();
        s.markdown = "x".repeat(MAX_SKILL_BYTES + 1);
        assert!(matches!(s.validate(), Err(SkillError::TooLarge(_))));
    }

    #[test]
    fn roundtrip() {
        let s = skill();
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Skill>(&json).unwrap(), s);
    }

    // added by the coverage pass: skill.rs

    #[test]
    fn a_skill_needs_a_name() {
        let mut s = skill();
        s.name = " ".into();
        assert_eq!(s.validate(), Err(SkillError::EmptyName));
    }
}
