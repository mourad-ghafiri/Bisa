//! Invitations (14-collaboration): the codes this host made, and what became
//! of each.
//!
//! Truth is `invites.json`, mode 0600 — it holds the secrets' SHA-256 alone,
//! so a copy of the file admits nobody, and a claim is compared in constant
//! time. A code is single-use, expires, and can be withdrawn; every one of
//! those refusals is a sentence the joiner reads ([`bisa_core::ClaimRefusal`]).

use crate::error::StoreError;
use crate::workspace::{now_secs, Workspace};
use bisa_core::{
    ChannelId, ClaimRefusal, Invite, InviteId, InviteState, MemberRole, PrincipalId,
    WorkspaceMember,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default)]
struct InviteFile {
    invites: Vec<InviteRow>,
}

/// One row of `invites.json`: the public record and, beside it, the SHA-256
/// hex of the secret the code carries. The hash lives in this file and in
/// this row alone — every method hands back the [`Invite`] and never the row,
/// so no answer, event or schema can carry it.
#[derive(Serialize, Deserialize)]
struct InviteRow {
    #[serde(flatten)]
    invite: Invite,
    secret_hash: String,
}

/// What a claim leaves behind: the invite as it now stands, and whether the
/// claim was admitted at once or waits for the host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Claimed {
    pub invite: Invite,
    pub waits: bool,
}

impl Workspace {
    fn read_invites(&self) -> Result<InviteFile, StoreError> {
        let path = self.paths.invites_file();
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| StoreError::unreadable(&path, "invitations", e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(InviteFile::default()),
            Err(e) => Err(StoreError::io(path.display().to_string(), e)),
        }
    }

    fn write_invites(&self, file: &InviteFile) -> Result<(), StoreError> {
        crate::paths::write_private(
            &self.paths.invites_file(),
            &serde_json::to_vec_pretty(file)?,
        )
    }

