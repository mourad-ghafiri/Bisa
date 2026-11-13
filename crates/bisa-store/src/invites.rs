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
pub(crate) struct InviteFile {
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

/// What a claim came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClaimOutcome {
    /// The code admitted its claimer, at its role and on its channels.
    Admitted {
        invite: Invite,
        member: WorkspaceMember,
    },
    /// The claim is recorded and waits on the host's word — made now, or
    /// made before by this same claimer and waiting still.
    Waiting { invite: Invite },
    /// The claimer is a member already — the owner among them: nothing was
    /// claimed, and the code stands as it was for whoever it was made for.
    AlreadyMember { member: WorkspaceMember },
}

/// Why a claim came to nothing.
#[derive(Debug)]
pub enum ClaimFailure {
    /// The code's own answer — the sentence the joiner reads.
    Refused(ClaimRefusal),
    /// The code was good and the host could not write the admission. The
    /// code is claimable again; the error is the host's to read, never the
    /// joiner's — it names files on the host's disk.
    NotAdmitted(StoreError),
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

    /// Called at open: an invitations file that is there and does not parse
    /// is moved under `quarantine/` and named, and the list starts again
    /// empty. Left where it was it failed every read and every write for
    /// good — no invitation could be made, and every claim was refused. The
    /// codes it held are spent with it; the moved file keeps who was waiting.
    pub(crate) fn repair_invites_at_open(&self) -> Result<(), StoreError> {
        if let Err(StoreError::Unreadable { path, what, reason }) = self.read_invites() {
            let to = self
                .paths
                .quarantine(std::path::Path::new(&path), now_secs())?;
            self.record_problem(crate::problems::WorkspaceProblem::new(
                crate::problems::ProblemKind::Quarantined,
                path.clone(),
                bisa_core::text!(
                    "error-store-problem-quarantined",
                    what = what.to_string(),
                    path = path,
                    reason = reason,
                    to = to.display().to_string()
                ),
                Some(&to),
            ));
        }
        Ok(())
    }

    fn no_invite(id: InviteId) -> StoreError {
        StoreError::Invalid(bisa_core::text!(
            "error-store-invalid-no-invite",
            id = id.to_string()
        ))
    }

    fn say_invite(&self, invite: &Invite) {
        self.emit_store_event(crate::workspace::StoreEvent::InviteChanged {
            invite: invite.clone(),
        });
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
            // The one rule every label passes: this one becomes the member's
            // name when the joiner gives none, and is told to everybody.
            label: bisa_core::clean_label(label),
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
        let _one_writer = self.people_writer();
        let mut file = self.read_invites()?;
        file.invites.push(InviteRow {
            invite: invite.clone(),
            secret_hash,
        });
        self.write_invites(&file)?;
        self.say_invite(&invite);
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
            .ok_or_else(|| Self::no_invite(id))
    }

    /// Withdraw a pending invite. A claimed or spent one stays as it is.
    pub fn revoke_invite(&self, id: InviteId) -> Result<Invite, StoreError> {
        let _one_writer = self.people_writer();
        let mut file = self.read_invites()?;
        let row = &mut file
            .invites
            .iter_mut()
            .find(|r| r.invite.id == id)
            .ok_or_else(|| Self::no_invite(id))?
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
        self.say_invite(&out);
        Ok(out)
    }

