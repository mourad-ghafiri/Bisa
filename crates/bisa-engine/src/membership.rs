//! Membership events on enablement transitions.
//!
//! Membership is derived and stored nowhere; its *history* is the record that
//! a change happened, and enablement is a boolean with no past — so the event
//! has to be produced at the moment the flag flips, by the transition and
//! never by the value. [`bisa_core::events_for_enablement`] decides which
//! channels are affected; this module posts them, as owner-signed
//! announcements into each channel, and is the only place that does. An edit
//! that carries the flag beside other fields is one write, announced once it
//! is made: what the store refuses has announced nothing.

use crate::{warn_on_err, EngineError, Inner};
use bisa_core::{events_for_enablement, Agent, AgentId, Member, Team, TeamId};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Flip an agent's enablement and announce what that did to every channel it
/// belongs to. Idempotent: an unchanged flag produces no event.
pub fn set_agent_enabled(
    inner: &Arc<Inner>,
    id: &AgentId,
    enabled: bool,
) -> Result<Agent, EngineError> {
    let (agent, was) = inner.ws.set_agent_enabled(id, enabled)?;
    announce(inner, Member::Agent(id.clone()), was, enabled);
    Ok(agent)
}

/// The same for a team.
pub fn set_team_enabled(
    inner: &Arc<Inner>,
    id: &TeamId,
    enabled: bool,
) -> Result<Team, EngineError> {
    let (team, was) = inner.ws.set_team_enabled(id, enabled)?;
    announce(inner, Member::Team(id.clone()), was, enabled);
    Ok(team)
}

/// Replace an agent's record, and announce what the edit did to its
/// enablement. One write: an edit the store refuses has stood nobody down.
pub fn update_agent(inner: &Arc<Inner>, agent: Agent) -> Result<Agent, EngineError> {
    let id = agent.id.clone();
    edit_agent(inner, &id, |current| {
        *current = agent;
        Ok(())
    })
}

/// The same for a team.
pub fn update_team(inner: &Arc<Inner>, team: Team) -> Result<Team, EngineError> {
    let id = team.id.clone();
    edit_team(inner, &id, |current| {
        *current = team;
        Ok(())
    })
}

/// Edit an agent's record on the record as it stands, under the roster's
/// one writer, and announce the flip — if the edit made one — from the
/// record the store answered before and after. Never from a read of its
/// own: between a read here and the write there another edit could flip
/// the flag, and the room would be told of a change that was not this one,
/// or of none.
pub(crate) fn edit_agent(
    inner: &Arc<Inner>,
    id: &AgentId,
    edit: impl FnOnce(&mut Agent) -> Result<(), bisa_store::StoreError>,
) -> Result<Agent, EngineError> {
    let (before, after) = inner.ws.update_agent_with(id, edit)?;
    announce(
        inner,
        Member::Agent(id.clone()),
        before.enabled,
        after.enabled,
    );
    Ok(after)
}

/// The same for a team.
pub(crate) fn edit_team(
    inner: &Arc<Inner>,
    id: &TeamId,
    edit: impl FnOnce(&mut Team) -> Result<(), bisa_store::StoreError>,
) -> Result<Team, EngineError> {
    let (before, after) = inner.ws.update_team_with(id, edit)?;
    announce(
        inner,
        Member::Team(id.clone()),
        before.enabled,
        after.enabled,
    );
    Ok(after)
}

fn announce(inner: &Arc<Inner>, member: Member, was: bool, now: bool) {
    if was == now {
        return;
    }
    let channels = match inner.ws.list_channels() {
        Ok(c) => c,
        // LCOV_EXCL_START: the channels unreadable; a disk arm
        Err(e) => {
            tracing::warn!("cannot list channels for a membership event: {e}");
            return;
        } // LCOV_EXCL_STOP
    };
    for event in events_for_enablement(member, was, now, &channels, now_secs()) {
        warn_on_err(
            inner.ws.post_membership(event),
            "posting a membership event",
        );
    }
}
