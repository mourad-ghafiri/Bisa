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
use crate::teams::AssigneeStanding;
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
            now_secs(),
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
            // The people of a roster are read from the record: the cache
            // keeps the two halves a removal asks about, agents and teams.
            RosterPolicy::Listed { agents, teams, .. } => agents
                .iter()
                .map(|a| ("agent", a.as_str()))
                .chain(teams.iter().map(|t| ("team", t.as_str())))
                .collect(),
        };
        idx.set_channel_roster(def.id.as_str(), &roster)?;
        idx.set_tags(TagEntity::Channel, def.id.as_str(), def.tags.as_slice())
    }

    /// Every agent, team and person a roster **adds** must exist. A core
    /// agent is dropped from a `Listed` roster: it is in every room already,
    /// and storing it would put it in a list an edit could remove it from —
    /// and so is the owner, for the same reason.
    ///
    /// A name the stored roster already holds stands unasked: membership is
    /// read through [`bisa_core::members`], which leaves out whoever is gone,
    /// so a stale name costs nothing — and asking about it again made every
    /// later edit of the room fail, an invite's admission and a person's
    /// leave among them.
    fn vetted_roster(
        &self,
        roster: RosterPolicy,
        stored: Option<&RosterPolicy>,
    ) -> Result<RosterPolicy, StoreError> {
        let (kept_agents, kept_teams, kept_humans): (&[AgentId], &[TeamId], &[PrincipalId]) =
            match stored {
                Some(RosterPolicy::Listed {
                    agents,
                    teams,
                    humans,
                }) => (agents, teams, humans),
                _ => (&[], &[], &[]),
            };
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
                    let named = bisa_core::Assignee::Agent(a.to_string());
                    if !kept_agents.contains(&a)
                        && self.check_assignee(&named)? != AssigneeStanding::There
                    {
                        return Err(StoreError::Invalid(bisa_core::text!(
                            "error-store-invalid-channel-roster-names-unknown-agent",
                            a = a.to_string()
                        )));
                    }
                    if !out_agents.contains(&a) {
                        out_agents.push(a);
                    }
                }
                let mut out_teams: Vec<TeamId> = Vec::with_capacity(teams.len());
                for t in teams {
                    let named = bisa_core::Assignee::Team(t.to_string());
                    if !kept_teams.contains(&t)
                        && self.check_assignee(&named)? != AssigneeStanding::There
                    {
                        return Err(StoreError::Invalid(bisa_core::text!(
                            "error-store-invalid-channel-roster-names-unknown-team",
                            t = t.to_string()
                        )));
                    }
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
                    if !kept_humans.contains(&p) && !self.is_member(&p)? {
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
    /// agent and team halves untouched, read from the record as it stands
    /// under the one writer. What an invite's channels and the People panel
    /// edit.
    pub fn set_roster_humans(
        &self,
        id: &ChannelId,
        humans: Vec<PrincipalId>,
    ) -> Result<Channel, StoreError> {
        let (_, after) = self.update_channel_with(id, |channel| {
            if channel.kind == ChannelKind::Direct || channel.is_general() {
                return Err(StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-has-no-roster-people-edit",
                    id = id.to_string()
                )));
            }
            let (agents, teams) = match &channel.roster {
                RosterPolicy::Listed { agents, teams, .. } => (agents.clone(), teams.clone()),
                RosterPolicy::Everyone => (vec![], vec![]), // LCOV_EXCL_LINE: only general's roster is everyone's — `Channel::validate` holds it for a written record and for a received one — and general was refused above
            };
            channel.roster = RosterPolicy::Listed {
                agents,
                teams,
                humans,
            };
            Ok(())
        })?;
        Ok(after)
    }

    /// Take one person off every roster that names them — they left. Each
    /// room is edited as it stands when its turn comes, so a roster another
    /// edit changed meanwhile loses this one name and nothing else.
    pub fn unroster_human_everywhere(&self, person: &PrincipalId) -> Result<usize, StoreError> {
        let mut touched = 0;
        for channel in self.list_channels()? {
            if !channel.roster.lists_human(person) {
                continue;
            }
            self.update_channel_with(&channel.id, |room| {
                room.roster.drop_human(person);
                Ok(())
            })?;
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

    /// Whether an id is free to be a new channel's: no record holds it —
    /// one this build cannot read holds it still, and is never written over
    /// — and no log of a room that was is waiting under it to be inherited.
    fn channel_id_is_free(&self, id: &ChannelId) -> Result<bool, StoreError> {
        match self.get_channel(id) {
            Err(StoreError::DefinitionNotFound { .. }) => {
                Ok(!self.paths.conversation_log(id.as_str())?.exists())
            }
            Ok(_) | Err(StoreError::Unreadable { .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }

    fn mint_channel_id(&self, name: &str) -> Result<ChannelId, StoreError> {
        let base = crate::agents::slugify(name).unwrap_or_else(|| "channel".to_string());
        let candidate = ChannelId::new(&base)?;
        if !candidate.is_general() && self.channel_id_is_free(&candidate)? {
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
        // The id is minted and taken under the one writer: two rooms made
        // under one name at once are two rooms, never one written twice.
        let _one_writer = self.roster_writer();
        let id = self.mint_channel_id(name)?;
        self.create_channel_locked(&id, name, topic, roster, tags, ChannelOrigin::Local)
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
        let _one_writer = self.roster_writer();
        self.create_channel_locked(id, name, topic, roster, tags, origin)
    }

    /// The write behind both creates, under the writer the caller holds.
    fn create_channel_locked(
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
        match self.get_channel(id) {
            Ok(_) => {
                return Err(StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-channel-already-exists",
                    id = id.to_string()
                )))
            }
            Err(StoreError::DefinitionNotFound { .. }) => {}
            // A record this build cannot read is there all the same: said
            // as what it is, by its file, and never made again over.
            Err(e) => return Err(e),
        }
        let def = Channel {
            id: id.clone(),
            name: name.to_string(),
            topic: topic.map(str::to_string),
            kind: ChannelKind::Standing,
            audience: Audience::Workspace,
            roster: self.vetted_roster(roster, None)?,
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
        let (_, after) = self.update_channel_with(id, |channel| {
            channel.topic = topic.map(str::to_string);
            channel.roster = roster;
            channel.tags = tags;
            Ok(())
        })?;
        Ok(after)
    }

    /// Edit a standing channel under the one writer, as
    /// [`Workspace::update_team_with`] edits a team: `edit` sees the record
    /// as it stands, and the record before and after are answered — two
    /// edits at once, one of the roster and one of the topic, both stand,
    /// and an edit that lost to a delete finds no room to bring back.
    ///
    /// The topic, the roster and the tags are what an edit may change; the
    /// name, the kind, the audience and the origin are kept whatever `edit`
    /// did to them. A blank topic is no topic.
    pub fn update_channel_with(
        &self,
        id: &ChannelId,
        edit: impl FnOnce(&mut Channel) -> Result<(), StoreError>,
    ) -> Result<(Channel, Channel), StoreError> {
        let _one_writer = self.roster_writer();
        let before = self.get_channel(id)?;
        if before.kind == ChannelKind::Direct {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-direct-message-has-no-roster-topic-edit"
            )));
        }
        let mut next = before.clone();
        edit(&mut next)?;
        let roster = if before.is_general() {
            RosterPolicy::Everyone
        } else {
            self.vetted_roster(next.roster, Some(&before.roster))?
        };
        let after = Channel {
            topic: next.topic.filter(|t| !t.trim().is_empty()),
            roster,
            tags: next.tags,
            ..before.clone()
        };
        self.write_channel(&after)?;
        Ok((before, after))
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
            // One channel this build cannot read — or the disk will not
            // give — costs its row, never the list: nor the rebuild, nor the
            // Inbox that lists the rooms, nor what a guest is told they
            // reach.
            let read = self
                .snapshots
                .get::<Channel>(Paths::NS_CHANNELS, KIND_CHANNEL, &d);
            let kept = if matches!(read, Err(StoreError::Io { .. })) {
                self.tolerated_definition("channel", &d, read)?
            } else {
                self.tolerated_record("channel", &d, read)?
            };
            if let Some((c, _)) = kept.flatten() {
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
    ///
    /// Found before anybody is asked about: the conversation with somebody
    /// who has since left is still the one there is. Looked for and made
    /// under the one writer, so two opens at once are one channel.
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
        let _one_writer = self.roster_writer();
        for channel in self.list_channels_of_kind(ChannelKind::Direct)? {
            if channel.audience.principals() == audience.as_slice() {
                return Ok(channel);
            }
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
    /// refused by name ([`crate::usage`]).
    pub fn delete_channel(&self, channel: DeletableChannel) -> Result<(), StoreError> {
        self.delete_channel_by_id(&channel.id())
    }

    /// Delete a channel by its id — the door a record nobody can read is
    /// deleted through, and the one [`Self::delete_channel`] goes through.
    ///
    /// What others read goes first and the record after it, so a delete
    /// stopped anywhere leaves either the room whole or nothing that reads
    /// as a room, and asked again it is finished. The room is forgotten by
    /// the cache — its messages, its read mark, its searchable words — and
    /// its log is kept aside under a name no scope has
    /// ([`Paths::conversation_log_archive`]): nothing of it resurfaces, and a
    /// channel made again under the id starts empty.
    pub fn delete_channel_by_id(
        &self,
        id: &bisa_core::DeletableChannelId,
    ) -> Result<(), StoreError> {
        let _one_writer = self.roster_writer();
        let id = id.as_id();
        let was_there = match self.get_channel(id) {
            Ok(_) => {
                self.refuse_if_used(crate::usage::UsageKind::Channel, id.as_str())?;
                true
            }
            // A record nobody can read is deleted by its id, once nothing
            // points at the id.
            Err(StoreError::Unreadable { .. }) => {
                self.refuse_if_id_used(crate::usage::UsageKind::Channel, id.as_str(), None)?;
                true
            }
            // A delete that stopped after the record went is finished here,
            // and nothing is said twice.
            Err(StoreError::DefinitionNotFound { .. }) if self.channel_remains(id)? => false,
            Err(e) => return Err(e),
        };
        {
            let idx = self.idx();
            idx.in_transaction(|| {
                idx.delete_channel(id.as_str())?;
                idx.clear_tags(TagEntity::Channel, id.as_str())?;
                idx.delete_scope_facts(id.as_str())
            })?;
        }
        self.snapshots
            .delete_snapshot(Paths::NS_CHANNELS, KIND_CHANNEL, id.as_str())?;
        if was_there {
            self.emit_store_event(StoreEvent::DefinitionRemoved {
                kind: KIND_CHANNEL,
                d: id.to_string(),
            });
        }
        self.archive_conversation_log(id)?;
        // Its notes and drawings live in their repositories: they leave with
        // the channel.
        let scope = bisa_core::OwnerScope::Channel { id: id.clone() };
        self.remove_notes_of(scope.clone(), &self.channel_notes_dir(id))?;
        self.remove_drawings_of(scope, &self.channel_drawings_dir(id))
    }

    fn channel_notes_dir(&self, id: &ChannelId) -> std::path::PathBuf {
        Paths::scoped_dir(&self.paths.notes_dir(), "channel", Some(id.as_str()))
    }

    fn channel_drawings_dir(&self, id: &ChannelId) -> std::path::PathBuf {
        Paths::scoped_dir(&self.paths.drawings_dir(), "channel", Some(id.as_str()))
    }

    /// Whether anything of a channel whose record is gone is still here —
    /// what a delete that stopped midway left behind.
    fn channel_remains(&self, id: &ChannelId) -> Result<bool, StoreError> {
        Ok(self.idx().channel_known(id.as_str())?
            || self.paths.conversation_log(id.as_str())?.exists()
            || self.channel_notes_dir(id).exists()
            || self.channel_drawings_dir(id).exists())
    }

    /// Move a deleted channel's log aside. A rename, so the words are
    /// untouched; a room that never spoke has no log to keep.
    fn archive_conversation_log(&self, id: &ChannelId) -> Result<(), StoreError> {
        let log = self.paths.conversation_log(id.as_str())?;
        if !log.exists() {
            return Ok(());
        }
        // A second room of the name deleted in the same second keeps both.
        let mut at = now_secs();
        let mut to = self.paths.conversation_log_archive(id.as_str(), at)?;
        while to.exists() {
            at += 1;
            to = self.paths.conversation_log_archive(id.as_str(), at)?;
        }
        std::fs::rename(&log, &to).map_err(|e| StoreError::io(log.display().to_string(), e))
    }

    /// The refusal a delete would answer, before anything is done — asked
    /// by the id alone, so it answers for a record nobody can read as for
    /// one that reads: the channel is there, and nothing points at its id.
    /// The engine asks first, so it stops no live turn for a delete the
    /// store then refuses.
    pub fn refuse_channel_delete_by_id(
        &self,
        id: &bisa_core::DeletableChannelId,
    ) -> Result<(), StoreError> {
        let id = id.as_id();
        match self.get_channel(id) {
            Ok(_) => self.refuse_if_used(crate::usage::UsageKind::Channel, id.as_str()),
            Err(StoreError::Unreadable { .. }) => {
                self.refuse_if_id_used(crate::usage::UsageKind::Channel, id.as_str(), None)
            }
            Err(e) => Err(e),
        }
    }

    /// Create `general` if this workspace does not have it, and leave it alone
    /// if it does. Called at every open; the only mechanism that restores it.
    pub fn ensure_general_channel(&self) -> Result<(), StoreError> {
        let id = ChannelId::general();
        let mut kept: Option<Channel> = None;
        match self.get_channel(&id) {
            Ok(general) if general.validate().is_ok() => return Ok(()),
            // A `general` that reads and is not `general`'s shape — listed,
            // or not the platform's own — is made again as itself, its topic
            // and tags kept: left standing, no edit of it could be stored.
            Ok(general) => kept = Some(general),
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
        if let Some(kept) = kept {
            def.topic = kept.topic;
            def.tags = kept.tags;
            def.created_at = kept.created_at;
        }
        self.write_channel(&def)
    }

    pub(crate) fn reindex_channels(&self) -> Result<(), StoreError> {
        for c in self.list_channels()? {
            // A record the cache refuses costs its row, said and named,
            // never the rebuild — and so never the open.
            self.tolerated_index("channel", c.id.as_str(), self.index_channel(&c))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::NewAgent;
    use crate::identity::MemoryKeyStore;
    use crate::problems::ProblemKind;
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
    // --- one writer, a whole delete (the channels hardening) ---

    fn listed(agents: Vec<AgentId>) -> RosterPolicy {
        RosterPolicy::Listed {
            agents,
            teams: vec![],
            humans: vec![],
        }
    }

    fn person(ws: &Workspace) -> PrincipalId {
        let p = PrincipalId::new(nostr::key::Keys::generate().public_key().to_hex()).unwrap();
        ws.add_member(
            p.clone(),
            bisa_core::MemberRole::Guest,
            crate::members::Admission::default(),
        )
        .unwrap();
        p
    }

    fn say(ws: &Workspace, scope: &ChannelId, text: &str) -> String {
        ws.post_message(
            scope.as_str(),
            bisa_core::MessageBody::post(text),
            None,
            &[],
            &[],
            None,
            crate::PostOrigin::Asked,
        )
        .unwrap()
    }

    fn removals(rx: &mut tokio::sync::broadcast::Receiver<StoreEvent>) -> Vec<(u16, String)> {
        let mut out = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            if let StoreEvent::DefinitionRemoved { kind, d } = ev {
                out.push((kind, d));
            }
        }
        out
    }

    /// A record of a channel as a later build might write it: signed and
    /// well-formed, with a field this build does not know.
    fn write_from_a_later_build(ws: &Workspace, id: &str) -> std::path::PathBuf {
        let record = serde_json::json!({
            "id": id, "name": id, "kind": "standing",
            "audience": {"audience": "workspace"},
            "roster": {"policy": "listed"},
            "origin": "local", "created_at": 1, "pinned": true,
        });
        ws.snapshots
            .put(
                Paths::NS_CHANNELS,
                KIND_CHANNEL,
                id,
                &record,
                1,
                &ws.owner,
                now_secs(),
                None,
                &[],
            )
            .unwrap();
        ws.paths()
            .state_dir(Paths::NS_CHANNELS)
            .join(format!("{KIND_CHANNEL}-{id}.json"))
    }

    /// An edit names what it changes and is made on the record as it stands,
    /// under the one writer. Read outside it and written back whole, the
    /// later write took the earlier one's agent with it.
    #[test]
    fn two_roster_edits_at_once_keep_both() {
        let (_d, ws) = ws();
        let ws = std::sync::Arc::new(ws);
        let room = ws
            .create_channel("Crew", None, RosterPolicy::default(), Tags::default())
            .unwrap()
            .id;
        let agents: Vec<AgentId> = (0..12).map(|n| agent(&ws, &format!("Hand {n}"))).collect();
        let start = std::sync::Arc::new(std::sync::Barrier::new(agents.len()));
        let writers: Vec<_> = agents
            .iter()
            .cloned()
            .map(|one| {
                let (ws, room, start) = (ws.clone(), room.clone(), start.clone());
                std::thread::spawn(move || {
                    start.wait();
                    ws.update_channel_with(&room, |c| {
                        if let RosterPolicy::Listed { agents, .. } = &mut c.roster {
                            agents.push(one);
                        }
                        Ok(())
                    })
                    .unwrap();
                })
            })
            .collect();
        for writer in writers {
            writer.join().unwrap();
        }
        let RosterPolicy::Listed {
            agents: mut on_it, ..
        } = ws.get_channel(&room).unwrap().roster
        else {
            panic!("a listed roster");
        };
        on_it.sort();
        let mut wanted = agents.clone();
        wanted.sort();
        assert_eq!(on_it, wanted, "every agent is on the roster");

        // The answer is the record before and after; an edit that refuses
        // writes nothing; a blank topic is no topic; the name, the kind and
        // the audience are not an edit's to change.
        let (before, after) = ws
            .update_channel_with(&room, |c| {
                c.topic = Some("  ".into());
                c.name = "renamed".into();
                c.kind = ChannelKind::Direct;
                Ok(())
            })
            .unwrap();
        assert_eq!(before.roster, after.roster);
        assert_eq!(after.topic, None);
        assert_eq!(after.name, "Crew");
        assert_eq!(after.kind, ChannelKind::Standing);
        let refused = ws.update_channel_with(&room, |c| {
            c.topic = Some("never stored".into());
            Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-channel-needs-name"
            )))
        });
        assert!(refused.is_err());
        assert_eq!(ws.get_channel(&room).unwrap(), after);
    }

    /// An edit and a delete of one room at once: whichever the writer takes
    /// first, the room is gone when both are done. Read before the delete
    /// and written after it, the edit made the room again — at revision 1,
    /// its notes gone.
    #[test]
    fn an_edit_that_loses_to_a_delete_does_not_bring_the_room_back() {
        let (_d, ws) = ws();
        let ws = std::sync::Arc::new(ws);
        for round in 0..24 {
            let room = ws
                .create_channel(
                    &format!("Brief {round}"),
                    None,
                    RosterPolicy::default(),
                    Tags::default(),
                )
                .unwrap();
            let start = std::sync::Arc::new(std::sync::Barrier::new(2));
            let editor = {
                let (ws, id, start) = (ws.clone(), room.id.clone(), start.clone());
                std::thread::spawn(move || {
                    start.wait();
                    // Refused when the delete came first; either way the
                    // room is gone after both.
                    drop(ws.update_channel(
                        &id,
                        Some("a topic"),
                        RosterPolicy::default(),
                        Tags::default(),
                    ));
                })
            };
            let deleter = {
                let (ws, start) = (ws.clone(), start.clone());
                let deletable = DeletableChannel::new(room.clone()).unwrap();
                std::thread::spawn(move || {
                    start.wait();
                    ws.delete_channel(deletable).unwrap();
                })
            };
            editor.join().unwrap();
            deleter.join().unwrap();
            assert!(
                matches!(
                    ws.get_channel(&room.id),
                    Err(StoreError::DefinitionNotFound { .. })
                ),
                "round {round}: the room came back"
            );
        }
        // Asked of a room that is gone, an edit is a refusal, not a create.
        let gone = ChannelId::new("brief-0").unwrap();
        assert!(matches!(
            ws.update_channel(&gone, None, RosterPolicy::default(), Tags::default()),
            Err(StoreError::DefinitionNotFound { .. })
        ));
    }

    /// The id is minted and taken under the one writer: eight rooms made
    /// under one name at once are eight rooms. Checked and then written
    /// apart, two of them took one id and the second replaced the first.
    #[test]
    fn two_creates_of_one_name_never_replace_each_other() {
        let (_d, ws) = ws();
        let ws = std::sync::Arc::new(ws);
        let start = std::sync::Arc::new(std::sync::Barrier::new(8));
        let makers: Vec<_> = (0..8)
            .map(|n| {
                let (ws, start) = (ws.clone(), start.clone());
                std::thread::spawn(move || {
                    start.wait();
                    ws.create_channel(
                        "Ops",
                        Some(&format!("made by {n}")),
                        RosterPolicy::default(),
                        Tags::default(),
                    )
                    .unwrap()
                })
            })
            .collect();
        let made: Vec<Channel> = makers.into_iter().map(|m| m.join().unwrap()).collect();
        let mut ids: Vec<String> = made.iter().map(|c| c.id.to_string()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 8, "{ids:?}");
        for room in &made {
            assert_eq!(&ws.get_channel(&room.id).unwrap(), room);
        }

        // A direct channel asked for by two at once is one channel.
        let scout = ws.get_agent(&agent(&ws, "Scout")).unwrap().pubkey;
        let start = std::sync::Arc::new(std::sync::Barrier::new(6));
        let openers: Vec<_> = (0..6)
            .map(|_| {
                let (ws, start, scout) = (ws.clone(), start.clone(), scout.clone());
                std::thread::spawn(move || {
                    start.wait();
                    ws.open_dm(std::slice::from_ref(&scout)).unwrap().id
                })
            })
            .collect();
        let mut opened: Vec<ChannelId> = openers.into_iter().map(|o| o.join().unwrap()).collect();
        opened.dedup();
        assert_eq!(opened.len(), 1, "{opened:?}");
        assert_eq!(
            ws.list_channels_of_kind(ChannelKind::Direct).unwrap().len(),
            1
        );
    }

    /// A delete forgets the room in the cache and keeps its log aside: what
    /// was said in it is in no list, count or search after, and a channel
    /// made again under the name starts empty — on this cache and on one
    /// built again from the files. The removal is said once.
    #[test]
    fn a_channel_made_again_under_a_deleted_ones_name_starts_empty() {
        let (_d, ws) = ws();
        let scout = agent(&ws, "Scout");
        let room = ws
            .create_channel("Design", None, listed(vec![scout.clone()]), Tags::default())
            .unwrap();
        ws.post_message(
            room.id.as_str(),
            bisa_core::MessageBody::post("what the old room said"),
            None,
            &[],
            &[],
            Some(&scout),
            crate::PostOrigin::Asked,
        )
        .unwrap();
        assert_eq!(ws.unread_counts().unwrap(), vec![("design".into(), 1)]);
        let log = ws.paths().conversation_log("design").unwrap();
        assert!(log.is_file());

        let mut rx = ws.subscribe_store_events();
        ws.delete_channel(DeletableChannel::new(room.clone()).unwrap())
            .unwrap();
        assert_eq!(
            removals(&mut rx),
            vec![(KIND_CHANNEL, "design".to_string())]
        );
        assert!(!log.exists(), "the log is no scope's any more");
        let kept: Vec<String> = std::fs::read_dir(ws.paths().conversation_dir())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("design.") && n.ends_with(".deleted"))
            .collect();
        assert_eq!(kept.len(), 1, "its words are kept aside: {kept:?}");
        assert!(ws.unread_counts().unwrap().is_empty());
        assert!(ws.latest_message("design").unwrap().is_none());
        assert!(ws
            .idx()
            .channels_rostering("agent", scout.as_str())
            .unwrap()
            .is_empty());
        // The agent it rostered is free to go: nothing of the room holds it.
        ws.remove_agent(&scout).unwrap();

        let again = ws
            .create_channel("Design", None, RosterPolicy::default(), Tags::default())
            .unwrap();
        assert_eq!(again.id.as_str(), "design");
        assert!(ws.messages("design", None, 10).unwrap().is_empty());
        ws.rebuild_index().unwrap();
        assert!(ws.messages("design", None, 10).unwrap().is_empty());
        say(&ws, &again.id, "the new room's first words");
        assert_eq!(ws.messages("design", None, 10).unwrap().len(), 1);

        // Deleted again in the same second, both logs are kept.
        ws.delete_channel(DeletableChannel::new(again).unwrap())
            .unwrap();
        let kept = std::fs::read_dir(ws.paths().conversation_dir())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".deleted"))
            .count();
        assert_eq!(kept, 2);
    }

    /// A delete stopped after the record went is finished by the next one,
    /// and the removal is said once. Stopped there before, the room's log
    /// stayed under its id with no door left to ask through.
    #[cfg(unix)]
    #[test]
    fn a_delete_stopped_midway_finishes_when_asked_again() {
        use std::os::unix::fs::PermissionsExt;
        let (_d, ws) = ws();
        let room = ws
            .create_channel("Ephemeral", None, RosterPolicy::default(), Tags::default())
            .unwrap();
        say(&ws, &room.id, "said before the delete");
        let logs = ws.paths().conversation_dir();
        let mut rx = ws.subscribe_store_events();

        std::fs::set_permissions(&logs, std::fs::Permissions::from_mode(0o500)).unwrap();
        let stopped = ws.delete_channel(DeletableChannel::new(room.clone()).unwrap());
        std::fs::set_permissions(&logs, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(matches!(stopped, Err(StoreError::Io { .. })), "{stopped:?}");
        assert!(ws.get_channel(&room.id).is_err(), "the record went first");
        assert!(ws.paths().conversation_log("ephemeral").unwrap().is_file());

        // The name is not free while the old room's log waits under it.
        let other = ws
            .create_channel("Ephemeral", None, RosterPolicy::default(), Tags::default())
            .unwrap();
        assert!(other.id.as_str().starts_with("ephemeral-"), "{}", other.id);

        ws.delete_channel_by_id(&room_id(&room.id)).unwrap();
        assert!(!ws.paths().conversation_log("ephemeral").unwrap().exists());
        assert_eq!(
            removals(&mut rx),
            vec![(KIND_CHANNEL, "ephemeral".to_string())],
            "said once, by the delete that took the record"
        );
        // Nothing is left: asked a third time, there is no such channel.
        assert!(matches!(
            ws.delete_channel_by_id(&room_id(&room.id)),
            Err(StoreError::DefinitionNotFound { .. })
        ));
    }

    /// What an earlier build's stopped delete left — the record gone, the
    /// cache still holding the room and its roster — is finished the same
    /// way: the agent that roster named is no longer held by a room nobody
    /// can reach.
    #[test]
    fn a_room_only_the_cache_still_holds_is_finished_by_a_delete() {
        let (_d, ws) = ws();
        let scout = agent(&ws, "Scout");
        let room = ws
            .create_channel("Stale", None, listed(vec![scout.clone()]), Tags::default())
            .unwrap();
        let record = ws
            .paths()
            .state_dir(Paths::NS_CHANNELS)
            .join(format!("{KIND_CHANNEL}-stale.json"));
        std::fs::remove_file(record).unwrap();
        assert!(ws.remove_agent(&scout).is_err(), "the cache's row holds it");
        let mut rx = ws.subscribe_store_events();
        ws.delete_channel_by_id(&room_id(&room.id)).unwrap();
        assert!(
            removals(&mut rx).is_empty(),
            "nothing is said of a record long gone"
        );
        ws.remove_agent(&scout).unwrap();
    }

    /// A post is said once it is in its log: a seen-list that will not take
    /// its line is a warning. Answered as the post's failure, the caller
    /// posted again and the words stood twice.
    #[cfg(unix)]
    #[test]
    fn a_post_stands_when_the_seen_list_cannot_be_written() {
        use std::os::unix::fs::PermissionsExt;
        let (_d, ws) = ws();
        let general = ChannelId::general();
        say(&ws, &general, "first");
        let seen = ws.paths().seen_file();
        std::fs::set_permissions(&seen, std::fs::Permissions::from_mode(0o400)).unwrap();
        let posted = ws.post_message(
            "general",
            bisa_core::MessageBody::post("second"),
            None,
            &[],
            &[],
            None,
            crate::PostOrigin::Asked,
        );
        std::fs::set_permissions(&seen, std::fs::Permissions::from_mode(0o600)).unwrap();
        posted.unwrap();
        assert_eq!(ws.messages("general", None, 10).unwrap().len(), 2);
    }

    fn room_id(id: &ChannelId) -> bisa_core::DeletableChannelId {
        bisa_core::DeletableChannelId::new(id.clone()).unwrap()
    }

    /// A channel this build cannot read is still a channel: its name is
    /// taken, it is never made again over, a list leaves it out and names
    /// it, and it is deleted by its id.
    #[test]
    fn a_channel_this_build_cannot_read_is_never_made_again_over_and_is_deleted_by_its_id() {
        let (_d, ws) = ws();
        let path = write_from_a_later_build(&ws, "ops");
        let stored = std::fs::read(&path).unwrap();
        let ops = ChannelId::new("ops").unwrap();
        assert!(matches!(
            ws.get_channel(&ops),
            Err(StoreError::Unreadable { .. })
        ));

        let beside = ws
            .create_channel("Ops", None, RosterPolicy::default(), Tags::default())
            .unwrap();
        assert!(beside.id.as_str().starts_with("ops-"), "{}", beside.id);
        let over = ws.create_channel_with_id(
            &ops,
            "Ops",
            None,
            RosterPolicy::default(),
            Tags::default(),
            ChannelOrigin::Local,
        );
        assert!(
            matches!(over, Err(StoreError::Unreadable { .. })),
            "{over:?}"
        );
        assert_eq!(
            std::fs::read(&path).unwrap(),
            stored,
            "not a byte of it changed"
        );
        assert!(ws.list_channels().unwrap().iter().all(|c| c.id != ops));

        let mut rx = ws.subscribe_store_events();
        ws.delete_channel_by_id(&room_id(&ops)).unwrap();
        assert!(!path.exists());
        assert_eq!(removals(&mut rx), vec![(KIND_CHANNEL, "ops".to_string())]);
        assert!(matches!(
            ws.get_channel(&ops),
            Err(StoreError::DefinitionNotFound { .. })
        ));
    }

    /// One channel file the disk will not give costs that room's row — named
    /// once in the problems — and never the list, nor the rebuild. It failed
    /// every list before: the Inbox, a rebuild, what a guest is told.
    #[cfg(unix)]
    #[test]
    fn a_channel_the_disk_will_not_give_costs_its_row_not_the_list() {
        use std::os::unix::fs::PermissionsExt;
        let (_d, ws) = ws();
        let kept = ws
            .create_channel("Kept", None, RosterPolicy::default(), Tags::default())
            .unwrap();
        let shut = ws
            .create_channel("Shut", None, RosterPolicy::default(), Tags::default())
            .unwrap();
        let path = ws
            .paths()
            .state_dir(Paths::NS_CHANNELS)
            .join(format!("{KIND_CHANNEL}-{}.json", shut.id));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        let listed = ws.list_channels();
        let rebuilt = ws.rebuild_index();
        let asked = ws.get_channel(&shut.id);
        // Its name is not known to be free: nothing is made under it.
        let made = ws.create_channel("Shut", None, RosterPolicy::default(), Tags::default());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(matches!(made, Err(StoreError::Io { .. })), "{made:?}");

        let ids: Vec<ChannelId> = listed.unwrap().into_iter().map(|c| c.id).collect();
        assert_eq!(ids, vec![ChannelId::general(), kept.id]);
        rebuilt.unwrap();
        assert!(
            matches!(asked, Err(StoreError::Io { .. })),
            "asked for by its id, it is the error it is: {asked:?}"
        );
        assert!(
            ws.problems()
                .iter()
                .any(|p| p.kind == ProblemKind::RebuildSkipped && p.path.contains("shut")),
            "{:?}",
            ws.problems()
        );
    }

    /// A record the cache refuses — a room other than `general` rostered
    /// everyone, as another build or a hand might write it — costs its row,
    /// said and named, never the rebuild and so never the open.
    #[test]
    fn a_channel_the_cache_refuses_never_stops_a_rebuild() {
        let (dir, ws) = ws();
        let record = serde_json::json!({
            "id": "wide", "name": "wide", "kind": "standing",
            "audience": {"audience": "workspace"},
            "roster": {"policy": "everyone"},
            "origin": "local", "created_at": 1,
        });
        ws.snapshots
            .put(
                Paths::NS_CHANNELS,
                KIND_CHANNEL,
                "wide",
                &record,
                1,
                &ws.owner,
                now_secs(),
                None,
                &[],
            )
            .unwrap();
        ws.rebuild_index().unwrap();
        assert!(
            ws.problems()
                .iter()
                .any(|p| p.kind == ProblemKind::IndexDisagrees),
            "{:?}",
            ws.problems()
        );
        drop(ws);
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    }

    /// A roster edit asks about the names it adds; the names already stored
    /// stand. Asked about again, one name of somebody gone made every later
    /// edit of the room fail — an invite's admission and a person's leave
    /// among them.
    #[test]
    fn a_roster_naming_someone_gone_still_takes_and_loses_people() {
        let (_d, ws) = ws();
        let scout = agent(&ws, "Scout");
        let (ada, grace) = (person(&ws), person(&ws));
        let room = ws
            .create_channel(
                "Design",
                None,
                RosterPolicy::Listed {
                    agents: vec![scout.clone()],
                    teams: vec![],
                    humans: vec![ada.clone()],
                },
                Tags::default(),
            )
            .unwrap();
        // Both names go stale behind the roster's back: the agent's record
        // is cut, the person's row removed without the unrostering.
        std::fs::remove_file(ws.paths().agent_file(&scout)).unwrap();
        ws.remove_member(&ada).unwrap();

        let with_grace = ws
            .set_roster_humans(&room.id, vec![ada.clone(), grace.clone()])
            .unwrap();
        assert_eq!(with_grace.roster.humans(), &[ada.clone(), grace.clone()]);
        ws.update_channel(
            &room.id,
            Some("still editable"),
            with_grace.roster.clone(),
            Tags::default(),
        )
        .unwrap();
        assert_eq!(ws.unroster_human_everywhere(&ada).unwrap(), 1);
        assert_eq!(
            ws.get_channel(&room.id).unwrap().roster.humans(),
            std::slice::from_ref(&grace)
        );
        assert_eq!(ws.unroster_human_everywhere(&ada).unwrap(), 0);

        // A name an edit adds is still asked about.
        let stranger =
            PrincipalId::new(nostr::key::Keys::generate().public_key().to_hex()).unwrap();
        assert!(ws
            .set_roster_humans(&room.id, vec![grace.clone(), stranger])
            .is_err());
        assert!(ws
            .update_channel(
                &room.id,
                None,
                listed(vec![AgentId::new("ghost").unwrap()]),
                Tags::default()
            )
            .is_err());
        let teamless = RosterPolicy::Listed {
            agents: vec![],
            teams: vec![TeamId::new("nobody").unwrap()],
            humans: vec![],
        };
        assert!(ws
            .update_channel(&room.id, None, teamless, Tags::default())
            .is_err());
    }

    /// The conversation with somebody who has since left is still the one
    /// there is: it is found before anybody is asked about.
    #[test]
    fn a_direct_channel_with_somebody_who_left_is_still_found() {
        let (_d, ws) = ws();
        let ada = person(&ws);
        let dm = ws.open_dm(std::slice::from_ref(&ada)).unwrap();
        ws.remove_member(&ada).unwrap();
        assert_eq!(ws.open_dm(std::slice::from_ref(&ada)).unwrap().id, dm.id);
        // And nobody new is opened with a stranger.
        let stranger =
            PrincipalId::new(nostr::key::Keys::generate().public_key().to_hex()).unwrap();
        assert!(ws.open_dm(&[stranger]).is_err());
    }

    /// A `general` that reads and is not `general`'s shape is made again as
    /// itself at the next open, its topic kept: left standing, no edit of it
    /// could be stored.
    #[test]
    fn a_general_of_the_wrong_shape_is_made_again_keeping_its_topic() {
        let (dir, ws) = ws();
        let record = serde_json::json!({
            "id": "general", "name": "general", "topic": "ours", "kind": "standing",
            "audience": {"audience": "workspace"},
            "roster": {"policy": "listed"},
            "origin": "local", "created_at": 7,
        });
        ws.snapshots
            .put(
                Paths::NS_CHANNELS,
                KIND_CHANNEL,
                "general",
                &record,
                99,
                &ws.owner,
                now_secs(),
                None,
                &[],
            )
            .unwrap();
        let general = ChannelId::general();
        assert!(ws
            .update_channel(
                &general,
                Some("x"),
                RosterPolicy::default(),
                Tags::default()
            )
            .is_err());
        drop(ws);
        let ws =
            Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
        let again = ws.get_channel(&general).unwrap();
        assert!(again.validate().is_ok());
        assert_eq!(again.topic.as_deref(), Some("ours"));
        assert_eq!(again.created_at, 7);
        ws.update_channel(
            &general,
            Some("x"),
            RosterPolicy::default(),
            Tags::default(),
        )
        .unwrap();
    }
}
