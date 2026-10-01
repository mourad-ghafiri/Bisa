//! A goal's **documents**: the files a person gave it as context — a brief,
//! a spec, a screenshot — kept under `goals/<GoalId>/documents/`.
//!
//! The record is the journal: one `document` fact per file, carrying the
//! attachment descriptor (the name the person gave, the hash, the type, the
//! size). The bytes are the attachment store's, content-addressed and
//! fetched by hash between peers. The folder is **derived** from the facts
//! wherever the bytes are held — materialised by hard link, else copy, like
//! an attachment's named copy — so a rebuild, or a peer that has fetched the
//! bytes, fills it in again from the same rule.
//!
//! **Names are decided by the facts, in journal order.** Two documents given
//! the same name are `brief.pdf` and `brief (2).pdf`, whichever machine
//! materialises them; nothing is decided by what happens to be on disk.

use crate::error::StoreError;
use crate::paths::sanitise_file_name;
use crate::workspace::Workspace;
use bisa_core::event::JournalPayload;
use bisa_core::{AttachmentRef, GoalId};
use std::collections::HashSet;
use std::path::PathBuf;

/// One of a goal's documents: the descriptor as given, where the file is
/// (or would be), and whether the bytes are here.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct GoalDocument {
    pub file: AttachmentRef,
    /// The name it is materialised under — the given one, numbered when taken.
    pub name: String,
    pub path: PathBuf,
    pub present: bool,
}

/// The name a document takes among those before it: the given name made
/// safe (a name nothing safe is left of becomes the hash's first twelve
/// characters), numbered before the extension when taken — `brief (2).pdf`.
pub fn distinct_name(taken: &HashSet<String>, file: &AttachmentRef) -> String {
    // Twelve *characters*: the hash is a peer's word until `is_valid_hash`
    // said so, and a short or multibyte one is never sliced into a panic.
    let base = sanitise_file_name(&file.name)
        .unwrap_or_else(|| file.sha256.chars().take(12).collect::<String>());
    if !taken.contains(&base) {
        return base;
    }
    let (stem, ext) = match base.rfind('.') {
        Some(at) if at > 0 => (&base[..at], &base[at..]),
        _ => (base.as_str(), ""),
    };
    (2..)
        .map(|n| format!("{stem} ({n}){ext}"))
        .find(|candidate| !taken.contains(candidate))
        .expect("an unbounded counter finds a free name")
}

/// The documents the journal names, in order, each with its settled name.
fn named(facts: impl IntoIterator<Item = AttachmentRef>) -> Vec<(AttachmentRef, String)> {
    let mut taken = HashSet::new();
    facts
        .into_iter()
        .map(|file| {
            let name = distinct_name(&taken, &file);
            taken.insert(name.clone());
            (file, name)
        })
        .collect()
}

impl Workspace {
    /// The `document` facts of a goal, in journal order. A fact whose hash
    /// is not one (a peer's malformed word) names no bytes anywhere and is
    /// left out, said in the log — never a name, a path or a panic.
    fn document_facts(&self, goal: GoalId) -> Result<Vec<AttachmentRef>, StoreError> {
        Ok(self
            .journal(&bisa_core::Home::Goal { goal })?
            .into_iter()
            .filter_map(|e| match e.payload {
                JournalPayload::Document { file } => Some(file),
                _ => None,
            })
            .filter(|file| {
                let valid = AttachmentRef::is_valid_hash(&file.sha256);
                if !valid {
                    tracing::warn!(goal = %goal, name = %file.name, "a document fact carries no sha256 hash; left out");
                }
                valid
            })
            .collect())
    }

