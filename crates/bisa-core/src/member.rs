//! Workspace membership: who else may write into this workspace, and as
//! what. A local policy table on the host's node, never relay state.
//!
//! **Four fixed roles, one matrix.** The owner is this node's own keypair;
//! every other member is *hosted* — a human on another node, served pairwise
//! what their role reaches and refused everything else at ingest. A role is
//! a name for a fixed set of [`Permission`]s ([`MemberRole::permissions`]),
//! never an editable list: what an Admin may do is the same on every node,
//! which is what makes the word mean something to the person reading it.
//!
//! A hosted member is a **human only**: their agents never write here, and
//! nothing they hold on their own node reaches this one. That rule is not a
//! permission — no role grants it — it is the store's admission ladder.

use crate::id::PrincipalId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemberRole {
    /// The workspace's own keypair. Exactly one; immutable; never removable.
    Owner,
    /// Runs the room for the owner: people, channels, gates.
    Admin,
    /// Every standing channel, direct messages, the agents; decides gates
    /// when governance names members.
    Member,
    /// Only the channels the owner puts them on, and direct messages.
    Guest,
}

/// What a role lets a person do here. The matrix is
/// [`MemberRole::permissions`]; a surface asks [`MemberRole::may`] and never
/// re-derives it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    /// Every standing channel and the members' directory.
    ReadWorkspace,
    /// Post, react and retract in the channels the role reaches.
    PostInChannels,
    /// Open a direct message with anyone here.
    OpenDms,
    /// Address an agent, which may answer under its own respond policy.
    MentionAgents,
    /// Decide a gate governance lets this role decide.
    DecideGates,
    /// Create, edit and delete standing channels and their rosters.
    ManageChannels,
    /// Invite, promote, demote and remove people.
    ManagePeople,
    /// Edit agents, teams and skills.
    ManageAgents,
    /// Edit who may sign which gate.
    ManageGovernance,
    /// Edit the workspace's settings.
    ManageSettings,
}

impl Permission {
    pub const ALL: [Permission; 10] = [
        Permission::ReadWorkspace,
        Permission::PostInChannels,
        Permission::OpenDms,
        Permission::MentionAgents,
        Permission::DecideGates,
        Permission::ManageChannels,
        Permission::ManagePeople,
        Permission::ManageAgents,
        Permission::ManageGovernance,
        Permission::ManageSettings,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Permission::ReadWorkspace => "read_workspace",
            Permission::PostInChannels => "post_in_channels",
            Permission::OpenDms => "open_dms",
            Permission::MentionAgents => "mention_agents",
            Permission::DecideGates => "decide_gates",
            Permission::ManageChannels => "manage_channels",
            Permission::ManagePeople => "manage_people",
            Permission::ManageAgents => "manage_agents",
            Permission::ManageGovernance => "manage_governance",
            Permission::ManageSettings => "manage_settings",
        }
    }

    /// The sentence a matrix row reads.
    pub fn words(self) -> &'static str {
        match self {
            Permission::ReadWorkspace => "read every standing channel and the members",
            Permission::PostInChannels => "post in the channels they reach",
            Permission::OpenDms => "open direct messages",
            Permission::MentionAgents => "address agents",
            Permission::DecideGates => "decide gates governance lets them",
            Permission::ManageChannels => "manage channels and rosters",
            Permission::ManagePeople => "invite, promote and remove people",
            Permission::ManageAgents => "edit agents, teams and skills",
            Permission::ManageGovernance => "edit governance",
            Permission::ManageSettings => "edit settings",
        }
    }
}

impl MemberRole {
    /// Every role, the owner first, the least reach last.
    pub const ALL: [MemberRole; 4] = [
        MemberRole::Owner,
        MemberRole::Admin,
        MemberRole::Member,
        MemberRole::Guest,
    ];
    /// The roles a person may be invited or promoted to — never the owner.
    pub const HOSTED: [MemberRole; 3] = [MemberRole::Admin, MemberRole::Member, MemberRole::Guest];

