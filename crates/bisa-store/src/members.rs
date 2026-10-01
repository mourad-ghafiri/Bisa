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
use crate::workspace::{now_secs, Workspace};
use bisa_core::{check_member_change, AttachmentRef, MemberRole, PrincipalId, WorkspaceMember};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default)]
struct MemberFile {
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

    /// Called at `open()`: the workspace owner identity is always a member.
    pub(crate) fn ensure_owner_member(&self) -> Result<(), StoreError> {
        let owner = self.owner_principal();
        let mut file = self.read_member_file()?;
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
        for m in &file.members {
            self.index_member(m)?;
        }
        Ok(())
    }

    /// Admit (or re-state) a hosted member at a role. Idempotent; the owner's
    /// row is immutable and nobody else may be made owner. A row already
    /// there keeps who invited it and what it said it was unless the
    /// admission says otherwise.
    pub fn add_member(
        &self,
        pubkey: PrincipalId,
        role: MemberRole,
        admission: Admission,
    ) -> Result<(), StoreError> {
        let mut file = self.read_member_file()?;
        check_member_change(&file.members, &pubkey, Some(role))?;
        if role == MemberRole::Owner && pubkey != self.owner_principal() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-workspace-has-exactly-one-owner-own-keypair"
            )));
        }
        match file.members.iter_mut().find(|m| m.pubkey == pubkey) {
            Some(existing) => {
                existing.role = role;
                existing.label = admission.label.or(existing.label.take());
                existing.photo = admission.photo.or(existing.photo.take());
                existing.invited_by = admission.invited_by.or(existing.invited_by.take());
                existing.client = admission.client.or(existing.client.take());
            }
            None => file.members.push(WorkspaceMember {
                pubkey: pubkey.clone(),
                role,
                label: admission.label,
                photo: admission.photo,
                invited_by: admission.invited_by,
                client: admission.client,
                added_at: now_secs(),
            }),
        }
        self.write_member_file(&file)?;
        if let Some(row) = file.members.iter().find(|m| m.pubkey == pubkey) {
            self.index_member(row)?;
        }
        if role.is_hosted() {
            self.emit_store_event(crate::workspace::StoreEvent::PeopleChanged {
                pubkey,
                change: crate::workspace::PeopleChange::Joined { role },
            });
        }
        Ok(())
    }

    /// Change a hosted member's role. The owner's cannot change; a role is
    /// never the owner's; an unknown person is a refusal, not a creation.
    pub fn set_role(
        &self,
        pubkey: &PrincipalId,
        role: MemberRole,
    ) -> Result<WorkspaceMember, StoreError> {
        let mut file = self.read_member_file()?;
        check_member_change(&file.members, pubkey, Some(role))?;
        if role == MemberRole::Owner {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-workspace-has-exactly-one-owner-own-keypair"
            )));
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
        row.role = role;
        let row = row.clone();
        self.write_member_file(&file)?;
        self.index_member(&row)?;
        self.emit_store_event(crate::workspace::StoreEvent::PeopleChanged {
            pubkey: pubkey.clone(),
            change: crate::workspace::PeopleChange::RoleChanged { role },
        });
        Ok(row)
    }

    /// A member's profile — their label, their face — as they set it: on the
    /// host, a hosted member's row when their node says so; on their own
    /// node, the owner's row. Each part is kept when `None` and cleared when
    /// `Some(None)`. One write, one `ProfileChanged` — the owner's row too,
    /// since the pump carries the owner's profile to every host it is a
    /// guest of. An unknown person is a refusal, not a creation; the role
    /// never moves here.
    pub fn set_member_profile(
        &self,
        pubkey: &PrincipalId,
        label: Option<Option<String>>,
        photo: Option<Option<AttachmentRef>>,
    ) -> Result<WorkspaceMember, StoreError> {
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
        if let Some(label) = label {
            row.label = bisa_core::clean_label(label);
        }
        if let Some(photo) = photo {
            row.photo = photo;
        }
        let row = row.clone();
        self.write_member_file(&file)?;
        self.index_member(&row)?;
        self.emit_store_event(crate::workspace::StoreEvent::PeopleChanged {
            pubkey: pubkey.clone(),
            change: crate::workspace::PeopleChange::ProfileChanged,
        });
        Ok(row)
    }

    /// Remove a member. The workspace owner can never be removed.
    pub fn remove_member(&self, pubkey: &PrincipalId) -> Result<(), StoreError> {
        let mut file = self.read_member_file()?;
        check_member_change(&file.members, pubkey, None)?;
        let was = file.members.len();
        file.members.retain(|m| m.pubkey != *pubkey);
        self.write_member_file(&file)?;
        self.idx().delete_member(pubkey.as_hex())?;
        if file.members.len() != was {
            self.emit_store_event(crate::workspace::StoreEvent::PeopleChanged {
                pubkey: pubkey.clone(),
                change: crate::workspace::PeopleChange::Left,
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
        for m in &self.read_member_file()?.members {
            self.index_member(m)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::MemoryKeyStore;
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
        assert!(ws.remove_member(&owner).is_err());
        for role in MemberRole::HOSTED {
            assert!(
                ws.add_member(owner.clone(), role, Admission::default())
                    .is_err(),
                "the owner cannot be demoted to {role}"
            );
            assert!(ws.set_role(&owner, role).is_err());
        }
        assert!(
            ws.add_member(pk(), MemberRole::Owner, Admission::default())
                .is_err(),
            "one owner"
        );
        assert!(ws.people().unwrap().is_empty());
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
        assert!(
            ws.set_role(&pk(), MemberRole::Guest).is_err(),
            "not a member"
        );
        assert!(ws.set_role(&bob, MemberRole::Owner).is_err());

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
        assert!(ws
            .set_member_profile(&pk(), Some(Some("x".into())), None)
            .is_err());
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
}
