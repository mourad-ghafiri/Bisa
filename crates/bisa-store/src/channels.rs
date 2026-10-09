//! Channels: standing conversations and direct messages (kind 33405).
//!
//! The domain type is [`bisa_core::Channel`]; membership is a
//! [`RosterPolicy`] evaluated at read time by [`bisa_core::members`], the
//! one implementation. `general` is the one channel rostered `Everyone`; it is
//! ensured at every open and cannot be deleted — [`DeletableChannel`] has no
//! constructor for it.
//!
//! Truth is the signed snapshot in `channels/state/33405-<id>.json` — a
//! channel has no journal, so the addressable event *is* the record. The
//! `channels` and `channel_roster` index tables are rebuildable.

use crate::error::StoreError;
use crate::paths::Paths;
use crate::problems::{ProblemKind, WorkspaceProblem};
use crate::workspace::{mint_ulid, now_secs, EventAudience, StoreEvent, Workspace};
use bisa_core::kind::KIND_CHANNEL;
use bisa_core::tags::TagEntity;
use bisa_core::{
    AgentId, Audience, Channel, ChannelId, ChannelKind, ChannelOrigin, DeletableChannel, Member,
    PrincipalId, RosterPolicy, Tags, TeamId,
};
use serde::Deserialize;
use std::path::Path;

/// `library/core/general.toml` — the copy for the one permanent channel.
const GENERAL_TOML: &str = include_str!("../../../library/core/general.toml");

#[derive(Deserialize)]
struct GeneralFile {
    channel: GeneralBody,
}

#[derive(Deserialize)]
struct GeneralBody {
    name: String,
    #[serde(default)]
    topic: Option<String>,
}

impl Workspace {
    fn write_channel(&self, def: &Channel) -> Result<(), StoreError> {
        def.validate()?;
        let existing_rev =
            self.snapshots
                .current_revision(Paths::NS_CHANNELS, KIND_CHANNEL, def.id.as_str())?;
        let event = self.snapshots.put(
            Paths::NS_CHANNELS,
            KIND_CHANNEL,
            def.id.as_str(),
            def,
            existing_rev + 1,
            &self.owner,
            now_secs().max(existing_rev),
            None,
            def.tags.as_slice(),
        )?;
        self.emit_store_event(StoreEvent::ConversationSnapshot {
            kind: KIND_CHANNEL,
            d: def.id.to_string(),
            event,
            audience: EventAudience::from(&def.audience),
        });
        self.index_channel(def)
    }

    pub(crate) fn index_channel(&self, def: &Channel) -> Result<(), StoreError> {
        let idx = self.idx();
        idx.upsert_channel(
            def.id.as_str(),
            &def.name,
            def.kind.as_str(),
            def.topic.as_deref(),
            &serde_json::to_string(&def.audience)?,
            def.roster.as_str(),
            def.origin.as_str(),
            def.created_at,
        )?;
        let roster: Vec<(&str, &str)> = match &def.roster {
            RosterPolicy::Everyone => vec![],
            RosterPolicy::Listed {
                agents,
                teams,
                humans,
            } => agents
                .iter()
                .map(|a| ("agent", a.as_str()))
                .chain(teams.iter().map(|t| ("team", t.as_str())))
                .chain(humans.iter().map(|p| ("human", p.as_hex())))
                .collect(),
        };
        idx.set_channel_roster(def.id.as_str(), &roster)?;
        idx.set_tags(TagEntity::Channel, def.id.as_str(), def.tags.as_slice())
    }

