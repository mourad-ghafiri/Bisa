//! Teams: agents + humans working together (kind 33408).
//!
//! The domain type is [`bisa_core::Team`]; nesting is refused and the
//! general agent is dropped from a stored membership by
//! [`Team::normalise_members`]. It is a participant of every team and is stored
//! in none — a list you can edit is a list you can empty.
//!
//! Three questions a team is asked, kept apart: [`Workspace::team_agents`] is
//! the *work-routing pool* (excludes the general agent, which delegates rather
//! than does); [`Workspace::team_participants`] is *who belongs here*
//! (includes it), whatever the team's switch says;
//! [`Workspace::team_addressed`] is *whom a message to it reaches* — the
//! participants, and nobody while the team is stood down.
//!
//! Truth: `teams/<id>.json`; snapshot events in `teams/state/33408-<id>.json`;
//! the `teams` index table and its tag rows (both rebuildable).

use crate::error::StoreError;
use crate::paths::Paths;
use crate::workspace::{mint_ulid, now_secs, EventAudience, StoreEvent, Workspace};
use bisa_core::kind::KIND_TEAM;
use bisa_core::tags::TagEntity;
use bisa_core::{AgentId, Assignee, Goal, GoalId, Origin, PrincipalId, Tags, Team, TeamId};

pub(crate) fn origin_str(o: &Origin) -> &'static str {
    match o {
        Origin::Local => "local",
        Origin::Catalog { .. } => "catalog",
    }
}

/// A team's stored members and the agents that are in every room: a list a
/// person can edit is a list a person can empty, so the core agents are
/// added where a team is read and written nowhere.
fn with_the_core(mut members: Vec<Assignee>) -> Vec<Assignee> {
    for core in AgentId::CORE {
        members.push(Assignee::Agent(core.to_string()));
    }
    members
}

/// What [`Workspace::check_assignee`] found behind a name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssigneeStanding {
    /// The record is there (or the assignee is a person, who needs none).
    There,
    NoSuchAgent(AgentId),
    NoSuchTeam(TeamId),
}

impl Workspace {
    fn write_team(&self, def: &Team) -> Result<(), StoreError> {
        def.validate()?;
        let path = self.paths.team_file(&def.id);
        crate::paths::write_atomic(&path, &serde_json::to_vec_pretty(def)?)?;
        let existing_rev =
            self.snapshots
                .current_revision(Paths::NS_TEAMS, KIND_TEAM, def.id.as_str())?;
        let event = self.snapshots.put(
            Paths::NS_TEAMS,
            KIND_TEAM,
            def.id.as_str(),
            def,
            existing_rev + 1,
            &self.owner,
            now_secs(),
            None,
            def.tags.as_slice(),
        )?; // LCOV_EXCL_LINE: the store's disk arm: a snapshot write that fails below the lock
        self.emit_store_event(StoreEvent::ConversationSnapshot {
            kind: KIND_TEAM,
            d: def.id.to_string(),
            event,
            audience: EventAudience::Workspace,
        });
        self.index_team(def)
    }

    pub(crate) fn index_team(&self, def: &Team) -> Result<(), StoreError> {
        let idx = self.idx();
        idx.upsert_team(
            def.id.as_str(),
            &def.name,
            def.enabled,
            origin_str(&def.origin),
            def.created_at,
        )?;
        idx.set_tags(TagEntity::Team, def.id.as_str(), def.tags.as_slice())
    }

    fn mint_team_id(&self, name: &str) -> Result<TeamId, StoreError> {
        let base = crate::agents::slugify(name).unwrap_or_else(|| "team".to_string());
        let candidate = TeamId::new(&base)?;
        if !self.paths.team_file(&candidate).exists() {
            return Ok(candidate);
        }
        let suffix = mint_ulid().to_string().to_ascii_lowercase();
        TeamId::new(format!("{base}-{}", &suffix[suffix.len() - 6..])).map_err(Into::into)
    }

    pub fn create_team(
        &self,
        name: &str,
        purpose: Option<&str>,
        members: Vec<Assignee>,
        tags: Tags,
    ) -> Result<Team, StoreError> {
        // Minted and taken under the one writer, as an agent's id is.
        let _one_writer = self.roster_writer();
        let id = self.mint_team_id(name)?;
        self.create_team_locked(&id, name, purpose, members, tags, Origin::Local)
    }

