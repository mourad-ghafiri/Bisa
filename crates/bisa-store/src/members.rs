//! Workspace membership: the local policy table that authorizes remote events
//! at ingest, and says as what. Identity is minted, authority is attestation,
//! membership is THIS table — no accounts anywhere.
//!
//! Truth lives in `members.json`; the SQLite `members` table is a rebuildable
//! mirror of the columns a lookup needs. The owner is always a member and can
//! never be removed or demoted ([`bisa_core::check_member_change`]); every
//! other row is a **hosted** human on another node, with the role the owner
//! gave them ([`bisa_core::MemberRole`]).

use crate::error::StoreError;
use crate::problems::{ProblemKind, WorkspaceProblem};
use crate::workspace::{now_secs, Workspace};
use bisa_core::{check_member_change, AttachmentRef, MemberRole, PrincipalId, WorkspaceMember};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize, Default)]
pub(crate) struct MemberFile {
    members: Vec<WorkspaceMember>,
}

/// What a new member is admitted with — who invited them, what they said
/// they are, the face they brought — beside the role.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Admission {
    pub label: Option<String>,
    pub photo: Option<AttachmentRef>,
    pub invited_by: Option<PrincipalId>,
    pub client: Option<String>,
}

impl Workspace {
    /// The member file as it stands: absent is nobody yet, present is read
    /// whole. A file that is there and does not parse is refused, never read
    /// as empty — the next write would replace every hosted member and
    /// their roles with the owner alone.
    fn read_member_file(&self) -> Result<MemberFile, StoreError> {
        let path = self.paths.members_file();
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| StoreError::unreadable(&path, "member file", e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(MemberFile::default()),
            Err(e) => Err(StoreError::io(path.display().to_string(), e)),
        }
    }

    /// The member file for the open and the rebuild. A file that is there
    /// and does not parse is moved under `quarantine/` and the owner alone
    /// is a member again — the people it named, and their roles, are kept
    /// in the moved file for the person to admit again, and the open says
    /// so. Every other read stays strict: a write while the file is broken
    /// happens after the open repaired it.
    fn read_member_file_or_quarantine(&self) -> Result<MemberFile, StoreError> {
        match self.read_member_file() {
            Err(StoreError::Unreadable { path, what, reason }) => {
                let to = self.paths.quarantine(Path::new(&path), now_secs())?;
                self.record_problem(WorkspaceProblem::new(
                    ProblemKind::Recreated,
                    path.clone(),
                    bisa_core::text!(
                        "error-store-problem-recreated",
                        what = what.to_string(),
                        path = path,
                        reason = reason,
                        to = to.display().to_string()
                    ),
                    Some(&to),
                ));
                Ok(MemberFile::default())
            }
            other => other,
        }
    }

    /// Whether the member file names `pubkey` — the file's word, whatever
    /// the mirror says.
    pub(crate) fn read_member_file_has(&self, pubkey: &PrincipalId) -> Result<bool, StoreError> {
        Ok(self
            .read_member_file()?
            .members
            .iter()
            .any(|m| m.pubkey == *pubkey))
    }

    fn write_member_file(&self, file: &MemberFile) -> Result<(), StoreError> {
        crate::paths::write_atomic(
            &self.paths.members_file(),
            &serde_json::to_vec_pretty(file)?,
        )
    }

    fn index_member(&self, m: &WorkspaceMember) -> Result<(), StoreError> {
        self.idx().upsert_member(
            m.pubkey.as_hex(),
            m.role.as_str(),
            m.label.as_deref(),
            m.added_at,
        )
    }

    /// The mirror made to say what the file says, whole: every row the file
    /// names and none it does not. What reads the mirror — `is_member`, and
    /// through it a roster, a direct channel and a mention — must not go on
    /// answering for people a file moved aside at open used to name.
    fn mirror_members(&self, members: &[WorkspaceMember]) -> Result<(), StoreError> {
        let idx = self.idx();
        idx.in_transaction(|| {
            idx.clear_members()?;
            for m in members {
                idx.upsert_member(
                    m.pubkey.as_hex(),
                    m.role.as_str(),
                    m.label.as_deref(),
                    m.added_at,
                )?;
            }
            Ok(())
        })
    }

