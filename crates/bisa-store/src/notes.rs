//! Notes: a markdown scratchpad, local to this machine. No kind, no wire —
//! a note's way off this machine is the **notes repository** the engine
//! keeps over `notes/` (ide/04 §The notes repository); this module only
//! writes the files it holds.
//!
//! The domain type is [`bisa_core::Note`]. Every note is a Markdown
//! file under the one root: `notes/workspace/<id>.md`,
//! `notes/goals/<id>/<id>.md`, `notes/projects/<slug>/<id>.md`,
//! `notes/workflows/<id>/<id>.md`, `notes/channels/<id>/<id>.md`,
//! `notes/node/<id>.md` — so one repository holds them all, deleting a goal,
//! a project, a workflow or a channel still takes its notes with it, and a
//! project's own tree is never written to.
//!
//! A listing is an [`OwnerFilter`]: every note, every note of one kind, or
//! one scope's — the three things the overlay's tabs ask for, answered by
//! the index in one query each rather than by the caller walking the kinds.
//!
//! Two writers on one note is the ordinary case (you in the editor, an agent
//! appending), so an edit is compare-and-swap on the body's hash and a
//! mismatch hands back the current value. Appends are unguarded by design:
//! nothing an agent does can overwrite what you wrote.

use crate::error::StoreError;
use crate::owner::OwnerFilter;
use crate::paths::Paths;
use crate::workspace::{mint_ulid, now_secs, Workspace};
use bisa_core::{Note, NoteId, OwnerScope, MAX_NOTE_BYTES};
use std::path::{Path, PathBuf};
use std::sync::MutexGuard;

/// The fields a caller supplies. `id`, timestamps and `pinned` are the store's.
#[derive(Clone, Debug)]
pub struct NewNote {
    pub scope: OwnerScope,
    pub title: String,
    pub body: String,
}

/// What an update may change.
#[derive(Clone, Debug, Default)]
pub struct NotePatch {
    pub title: Option<String>,
    pub body: Option<String>,
    pub pinned: Option<bool>,
}

/// The hash a caller passes back to prove what it was editing.
pub fn body_hash(body: &str) -> String {
    crate::recall::sha256_hex(body.as_bytes())
}

impl Workspace {
    /// The directory a scope's notes sit in. Checks the scope exists.
    fn notes_dir_for(&self, scope: &OwnerScope) -> Result<PathBuf, StoreError> {
        self.owner_dir(&self.paths.notes_dir(), scope)
    }

    fn note_path(&self, scope: &OwnerScope, id: NoteId) -> Result<PathBuf, StoreError> {
        Ok(Paths::note_file_in(&self.notes_dir_for(scope)?, id))
    }

    fn write_note(&self, def: &Note) -> Result<(), StoreError> {
        def.validate()?;
        let path = self.note_path(&def.scope, def.id)?;
        crate::paths::write_atomic(&path, def.to_markdown().as_bytes())?;
        self.index_note(def)
    }

    fn index_note(&self, def: &Note) -> Result<(), StoreError> {
        let home_goal = match &def.scope {
            OwnerScope::Goal { id } => Some(id.to_string()),
            _ => None,
        };
        self.idx().upsert_note(
            &def.id.to_string(),
            def.scope.kind(),
            def.scope.id().as_deref(),
            home_goal.as_deref(),
            &def.title,
            def.updated_at,
            def.pinned,
        )
    }

    pub fn create_note(&self, new: NewNote) -> Result<Note, StoreError> {
        let at = now_secs();
        let def = Note {
            id: NoteId::from_ulid(mint_ulid()),
            scope: new.scope,
            title: new.title.trim().to_string(),
            body: new.body,
            created_at: at,
            updated_at: at,
            pinned: false,
        };
        self.write_note(&def)?;
        Ok(def)
    }

