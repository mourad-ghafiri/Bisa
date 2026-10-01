//! Invitations: how a human on another node is admitted here, as what, and
//! on which channels.
//!
//! An invite is a record the host keeps (`invites.json`, the store's) and a
//! **code** the person carries: the host's `nprofile` (its pubkey and relay
//! hints) and a single-use secret, shown as a link (`bisa://join/…`) and as
//! text. The record keeps the secret's SHA-256 alone, so a stolen file admits
//! nobody; a code claimed once, expired, or revoked admits nobody either.
//! The wire form of the code lives with the protocol (`bisa-collab`), which
//! can spell an `nprofile`; this module is the record and its life.

use crate::id::{ChannelId, InviteId, PrincipalId};
use crate::member::MemberRole;
use serde::{Deserialize, Serialize};

/// The default life of a code, in hours, and its bounds — the
/// `collab.invite_ttl_hours` setting's.
pub const INVITE_TTL_HOURS_DEFAULT: u64 = 24;
pub const INVITE_TTL_HOURS_MIN: u64 = 1;
pub const INVITE_TTL_HOURS_MAX: u64 = 720;

/// Where an invite stands.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "state")]
pub enum InviteState {
    /// Waiting for its code to be claimed.
    Pending,
    /// A code was claimed but the owner asked to admit by hand
    /// (`collab.join = ask`): the person waits in the Inbox.
    Requested {
        by: PrincipalId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        at: u64,
    },
    /// Claimed and admitted: the person is a member.
    Accepted { by: PrincipalId, at: u64 },
    /// Withdrawn by the host before anyone claimed it.
    Revoked { at: u64 },
    /// Turned down by the host after a claim in `ask` mode.
    Refused { by: PrincipalId, at: u64 },
    /// Nobody claimed it in time.
    Expired,
}

impl InviteState {
    pub fn as_str(&self) -> &'static str {
        match self {
            InviteState::Pending => "pending",
            InviteState::Requested { .. } => "requested",
            InviteState::Accepted { .. } => "accepted",
            InviteState::Revoked { .. } => "revoked",
            InviteState::Refused { .. } => "refused",
            InviteState::Expired => "expired",
        }
    }

    /// Whether a claim may still land on it.
    pub fn is_open(&self) -> bool {
        matches!(self, InviteState::Pending)
    }
}

/// One invitation as everyone sees it: the wire shape and the row of the
/// host's list. The hash of the code's secret is the store's alone
/// (`bisa-store/invites.rs`) — this type never carries it, so no answer, event
/// or generated schema can leak it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Invite {
    pub id: InviteId,
    /// The role the person is admitted at.
    pub role: MemberRole,
    /// The standing channels a guest is put on; ignored for wider roles,
    /// which reach every channel.
    #[serde(default)]
    pub channels: Vec<ChannelId>,
    /// Who the host means to invite, for the list — never trusted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub created_at: u64,
    pub expires_at: u64,
    pub state: InviteState,
}

impl Invite {
    /// An invite may name a hosted role only: nobody is invited to be the
    /// owner.
    pub fn validate(&self) -> Result<(), InviteError> {
        if !self.role.is_hosted() {
            return Err(InviteError::OwnerRole);
        }
        if self.expires_at <= self.created_at {
            return Err(InviteError::NoLife);
        }
        Ok(())
    }

    /// Open right now: pending and not past its time.
    pub fn is_claimable(&self, now: u64) -> bool {
        self.state.is_open() && now < self.expires_at
    }
}

/// Why a claim on an invite is refused — the sentence the joiner reads.
#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, thiserror::Error,
)]
#[serde(rename_all = "snake_case", tag = "reason")]
pub enum ClaimRefusal {
    #[error("this invite code is unknown or was already used")]
    UnknownOrUsed,
    #[error("this invite has expired — ask for a new one")]
    Expired,
    #[error("this invite was withdrawn")]
    Revoked,
    #[error("the host asked to admit people by hand; you will hear back")]
    AwaitingHost,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum InviteError {
    #[error("an invite names a hosted role, never the owner")]
    OwnerRole,
    #[error("an invite expires after it is made")]
    NoLife,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invite(role: MemberRole, created_at: u64, expires_at: u64) -> Invite {
        Invite {
            id: InviteId::from_ulid(ulid::Ulid::from_datetime(std::time::SystemTime::now())),
            role,
            channels: vec![ChannelId::new("design").unwrap()],
            label: Some("Bob".into()),
            created_at,
            expires_at,
            state: InviteState::Pending,
        }
    }

    #[test]
    fn an_invite_names_a_hosted_role_and_lives_forward() {
        assert!(invite(MemberRole::Guest, 10, 20).validate().is_ok());
        assert_eq!(
            invite(MemberRole::Owner, 10, 20).validate(),
            Err(InviteError::OwnerRole)
        );
        assert_eq!(
            invite(MemberRole::Member, 10, 10).validate(),
            Err(InviteError::NoLife)
        );
    }

    #[test]
    fn a_claim_lands_only_on_a_pending_unexpired_invite() {
        let mut i = invite(MemberRole::Guest, 10, 20);
        assert!(i.is_claimable(15));
        assert!(!i.is_claimable(20), "the last second is past");
        let by = PrincipalId::new("cd".repeat(32)).unwrap();
        for state in [
            InviteState::Requested {
                by: by.clone(),
                label: None,
                at: 12,
            },
            InviteState::Accepted {
                by: by.clone(),
                at: 12,
            },
            InviteState::Revoked { at: 12 },
            InviteState::Refused { by, at: 12 },
            InviteState::Expired,
        ] {
            i.state = state;
            assert!(!i.is_claimable(15), "{}", i.state.as_str());
        }
    }

    #[test]
    fn the_wire_shape_tags_the_state_and_never_carries_a_secret_or_its_hash() {
        let i = invite(MemberRole::Guest, 10, 20);
        let json = serde_json::to_value(&i).unwrap();
        assert_eq!(json["state"], serde_json::json!({"state": "pending"}));
        assert_eq!(json["role"], "guest");
        assert_eq!(json["channels"], serde_json::json!(["design"]));
        assert!(json.get("secret").is_none());
        assert!(
            json.get("secret_hash").is_none(),
            "the hash is the store's alone"
        );
        assert_eq!(serde_json::from_value::<Invite>(json).unwrap(), i);
        assert_eq!(
            serde_json::to_value(ClaimRefusal::Expired).unwrap(),
            serde_json::json!({"reason": "expired"})
        );
        assert!(ClaimRefusal::UnknownOrUsed
            .to_string()
            .contains("already used"));
        assert_eq!(INVITE_TTL_HOURS_DEFAULT, 24);
        assert!(
            INVITE_TTL_HOURS_DEFAULT.clamp(INVITE_TTL_HOURS_MIN, INVITE_TTL_HOURS_MAX)
                == INVITE_TTL_HOURS_DEFAULT
        );
    }
}
