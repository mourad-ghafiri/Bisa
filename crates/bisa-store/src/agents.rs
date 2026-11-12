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
            // LCOV_EXCL_START: the store's disk arm: a snapshot revision read that fails below the lock
        )?;
        // LCOV_EXCL_STOP
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
        )?; // LCOV_EXCL_LINE: the store's disk arm: a snapshot write that fails below the lock
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
        // Taken: an agent holds the id, the platform keeps it for itself
        // (`Paths::RESERVED_AGENT_IDS`), or a folder already stands where
        // this agent's own would — somebody else's scratch and recall are
        // nobody's to inherit.
        let taken = self.agent_file_exists(&candidate)
            || Paths::RESERVED_AGENT_IDS.contains(&candidate.as_str())
            || self.paths.agent(&candidate).dir().exists();
        if !taken {
            return Ok(candidate);
        }
        let suffix = mint_ulid().to_string().to_ascii_lowercase();
        AgentId::new(format!("{base}-{}", &suffix[suffix.len() - 6..])).map_err(Into::into)
    }

    /// Create an agent: mints its id and keypair, attests it with the owner
    /// key, persists truth + snapshot + index.
    pub fn add_agent(&self, new: NewAgent) -> Result<Agent, StoreError> {
        // The id is minted and taken under the one writer: two agents made
        // at once under one name are two ids, never one file written twice.
        let _one_writer = self.roster_writer();
        let id = self.mint_agent_id(&new.name)?;
        self.add_agent_locked(&id, new, AgentOrigin::Local)
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
        let _one_writer = self.roster_writer();
        self.add_agent_locked(id, new, origin)
    }

    /// [`Self::add_agent_with_id`] under the writer its caller holds.
    fn add_agent_locked(
        &self,
        id: &AgentId,
        new: NewAgent,
        origin: AgentOrigin,
    ) -> Result<Agent, StoreError> {
        if Paths::RESERVED_AGENT_IDS.contains(&id.as_str()) {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-agent-id-reserved",
                id = id.to_string()
            )));
        }
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
        if let Err(e) = def.check_new() {
            // Nothing is written yet; the minted key is the one thing to undo.
            if let Err(k) = self.identity.delete_agent(def.pubkey.as_hex()) {
                // LCOV_EXCL_START: the file and memory stores delete the key they just minted; only a keyring that lost it fails here
                tracing::warn!("could not discard the key of a refused agent: {k}");
                // LCOV_EXCL_STOP
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
        let _one_writer = self.roster_writer();
        let stored = self.read_agent_file(&def.id)?;
        self.update_agent_locked(stored, def)
    }

    /// Edit an agent's record under the one writer: `edit` sees the agent as
    /// it stands and changes what it means to — a name, a plan, a skill, the
    /// switch — and the result is judged and written as [`Self::update_agent`]
    /// writes it. Two edits of different fields at once never lose one: each
    /// is applied to the record the other left. Answers the record before
    /// and after, so a caller that announces a change reads no second time.
    pub fn update_agent_with(
        &self,
        id: &AgentId,
        edit: impl FnOnce(&mut Agent) -> Result<(), StoreError>,
    ) -> Result<(Agent, Agent), StoreError> {
        let _one_writer = self.roster_writer();
        let stored = self.read_agent_file(id)?;
        let before = stored.def.clone();
        let mut next = before.clone();
        edit(&mut next)?;
        let after = self.update_agent_locked(stored, next)?;
        Ok((before, after))
    }

    /// The write behind [`Self::update_agent`] and
    /// [`Self::update_agent_with`], under the writer the caller holds.
    fn update_agent_locked(&self, stored: AgentFile, def: Agent) -> Result<Agent, StoreError> {
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

    /// The shipped definition's words on a core agent's existing record —
    /// its description, its prompt and its respond policy, the fields
    /// [`Agent::check_update`] holds fixed to a person — with everything a
    /// person or the platform chose kept exactly: harness, models, the
    /// decision-making switch, photo, key, origin, birth and the
    /// attestation. `Ok(false)` when the record already says so. A truth
    /// file that does not parse is left alone and named, never written over.
    pub(crate) fn restate_core_agent(
        &self,
        id: &AgentId,
        description: Option<String>,
        system_prompt: String,
        respond: bisa_core::RespondPolicy,
    ) -> Result<bool, StoreError> {
        let _one_writer = self.roster_writer();
        let stored = match self.read_agent_file(id) {
            Ok(file) => file,
            Err(e) => {
                tracing::warn!(
                    target: "bisa_store::agents",
                    "the core agent {id} could not be restated to the shipped definition: {e}"
                );
                return Ok(false);
            }
        };
        if stored.def.description == description
            && stored.def.system_prompt == system_prompt
            && stored.def.respond == respond
        {
            return Ok(false);
        }
        let def = Agent {
            description,
            system_prompt,
            respond,
            ..stored.def
        };
        self.write_agent_file(&AgentFile {
            def: def.clone(),
            attestation: stored.attestation,
        })?;
        self.snapshot_agent(&def)?;
        self.index_agent(&def)?;
        Ok(true)
    }

    /// Flip enablement. Returns `(was, now)` so the caller can derive the
    /// membership events the flip produces
    /// ([`bisa_core::events_for_enablement`]).
    pub fn set_agent_enabled(
        &self,
        id: &AgentId,
        enabled: bool,
    ) -> Result<(Agent, bool), StoreError> {
        let (before, after) = self.update_agent_with(id, |agent| {
            agent.enabled = enabled;
            Ok(())
        })?;
        Ok((after, before.enabled))
    }

    /// Remove an agent: truth file, work dir, snapshot, index row, keypair.
    ///
    /// Two refusals, in order: the core agent on **identity**
    /// ([`Agent::check_remove`]), every other agent while something still
    /// points at it ([`crate::usage`]).
    pub fn remove_agent(&self, id: &AgentId) -> Result<(), StoreError> {
        let _one_writer = self.roster_writer();
        // The key to take out of the keystore: the record's, or — for a
        // record nobody can read — the one the index held for it.
        let pubkey = match self.read_agent_file(id) {
            Ok(stored) => {
                stored.def.check_remove()?;
                self.refuse_if_used(crate::usage::UsageKind::Agent, id.as_str())?;
                Some(stored.def.pubkey.as_hex().to_string())
            }
            // A removal that stopped after the record went is finished here:
            // what is left is the agent's own folder, and nothing is said
            // twice.
            Err(StoreError::DefinitionNotFound { .. }) if self.agent_folder_is_left(id) => {
                return self.remove_agent_folder(id);
            }
            Err(StoreError::Unreadable { .. }) => {
                if id.is_core_id() {
                    return Err(bisa_core::AgentError::CoreCannotBeRemoved.into());
                }
                let pubkey = self.idx().agent_pubkey(id.as_str())?;
                self.refuse_if_id_used(
                    crate::usage::UsageKind::Agent,
                    id.as_str(),
                    pubkey.as_deref(),
                )?;
                pubkey
            }
            Err(e) => return Err(e),
        };
        // What others read first, the record last of the three: stopped
        // anywhere before it, the agent is whole on disk and the removal is
        // asked again; past it, nothing reads as an agent any more.
        {
            let idx = self.idx();
            idx.delete_agent(id.as_str())?;
            idx.clear_tags(TagEntity::Agent, id.as_str())?;
        }
        self.snapshots
            .delete_snapshot(Paths::NS_AGENTS, KIND_AGENT_PROFILE, id.as_str())?;
        let path = self.paths.agent_file(id);
        std::fs::remove_file(&path).map_err(|e| StoreError::io(path.display().to_string(), e))?;
        self.emit_store_event(StoreEvent::DefinitionRemoved {
            kind: KIND_AGENT_PROFILE,
            d: id.to_string(),
        });
        if let Some(pubkey) = pubkey {
            if let Err(e) = self.identity.delete_agent(&pubkey) {
                tracing::warn!("agent {id}: key not removed from the keystore: {e}");
            }
        }
        self.remove_agent_folder(id)
    }

    /// Whether an agent may be removed, asked before anything is stopped for
    /// it: it is not a core agent, and nothing points at it. The removal
    /// asks again under the writer; this is for a caller that has something
    /// to end first — the engine, the agent's live turns.
    pub fn check_agent_removal(&self, id: &AgentId) -> Result<(), StoreError> {
        self.get_agent(id)?.check_remove()?;
        self.refuse_if_used(crate::usage::UsageKind::Agent, id.as_str())
    }

    /// Whether an agent with no record still has something of its own on
    /// disk — what a removal that stopped midway left.
    fn agent_folder_is_left(&self, id: &AgentId) -> bool {
        let own = self.paths.agent(id);
        if own.dir() != self.paths.state_dir(Paths::NS_AGENTS) {
            return own.dir().exists();
        }
        own.scratch().exists() || own.recall().exists() || own.recall_index().exists()
    }

    /// Remove what an agent keeps of its own on disk — its scratch, its
    /// recall — a folder this store created under a validated id.
    ///
    /// An agent a workspace made before the id was reserved may be called
    /// `state`: its folder is then the one every agent's snapshot lives in,
    /// and only its own parts are taken out of it, never the folder.
    fn remove_agent_folder(&self, id: &AgentId) -> Result<(), StoreError> {
        let own = self.paths.agent(id);
        let gone = |path: &std::path::Path, removed: std::io::Result<()>| match removed {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(StoreError::io(path.display().to_string(), e)),
        };
        if own.dir() != self.paths.state_dir(Paths::NS_AGENTS) {
            return gone(own.dir(), std::fs::remove_dir_all(own.dir()));
        }
        for part in [own.scratch(), own.recall()] {
            gone(&part, std::fs::remove_dir_all(&part))?;
        }
        let index = own.recall_index();
        gone(&index, std::fs::remove_file(&index))
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
                self.tolerated_definition("agent", stem, self.read_agent_file(&id))?
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
        let edited = self.update_agent_with(agent_id, |agent| {
            if !agent.skills.contains(skill_id) {
                agent.skills.push(skill_id.clone());
            }
            Ok(())
        })?;
        Ok(edited.1)
    }

    pub fn detach_skill(
        &self,
        agent_id: &AgentId,
        skill_id: &SkillId,
    ) -> Result<Agent, StoreError> {
        let edited = self.update_agent_with(agent_id, |agent| {
            agent.skills.retain(|s| s != skill_id);
            Ok(())
        })?;
        Ok(edited.1)
    }

    pub fn attach_mcp(&self, agent_id: &AgentId, mcp_id: &McpId) -> Result<Agent, StoreError> {
        self.get_mcp(mcp_id)?;
        let edited = self.update_agent_with(agent_id, |agent| {
            if !agent.mcps.contains(mcp_id) {
                agent.mcps.push(mcp_id.clone());
            }
            Ok(())
        })?;
        Ok(edited.1)
    }

    pub fn detach_mcp(&self, agent_id: &AgentId, mcp_id: &McpId) -> Result<Agent, StoreError> {
        let edited = self.update_agent_with(agent_id, |agent| {
            agent.mcps.retain(|m| m != mcp_id);
            Ok(())
        })?;
        Ok(edited.1)
    }

    /// Rebuild support: repopulate the `agents` index table from truth files.
    pub(crate) fn reindex_agents(&self) -> Result<(), StoreError> {
        for def in self.list_agents()? {
            // Two files with one key cannot both have the row: the second
            // is named, and the rebuild goes on.
            self.tolerated_index("agent", def.id.as_str(), self.index_agent(&def))?;
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

    // added by the coverage pass: agents.rs

    // --- the bare lines of the agents module ---

    fn ws5() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open_with_keystore(
            dir.path(),
            Box::new(crate::identity::MemoryKeyStore::default()),
        )
        .unwrap();
        (dir, ws)
    }

    fn scout() -> NewAgent {
        NewAgent {
            name: "Scout".into(),
            harness: "mock".into(),
            system_prompt: "look".into(),
            ..Default::default()
        }
    }

    /// An id taken is refused; the folder skips what is not an agent's file
    /// and names what it cannot read; an agent's own folder goes with it,
    /// and one that cannot be removed is said by its path.
    #[test]
    fn agent_ids_are_taken_once_and_the_folder_skips_strangers() {
        let (_d, ws) = ws5();
        let id = ws.add_agent(scout()).unwrap().id;
        let err = ws
            .add_agent_with_id(&id, scout(), AgentOrigin::Local)
            .unwrap_err();
        assert!(matches!(&err, StoreError::Invalid(_)), "{err:?}");
        let agents = ws.paths.agents_dir();
        std::fs::write(agents.join("Bad Name.json"), b"{}").unwrap();
        std::fs::write(agents.join("README"), b"").unwrap();
        let before = ws.list_agents().unwrap().len();
        let own = ws.paths.agent(&id).dir().to_path_buf();
        std::fs::create_dir_all(own.join("work")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let file = ws.paths.agent_file(&id);
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
            let unreadable = ws.get_agent(&id);
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(matches!(unreadable, Err(StoreError::Io { .. })));
            std::fs::set_permissions(&agents, std::fs::Permissions::from_mode(0o000)).unwrap();
            let unreadable = ws.list_agents();
            std::fs::set_permissions(&agents, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert!(matches!(unreadable, Err(StoreError::Io { .. })));
            let sealed = own.join("work").join("sealed");
            std::fs::create_dir_all(&sealed).unwrap();
            std::fs::write(sealed.join("f"), b"").unwrap();
            std::fs::set_permissions(&sealed, std::fs::Permissions::from_mode(0o000)).unwrap();
            let unremovable = ws.remove_agent(&id);
            std::fs::set_permissions(&sealed, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert!(
                matches!(unremovable, Err(StoreError::Io { .. })),
                "{unremovable:?}"
            );
            // The record went before its folder; the next removal finishes.
            assert!(!ws.paths.agent_file(&id).exists());
        }
        ws.remove_agent(&id).unwrap();
        assert!(!own.exists(), "the agent's own folder went with it");
        assert_eq!(ws.list_agents().unwrap().len(), before - 1);
    }

    /// A removal that stops at the agent's folder has already taken the
    /// record, its index row and its snapshot and said so; asked again it
    /// finishes — the folder goes — and says nothing a second time. Asked a
    /// third time there is no such agent.
    #[cfg(unix)]
    #[test]
    fn a_removal_that_failed_midway_is_finished_by_the_next() {
        use std::os::unix::fs::PermissionsExt;
        let (_d, ws) = ws5();
        let agent = ws.add_agent(scout()).unwrap();
        let id = agent.id.clone();
        let own = ws.paths.agent(&id).dir().to_path_buf();
        let sealed = own.join("work");
        std::fs::create_dir_all(&sealed).unwrap();
        std::fs::write(sealed.join("f"), b"").unwrap();
        std::fs::set_permissions(&sealed, std::fs::Permissions::from_mode(0o000)).unwrap();
        let mut rx = ws.subscribe_store_events();
        let said = |rx: &mut tokio::sync::broadcast::Receiver<StoreEvent>| {
            let mut n = 0;
            while let Ok(ev) = rx.try_recv() {
                if matches!(ev, StoreEvent::DefinitionRemoved { .. }) {
                    n += 1;
                }
            }
            n
        };

        let stopped = ws.remove_agent(&id);
        std::fs::set_permissions(&sealed, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(stopped, Err(StoreError::Io { .. })), "{stopped:?}");
        assert!(!ws.paths.agent_file(&id).exists(), "the record went");
        assert!(ws.list_agents().unwrap().iter().all(|a| a.id != id));
        assert_eq!(
            ws.idx().agent_id_for_pubkey(agent.pubkey.as_hex()).unwrap(),
            None,
            "no row is left to attribute a fact to it"
        );
        assert!(ws
            .snapshots
            .get_raw(Paths::NS_AGENTS, KIND_AGENT_PROFILE, id.as_str())
            .unwrap()
            .is_none());
        assert_eq!(said(&mut rx), 1, "said when the record went");
        assert!(own.exists(), "the folder is what was left");

        ws.remove_agent(&id).unwrap();
        assert!(!own.exists(), "the next removal finished it");
        assert_eq!(said(&mut rx), 0, "and said nothing twice");
        let gone = ws.remove_agent(&id).unwrap_err();
        assert!(
            matches!(gone, StoreError::DefinitionNotFound { .. }),
            "{gone:?}"
        );
    }

    /// An agent whose record cannot be read still gives its key back: the
    /// index held it.
    #[test]
    fn an_unreadable_agents_key_goes_by_what_the_index_held() {
        let (_d, ws) = ws5();
        let agent = ws.add_agent(scout()).unwrap();
        std::fs::write(ws.paths.agent_file(&agent.id), b"{").unwrap();
        assert!(ws.identity.agent(agent.pubkey.as_hex()).is_ok());
        ws.remove_agent(&agent.id).unwrap();
        assert!(ws.identity.agent(agent.pubkey.as_hex()).is_err());
    }

    /// One record the disk will not give — its permissions, a failing
    /// sector — costs that one agent, team or skill: every list still
    /// answers with the rest, so every dispatch, picker and rebuild that
    /// reads the roster goes on. Asked for by its id, it is still the error
    /// it is.
    #[cfg(unix)]
    #[test]
    fn one_agent_file_nobody_may_read_costs_that_agent_alone() {
        use std::os::unix::fs::PermissionsExt;
        let (_d, ws) = ws5();
        let kept = ws.add_agent(scout()).unwrap();
        let lost = ws.add_agent(scout()).unwrap();
        let team_kept = ws
            .create_team("Kept", None, vec![], Tags::default())
            .unwrap();
        let team_lost = ws
            .create_team("Lost", None, vec![], Tags::default())
            .unwrap();
        let skill = |id: &str| {
            ws.create_skill(crate::skills::NewSkill {
                id: bisa_core::SkillId::new(id).unwrap(),
                name: id.into(),
                description: "a skill".into(),
                tags: Tags::default(),
                markdown: "do it".into(),
            })
            .unwrap()
        };
        let (skill_kept, skill_lost) = (skill("kept"), skill("lost"));
        let sealed = [
            ws.paths.agent_file(&lost.id),
            ws.paths.team_file(&team_lost.id),
            ws.paths.skill_file(&skill_lost.id),
        ];
        for file in &sealed {
            std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o000)).unwrap();
        }
        let agents = ws.list_agents();
        let teams = ws.list_teams();
        let skills = ws.list_skills();
        let asked = ws.get_agent(&lost.id);
        let rebuilt = ws.rebuild_index();
        for file in &sealed {
            std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        let agents: Vec<AgentId> = agents.unwrap().into_iter().map(|a| a.id).collect();
        assert!(agents.contains(&kept.id), "{agents:?}");
        assert!(!agents.contains(&lost.id), "{agents:?}");
        let teams: Vec<_> = teams.unwrap().into_iter().map(|t| t.id).collect();
        assert_eq!(teams, vec![team_kept.id]);
        let skills: Vec<_> = skills.unwrap().into_iter().map(|s| s.id).collect();
        assert!(skills.contains(&skill_kept.id), "{skills:?}");
        assert!(!skills.contains(&skill_lost.id), "{skills:?}");
        assert!(matches!(asked, Err(StoreError::Io { .. })), "{asked:?}");
        rebuilt.unwrap();
    }

    /// Two agent files that carry one key — a file copied by hand — cannot
    /// both have the index's row. The rebuild keeps the first, names the
    /// other among the workspace's problems, and finishes.
    #[test]
    fn two_files_with_one_key_do_not_stop_a_rebuild() {
        let (_d, ws) = ws5();
        let agent = ws.add_agent(scout()).unwrap();
        let copy = AgentId::new("copy").unwrap();
        let mut stored = ws.read_agent_file(&agent.id).unwrap();
        stored.def.id = copy.clone();
        ws.write_agent_file(&stored).unwrap();
        ws.rebuild_index().unwrap();
        let problems = ws.problems();
        assert!(
            problems
                .iter()
                .any(|p| p.kind == crate::problems::ProblemKind::IndexDisagrees),
            "{problems:?}"
        );
        assert!(ws
            .idx()
            .agent_id_for_pubkey(agent.pubkey.as_hex())
            .unwrap()
            .is_some());
    }

    /// A record cut short whose row the index lost too has no key to go by:
    /// it is removed all the same, and its key is left where it is. A
    /// removal of an agent called `state` that stopped after its record is
    /// finished by what is its own in the shared folder. A writer that
    /// panicked holding the roster's lock costs nobody the roster.
    #[test]
    fn a_removal_goes_on_without_a_row_and_past_a_poisoned_writer() {
        let (_d, ws) = ws5();
        let agent = ws.add_agent(scout()).unwrap();
        std::fs::write(ws.paths.agent_file(&agent.id), b"{").unwrap();
        ws.idx().delete_agent(agent.id.as_str()).unwrap();
        ws.remove_agent(&agent.id).unwrap();
        assert!(!ws.paths.agent_file(&agent.id).exists());
        assert!(ws.identity.agent(agent.pubkey.as_hex()).is_ok());

        let state = AgentId::new("state").unwrap();
        let own = ws.paths.agent(&state);
        std::fs::create_dir_all(own.scratch()).unwrap();
        ws.remove_agent(&state).unwrap();
        assert!(!own.scratch().exists());
        assert!(own.dir().exists(), "the snapshots' folder stays");
        let gone = ws.remove_agent(&state).unwrap_err();
        assert!(
            matches!(gone, StoreError::DefinitionNotFound { .. }),
            "{gone:?}"
        );

        let ws = std::sync::Arc::new(ws);
        let held = ws.clone();
        let panicked = std::thread::spawn(move || {
            let _writer = held.roster_writer();
            panic!("a writer that fell over");
        })
        .join();
        assert!(panicked.is_err());
        ws.add_agent(scout()).unwrap();
    }

    /// A key the file store cannot remove is said, and the agent still goes.
    #[cfg(unix)]
    #[test]
    fn an_agent_whose_key_cannot_be_removed_still_goes() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let identity = Paths::new(dir.path()).identity_dir();
        let ws = Workspace::open_with_keystore(
            dir.path(),
            Box::new(crate::identity::FileKeyStore::new(identity.clone())),
        )
        .unwrap();
        let id = ws.add_agent(scout()).unwrap().id;
        std::fs::set_permissions(&identity, std::fs::Permissions::from_mode(0o500)).unwrap();
        let removed = ws.remove_agent(&id);
        std::fs::set_permissions(&identity, std::fs::Permissions::from_mode(0o700)).unwrap();
        removed.unwrap();
        assert!(ws.get_agent(&id).is_err());
    }

    // --- the id `state`, and an agent's own folder ---

    /// Every 33403 snapshot under `agents/state/`, with its bytes.
    fn snapshots(ws: &Workspace) -> Vec<(String, Vec<u8>)> {
        let dir = ws.paths.state_dir(Paths::NS_AGENTS);
        let mut out: Vec<(String, Vec<u8>)> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("33403-"))
            .map(|e| {
                (
                    e.file_name().to_string_lossy().into_owned(),
                    std::fs::read(e.path()).unwrap(),
                )
            })
            .collect();
        out.sort();
        out
    }

    /// `agents/state/` is where every agent's snapshot is kept: no agent's
    /// own folder may be it. An agent named *State* is given another id, an
    /// id chosen for it is refused, and a name whose folder a removed agent
    /// left behind is not handed that folder.
    #[test]
    fn an_agent_named_state_takes_another_id() {
        let (_d, ws) = ws5();
        let named = ws
            .add_agent(NewAgent {
                name: "State".into(),
                ..scout()
            })
            .unwrap();
        assert_ne!(named.id.as_str(), "state");
        assert!(named.id.as_str().starts_with("state-"), "{}", named.id);
        assert_ne!(
            ws.paths.agent(&named.id).dir(),
            ws.paths.state_dir(Paths::NS_AGENTS)
        );
        let refused = ws
            .add_agent_with_id(&AgentId::new("state").unwrap(), scout(), AgentOrigin::Local)
            .unwrap_err();
        assert!(matches!(&refused, StoreError::Invalid(_)), "{refused:?}");
        assert!(
            !ws.paths
                .agent_file(&AgentId::new("state").unwrap())
                .exists(),
            "nothing was written for it"
        );
        // A folder somebody left under a free name is nobody's to inherit.
        std::fs::create_dir_all(ws.paths.agent(&AgentId::new("ghost").unwrap()).scratch()).unwrap();
        let ghost = ws
            .add_agent(NewAgent {
                name: "Ghost".into(),
                ..scout()
            })
            .unwrap();
        assert_ne!(
            ghost.id.as_str(),
            "ghost",
            "the stranger's folder stays its own"
        );
    }

    /// A workspace made before the id was reserved may hold an agent
    /// `state`: it lists, and removing it takes what is its own — its
    /// scratch, its recall — and leaves every agent's snapshot as it was.
    /// Removing it used to remove the folder, every snapshot with it.
    #[test]
    fn removing_an_agent_called_state_leaves_every_snapshot() {
        let (_d, ws) = ws5();
        let other = ws.add_agent(scout()).unwrap();
        // The record as an older build wrote it: an agent whose id is `state`.
        let seeded = ws
            .add_agent(NewAgent {
                name: "Older".into(),
                ..scout()
            })
            .unwrap();
        let state = AgentId::new("state").unwrap();
        let from = ws.paths.agent_file(&seeded.id);
        let mut record: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&from).unwrap()).unwrap();
        record["id"] = serde_json::json!("state");
        std::fs::write(
            ws.paths.agent_file(&state),
            serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();
        std::fs::remove_file(&from).unwrap();
        ws.snapshots
            .delete_snapshot(Paths::NS_AGENTS, KIND_AGENT_PROFILE, seeded.id.as_str())
            .unwrap();
        ws.rebuild_index().unwrap();
        assert!(ws.list_agents().unwrap().iter().any(|a| a.id == state));
        // What is its own, inside the folder it shares with the snapshots.
        let own = ws.paths.agent(&state);
        std::fs::create_dir_all(own.scratch().join("work")).unwrap();
        std::fs::write(own.scratch().join("work").join("f"), b"x").unwrap();
        std::fs::write(own.recall_index(), b"{}").unwrap();
        std::fs::create_dir_all(own.recall().join("state")).unwrap();
        let before = snapshots(&ws);
        assert!(
            before
                .iter()
                .any(|(name, _)| name.contains(other.id.as_str())),
            "{before:?}"
        );

        ws.remove_agent(&state).unwrap();
        assert!(ws.get_agent(&state).is_err());
        assert_eq!(snapshots(&ws), before, "every snapshot, byte for byte");
        assert!(!own.scratch().exists() && !own.recall_index().exists());
        assert!(!own.recall().exists());
        assert!(ws.get_agent(&other.id).is_ok());
    }

    // --- one writer a record ---

    fn skill(ws: &Workspace, id: &str) -> SkillId {
        ws.create_skill(crate::skills::NewSkill {
            id: SkillId::new(id).unwrap(),
            name: id.to_string(),
            description: "a skill".into(),
            tags: Default::default(),
            markdown: "# do it".into(),
        })
        .unwrap()
        .id
    }

    /// Two edits of one agent at once both stand. Each edit read the whole
    /// record, changed its own part and wrote the whole record back, so the
    /// later write took the earlier one's change with it.
    #[test]
    fn two_edits_of_one_agent_both_stand() {
        let (_d, ws) = ws5();
        let ws = std::sync::Arc::new(ws);
        let agent = ws.add_agent(scout()).unwrap().id;
        let skills: Vec<SkillId> = (0..12).map(|n| skill(&ws, &format!("skill-{n}"))).collect();
        let start = std::sync::Arc::new(std::sync::Barrier::new(skills.len()));
        let writers: Vec<_> = skills
            .iter()
            .cloned()
            .map(|skill| {
                let (ws, agent, start) = (ws.clone(), agent.clone(), start.clone());
                std::thread::spawn(move || {
                    start.wait();
                    ws.attach_skill(&agent, &skill).unwrap();
                })
            })
            .collect();
        for writer in writers {
            writer.join().unwrap();
        }
        let mut carried = ws.get_agent(&agent).unwrap().skills;
        carried.sort();
        let mut wanted = skills.clone();
        wanted.sort();
        assert_eq!(carried, wanted, "every attach is on the record");
    }
}