    pub fn as_str(self) -> &'static str {
        match self {
            MemberRole::Owner => "owner",
            MemberRole::Admin => "admin",
            MemberRole::Member => "member",
            MemberRole::Guest => "guest",
        }
    }

    /// The word a person reads.
    pub fn words(self) -> &'static str {
        match self {
            MemberRole::Owner => "Owner",
            MemberRole::Admin => "Admin",
            MemberRole::Member => "Member",
            MemberRole::Guest => "Guest",
        }
    }

    /// **The matrix.** Owner: everything. Admin: the room — people, channels,
    /// gates — never agents, governance or settings, which are the owner's
    /// keyboard. Member: the whole workspace to read and speak in. Guest:
    /// the channels they are put on, and direct messages.
    pub fn permissions(self) -> &'static [Permission] {
        match self {
            MemberRole::Owner => &Permission::ALL,
            MemberRole::Admin => &[
                Permission::ReadWorkspace,
                Permission::PostInChannels,
                Permission::OpenDms,
                Permission::MentionAgents,
                Permission::DecideGates,
                Permission::ManageChannels,
                Permission::ManagePeople,
            ],
            MemberRole::Member => &[
                Permission::ReadWorkspace,
                Permission::PostInChannels,
                Permission::OpenDms,
                Permission::MentionAgents,
                Permission::DecideGates,
            ],
            MemberRole::Guest => &[Permission::PostInChannels, Permission::OpenDms],
        }
    }

    pub fn may(self, permission: Permission) -> bool {
        self.permissions().contains(&permission)
    }

    /// Everybody but the owner is a human on another node.
    pub fn is_hosted(self) -> bool {
        self != MemberRole::Owner
    }

    /// Whether the role reaches every standing channel without being on its
    /// roster — the owner, admins and members; a guest reaches only rostered
    /// channels.
    pub fn reaches_every_channel(self) -> bool {
        self.may(Permission::ReadWorkspace)
    }
}

impl std::str::FromStr for MemberRole {
    type Err = crate::CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        MemberRole::ALL
            .into_iter()
            .find(|r| r.as_str() == s)
            .ok_or_else(|| crate::CoreError::InvalidId {
                what: "member role".into(),
                value: s.to_string(),
            })
    }
}

impl std::fmt::Display for MemberRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The most of a person's label a workspace keeps.
pub const MAX_LABEL_CHARS: usize = 64;

/// A person's words for themselves, kept short and printable: control
/// characters dropped, cut at [`MAX_LABEL_CHARS`], trimmed, and `None` when
/// nothing is left — the one rule a joiner's label, a profile's and the
/// owner's own all pass.
pub fn clean_label(label: Option<String>) -> Option<String> {
    label
        .map(|l| {
            l.chars()
                .filter(|c| !c.is_control())
                .take(MAX_LABEL_CHARS)
                .collect::<String>()
                .trim()
                .to_string()
        })
        .filter(|l| !l.is_empty())
}

/// One row of the members table.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceMember {
    pub pubkey: PrincipalId,
    pub role: MemberRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The person's face — a small picture this machine holds, set on their
    /// own node and carried with their profile to every workspace they are a
    /// member of (14-collaboration; ide/14 §Photos). Display only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub photo: Option<crate::attachment::AttachmentRef>,
    /// Who admitted this person — the invite's maker; none for the owner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invited_by: Option<PrincipalId>,
    /// What the joiner said it is — `desktop`, `mobile`, `cli` — for the
    /// people list; never trusted for anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
    pub added_at: u64,
}

impl WorkspaceMember {
    /// The people: every member but the owner.
    pub fn is_person(&self) -> bool {
        self.role.is_hosted()
    }
}

