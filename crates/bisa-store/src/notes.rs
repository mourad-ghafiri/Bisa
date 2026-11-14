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

/// What a [`Workspace::resync_notes`] found had moved under the store.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NotesResync {
    /// Notes that are new to the index or read differently than they did.
    pub changed: Vec<Note>,
    /// Notes the index held whose files are gone, with where they were.
    pub removed: Vec<(NoteId, OwnerScope)>,
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

    /// Write the file, then the row. What is written was checked by the
    /// caller, field by field: a note that arrived out of bounds (a pull, a
    /// hand edit) must still be writable where nobody touched the fault.
    fn write_note(&self, def: &Note) -> Result<(), StoreError> {
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
            title: Note::clean_title(&new.title),
            body: new.body,
            created_at: at,
            updated_at: at,
            pinned: false,
        };
        def.validate()?;
        self.write_note(&def)?;
        Ok(def)
    }

    /// Read one note by id. The index is the **locator** — which scope's
    /// directory the file is in — and the file is the truth.
    pub fn get_note(&self, id: NoteId) -> Result<Note, StoreError> {
        let scope = self.note_scope(id)?;
        self.read_note(&scope, id)
    }

    /// Where the index says a note is, whatever its file now holds — the one
    /// lookup a read and a deletion share, so a note nobody can read is still
    /// one that can be removed.
    fn note_scope(&self, id: NoteId) -> Result<OwnerScope, StoreError> {
        let (kind, scope_id) = self
            .idx()
            .note_scope(&id.to_string())?
            .ok_or_else(|| not_found(id))?;
        OwnerScope::from_parts(&kind, scope_id.as_deref())
            .ok_or_else(|| bad_scope_in_index(&kind, scope_id.as_deref()))
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
    /// A deletion — one note's, or a whole scope's — is a write like any
    /// other: outside the lock, a write that had already read the note would
    /// put the file and the row back after they went.
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
            Note::validate_body(body)?;
            def.body = body.clone();
        }
        if let Some(title) = patch.title {
            let title = Note::clean_title(&title);
            Note::validate_title(&title)?;
            def.title = title;
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

    /// Remove a note: the file and the row, together, under the one writer's
    /// lock. The index says where the file is and the file is never parsed:
    /// a note this build cannot read, and a row whose file a pull or a hand
    /// already removed, are both removed like any other — only a note with
    /// no row is *not found*.
    ///
    /// Answers the scope the note was in, so the deletion can be announced
    /// without the note having been read.
    pub fn delete_note(&self, id: NoteId) -> Result<OwnerScope, StoreError> {
        let _writer = self.note_writer();
        let scope = self.note_scope(id)?;
        // Its conversations first: a thread about a note that is gone has
        // nothing to be about, and nothing to default the note tools to.
        self.remove_conversations_of("note", &id.to_string())?;
        let path = self.note_path(&scope, id)?;
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(StoreError::io(path.display().to_string(), e)),
        }
        self.idx().delete_note(&id.to_string())?;
        Ok(scope)
    }

    /// A scope's notes directory and rows go together — the goal's, the
    /// project's, the workflow's or the channel's delete calls this, so the
    /// repository's next commit records the notes leaving with what they
    /// were about.
    pub(crate) fn remove_notes_of(&self, scope: OwnerScope, dir: &Path) -> Result<(), StoreError> {
        let _writer = self.note_writer();
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

    /// Make the index say what the notes folder holds, after files moved
    /// without the store hearing of it — a pull. Under the one writer's
    /// lock: every file is indexed again, a row whose file is gone is
    /// cleared with the conversations about it, and what read differently
    /// before and after is answered so each can be announced. A file nobody
    /// can read keeps its row — it is still there to delete.
    ///
    /// *Read differently* is judged by what the index remembers: the title,
    /// the pin and the time of the last write, which every write the
    /// platform makes moves. A body changed by hand under an unchanged
    /// `updated_at` is indexed all the same and announced by nobody.
    pub fn resync_notes(&self) -> Result<NotesResync, StoreError> {
        let _writer = self.note_writer();
        let rows = self.idx().notes_matching(None, None)?;
        let before = self.idx().note_stamps()?;
        let mut on_disk = std::collections::HashSet::new();
        self.reindex_notes_seen(&mut on_disk)?;
        let mut removed = Vec::new();
        for (raw, kind, scope_id) in rows {
            let (Ok(id), Some(scope)) = (
                raw.parse::<NoteId>(),
                OwnerScope::from_parts(&kind, scope_id.as_deref()),
            ) else {
                continue; // LCOV_EXCL_LINE: the store writes only note ids and the six scope kinds into the table (`check_constraints_mirror_the_core_enums`)
            };
            if on_disk.contains(&id) {
                continue;
            }
            self.remove_conversations_of("note", &raw)?;
            self.idx().delete_note(&raw)?;
            removed.push((id, scope));
        }
        let changed = self
            .list_notes(OwnerFilter::All)?
            .into_iter()
            .filter(|n| {
                before.get(&n.id.to_string()) != Some(&(n.title.clone(), n.updated_at, n.pinned))
            })
            .collect();
        Ok(NotesResync { changed, removed })
    }

    /// Rebuild the `notes` table by scanning every place a note can live.
    pub(crate) fn reindex_notes(&self) -> Result<(), StoreError> {
        self.reindex_notes_seen(&mut std::collections::HashSet::new())
    }

    /// [`Self::reindex_notes`], naming in `seen` every note file it walked
    /// past — the ones it could not read among them.
    fn reindex_notes_seen(
        &self,
        seen: &mut std::collections::HashSet<NoteId>,
    ) -> Result<(), StoreError> {
        let base = self.paths.notes_dir();
        let dir = |kind: &str, id: Option<&str>| Paths::scoped_dir(&base, kind, id);
        self.scan_notes_dir(&dir("workspace", None), OwnerScope::Workspace, seen)?;
        self.scan_notes_dir(&dir("node", None), OwnerScope::Node, seen)?;
        for id in self.goal_ids_on_disk()? {
            self.scan_notes_dir(
                &dir("goal", Some(&id.to_string())),
                OwnerScope::Goal { id },
                seen,
            )?;
        }
        for project in self.list_projects()? {
            self.scan_notes_dir(
                &dir("project", Some(project.slug.as_str())),
                OwnerScope::Project { id: project.id },
                seen,
            )?;
        }
        for workflow in self.list_workflows()? {
            self.scan_notes_dir(
                &dir("workflow", Some(&workflow.id.to_string())),
                OwnerScope::Workflow { id: workflow.id },
                seen,
            )?;
        }
        for channel in self.list_channels()? {
            self.scan_notes_dir(
                &dir("channel", Some(channel.id.as_str())),
                OwnerScope::Channel { id: channel.id },
                seen,
            )?;
        }
        Ok(())
    }

    fn scan_notes_dir(
        &self,
        dir: &Path,
        scope: OwnerScope,
        seen: &mut std::collections::HashSet<NoteId>,
    ) -> Result<(), StoreError> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Ok(());
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if stale_temporary(name, &entry) {
                // What a crash between a write and its rename left behind.
                if let Err(e) = std::fs::remove_file(entry.path()) {
                    tracing::warn!("{}: {e}", entry.path().display()); // LCOV_EXCL_LINE: a leftover the disk will not give up is said and walked past
                }
                continue;
            }
            let Some(id) = Paths::note_id_of_file(name) else {
                continue;
            };
            seen.insert(id);
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

/// How old a note's temporary must be before the rebuild removes it: far
/// longer than any write takes, so one still being written is never touched.
const STALE_TEMPORARY: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// Whether a file is a note's write that never finished — `<id>.md.tmp.…`,
/// the name [`crate::paths::write_atomic`] gives its temporary — and old
/// enough that no write in flight owns it.
fn stale_temporary(name: &str, entry: &std::fs::DirEntry) -> bool {
    let a_notes = name
        .split_once(".md.tmp.")
        .is_some_and(|(stem, rest)| stem.parse::<NoteId>().is_ok() && !rest.is_empty());
    a_notes
        && entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|at| at.elapsed().ok())
            .is_some_and(|age| age > STALE_TEMPORARY)
}