    /// **The one admission by a code.** A joiner sent the hash of a secret:
    /// the first row whose hash matches in constant time is the code they
    /// hold. `waits` says the host admits by hand — the claim is recorded as
    /// a request and answered *waiting*; otherwise the claimer is a member
    /// at the code's role, on the channels a guest's code named.
    ///
    /// Under the people's one writer, whole:
    ///
    /// - **Somebody already here claims nothing.** A member's claim — the
    ///   owner's own key among them — is answered by what they are, and the
    ///   code stands as it was. Taken, it restated their role to the code's:
    ///   an admin who pasted a guest's link was a guest.
    /// - **The code is spent first, then the person admitted** — and put
    ///   back to *pending* when the member could not be written, so the
    ///   joiner may try again. A stop between the two fails closed: a used
    ///   code and nobody admitted, never a member beside a live code.
    /// - **Its claimer hears *waiting* again** on a code that waits for the
    ///   host; anybody else's claim of it is refused.
    ///
    /// A spent, expired or withdrawn code answers as such, an unknown one as
    /// unknown. What is said on the bus is said after the last write.
    pub fn claim_and_admit(
        &self,
        secret_hash: &str,
        by: &PrincipalId,
        label: Option<String>,
        client: Option<String>,
        waits: bool,
    ) -> Result<ClaimOutcome, ClaimFailure> {
        use ClaimFailure::{NotAdmitted, Refused};
        let _one_writer = self.people_writer();
        if let Some(member) = self.member(by).map_err(NotAdmitted)? {
            return Ok(ClaimOutcome::AlreadyMember { member });
        }
        let now = now_secs();
        let mut file = self.read_invites().map_err(NotAdmitted)?;
        let mut matched: Option<usize> = None;
        for (i, row) in file.invites.iter().enumerate() {
            if bisa_collab_hashes_equal(&row.secret_hash, secret_hash) {
                matched = Some(i);
                break;
            }
        }
        let Some(i) = matched else {
            return Err(Refused(ClaimRefusal::UnknownOrUsed));
        };
        match file.invites[i].invite.state.clone() {
            InviteState::Pending if now >= file.invites[i].invite.expires_at => {
                file.invites[i].invite.state = InviteState::Expired;
                // The mark is a convenience for the list; a write that fails
                // is re-made on the next read, and the refusal stands.
                if self.write_invites(&file).is_ok() {
                    self.say_invite(&file.invites[i].invite);
                }
                Err(Refused(ClaimRefusal::Expired))
            }
            InviteState::Pending if waits => {
                file.invites[i].invite.state = InviteState::Requested {
                    by: by.clone(),
                    label,
                    at: now,
                };
                self.write_invites(&file).map_err(NotAdmitted)?;
                let invite = file.invites[i].invite.clone();
                self.say_invite(&invite);
                Ok(ClaimOutcome::Waiting { invite })
            }
            InviteState::Pending => {
                file.invites[i].invite.state = InviteState::Accepted {
                    by: by.clone(),
                    at: now,
                };
                self.write_invites(&file).map_err(NotAdmitted)?;
                let invite = file.invites[i].invite.clone();
                match self.admit_locked(&invite, by, label, client) {
                    Ok((member, said)) => {
                        self.say_invite(&invite);
                        if let Some(change) = said {
                            self.emit_store_event(crate::workspace::StoreEvent::PeopleChanged {
                                pubkey: by.clone(),
                                change,
                            });
                        }
                        Ok(ClaimOutcome::Admitted { invite, member })
                    }
                    Err(e) => {
                        self.unspend(&mut file, i, InviteState::Pending, by);
                        Err(NotAdmitted(e))
                    }
                }
            }
            InviteState::Requested { by: asker, .. } if asker == *by => Ok(ClaimOutcome::Waiting {
                invite: file.invites[i].invite.clone(),
            }),
            InviteState::Requested { .. } => Err(Refused(ClaimRefusal::AwaitingHost)),
            InviteState::Revoked { .. } => Err(Refused(ClaimRefusal::Revoked)),
            InviteState::Expired => Err(Refused(ClaimRefusal::Expired)),
            InviteState::Accepted { .. } | InviteState::Refused { .. } => {
                Err(Refused(ClaimRefusal::UnknownOrUsed))
            }
        }
    }

    /// An admission failed after its code was marked spent: the code goes
    /// back to where it stood, unless the member's row did land — the
    /// mirror's write failed after the file's — in which case the person is
    /// here, the code stays spent, and their next claim is answered as a
    /// member's.
    fn unspend(&self, file: &mut InviteFile, i: usize, back_to: InviteState, by: &PrincipalId) {
        let landed = self.read_member_file_has(by).unwrap_or(false);
        if landed {
            return; // LCOV_EXCL_LINE: the mirror refusing a row the file took; a disk arm
        }
        file.invites[i].invite.state = back_to;
        if let Err(e) = self.write_invites(file) {
            // LCOV_EXCL_START: the invitations unwritable a moment after they were written; a disk arm
            tracing::warn!(
                "an invitation whose admission failed could not be put back: it stays spent ({e})"
            );
            // LCOV_EXCL_STOP
        }
    }

