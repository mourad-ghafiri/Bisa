//! Agents: the durable definitions behind kind 33403.
//!
//! The domain type is [`bisa_core::Agent`]; this module owns the keys,
//! the truth file, the snapshot and the index rows. Skills and MCP servers are
//! **referenced by id, never embedded**, so the public 33403 snapshot carries
//! ids rather than command lines.
//!
//! Secrets never enter the truth file: keys live in the keystore under
//! `agent:<pubkey>`; the owner's attestation tag IS stored (it is public).
//!
//! The rules — the core agent's id and origin belong together, it cannot be
//! disabled or removed, only its harness and model plan are editable — are the
//! core's ([`Agent::validate`], [`Agent::check_update`], [`Agent::check_remove`])
//! and are applied here before anything is written.
//!
//! Truth: `agents/<id>.json`; snapshot events in `agents/state/33403-<id>.json`;
//! the `agents` index table and its tag rows (both rebuildable).

use crate::error::StoreError;
use crate::identity::attest_agent;
use crate::paths::Paths;
use crate::workspace::{mint_ulid, now_secs, EventAudience, StoreEvent, Workspace};
use bisa_core::kind::KIND_AGENT_PROFILE;
use bisa_core::tags::TagEntity;
use bisa_core::{
    Agent, AgentId, AgentOrigin, McpId, ModelPlan, PrincipalId, RespondPolicy, SkillId, Tags,
};
use nostr::event::Tag;
use nostr::key::Keys;
use serde::{Deserialize, Serialize};

/// The on-disk truth file: the definition plus the owner's attestation tag.
#[derive(Serialize, Deserialize)]
struct AgentFile {
    #[serde(flatten)]
    def: Agent,
    /// NIP-OA `auth` tag (public), stored so `signer_for` needs no re-signing.
    attestation: Vec<String>,
}

/// Fields callers may supply when creating an agent; the id, the keys and the
/// provenance are minted here.
#[derive(Clone, Debug, Default)]
pub struct NewAgent {
    pub name: String,
    pub photo: Option<bisa_core::AttachmentRef>,
    pub description: Option<String>,
    pub system_prompt: String,
    pub harness: String,
    pub models: ModelPlan,
    pub skills: Vec<SkillId>,
    pub mcps: Vec<McpId>,
    pub tags: Tags,
    pub respond: RespondPolicy,
    /// Whether the Decision-Making Agent stands in at the decision points
    /// this agent reaches ([`Agent::decision_making`]).
    pub decision_making: bool,
}

pub(crate) fn agent_origin_str(o: &AgentOrigin) -> &'static str {
    match o {
        AgentOrigin::Local => "local",
        AgentOrigin::Catalog { .. } => "catalog",
        AgentOrigin::Core => "core",
    }
}

