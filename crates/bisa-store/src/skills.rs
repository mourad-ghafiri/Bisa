//! Skills: the shared library of procedures agents follow (kind 33411).
//!
//! An agent's **system prompt is its role**; a **skill is a procedure**.
//! Skills live in one library; agents reference them by id, and
//! [`Workspace::skill_payloads`] resolves a reference list at launch — an
//! unknown id costs a skill, never a session.
//!
//! **Nothing is deleted while something points at it, and the refusal names
//! what** ([`crate::usage`]).
//!
//! Truth: `skills/<id>.json`; snapshot events in `skills/state/33411-<id>.json`;
//! `skills` index table plus its tag rows (both rebuildable).

use crate::error::StoreError;
use crate::paths::Paths;
use crate::teams::origin_str;
use crate::workspace::{now_secs, EventAudience, StoreEvent, Workspace};
use bisa_core::kind::KIND_SKILL;
use bisa_core::tags::TagEntity;
use bisa_core::{AgentId, Origin, Skill, SkillId, Tags};
use bisa_harness::SkillPayload;

pub use bisa_core::MAX_SKILL_BYTES;

/// Fields a caller supplies when creating a skill.
#[derive(Clone, Debug)]
pub struct NewSkill {
    pub id: SkillId,
    pub name: String,
    pub description: String,
    pub tags: Tags,
    pub markdown: String,
}

impl Workspace {
    fn write_skill(&self, def: &Skill) -> Result<(), StoreError> {
        def.validate()?;
        if def.markdown.trim().is_empty() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-skill-empty",
                a0 = (def.id).to_string()
            )));
        }
        let path = self.paths.skill_file(&def.id);
        crate::paths::write_atomic(&path, &serde_json::to_vec_pretty(def)?)?;
        let existing_rev =
            self.snapshots
                .current_revision(Paths::NS_SKILLS, KIND_SKILL, def.id.as_str())?;
        let event = self.snapshots.put(
            Paths::NS_SKILLS,
            KIND_SKILL,
            def.id.as_str(),
            def,
            existing_rev + 1,
            &self.owner,
            now_secs(),
            None,
            def.tags.as_slice(),
        )?;
        self.emit_store_event(StoreEvent::ConversationSnapshot {
            kind: KIND_SKILL,
            d: def.id.to_string(),
            event,
            audience: EventAudience::Workspace,
        });
        self.index_skill(def)
    }

    pub(crate) fn index_skill(&self, def: &Skill) -> Result<(), StoreError> {
        let idx = self.idx();
        idx.upsert_skill(
            def.id.as_str(),
            &def.name,
            &def.description,
            origin_str(&def.origin),
            def.created_at,
        )?;
        idx.set_tags(TagEntity::Skill, def.id.as_str(), def.tags.as_slice())
    }

    pub fn create_skill(&self, new: NewSkill) -> Result<Skill, StoreError> {
        self.create_skill_with_origin(new, Origin::Local)
    }

    /// Only [`crate::catalog`] passes anything but `Local`: provenance is not
    /// a caller's to claim.
    pub(crate) fn create_skill_with_origin(
        &self,
        new: NewSkill,
        origin: Origin,
    ) -> Result<Skill, StoreError> {
        if self.paths.skill_file(&new.id).exists() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-skill-already-exists",
                a0 = (new.id).to_string()
            )));
        }
        let def = Skill {
            id: new.id,
            name: new.name,
            description: new.description,
            tags: new.tags,
            markdown: new.markdown,
            origin,
            created_at: now_secs(),
        };
        self.write_skill(&def)?;
        Ok(def)
    }

    pub fn get_skill(&self, id: &SkillId) -> Result<Skill, StoreError> {
        let path = self.paths.skill_file(id);
        let bytes = std::fs::read(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StoreError::DefinitionNotFound {
                    kind: "skill",
                    id: id.to_string(),
                }
            } else {
                StoreError::io(path.display().to_string(), e)
            }
        })?;
        serde_json::from_slice(&bytes).map_err(|e| StoreError::unreadable(&path, "skill", e))
    }

    pub fn list_skills(&self) -> Result<Vec<Skill>, StoreError> {
        let dir = self.paths.skills_dir();
        let mut out = Vec::new();
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(StoreError::io(dir.display().to_string(), e)),
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(stem) = name.strip_suffix(".json") else {
                continue;
            };
            let Ok(id) = SkillId::new(stem) else {
                tracing::warn!("skills/{name}: not a skill id, skipping");
                continue;
            };
            if let Some(skill) = crate::workspace::tolerated("skill", stem, self.get_skill(&id))? {
                out.push(skill);
            }
        }
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }

    /// Replace a skill's fields. The id is immutable — agents reference it.
    pub fn update_skill(&self, def: Skill) -> Result<Skill, StoreError> {
        let existing = self.get_skill(&def.id)?;
        let def = Skill {
            origin: existing.origin,
            created_at: existing.created_at,
            ..def
        };
        self.write_skill(&def)?;
        Ok(def)
    }

    /// Delete a skill, and refuse while any agent still carries it.
    pub fn remove_skill(&self, id: &SkillId) -> Result<(), StoreError> {
        self.get_skill(id)?;
        self.refuse_if_used(crate::usage::UsageKind::Skill, id.as_str())?;
        let path = self.paths.skill_file(id);
        std::fs::remove_file(&path).map_err(|e| StoreError::io(path.display().to_string(), e))?;
        self.snapshots
            .delete_snapshot(Paths::NS_SKILLS, KIND_SKILL, id.as_str())?;
        let idx = self.idx();
        idx.delete_skill(id.as_str())?;
        idx.clear_tags(TagEntity::Skill, id.as_str())
    }

    /// Resolve `agent`'s skill reference list into launch payloads. A
    /// missing id costs the session that one skill and nothing else: it is
    /// logged with the agent whose launch lost it, and skipped.
    pub fn skill_payloads(&self, agent: &AgentId, ids: &[SkillId]) -> Vec<SkillPayload> {
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            match self.get_skill(id) {
                Ok(def) => out.push(SkillPayload {
                    id: def.id.to_string(),
                    name: def.name,
                    description: def.description,
                    markdown: def.markdown,
                }),
                Err(e) => tracing::warn!(
                    target: "bisa_store::skills",
                    %agent,
                    skill = %id,
                    "a skill the agent names does not resolve and is left out of this session: {e}"
                ),
            }
        }
        out
    }

    pub(crate) fn reindex_skills(&self) -> Result<(), StoreError> {
        for def in self.list_skills()? {
            self.index_skill(&def)?;
        }
        Ok(())
    }
}