    /// Called at `open()`: the workspace owner identity is always a member.
    pub(crate) fn ensure_owner_member(&self) -> Result<(), StoreError> {
        let owner = self.owner_principal();
        let mut file = self.read_member_file_or_quarantine()?;
        if !file.members.iter().any(|m| m.pubkey == owner) {
            file.members.push(WorkspaceMember {
                pubkey: owner.clone(),
                role: MemberRole::Owner,
                // No name until the person gives one (their profile): a
                // word made up here would be read as theirs, by every
                // screen and every guest, in one language.
                label: None,
                photo: None,
                invited_by: None,
                client: None,
                added_at: now_secs(),
            });
            self.write_member_file(&file)?;
        }
        self.mirror_members(&file.members)
    }

    fn say(&self, pubkey: &PrincipalId, change: crate::workspace::PeopleChange) {
        self.emit_store_event(crate::workspace::StoreEvent::PeopleChanged {
            pubkey: pubkey.clone(),
            change,
        });
    }

    /// Admit a hosted member at a role, or re-state one who is here. The
    /// owner's row is immutable and nobody else may be made owner. A row
    /// already there keeps who invited it and what it said it was unless the
    /// admission says otherwise — and says what moved: a role as a role
    /// change, a name or a face as a profile change, nothing when nothing
    /// did. *Joined* is said once, for the admission that made the row.
    pub fn add_member(
        &self,
        pubkey: PrincipalId,
        role: MemberRole,
        admission: Admission,
    ) -> Result<(), StoreError> {
        let _one_writer = self.people_writer();
        if let Some(change) = self.add_member_locked(&pubkey, role, admission)? {
            self.say(&pubkey, change);
        }
        Ok(())
    }

