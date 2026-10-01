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
    // A message that leaves this machine for a person on another node is
    // redacted before it is signed (14-collaboration): a key typed into a
    // room a guest reads must not sit raw in their copy of the log.
    let text = if crate::collab::scope_leaves_the_machine(inner, scope) {
        inner.security.redact_inbound(&post.text, "person_post")
    } else {
        post.text
    };
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

pub fn create_channel(
    inner: &Arc<Inner>,
    name: &str,
    topic: Option<&str>,
    roster: RosterPolicy,
    tags: Tags,
) -> Result<Channel, EngineError> {
    Ok(inner.ws.create_channel(name, topic, roster, tags)?)
}

pub fn update_channel(
    inner: &Arc<Inner>,
    id: &ChannelId,
    topic: Option<&str>,
    roster: RosterPolicy,
    tags: Tags,
) -> Result<Channel, EngineError> {
    Ok(inner.ws.update_channel(id, topic, roster, tags)?)
}

/// Delete a channel the type has already agreed is deletable (`general` never
/// is). A channel something still points at is refused by the store — before
/// anything is stopped; then every live turn an agent runs in the channel is
/// disposed, as a conversation's are on its deletion, so no session goes on
/// posting into a scope that no longer resolves.
pub async fn delete_channel(
    inner: &Arc<Inner>,
    channel: DeletableChannel,
) -> Result<(), EngineError> {
    let id = channel.channel().id.to_string();
    inner.ws.refuse_channel_delete_if_used(&channel)?;
    crate::conversation::dispose_live_sessions(inner, &id).await;
    Ok(inner.ws.delete_channel(channel)?)
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

pub fn mark_read(inner: &Arc<Inner>, scope: &str) -> Result<(), EngineError> {
    Ok(inner.ws.mark_read(scope)?)
}

pub fn mark_unread(inner: &Arc<Inner>, scope: &str) -> Result<(), EngineError> {
    Ok(inner.ws.mark_unread(scope)?)
}