    /// Give a goal a document: the bytes must already be here (an upload
    /// through the attachment store), the fact is appended as the owner,
    /// and the file is materialised under `documents/` at once. Answers the
    /// document as it now stands.
    pub fn add_goal_document(
        &self,
        goal: GoalId,
        file: &AttachmentRef,
    ) -> Result<GoalDocument, StoreError> {
        self.get_goal(goal)?;
        if self.attachment_path(&file.sha256).is_none() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-no-attachment-workspace-upload-bytes-first",
                a0 = (file.sha256).to_string()
            )));
        }
        if file.name.trim().is_empty() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-document-needs-name"
            )));
        }
        let taken: HashSet<String> = named(self.document_facts(goal)?)
            .into_iter()
            .map(|(_, n)| n)
            .collect();
        let name = distinct_name(&taken, file);
        let keys = self.owner_keys().clone();
        self.append_journal(
            &bisa_core::Home::Goal { goal },
            JournalPayload::Document { file: file.clone() },
            &keys,
            None,
        )?;
        let path = self.materialise_document(goal, &file.sha256, &name)?;
        Ok(GoalDocument {
            file: file.clone(),
            name,
            path,
            present: true,
        })
    }

    /// A goal's documents, from the facts: each with its settled name,
    /// its path and whether the bytes — and so the file — are here.
    pub fn goal_documents(&self, goal: GoalId) -> Result<Vec<GoalDocument>, StoreError> {
        self.get_goal(goal)?;
        let paths = self.paths().goal(goal);
        Ok(named(self.document_facts(goal)?)
            .into_iter()
            .map(|(file, name)| {
                let path = paths.document(&name);
                let present = path.is_file();
                GoalDocument {
                    file,
                    name,
                    path,
                    present,
                }
            })
            .collect())
    }

    /// The folder brought up to the facts: every document whose bytes are
    /// here is materialised; one whose bytes are not is left for the next
    /// pass, once they arrive. Idempotent — the rebuild's and the ingest's
    /// one call.
    pub(crate) fn materialise_goal_documents(&self, goal: GoalId) -> Result<(), StoreError> {
        for (file, name) in named(self.document_facts(goal)?) {
            if self.attachment_path(&file.sha256).is_some() {
                self.materialise_document(goal, &file.sha256, &name)?;
            } else {
                tracing::debug!(
                    "goal {goal}: document {name} waits for its bytes {}",
                    file.sha256
                );
            }
        }
        Ok(())
    }

    /// The file under `documents/<name>`: a hard link to the blob, a copy
    /// where the filesystem refuses one. Idempotent.
    fn materialise_document(
        &self,
        goal: GoalId,
        sha256: &str,
        name: &str,
    ) -> Result<PathBuf, StoreError> {
        let blob = self.attachment_path(sha256).ok_or_else(|| {
            StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-no-attachment-workspace-2",
                sha256 = sha256.to_string()
            ))
        })?;
        let path = self.paths().goal(goal).document(name);
        if path.is_file() {
            return Ok(path);
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| StoreError::io(dir.display().to_string(), e))?;
        }
        if std::fs::hard_link(&blob, &path).is_err() {
            std::fs::copy(&blob, &path)
                .map_err(|e| StoreError::io(path.display().to_string(), e))?;
        }
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str) -> AttachmentRef {
        AttachmentRef {
            sha256: "ab".repeat(32),
            name: name.into(),
            mime: "application/octet-stream".into(),
            size: 1,
        }
    }

    #[test]
    fn a_taken_name_is_numbered_before_its_extension_and_a_bad_one_falls_back_to_the_hash() {
        let names: Vec<String> = named([
            file("brief.pdf"),
            file("brief.pdf"),
            file("brief.pdf"),
            file("notes"),
            file("notes"),
            file("../../etc/passwd"),
        ])
        .into_iter()
        .map(|(_, n)| n)
        .collect();
        assert_eq!(
            names[..5],
            [
                "brief.pdf",
                "brief (2).pdf",
                "brief (3).pdf",
                "notes",
                "notes (2)"
            ]
        );
        assert_eq!(
            names[5], "passwd",
            "a path is its last component, never a way out of the folder"
        );
        let mut taken = HashSet::new();
        taken.insert("abababababab".to_string());
        assert_eq!(
            distinct_name(&taken, &file("...")),
            "abababababab (2)",
            "nothing safe left: the hash names it, numbered like any other"
        );
    }
}