enum ReadError {
    Missing,
    Io(std::io::Error),
    Unreadable(bisa_core::NoteError),
}

// LCOV_EXCL_START: the cache's CHECK admits only the six scope kinds `from_parts` reads (`check_constraints_mirror_the_core_enums`)
fn bad_scope_in_index(kind: &str, scope_id: Option<&str>) -> StoreError {
    StoreError::Invalid(bisa_core::text!(
        "error-store-invalid-bad-note-scope-index",
        kind = kind.to_string(),
        scope_id = format!("{scope_id:?}")
    ))
}
// LCOV_EXCL_STOP

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

    /// A write a crash cut short leaves its temporary beside the note. The
    /// rebuild removes one old enough to be nobody's, and only that: a fresh
    /// one may be a write in flight, and a file that is no note's is not ours.
    #[test]
    fn the_rebuild_removes_a_notes_stale_temporary_and_nothing_else() {
        let (_d, ws) = ws();
        let note = ws
            .create_note(NewNote {
                scope: OwnerScope::Workspace,
                title: "Kept".into(),
                body: "words".into(),
            })
            .unwrap();
        let path = ws.note_path(&OwnerScope::Workspace, note.id).unwrap();
        let folder = path.parent().unwrap();
        let long_ago = std::time::SystemTime::now() - STALE_TEMPORARY * 2;
        let aged = |name: String| {
            let p = folder.join(name);
            std::fs::write(&p, b"half a note").unwrap();
            std::fs::File::options()
                .write(true)
                .open(&p)
                .unwrap()
                .set_modified(long_ago)
                .unwrap();
            p
        };
        let stale = aged(format!("{}.md.tmp.4242.7", note.id));
        let not_a_notes = aged("README.md.tmp.4242.7".into());
        let fresh = folder.join(format!("{}.md.tmp.4242.8", note.id));
        std::fs::write(&fresh, b"being written").unwrap();
        ws.rebuild_index().unwrap();
        assert!(!stale.exists(), "the stale temporary is removed");
        assert!(fresh.exists(), "a write in flight is left alone");
        assert!(not_a_notes.exists(), "a file that is no note's is not ours");
        assert_eq!(ws.list_notes(OwnerFilter::All).unwrap().len(), 1);
        assert_eq!(ws.get_note(note.id).unwrap().body, "words");
    }

    // added by the coverage pass: notes-s7.rs

    /// A note whose file is gone while its row stands is refused as not
    /// found by a read and skipped by a list; a deletion clears the row, so
    /// no ghost is left that no door can remove.
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
        ws.delete_note(gone.id).unwrap();
        assert!(matches!(
            ws.delete_note(gone.id),
            Err(StoreError::DefinitionNotFound { kind: "note", .. })
        ));
    }
}
