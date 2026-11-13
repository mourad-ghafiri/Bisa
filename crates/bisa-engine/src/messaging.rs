//! What a person does in a conversation, as the engine's door to the store's
//! conversation surface: posting, channels, direct channels, reactions, retractions
//! and read markers.
//!
//! Thin on purpose. The store owns the records and the `StoreEvent`s that wake
//! the conversation listener and feed the SSE streams; this module exists so
//! the node never writes to the store (`docs/architecture/07-layering.md`,
//! rule 2).

use crate::{EngineError, Inner};
use bisa_core::{
    AttachmentRef, Channel, ChannelId, ContextRef, DeletableChannel, MessageBody, PrincipalId,
    RosterPolicy, Tags,
};
use bisa_store::PostOrigin;
use std::sync::Arc;

/// A message a person typed: the text, the chips it carries, what it answers,
/// whom it addresses (as tokens the scope resolves) and the files it names.
pub struct PersonPost {
    pub text: String,
    pub context: Vec<ContextRef>,
    pub reply_to: Option<String>,
    pub mentions: Vec<String>,
    pub attachments: Vec<AttachmentRef>,
    /// What the person shares to be looked at rather than handed over as a
    /// file — uploaded first, described here.
    pub artifacts: Vec<bisa_core::ArtifactRef>,
}

/// Post into a scope as the person at the keyboard.
///
/// The mentions are resolved by the store: a token may be a pubkey, an agent
/// definition id, or the scope's own id — the channel handle, which expands to
/// that channel's roster — and an unknown token is a refusal, never a silent
/// drop. No mentions means nobody is addressed; `conversation::dispatch` wakes
/// only whom a message `p`-tags.
pub fn post_as_person(
    inner: &Arc<Inner>,
    scope: &str,
    post: PersonPost,
) -> Result<String, EngineError> {
    let mentions = inner.ws.resolve_mentions(scope, &post.mentions)?;
    let text = words_for(inner, scope, post.text);
    Ok(inner.ws.post_message(
        scope,
        MessageBody::Post {
            text,
            context: post.context,
            artifacts: post.artifacts,
            thinking: None,
            said: None,
        },
        post.reply_to,
        &mentions,
        &post.attachments,
        None,
        PostOrigin::Asked,
    )?)
}

/// A person's words as they are stored in a scope. A message that leaves
/// this machine for a person on another node is redacted before it is
/// signed (14-collaboration): a key typed into a room a guest reads must
/// not sit raw in their copy of the log. The one rule for every door a
/// person speaks through — the node's routes, and the command line when no
/// node runs.
pub fn words_for(inner: &Arc<Inner>, scope: &str, text: String) -> String {
    if crate::collab::scope_leaves_the_machine(inner, scope) {
        inner.security.redact_inbound(&text, "person_post")
    } else {
        text
    }
}

pub fn create_channel(
    inner: &Arc<Inner>,
    name: &str,
    topic: Option<&str>,
    roster: RosterPolicy,
    tags: Tags,
) -> Result<Channel, EngineError> {
    Ok(inner.ws.create_channel(name, topic, roster, tags)?)
}

/// What an edit of a standing channel names: each field absent keeps what is
/// stored. The roster has three halves and an edit replaces the ones it
/// names; a topic of nothing clears it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChannelEdit {
    pub topic: Option<String>,
    pub agents: Option<Vec<bisa_core::AgentId>>,
    pub teams: Option<Vec<bisa_core::TeamId>>,
    pub humans: Option<Vec<PrincipalId>>,
    pub tags: Option<Tags>,
}