    /// The host's word on a claim that waited: the person is admitted at the
    /// invite's role and on its channels, and the request reads *accepted*.
    /// Spent first and put back to *requested* when the member could not be
    /// written, as a claim's is — so the owner may answer again. Somebody
    /// admitted by another door meanwhile keeps their role: the owner who
    /// made them an admin by key does not make them a guest by answering an
    /// older request.
    pub fn admit_request(&self, id: InviteId) -> Result<(Invite, WorkspaceMember), StoreError> {
        let _one_writer = self.people_writer();
        let mut file = self.read_invites()?;
        let i = file
            .invites
            .iter()
            .position(|r| r.invite.id == id)
            .ok_or_else(|| Self::no_invite(id))?;
        let waiting = file.invites[i].invite.state.clone();
        let InviteState::Requested { by, label, .. } = &waiting else {
            return Err(Self::not_waiting(id, &waiting));
        };
        let here = self.member(by)?;
        file.invites[i].invite.state = InviteState::Accepted {
            by: by.clone(),
            at: now_secs(),
        };
        self.write_invites(&file)?;
        let invite = file.invites[i].invite.clone();
        // Somebody admitted by another door while their request waited is
        // here already: the request is settled and they stay what they are.
        if let Some(member) = here {
            self.say_invite(&invite);
            return Ok((invite, member));
        }
        match self.admit_locked(&invite, by, label.clone(), None) {
            Ok((member, said)) => {
                self.say_invite(&invite);
                if let Some(change) = said {
                    self.emit_store_event(crate::workspace::StoreEvent::PeopleChanged {
                        pubkey: by.clone(),
                        change,
                    });
                }
                Ok((invite, member))
            }
            Err(e) => {
                self.unspend(&mut file, i, waiting.clone(), by);
                Err(e)
            }
        }
    }

    /// The host's word on a claim that waited: no. The request reads
    /// *refused*, and its claimer is told.
    pub fn refuse_request(&self, id: InviteId) -> Result<Invite, StoreError> {
        let _one_writer = self.people_writer();
        let mut file = self.read_invites()?;
        let row = &mut file
            .invites
            .iter_mut()
            .find(|r| r.invite.id == id)
            .ok_or_else(|| Self::no_invite(id))?
            .invite;
        let InviteState::Requested { by, .. } = &row.state else {
            return Err(Self::not_waiting(id, &row.state));
        };
        row.state = InviteState::Refused {
            by: by.clone(),
            at: now_secs(),
        };
        let out = row.clone();
        self.write_invites(&file)?;
        self.say_invite(&out);
        Ok(out)
    }

    fn not_waiting(id: InviteId, state: &InviteState) -> StoreError {
        StoreError::Invalid(bisa_core::text!(
            "error-store-invalid-invite-not-waiting-you",
            id = id.to_string(),
            a0 = (state.as_str()).to_string()
        ))
    }