    /// Every rostered agent, team and person must exist. A core agent is
    /// dropped from a `Listed` roster: it is in every room already, and
    /// storing it would put it in a list an edit could remove it from — and
    /// so is the owner, for the same reason.
    fn vetted_roster(&self, roster: RosterPolicy) -> Result<RosterPolicy, StoreError> {
        match roster {
            RosterPolicy::Everyone => Ok(RosterPolicy::Everyone),
            RosterPolicy::Listed {
                agents,
                teams,
                humans,
            } => {
                let mut out_agents: Vec<AgentId> = Vec::with_capacity(agents.len());
                for a in agents {
                    if a.is_core_id() {
                        continue;
                    }
                    self.get_agent(&a).map_err(|_| {
                        StoreError::Invalid(bisa_core::text!(
                            "error-store-invalid-channel-roster-names-unknown-agent",
                            a = a.to_string()
                        ))
                    })?;
                    if !out_agents.contains(&a) {
                        out_agents.push(a);
                    }
                }
                let mut out_teams: Vec<TeamId> = Vec::with_capacity(teams.len());
                for t in teams {
                    self.get_team(&t).map_err(|_| {
                        StoreError::Invalid(bisa_core::text!(
                            "error-store-invalid-channel-roster-names-unknown-team",
                            t = t.to_string()
                        ))
                    })?;
                    if !out_teams.contains(&t) {
                        out_teams.push(t);
                    }
                }
                let owner = self.owner_principal();
                let mut out_humans: Vec<PrincipalId> = Vec::with_capacity(humans.len());
                for p in humans {
                    if p == owner {
                        continue;
                    }
                    if !self.is_member(&p)? {
                        return Err(StoreError::Invalid(bisa_core::text!(
                            "error-store-invalid-channel-roster-names-who-not-person-workspace",
                            p = p.to_string()
                        )));
                    }
                    if !out_humans.contains(&p) {
                        out_humans.push(p);
                    }
                }
                Ok(RosterPolicy::Listed {
                    agents: out_agents,
                    teams: out_teams,
                    humans: out_humans,
                })
            }
        }
    }

