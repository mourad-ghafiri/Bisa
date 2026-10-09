//! Channels: standing conversations and direct messages, one object.
//!
//! **Audience and roster answer different questions.** An audience restricts
//! who can *read*; a roster says who *belongs*. A roster is a directory, not a
//! subscription: a rostered agent still speaks only when addressed. A roster
//! also names the **humans** a channel is for: a guest of the workspace
//! reaches a standing channel only by being listed on it ([`reaches`]), while
//! the owner, admins and members reach every standing channel whether listed
//! or not — listing them is the room's word about who belongs there.
//!
//! **Membership is a policy, not a list** ([`RosterPolicy`], the Strategy
//! pattern). `Everyone` stores no list, so there is no list to empty
//! and nothing to drift from the enablement it reflects. [`members`] is the one
//! implementation; nobody re-derives it.
//!
//! **`general` cannot be deleted** — [`DeletableChannel`] has no constructor
//! for it, so `delete_channel` has no code path that could.

use crate::agent::Agent;
use crate::id::{AgentId, ChannelId, PrincipalId, TeamId};
use crate::member::MemberRole;
use crate::tags::Tags;
use crate::team::Team;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Channel {
    pub id: ChannelId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    pub kind: ChannelKind,
    #[serde(default)]
    pub audience: Audience,
    #[serde(default)]
    pub roster: RosterPolicy,
    #[serde(default, skip_serializing_if = "Tags::is_empty")]
    pub tags: Tags,
    pub origin: ChannelOrigin,
    pub created_at: u64,
}

impl Channel {
    pub fn is_general(&self) -> bool {
        self.id.is_general()
    }

    /// The invariants a channel must hold to be stored. `general` is
    /// `Standing`, workspace-visible, rostered `Everyone` and of `Core` origin;
    /// nothing else may be rostered `Everyone`; a direct message has a
    /// restricted audience.
    pub fn validate(&self) -> Result<(), ChannelError> {
        if self.is_general() {
            let ok = self.kind == ChannelKind::Standing
                && self.audience == Audience::Workspace
                && self.roster == RosterPolicy::Everyone
                && self.origin == ChannelOrigin::Core;
            if !ok {
                return Err(ChannelError::GeneralShape);
            }
            return Ok(());
        }
        if self.roster == RosterPolicy::Everyone {
            return Err(ChannelError::EveryoneReserved {
                id: self.id.clone(),
            });
        }
        if self.origin == ChannelOrigin::Core {
            return Err(ChannelError::CoreReserved {
                id: self.id.clone(),
            });
        }
        if self.kind == ChannelKind::Direct && self.audience == Audience::Workspace {
            return Err(ChannelError::DirectNeedsAudience);
        }
        Ok(())
    }

    /// The `general` channel, exactly as a fresh workspace seeds it.
    pub fn general(created_at: u64) -> Self {
        Self {
            id: ChannelId::general(),
            name: "general".into(),
            topic: Some("Everything that does not have a home yet.".into()),
            kind: ChannelKind::Standing,
            audience: Audience::Workspace,
            roster: RosterPolicy::Everyone,
            tags: Tags::default(),
            origin: ChannelOrigin::Core,
            created_at,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChannelKind {
    /// A standing conversation the workspace can see.
    Standing,
    /// A conversation with a restricted audience. A different set of
    /// participants is a different channel.
    Direct,
}

impl ChannelKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ChannelKind::Standing => "standing",
            ChannelKind::Direct => "direct",
        }
    }
}

/// Who may read. `Workspace` uses the shared workspace key; `Restricted` is
/// wrapped pairwise to exactly those principals and never touches it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "audience", content = "principals")]
pub enum Audience {
    #[default]
    Workspace,
    Restricted(Vec<PrincipalId>),
}

impl Audience {
    pub fn principals(&self) -> &[PrincipalId] {
        match self {
            Audience::Workspace => &[],
            Audience::Restricted(p) => p,
        }
    }

    pub fn is_restricted(&self) -> bool {
        matches!(self, Audience::Restricted(_))
    }
}