    pub(crate) fn create_team_with_id(
        &self,
        id: &TeamId,
        name: &str,
        purpose: Option<&str>,
        members: Vec<Assignee>,
        tags: Tags,
        origin: Origin,
    ) -> Result<Team, StoreError> {
        let _one_writer = self.roster_writer();
        self.create_team_locked(id, name, purpose, members, tags, origin)
    }

    /// [`Self::create_team_with_id`] under the writer its caller holds.
    fn create_team_locked(
        &self,
        id: &TeamId,
        name: &str,
        purpose: Option<&str>,
        members: Vec<Assignee>,
        tags: Tags,
        origin: Origin,
    ) -> Result<Team, StoreError> {
        if self.paths.team_file(id).exists() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-team-already-exists",
                id = id.to_string()
            )));
        }
        let def = Team {
            id: id.clone(),
            name: name.to_string(),
            purpose: purpose.map(str::to_string),
            photo: None,
            members: self.vetted_members(members)?,
            enabled: true,
            tags,
            origin,
            created_at: now_secs(),
        };
        self.write_team(&def)?;
        Ok(def)
    }

    pub fn update_team(&self, def: Team) -> Result<Team, StoreError> {
        let _one_writer = self.roster_writer();
        let stored = self.get_team(&def.id)?;
        self.update_team_locked(stored, def)
    }

    /// Edit a team's record under the one writer, as
    /// [`Workspace::update_agent_with`] edits an agent's: `edit` sees the
    /// team as it stands, and the record before and after are answered —
    /// two edits at once, one of the roster and one of the name, both stand.
    pub fn update_team_with(
        &self,
        id: &TeamId,
        edit: impl FnOnce(&mut Team) -> Result<(), StoreError>,
    ) -> Result<(Team, Team), StoreError> {
        let _one_writer = self.roster_writer();
        let before = self.get_team(id)?;
        let mut next = before.clone();
        edit(&mut next)?;
        let after = self.update_team_locked(before.clone(), next)?;
        Ok((before, after))
    }

    /// The write behind [`Self::update_team`] and [`Self::update_team_with`],
    /// under the writer the caller holds.
    fn update_team_locked(&self, stored: Team, def: Team) -> Result<Team, StoreError> {
        let def = Team {
            members: self.vetted_members(def.members)?,
            origin: stored.origin,
            created_at: stored.created_at,
            ..def
        };
        self.write_team(&def)?;
        Ok(def)
    }

    /// Flip enablement. Returns the team and whether it was enabled
    /// before, so the caller can derive membership events.
    pub fn set_team_enabled(&self, id: &TeamId, enabled: bool) -> Result<(Team, bool), StoreError> {
        let (before, after) = self.update_team_with(id, |team| {
            team.enabled = enabled;
            Ok(())
        })?;
        Ok((after, before.enabled))
    }

    /// The membership as it will be stored: normalised by the core, and every
    /// named agent must exist.
    fn vetted_members(&self, members: Vec<Assignee>) -> Result<Vec<Assignee>, StoreError> {
        let members = Team::normalise_members(members)?;
        for m in &members {
            // An agent that is not there is the caller's to fix; one whose
            // record cannot be read is said as that, by its file.
            if let AssigneeStanding::NoSuchAgent(id) = self.check_assignee(m)? {
                return Err(StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-team-references-unknown-agent",
                    id = id.to_string()
                )));
            }
        }
        Ok(members)
    }

    /// Delete a team, and refuse while anything still names it.
    pub fn remove_team(&self, id: &TeamId) -> Result<(), StoreError> {
        let _one_writer = self.roster_writer();
        match self.get_team(id) {
            Ok(_) => self.refuse_if_used(crate::usage::UsageKind::Team, id.as_str())?,
            // A record nobody can read is removed by its id, once nothing
            // points at the id.
            Err(StoreError::Unreadable { .. }) => {
                self.refuse_if_id_used(crate::usage::UsageKind::Team, id.as_str(), None)?
            }
            Err(e) => return Err(e),
        }
        // What others read first, the record last: stopped before it, the
        // team is whole on disk and the removal is asked again.
        {
            let idx = self.idx();
            idx.delete_team(id.as_str())?;
            idx.clear_tags(TagEntity::Team, id.as_str())?;
        }
        self.snapshots
            .delete_snapshot(Paths::NS_TEAMS, KIND_TEAM, id.as_str())?;
        let path = self.paths.team_file(id);
        std::fs::remove_file(&path).map_err(|e| StoreError::io(path.display().to_string(), e))?;
        self.emit_store_event(StoreEvent::DefinitionRemoved {
            kind: KIND_TEAM,
            d: id.to_string(),
        });
        Ok(())
    }

    pub fn get_team(&self, id: &TeamId) -> Result<Team, StoreError> {
        let path = self.paths.team_file(id);
        let bytes = std::fs::read(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StoreError::DefinitionNotFound {
                    kind: "team",
                    id: id.to_string(),
                }
            } else {
                StoreError::io(path.display().to_string(), e)
            }
        })?;
        serde_json::from_slice(&bytes).map_err(|e| StoreError::unreadable(&path, "team", e))
    }

    pub fn list_teams(&self) -> Result<Vec<Team>, StoreError> {
        let dir = self.paths.teams_dir();
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
            let Ok(id) = TeamId::new(stem) else {
                tracing::warn!("teams/{name}: not a team id, skipping");
                continue;
            };
            if let Some(team) = self.tolerated_definition("team", stem, self.get_team(&id))? {
                out.push(team);
            }
        }
        out.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));
        Ok(out)
    }

    /// The team's human principals (gate/question audience).
    pub fn team_humans(&self, id: &TeamId) -> Result<Vec<PrincipalId>, StoreError> {
        Ok(self.get_team(id)?.humans().cloned().collect())
    }

    /// The team's agent ids: the **work routing pool**. The general agent is
    /// deliberately absent — it delegates work rather than doing it.
    pub fn team_agents(&self, id: &TeamId) -> Result<Vec<AgentId>, StoreError> {
        self.get_team(id)?
            .agent_ids()
            .map(|a| AgentId::new(a).map_err(Into::into))
            .collect()
    }

    /// Everyone who belongs to this team: its stored members plus the core
    /// agents, for addressing and display.
    pub fn team_participants(&self, id: &TeamId) -> Result<Vec<Assignee>, StoreError> {
        Ok(with_the_core(self.get_team(id)?.members))
    }

    /// Who a message to this team reaches: its participants — and nobody
    /// while the team is stood down, which is out of every room until it is
    /// stood up again.
    pub fn team_addressed(&self, id: &TeamId) -> Result<Vec<Assignee>, StoreError> {
        let team = self.get_team(id)?;
        if !team.enabled {
            return Ok(Vec::new());
        }
        Ok(with_the_core(team.members))
    }

    /// Whether what an assignee names is there — **the one existence check**
    /// behind every door that takes an assignee: a team's members, a
    /// channel's roster, a goal's, a project's and a work item's assignees.
    ///
    /// The answer is typed so each door keeps its own sentence for a name
    /// that is not there. A record that is there and cannot be read is an
    /// error, by its file, and never an answer: no door may call it unknown.
    pub fn check_assignee(&self, who: &Assignee) -> Result<AssigneeStanding, StoreError> {
        let missing = |read: Result<(), StoreError>, standing| match read {
            Ok(()) => Ok(AssigneeStanding::There),
            Err(StoreError::DefinitionNotFound { .. }) => Ok(standing),
            Err(e) => Err(e),
        };
        match who {
            Assignee::Agent(id) => {
                let id = AgentId::new(id)?;
                let read = self.get_agent(&id).map(drop);
                missing(read, AssigneeStanding::NoSuchAgent(id))
            }
            Assignee::Team(id) => {
                let id = TeamId::new(id)?;
                let read = self.get_team(&id).map(drop);
                missing(read, AssigneeStanding::NoSuchTeam(id))
            }
            // A human is a bare pubkey: valid whether or not that person has
            // joined this workspace yet.
            Assignee::Human(_) => Ok(AssigneeStanding::There),
        }
    }

    /// The check for the doors that answer a missing name as *no such
    /// definition* — a goal's and a project's assignees.
    pub(crate) fn require_assignees(&self, assignees: &[Assignee]) -> Result<(), StoreError> {
        for who in assignees {
            let (kind, id) = match self.check_assignee(who)? {
                AssigneeStanding::There => continue,
                AssigneeStanding::NoSuchAgent(id) => ("agent", id.to_string()),
                AssigneeStanding::NoSuchTeam(id) => ("team", id.to_string()),
            };
            return Err(StoreError::DefinitionNotFound { kind, id });
        }
        Ok(())
    }

    /// Set (or clear, with an empty list) who carries a goal. Every named
    /// agent and team must exist.
    pub fn set_goal_assignees(
        &self,
        goal: GoalId,
        assignees: Vec<Assignee>,
    ) -> Result<Goal, StoreError> {
        self.require_assignees(&assignees)?;
        self.update_goal_with(goal, |current| {
            current.assignees = assignees;
            Ok(())
        })
    }

    /// The goals directly naming this assignee. Inheritance through `parent`
    /// is resolved above the index.
    pub fn goals_for_assignee(&self, assignee: &Assignee) -> Result<Vec<Goal>, StoreError> {
        let ids = self.idx().goals_for_assignee(&assignee.to_string())?;
        ids.into_iter()
            .filter_map(|id| id.parse().ok())
            .map(|id| self.get_goal(id))
            .collect()
    }

    pub(crate) fn reindex_teams(&self) -> Result<(), StoreError> {
        for def in self.list_teams()? {
            self.tolerated_index("team", def.id.as_str(), self.index_team(&def))?;
        }
        Ok(())
    }
}

