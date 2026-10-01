//! Review notes: annotations on diff lines, pinned to the diff they were
//! written against, handed to agents as tool results.
//!
//! Local state, no kind. Truth: `projects/<slug>/review/<NoteId>.json` — a
//! workspace directory even for an adopted root, so notes survive a detach and
//! never land in somebody's repository.

use crate::error::StoreError;
use crate::workspace::{mint_ulid, now_secs, Workspace};
use bisa_core::{
    DiffScope, LineRange, NoteId, ProjectId, RelPath, ReviewNote, Sha256, WorkstreamId,
};
use std::path::PathBuf;

/// The fields a person supplies when annotating a hunk.
#[derive(Clone, Debug)]
pub struct NewReviewNote {
    pub project: ProjectId,
    pub workstream: Option<WorkstreamId>,
    pub path: RelPath,
    pub range: LineRange,
    pub scope: DiffScope,
    pub diff_identity: Sha256,
    /// The hunk text, already bounded by the caller.
    pub hunk: String,
    pub body: String,
}

impl Workspace {
    fn review_dir(&self, project: ProjectId) -> Result<PathBuf, StoreError> {
        let p = self.get_project(project)?;
        Ok(self.paths.project(&p.slug).review())
    }

    fn review_path(&self, project: ProjectId, id: NoteId) -> Result<PathBuf, StoreError> {
        Ok(self.review_dir(project)?.join(format!("{id}.json")))
    }

    fn write_review_note(&self, note: &ReviewNote) -> Result<(), StoreError> {
        note.validate()?;
        let path = self.review_path(note.project, note.id)?;
        crate::paths::write_atomic(&path, &serde_json::to_vec_pretty(note)?)
    }

    pub fn create_review_note(&self, new: NewReviewNote) -> Result<ReviewNote, StoreError> {
        if let Some(ws) = new.workstream {
            let w = self.get_workstream(ws)?;
            if w.project != new.project {
                return Err(StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-workstream-belongs-another-project",
                    ws = ws.to_string()
                )));
            }
        }
        let at = now_secs();
        let note = ReviewNote {
            id: NoteId::from_ulid(mint_ulid()),
            project: new.project,
            workstream: new.workstream,
            path: new.path,
            range: new.range,
            scope: new.scope,
            diff_identity: new.diff_identity,
            hunk: new.hunk,
            body: new.body,
            sent_at: None,
            resolved_at: None,
            created_at: at,
            updated_at: at,
        };
        self.write_review_note(&note)?;
        Ok(note)
    }

    pub fn get_review_note(
        &self,
        project: ProjectId,
        id: NoteId,
    ) -> Result<ReviewNote, StoreError> {
        let path = self.review_path(project, id)?;
        let bytes = std::fs::read(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StoreError::DefinitionNotFound {
                    kind: "review note",
                    id: id.to_string(),
                }
            } else {
                StoreError::io(path.display().to_string(), e)
            }
        })?;
        serde_json::from_slice(&bytes).map_err(|e| StoreError::unreadable(&path, "review note", e))
    }

    /// Every review note of a project, optionally only those on one workstream,
    /// oldest first.
    pub fn list_review_notes(
        &self,
        project: ProjectId,
        workstream: Option<WorkstreamId>,
    ) -> Result<Vec<ReviewNote>, StoreError> {
        let dir = self.review_dir(project)?;
        let mut out = Vec::new();
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(StoreError::io(dir.display().to_string(), e)),
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.ends_with(".json") {
                continue;
            }
            let path = entry.path();
            let bytes =
                std::fs::read(&path).map_err(|e| StoreError::io(path.display().to_string(), e))?;
            let n: ReviewNote = serde_json::from_slice(&bytes)
                .map_err(|e| StoreError::unreadable(&path, "review note", e))?;
            if workstream.is_none() || n.workstream == workstream {
                out.push(n);
            }
        }
        out.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));
        Ok(out)
    }

    /// Edit the body. Clears the sent mark: the agent has not seen this version.
    pub fn edit_review_note(
        &self,
        project: ProjectId,
        id: NoteId,
        body: String,
    ) -> Result<ReviewNote, StoreError> {
        let note = self.get_review_note(project, id)?.edited(body, now_secs());
        self.write_review_note(&note)?;
        Ok(note)
    }

    /// Mark the note as handed to an agent.
    pub fn mark_review_note_sent(
        &self,
        project: ProjectId,
        id: NoteId,
    ) -> Result<ReviewNote, StoreError> {
        let mut note = self.get_review_note(project, id)?;
        note.sent_at = Some(now_secs());
        self.write_review_note(&note)?;
        Ok(note)
    }

    /// Mark the note dealt with. It stays on disk; `list_review_notes` still
    /// returns it, so callers filter with [`ReviewNote::is_resolved`].
    pub fn resolve_review_note(
        &self,
        project: ProjectId,
        id: NoteId,
    ) -> Result<ReviewNote, StoreError> {
        let note = self.get_review_note(project, id)?.resolved(now_secs());
        self.write_review_note(&note)?;
        Ok(note)
    }

    pub fn delete_review_note(&self, project: ProjectId, id: NoteId) -> Result<(), StoreError> {
        let path = self.review_path(project, id)?;
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(StoreError::io(path.display().to_string(), e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::MemoryKeyStore;
    use crate::projects::NewProject;

    #[test]
    fn review_notes_live_under_the_project_and_forget_being_sent_when_edited() {
        let dir = tempfile::tempdir().unwrap();
        let ws =
            Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
        let p = ws
            .create_project(NewProject::managed("web").unwrap())
            .unwrap();
        let n = ws
            .create_review_note(NewReviewNote {
                project: p.id,
                workstream: None,
                path: RelPath::new("src/main.rs").unwrap(),
                range: LineRange::new(10, 12).unwrap(),
                scope: DiffScope::Unstaged,
                diff_identity: Sha256::new("a".repeat(64)).unwrap(),
                hunk: String::new(),
                body: "rename this".into(),
            })
            .unwrap();
        assert!(ws
            .review_path(p.id, n.id)
            .unwrap()
            .starts_with(ws.project_paths(&p).review()));
        let sent = ws.mark_review_note_sent(p.id, n.id).unwrap();
        assert!(sent.sent_at.is_some());
        let edited = ws
            .edit_review_note(p.id, n.id, "rename it properly".into())
            .unwrap();
        assert_eq!(edited.sent_at, None);
        assert!(ws.edit_review_note(p.id, n.id, "  ".into()).is_err());
        assert_eq!(ws.list_review_notes(p.id, None).unwrap().len(), 1);
        ws.delete_review_note(p.id, n.id).unwrap();
        assert!(ws.list_review_notes(p.id, None).unwrap().is_empty());
    }
}