/// Who belongs. Evaluated at read time against enablement and membership —
/// never stored as a materialised list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "policy")]
pub enum RosterPolicy {
    /// Every enabled agent, every enabled team, every person. Stores no
    /// list. Only `general` may hold this.
    Everyone,
    /// Exactly these, intersected with enablement and membership when read.
    Listed {
        #[serde(default)]
        agents: Vec<AgentId>,
        #[serde(default)]
        teams: Vec<TeamId>,
        /// The people the channel is for — what puts a guest in the room.
        #[serde(default)]
        humans: Vec<PrincipalId>,
    },
}

impl Default for RosterPolicy {
    fn default() -> Self {
        RosterPolicy::Listed {
            agents: vec![],
            teams: vec![],
            humans: vec![],
        }
    }
}

impl RosterPolicy {
    pub fn as_str(&self) -> &'static str {
        match self {
            RosterPolicy::Everyone => "everyone",
            RosterPolicy::Listed { .. } => "listed",
        }
    }

    /// The people a `Listed` roster names; `Everyone` names them all and
    /// lists none.
    pub fn humans(&self) -> &[PrincipalId] {
        match self {
            RosterPolicy::Everyone => &[],
            RosterPolicy::Listed { humans, .. } => humans,
        }
    }

    /// Whether this roster puts `person` in the room by name.
    pub fn lists_human(&self, person: &PrincipalId) -> bool {
        self.humans().contains(person)
    }
}

/// Where a channel came from. A separate type from [`crate::Origin`] because
/// exactly one channel is the platform's own.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum ChannelOrigin {
    #[default]
    Local,
    Catalog {
        slug: String,
    },
    Core,
}

impl ChannelOrigin {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChannelOrigin::Local => "local",
            ChannelOrigin::Catalog { .. } => "catalog",
            ChannelOrigin::Core => "core",
        }
    }
}

/// One member of a channel, as derived.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Member {
    Agent(AgentId),
    Team(TeamId),
    /// A person of the workspace, by pubkey.
    Human(PrincipalId),
}

impl Member {
    pub fn kind(&self) -> &'static str {
        match self {
            Member::Agent(_) => "agent",
            Member::Team(_) => "team",
            Member::Human(_) => "human",
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Member::Agent(a) => a.as_str(),
            Member::Team(t) => t.as_str(),
            Member::Human(p) => p.as_hex(),
        }
    }
}

/// **The one implementation.** Every caller asks this; nobody re-derives it.
///
/// `Listed` is also filtered by enablement and membership, so there is one
/// rule rather than two: a disabled agent is out of every roster whether the
/// roster names it or not, and a person who left the workspace is out of
/// every roster that still names them. The core agent is included wherever
/// it is enabled — which is always — because it participates in every room.
/// `people` are the workspace's members other than the owner, who is in
/// every room the way the core agents are and is listed nowhere.
pub fn members(
    channel: &Channel,
    agents: &[Agent],
    teams: &[Team],
    people: &[PrincipalId],
) -> Vec<Member> {
    let enabled_agents = agents.iter().filter(|a| a.enabled);
    let enabled_teams = teams.iter().filter(|t| t.enabled);
    match &channel.roster {
        RosterPolicy::Everyone => enabled_agents
            .map(|a| Member::Agent(a.id.clone()))
            .chain(enabled_teams.map(|t| Member::Team(t.id.clone())))
            .chain(people.iter().map(|p| Member::Human(p.clone())))
            .collect(),
        RosterPolicy::Listed {
            agents: listed_agents,
            teams: listed_teams,
            humans: listed_humans,
        } => enabled_agents
            .filter(|a| listed_agents.contains(&a.id))
            .map(|a| Member::Agent(a.id.clone()))
            .chain(
                enabled_teams
                    .filter(|t| listed_teams.contains(&t.id))
                    .map(|t| Member::Team(t.id.clone())),
            )
            .chain(
                people
                    .iter()
                    .filter(|p| listed_humans.contains(p))
                    .map(|p| Member::Human(p.clone())),
            )
            .collect(),
    }
}