/// A slug from a display name, or `None` when nothing usable is left.
pub(crate) fn slugify(name: &str) -> Option<String> {
    let mut out = String::with_capacity(name.len());
    for ch in name.trim().chars() {
        match ch {
            'a'..='z' | '0'..='9' => out.push(ch),
            'A'..='Z' => out.push(ch.to_ascii_lowercase()),
            ' ' | '\t' | '_' | '-' | '/' | '.' if !out.ends_with('-') => out.push('-'),
            _ => {}
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() || !trimmed.as_bytes()[0].is_ascii_alphanumeric() {
        return None;
    }
    Some(trimmed.chars().take(48).collect())
}

impl Workspace {
    fn read_agent_file(&self, id: &AgentId) -> Result<AgentFile, StoreError> {
        let path = self.paths.agent_file(id);
        let bytes = std::fs::read(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StoreError::DefinitionNotFound {
                    kind: "agent",
                    id: id.to_string(),
                }
            } else {
                StoreError::io(path.display().to_string(), e)
            }
        })?;
        serde_json::from_slice(&bytes).map_err(|e| StoreError::unreadable(&path, "agent", e))
    }

    fn write_agent_file(&self, file: &AgentFile) -> Result<(), StoreError> {
        let path = self.paths.agent_file(&file.def.id);
        crate::paths::write_atomic(&path, &serde_json::to_vec_pretty(file)?)
    }

    pub(crate) fn index_agent(&self, def: &Agent) -> Result<(), StoreError> {
        let idx = self.idx();
        idx.upsert_agent(
            def.id.as_str(),
            &def.name,
            &def.harness,
            def.enabled,
            def.pubkey.as_hex(),
            agent_origin_str(&def.origin),
            def.created_at,
        )?;
        idx.set_tags(TagEntity::Agent, def.id.as_str(), def.tags.as_slice())
    }

    /// Publish the public 33403 snapshot: the definition, minus the
    /// attestation. Nothing secret-bearing can reach it by construction.
    fn snapshot_agent(&self, def: &Agent) -> Result<(), StoreError> {
        let at = now_secs();
        let existing_rev = self.snapshots.current_revision(
            Paths::NS_AGENTS,
            KIND_AGENT_PROFILE,
            def.id.as_str(),
        )?;
        let event = self.snapshots.put(
            Paths::NS_AGENTS,
            KIND_AGENT_PROFILE,
            def.id.as_str(),
            def,
            existing_rev + 1,
            &self.owner,
            at,
            None,
            def.tags.as_slice(),
        )?;
        self.emit_store_event(StoreEvent::ConversationSnapshot {
            kind: KIND_AGENT_PROFILE,
            d: def.id.to_string(),
            event,
            audience: EventAudience::Workspace,
        });
        Ok(())
    }

    /// A free id for a new agent: the name's slug, or the slug with a short
    /// suffix when that is taken.
    fn mint_agent_id(&self, name: &str) -> Result<AgentId, StoreError> {
        let base = slugify(name).unwrap_or_else(|| "agent".to_string());
        let candidate = AgentId::new(&base)?;
        if !self.agent_file_exists(&candidate) {
            return Ok(candidate);
        }
        let suffix = mint_ulid().to_string().to_ascii_lowercase();
        AgentId::new(format!("{base}-{}", &suffix[suffix.len() - 6..])).map_err(Into::into)
    }

    /// Create an agent: mints its id and keypair, attests it with the owner
    /// key, persists truth + snapshot + index.
    pub fn add_agent(&self, new: NewAgent) -> Result<Agent, StoreError> {
        let id = self.mint_agent_id(&new.name)?;
        self.add_agent_with_id(&id, new, AgentOrigin::Local)
    }

    /// Whether an agent truth file is there, without parsing it.
    pub(crate) fn agent_file_exists(&self, id: &AgentId) -> bool {
        self.paths.agent_file(id).exists()
    }

    /// Create an agent under a chosen id with a stated origin. The catalog
    /// installer and the general agent are the callers: an id and a provenance
    /// are both things the platform decides, never a client.
    pub(crate) fn add_agent_with_id(
        &self,
        id: &AgentId,
        new: NewAgent,
        origin: AgentOrigin,
    ) -> Result<Agent, StoreError> {
        if self.agent_file_exists(id) {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-agent-already-exists",
                id = id.to_string()
            )));
        }
        for s in &new.skills {
            self.get_skill(s)?;
        }
        for m in &new.mcps {
            self.get_mcp(m)?;
        }
        let keys = self.identity.mint_agent()?;
        let pubkey_hex = keys.public_key().to_hex();
        let attestation = attest_agent(&self.owner, &pubkey_hex, "")?;
        let def = Agent {
            id: id.clone(),
            name: new.name,
            photo: new.photo,
            description: new.description,
            system_prompt: new.system_prompt,
            harness: new.harness,
            models: new.models,
            skills: new.skills,
            mcps: new.mcps,
            tags: new.tags,
            respond: new.respond,
            decision_making: new.decision_making,
            pubkey: PrincipalId::new(pubkey_hex)?,
            origin,
            enabled: true,
            created_at: now_secs(),
        };
        if let Err(e) = def.validate() {
            // Nothing is written yet; the minted key is the one thing to undo.
            if let Err(k) = self.identity.delete_agent(def.pubkey.as_hex()) {
                tracing::warn!("could not discard the key of a refused agent: {k}");
            }
            return Err(e.into());
        }
        self.write_agent_file(&AgentFile {
            def: def.clone(),
            attestation: attestation.as_slice().to_vec(),
        })?;
        self.snapshot_agent(&def)?;
        self.index_agent(&def)?;
        Ok(def)
    }

    /// Update an agent's definition. What may change is the core's decision:
    /// see [`Agent::check_update`].
    pub fn update_agent(&self, def: Agent) -> Result<Agent, StoreError> {
        let stored = self.read_agent_file(&def.id)?;
        let def = Agent {
            // Keys, provenance and birth are the file's, not the caller's — a
            // client that PATCHes them back unchanged still succeeds.
            pubkey: stored.def.pubkey.clone(),
            origin: stored.def.origin.clone(),
            created_at: stored.def.created_at,
            ..def
        };
        stored.def.check_update(&def)?;
        def.validate()?;
        for s in &def.skills {
            self.get_skill(s)?;
        }
        for m in &def.mcps {
            self.get_mcp(m)?;
        }
        self.write_agent_file(&AgentFile {
            def: def.clone(),
            attestation: stored.attestation,
        })?;
        self.snapshot_agent(&def)?;
        self.index_agent(&def)?;
        Ok(def)
    }

    /// Flip enablement. Returns `(was, now)` so the caller can derive the
    /// membership events the flip produces
    /// ([`bisa_core::events_for_enablement`]).
    pub fn set_agent_enabled(
        &self,
        id: &AgentId,
        enabled: bool,
    ) -> Result<(Agent, bool), StoreError> {
        let mut def = self.get_agent(id)?;
        let was = def.enabled;
        def.enabled = enabled;
        let def = self.update_agent(def)?;
        Ok((def, was))
    }

    /// Remove an agent: truth file, work dir, snapshot, index row, keypair.
    ///
    /// Two refusals, in order: the core agent on **identity**
    /// ([`Agent::check_remove`]), every other agent while something still
    /// points at it ([`crate::usage`]).
    pub fn remove_agent(&self, id: &AgentId) -> Result<(), StoreError> {
        let stored = self.read_agent_file(id)?;
        stored.def.check_remove()?;
        self.refuse_if_used(crate::usage::UsageKind::Agent, id.as_str())?;
        if let Err(e) = self.identity.delete_agent(stored.def.pubkey.as_hex()) {
            tracing::warn!("agent {id}: key not removed from the keystore: {e}");
        }
        let path = self.paths.agent_file(id);
        std::fs::remove_file(&path).map_err(|e| StoreError::io(path.display().to_string(), e))?;
        // The agent's own directory (work/, recall/) is one this store created
        // under a validated id.
        let dir = self.paths.agent(id).dir().to_path_buf();
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(StoreError::io(dir.display().to_string(), e)),
        }
        self.snapshots
            .delete_snapshot(Paths::NS_AGENTS, KIND_AGENT_PROFILE, id.as_str())?;
        let idx = self.idx();
        idx.delete_agent(id.as_str())?;
        idx.clear_tags(TagEntity::Agent, id.as_str())
    }

    pub fn get_agent(&self, id: &AgentId) -> Result<Agent, StoreError> {
        Ok(self.read_agent_file(id)?.def)
    }

    pub fn list_agents(&self) -> Result<Vec<Agent>, StoreError> {
        let dir = self.paths.agents_dir();
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
            let Ok(id) = AgentId::new(stem) else {
                tracing::warn!("agents/{name}: not an agent id, skipping");
                continue;
            };
            if let Some(file) =
                crate::workspace::tolerated("agent", stem, self.read_agent_file(&id))?
            {
                out.push(file.def);
            }
        }
        out.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));
        Ok(out)
    }

    /// The signing keys + NIP-OA attestation tag for an agent — what the
    /// executor and intake use so an agent's work is signed by its own key.
    pub fn signer_for(&self, agent_id: &AgentId) -> Result<(Keys, Option<Tag>), StoreError> {
        let file = self.read_agent_file(agent_id)?;
        let keys = self.identity.agent(file.def.pubkey.as_hex())?;
        let tag = Tag::parse(file.attestation.clone()).map_err(StoreError::nostr)?;
        Ok((keys, Some(tag)))
    }

    /// Attach a library skill to an agent. Idempotent; order is attach order.
    pub fn attach_skill(
        &self,
        agent_id: &AgentId,
        skill_id: &SkillId,
    ) -> Result<Agent, StoreError> {
        self.get_skill(skill_id)?;
        let mut def = self.get_agent(agent_id)?;
        if !def.skills.contains(skill_id) {
            def.skills.push(skill_id.clone());
        }
        self.update_agent(def)
    }

    pub fn detach_skill(
        &self,
        agent_id: &AgentId,
        skill_id: &SkillId,
    ) -> Result<Agent, StoreError> {
        let mut def = self.get_agent(agent_id)?;
        def.skills.retain(|s| s != skill_id);
        self.update_agent(def)
    }

    pub fn attach_mcp(&self, agent_id: &AgentId, mcp_id: &McpId) -> Result<Agent, StoreError> {
        self.get_mcp(mcp_id)?;
        let mut def = self.get_agent(agent_id)?;
        if !def.mcps.contains(mcp_id) {
            def.mcps.push(mcp_id.clone());
        }
        self.update_agent(def)
    }

    pub fn detach_mcp(&self, agent_id: &AgentId, mcp_id: &McpId) -> Result<Agent, StoreError> {
        let mut def = self.get_agent(agent_id)?;
        def.mcps.retain(|m| m != mcp_id);
        self.update_agent(def)
    }

    /// Rebuild support: repopulate the `agents` index table from truth files.
    pub(crate) fn reindex_agents(&self) -> Result<(), StoreError> {
        for def in self.list_agents()? {
            self.index_agent(&def)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_become_ids() {
        assert_eq!(slugify("Scout").as_deref(), Some("scout"));
        assert_eq!(
            slugify("Code Reviewer 2").as_deref(),
            Some("code-reviewer-2")
        );
        assert_eq!(slugify("  ÜBER  ").as_deref(), Some("ber"));
        assert_eq!(slugify("!!!"), None);
        assert_eq!(slugify("-lead"), Some("lead".into()));
    }
}