// added by the coverage pass: teams_mod.rs
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

    /// Twelve members added to one team at once are all on it: each edit is
    /// made on the record as it stands, under the roster's one writer. Read
    /// outside it, changed and written back whole, the later write took the
    /// earlier one's member with it. The answer names the record before and
    /// after, and an edit that refuses writes nothing.
    #[test]
    fn two_edits_of_one_team_both_stand() {
        let (_d, ws) = ws();
        let ws = std::sync::Arc::new(ws);
        let team = ws
            .create_team("Crew", None, vec![], Tags::default())
            .unwrap()
            .id;
        let agents: Vec<AgentId> = (0..12)
            .map(|n| {
                ws.add_agent(crate::agents::NewAgent {
                    name: format!("Hand {n}"),
                    harness: "mock".into(),
                    system_prompt: "lend a hand".into(),
                    ..Default::default()
                })
                .unwrap()
                .id
            })
            .collect();
        let start = std::sync::Arc::new(std::sync::Barrier::new(agents.len()));
        let writers: Vec<_> = agents
            .iter()
            .cloned()
            .map(|agent| {
                let (ws, team, start) = (ws.clone(), team.clone(), start.clone());
                std::thread::spawn(move || {
                    start.wait();
                    ws.update_team_with(&team, |t| {
                        t.members.push(Assignee::Agent(agent.to_string()));
                        Ok(())
                    })
                    .unwrap();
                })
            })
            .collect();
        for writer in writers {
            writer.join().unwrap();
        }
        let mut on_it = ws.team_agents(&team).unwrap();
        on_it.sort();
        let mut wanted = agents.clone();
        wanted.sort();
        assert_eq!(on_it, wanted, "every member is on the record");

        let (before, after) = ws
            .update_team_with(&team, |t| {
                t.name = "Deck crew".into();
                Ok(())
            })
            .unwrap();
        assert_eq!(
            (before.name.as_str(), after.name.as_str()),
            ("Crew", "Deck crew")
        );
        assert_eq!(after.members.len(), 12, "a rename keeps the roster");
        let refused = ws.update_team_with(&team, |t| {
            t.name = "Lost".into();
            Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-team-references-unknown-agent",
                id = "nobody".to_string()
            )))
        });
        assert!(refused.is_err());
        assert_eq!(ws.get_team(&team).unwrap().name, "Deck crew");
    }

    /// A name taken twice gets a suffix; an id taken is refused; the folder
    /// skips what is not a team's file and names what it cannot read.
    #[test]
    fn team_ids_are_minted_apart_and_the_folder_skips_strangers() {
        let (_d, ws) = ws();
        let ops = ws
            .create_team("Ops", None, vec![], Tags::default())
            .unwrap();
        assert_eq!(ops.id.as_str(), "ops");
        let twin = ws
            .create_team("Ops", None, vec![], Tags::default())
            .unwrap();
        assert!(twin.id.as_str().starts_with("ops-"), "{}", twin.id);
        let err = ws
            .create_team_with_id(&ops.id, "Ops", None, vec![], Tags::default(), Origin::Local)
            .unwrap_err();
        assert!(matches!(&err, StoreError::Invalid(_)), "{err:?}");
        let teams = ws.paths.teams_dir();
        std::fs::write(teams.join("Bad Name.json"), b"{}").unwrap();
        std::fs::write(teams.join("README"), b"").unwrap();
        assert_eq!(ws.list_teams().unwrap().len(), 2);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let file = ws.paths.team_file(&ops.id);
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
            let unreadable = ws.get_team(&ops.id);
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(matches!(unreadable, Err(StoreError::Io { .. })));
            std::fs::set_permissions(&teams, std::fs::Permissions::from_mode(0o000)).unwrap();
            let unreadable = ws.list_teams();
            std::fs::set_permissions(&teams, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert!(matches!(unreadable, Err(StoreError::Io { .. })));
        }
    }
}
