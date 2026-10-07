//! Conversations: the engine's door to the records — a saved exchange with
//! agents, with an origin ([`bisa_core::Conversation`]).
//!
//! The store holds the record and the messages; this module is what the node
//! and the CLI call, so that every change is one act: the record moves, the
//! sessions that were its turns are stopped when it is archived or deleted,
//! and the bus says so (`ConversationCreated`, `ConversationChanged`). The
//! turns themselves — who answers, where, what streams while one runs —
//! are [`crate::conversation`]'s; [`live_turns`] reads the turns in flight.

use crate::events::{ConversationChange, EngineEvent, EnginePayload};
use crate::{EngineError, Inner};
use bisa_core::{Conversation, ConversationId, ConversationMode, ConversationOrigin};
use bisa_store::{ConversationFilter, ConversationRow, NewConversation};
use std::sync::Arc;

fn emit_change(inner: &Inner, conversation: &Conversation, change: ConversationChange) {
    let payload = EnginePayload::ConversationChanged {
        id: conversation.id,
        change,
    };
    inner.emit(match conversation.origin.goal() {
        Some(goal) => EngineEvent::scoped(goal, None, payload),
        None => EngineEvent::global(payload),
    });
}

/// Start a conversation. The origin is checked by the store — a goal, a
/// workflow, a project or a workstream that is not here refuses it.
pub fn create(inner: &Inner, new: NewConversation) -> Result<Conversation, EngineError> {
    let conversation = inner.ws.create_conversation(new)?;
    let payload = EnginePayload::ConversationCreated {
        id: conversation.id,
        origin: conversation.origin.clone(),
    };
    inner.emit(match conversation.origin.goal() {
        Some(goal) => EngineEvent::scoped(goal, None, payload),
        None => EngineEvent::global(payload),
    });
    Ok(conversation)
}

/// One conversation as a list reads it: the record with its facts.
pub fn get(inner: &Inner, id: ConversationId) -> Result<ConversationRow, EngineError> {
    Ok(inner.ws.conversation_row(id)?)
}

/// The record itself.
pub fn record(inner: &Inner, id: ConversationId) -> Result<Conversation, EngineError> {
    Ok(inner.ws.get_conversation(id)?)
}

pub fn list(
    inner: &Inner,
    filter: &ConversationFilter,
) -> Result<Vec<ConversationRow>, EngineError> {
    Ok(inner.ws.list_conversations(filter)?)
}

/// The conversations about one thing.
pub fn of(
    inner: &Inner,
    origin: &ConversationOrigin,
    limit: usize,
) -> Result<Vec<ConversationRow>, EngineError> {
    list(inner, &ConversationFilter::of(origin, limit))
}

pub fn rename(
    inner: &Inner,
    id: ConversationId,
    title: Option<&str>,
) -> Result<Conversation, EngineError> {
    let conversation = inner.ws.rename_conversation(id, title)?;
    emit_change(inner, &conversation, ConversationChange::Renamed);
    Ok(conversation)
}

/// The mode a conversation about `origin` starts in: `agents.conversation.mode`,
/// resolved at the project when the origin stands in one.
pub fn default_mode(inner: &Inner, origin: &ConversationOrigin) -> ConversationMode {
    inner
        .ws
        .settings(origin.project())
        .ok()
        .and_then(|rows| {
            rows.into_iter()
                .find(|r| r.key == "agents.conversation.mode")
                .and_then(|r| r.value.as_str().and_then(ConversationMode::parse))
        })
        .unwrap_or_default()
}

/// Move a conversation to another mode. It holds from the next call a live
/// turn makes: the pump reads the mode each time it judges one.
pub fn set_mode(
    inner: &Inner,
    id: ConversationId,
    mode: ConversationMode,
) -> Result<Conversation, EngineError> {
    let before = inner.ws.get_conversation(id)?;
    if !before.origin.is_checkout() {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-only-conversation-about-project-workstream-has-mode"
        )));
    }
    if before.mode == mode {
        return Ok(before);
    }
    let conversation = inner.ws.set_conversation_mode(id, mode)?;
    inner.changes.asks.revoke_grants(id);
    emit_change(inner, &conversation, ConversationChange::Mode);
    Ok(conversation)
}

/// *Build this plan*: the conversation leaves `plan` for the mode it was in
/// before it — the record remembers which — or for the default when it began
/// in a plan. Out of a plan there is nothing to build: a refusal, so a
/// second press changes no mode a person set since.
pub fn build_plan(inner: &Inner, id: ConversationId) -> Result<Conversation, EngineError> {
    let planned = inner.ws.get_conversation(id)?;
    if !planned.origin.is_checkout() {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-only-conversation-about-project-workstream-has-mode"
        )));
    }
    if planned.mode != ConversationMode::Plan {
        return Err(EngineError::Conflict(bisa_core::text!(
            "error-engine-conflict-conversation-not-planning"
        )));
    }
    set_mode(inner, id, planned.mode_after_plan())
}

/// Put a conversation away, or take it back out. Archiving stops the turn
/// that may be running: an archived conversation is read, not continued.
pub async fn set_archived(
    inner: &Arc<Inner>,
    id: ConversationId,
    archived: bool,
) -> Result<Conversation, EngineError> {
    let before = inner.ws.get_conversation(id)?;
    if before.archived == archived {
        return Ok(before);
    }
    if archived {
        stop_sessions(inner, id).await;
    }
    let conversation = inner.ws.set_conversation_archived(id, archived)?;
    emit_change(
        inner,
        &conversation,
        if archived {
            ConversationChange::Archived
        } else {
            ConversationChange::Unarchived
        },
    );
    Ok(conversation)
}

/// Remove a conversation: its turns stopped first, then the record, the log
/// and the rows, then the bus told.
pub async fn delete(inner: &Arc<Inner>, id: ConversationId) -> Result<(), EngineError> {
    let conversation = inner.ws.get_conversation(id)?;
    stop_sessions(inner, id).await;
    inner.ws.delete_conversation(id)?;
    inner.changes.forget(id);
    emit_change(inner, &conversation, ConversationChange::Deleted);
    Ok(())
}

/// Stop every live session that is a turn of this conversation: the live
/// chat sessions disposed and parked, and any row on the roster that still
/// names the conversation aborted. Answers how many rows were aborted.
pub async fn stop_sessions(inner: &Arc<Inner>, id: ConversationId) -> usize {
    crate::conversation::dispose_live_sessions(inner, &id.to_string()).await;
    let mut stopped = 0;
    for row in inner.presence.snapshot() {
        if row.conversation != Some(id) || !row.state.is_live() {
            continue;
        }
        if crate::sessions::stop_row(inner, &row, crate::sessions::EndCause::Close) {
            stopped += 1;
        }
    }
    stopped
}

/// The turns in flight in a conversation — each agent's words and thinking
/// so far — for a reader that joins mid-turn; empty when nothing runs.
pub fn live_turns(inner: &Arc<Inner>, id: ConversationId) -> Result<Vec<LiveTurnRow>, EngineError> {
    inner.ws.get_conversation(id)?;
    Ok(crate::conversation::live_turns(inner, &id.to_string())
        .into_iter()
        .map(|(agent, turn)| LiveTurnRow {
            agent,
            text: turn.text,
            thinking: turn.thinking,
            since: turn.since,
            working: turn.working,
        })
        .collect())
}

/// One turn in flight: who speaks, what it has said and thought, since
/// when, and the tool it runs right now, if one.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct LiveTurnRow {
    pub agent: String,
    pub text: String,
    pub thinking: String,
    pub since: u64,
    pub working: Option<String>,
}
