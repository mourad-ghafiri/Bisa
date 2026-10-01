//! `Note`: a markdown scratchpad belonging to no record the platform maintains.
//! Local, never synced by the platform — a note's way off this machine is
//! the **notes repository**: every note is a Markdown file under
//! `<data_dir>/notes/`, a git repository the person commits and pushes from
//! the Notes overlay.
//!
//! On disk a note is Markdown with a front matter block — the four facts a
//! file cannot carry in its name — then the body as written:
//!
//! ```text
//! ---
//! title: Why the cache is a cache
//! pinned: true
//! created_at: 1788775200
//! updated_at: 1788775300
//! ---
//! The body, as Markdown.
//! ```
//!
//! [`Note::to_markdown`] and [`Note::from_markdown`] are the one pair; the
//! id is the file's name and the scope its directory, so neither is in the
//! block. The block is strict: those four keys, nothing else, so a file a
//! person edited by hand in a clone says exactly what it cannot say.

use crate::id::NoteId;
use crate::owner_scope::OwnerScope;
use serde::{Deserialize, Serialize};

/// The line that opens and closes the front matter block.
const FENCE: &str = "---";

/// Longest note body, in bytes — a prompt budget: asking an agent about a note
/// puts the whole body in its first prompt.
pub const MAX_NOTE_BYTES: usize = 128 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Note {
    pub id: NoteId,
    pub scope: OwnerScope,
    pub title: String,
    /// One markdown string. An agent's contribution is appended under an
    /// attribution line that is for the reader, never parsed.
    pub body: String,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default)]
    pub pinned: bool,
}

impl Note {
    pub fn validate(&self) -> Result<(), NoteError> {
        if self.title.trim().is_empty() {
            return Err(NoteError::EmptyTitle);
        }
        if self.body.len() > MAX_NOTE_BYTES {
            return Err(NoteError::TooLarge(self.body.len()));
        }
        Ok(())
    }

    /// The file: the front matter block, then the body as written. A title
    /// is one line by construction; a newline in it would break the block,
    /// so it is folded to a space here rather than trusted.
    pub fn to_markdown(&self) -> String {
        let title = self.title.split(['\n', '\r']).collect::<Vec<_>>().join(" ");
        let mut out = String::with_capacity(self.body.len() + 128);
        out.push_str(FENCE);
        out.push('\n');
        out.push_str(&format!("title: {title}\n"));
        out.push_str(&format!("pinned: {}\n", self.pinned));
        out.push_str(&format!("created_at: {}\n", self.created_at));
        out.push_str(&format!("updated_at: {}\n", self.updated_at));
        out.push_str(FENCE);
        out.push('\n');
        out.push_str(&self.body);
        out
    }

    /// The note a file holds: the id and scope come from where the file is,
    /// the four facts from its block, the body from what follows it.
    pub fn from_markdown(id: NoteId, scope: OwnerScope, text: &str) -> Result<Self, NoteError> {
        let mut lines = text.split_inclusive('\n');
        let opening = lines.next().map(|l| l.trim_end_matches(['\r', '\n']));
        if opening != Some(FENCE) {
            return Err(NoteError::BadFrontMatter(
                "the file does not open with a front matter block".into(),
            ));
        }
        let mut title = None;
        let mut pinned = None;
        let mut created_at = None;
        let mut updated_at = None;
        let mut consumed = opening.map(|_| FENCE.len() + 1).unwrap_or(0);
        let mut closed = false;
        for line in lines {
            consumed += line.len();
            let line = line.trim_end_matches(['\r', '\n']);
            if line == FENCE {
                closed = true;
                break;
            }
            let Some((key, value)) = line.split_once(':') else {
                return Err(NoteError::BadFrontMatter(format!(
                    "not a `key: value` line: {line:?}"
                )));
            };
            let value = value.trim();
            match key.trim() {
                "title" => title = Some(value.to_string()),
                "pinned" => {
                    pinned = Some(value.parse::<bool>().map_err(|_| {
                        NoteError::BadFrontMatter(format!("pinned is not true or false: {value:?}"))
                    })?)
                }
                "created_at" => created_at = Some(parse_secs("created_at", value)?),
                "updated_at" => updated_at = Some(parse_secs("updated_at", value)?),
                other => return Err(NoteError::BadFrontMatter(format!("unknown key: {other:?}"))),
            }
        }
        if !closed {
            return Err(NoteError::BadFrontMatter(
                "the front matter block never closes".into(),
            ));
        }
        let note = Note {
            id,
            scope,
            title: title.ok_or_else(|| NoteError::BadFrontMatter("no title".into()))?,
            body: text[consumed..].to_string(),
            created_at: created_at
                .ok_or_else(|| NoteError::BadFrontMatter("no created_at".into()))?,
            updated_at: updated_at
                .ok_or_else(|| NoteError::BadFrontMatter("no updated_at".into()))?,
            pinned: pinned.unwrap_or(false),
        };
        Ok(note)
    }
}