    /// Put people on a standing channel's roster, or take them off — the
    /// agent and team halves untouched. What an invite's channels and the
    /// People panel edit.
    pub fn set_roster_humans(
        &self,
        id: &ChannelId,
        humans: Vec<PrincipalId>,
    ) -> Result<Channel, StoreError> {
        let stored = self.get_channel(id)?;
        if stored.kind == ChannelKind::Direct || stored.is_general() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-has-no-roster-people-edit",
                id = id.to_string()
            )));
        }
        let (agents, teams) = match &stored.roster {
            RosterPolicy::Listed { agents, teams, .. } => (agents.clone(), teams.clone()),
            RosterPolicy::Everyone => (vec![], vec![]), // LCOV_EXCL_LINE: only general's roster is everyone's, and general was refused above
        };
        let roster = self.vetted_roster(RosterPolicy::Listed {
            agents,
            teams,
            humans,
        })?;
        let def = Channel { roster, ..stored };
        self.write_channel(&def)?;
        Ok(def)
    }

    /// Take one person off every roster that names them — they left.
    pub fn unroster_human_everywhere(&self, person: &PrincipalId) -> Result<usize, StoreError> {
        let mut touched = 0;
        for channel in self.list_channels()? {
            if !channel.roster.lists_human(person) {
                continue;
            }
            let humans: Vec<PrincipalId> = channel
                .roster
                .humans()
                .iter()
                .filter(|p| *p != person)
                .cloned()
                .collect();
            self.set_roster_humans(&channel.id, humans)?;
            touched += 1;
        }
        Ok(touched)
    }

    /// The standing channels a person reaches ([`bisa_core::reaches`]) —
    /// what the host tells them about, and what their pump wraps to them.
    pub fn channels_reached_by(
        &self,
        person: &PrincipalId,
        role: bisa_core::MemberRole,
    ) -> Result<Vec<Channel>, StoreError> {
        Ok(self
            .list_channels()?
            .into_iter()
            .filter(|c| bisa_core::reaches(c, role, person))
            .collect())
    }

    fn mint_channel_id(&self, name: &str) -> Result<ChannelId, StoreError> {
        let base = crate::agents::slugify(name).unwrap_or_else(|| "channel".to_string());
        let candidate = ChannelId::new(&base)?;
        if !candidate.is_general() && self.get_channel(&candidate).is_err() {
            return Ok(candidate);
        }
        let suffix = mint_ulid().to_string().to_ascii_lowercase();
        ChannelId::new(format!("{base}-{}", &suffix[suffix.len() - 6..])).map_err(Into::into)
    }

    pub fn create_channel(
        &self,
        name: &str,
        topic: Option<&str>,
        roster: RosterPolicy,
        tags: Tags,
    ) -> Result<Channel, StoreError> {
        let id = self.mint_channel_id(name)?;
        self.create_channel_with_id(&id, name, topic, roster, tags, ChannelOrigin::Local)
    }

    pub(crate) fn create_channel_with_id(
        &self,
        id: &ChannelId,
        name: &str,
        topic: Option<&str>,
        roster: RosterPolicy,
        tags: Tags,
        origin: ChannelOrigin,
    ) -> Result<Channel, StoreError> {
        if name.trim().is_empty() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-channel-needs-name"
            )));
        }
        if self.get_channel(id).is_ok() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-channel-already-exists",
                id = id.to_string()
            )));
        }
        let def = Channel {
            id: id.clone(),
            name: name.to_string(),
            topic: topic.map(str::to_string),
            kind: ChannelKind::Standing,
            audience: Audience::Workspace,
            roster: self.vetted_roster(roster)?,
            tags,
            origin,
            created_at: now_secs(),
        };
        self.write_channel(&def)?;
        Ok(def)
    }

    /// Replace a standing channel's roster, topic and tags. The audience and
    /// the kind are not editable: changing an audience changes who a past
    /// message was encrypted to. `general`'s roster is the policy, not a list.
    pub fn update_channel(
        &self,
        id: &ChannelId,
        topic: Option<&str>,
        roster: RosterPolicy,
        tags: Tags,
    ) -> Result<Channel, StoreError> {
        let stored = self.get_channel(id)?;
        if stored.kind == ChannelKind::Direct {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-direct-message-has-no-roster-topic-edit"
            )));
        }
        let roster = if stored.is_general() {
            RosterPolicy::Everyone
        } else {
            self.vetted_roster(roster)?
        };
        let def = Channel {
            topic: topic.map(str::to_string),
            roster,
            tags,
            ..stored
        };
        self.write_channel(&def)?;
        Ok(def)
    }

    pub fn get_channel(&self, id: &ChannelId) -> Result<Channel, StoreError> {
        self.snapshots
            .get::<Channel>(Paths::NS_CHANNELS, KIND_CHANNEL, id.as_str())?
            .map(|(c, _)| c)
            .ok_or_else(|| StoreError::DefinitionNotFound {
                kind: "channel",
                id: id.to_string(),
            })
    }

    /// Every channel of both kinds, in creation order. The raw enumerator —
    /// anything that means "the rooms a person joins" asks
    /// [`Self::list_channels_of_kind`].
    pub fn list_channels(&self) -> Result<Vec<Channel>, StoreError> {
        let mut out = Vec::new();
        for d in self.snapshots.list_ds(Paths::NS_CHANNELS, KIND_CHANNEL)? {
            // One channel this build cannot read costs its row, never the
            // list — nor the rebuild, nor the Inbox that lists the rooms.
            if let Some((c, _)) = self
                .tolerated_record(
                    "channel",
                    &d,
                    self.snapshots
                        .get::<Channel>(Paths::NS_CHANNELS, KIND_CHANNEL, &d),
                )?
                .flatten()
            {
                out.push(c);
            }
        }
        out.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));
        Ok(out)
    }

    pub fn list_channels_of_kind(&self, kind: ChannelKind) -> Result<Vec<Channel>, StoreError> {
        Ok(self
            .list_channels()?
            .into_iter()
            .filter(|c| c.kind == kind)
            .collect())
    }

    /// Open (or find) the direct message for exactly this set of participants.
    /// The owner is always included; a different set is a different channel.
    pub fn open_dm(&self, participants: &[PrincipalId]) -> Result<Channel, StoreError> {
        let mut audience: Vec<PrincipalId> = participants.to_vec();
        let owner = self.owner_principal();
        if !audience.contains(&owner) {
            audience.push(owner.clone());
        }
        audience.sort_by(|a, b| a.as_hex().cmp(b.as_hex()));
        audience.dedup();
        if audience.len() < 2 {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-direct-message-needs-least-one-other-participant"
            )));
        }
        // A participant is a person of this workspace or one of its agents;
        // a stranger's key opens nothing here.
        let agent_keys: Vec<PrincipalId> =
            self.list_agents()?.into_iter().map(|a| a.pubkey).collect();
        for p in &audience {
            if *p == owner || agent_keys.contains(p) || self.is_member(p)? {
                continue;
            }
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-neither-person-workspace-nor-one-agents",
                p = p.to_string()
            )));
        }
        for channel in self.list_channels_of_kind(ChannelKind::Direct)? {
            if channel.audience.principals() == audience.as_slice() {
                return Ok(channel);
            }
        }
        let name = audience
            .iter()
            .map(|p| p.as_hex()[..8].to_string())
            .collect::<Vec<_>>()
            .join(" · ");
        let def = Channel {
            id: ChannelId::new(format!(
                "dm-{}",
                mint_ulid().to_string().to_ascii_lowercase()
            ))?,
            name,
            topic: None,
            kind: ChannelKind::Direct,
            audience: Audience::Restricted(audience),
            // A DM's participants are its audience; a roster on top would be
            // the same list said twice.
            roster: RosterPolicy::default(),
            tags: Tags::default(),
            origin: ChannelOrigin::Local,
            created_at: now_secs(),
        };
        self.write_channel(&def)?;
        Ok(def)
    }

    /// Who belongs to a channel, derived: [`bisa_core::members`] over the
    /// enabled agents and teams, and the people of the workspace.
    pub fn channel_members(&self, id: &ChannelId) -> Result<Vec<Member>, StoreError> {
        let channel = self.get_channel(id)?;
        let agents = self.list_agents()?;
        let teams = self.list_teams()?;
        let people = self.people_pubkeys()?;
        Ok(bisa_core::members(&channel, &agents, &teams, &people))
    }

    /// The pubkeys a channel's roster resolves to — what `@<channel>` expands
    /// to. Teams expand one level to their enabled agents; the two core
    /// agents are in every room and come last, in their fixed order, because
    /// the roster's order is the `@`-picker's order.
    pub fn channel_roster_pubkeys(&self, id: &ChannelId) -> Result<Vec<PrincipalId>, StoreError> {
        let agents = self.list_agents()?;
        let mut ids: Vec<AgentId> = Vec::new();
        for m in self.channel_members(id)? {
            match m {
                Member::Agent(a) => {
                    if !ids.contains(&a) {
                        ids.push(a);
                    }
                }
                Member::Team(t) => {
                    for a in self.team_agents(&t).unwrap_or_default() {
                        if !ids.contains(&a) {
                            ids.push(a);
                        }
                    }
                }
                // A person is addressed by name, never through the handle:
                // `@design` wakes the room's agents, not its humans.
                Member::Human(_) => {}
            }
        }
        // The core agents are in every room, last and in their fixed order,
        // whatever the roster stored.
        ids.retain(|a| !a.is_core_id());
        for core in AgentId::CORE {
            if let Ok(id) = AgentId::new(core) {
                ids.push(id);
            }
        }
        Ok(ids
            .into_iter()
            .filter_map(|id| agents.iter().find(|a| a.id == id && a.enabled))
            .map(|a| a.pubkey.clone())
            .collect())
    }

    /// Delete a channel. Taking a [`DeletableChannel`] is what makes deleting
    /// `general` unrepresentable; a channel something still points at is
    /// refused by name ([`crate::usage`]). Its message log stays on disk as
    /// history.
    pub fn delete_channel(&self, channel: DeletableChannel) -> Result<(), StoreError> {
        let id = channel.channel().id.clone();
        self.refuse_channel_delete_if_used(&channel)?;
        self.snapshots
            .delete_snapshot(Paths::NS_CHANNELS, KIND_CHANNEL, id.as_str())?;
        // Its notes and drawings live in their repositories: they leave with
        // the channel.
        let scope = bisa_core::OwnerScope::Channel { id: id.clone() };
        let scoped = |base: std::path::PathBuf| {
            crate::paths::Paths::scoped_dir(&base, "channel", Some(id.as_str()))
        };
        self.remove_notes_of(scope.clone(), &scoped(self.paths.notes_dir()))?;
        self.remove_drawings_of(scope, &scoped(self.paths.drawings_dir()))?;
        let idx = self.idx();
        idx.delete_channel(id.as_str())?;
        idx.clear_tags(TagEntity::Channel, id.as_str())
    }

    /// The refusal `delete_channel` would answer, before anything is done:
    /// the channel is there and nothing points at it. The engine asks first,
    /// so it stops no live turn for a delete the store then refuses.
    pub fn refuse_channel_delete_if_used(
        &self,
        channel: &DeletableChannel,
    ) -> Result<(), StoreError> {
        let id = &channel.channel().id;
        self.get_channel(id)?;
        self.refuse_if_used(crate::usage::UsageKind::Channel, id.as_str())
    }

    /// Create `general` if this workspace does not have it, and leave it alone
    /// if it does. Called at every open; the only mechanism that restores it.
    pub fn ensure_general_channel(&self) -> Result<(), StoreError> {
        let id = ChannelId::general();
        match self.get_channel(&id) {
            Ok(_) => return Ok(()),
            // A `general` this build cannot read is moved aside and made
            // again: the room every workspace has is never the reason a
            // workspace does not open.
            Err(StoreError::Unreadable { path, what, reason }) => {
                let to = self.paths.quarantine(Path::new(&path), now_secs())?;
                self.record_problem(WorkspaceProblem::new(
                    ProblemKind::Recreated,
                    path.clone(),
                    bisa_core::text!(
                        "error-store-problem-quarantined",
                        path = path,
                        what = what.to_string(),
                        reason = reason,
                        to = to.display().to_string()
                    ),
                    Some(&to),
                ));
            }
            Err(_) => {}
        }
        let copy: GeneralFile = toml::from_str(GENERAL_TOML).map_err(|e| {
            // LCOV_EXCL_START: the bundled file is held well-formed by the bundle tests
            StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-library-core-general-toml",
                e = e.to_string()
            ))
            // LCOV_EXCL_STOP
        })?;
        let mut def = Channel::general(now_secs());
        def.name = copy.channel.name;
        def.topic = copy.channel.topic;
        self.write_channel(&def)
    }

    pub(crate) fn reindex_channels(&self) -> Result<(), StoreError> {
        for c in self.list_channels()? {
            self.index_channel(&c)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::NewAgent;
    use crate::identity::MemoryKeyStore;
    use bisa_core::Assignee;

    fn ws() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let ws =
            Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
        (dir, ws)
    }

    fn agent(ws: &Workspace, name: &str) -> AgentId {
        ws.add_agent(NewAgent {
            name: name.into(),
            harness: "mock".into(),
            system_prompt: "x".into(),
            ..Default::default()
        })
        .unwrap()
        .id
    }

    #[test]
    fn general_is_seeded_everyone_and_restored_when_its_truth_file_goes() {
        let (dir, ws) = ws();
        let g = ws.get_channel(&ChannelId::general()).unwrap();
        assert_eq!(g.roster, RosterPolicy::Everyone);
        assert_eq!(g.origin, ChannelOrigin::Core);
        assert!(g.topic.as_deref().unwrap_or_default().contains("home"));
        assert!(matches!(
            DeletableChannel::new(g.clone()),
            Err(bisa_core::ChannelError::Permanent { .. })
        ));
        // Membership is derived from enablement: the General Agent and the Workflow Agent are there.
        assert_eq!(
            ws.channel_members(&g.id).unwrap(),
            vec![
                Member::Agent(AgentId::general()),
                Member::Agent(AgentId::workflow())
            ]
        );
        // Remove the truth file and reopen: it is back.
        let path = ws
            .paths()
            .state_dir(Paths::NS_CHANNELS)
            .join(format!("{KIND_CHANNEL}-general.json"));
        assert!(path.exists());
        std::fs::rename(&path, path.with_extension("json.aside")).unwrap();
        drop(ws);
        let ws2 =
            Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
        assert!(ws2.get_channel(&ChannelId::general()).is_ok());
    }

    #[test]
    fn a_listed_roster_is_vetted_filtered_by_enablement_and_expands_teams() {
        let (_dir, ws) = ws();
        let dev = agent(&ws, "Developer");
        let qa = agent(&ws, "QA");
        let team = ws
            .create_team(
                "Engineering",
                None,
                vec![Assignee::Agent(dev.to_string())],
                Tags::default(),
            )
            .unwrap();
        assert!(ws
            .create_channel(
                "eng",
                None,
                RosterPolicy::Listed {
                    agents: vec![AgentId::new("ghost").unwrap()],
                    teams: vec![],
                    humans: vec![],
                },
                Tags::default()
            )
            .is_err());
        let c = ws
            .create_channel(
                "Engineering room",
                Some("code"),
                RosterPolicy::Listed {
                    agents: vec![qa.clone(), AgentId::general()],
                    teams: vec![team.id.clone()],
                    humans: vec![],
                },
                Tags::default(),
            )
            .unwrap();
        assert_eq!(c.id.as_str(), "engineering-room");
        assert!(
            matches!(&c.roster, RosterPolicy::Listed { agents, .. } if agents == &vec![qa.clone()]),
            "the general agent is never stored in a roster"
        );
        assert_eq!(
            ws.channel_members(&c.id).unwrap(),
            vec![Member::Agent(qa.clone()), Member::Team(team.id.clone())]
        );
        let pks = ws.channel_roster_pubkeys(&c.id).unwrap();
        assert_eq!(
            pks.len(),
            4,
            "qa, dev through the team, and the General Agent and the Workflow Agent last"
        );
        assert_eq!(pks[2], ws.get_agent(&AgentId::general()).unwrap().pubkey);
        assert_eq!(pks[3], ws.get_agent(&AgentId::workflow()).unwrap().pubkey);

        ws.set_agent_enabled(&qa, false).unwrap();
        assert_eq!(
            ws.channel_members(&c.id).unwrap(),
            vec![Member::Team(team.id.clone())]
        );
        ws.set_team_enabled(&team.id, false).unwrap();
        assert!(ws.channel_members(&c.id).unwrap().is_empty());
        // Only general may be `everyone`.
        assert!(ws
            .update_channel(&c.id, None, RosterPolicy::Everyone, Tags::default())
            .is_err());
        ws.delete_channel(DeletableChannel::new(c.clone()).unwrap())
            .unwrap();
        assert!(ws.get_channel(&c.id).is_err());
    }

    #[test]
    fn direct_messages_are_channels_with_a_restricted_audience() {
        let (_dir, ws) = ws();
        let other = PrincipalId::new(nostr::key::Keys::generate().public_key().to_hex()).unwrap();
        // A direct channel is between people of this workspace and its agents; a stranger is refused by name.
        let refused = ws.open_dm(std::slice::from_ref(&other)).unwrap_err();
        assert!(
            refused
                .to_string()
                .contains("neither a person of this workspace nor one of its agents"),
            "{refused}"
        );
        ws.add_member(
            other.clone(),
            bisa_core::MemberRole::Member,
            crate::members::Admission::default(),
        )
        .unwrap();
        let dm = ws.open_dm(std::slice::from_ref(&other)).unwrap();
        assert_eq!(dm.kind, ChannelKind::Direct);
        assert!(dm.audience.is_restricted());
        assert_eq!(dm.audience.principals().len(), 2);
        assert_eq!(
            ws.open_dm(std::slice::from_ref(&other)).unwrap().id,
            dm.id,
            "idempotent"
        );
        assert!(ws.open_dm(&[]).is_err());
        assert_eq!(
            ws.list_channels_of_kind(ChannelKind::Standing)
                .unwrap()
                .len(),
            1
        );
        assert!(ws
            .update_channel(&dm.id, Some("t"), RosterPolicy::default(), Tags::default())
            .is_err());
    }

    #[test]
    fn rebuild_restores_channels_and_rosters() {
        let (_dir, ws) = ws();
        let dev = agent(&ws, "Developer");
        let c = ws
            .create_channel(
                "eng",
                None,
                RosterPolicy::Listed {
                    agents: vec![dev.clone()],
                    teams: vec![],
                    humans: vec![],
                },
                Tags::default(),
            )
            .unwrap();
        ws.rebuild_index().unwrap();
        assert_eq!(ws.get_channel(&c.id).unwrap(), c);
        assert_eq!(
            ws.idx().channels_rostering("agent", dev.as_str()).unwrap(),
            vec![c.id.to_string()]
        );
    }

    // added by the coverage pass: channels.rs

    // --- the bare lines of the channels module ---

    /// People are put on a standing channel's roster and never on a direct
    /// message's or general's; the owner is in every room and never listed;
    /// an open roster edited becomes a listed one; general's roster stays
    /// everyone's whatever an edit says; a name taken twice gets a suffix;
    /// a blank name and a taken id are refused; two audiences are two
    /// direct messages.
    #[test]
    fn rosters_are_edited_within_their_rules_and_ids_are_minted_apart() {
        let (_d, ws) = ws();
        let scout = ws.get_agent(&agent(&ws, "Scout")).unwrap().pubkey;
        let ranger = ws.get_agent(&agent(&ws, "Ranger")).unwrap().pubkey;
        let dm = ws.open_dm(std::slice::from_ref(&scout)).unwrap();
        let other = ws.open_dm(std::slice::from_ref(&ranger)).unwrap();
        assert_ne!(dm.id, other.id);
        assert_eq!(ws.open_dm(std::slice::from_ref(&scout)).unwrap().id, dm.id);
        let general = ChannelId::new("general").unwrap();
        assert!(ws.set_roster_humans(&dm.id, vec![]).is_err());
        assert!(ws.set_roster_humans(&general, vec![]).is_err());
        let open = ws
            .create_channel("Open", None, RosterPolicy::default(), Tags::default())
            .unwrap();
        let listed = ws
            .set_roster_humans(&open.id, vec![ws.owner_principal()])
            .unwrap();
        assert!(listed.roster.humans().is_empty(), "{:?}", listed.roster);
        let general_again = ws
            .update_channel(
                &general,
                Some("all hands"),
                RosterPolicy::default(),
                Tags::default(),
            )
            .unwrap();
        assert_eq!(general_again.roster, RosterPolicy::Everyone);
        let twin = ws
            .create_channel("Open", None, RosterPolicy::default(), Tags::default())
            .unwrap();
        assert!(twin.id.as_str().starts_with("open-"), "{}", twin.id);
        assert!(ws
            .create_channel_with_id(
                &ChannelId::new("blank").unwrap(),
                "   ",
                None,
                RosterPolicy::default(),
                Tags::default(),
                ChannelOrigin::Local,
            )
            .is_err());
        assert!(ws
            .create_channel_with_id(
                &open.id,
                "Open",
                None,
                RosterPolicy::default(),
                Tags::default(),
                ChannelOrigin::Local,
            )
            .is_err());
    }
}