// added by the coverage pass: skills_mod.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::MemoryKeyStore;

    fn ws() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let ws =
            Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
        (dir, ws)
    }

    fn tidy(markdown: &str) -> NewSkill {
        NewSkill {
            id: SkillId::new("tidy").unwrap(),
            name: "Tidy".into(),
            description: "keeps things neat".into(),
            tags: Tags::default(),
            markdown: markdown.into(),
        }
    }

    /// A skill with no words is refused; the folder skips what is not a
    /// skill's file and names what it cannot read; a skill an agent names
    /// that does not resolve costs the session that skill alone; a rebuild
    /// indexes every skill again.
    #[test]
    fn a_wordless_skill_is_refused_and_the_folder_skips_strangers() {
        let (_d, ws) = ws();
        let err = ws.create_skill(tidy("   ")).unwrap_err();
        assert!(matches!(&err, StoreError::Invalid(_)), "{err:?}");
        let skill = ws.create_skill(tidy("# Tidy\nkeep it neat")).unwrap();
        let skills = ws.paths.skills_dir();
        std::fs::write(skills.join("Bad Name.json"), b"{}").unwrap();
        std::fs::write(skills.join("README"), b"").unwrap();
        assert_eq!(ws.list_skills().unwrap().len(), 1);
        let agent = AgentId::new("scout").unwrap();
        let payloads =
            ws.skill_payloads(&agent, &[skill.id.clone(), SkillId::new("nope").unwrap()]);
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0].id, "tidy");
        ws.rebuild_index().unwrap();
        assert_eq!(ws.list_skills().unwrap().len(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let file = ws.paths.skill_file(&skill.id);
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
            let unreadable = ws.get_skill(&skill.id);
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(matches!(unreadable, Err(StoreError::Io { .. })));
            std::fs::set_permissions(&skills, std::fs::Permissions::from_mode(0o000)).unwrap();
            let unreadable = ws.list_skills();
            std::fs::set_permissions(&skills, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert!(matches!(unreadable, Err(StoreError::Io { .. })));
        }
    }
}