    /// The admission both doors share, under the people's writer: the
    /// member's row at the invite's role, then — for a role that does not
    /// reach every channel — their name on each standing channel the invite
    /// listed. Answers the member and what to say once the caller's last
    /// write is done.
    ///
    /// The row is the admission. A room that is gone is passed over, and one
    /// whose roster would not take the name is said in the log: the person
    /// is here either way, and the owner puts them on a room by its roster.
    /// Each name is added to the roster **as it stands** under the roster's
    /// own writer — written whole from a read made out here, it undid
    /// whatever changed in between.
    fn admit_locked(
        &self,
        invite: &Invite,
        by: &PrincipalId,
        label: Option<String>,
        client: Option<String>,
    ) -> Result<(WorkspaceMember, Option<crate::workspace::PeopleChange>), StoreError> {
        let said = self.add_member_locked(
            by,
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
                match self.get_channel(channel) {
                    Ok(room) if room.roster.lists_human(by) => continue,
                    Ok(_) => {}
                    Err(_) => continue,
                }
                if let Err(e) = self.update_channel_with(channel, |room| {
                    room.roster.add_human(by);
                    Ok(())
                }) {
                    // LCOV_EXCL_START: a room read a moment ago refusing one name; a race with its deletion, or a disk arm
                    tracing::warn!("{by} was admitted and could not be put on {channel}: {e}");
                    // LCOV_EXCL_STOP
                }
            }
        }
        let member = self.member(by)?.ok_or_else(|| {
            // LCOV_EXCL_START: `add_member_locked` above wrote the row this reads
            StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-was-not-admitted",
                by = by.to_string()
            ))
        })?;
        // LCOV_EXCL_STOP
        Ok((member, said))
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
    use crate::members::Admission;
    use crate::workspace::{PeopleChange, StoreEvent};
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

    /// A claim that admits at once, as a host that admits on a valid code
    /// makes it.
    fn claim(ws: &Workspace, secret: &str, by: &PrincipalId) -> Result<ClaimOutcome, ClaimFailure> {
        ws.claim_and_admit(&hash(secret), by, None, None, false)
    }

    /// The code's own refusal, or a panic naming what came instead.
    fn refusal(outcome: Result<ClaimOutcome, ClaimFailure>) -> ClaimRefusal {
        match outcome {
            Err(ClaimFailure::Refused(why)) => why,
            other => panic!("not a refusal by the code: {other:?}"),
        }
    }

    fn words(e: &StoreError) -> String {
        e.to_string()
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
            refusal(claim(&ws, "nope", &bob)),
            ClaimRefusal::UnknownOrUsed
        );
        let claimed = ws
            .claim_and_admit(
                &hash("s1"),
                &bob,
                Some("bob".into()),
                Some("cli".into()),
                false,
            )
            .unwrap();
        let ClaimOutcome::Admitted {
            invite: spent,
            member,
        } = claimed
        else {
            panic!("a valid code admits at once: {claimed:?}");
        };
        assert!(matches!(spent.state, InviteState::Accepted { ref by, .. } if by == &bob));
        assert_eq!(member.pubkey, bob);
        assert_eq!(member.role, MemberRole::Guest);
        assert_eq!(
            member.label.as_deref(),
            Some("bob"),
            "the joiner's own word"
        );
        assert_eq!(member.client.as_deref(), Some("cli"));
        assert_eq!(member.invited_by, Some(ws.owner_principal()));
        assert!(
            ws.get_channel(&design.id).unwrap().roster.lists_human(&bob),
            "the guest is on the channel the code named"
        );
        assert_eq!(
            refusal(claim(&ws, "s1", &pk())),
            ClaimRefusal::UnknownOrUsed,
            "spent"
        );
        let no_longer_open = ws.revoke_invite(invite.id).unwrap_err();
        assert!(
            words(&no_longer_open).contains("cannot be withdrawn"),
            "{no_longer_open}"
        );
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
        assert_eq!(refusal(claim(&ws, "s2", &bob)), ClaimRefusal::Expired);
        assert_eq!(ws.invite(short.id).unwrap().state, InviteState::Expired);

        let withdrawn = ws
            .create_invite(MemberRole::Guest, vec![], None, 3600, hash("s3"))
            .unwrap();
        ws.revoke_invite(withdrawn.id).unwrap();
        assert_eq!(refusal(claim(&ws, "s3", &bob)), ClaimRefusal::Revoked);

        let asked = ws
            .create_invite(MemberRole::Guest, vec![], None, 3600, hash("s4"))
            .unwrap();
        let claimed = ws
            .claim_and_admit(&hash("s4"), &bob, Some("bob".into()), None, true)
            .unwrap();
        let ClaimOutcome::Waiting { invite } = claimed else {
            panic!("a host that admits by hand records a request: {claimed:?}");
        };
        assert!(matches!(invite.state, InviteState::Requested { .. }));
        assert_eq!(ws.member(&bob).unwrap(), None, "nobody is admitted yet");
        assert_eq!(
            refusal(ws.claim_and_admit(&hash("s4"), &pk(), None, None, true)),
            ClaimRefusal::AwaitingHost,
            "somebody else's claim of a code that waits"
        );
        let settled = ws.refuse_request(asked.id).unwrap();
        assert!(matches!(settled.state, InviteState::Refused { ref by, .. } if by == &bob));
        let settled_once = ws.admit_request(asked.id).unwrap_err();
        assert!(
            words(&settled_once).contains("not waiting on you"),
            "{settled_once}"
        );
        let again = ws.refuse_request(asked.id).unwrap_err();
        assert!(words(&again).contains("not waiting on you"), "{again}");
        assert_eq!(ws.invites().unwrap()[0].id, asked.id, "newest first");
    }

    /// Asking twice is asking once: the claimer of a code that waits on the
    /// host is told *waiting* again. Answered as anybody else's claim of it
    /// is — a refusal — their own node wrote *refused* over a request that
    /// still stood in the owner's Inbox.
    #[test]
    fn a_second_claim_while_the_first_waits_is_told_waiting() {
        let (_dir, ws) = ws();
        let bob = pk();
        ws.create_invite(MemberRole::Member, vec![], None, 3600, hash("w"))
            .unwrap();
        let first = ws
            .claim_and_admit(&hash("w"), &bob, Some("Bob".into()), None, true)
            .unwrap();
        let second = ws
            .claim_and_admit(&hash("w"), &bob, Some("Bobby".into()), None, true)
            .unwrap();
        assert!(matches!(first, ClaimOutcome::Waiting { .. }), "{first:?}");
        assert_eq!(second, first, "the same request, as it stands");
        // And when the host stops admitting by hand meanwhile: the request
        // that stands is still the owner's to answer.
        let third = claim(&ws, "w", &bob).unwrap();
        assert_eq!(third, first);
        assert_eq!(ws.member(&bob).unwrap(), None);
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
        let owner_role = ws
            .create_invite(MemberRole::Owner, vec![], None, 3600, hash("x"))
            .unwrap_err();
        assert!(
            words(&owner_role).contains("never the owner"),
            "{owner_role}"
        );
        let general = ws
            .create_invite(
                MemberRole::Guest,
                vec![ChannelId::general()],
                None,
                3600,
                hash("y"),
            )
            .unwrap_err();
        assert!(
            words(&general).contains("not a standing channel a guest can be put on"),
            "general is not a guest's room: {general}"
        );
        let nowhere = ws
            .create_invite(
                MemberRole::Guest,
                vec![ChannelId::new("nowhere").unwrap()],
                None,
                3600,
                hash("z"),
            )
            .unwrap_err();
        assert!(
            matches!(
                nowhere,
                StoreError::DefinitionNotFound {
                    kind: "channel",
                    ..
                }
            ),
            "{nowhere:?}"
        );
        assert!(ws.invites().unwrap().is_empty(), "none was made");
    }

    /// A label is what a person is called by everybody: an invitation's
    /// passes the one rule a joiner's does, and so does the member's it
    /// becomes when the joiner gives none.
    #[test]
    fn an_invitations_label_is_cleaned_and_becomes_the_members_name() {
        let (_dir, ws) = ws();
        let long = format!("Bob\u{7}{}", "x".repeat(200));
        let invite = ws
            .create_invite(MemberRole::Member, vec![], Some(long), 3600, hash("l"))
            .unwrap();
        let label = invite.label.clone().expect("a label");
        assert_eq!(label.chars().count(), bisa_core::MAX_LABEL_CHARS);
        assert!(!label.contains('\u{7}'));
        let bob = pk();
        let ClaimOutcome::Admitted { member, .. } = claim(&ws, "l", &bob).unwrap() else {
            panic!("admitted");
        };
        assert_eq!(member.label, Some(label));
    }

    // added by the coverage pass: invites.rs

    // --- the bare lines of the invites module ---

    /// An open code past its time lists as expired; claiming it marks the
    /// file — or not, when the file cannot be written — and refuses either
    /// way; a second claim meets the mark. An answer names the invite it
    /// cannot find. The file nobody may read is an I/O error by its path —
    /// to a list, and to a claim, which is the host's trouble and never the
    /// code's answer.
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
            let refused = claim(&ws, "s5", &bob);
            std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
            assert_eq!(refusal(refused), ClaimRefusal::Expired);
            assert_eq!(
                ws.read_invites().unwrap().invites[0].invite.state,
                InviteState::Pending,
                "the mark did not land"
            );
            let invites = ws.paths.invites_file();
            std::fs::set_permissions(&invites, std::fs::Permissions::from_mode(0o000)).unwrap();
            let unreadable = ws.invites();
            let unclaimable = claim(&ws, "s5", &bob);
            std::fs::set_permissions(&invites, std::fs::Permissions::from_mode(0o600)).unwrap();
            assert!(matches!(unreadable, Err(StoreError::Io { .. })));
            assert!(
                matches!(
                    unclaimable,
                    Err(ClaimFailure::NotAdmitted(StoreError::Io { .. }))
                ),
                "the host's trouble, not the code's answer: {unclaimable:?}"
            );
        }
        assert_eq!(refusal(claim(&ws, "s5", &bob)), ClaimRefusal::Expired);
        assert_eq!(
            ws.read_invites().unwrap().invites[0].invite.state,
            InviteState::Expired
        );
        assert_eq!(
            refusal(claim(&ws, "s5", &bob)),
            ClaimRefusal::Expired,
            "the mark is met"
        );
        let nobody = InviteId::from_ulid(crate::workspace::mint_ulid());
        for err in [
            ws.admit_request(nobody).map(|_| ()).unwrap_err(),
            ws.refuse_request(nobody).map(|_| ()).unwrap_err(),
            ws.revoke_invite(nobody).map(|_| ()).unwrap_err(),
        ] {
            assert!(words(&err).contains("no invite"), "{err}");
        }
        assert!(!bisa_collab_hashes_equal("ab", "abc"));
    }

    /// Admitting a guest puts them on the invite's channels: one that is
    /// gone is passed over, and one whose roster already names them — a
    /// name a roster kept from before they were removed — is left as it is.
    #[test]
    fn admitting_a_guest_skips_a_channel_that_is_gone_and_one_they_are_already_on() {
        let (_dir, ws) = ws();
        let bob = pk();
        let design = ws
            .create_channel("design", None, RosterPolicy::default(), Tags::default())
            .unwrap();
        let gone = ws
            .create_channel("gone", None, RosterPolicy::default(), Tags::default())
            .unwrap();
        let invite = ws
            .create_invite(
                MemberRole::Guest,
                vec![design.id.clone(), gone.id.clone()],
                Some("Bob".into()),
                3600,
                hash("g"),
            )
            .unwrap();
        ws.delete_channel_by_id(&bisa_core::DeletableChannelId::new(gone.id.clone()).unwrap())
            .unwrap();
        let waiting = ws
            .claim_and_admit(&hash("g"), &bob, None, None, true)
            .unwrap();
        assert!(matches!(waiting, ClaimOutcome::Waiting { .. }));

        let (settled, member) = ws.admit_request(invite.id).unwrap();
        assert!(matches!(settled.state, InviteState::Accepted { ref by, .. } if by == &bob));
        assert_eq!(member.role, MemberRole::Guest);
        assert_eq!(member.label.as_deref(), Some("Bob"), "the invite's word");
        assert_eq!(
            ws.get_channel(&design.id).unwrap().roster.humans(),
            std::slice::from_ref(&bob),
            "on the room that is there"
        );

        // A second invitation to the same room, claimed by somebody its
        // roster already names: listed once.
        ws.create_invite(
            MemberRole::Guest,
            vec![design.id.clone()],
            None,
            3600,
            hash("g2"),
        )
        .unwrap();
        let carol = pk();
        ws.add_member(carol.clone(), MemberRole::Guest, Admission::default())
            .unwrap();
        ws.set_roster_humans(&design.id, vec![bob.clone(), carol.clone()])
            .unwrap();
        ws.remove_member(&carol).unwrap();
        assert!(matches!(
            claim(&ws, "g2", &carol),
            Ok(ClaimOutcome::Admitted { .. })
        ));
        assert_eq!(
            ws.get_channel(&design.id).unwrap().roster.humans(),
            [bob.clone(), carol.clone()],
            "a name the roster kept is listed once"
        );
    }

    /// Somebody admitted by another door while their request waited is here
    /// already: answering the request settles it and changes nothing of
    /// them. Admitted again at the request's role, an admin made by key was
    /// a guest for the owner's click on an older row.
    #[test]
    fn a_request_from_somebody_admitted_meanwhile_settles_and_changes_nothing() {
        let (_dir, ws) = ws();
        let bob = pk();
        let asked = ws
            .create_invite(
                MemberRole::Guest,
                vec![],
                Some("Bob".into()),
                3600,
                hash("r"),
            )
            .unwrap();
        ws.claim_and_admit(&hash("r"), &bob, None, None, true)
            .unwrap();
        ws.add_member(bob.clone(), MemberRole::Admin, Admission::default())
            .unwrap();
        let mut events = ws.subscribe_store_events();
        let (settled, member) = ws.admit_request(asked.id).unwrap();
        assert!(matches!(settled.state, InviteState::Accepted { ref by, .. } if by == &bob));
        assert_eq!(member.role, MemberRole::Admin, "they stay what they are");
        assert_eq!(ws.member_role(&bob).unwrap(), Some(MemberRole::Admin));
        let mut said = Vec::new();
        while let Ok(ev) = events.try_recv() {
            said.push(matches!(ev, StoreEvent::InviteChanged { .. }));
        }
        assert_eq!(
            said,
            [true],
            "the request is settled, and nothing else is said"
        );
    }

    // --- one writer for the invitations ---

    /// An invitation is made while a claim lands on another: each reads the
    /// file, changes it and writes it. With two writers at once the new one
    /// was written beside the claimed one as it stood before the claim —
    /// pending — so a spent code admitted a second person; or the claim
    /// wrote the list as it stood before the new one, and that code was
    /// unknown from its first day.
    #[test]
    fn an_invitation_made_while_a_claim_lands_leaves_the_code_spent() {
        let (_dir, ws) = ws();
        for round in 0..40 {
            let (spent, fresh) = (format!("spent-{round}"), format!("fresh-{round}"));
            ws.create_invite(MemberRole::Member, vec![], None, 3600, hash(&spent))
                .unwrap();
            let bob = pk();
            let together = std::sync::Barrier::new(2);
            std::thread::scope(|s| {
                s.spawn(|| {
                    together.wait();
                    claim(&ws, &spent, &bob).unwrap();
                });
                s.spawn(|| {
                    together.wait();
                    ws.create_invite(MemberRole::Guest, vec![], None, 3600, hash(&fresh))
                        .unwrap();
                });
            });
            assert_eq!(
                refusal(claim(&ws, &spent, &pk())),
                ClaimRefusal::UnknownOrUsed,
                "round {round}: a spent code admitted a second person"
            );
            assert!(
                matches!(claim(&ws, &fresh, &pk()), Ok(ClaimOutcome::Admitted { .. })),
                "round {round}: the invitation made meanwhile was lost"
            );
        }
    }

    /// One code, two claimers at the same moment: one is admitted, the
    /// other reads *unknown or used*, and one member row is written.
    #[test]
    fn two_claims_of_one_code_at_once_admit_one() {
        let (_dir, ws) = ws();
        for round in 0..40 {
            let secret = format!("one-{round}");
            ws.create_invite(MemberRole::Member, vec![], None, 3600, hash(&secret))
                .unwrap();
            let (bob, eve) = (pk(), pk());
            let together = std::sync::Barrier::new(2);
            let (first, second) = std::thread::scope(|s| {
                let a = s.spawn(|| {
                    together.wait();
                    claim(&ws, &secret, &bob)
                });
                let b = s.spawn(|| {
                    together.wait();
                    claim(&ws, &secret, &eve)
                });
                (a.join().unwrap(), b.join().unwrap())
            });
            let admitted = [&first, &second]
                .iter()
                .filter(|o| matches!(o, Ok(ClaimOutcome::Admitted { .. })))
                .count();
            assert_eq!(admitted, 1, "round {round}: {first:?} / {second:?}");
            let members = [&bob, &eve]
                .iter()
                .filter(|p| ws.member(p).unwrap().is_some())
                .count();
            assert_eq!(members, 1, "round {round}: one code, one member");
        }
    }

    /// A guest is put on the code's room while its roster is being edited —
    /// six people put on it one after another, as a person working down the
    /// People picker does: every name stands, and the guest's. The name is
    /// added to the roster as it stands under the roster's own writer.
    /// Written whole from a read made before it waited for that writer, the
    /// admission put back the roster it had read and undid what landed
    /// meanwhile.
    #[test]
    fn a_guest_is_put_on_a_room_without_undoing_a_roster_change() {
        let (_dir, ws) = ws();
        for round in 0..12 {
            let design = ws
                .create_channel(
                    &format!("design {round}"),
                    None,
                    RosterPolicy::default(),
                    Tags::default(),
                )
                .unwrap();
            let secret = format!("room-{round}");
            ws.create_invite(
                MemberRole::Guest,
                vec![design.id.clone()],
                None,
                3600,
                hash(&secret),
            )
            .unwrap();
            let bob = pk();
            let others: Vec<PrincipalId> = (0..6).map(|_| pk()).collect();
            for p in &others {
                ws.add_member(p.clone(), MemberRole::Guest, Admission::default())
                    .unwrap();
            }
            let together = std::sync::Barrier::new(2);
            std::thread::scope(|s| {
                s.spawn(|| {
                    together.wait();
                    for p in &others {
                        ws.update_channel_with(&design.id, |room| {
                            room.roster.add_human(p);
                            Ok(())
                        })
                        .unwrap();
                    }
                });
                s.spawn(|| {
                    together.wait();
                    claim(&ws, &secret, &bob).unwrap();
                });
            });
            let roster = ws.get_channel(&design.id).unwrap().roster;
            assert!(
                roster.lists_human(&bob),
                "round {round}: the guest is not on their room"
            );
            for p in &others {
                assert!(
                    roster.lists_human(p),
                    "round {round}: the admission undid a roster change"
                );
            }
        }
    }

    /// Somebody already here claims nothing — a member, and the owner's own
    /// key: they are answered by what they are, and the code stands for
    /// whoever it was made for. Taken, it restated their role to the code's.
    #[test]
    fn a_member_who_claims_another_code_keeps_their_role_and_the_code() {
        let (_dir, ws) = ws();
        let mut events = ws.subscribe_store_events();
        let alice = pk();
        ws.add_member(alice.clone(), MemberRole::Admin, Admission::default())
            .unwrap();
        ws.create_invite(MemberRole::Guest, vec![], None, 3600, hash("g"))
            .unwrap();
        while events.try_recv().is_ok() {}

        for who in [alice.clone(), ws.owner_principal()] {
            let before = ws.member(&who).unwrap().unwrap();
            for waits in [false, true] {
                let outcome = ws
                    .claim_and_admit(&hash("g"), &who, Some("x".into()), None, waits)
                    .unwrap();
                assert_eq!(
                    outcome,
                    ClaimOutcome::AlreadyMember {
                        member: before.clone()
                    }
                );
            }
        }
        assert_eq!(ws.member_role(&alice).unwrap(), Some(MemberRole::Admin));
        assert_eq!(ws.invites().unwrap()[0].state, InviteState::Pending);
        assert!(events.try_recv().is_err(), "nothing was written or said");
        // The code is still good for the person it was made for.
        assert!(matches!(
            claim(&ws, "g", &pk()),
            Ok(ClaimOutcome::Admitted { .. })
        ));
    }

    /// A good code whose admission cannot be written is claimable again —
    /// here a key the members' table refuses, one of this workspace's own
    /// agents. Marked spent and left so, the code was used and nobody was
    /// admitted. The same for a request the owner admits.
    #[test]
    fn a_claim_whose_admission_fails_can_be_made_again() {
        let (_dir, ws) = ws();
        let agent = ws.get_agent(&bisa_core::AgentId::general()).unwrap().pubkey;
        let invite = ws
            .create_invite(MemberRole::Member, vec![], None, 3600, hash("c"))
            .unwrap();
        let failed = claim(&ws, "c", &agent);
        assert!(
            matches!(&failed, Err(ClaimFailure::NotAdmitted(e)) if words(e).contains("own agents")),
            "{failed:?}"
        );
        assert_eq!(
            ws.invite(invite.id).unwrap().state,
            InviteState::Pending,
            "the code is as it was"
        );
        assert_eq!(ws.member(&agent).unwrap(), None);
        assert!(matches!(
            claim(&ws, "c", &pk()),
            Ok(ClaimOutcome::Admitted { .. })
        ));

        let asked = ws
            .create_invite(MemberRole::Member, vec![], None, 3600, hash("a"))
            .unwrap();
        let waiting = ws
            .claim_and_admit(&hash("a"), &agent, None, None, true)
            .unwrap();
        assert!(matches!(waiting, ClaimOutcome::Waiting { .. }));
        let failed = ws.admit_request(asked.id).unwrap_err();
        assert!(words(&failed).contains("own agents"), "{failed}");
        assert!(
            matches!(
                ws.invite(asked.id).unwrap().state,
                InviteState::Requested { .. }
            ),
            "the request waits still: the owner may refuse it"
        );
        ws.refuse_request(asked.id).unwrap();
    }

    /// What an admission says, it says after its last write: the rosters
    /// are written before anybody hears *joined*, so the welcome that word
    /// starts names the rooms the code gave.
    #[test]
    fn an_admission_is_said_after_the_rosters_are_written() {
        let (_dir, ws) = ws();
        let design = ws
            .create_channel("design", None, RosterPolicy::default(), Tags::default())
            .unwrap();
        ws.create_invite(
            MemberRole::Guest,
            vec![design.id.clone()],
            None,
            3600,
            hash("o"),
        )
        .unwrap();
        let mut events = ws.subscribe_store_events();
        let bob = pk();
        claim(&ws, "o", &bob).unwrap();
        let mut said = Vec::new();
        while let Ok(ev) = events.try_recv() {
            said.push(match ev {
                StoreEvent::ConversationSnapshot { .. } => "room",
                StoreEvent::InviteChanged { .. } => "invite",
                StoreEvent::PeopleChanged {
                    change: PeopleChange::Joined { .. },
                    ..
                } => "joined",
                _ => "other",
            });
        }
        let at = |word: &str| said.iter().position(|w| *w == word);
        assert!(
            at("room") < at("joined") && at("room").is_some(),
            "the roster first, then the word: {said:?}"
        );
        assert_eq!(
            said.iter().filter(|w| **w == "joined").count(),
            1,
            "{said:?}"
        );
    }
}