    /// [`Self::add_member`] under the people's writer, answering what to say
    /// once the caller's last write is done.
    pub(crate) fn add_member_locked(
        &self,
        pubkey: &PrincipalId,
        role: MemberRole,
        admission: Admission,
    ) -> Result<Option<crate::workspace::PeopleChange>, StoreError> {
        use crate::workspace::PeopleChange;
        let mut file = self.read_member_file()?;
        check_member_change(&file.members, pubkey, Some(role))?;
        if role == MemberRole::Owner && *pubkey != self.owner_principal() {
            // LCOV_EXCL_START: the core refuses a second owner first (`check_member_change`); this holds when the file has no owner row yet
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-workspace-has-exactly-one-owner-own-keypair"
            )));
            // LCOV_EXCL_STOP
        }
        // A person is a human on another node; one of this workspace's own
        // agents, admitted as one, would be relayed to and listed as a face.
        if role.is_hosted() && self.idx().agent_id_for_pubkey(pubkey.as_hex())?.is_some() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-key-of-agent-not-person",
                pubkey = pubkey.to_string()
            )));
        }
        // The one rule every label passes, an invitation's and one typed by
        // the owner among them.
        let label = bisa_core::clean_label(admission.label);
        let said = match file.members.iter_mut().find(|m| m.pubkey == *pubkey) {
            Some(existing) => {
                let before = existing.clone();
                existing.role = role;
                existing.label = label.or(existing.label.take());
                existing.photo = admission.photo.or(existing.photo.take());
                existing.invited_by = admission.invited_by.or(existing.invited_by.take());
                existing.client = admission.client.or(existing.client.take());
                if *existing == before {
                    return Ok(None);
                }
                if existing.role != before.role {
                    PeopleChange::RoleChanged { role }
                } else {
                    PeopleChange::ProfileChanged
                }
            }
            None => {
                file.members.push(WorkspaceMember {
                    pubkey: pubkey.clone(),
                    role,
                    label,
                    photo: admission.photo,
                    invited_by: admission.invited_by,
                    client: admission.client,
                    added_at: now_secs(),
                });
                PeopleChange::Joined { role }
            }
        };
        self.write_member_file(&file)?;
        if let Some(row) = file.members.iter().find(|m| m.pubkey == *pubkey) {
            self.index_member(row)?;
        } // LCOV_EXCL_LINE: the row was written just above
        Ok(role.is_hosted().then_some(said))
    }

    /// Change a hosted member's role. The owner's cannot change; a role is
    /// never the owner's; an unknown person is a refusal, not a creation.
    /// The role they already hold is no change: nothing is written and
    /// nobody is told.
    pub fn set_role(
        &self,
        pubkey: &PrincipalId,
        role: MemberRole,
    ) -> Result<WorkspaceMember, StoreError> {
        let _one_writer = self.people_writer();
        let mut file = self.read_member_file()?;
        check_member_change(&file.members, pubkey, Some(role))?;
        if role == MemberRole::Owner {
            // LCOV_EXCL_START: the core refuses a second owner first (`check_member_change`); this holds when the file has no owner row yet
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-workspace-has-exactly-one-owner-own-keypair"
            )));
            // LCOV_EXCL_STOP
        }
        let row = file
            .members
            .iter_mut()
            .find(|m| &m.pubkey == pubkey)
            .ok_or_else(|| {
                StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-not-member",
                    pubkey = pubkey.to_string()
                ))
            })?;
        if row.role == role {
            return Ok(row.clone());
        }
        row.role = role;
        let row = row.clone();
        self.write_member_file(&file)?;
        self.index_member(&row)?;
        self.say(pubkey, crate::workspace::PeopleChange::RoleChanged { role });
        Ok(row)
    }

    /// A member's profile — their label, their face — as they set it: on the
    /// host, a hosted member's row when their node says so; on their own
    /// node, the owner's row. Each part is kept when `None` and cleared when
    /// `Some(None)`. One write, one `ProfileChanged` — the owner's row too,
    /// since the pump carries the owner's profile to every host it is a
    /// guest of — and neither when the profile said is the one that stands.
    /// An unknown person is a refusal, not a creation; the role never moves
    /// here.
    pub fn set_member_profile(
        &self,
        pubkey: &PrincipalId,
        label: Option<Option<String>>,
        photo: Option<Option<AttachmentRef>>,
    ) -> Result<WorkspaceMember, StoreError> {
        let _one_writer = self.people_writer();
        let mut file = self.read_member_file()?;
        let row = file
            .members
            .iter_mut()
            .find(|m| &m.pubkey == pubkey)
            .ok_or_else(|| {
                StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-not-member",
                    pubkey = pubkey.to_string()
                ))
            })?;
        let before = row.clone();
        if let Some(label) = label {
            row.label = bisa_core::clean_label(label);
        }
        if let Some(photo) = photo {
            row.photo = photo;
        }
        if *row == before {
            return Ok(before);
        }
        let row = row.clone();
        self.write_member_file(&file)?;
        self.index_member(&row)?;
        self.say(pubkey, crate::workspace::PeopleChange::ProfileChanged);
        Ok(row)
    }

    /// Take one member's row away — the file's, then the mirror's. The
    /// workspace owner can never be removed. Removing a person whole — their
    /// held messages and their places on rosters with the row — is
    /// [`Self::remove_person`].
    pub fn remove_member(&self, pubkey: &PrincipalId) -> Result<(), StoreError> {
        let _one_writer = self.people_writer();
        if self.remove_member_locked(pubkey)? {
            self.say(pubkey, crate::workspace::PeopleChange::Left);
        }
        Ok(())
    }

    /// [`Self::remove_member`] under the people's writer: whether a row was
    /// there to take — the file's or the mirror's. Each is taken when it is
    /// there, so a removal whose mirror write failed is finished, and said,
    /// by the next one.
    fn remove_member_locked(&self, pubkey: &PrincipalId) -> Result<bool, StoreError> {
        let mut file = self.read_member_file()?;
        check_member_change(&file.members, pubkey, None)?;
        let in_file = file.members.iter().any(|m| m.pubkey == *pubkey);
        let in_mirror = self.idx().is_member(pubkey.as_hex())?;
        if in_file {
            file.members.retain(|m| m.pubkey != *pubkey);
            self.write_member_file(&file)?;
        }
        if in_mirror {
            self.idx().delete_member(pubkey.as_hex())?;
        }
        Ok(in_file || in_mirror)
    }

    /// Remove a person, whole: their row, what is held of theirs, their
    /// places on rosters. The one removal — the owner's and a person's own
    /// leave alike.
    ///
    /// **The row goes first.** From then on ingest refuses them, a roster
    /// takes no new add of their name, and a verdict landing meanwhile on a
    /// message of theirs finds nobody to hold it for. *Left* is said there,
    /// once: the person is out from that moment, whatever a later step does.
    /// Each step is safe to run again, so a removal stopped midway — a held
    /// list that would not read — is finished by asking again, and says
    /// nothing twice. Somebody who is not here, with nothing left of theirs,
    /// is nobody to remove.
    pub fn remove_person(&self, pubkey: &PrincipalId) -> Result<(), StoreError> {
        let _one_writer = self.people_writer();
        let was_member = self.remove_member_locked(pubkey)?;
        if was_member {
            self.say(pubkey, crate::workspace::PeopleChange::Left);
        }
        let held = self.drop_held_of(pubkey)?;
        let rooms = self.unroster_human_everywhere(pubkey)?;
        if !was_member && held == 0 && rooms == 0 {
            return Err(StoreError::DefinitionNotFound {
                kind: "person",
                id: pubkey.to_string(),
            });
        }
        Ok(())
    }

    /// Every row, the owner first, then in the order they were admitted.
    pub fn members(&self) -> Result<Vec<WorkspaceMember>, StoreError> {
        let mut rows = self.read_member_file()?.members;
        rows.sort_by_key(|m| (m.role != MemberRole::Owner, m.added_at));
        Ok(rows)
    }

    /// The people: every member but the owner.
    pub fn people(&self) -> Result<Vec<WorkspaceMember>, StoreError> {
        Ok(self
            .members()?
            .into_iter()
            .filter(WorkspaceMember::is_person)
            .collect())
    }

    /// The people's pubkeys — what a roster derivation is given.
    pub fn people_pubkeys(&self) -> Result<Vec<PrincipalId>, StoreError> {
        Ok(self.people()?.into_iter().map(|m| m.pubkey).collect())
    }

    pub fn is_member(&self, pubkey: &PrincipalId) -> Result<bool, StoreError> {
        self.idx().is_member(pubkey.as_hex())
    }

    /// The role a pubkey holds here, or none for a stranger.
    pub fn member_role(&self, pubkey: &PrincipalId) -> Result<Option<MemberRole>, StoreError> {
        Ok(self
            .read_member_file()?
            .members
            .into_iter()
            .find(|m| &m.pubkey == pubkey)
            .map(|m| m.role))
    }

    /// One member's row.
    pub fn member(&self, pubkey: &PrincipalId) -> Result<Option<WorkspaceMember>, StoreError> {
        Ok(self
            .read_member_file()?
            .members
            .into_iter()
            .find(|m| &m.pubkey == pubkey))
    }

    pub(crate) fn reindex_members(&self) -> Result<(), StoreError> {
        let file = self.read_member_file_or_quarantine()?;
        self.mirror_members(&file.members)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::held::HeldReason;
    use crate::identity::MemoryKeyStore;
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

    #[test]
    fn owner_is_always_a_member_and_unremovable() {
        let (_dir, ws) = ws();
        let owner = ws.owner_principal();
        assert!(ws.is_member(&owner).unwrap());
        let members = ws.members().unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].role, MemberRole::Owner);
        assert_eq!(ws.member_role(&owner).unwrap(), Some(MemberRole::Owner));
        let immutable = "cannot be removed or demoted";
        for removal in [ws.remove_member(&owner), ws.remove_person(&owner)] {
            let refused = removal.unwrap_err();
            assert!(refused.to_string().contains(immutable), "{refused}");
        }
        for role in MemberRole::HOSTED {
            let refused = ws
                .add_member(owner.clone(), role, Admission::default())
                .unwrap_err();
            assert!(
                refused.to_string().contains(immutable),
                "the owner cannot be demoted to {role}: {refused}"
            );
            let refused = ws.set_role(&owner, role).unwrap_err();
            assert!(refused.to_string().contains(immutable), "{refused}");
        }
        let second = ws
            .add_member(pk(), MemberRole::Owner, Admission::default())
            .unwrap_err();
        assert!(
            second.to_string().contains("exactly one owner"),
            "one owner: {second}"
        );
        assert!(ws.people().unwrap().is_empty());
        assert_eq!(ws.members().unwrap().len(), 1, "nothing was written");
    }

    #[test]
    fn people_are_admitted_with_a_role_and_who_let_them_in_and_their_role_moves() {
        let (_dir, ws) = ws();
        let alice = pk();
        let bob = pk();
        ws.add_member(
            alice.clone(),
            MemberRole::Guest,
            Admission {
                label: Some("alice".into()),
                photo: None,
                invited_by: Some(ws.owner_principal()),
                client: Some("mobile".into()),
            },
        )
        .unwrap();
        ws.add_member(bob.clone(), MemberRole::Member, Admission::default())
            .unwrap();
        // Re-stating keeps what the first admission said.
        ws.add_member(
            alice.clone(),
            MemberRole::Member,
            Admission {
                label: Some("alice2".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(ws.is_member(&alice).unwrap());
        assert_eq!(ws.member_role(&alice).unwrap(), Some(MemberRole::Member));
        assert_eq!(
            ws.member_role(&pk()).unwrap(),
            None,
            "a stranger has no role"
        );
        let row = ws.member(&alice).unwrap().unwrap();
        assert_eq!(row.label.as_deref(), Some("alice2"));
        assert_eq!(row.invited_by, Some(ws.owner_principal()));
        assert_eq!(row.client.as_deref(), Some("mobile"));
        assert_eq!(ws.members().unwrap().len(), 3);
        assert_eq!(
            ws.members().unwrap()[0].role,
            MemberRole::Owner,
            "the owner first"
        );
        assert_eq!(ws.people().unwrap().len(), 2);
        assert_eq!(
            ws.people_pubkeys().unwrap(),
            vec![alice.clone(), bob.clone()]
        );

        let promoted = ws.set_role(&bob, MemberRole::Admin).unwrap();
        assert_eq!(promoted.role, MemberRole::Admin);
        let stranger = ws.set_role(&pk(), MemberRole::Guest).unwrap_err();
        assert!(
            stranger.to_string().contains("is not a member"),
            "{stranger}"
        );
        let second = ws.set_role(&bob, MemberRole::Owner).unwrap_err();
        assert!(second.to_string().contains("exactly one owner"), "{second}");

        ws.remove_member(&bob).unwrap();
        assert!(!ws.is_member(&bob).unwrap());
        let before = ws.members().unwrap();
        ws.rebuild_index().unwrap();
        assert_eq!(ws.members().unwrap(), before);
        assert!(
            ws.is_member(&alice).unwrap(),
            "the mirror is rebuilt from the file"
        );
    }

    #[test]
    fn a_profile_is_one_write_and_one_word_for_the_owner_and_a_person_alike() {
        let (_dir, ws) = ws();
        let mut events = ws.subscribe_store_events();
        let face = AttachmentRef {
            sha256: "f".repeat(64),
            name: "me-face.jpg".into(),
            mime: "image/jpeg".into(),
            size: 900,
        };
        let owner = ws.owner_principal();
        let row = ws
            .set_member_profile(
                &owner,
                Some(Some("  Ada\u{7} ".into())),
                Some(Some(face.clone())),
            )
            .unwrap();
        assert_eq!(
            row.label.as_deref(),
            Some("Ada"),
            "the label is cleaned as a joiner's is"
        );
        assert_eq!(row.photo, Some(face.clone()));
        assert_eq!(row.role, MemberRole::Owner, "the role never moves here");
        match events.try_recv().unwrap() {
            crate::workspace::StoreEvent::PeopleChanged { pubkey, change } => {
                assert_eq!(pubkey, owner);
                assert_eq!(change, crate::workspace::PeopleChange::ProfileChanged);
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(events.try_recv().is_err(), "one write, one word");
        // Kept when absent, cleared when null.
        let row = ws.set_member_profile(&owner, None, None).unwrap();
        assert_eq!(row.label.as_deref(), Some("Ada"));
        assert_eq!(row.photo, Some(face));
        let row = ws
            .set_member_profile(&owner, Some(None), Some(None))
            .unwrap();
        assert_eq!(row.label, None);
        assert_eq!(row.photo, None);
        assert_eq!(
            ws.members().unwrap()[0].pubkey,
            owner,
            "the owner stays first"
        );
        // A stranger has no row to set.
        let stranger = ws
            .set_member_profile(&pk(), Some(Some("x".into())), None)
            .unwrap_err();
        assert!(
            stranger.to_string().contains("is not a member"),
            "{stranger}"
        );
        // A person's face brought at admission is kept and re-stated.
        let alice = pk();
        ws.add_member(
            alice.clone(),
            MemberRole::Guest,
            Admission {
                photo: Some(AttachmentRef {
                    sha256: "a".repeat(64),
                    name: "alice.jpg".into(),
                    mime: "image/jpeg".into(),
                    size: 1,
                }),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(ws.member(&alice).unwrap().unwrap().photo.is_some());
        ws.add_member(alice.clone(), MemberRole::Member, Admission::default())
            .unwrap();
        assert!(
            ws.member(&alice).unwrap().unwrap().photo.is_some(),
            "a re-admission keeps the face"
        );
    }

    // added by the coverage pass: members.rs

    // --- the bare lines of the members module ---

    #[cfg(unix)]
    #[test]
    fn a_member_file_nobody_may_read_is_an_io_error_by_its_path() {
        use std::os::unix::fs::PermissionsExt;
        let (_d, ws) = ws();
        let file = ws.paths.members_file();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
        let unreadable = ws.members();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(
            matches!(unreadable, Err(StoreError::Io { .. })),
            "{unreadable:?}"
        );
    }

    // --- one writer for the people ---

    /// A member's own node says who they are while the owner removes them:
    /// each is a read of the file, a change and a write. With two writers at
    /// once the profile wrote the file as it had read it — the person in it
    /// — over the removal: removed, told so, and a member again.
    #[test]
    fn a_profile_and_a_removal_at_once_never_bring_the_person_back() {
        let (_dir, ws) = ws();
        for round in 0..40 {
            let bob = pk();
            ws.add_member(bob.clone(), MemberRole::Member, Admission::default())
                .unwrap();
            let together = std::sync::Barrier::new(2);
            std::thread::scope(|s| {
                s.spawn(|| {
                    together.wait();
                    ws.remove_member(&bob).unwrap();
                });
                s.spawn(|| {
                    together.wait();
                    // Refused when the removal came first; either way the
                    // person is gone once both are done.
                    ws.set_member_profile(&bob, Some(Some("Bob".into())), None)
                        .ok();
                });
            });
            assert_eq!(
                ws.member(&bob).unwrap(),
                None,
                "round {round}: the file has them back"
            );
            assert!(
                !ws.is_member(&bob).unwrap(),
                "round {round}: the mirror has them back"
            );
        }
    }

    /// A role is changed while somebody else is admitted: both stand.
    #[test]
    fn a_role_change_and_an_admission_at_once_keep_both() {
        let (_dir, ws) = ws();
        for round in 0..40 {
            let (alice, carol) = (pk(), pk());
            ws.add_member(alice.clone(), MemberRole::Member, Admission::default())
                .unwrap();
            let together = std::sync::Barrier::new(2);
            std::thread::scope(|s| {
                s.spawn(|| {
                    together.wait();
                    ws.set_role(&alice, MemberRole::Admin).unwrap();
                });
                s.spawn(|| {
                    together.wait();
                    ws.add_member(carol.clone(), MemberRole::Guest, Admission::default())
                        .unwrap();
                });
            });
            assert_eq!(
                ws.member_role(&alice).unwrap(),
                Some(MemberRole::Admin),
                "round {round}: the role change was lost"
            );
            assert_eq!(
                ws.member_role(&carol).unwrap(),
                Some(MemberRole::Guest),
                "round {round}: the admission was lost"
            );
        }
    }

    /// What was said about one person since the receiver was last drained.
    fn said_of(
        events: &mut tokio::sync::broadcast::Receiver<StoreEvent>,
        who: &PrincipalId,
    ) -> Vec<PeopleChange> {
        let mut said = Vec::new();
        while let Ok(ev) = events.try_recv() {
            if let StoreEvent::PeopleChanged { pubkey, change } = ev {
                if pubkey == *who {
                    said.push(change);
                }
            }
        }
        said
    }

    /// One removal, whole: the row, what is held of theirs, their places on
    /// rosters — the row first. Stopped at its second step by a held list
    /// that will not read, the person is out already and *left* is said;
    /// asked again once the list reads, the rest is done and nothing is said
    /// twice; asked a third time, there is nobody to remove.
    #[test]
    fn a_removal_stopped_midway_finishes_when_asked_again_and_is_said_once() {
        let (_dir, ws) = ws();
        let bob = pk();
        ws.add_member(bob.clone(), MemberRole::Guest, Admission::default())
            .unwrap();
        let design = ws
            .create_channel("design", None, RosterPolicy::default(), Tags::default())
            .unwrap();
        ws.set_roster_humans(&design.id, vec![bob.clone()]).unwrap();
        ws.hold_message("e1", "design", bob.clone(), HeldReason::Pending)
            .unwrap();
        let held = ws.paths.held_file();
        let sound = std::fs::read(&held).unwrap();
        std::fs::write(&held, b"{ not json").unwrap();
        let mut events = ws.subscribe_store_events();

        let stopped = ws.remove_person(&bob).unwrap_err();
        assert!(
            matches!(
                stopped,
                StoreError::Unreadable {
                    what: "held messages",
                    ..
                }
            ),
            "{stopped:?}"
        );
        assert_eq!(ws.member(&bob).unwrap(), None, "out from the first step");
        assert!(!ws.is_member(&bob).unwrap());
        assert!(
            ws.get_channel(&design.id).unwrap().roster.lists_human(&bob),
            "the step after the one that stopped was not reached"
        );

        std::fs::write(&held, &sound).unwrap();
        ws.remove_person(&bob).unwrap();
        assert!(ws.held_messages().unwrap().is_empty());
        assert!(!ws.get_channel(&design.id).unwrap().roster.lists_human(&bob));
        assert_eq!(
            said_of(&mut events, &bob),
            vec![PeopleChange::Left],
            "said once, at the step that took the row"
        );

        for nobody in [bob, pk()] {
            let refused = ws.remove_person(&nobody).unwrap_err();
            assert!(
                matches!(
                    refused,
                    StoreError::DefinitionNotFound { kind: "person", .. }
                ),
                "{refused:?}"
            );
        }
        assert!(said_of(&mut events, &ws.owner_principal()).is_empty());
    }

    /// A removal whose mirror write failed left the file without the person
    /// and the mirror with them — and nothing said. The next removal takes
    /// the mirror's row and says *left*: the person is told.
    #[test]
    fn a_row_the_mirror_alone_still_holds_is_taken_and_said_by_the_next_removal() {
        let (_dir, ws) = ws();
        let bob = pk();
        ws.add_member(bob.clone(), MemberRole::Member, Admission::default())
            .unwrap();
        // The file without the row, the mirror with it: what a failed
        // mirror write leaves.
        let mut file = ws.read_member_file().unwrap();
        file.members.retain(|m| m.pubkey != bob);
        ws.write_member_file(&file).unwrap();
        assert!(ws.is_member(&bob).unwrap());
        let mut events = ws.subscribe_store_events();
        ws.remove_person(&bob).unwrap();
        assert!(!ws.is_member(&bob).unwrap());
        assert_eq!(said_of(&mut events, &bob), vec![PeopleChange::Left]);
    }

    /// What already stands is no change: the role a person holds, the
    /// profile they have, an admission that says nothing new — nothing is
    /// written and nobody is told. Each used to write the file and tell
    /// everyone the directory. A re-statement that does move something says
    /// what moved, and never *joined* a second time.
    #[test]
    fn what_already_stands_is_no_change_and_a_restatement_says_what_moved() {
        let (_dir, ws) = ws();
        let bob = pk();
        ws.add_member(
            bob.clone(),
            MemberRole::Member,
            Admission {
                label: Some("Bob".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let mut events = ws.subscribe_store_events();
        let before = std::fs::metadata(ws.paths.members_file())
            .unwrap()
            .modified()
            .unwrap();
        let file_before = std::fs::read(ws.paths.members_file()).unwrap();

        assert_eq!(
            ws.set_role(&bob, MemberRole::Member).unwrap().role,
            MemberRole::Member
        );
        ws.set_member_profile(&bob, Some(Some(" Bob ".into())), None)
            .unwrap();
        ws.set_member_profile(&bob, None, Some(None)).unwrap();
        ws.add_member(bob.clone(), MemberRole::Member, Admission::default())
            .unwrap();
        assert!(said_of(&mut events, &bob).is_empty(), "nobody was told");
        assert_eq!(std::fs::read(ws.paths.members_file()).unwrap(), file_before);
        assert_eq!(
            std::fs::metadata(ws.paths.members_file())
                .unwrap()
                .modified()
                .unwrap(),
            before,
            "nothing was written"
        );

        ws.add_member(bob.clone(), MemberRole::Admin, Admission::default())
            .unwrap();
        ws.add_member(
            bob.clone(),
            MemberRole::Admin,
            Admission {
                label: Some("Robert".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            said_of(&mut events, &bob),
            vec![
                PeopleChange::RoleChanged {
                    role: MemberRole::Admin
                },
                PeopleChange::ProfileChanged
            ]
        );
    }

    /// A label given at an admission passes the one rule every label does:
    /// short, printable, or none.
    #[test]
    fn a_label_given_at_admission_passes_the_one_rule() {
        let (_dir, ws) = ws();
        let (bob, carol) = (pk(), pk());
        ws.add_member(
            bob.clone(),
            MemberRole::Guest,
            Admission {
                label: Some(format!("Bob\u{7}{}", "x".repeat(200))),
                ..Default::default()
            },
        )
        .unwrap();
        let label = ws.member(&bob).unwrap().unwrap().label.unwrap();
        assert_eq!(label.chars().count(), bisa_core::MAX_LABEL_CHARS);
        assert!(label.starts_with("Bobx"), "{label}");
        ws.add_member(
            carol.clone(),
            MemberRole::Guest,
            Admission {
                label: Some("   ".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(ws.member(&carol).unwrap().unwrap().label, None);
    }

    /// A person is a human on another node. One of this workspace's own
    /// agents is not one: admitted, it was relayed to as a person and
    /// listed with a face in everybody's directory.
    #[test]
    fn one_of_this_workspaces_own_agents_is_no_person() {
        let (_dir, ws) = ws();
        let agent = ws.get_agent(&bisa_core::AgentId::general()).unwrap().pubkey;
        for role in MemberRole::HOSTED {
            let refused = ws
                .add_member(agent.clone(), role, Admission::default())
                .unwrap_err();
            assert!(refused.to_string().contains("own agents"), "{refused}");
        }
        assert_eq!(ws.member(&agent).unwrap(), None);
        assert!(ws.people().unwrap().is_empty());
    }

    /// A writer that fell over while it held the people's lock does not stop
    /// the next: the files are whatever its last write left, and the next
    /// write reads them again.
    #[test]
    fn a_writer_that_fell_over_does_not_stop_the_next() {
        let (_dir, ws) = ws();
        let ws = std::sync::Arc::new(ws);
        let held = ws.clone();
        let panicked = std::thread::spawn(move || {
            let _writer = held.people_writer();
            panic!("a writer that fell over");
        })
        .join();
        assert!(panicked.is_err());
        let bob = pk();
        ws.add_member(bob.clone(), MemberRole::Guest, Admission::default())
            .unwrap();
        assert!(ws.is_member(&bob).unwrap());
    }
}