    /// Make an invite: the record kept here and the secret minted once,
    /// handed back to be put in the code and never stored. `secret_hash` is
    /// what the record keeps. Channels a guest is put on are checked to be
    /// standing channels of this workspace.
    pub fn create_invite(
        &self,
        role: MemberRole,
        channels: Vec<ChannelId>,
        label: Option<String>,
        ttl_secs: u64,
        secret_hash: String,
    ) -> Result<Invite, StoreError> {
        for c in &channels {
            let channel = self.get_channel(c)?;
            if channel.kind != bisa_core::ChannelKind::Standing || channel.is_general() {
                return Err(StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-not-standing-channel-guest-can-be-put",
                    c = c.to_string()
                )));
            }
        }
        let now = now_secs();
        let invite = Invite {
            id: InviteId::from_ulid(crate::workspace::mint_ulid()),
            role,
            channels,
            label: label
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty()),
            created_at: now,
            expires_at: now.saturating_add(ttl_secs.max(1)),
            state: InviteState::Pending,
        };
        invite.validate().map_err(|e| {
            StoreError::Invalid(bisa_core::text!(
                "error-store-invites-refused",
                detail = e.to_string()
            ))
        })?;
        let mut file = self.read_invites()?;
        file.invites.push(InviteRow {
            invite: invite.clone(),
            secret_hash,
        });
        self.write_invites(&file)?;
        self.emit_store_event(crate::workspace::StoreEvent::InviteChanged {
            invite: invite.clone(),
        });
        Ok(invite)
    }

    /// Every invite, newest first, expired ones marked as such.
    pub fn invites(&self) -> Result<Vec<Invite>, StoreError> {
        let now = now_secs();
        let mut rows: Vec<Invite> = self
            .read_invites()?
            .invites
            .into_iter()
            .map(|row| row.invite)
            .collect();
        for i in &mut rows {
            if i.state.is_open() && now >= i.expires_at {
                i.state = InviteState::Expired;
            }
        }
        // Newest first; two made in the same second keep their file order
        // reversed, so the later one still leads — the sort is stable.
        rows.reverse();
        rows.sort_by_key(|i| std::cmp::Reverse(i.created_at));
        Ok(rows)
    }

    pub fn invite(&self, id: InviteId) -> Result<Invite, StoreError> {
        self.invites()?
            .into_iter()
            .find(|i| i.id == id)
            .ok_or_else(|| {
                StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-no-invite",
                    id = id.to_string()
                ))
            })
    }

    /// Withdraw a pending invite. A claimed or spent one stays as it is.
    pub fn revoke_invite(&self, id: InviteId) -> Result<Invite, StoreError> {
        let mut file = self.read_invites()?;
        let row = &mut file
            .invites
            .iter_mut()
            .find(|r| r.invite.id == id)
            .ok_or_else(|| {
                StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-no-invite",
                    id = id.to_string()
                ))
            })?
            .invite;
        if !row.state.is_open() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-invite-cannot-be-withdrawn",
                id = id.to_string(),
                a0 = (row.state.as_str()).to_string()
            )));
        }
        row.state = InviteState::Revoked { at: now_secs() };
        let out = row.clone();
        self.write_invites(&file)?;
        self.emit_store_event(crate::workspace::StoreEvent::InviteChanged {
            invite: out.clone(),
        });
        Ok(out)
    }

    /// Claim an invite by the hash of the secret a joiner sent: the first
    /// pending row whose hash matches in constant time. `waits` says the
    /// claim is recorded as a request rather than an acceptance — the host
    /// admits by hand. Every other outcome is a refusal in words; a spent,
    /// expired or withdrawn code answers as such, an unknown one as unknown.
    pub fn claim_invite(
        &self,
        secret_hash: &str,
        by: &PrincipalId,
        label: Option<String>,
        waits: bool,
    ) -> Result<Claimed, ClaimRefusal> {
        let now = now_secs();
        let mut file = self
            .read_invites()
            .map_err(|_| ClaimRefusal::UnknownOrUsed)?;
        let mut matched: Option<usize> = None;
        for (i, row) in file.invites.iter().enumerate() {
            if bisa_collab_hashes_equal(&row.secret_hash, secret_hash) {
                matched = Some(i);
                break;
            }
        }
        let Some(i) = matched else {
            return Err(ClaimRefusal::UnknownOrUsed);
        };
        let row = &mut file.invites[i].invite;
        match &row.state {
            InviteState::Pending if now >= row.expires_at => {
                row.state = InviteState::Expired;
                // The mark is a convenience for the list; a write that fails
                // is re-made on the next read, and the refusal stands.
                if self.write_invites(&file).is_err() {
                    return Err(ClaimRefusal::Expired);
                }
                Err(ClaimRefusal::Expired)
            }
            InviteState::Pending => {
                row.state = if waits {
                    InviteState::Requested {
                        by: by.clone(),
                        label,
                        at: now,
                    }
                } else {
                    InviteState::Accepted {
                        by: by.clone(),
                        at: now,
                    }
                };
                let invite = row.clone();
                self.write_invites(&file)
                    .map_err(|_| ClaimRefusal::UnknownOrUsed)?;
                self.emit_store_event(crate::workspace::StoreEvent::InviteChanged {
                    invite: invite.clone(),
                });
                Ok(Claimed { invite, waits })
            }
            InviteState::Revoked { .. } => Err(ClaimRefusal::Revoked),
            InviteState::Expired => Err(ClaimRefusal::Expired),
            InviteState::Requested { .. } => Err(ClaimRefusal::AwaitingHost),
            InviteState::Accepted { .. } | InviteState::Refused { .. } => {
                Err(ClaimRefusal::UnknownOrUsed)
            }
        }
    }

    /// The host's word on a requested claim: accepted, or refused.
    pub fn settle_invite(&self, id: InviteId, admit: bool) -> Result<Invite, StoreError> {
        let mut file = self.read_invites()?;
        let row = &mut file
            .invites
            .iter_mut()
            .find(|r| r.invite.id == id)
            .ok_or_else(|| {
                StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-no-invite",
                    id = id.to_string()
                ))
            })?
            .invite;
        let InviteState::Requested { by, .. } = &row.state else {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-invite-not-waiting-you",
                id = id.to_string(),
                a0 = (row.state.as_str()).to_string()
            )));
        };
        let by = by.clone();
        let at = now_secs();
        row.state = if admit {
            InviteState::Accepted { by, at }
        } else {
            InviteState::Refused { by, at }
        };
        let out = row.clone();
        self.write_invites(&file)?;
        self.emit_store_event(crate::workspace::StoreEvent::InviteChanged {
            invite: out.clone(),
        });
        Ok(out)
    }
}

impl Workspace {
    /// Admit the person a claimed invite names: a member at the invite's
    /// role, on the standing channels a guest's invite listed. The one
    /// admission for both `admit` and `ask` modes.
    pub fn admit_claimed(
        &self,
        invite: &Invite,
        by: &PrincipalId,
        label: Option<String>,
        client: Option<String>,
    ) -> Result<WorkspaceMember, StoreError> {
        self.add_member(
            by.clone(),
            invite.role,
            crate::members::Admission {
                label: label.or(invite.label.clone()),
                photo: None,
                invited_by: Some(self.owner_principal()),
                client,
            },
        )?;
        if !invite.role.reaches_every_channel() {
            for channel in &invite.channels {
                let Ok(stored) = self.get_channel(channel) else {
                    continue;
                };
                let mut humans = stored.roster.humans().to_vec();
                if !humans.contains(by) {
                    humans.push(by.clone());
                    self.set_roster_humans(channel, humans)?;
                }
            }
        }
        self.member(by)?.ok_or_else(|| {
            // LCOV_EXCL_START: `add_member` above wrote the row this reads
            StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-was-not-admitted",
                by = by.to_string()
            ))
        })
        // LCOV_EXCL_STOP
    }
}

