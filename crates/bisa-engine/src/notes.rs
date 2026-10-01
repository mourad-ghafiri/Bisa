//! Notes on the engine's side: making, editing and deleting one, and telling
//! the screens a note moved.
//!
//! # The conversation stands beside the document
//!
//! An agent is asked about a note in a **conversation** whose origin is the
//! note ([`bisa_core::ConversationOrigin::Note`], 13 — Conversations): the
//! same thread, streaming and mentions as every conversation, opened in a
//! drawer beside the note. The agent reads the note with `note_read` and
//! writes into it only when asked, with `note_append` — the engine chooses
//! this note for a note tool call that names none, exactly as it chooses a
//! drawing for the drawing tools. Its reply is the conversation's, never
//! the note's: nothing lands in the document that a hand did not ask for.
//!
//! # Two writers, one shape of write
//!
//! There is deliberately no op that lets an agent rewrite a note. The one
//! property that makes a shared document safe is that the other writer can
//! only add — so `note_append` is the whole of an agent's reach, and every
//! block it adds wears [`attribution_block`], the same shape whether asked
//! for or volunteered.

use crate::events::{EngineEvent, EnginePayload};
use crate::{EngineError, Inner};
use bisa_core::{Note, NoteId};
use bisa_store::{NewNote, NotePatch};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Create a note and announce it.
pub fn create(inner: &Arc<Inner>, new: NewNote) -> Result<Note, EngineError> {
    inner.notes_git.touched(inner);
    let note = inner.ws.create_note(new)?;
    inner.notes_git.touched(inner);
    emit_changed(inner, &note);
    Ok(note)
}

/// Edit a note against the hash the editor last saw.
///
/// Deliberately silent on the bus. `note_changed` exists to tell a surface
/// about a write it did **not** make, and this caller is holding the answer —
/// the note is in the response. Announcing it drove a feedback loop: the frame
/// came back, the overlay refetched, and the refetch replaced the text
/// somebody was still typing, which produced another write.
pub fn update(
    inner: &Arc<Inner>,
    id: NoteId,
    patch: NotePatch,
    base_hash: Option<&str>,
) -> Result<Note, EngineError> {
    let note = inner.ws.update_note(id, patch, base_hash)?;
    inner.notes_git.touched(inner);
    Ok(note)
}

/// Delete a note and announce it, so an overlay showing it lets go.
pub fn delete(inner: &Arc<Inner>, id: NoteId) -> Result<(), EngineError> {
    let note = inner.ws.get_note(id)?;
    inner.ws.delete_note(id)?;
    inner.notes_git.touched(inner);
    emit_changed(inner, &note);
    Ok(())
}

/// Say on the bus that a note changed.
///
/// Called by every path that writes one — the HTTP routes and an agent's
/// `note_append` — for the reason
/// [`crate::projects::emit_created`] gives: a surface can only refresh on a
/// fact it is told, and the notes overlay is open on top of whatever screen
/// you are on rather than being a screen that could reload itself.
pub fn emit_changed(inner: &Inner, note: &Note) {
    inner.emit(EngineEvent::global(EnginePayload::NoteChanged {
        note: note.id.to_string(),
        scope: note.scope.kind().to_string(),
        scope_id: note.scope.id(),
    }));
}

/// The block an agent's `note_append` becomes.
///
/// Markdown, and **nothing ever parses it back out**. The attribution is for
/// whoever re-reads the note, which means you can edit or delete an agent's
/// paragraph exactly like your own and nothing will notice or object — the
/// property that keeps this a document rather than a transcript with an owner.
pub fn attribution_block(author: &str, at: u64, text: &str) -> String {
    // Local time, and the date as well as the clock. A note is re-read weeks
    // later, where a bare "14:22" is the one timestamp shape that cannot be
    // placed at all.
    let when = chrono::DateTime::<chrono::Local>::from(
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(at),
    )
    .format("%Y-%m-%d %H:%M");
    format!("---\n\n**{author}** · {when}\n\n{text}")
}
