//! A goal's **documents** as the engine sees them: the files a person gave
//! the goal as context, under its `documents/` folder.
//!
//! The store owns the record and the folder (`bisa_store::goal_documents`).
//! What is the engine's: giving several at once and telling the feed
//! ([`add_documents`]), and telling a session — the Workflow Agent at its
//! wake, a work item in its first prompt, any session asking `get_goal` —
//! that the documents are there and where ([`note`], [`orientation`]).
//! No tool reads them: a harness reads a file by its path.

use crate::events::{EngineEvent, EnginePayload};
use crate::{EngineError, Inner};
use bisa_core::{AttachmentRef, GoalId};
use bisa_store::GoalDocument;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;

/// Give a goal documents, in the order given: each materialised and
/// recorded by the store, each announced. A descriptor whose bytes this
/// machine does not hold refuses the whole call before anything is written —
/// the caller uploaded them first, or did not.
/// Every document a request names has its bytes uploaded already — checked
/// before anything is written, so a goal is never created for documents it
/// cannot have (`ops::submit`) and never told to add them (`add_documents`).
pub fn check(inner: &Inner, files: &[AttachmentRef]) -> Result<(), EngineError> {
    if let Some(missing) = files
        .iter()
        .find(|f| inner.ws.attachment_path(&f.sha256).is_none())
    {
        return Err(EngineError::Store(bisa_store::StoreError::Invalid(
            bisa_core::text!(
                "error-store-invalid-no-attachment-upload-bytes-first",
                a0 = (missing.sha256).to_string(),
                a1 = (missing.name).to_string()
            ),
        )));
    }
    Ok(())
}

pub fn add_documents(
    inner: &Arc<Inner>,
    goal: GoalId,
    files: &[AttachmentRef],
) -> Result<Vec<GoalDocument>, EngineError> {
    check(inner, files)?;
    let mut added = Vec::with_capacity(files.len());
    for file in files {
        let doc = inner.ws.add_goal_document(goal, file)?;
        inner.emit(EngineEvent::scoped(
            goal,
            None,
            EnginePayload::DocumentAdded {
                goal,
                file: doc.file.clone(),
            },
        ));
        added.push(doc);
    }
    Ok(added)
}

/// The goal's documents as a session is told about them in `get_goal`:
/// name, absolute path, type and size — the precedent being the projects'
/// roots. A document whose bytes are not on this machine is left out: a
/// path that is not there is worse than none.
pub fn orientation(inner: &Inner, goal: GoalId) -> Vec<Value> {
    inner
        .ws
        .goal_documents(goal)
        .unwrap_or_default()
        .into_iter()
        .filter(|d| d.present)
        .map(|d| {
            json!({
                "name": d.name,
                "path": d.path.display().to_string(),
                "mime": d.file.mime,
                "size": d.file.size,
            })
        })
        .collect()
}

/// One sentence for a prompt when the goal has documents; nothing when it
/// has none, so a prompt never mentions an empty folder.
pub fn note(inner: &Inner, goal: GoalId) -> Option<String> {
    let docs = inner.ws.goal_documents(goal).unwrap_or_default();
    sentence(&inner.ws.paths().goal(goal).documents(), &docs)
}

/// The sentence itself, from the folder and the documents present in it.
pub fn sentence(dir: &Path, docs: &[GoalDocument]) -> Option<String> {
    let names: Vec<&str> = docs
        .iter()
        .filter(|d| d.present)
        .map(|d| d.name.as_str())
        .collect();
    if names.is_empty() {
        return None;
    }
    let plural = if names.len() == 1 { "" } else { "s" };
    Some(format!(
        "The person gave this goal {} document{plural} as context, under `{}`: {}. Read what \
         is relevant before deciding anything they might already have settled.",
        names.len(),
        dir.display(),
        names.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn doc(name: &str, present: bool) -> GoalDocument {
        GoalDocument {
            file: AttachmentRef {
                sha256: "ab".repeat(32),
                name: name.into(),
                mime: "application/pdf".into(),
                size: 3,
            },
            name: name.into(),
            path: PathBuf::from("/w/goals/g/documents").join(name),
            present,
        }
    }

    #[test]
    fn the_sentence_names_the_folder_and_the_documents_present_or_says_nothing() {
        let dir = PathBuf::from("/w/goals/g/documents");
        assert_eq!(sentence(&dir, &[]), None);
        assert_eq!(
            sentence(&dir, &[doc("ghost.pdf", false)]),
            None,
            "absent bytes are not a path to hand out"
        );
        let one = sentence(&dir, &[doc("brief.pdf", true)]).unwrap();
        assert!(one.starts_with("The person gave this goal 1 document as context, under `/w/goals/g/documents`: brief.pdf."), "{one}");
        let two = sentence(
            &dir,
            &[
                doc("brief.pdf", true),
                doc("mockup.png", true),
                doc("ghost.pdf", false),
            ],
        )
        .unwrap();
        assert!(two.contains("2 documents as context"), "{two}");
        assert!(two.contains(": brief.pdf, mockup.png."), "{two}");
    }
}