fn parse_secs(key: &str, value: &str) -> Result<u64, NoteError> {
    value
        .parse::<u64>()
        .map_err(|_| NoteError::BadFrontMatter(format!("{key} is not unix seconds: {value:?}")))
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NoteError {
    #[error("a note needs a title")]
    EmptyTitle,
    #[error("note body is {0} bytes; the cap is {MAX_NOTE_BYTES}")]
    TooLarge(usize),
    /// The file's front matter block is not the platform's — a hand edit in
    /// a clone, or not a note at all.
    #[error("the note's front matter is not readable: {0}")]
    BadFrontMatter(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::ProjectId;

    #[test]
    fn scope_and_note_roundtrip() {
        let n = Note {
            id: NoteId::from_ulid(ulid::Ulid::from_parts(5, 1)),
            scope: OwnerScope::Project {
                id: ProjectId::from_ulid(ulid::Ulid::from_parts(3, 1)),
            },
            title: "Why the cache is a cache".into(),
            body: "…".into(),
            created_at: 1,
            updated_at: 2,
            pinned: true,
        };
        assert!(n.validate().is_ok());
        let json = serde_json::to_value(&n).unwrap();
        assert_eq!(json["scope"]["kind"], "project");
        assert_eq!(serde_json::from_value::<Note>(json).unwrap(), n);
        assert_eq!(OwnerScope::Workspace.id(), None);
        assert_eq!(OwnerScope::Workspace.kind(), "workspace");
        assert_eq!(OwnerScope::Node.id(), None);
        assert_eq!(OwnerScope::Node.kind(), "node");
    }

    #[test]
    fn a_note_is_a_markdown_file_with_a_front_matter_block_and_reads_back_whole() {
        let n = Note {
            id: NoteId::from_ulid(ulid::Ulid::from_parts(5, 1)),
            scope: OwnerScope::Workspace,
            title: "Why the cache is a cache".into(),
            body: "# Heading\n\n---\n\nA rule, not a fence.\n".into(),
            created_at: 1_788_775_200,
            updated_at: 1_788_775_300,
            pinned: true,
        };
        let text = n.to_markdown();
        assert!(text.starts_with("---\ntitle: Why the cache is a cache\npinned: true\ncreated_at: 1788775200\nupdated_at: 1788775300\n---\n"), "{text}");
        assert!(text.ends_with(&n.body));
        assert_eq!(
            Note::from_markdown(n.id, n.scope.clone(), &text).unwrap(),
            n,
            "the body's own --- lines are the body's"
        );
        let empty = Note {
            body: String::new(),
            ..n.clone()
        };
        assert_eq!(
            Note::from_markdown(n.id, n.scope.clone(), &empty.to_markdown()).unwrap(),
            empty,
            "an empty body reads back empty"
        );
        let folded = Note {
            title: "two\nlines".into(),
            ..n.clone()
        };
        assert_eq!(
            Note::from_markdown(n.id, n.scope.clone(), &folded.to_markdown())
                .unwrap()
                .title,
            "two lines",
            "a title is one line"
        );
    }

    #[test]
    fn a_file_that_is_not_a_note_says_what_it_cannot_say() {
        let id = NoteId::from_ulid(ulid::Ulid::from_parts(5, 1));
        let bad = |text: &str| Note::from_markdown(id, OwnerScope::Workspace, text).unwrap_err();
        assert!(
            matches!(bad("# just markdown\n"), NoteError::BadFrontMatter(m) if m.contains("does not open"))
        );
        assert!(
            matches!(bad("---\ntitle: t\n"), NoteError::BadFrontMatter(m) if m.contains("never closes"))
        );
        assert!(
            matches!(bad("---\ntitle: t\ncolour: red\n---\n"), NoteError::BadFrontMatter(m) if m.contains("unknown key"))
        );
        assert!(
            matches!(bad("---\npinned: maybe\n---\n"), NoteError::BadFrontMatter(m) if m.contains("pinned"))
        );
        assert!(
            matches!(bad("---\ntitle: t\ncreated_at: soon\n---\n"), NoteError::BadFrontMatter(m) if m.contains("created_at"))
        );
        assert!(
            matches!(bad("---\npinned: true\ncreated_at: 1\nupdated_at: 1\n---\n"), NoteError::BadFrontMatter(m) if m == "no title")
        );
        assert!(
            matches!(bad("---\ntitle: t\nno colon here\n---\n"), NoteError::BadFrontMatter(m) if m.contains("key: value"))
        );
    }

    #[test]
    fn bounds() {
        let mut n = Note {
            id: NoteId::from_ulid(ulid::Ulid::from_parts(5, 1)),
            scope: OwnerScope::Workspace,
            title: " ".into(),
            body: String::new(),
            created_at: 0,
            updated_at: 0,
            pinned: false,
        };
        assert_eq!(n.validate(), Err(NoteError::EmptyTitle));
        n.title = "t".into();
        n.body = "x".repeat(MAX_NOTE_BYTES + 1);
        assert!(matches!(n.validate(), Err(NoteError::TooLarge(_))));
    }
}
