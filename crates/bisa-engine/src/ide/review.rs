//! Review notes (ide/04 §6): annotations a person leaves on a
//! diff, pinned to the hunk they were written against, and handed to agents
//! as data — a message chip in the goal thread and a tool result over MCP —
//! never as text pasted into a terminal.
//!
//! The store owns the files under `projects/<slug>/review/`; this module is
//! the only writer the node and the intake reach, so bounds and identity are
//! decided in one place.

use crate::{EngineError, Inner};
use bisa_core::{
    ContextRef, DiffScope, LineRange, MessageBody, NoteId, ProjectId, RelPath, ReviewNote, Sha256,
    WorkstreamId,
};
use bisa_store::{content_hash, NewReviewNote, PostOrigin};

/// The most hunk text one note carries. A hunk somebody annotated is a
/// screenful, not a file; anything larger is truncated and the identity is
/// taken over what was kept.
pub const MAX_HUNK_BYTES: usize = 32 * 1024;
/// Serialised chips on one message stay under the core's 64 KiB bound with
/// room for the text around them.
const CHIPS_BUDGET: usize = 56 * 1024;

/// What the person supplies when annotating a hunk. Plain strings and
/// numbers: the node hands them over unparsed and the refusals come from here.
#[derive(Clone, Debug)]
pub struct NewNote {
    pub project: ProjectId,
    pub workstream: Option<WorkstreamId>,
    pub path: String,
    pub start: u32,
    pub end: u32,
    pub scope: DiffScope,
    /// The hunk text as the client had it. Hashed for `diff_identity`.
    pub hunk: String,
    pub body: String,
}

fn invalid(e: impl std::fmt::Display) -> EngineError {
    EngineError::Invalid(bisa_core::text!(
        "error-engine-ide-review-refused",
        detail = e.to_string()
    ))
}

/// Cut `hunk` to the cap on a line boundary so a chip never ends mid-line.
fn bounded_hunk(hunk: &str) -> String {
    if hunk.len() <= MAX_HUNK_BYTES {
        return hunk.to_string();
    }
    let mut end = MAX_HUNK_BYTES;
    while !hunk.is_char_boundary(end) {
        end -= 1;
    }
    let kept = &hunk[..end];
    let cut = kept.rfind('\n').map(|i| i + 1).unwrap_or(end);
    let mut out = kept[..cut].to_string();
    out.push_str("… (truncated)\n");
    out
}

pub fn create(inner: &Inner, new: NewNote) -> Result<ReviewNote, EngineError> {
    let path = RelPath::new(&new.path).map_err(invalid)?;
    let range = LineRange::new(new.start, new.end).map_err(invalid)?;
    if new.body.trim().is_empty() {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-review-note-needs-body"
        )));
    }
    let hunk = bounded_hunk(&new.hunk);
    let diff_identity = Sha256::new(content_hash(hunk.as_bytes())).map_err(invalid)?;
    Ok(inner.ws.create_review_note(NewReviewNote {
        project: new.project,
        workstream: new.workstream,
        path,
        range,
        scope: new.scope,
        diff_identity,
        hunk,
        body: new.body,
    })?)
}

/// A project's notes, oldest first. Resolved notes are hidden unless asked for.
pub fn list(
    inner: &Inner,
    project: ProjectId,
    workstream: Option<WorkstreamId>,
    include_resolved: bool,
) -> Result<Vec<ReviewNote>, EngineError> {
    let notes = inner.ws.list_review_notes(project, workstream)?;
    Ok(notes
        .into_iter()
        .filter(|n| include_resolved || !n.is_resolved())
        .collect())
}

pub fn get(inner: &Inner, project: ProjectId, id: NoteId) -> Result<ReviewNote, EngineError> {
    Ok(inner.ws.get_review_note(project, id)?)
}

/// Edit the body. Clears `sent_at`: the agent has not seen this version.
pub fn edit(
    inner: &Inner,
    project: ProjectId,
    id: NoteId,
    body: String,
) -> Result<ReviewNote, EngineError> {
    if body.trim().is_empty() {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-review-note-needs-body"
        )));
    }
    Ok(inner.ws.edit_review_note(project, id, body)?)
}

pub fn resolve(inner: &Inner, project: ProjectId, id: NoteId) -> Result<ReviewNote, EngineError> {
    Ok(inner.ws.resolve_review_note(project, id)?)
}

