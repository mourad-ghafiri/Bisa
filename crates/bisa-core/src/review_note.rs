//! `ReviewNote`: an annotation on a diff line, pinned to the diff it was written
//! against, handed to an agent as data.
//!
//! The shape has two load-bearing fields: the
//! diff identity — so a note does not silently reattach to a different line
//! after the file changes — and a sent-at mark that editing clears, so the
//! note knows whether the agent saw *this* version.

use crate::id::{NoteId, ProjectId, Sha256, WorkstreamId};
use crate::message::{DiffScope, LineRange};
use crate::path::RelPath;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReviewNote {
    pub id: NoteId,
    pub project: ProjectId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workstream: Option<WorkstreamId>,
    pub path: RelPath,
    pub range: LineRange,
    pub scope: DiffScope,
    /// Hash of the hunk text the note was written against.
    pub diff_identity: Sha256,
    /// The hunk text itself, so the note can be handed on as a
    /// `ContextRef::DiffHunk` chip and read by an agent without re-deriving a
    /// diff that may no longer exist. Bounded by the writer; empty when the
    /// note was written without one.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hunk: String,
    pub body: String,
    /// Set when handed to an agent; cleared on edit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sent_at: Option<u64>,
    /// Set when the agent (or the person) marked the note as dealt with.
    /// A resolved note stays on disk — it is the record that a remark was
    /// acted on — and is hidden from the default listings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<u64>,
    pub created_at: u64,
    pub updated_at: u64,
}

impl ReviewNote {
    pub fn is_resolved(&self) -> bool {
        self.resolved_at.is_some()
    }

    /// Mark the note dealt with. Idempotent: a second resolve keeps the first time.
    pub fn resolved(mut self, at: u64) -> Self {
        self.resolved_at.get_or_insert(at);
        self.updated_at = at;
        self
    }

    pub fn validate(&self) -> Result<(), ReviewNoteError> {
        if self.body.trim().is_empty() {
            return Err(ReviewNoteError::EmptyBody);
        }
        Ok(())
    }

    /// An edit clears the sent mark: the agent has not seen this version.
    pub fn edited(mut self, body: String, at: u64) -> Self {
        self.body = body;
        self.sent_at = None;
        self.updated_at = at;
        self
    }

    /// Whether the note still points at the diff it was written against.
    pub fn is_attached_to(&self, current_hunk_identity: &Sha256) -> bool {
        &self.diff_identity == current_hunk_identity
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ReviewNoteError {
    #[error("a review note needs a body")]
    EmptyBody,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note() -> ReviewNote {
        ReviewNote {
            id: NoteId::from_ulid(ulid::Ulid::from_parts(7, 1)),
            project: ProjectId::from_ulid(ulid::Ulid::from_parts(3, 1)),
            workstream: None,
            path: RelPath::new("src/main.rs").unwrap(),
            range: LineRange::new(10, 12).unwrap(),
            scope: DiffScope::Unstaged,
            diff_identity: Sha256::new("a".repeat(64)).unwrap(),
            hunk: "@@ -10,3 +10,3 @@\n-old\n+new\n".into(),
            body: "rename this".into(),
            sent_at: Some(5),
            resolved_at: None,
            created_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn resolving_is_idempotent_and_keeps_the_first_time() {
        let n = note().resolved(20).resolved(30);
        assert_eq!(n.resolved_at, Some(20));
        assert_eq!(n.updated_at, 30);
        assert!(n.is_resolved());
        assert!(!note().is_resolved());
    }

    #[test]
    fn editing_clears_the_sent_mark() {
        let n = note().edited("rename this properly".into(), 9);
        assert_eq!(n.sent_at, None);
        assert_eq!(n.updated_at, 9);
        assert!(n.validate().is_ok());
        assert_eq!(
            note().edited(" ".into(), 9).validate(),
            Err(ReviewNoteError::EmptyBody)
        );
    }

    #[test]
    fn identity_pins_the_note() {
        let n = note();
        assert!(n.is_attached_to(&Sha256::new("a".repeat(64)).unwrap()));
        assert!(!n.is_attached_to(&Sha256::new("b".repeat(64)).unwrap()));
        let json = serde_json::to_string(&n).unwrap();
        assert_eq!(serde_json::from_str::<ReviewNote>(&json).unwrap(), n);
    }
}
