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

/// Longest note body, in bytes — a reading budget: an agent asked about a note
/// reads the whole body with `note_read` (its first prompt carries the title).
pub const MAX_NOTE_BYTES: usize = 128 * 1024;

/// Longest note title, in characters — a title is a line in a list, in a
/// front matter block and in a prompt's first words. A file that arrived
/// with a longer one still reads; the cap is on what is written.
pub const MAX_NOTE_TITLE_CHARS: usize = 200;

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
        Self::validate_title(&self.title)?;
        Self::validate_body(&self.body)
    }

    /// A title as the platform keeps it: one trimmed line. A line break or
    /// any other control character would break the front matter block — or a
    /// list row, or a terminal — so each is a space, a `\r\n` pair one. The
    /// one rule the file, the index and every answer share.
    pub fn clean_title(raw: &str) -> String {
        raw.replace("\r\n", "\n")
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect::<String>()
            .trim()
            .to_string()
    }

    /// What a title being written must be: something, within the cap.
    pub fn validate_title(title: &str) -> Result<(), NoteError> {
        if title.trim().is_empty() {
            return Err(NoteError::EmptyTitle);
        }
        let chars = title.chars().count();
        if chars > MAX_NOTE_TITLE_CHARS {
            return Err(NoteError::TitleTooLong { chars });
        }
        Ok(())
    }

    /// What a body being written must be: within the cap.
    pub fn validate_body(body: &str) -> Result<(), NoteError> {
        if body.len() > MAX_NOTE_BYTES {
            return Err(NoteError::TooLarge(body.len()));
        }
        Ok(())
    }

    /// The file: the front matter block, then the body as written. A title
    /// is one line by construction ([`Note::clean_title`]); it is cleaned
    /// here too rather than trusted.
    pub fn to_markdown(&self) -> String {
        let title = Self::clean_title(&self.title);
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
        // A byte-order mark an editor left is not the note's.
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let mut lines = text.split_inclusive('\n');
        let Some(opening) = lines
            .next()
            .filter(|l| l.trim_end_matches(['\r', '\n']) == FENCE)
        else {
            return Err(NoteError::BadFrontMatter(
                "the file does not open with a front matter block".into(),
            ));
        };
        let mut title = None;
        let mut pinned = None;
        let mut created_at = None;
        let mut updated_at = None;
        // How far the block reaches is counted in the lines as they are in
        // the file — a `\r\n` ending is two bytes, not one — so the body
        // starts exactly after the closing fence.
        let mut consumed = opening.len();
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
    #[error("a note's title is {chars} characters; the cap is {MAX_NOTE_TITLE_CHARS}")]
    TitleTooLong { chars: usize },
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

    /// A file a Windows checkout rewrote, one with stray carriage returns on
    /// its opening line, and one an editor put a byte-order mark on: each
    /// reads back with the body it was written with, and none panics.
    #[test]
    fn a_file_with_other_line_endings_reads_back_with_its_own_body() {
        let id = NoteId::from_ulid(ulid::Ulid::from_parts(5, 1));
        let read = |text: &str| Note::from_markdown(id, OwnerScope::Workspace, text).unwrap();
        let crlf = "---\r\ntitle: t\r\ncreated_at: 1\r\nupdated_at: 2\r\n---\r\nbody\r\n";
        assert_eq!(read(crlf).body, "body\r\n", "no newline is gained");
        assert_eq!(read(crlf).title, "t");
        // The opening line's length is the line's, not the fence's: with the
        // count short, the body's start fell inside the `é` and the slice
        // panicked.
        let stray = "---\r\r\r\r\r\r\ncreated_at: 1\nupdated_at: 2\ntitle: é\n---\nbody";
        assert_eq!(read(stray).body, "body");
        assert_eq!(read(stray).title, "é");
        let marked = "\u{feff}---\ntitle: t\ncreated_at: 1\nupdated_at: 2\n---\nbody";
        assert_eq!(read(marked).body, "body");
        assert!(matches!(
            Note::from_markdown(id, OwnerScope::Workspace, ""),
            Err(NoteError::BadFrontMatter(m)) if m.contains("does not open")
        ));
    }

    /// A title is one trimmed line of at most the cap: what the file can
    /// hold is what the answer and the index say.
    #[test]
    fn a_title_is_one_trimmed_line_within_its_cap() {
        assert_eq!(Note::clean_title("  a\nb\r\n "), "a b");
        assert_eq!(Note::clean_title("a\r\nb"), "a b", "one break, one space");
        assert_eq!(Note::clean_title("tab\there\u{7}"), "tab here");
        assert_eq!(Note::clean_title(" \n "), "");
        assert_eq!(Note::validate_title(""), Err(NoteError::EmptyTitle));
        assert_eq!(
            Note::validate_title(&"é".repeat(MAX_NOTE_TITLE_CHARS)),
            Ok(())
        );
        assert_eq!(
            Note::validate_title(&"é".repeat(MAX_NOTE_TITLE_CHARS + 1)),
            Err(NoteError::TitleTooLong {
                chars: MAX_NOTE_TITLE_CHARS + 1
            })
        );
        assert_eq!(Note::validate_body(&"x".repeat(MAX_NOTE_BYTES)), Ok(()));
        let n = Note {
            id: NoteId::from_ulid(ulid::Ulid::from_parts(5, 1)),
            scope: OwnerScope::Workspace,
            title: "bell\u{7}rung".into(),
            body: String::new(),
            created_at: 1,
            updated_at: 2,
            pinned: false,
        };
        assert!(n.to_markdown().contains("title: bell rung\n"));
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