/// **The one reach rule.** Whether a person of `role` reaches `channel` —
/// reads it, and may post in it when the role may post at all. A direct
/// channel is reached by its audience alone, whatever the role. A standing
/// channel is reached by every role that reads the workspace — the owner,
/// admins, members — and by a guest only when the roster lists them.
pub fn reaches(channel: &Channel, role: MemberRole, person: &PrincipalId) -> bool {
    match channel.kind {
        ChannelKind::Direct => channel.audience.principals().contains(person),
        ChannelKind::Standing => role.reaches_every_channel() || channel.roster.lists_human(person),
    }
}

/// A channel that may be deleted. The only constructor refuses `general`, so a
/// delete function taking this has no code path that could remove it.
#[derive(Clone, Debug, PartialEq)]
pub struct DeletableChannel(Channel);

impl DeletableChannel {
    pub fn new(channel: Channel) -> Result<Self, ChannelError> {
        if channel.is_general() {
            return Err(ChannelError::Permanent { id: channel.id });
        }
        Ok(Self(channel))
    }

    pub fn channel(&self) -> &Channel {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ChannelError {
    #[error("the {id} channel cannot be deleted")]
    Permanent { id: ChannelId },
    #[error("the general channel must be standing, workspace-visible, rostered everyone and of core origin")]
    GeneralShape,
    #[error("only the general channel may be rostered everyone (refused for {id})")]
    EveryoneReserved { id: ChannelId },
    #[error("only the general channel may be of core origin (refused for {id})")]
    CoreReserved { id: ChannelId },
    #[error("a direct message needs a restricted audience")]
    DirectNeedsAudience,
}

// ---------------------------------------------------------------------------
// Membership events
// ---------------------------------------------------------------------------

/// The record that a membership change happened. Membership itself is derived
/// and stored nowhere; this is the history, which cannot be recomputed because
/// enablement is a boolean with no past.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MembershipEvent {
    pub channel: ChannelId,
    pub member: Member,
    pub change: MembershipChange,
    pub cause: MembershipCause,
    pub at: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MembershipChange {
    Joined,
    Left,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MembershipCause {
    /// The member's enablement flipped — the one change that produces a
    /// membership event. A roster edited, a member made or removed produce
    /// none: the roster is a directory and a membership is derived, so a
    /// channel's list changes without a line in its timeline.
    Enabled,
    Disabled,
}

/// The membership events an enablement flip produces, for every channel that
/// derives membership from it. The event is produced by the **transition** of
/// the flag, never by its value: `was == now` yields nothing, which is the
/// idempotency rule.
pub fn events_for_enablement(
    member: Member,
    was_enabled: bool,
    now_enabled: bool,
    channels: &[Channel],
    at: u64,
) -> Vec<MembershipEvent> {
    if was_enabled == now_enabled {
        return vec![];
    }
    let (change, cause) = if now_enabled {
        (MembershipChange::Joined, MembershipCause::Enabled)
    } else {
        (MembershipChange::Left, MembershipCause::Disabled)
    };
    channels
        .iter()
        .filter(|c| match &c.roster {
            RosterPolicy::Everyone => true,
            RosterPolicy::Listed {
                agents,
                teams,
                humans,
            } => match &member {
                Member::Agent(a) => agents.contains(a),
                Member::Team(t) => teams.contains(t),
                Member::Human(p) => humans.contains(p),
            },
        })
        .map(|c| MembershipEvent {
            channel: c.id.clone(),
            member: member.clone(),
            change,
            cause,
            at,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tests::agent;
    use crate::team::tests::team;

    fn listed(name: &str, agents: &[&str], teams: &[&str]) -> Channel {
        Channel {
            id: ChannelId::new(name).unwrap(),
            name: name.into(),
            topic: None,
            kind: ChannelKind::Standing,
            audience: Audience::Workspace,
            roster: RosterPolicy::Listed {
                agents: agents.iter().map(|a| AgentId::new(*a).unwrap()).collect(),
                teams: teams.iter().map(|t| TeamId::new(*t).unwrap()).collect(),
                humans: vec![],
            },
            tags: Tags::default(),
            origin: ChannelOrigin::Local,
            created_at: 0,
        }
    }

    fn person(byte: &str) -> PrincipalId {
        PrincipalId::new(byte.repeat(32)).unwrap()
    }

    #[test]
    fn general_cannot_be_made_deletable() {
        let g = Channel::general(0);
        assert!(g.validate().is_ok());
        assert!(matches!(
            DeletableChannel::new(g),
            Err(ChannelError::Permanent { .. })
        ));
        assert!(DeletableChannel::new(listed("eng", &[], &[])).is_ok());
    }

    #[test]
    fn only_general_may_be_everyone_or_core() {
        let mut c = listed("eng", &[], &[]);
        c.roster = RosterPolicy::Everyone;
        assert!(matches!(
            c.validate(),
            Err(ChannelError::EveryoneReserved { .. })
        ));
        let mut c = listed("eng", &[], &[]);
        c.origin = ChannelOrigin::Core;
        assert!(matches!(
            c.validate(),
            Err(ChannelError::CoreReserved { .. })
        ));
        let mut g = Channel::general(0);
        g.roster = RosterPolicy::default();
        assert_eq!(g.validate(), Err(ChannelError::GeneralShape));
        let mut d = listed("dm1", &[], &[]);
        d.kind = ChannelKind::Direct;
        assert_eq!(d.validate(), Err(ChannelError::DirectNeedsAudience));
    }

    #[test]
    fn everyone_derives_from_enablement_and_stores_nothing() {
        let agents = vec![
            agent("developer", true),
            agent("qa", false),
            agent("general-agent", true),
        ];
        let teams = vec![team("engineering", true), team("design", false)];
        let bob = person("cd");
        let got = members(
            &Channel::general(0),
            &agents,
            &teams,
            std::slice::from_ref(&bob),
        );
        assert_eq!(
            got,
            vec![
                Member::Agent(AgentId::new("developer").unwrap()),
                Member::Agent(AgentId::general()),
                Member::Team(TeamId::new("engineering").unwrap()),
                Member::Human(bob),
            ]
        );
        let json = serde_json::to_value(Channel::general(0)).unwrap();
        assert_eq!(json["roster"], serde_json::json!({"policy": "everyone"}));
    }

    #[test]
    fn listed_is_also_filtered_by_enablement() {
        let agents = vec![agent("developer", true), agent("qa", false)];
        let teams = vec![team("engineering", false)];
        let c = listed("eng", &["developer", "qa", "ghost"], &["engineering"]);
        assert_eq!(
            members(&c, &agents, &teams, &[]),
            vec![Member::Agent(AgentId::new("developer").unwrap())]
        );
    }

    #[test]
    fn a_listed_human_is_a_member_only_while_still_a_person_of_the_workspace() {
        let bob = person("cd");
        let gone = person("ef");
        let mut c = listed("eng", &[], &[]);
        c.roster = RosterPolicy::Listed {
            agents: vec![],
            teams: vec![],
            humans: vec![bob.clone(), gone.clone()],
        };
        assert_eq!(
            members(&c, &[], &[], std::slice::from_ref(&bob)),
            vec![Member::Human(bob.clone())],
            "a person who left is out of every roster that still names them"
        );
        assert!(c.roster.lists_human(&bob));
        assert!(
            !Channel::general(0).roster.lists_human(&bob),
            "everyone lists nobody"
        );
        assert_eq!(c.roster.humans().len(), 2);
        let json = serde_json::to_value(&c.roster).unwrap();
        assert_eq!(json["humans"][0], bob.as_hex());
        assert_eq!(Member::Human(bob.clone()).kind(), "human");
        assert_eq!(Member::Human(bob.clone()).id(), bob.as_hex());
    }

    #[test]
    fn reach_is_the_role_for_a_standing_channel_and_the_audience_for_a_direct_one() {
        let bob = person("cd");
        let eve = person("ef");
        let mut design = listed("design", &[], &[]);
        design.roster = RosterPolicy::Listed {
            agents: vec![],
            teams: vec![],
            humans: vec![bob.clone()],
        };
        let eng = listed("eng", &[], &[]);
        for role in [MemberRole::Owner, MemberRole::Admin, MemberRole::Member] {
            assert!(
                reaches(&eng, role, &bob),
                "{role} reaches every standing channel"
            );
            assert!(reaches(&design, role, &eve));
        }
        assert!(
            reaches(&design, MemberRole::Guest, &bob),
            "a guest reaches the channel that lists them"
        );
        assert!(!reaches(&eng, MemberRole::Guest, &bob), "and no other");
        assert!(!reaches(&design, MemberRole::Guest, &eve));
        assert!(reaches(&Channel::general(0), MemberRole::Member, &eve));
        assert!(
            !reaches(&Channel::general(0), MemberRole::Guest, &eve),
            "general is not a guest's room"
        );
        let mut dm = listed("dm-1", &[], &[]);
        dm.kind = ChannelKind::Direct;
        dm.audience = Audience::Restricted(vec![bob.clone()]);
        assert!(reaches(&dm, MemberRole::Guest, &bob));
        assert!(
            !reaches(&dm, MemberRole::Admin, &eve),
            "a direct channel is its audience's, whatever the role"
        );
    }

    #[test]
    fn enablement_events_come_from_the_transition_not_the_value() {
        let channels = vec![
            Channel::general(0),
            listed("eng", &["developer"], &[]),
            listed("ops", &[], &[]),
        ];
        let dev = Member::Agent(AgentId::new("developer").unwrap());
        assert!(events_for_enablement(dev.clone(), true, true, &channels, 1).is_empty());
        let joined = events_for_enablement(dev.clone(), false, true, &channels, 1);
        assert_eq!(joined.len(), 2, "general and eng, not ops");
        assert!(joined
            .iter()
            .all(|e| e.change == MembershipChange::Joined && e.cause == MembershipCause::Enabled));
        let left = events_for_enablement(dev, true, false, &channels, 2);
        assert_eq!(left[0].change, MembershipChange::Left);
    }

    #[test]
    fn membership_event_wire_shape() {
        let e = MembershipEvent {
            channel: ChannelId::general(),
            member: Member::Agent(AgentId::new("developer").unwrap()),
            change: MembershipChange::Joined,
            cause: MembershipCause::Enabled,
            at: 1735689600,
        };
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "channel": "general",
                "member": {"agent": "developer"},
                "change": "joined",
                "cause": "enabled",
                "at": 1735689600
            })
        );
    }

    // added by the coverage pass: channel.rs

    #[test]
    fn a_listed_team_and_a_listed_person_derive_their_membership_from_the_flip() {
        let team = TeamId::new("ops").unwrap();
        let human = person("cc");
        let mut room = listed("ops-room", &[], &["ops"]);
        if let RosterPolicy::Listed { humans, .. } = &mut room.roster {
            humans.push(human.clone());
        }
        let for_team = events_for_enablement(Member::Team(team), false, true, &[room.clone()], 1);
        assert_eq!(for_team.len(), 1);
        let for_person = events_for_enablement(Member::Human(human), true, false, &[room], 1);
        assert_eq!(for_person.len(), 1);
    }

    // added by the coverage pass: b5-channel.rs
    #[test]
    fn a_direct_channel_with_an_audience_is_well_formed_and_the_wire_words_hold() {
        let mut dm = listed("dm", &[], &[]);
        dm.kind = ChannelKind::Direct;
        dm.audience = Audience::Restricted(vec![person("aa")]);
        assert!(dm.validate().is_ok());
        assert!(dm.audience.is_restricted());
        assert_eq!(dm.audience.principals(), &[person("aa")]);
        assert!(!Audience::Workspace.is_restricted());
        assert!(Audience::Workspace.principals().is_empty());
        assert_eq!(
            (ChannelKind::Standing.as_str(), ChannelKind::Direct.as_str()),
            ("standing", "direct")
        );
        assert_eq!(
            (RosterPolicy::Everyone.as_str(), dm.roster.as_str()),
            ("everyone", "listed")
        );
        assert_eq!(
            [
                ChannelOrigin::Local,
                ChannelOrigin::Catalog { slug: "x".into() },
                ChannelOrigin::Core
            ]
            .map(|o| o.as_str()),
            ["local", "catalog", "core"]
        );
        let agent = Member::Agent(AgentId::new("dev").unwrap());
        let team = Member::Team(TeamId::new("ops").unwrap());
        assert_eq!((agent.kind(), agent.id()), ("agent", "dev"));
        assert_eq!((team.kind(), team.id()), ("team", "ops"));
        let deletable = DeletableChannel::new(listed("eng", &[], &[])).unwrap();
        assert_eq!(deletable.channel().name, "eng");
    }
}