/// The owner's row can neither be removed nor demoted, and there is exactly
/// one: while an owner exists, nobody else may be made one. `new_role: None`
/// is a removal.
pub fn check_member_change(
    current: &[WorkspaceMember],
    target: &PrincipalId,
    new_role: Option<MemberRole>,
) -> Result<(), MemberError> {
    let is_owner = current
        .iter()
        .any(|m| &m.pubkey == target && m.role == MemberRole::Owner);
    let an_owner_exists = current.iter().any(|m| m.role == MemberRole::Owner);
    match (is_owner, new_role) {
        (true, None) => Err(MemberError::OwnerImmutable),
        (true, Some(role)) if role != MemberRole::Owner => Err(MemberError::OwnerImmutable),
        (false, Some(MemberRole::Owner)) if an_owner_exists => Err(MemberError::SecondOwner),
        _ => Ok(()),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MemberError {
    #[error("the workspace owner cannot be removed or demoted")]
    OwnerImmutable,
    #[error("a workspace has exactly one owner")]
    SecondOwner,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(pubkey: &PrincipalId, role: MemberRole) -> WorkspaceMember {
        WorkspaceMember {
            pubkey: pubkey.clone(),
            role,
            label: None,
            photo: None,
            invited_by: None,
            client: None,
            added_at: 0,
        }
    }

    #[test]
    fn the_matrix_is_fixed_and_narrows_from_owner_to_guest() {
        assert_eq!(MemberRole::Owner.permissions(), &Permission::ALL);
        for role in MemberRole::HOSTED {
            assert!(role.is_hosted());
            for p in [
                Permission::ManageAgents,
                Permission::ManageGovernance,
                Permission::ManageSettings,
            ] {
                assert!(!role.may(p), "{role} may not {}", p.as_str());
            }
        }
        assert!(!MemberRole::Owner.is_hosted());
        assert!(MemberRole::Admin.may(Permission::ManagePeople));
        assert!(MemberRole::Admin.may(Permission::ManageChannels));
        assert!(!MemberRole::Member.may(Permission::ManagePeople));
        assert!(MemberRole::Member.may(Permission::MentionAgents));
        assert!(!MemberRole::Guest.may(Permission::MentionAgents));
        assert!(!MemberRole::Guest.may(Permission::ReadWorkspace));
        assert!(MemberRole::Guest.may(Permission::PostInChannels));
        assert!(MemberRole::Guest.may(Permission::OpenDms));
        assert!(!MemberRole::Guest.may(Permission::DecideGates));
        // Reach follows the matrix: reading the workspace is reaching every channel.
        assert!(MemberRole::Member.reaches_every_channel());
        assert!(!MemberRole::Guest.reaches_every_channel());
        // Each role's set is a subset of the next wider one.
        let mut wider: &[Permission] = MemberRole::Owner.permissions();
        for role in [MemberRole::Admin, MemberRole::Member, MemberRole::Guest] {
            let set = role.permissions();
            assert!(
                set.iter().all(|p| wider.contains(p)),
                "{role} reaches beyond the role above"
            );
            wider = set;
        }
    }

    #[test]
    fn the_owner_row_is_immutable() {
        let owner = PrincipalId::new("ab".repeat(32)).unwrap();
        let other = PrincipalId::new("cd".repeat(32)).unwrap();
        let members = vec![row(&owner, MemberRole::Owner)];
        assert_eq!(
            check_member_change(&members, &owner, None),
            Err(MemberError::OwnerImmutable)
        );
        for role in MemberRole::HOSTED {
            assert_eq!(
                check_member_change(&members, &owner, Some(role)),
                Err(MemberError::OwnerImmutable)
            );
        }
        assert!(check_member_change(&members, &other, None).is_ok());
        assert!(check_member_change(&members, &other, Some(MemberRole::Guest)).is_ok());
    }

    #[test]
    fn there_is_exactly_one_owner() {
        let owner = PrincipalId::new("ab".repeat(32)).unwrap();
        let other = PrincipalId::new("cd".repeat(32)).unwrap();
        let stranger = PrincipalId::new("ef".repeat(32)).unwrap();
        let members = vec![
            row(&owner, MemberRole::Owner),
            row(&other, MemberRole::Member),
        ];
        assert_eq!(
            check_member_change(&members, &other, Some(MemberRole::Owner)),
            Err(MemberError::SecondOwner)
        );
        assert_eq!(
            check_member_change(&members, &stranger, Some(MemberRole::Owner)),
            Err(MemberError::SecondOwner)
        );
        assert!(check_member_change(&members, &owner, Some(MemberRole::Owner)).is_ok());
        assert!(check_member_change(&members, &other, Some(MemberRole::Admin)).is_ok());
        assert!(check_member_change(&members, &other, None).is_ok());
        assert!(check_member_change(&[], &owner, Some(MemberRole::Owner)).is_ok());
    }

    #[test]
    fn roles_permissions_and_rows_are_snake_case_on_the_wire() {
        assert_eq!(
            MemberRole::ALL.map(|r| r.as_str()),
            ["owner", "admin", "member", "guest"]
        );
        for role in MemberRole::ALL {
            assert_eq!(serde_json::to_value(role).unwrap(), role.as_str());
            assert_eq!(role.as_str().parse::<MemberRole>().unwrap(), role);
            assert_eq!(role.to_string(), role.as_str());
        }
        assert!(
            "collaborator".parse::<MemberRole>().is_err(),
            "no such role any more"
        );
        for p in Permission::ALL {
            assert_eq!(serde_json::to_value(p).unwrap(), p.as_str());
            assert!(!p.words().is_empty());
        }
        let m = WorkspaceMember {
            pubkey: PrincipalId::new("ab".repeat(32)).unwrap(),
            role: MemberRole::Guest,
            label: None,
            photo: None,
            invited_by: None,
            client: Some("mobile".into()),
            added_at: 7,
        };
        assert_eq!(clean_label(None), None);
        assert_eq!(clean_label(Some("   ".into())), None);
        assert_eq!(clean_label(Some("Bob\u{7}".into())), Some("Bob".into()));
        assert_eq!(
            clean_label(Some("x".repeat(200))).unwrap().chars().count(),
            MAX_LABEL_CHARS
        );
        let json = serde_json::to_value(&m).unwrap();
        assert!(json.get("label").is_none(), "no label, no field");
        assert!(json.get("invited_by").is_none());
        assert_eq!(json["client"], "mobile");
        assert_eq!(serde_json::from_value::<WorkspaceMember>(json).unwrap(), m);
        assert!(m.is_person());
        assert_eq!(
            MemberError::SecondOwner.to_string(),
            "a workspace has exactly one owner"
        );
    }
}