/// Constant-time equality on two hex hash strings — the same rule the
/// protocol crate spells, kept here so the store depends on nothing above it.
fn bisa_collab_hashes_equal(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::MemoryKeyStore;
    use bisa_core::{RosterPolicy, Tags};
    use nostr::key::Keys;

    fn ws() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let ws =
            Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
        (dir, ws)
    }

    fn pk() -> PrincipalId {
        PrincipalId::new(Keys::generate().public_key().to_hex()).unwrap()
    }

    fn hash(s: &str) -> String {
        use sha2::{Digest, Sha256};
        hex::encode(Sha256::digest(s.as_bytes()))
    }

    #[test]
    fn a_code_is_claimed_once_and_only_while_it_lives() {
        let (dir, ws) = ws();
        let design = ws
            .create_channel("design", None, RosterPolicy::default(), Tags::default())
            .unwrap();
        let invite = ws
            .create_invite(
                MemberRole::Guest,
                vec![design.id.clone()],
                Some("  Bob ".into()),
                3600,
                hash("s1"),
            )
            .unwrap();
        assert_eq!(invite.label.as_deref(), Some("Bob"));
        assert_eq!(invite.state, InviteState::Pending);
        let mode = std::fs::metadata(dir.path().join("invites.json"))
            .unwrap()
            .permissions();
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(&mode) & 0o777,
            0o600,
            "the file is the owner's alone"
        );
        let bob = pk();
        assert_eq!(
            ws.claim_invite(&hash("nope"), &bob, None, false),
            Err(ClaimRefusal::UnknownOrUsed)
        );
        let claimed = ws
            .claim_invite(&hash("s1"), &bob, Some("bob".into()), false)
            .unwrap();
        assert!(!claimed.waits);
        assert!(matches!(claimed.invite.state, InviteState::Accepted { ref by, .. } if by == &bob));
        assert_eq!(
            ws.claim_invite(&hash("s1"), &pk(), None, false),
            Err(ClaimRefusal::UnknownOrUsed),
            "spent"
        );
        assert!(ws.revoke_invite(invite.id).is_err(), "no longer open");
        assert_eq!(ws.invites().unwrap().len(), 1);
    }

    #[test]
    fn expired_withdrawn_and_requested_codes_refuse_in_their_own_words() {
        let (_dir, ws) = ws();
        let bob = pk();
        let short = ws
            .create_invite(MemberRole::Member, vec![], None, 1, hash("s2"))
            .unwrap();
        // Clock the row into the past by hand: the file is the truth.
        let mut file = ws.read_invites().unwrap();
        file.invites[0].invite.expires_at = file.invites[0].invite.created_at;
        ws.write_invites(&file).unwrap();
        assert_eq!(
            ws.claim_invite(&hash("s2"), &bob, None, false),
            Err(ClaimRefusal::Expired)
        );
        assert_eq!(ws.invite(short.id).unwrap().state, InviteState::Expired);

        let withdrawn = ws
            .create_invite(MemberRole::Guest, vec![], None, 3600, hash("s3"))
            .unwrap();
        ws.revoke_invite(withdrawn.id).unwrap();
        assert_eq!(
            ws.claim_invite(&hash("s3"), &bob, None, false),
            Err(ClaimRefusal::Revoked)
        );

        let asked = ws
            .create_invite(MemberRole::Guest, vec![], None, 3600, hash("s4"))
            .unwrap();
        let claimed = ws
            .claim_invite(&hash("s4"), &bob, Some("bob".into()), true)
            .unwrap();
        assert!(claimed.waits);
        assert!(matches!(
            claimed.invite.state,
            InviteState::Requested { .. }
        ));
        assert_eq!(
            ws.claim_invite(&hash("s4"), &pk(), None, true),
            Err(ClaimRefusal::AwaitingHost)
        );
        let settled = ws.settle_invite(asked.id, false).unwrap();
        assert!(matches!(settled.state, InviteState::Refused { ref by, .. } if by == &bob));
        assert!(ws.settle_invite(asked.id, true).is_err(), "settled once");
        assert_eq!(ws.invites().unwrap()[0].id, asked.id, "newest first");
    }

    #[test]
    fn the_hash_is_in_the_file_and_never_in_the_record() {
        let (dir, ws) = ws();
        let invite = ws
            .create_invite(MemberRole::Guest, vec![], None, 3600, hash("s5"))
            .unwrap();
        let record = serde_json::to_value(&invite).unwrap();
        assert!(record.get("secret_hash").is_none(), "{record}");
        let file: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.path().join("invites.json")).unwrap())
                .unwrap();
        assert_eq!(file["invites"][0]["secret_hash"], hash("s5"));
        assert_eq!(file["invites"][0]["id"], invite.id.to_string());
        assert_eq!(ws.invites().unwrap(), vec![invite], "read back whole");
    }

    #[test]
    fn an_invite_names_a_hosted_role_and_standing_channels_only() {
        let (_dir, ws) = ws();
        assert!(ws
            .create_invite(MemberRole::Owner, vec![], None, 3600, hash("x"))
            .is_err());
        assert!(
            ws.create_invite(
                MemberRole::Guest,
                vec![ChannelId::general()],
                None,
                3600,
                hash("y")
            )
            .is_err(),
            "general is not a guest's room"
        );
        assert!(ws
            .create_invite(
                MemberRole::Guest,
                vec![ChannelId::new("nowhere").unwrap()],
                None,
                3600,
                hash("z")
            )
            .is_err());
    }

    // added by the coverage pass: invites.rs

    // --- the bare lines of the invites module ---

    /// An open code past its time lists as expired; claiming it marks the
    /// file — or not, when the file cannot be written — and refuses either
    /// way; a second claim meets the mark. A settlement names the invite it
    /// cannot find. The file nobody may read is an I/O error by its path.
    #[test]
    fn an_expired_code_is_marked_on_claim_and_refused_whether_or_not_the_mark_lands() {
        let (dir, ws) = ws();
        let bob = pk();
        ws.create_invite(MemberRole::Member, vec![], None, 1, hash("s5"))
            .unwrap();
        let mut file = ws.read_invites().unwrap();
        file.invites[0].invite.expires_at = file.invites[0].invite.created_at;
        ws.write_invites(&file).unwrap();
        assert_eq!(ws.invites().unwrap()[0].state, InviteState::Expired);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o500)).unwrap();
            let refused = ws.claim_invite(&hash("s5"), &bob, None, false);
            std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
            assert_eq!(refused, Err(ClaimRefusal::Expired));
            assert_eq!(
                ws.read_invites().unwrap().invites[0].invite.state,
                InviteState::Pending,
                "the mark did not land"
            );
            let invites = ws.paths.invites_file();
            std::fs::set_permissions(&invites, std::fs::Permissions::from_mode(0o000)).unwrap();
            let unreadable = ws.invites();
            std::fs::set_permissions(&invites, std::fs::Permissions::from_mode(0o600)).unwrap();
            assert!(matches!(unreadable, Err(StoreError::Io { .. })));
        }
        assert_eq!(
            ws.claim_invite(&hash("s5"), &bob, None, false),
            Err(ClaimRefusal::Expired)
        );
        assert_eq!(
            ws.read_invites().unwrap().invites[0].invite.state,
            InviteState::Expired
        );
        assert_eq!(
            ws.claim_invite(&hash("s5"), &bob, None, false),
            Err(ClaimRefusal::Expired),
            "the mark is met"
        );
        let err = ws
            .settle_invite(InviteId::from_ulid(crate::workspace::mint_ulid()), true)
            .unwrap_err();
        assert!(matches!(&err, StoreError::Invalid(_)), "{err:?}");
        assert!(!bisa_collab_hashes_equal("ab", "abc"));
    }

    /// Admitting a guest puts them on the invite's channels: one that is
    /// gone is skipped, one they are already on is left as it is.
    #[test]
    fn admitting_a_guest_skips_a_channel_that_is_gone_and_one_they_are_already_on() {
        let (_dir, ws) = ws();
        let bob = pk();
        let design = ws
            .create_channel("design", None, RosterPolicy::default(), Tags::default())
            .unwrap();
        let invite = Invite {
            id: InviteId::from_ulid(crate::workspace::mint_ulid()),
            role: MemberRole::Guest,
            channels: vec![design.id.clone(), ChannelId::new("gone").unwrap()],
            label: Some("Bob".into()),
            created_at: 1,
            expires_at: 2,
            state: InviteState::Accepted {
                by: bob.clone(),
                at: 1,
            },
        };
        let member = ws.admit_claimed(&invite, &bob, None, None).unwrap();
        assert_eq!(member.role, MemberRole::Guest);
        assert_eq!(
            ws.get_channel(&design.id).unwrap().roster.humans(),
            std::slice::from_ref(&bob)
        );
        ws.admit_claimed(&invite, &bob, Some("Bobby".into()), None)
            .unwrap();
        assert_eq!(
            ws.get_channel(&design.id).unwrap().roster.humans(),
            std::slice::from_ref(&bob),
            "listed once"
        );
    }
}