/// Edit a channel by the fields named, on the record as it stands under the
/// store's one writer ([`bisa_store::Workspace::update_channel_with`]): two
/// edits at once of different fields both stand. Read, merged and written
/// back whole by the caller, the later one undid the earlier.
pub fn edit_channel(
    inner: &Arc<Inner>,
    id: &ChannelId,
    edit: ChannelEdit,
) -> Result<Channel, EngineError> {
    let (_, after) = inner.ws.update_channel_with(id, |channel| {
        if let Some(topic) = edit.topic {
            channel.topic = Some(topic);
        }
        if let Some(tags) = edit.tags {
            channel.tags = tags;
        }
        if edit.agents.is_some() || edit.teams.is_some() || edit.humans.is_some() {
            let (agents, teams, humans) = match &channel.roster {
                RosterPolicy::Listed {
                    agents,
                    teams,
                    humans,
                } => (agents.clone(), teams.clone(), humans.clone()),
                RosterPolicy::Everyone => (vec![], vec![], vec![]),
            };
            channel.roster = RosterPolicy::Listed {
                agents: edit.agents.unwrap_or(agents),
                teams: edit.teams.unwrap_or(teams),
                humans: edit.humans.unwrap_or(humans),
            };
        }
        Ok(())
    })?;
    Ok(after)
}

/// Delete a channel the type has already agreed is deletable (`general` never
/// is). A channel something still points at is refused by the store — before
/// anything is stopped; then every turn an agent runs in the channel is
/// stopped — the one still launching and the message queued behind one
/// among them — so no session goes on posting into a scope that no longer
/// resolves.
pub async fn delete_channel(
    inner: &Arc<Inner>,
    channel: DeletableChannel,
) -> Result<(), EngineError> {
    delete_channel_by_id(inner, &channel.id()).await
}

/// [`delete_channel`] by the id alone — the door a record nobody can read
/// is deleted through.
pub async fn delete_channel_by_id(
    inner: &Arc<Inner>,
    id: &bisa_core::DeletableChannelId,
) -> Result<(), EngineError> {
    inner.ws.refuse_channel_delete_by_id(id)?;
    crate::conversation::end_scope_sessions(inner, id.as_id().as_str()).await;
    Ok(inner.ws.delete_channel_by_id(id)?)
}

/// The turns in flight in a channel or a direct channel — each agent's
/// words and thinking so far — for a reader that joins mid-turn; empty when
/// nothing runs.
pub fn live_turns(
    inner: &Arc<Inner>,
    id: &ChannelId,
) -> Result<Vec<crate::conversations::LiveTurnRow>, EngineError> {
    inner.ws.get_channel(id)?;
    Ok(crate::conversations::live_rows(inner, id.as_str()))
}

/// Open — or find — the direct channel between these people and the owner.
pub fn open_dm(inner: &Arc<Inner>, participants: &[PrincipalId]) -> Result<Channel, EngineError> {
    Ok(inner.ws.open_dm(participants)?)
}

/// Retract one of the owner's own facts; answers the retraction's event id.
pub fn retract(inner: &Arc<Inner>, event_id: &str) -> Result<String, EngineError> {
    Ok(inner.ws.retract(event_id)?)
}

/// React as the person at the keyboard; answers the reaction's event id.
pub fn react(inner: &Arc<Inner>, event_id: &str, emoji: &str) -> Result<String, EngineError> {
    Ok(inner.ws.react(event_id, emoji, None)?)
}

/// Read a scope up to `at` — the newest moment its reader was shown — or up
/// to now when none is named; answers the mark as it stands, which never
/// moves backwards ([`bisa_store::Workspace::mark_read_up_to`]).
pub fn mark_read(inner: &Arc<Inner>, scope: &str, at: Option<u64>) -> Result<u64, EngineError> {
    a_key(scope)?;
    Ok(inner.ws.mark_read_up_to(scope, at)?)
}

/// Put a scope back unread; its true count is kept.
pub fn mark_unread(inner: &Arc<Inner>, scope: &str) -> Result<(), EngineError> {
    a_key(scope)?;
    Ok(inner.ws.mark_unread(scope)?)
}

/// A mark is kept only for a key a row could have ([`crate::inbox::is_key`]).
fn a_key(scope: &str) -> Result<(), EngineError> {
    if crate::inbox::is_key(scope) {
        return Ok(());
    }
    Err(EngineError::Invalid(bisa_core::text!(
        "error-engine-invalid-not-a-key",
        scope = scope.to_string()
    )))
}