pub fn delete(inner: &Inner, project: ProjectId, id: NoteId) -> Result<(), EngineError> {
    Ok(inner.ws.delete_review_note(project, id)?)
}

/// What [`send`] did.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Sent {
    /// The notes, now marked sent.
    pub notes: Vec<ReviewNote>,
    /// The goal threads a message was posted into — every goal the project
    /// is attached to. Empty for a standalone project: the notes are still
    /// marked sent, and an agent in the project reads them with
    /// `review_notes_list`.
    pub posted_to: Vec<String>,
}

/// Hand notes to the agents: mark them sent and post one message per
/// attached goal, the notes as `DiffHunk` chips so what the agent sees is
/// what the person sees.
///
/// `ids` empty means every unsent, unresolved note of the project.
pub fn send(inner: &Inner, project: ProjectId, ids: Vec<NoteId>) -> Result<Sent, EngineError> {
    let p = inner.ws.get_project(project)?;
    let chosen: Vec<ReviewNote> = if ids.is_empty() {
        list(inner, project, None, false)?
            .into_iter()
            .filter(|n| n.sent_at.is_none())
            .collect()
    } else {
        ids.iter()
            .map(|id| get(inner, project, *id))
            .collect::<Result<Vec<_>, _>>()?
    };
    if chosen.is_empty() {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-no-review-notes-send"
        )));
    }

    let mut text = format!("Review notes on `{}` ({}):\n", p.slug, chosen.len());
    let mut chips: Vec<ContextRef> = Vec::with_capacity(chosen.len());
    for (i, n) in chosen.iter().enumerate() {
        let scope = match &n.scope {
            DiffScope::Unstaged => "unstaged".to_string(),
            DiffScope::Staged => "staged".to_string(),
            DiffScope::Branch { base } => format!("vs {base}"),
        };
        let lines = if n.range.start == n.range.end {
            n.range.start.to_string()
        } else {
            format!("{}-{}", n.range.start, n.range.end)
        };
        text.push_str(&format!(
            "{}. `{}:{lines}` ({scope}) — {}\n",
            i + 1,
            n.path.as_str(),
            n.body.trim()
        ));
        chips.push(ContextRef::DiffHunk {
            path: n.path.clone(),
            scope: n.scope.clone(),
            hunk: n.id.to_string(),
            patch: n.hunk.clone(),
        });
    }
    text.push_str(
        "Resolve each with `review_note_resolve` once it is dealt with; `review_notes_list` shows what is still open.",
    );
    // Keep the chips under the wire bound: drop patch text from the end
    // until they fit. The hunk id still names the note.
    let mut chip_bytes = serde_json::to_vec(&chips)
        .map(|v| v.len())
        .unwrap_or(usize::MAX);
    let mut trim_from = chips.len();
    while chip_bytes > CHIPS_BUDGET && trim_from > 0 {
        trim_from -= 1;
        if let ContextRef::DiffHunk { patch, .. } = &mut chips[trim_from] {
            patch.clear();
        }
        chip_bytes = serde_json::to_vec(&chips)
            .map(|v| v.len())
            .unwrap_or(usize::MAX);
    }

    let mut posted_to = Vec::new();
    for goal in inner.ws.goals_for_project(project)? {
        let scope = goal.id.to_string();
        inner.ws.post_message(
            &scope,
            MessageBody::Post {
                text: text.clone(),
                context: chips.clone(),
                artifacts: vec![],
                thinking: None,
                said: None,
            },
            None,
            &[],
            &[],
            None,
            PostOrigin::Asked,
        )?;
        posted_to.push(scope);
    }

    let mut notes = Vec::with_capacity(chosen.len());
    for n in &chosen {
        notes.push(inner.ws.mark_review_note_sent(project, n.id)?);
    }
    Ok(Sent { notes, posted_to })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hunk_is_cut_on_a_line_boundary_and_says_so() {
        let line = "+".repeat(1000) + "\n";
        let big = line.repeat(40); // 40 KiB
        let cut = bounded_hunk(&big);
        assert!(cut.len() <= MAX_HUNK_BYTES + 32, "{}", cut.len());
        assert!(cut.ends_with("… (truncated)\n"));
        let body = cut.trim_end_matches("… (truncated)\n");
        assert!(body.ends_with('\n'), "cut mid-line");
        assert_eq!(
            bounded_hunk("@@ -1 +1 @@\n-a\n+b\n"),
            "@@ -1 +1 @@\n-a\n+b\n"
        );
    }
}
