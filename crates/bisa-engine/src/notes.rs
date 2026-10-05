//! Notes on the engine's side: making, editing and deleting one, and telling
//! the screens a note moved.
//!
//! # The conversation stands beside the document
//!
//! An agent is asked about a note in a **conversation** whose origin is the
//! note ([`bisa_core::ConversationOrigin::Note`], 13 — Conversations): the
//! same thread, streaming and mentions as every conversation, opened in a
//! drawer beside the note. The agent reads the note with `note_read` and
//! writes into it only when asked — the engine chooses this note for a note
//! tool call that names none, exactly as it chooses a drawing for the
//! drawing tools. Its reply is the conversation's, never the note's:
//! nothing lands in the document that a hand did not ask for.
//!
//! # Two writers, two bounded writes
//!
//! An agent has two writes, each bounded so the shared document stays
//! safe. `note_append` **adds**: a block at the end wearing
//! [`attribution_block`], the same shape whether asked for or volunteered,
//! never touching what is there. `note_write` **replaces** the body — asked
//! to change the text, the agent rewrites it — but only at the hash it read
//! (`note_read` answers it): a note that moved since is refused with its
//! current hash, so a write never lands over a text the agent has not seen.
//! Neither write empties a note, neither writes over a body the redactor
//! would change (the agent read a placeholder, and writing it back would
//! store the placeholder over the person's secret), and what a person is
//! still typing is the desktop's to keep: the editor never adopts a change
//! over unsaved text, and offers the previous version back after a rewrite.
//! Every write — a person's or an agent's — is announced with the body's
//! hash, so an open editor tells its own save from somebody else's.

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

/// Said when an agent asks to write nothing: a note is emptied by its owner,
/// never by an agent.
pub const WRITE_NOTHING: &str =
    "Nothing to write — `text` was empty; a note is emptied by its owner, never by an agent";

/// Said when the note holds a secret the platform never hands an agent: the
/// agent read a placeholder, and writing the body back would store the
/// placeholder over the person's secret.
pub const WRITE_HOLDS_SECRET: &str = "this note holds a secret the platform never hands you — \
rewriting it would store a placeholder over it; add with note_append, or ask the person to make \
the change";

/// Said when a write states a hash the note has moved past: read again,
/// write once at the hash read.
pub fn stale_write_words(current_hash: &str) -> String {
    format!(
        "the note changed since you read it (current hash {current_hash}): note_read it again \
         and write once more at that hash"
    )
}

/// Rewrite a note's body as an agent, at the hash it read — the store's
/// compare-and-swap (`update_note`) refuses a note that moved since — and
/// announce the change under the agent's name. The checks that read as
/// words to the agent (an empty text, a secret in the body, the stale hash
/// said early) are the intake's; this is the write.
pub fn write_by_agent(
    inner: &Arc<Inner>,
    id: NoteId,
    text: &str,
    base_hash: &str,
    by: Option<bisa_core::AgentId>,
) -> Result<Note, EngineError> {
    let note = inner.ws.update_note(
        id,
        NotePatch {
            title: None,
            body: Some(text.to_string()),
            pinned: None,
        },
        Some(base_hash),
    )?;
    inner.notes_git.touched(inner);
    emit_written(inner, &note, by);
    Ok(note)
}

/// Say on the bus that a note changed.
///
/// Called by every path that writes one — the HTTP routes and an agent's
/// `note_append` — for the reason
/// [`crate::projects::emit_created`] gives: a surface can only refresh on a
/// fact it is told, and the notes overlay is open on top of whatever screen
/// you are on rather than being a screen that could reload itself.
pub fn emit_changed(inner: &Inner, note: &Note) {
    emit_written(inner, note, None);
}

/// [`emit_changed`], naming the agent whose tool wrote when one did. The
/// frame carries the body's hash, so an open editor tells its own save
/// from somebody else's write without a round trip.
pub fn emit_written(inner: &Inner, note: &Note, by: Option<bisa_core::AgentId>) {
    inner.emit(EngineEvent::global(EnginePayload::NoteChanged {
        note: note.id.to_string(),
        scope: note.scope.kind().to_string(),
        scope_id: note.scope.id(),
        hash: bisa_store::body_hash(&note.body),
        by,
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