    /// Read one note by id. The index is the **locator** — which scope's
    /// directory the file is in — and the file is the truth.
    pub fn get_note(&self, id: NoteId) -> Result<Note, StoreError> {
        let (kind, scope_id) = self
            .idx()
            .note_scope(&id.to_string())?
            .ok_or_else(|| not_found(id))?;
        let scope = OwnerScope::from_parts(&kind, scope_id.as_deref()).ok_or_else(|| {
            // LCOV_EXCL_START: the cache's CHECK admits only the six scope kinds `from_parts` reads (`check_constraints_mirror_the_core_enums`)
            StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-bad-note-scope-index",
                kind = kind.to_string(),
                scope_id = format!("{scope_id:?}")
            ))
            // LCOV_EXCL_STOP
        })?;
        self.read_note(&scope, id)
    }

    fn read_note(&self, scope: &OwnerScope, id: NoteId) -> Result<Note, StoreError> {
        let path = self.note_path(scope, id)?;
        read_note_file(&path, id, scope.clone()).map_err(|e| match e {
            ReadError::Missing => not_found(id),
            ReadError::Io(e) => StoreError::io(path.display().to_string(), e),
            ReadError::Unreadable(e) => StoreError::unreadable(&path, "note", e),
        })
    }

    /// The notes a filter admits, pinned first and newest first within that.
    ///
    /// The index says which rows and where each file is; every file is then
    /// read whole, because a listing carries bodies (the overlay searches
    /// them without a second round trip). A file the index names but the
    /// disk no longer has is logged and skipped rather than failing the list.
    pub fn list_notes(&self, filter: OwnerFilter) -> Result<Vec<Note>, StoreError> {
        let (kind, scope_id) = filter.parts();
        let rows = self.idx().notes_matching(kind, scope_id.as_deref())?;
        let mut out = Vec::with_capacity(rows.len());
        for (id, kind, scope_id) in rows {
            let Ok(note_id) = id.parse::<NoteId>() else {
                continue;
            };
            let Some(scope) = OwnerScope::from_parts(&kind, scope_id.as_deref()) else {
                // LCOV_EXCL_START: the cache's CHECK admits only the six scope kinds `from_parts` reads (`check_constraints_mirror_the_core_enums`)
                tracing::warn!("note {note_id}: bad scope in index: {kind} {scope_id:?}");
                continue;
                // LCOV_EXCL_STOP
            };
            match self.read_note(&scope, note_id) {
                Ok(def) => out.push(def),
                Err(e) => tracing::warn!("note {note_id}: {e}"),
            }
        }
        out.sort_by(|a, b| {
            b.pinned
                .cmp(&a.pinned)
                .then(b.updated_at.cmp(&a.updated_at))
                .then(b.id.cmp(&a.id))
        });
        Ok(out)
    }

    /// The one writer of notes at a time (`notes_writes`): the person's
    /// editor and an agent's tool each read, check the hash and write, and
    /// two at once must not each write the body as it was before the other.
    /// A poisoned lock is taken over, as the work items' is: the note on
    /// disk is whatever the last write left, and the next write reads it.
    fn note_writer(&self) -> MutexGuard<'_, ()> {
        self.notes_writes.lock().unwrap_or_else(|poisoned| {
            tracing::warn!(
                "the notes write lock was poisoned; continuing with the note as it stands"
            );
            poisoned.into_inner()
        })
    }

    /// Edit a note, refusing if it changed under you. `base_hash` is
    /// [`body_hash`] of the body the caller last read; `None` is accepted only
    /// when the body is not being changed. Read, checked and written under
    /// the one writer's lock.
    pub fn update_note(
        &self,
        id: NoteId,
        patch: NotePatch,
        base_hash: Option<&str>,
    ) -> Result<Note, StoreError> {
        let _writer = self.note_writer();
        let mut def = self.get_note(id)?;
        if let Some(body) = &patch.body {
            let current = body_hash(&def.body);
            match base_hash {
                Some(h) if h == current => {}
                _ => {
                    return Err(StoreError::EditConflict {
                        what: "note",
                        current: serde_json::Value::String(def.body),
                        current_hash: current,
                    })
                }
            }
            def.body = body.clone();
        }
        if let Some(title) = patch.title {
            def.title = title.trim().to_string();
        }
        if let Some(pinned) = patch.pinned {
            def.pinned = pinned;
        }
        def.updated_at = now_secs();
        self.write_note(&def)?;
        Ok(def)
    }

    /// Add a block to the end of a note. Append-only and unguarded by a
    /// hash — it adds to whatever is there — but under the one writer's lock,
    /// so a block never lands on a body another write is replacing.
    pub fn append_note(&self, id: NoteId, block: &str) -> Result<Note, StoreError> {
        let _writer = self.note_writer();
        let mut def = self.get_note(id)?;
        let block = block.trim_end();
        if block.trim().is_empty() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-nothing-append"
            )));
        }
        if def.body.trim().is_empty() {
            def.body = block.to_string();
        } else {
            def.body = format!("{}\n\n{block}", def.body.trim_end());
        }
        if def.body.len() > MAX_NOTE_BYTES {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-note-would-be-bytes-limit",
                a0 = (def.body.len()).to_string(),
                max_note_bytes = (MAX_NOTE_BYTES).to_string()
            )));
        }
        def.updated_at = now_secs();
        self.write_note(&def)?;
        Ok(def)
    }

    /// Remove a note: the file and the row, together.
    pub fn delete_note(&self, id: NoteId) -> Result<(), StoreError> {
        let def = self.get_note(id)?;
        // Its conversations first: a thread about a note that is gone has
        // nothing to be about, and nothing to default the note tools to.
        self.remove_conversations_of("note", &id.to_string())?;
        let path = self.note_path(&def.scope, id)?;
        if let Err(e) = std::fs::remove_file(&path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(StoreError::io(path.display().to_string(), e));
            } // LCOV_EXCL_LINE: `get_note` above read the file; gone only under a concurrent delete
        }
        self.idx().delete_note(&id.to_string())
    }

    /// A scope's notes directory and rows go together — the goal's, the
    /// project's, the workflow's or the channel's delete calls this, so the
    /// repository's next commit records the notes leaving with what they
    /// were about.
    pub(crate) fn remove_notes_of(&self, scope: OwnerScope, dir: &Path) -> Result<(), StoreError> {
        match std::fs::remove_dir_all(dir) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(StoreError::io(dir.display().to_string(), e)),
        }
        let rows = self
            .idx()
            .notes_matching(Some(scope.kind()), scope.id().as_deref())?;
        for (id, _, _) in &rows {
            self.remove_conversations_of("note", id)?;
        }
        let idx = self.idx();
        idx.in_transaction(|| {
            for (id, _, _) in &rows {
                idx.delete_note(id)?;
            }
            Ok(())
        })
    }

    /// Rebuild the `notes` table by scanning every place a note can live.
    pub(crate) fn reindex_notes(&self) -> Result<(), StoreError> {
        let base = self.paths.notes_dir();
        let dir = |kind: &str, id: Option<&str>| Paths::scoped_dir(&base, kind, id);
        self.scan_notes_dir(&dir("workspace", None), OwnerScope::Workspace)?;
        self.scan_notes_dir(&dir("node", None), OwnerScope::Node)?;
        for id in self.goal_ids_on_disk()? {
            self.scan_notes_dir(&dir("goal", Some(&id.to_string())), OwnerScope::Goal { id })?;
        }
        for project in self.list_projects()? {
            self.scan_notes_dir(
                &dir("project", Some(project.slug.as_str())),
                OwnerScope::Project { id: project.id },
            )?;
        }
        for workflow in self.list_workflows()? {
            self.scan_notes_dir(
                &dir("workflow", Some(&workflow.id.to_string())),
                OwnerScope::Workflow { id: workflow.id },
            )?;
        }
        for channel in self.list_channels()? {
            self.scan_notes_dir(
                &dir("channel", Some(channel.id.as_str())),
                OwnerScope::Channel { id: channel.id },
            )?;
        }
        Ok(())
    }

    fn scan_notes_dir(&self, dir: &Path, scope: OwnerScope) -> Result<(), StoreError> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Ok(());
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let Some(id) = Paths::note_id_of_file(name) else {
                continue;
            };
            let path = entry.path();
            // A note this build cannot read — a merge left in it, a hand edit
            // — is named and skipped; the rebuild goes on.
            let read = read_note_file(&path, id, scope.clone()).map_err(|e| match e {
                ReadError::Missing | ReadError::Io(_) => StoreError::io(
                    path.display().to_string(),
                    std::io::Error::other("unreadable"),
                ),
                ReadError::Unreadable(e) => StoreError::unreadable(&path, "note", e),
            });
            let Some(def) = self.tolerated_record("note", &path.display().to_string(), read)?
            else {
                continue;
            };
            // A goal note whose goal is gone cannot be indexed (its home_goal
            // row would dangle); the file is an orphan.
            if let OwnerScope::Goal { id } = &def.scope {
                if !self.idx().goal_exists(&id.to_string())? {
                    continue;
                }
            }
            self.index_note(&def)?;
        }
        Ok(())
    }
}

