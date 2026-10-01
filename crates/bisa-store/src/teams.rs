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
        )?;
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
        let id = self.mint_team_id(name)?;
        self.create_team_with_id(&id, name, purpose, members, tags, Origin::Local)
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
        let stored = self.get_team(&def.id)?;
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
        let mut def = self.get_team(id)?;
        let was = def.enabled;
        def.enabled = enabled;
        let def = self.update_team(def)?;
        Ok((def, was))
    }

    /// The membership as it will be stored: normalised by the core, and every
    /// named agent must exist.
    fn vetted_members(&self, members: Vec<Assignee>) -> Result<Vec<Assignee>, StoreError> {
        let members = Team::normalise_members(members)?;
        for m in &members {
            if let Assignee::Agent(id) = m {
                let id = AgentId::new(id)?;
                // An agent that is not there is the caller's to fix; one
                // whose record cannot be read is said as that, by its file.
                match self.get_agent(&id) {
                    Ok(_) => {}
                    Err(StoreError::DefinitionNotFound { .. }) => {
                        return Err(StoreError::Invalid(bisa_core::text!(
                            "error-store-invalid-team-references-unknown-agent",
                            id = id.to_string()
                        )))
                    }
                    Err(e) => return Err(e),
                }
            }
        }
        Ok(members)
    }

    /// Delete a team, and refuse while anything still names it.
    pub fn remove_team(&self, id: &TeamId) -> Result<(), StoreError> {
        self.get_team(id)?;
        self.refuse_if_used(crate::usage::UsageKind::Team, id.as_str())?;
        let path = self.paths.team_file(id);
        std::fs::remove_file(&path).map_err(|e| StoreError::io(path.display().to_string(), e))?;
        self.snapshots
            .delete_snapshot(Paths::NS_TEAMS, KIND_TEAM, id.as_str())?;
        let idx = self.idx();
        idx.delete_team(id.as_str())?;
        idx.clear_tags(TagEntity::Team, id.as_str())
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
            if let Some(team) = crate::workspace::tolerated("team", stem, self.get_team(&id))? {
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

    /// Set (or clear, with an empty list) who carries a goal. Every named
    /// agent and team must exist.
    pub fn set_goal_assignees(
        &self,
        goal: GoalId,
        assignees: Vec<Assignee>,
    ) -> Result<Goal, StoreError> {
        for a in &assignees {
            match a {
                Assignee::Team(id) => {
                    let id = TeamId::new(id)?;
                    self.get_team(&id)?;
                }
                Assignee::Agent(id) => {
                    let id = AgentId::new(id)?;
                    self.get_agent(&id)?;
                }
                // A human is a bare pubkey: valid whether or not that person
                // has joined this workspace yet.
                Assignee::Human(_) => {}
            }
        }
        let mut current = self.get_goal(goal)?;
        current.assignees = assignees;
        self.update_goal(current)
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
            self.index_team(&def)?;
        }
        Ok(())
    }
}