enum ReadError {
    Missing,
    Io(std::io::Error),
    Unreadable(bisa_core::NoteError),
}

/// One file as a note: the id and scope from where it is, the rest from it.
/// A note that is not here is *not found* — what a drawer standing on it,
/// and a conversation asked to be about it, read as gone — never a request
/// that was wrong.
fn not_found(id: NoteId) -> StoreError {
    StoreError::DefinitionNotFound {
        kind: "note",
        id: id.to_string(),
    }
}

fn read_note_file(path: &Path, id: NoteId, scope: OwnerScope) -> Result<Note, ReadError> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ReadError::Missing
        } else {
            ReadError::Io(e)
        }
    })?;
    Note::from_markdown(id, scope, &text).map_err(ReadError::Unreadable)
}

// added by the coverage pass: notes_mod.rs
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

    // --- the notes module's remaining arms ---

    /// A note whose file is gone, nobody may read, or is no note: each is
    /// refused in its own words on a read and skipped, said, by a list. A
    /// row that names no note is skipped. The writer recovers from poison.
    #[test]
    fn a_notes_file_that_is_gone_unreadable_or_no_note_is_refused_by_name_and_skipped_by_a_list() {
        let (_d, ws) = ws();
        let note = ws
            .create_note(NewNote {
                scope: OwnerScope::Workspace,
                title: "Kept".into(),
                body: "words".into(),
            })
            .unwrap();
        let odd = ws
            .create_note(NewNote {
                scope: OwnerScope::Workspace,
                title: "Odd".into(),
                body: "words".into(),
            })
            .unwrap();
        let path = ws.note_path(&OwnerScope::Workspace, note.id).unwrap();
        std::fs::write(&path, b"no front matter here").unwrap();
        let err = ws.get_note(note.id).unwrap_err();
        assert!(
            matches!(&err, StoreError::Unreadable { what, .. } if *what == "note"),
            "{err:?}"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
            let unreadable = ws.get_note(note.id);
            let listed = ws.list_notes(OwnerFilter::All);
            let rebuilt = ws.rebuild_index();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(
                matches!(unreadable, Err(StoreError::Io { .. })),
                "{unreadable:?}"
            );
            let listed = listed.unwrap();
            assert_eq!(
                listed.len(),
                1,
                "the list skips what it cannot read: {listed:?}"
            );
            assert!(matches!(rebuilt, Err(StoreError::Io { .. })), "{rebuilt:?}");
            // The rebuild that stopped left the cache half-built, as a crash
            // would: the next one, with the file readable again, completes it.
            ws.rebuild_index().unwrap();
        }
        std::fs::remove_file(&path).unwrap();
        assert!(matches!(
            ws.get_note(note.id),
            Err(StoreError::DefinitionNotFound { kind: "note", .. })
        ));
        assert_eq!(ws.list_notes(OwnerFilter::All).unwrap().len(), 1);
        ws.idx()
            .execute_for_test(&format!(
                "UPDATE notes SET id = 'not-a-note' WHERE id = '{}'",
                odd.id
            ))
            .unwrap();
        assert!(ws.list_notes(OwnerFilter::All).unwrap().is_empty());
        ws.rebuild_index().unwrap();
        let ws = std::sync::Arc::new(ws);
        let poisoner = std::sync::Arc::clone(&ws);
        let poisoned = std::thread::spawn(move || {
            let _writer = poisoner.note_writer();
            panic!("poison the notes writer on purpose");
        })
        .join();
        assert!(poisoned.is_err());
        let renamed = ws
            .update_note(
                odd.id,
                NotePatch {
                    title: Some("Odd still".into()),
                    ..Default::default()
                },
                None,
            )
            .unwrap();
        assert_eq!(renamed.title, "Odd still");
    }

    /// What the rebuild walks past in a notes folder: a stray file, a goal
    /// note whose goal cannot be indexed; what a deletion refuses: a file
    /// or a folder nobody may remove.
    #[test]
    fn the_rebuild_walks_past_strays_and_orphans_and_a_deletion_names_what_it_cannot_remove() {
        let (_d, ws) = ws();
        let goal = ws
            .create_goal(crate::workspace::NewGoal::captured("noted"))
            .unwrap();
        let of_goal = ws
            .create_note(NewNote {
                scope: OwnerScope::Goal { id: goal.id },
                title: "Of the goal".into(),
                body: "words".into(),
            })
            .unwrap();
        let folder = ws
            .note_path(&OwnerScope::Goal { id: goal.id }, of_goal.id)
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        std::fs::write(folder.join("README.md"), b"not a note").unwrap();
        ws.rebuild_index().unwrap();
        assert_eq!(ws.list_notes(OwnerFilter::All).unwrap().len(), 1);
        // The goal's snapshot torn: the goal has no row, so its note is an
        // orphan the rebuild leaves on disk and out of the cache.
        let snapshot = ws.paths.state_dir(&Paths::ns_goal(goal.id)).join(format!(
            "{}-{}.json",
            bisa_core::kind::KIND_GOAL,
            goal.id
        ));
        let kept = std::fs::read(&snapshot).unwrap();
        std::fs::write(&snapshot, b"{torn").unwrap();
        ws.rebuild_index().unwrap();
        assert!(ws.list_notes(OwnerFilter::All).unwrap().is_empty());
        std::fs::write(&snapshot, kept).unwrap();
        ws.rebuild_index().unwrap();
        assert_eq!(ws.list_notes(OwnerFilter::All).unwrap().len(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o500)).unwrap();
            let unremovable = ws.delete_note(of_goal.id);
            let parent = folder.parent().unwrap().to_path_buf();
            std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
            std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o500)).unwrap();
            let unremovable_folder = ws.remove_notes_of(OwnerScope::Goal { id: goal.id }, &folder);
            std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert!(
                matches!(unremovable, Err(StoreError::Io { .. })),
                "{unremovable:?}"
            );
            assert!(
                matches!(unremovable_folder, Err(StoreError::Io { .. })),
                "{unremovable_folder:?}"
            );
        }
    }

    // added by the coverage pass: notes-s7.rs

    /// A note whose file is gone while its row stands is refused as not
    /// found — by a read and by a deletion alike — and skipped by a list.
    #[test]
    fn a_note_whose_file_is_gone_is_not_found_and_skipped_by_a_list() {
        let (_d, ws) = ws();
        let gone = ws
            .create_note(NewNote {
                scope: OwnerScope::Workspace,
                title: "Gone".into(),
                body: "words".into(),
            })
            .unwrap();
        let kept = ws
            .create_note(NewNote {
                scope: OwnerScope::Workspace,
                title: "Kept".into(),
                body: "words".into(),
            })
            .unwrap();
        std::fs::remove_file(ws.note_path(&OwnerScope::Workspace, gone.id).unwrap()).unwrap();
        assert!(matches!(
            ws.get_note(gone.id),
            Err(StoreError::DefinitionNotFound { kind: "note", .. })
        ));
        assert_eq!(
            ws.list_notes(OwnerFilter::All)
                .unwrap()
                .iter()
                .map(|n| n.id)
                .collect::<Vec<_>>(),
            vec![kept.id]
        );
        assert!(matches!(
            ws.delete_note(gone.id),
            Err(StoreError::DefinitionNotFound { kind: "note", .. })
        ));
    }
}
