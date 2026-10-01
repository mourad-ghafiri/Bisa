//! Conversations: messages, reactions, retractions, attachments and read
//! markers — one kind (3407) for every conversation surface.
//!
//! A **scope** is a channel, a goal's thread or a conversation
//! ([`bisa_core::ScopeKind`]); [`Workspace::resolve_scope`] is the one
//! place a scope id becomes a coordinate and an audience. The conversation
//! *records* live in [`crate::conversations`]; this module is every message
//! stream's facts.
//!
//! A message's content is a [`MessageBody`]: a post with its context chips, or
//! a membership event. The chips are on the wire — what the agent sees is
//! exactly the list a person attached.
//!
//! Truth: `conversation/<scope>.jsonl` (append-only signed events);
//! `attachments/<sha256>` (content-addressed).
//! Every table under it is rebuildable. Read markers are local-only and lost
//! on rebuild by design.

use crate::error::StoreError;
use crate::index::{ArtifactListRow, MessageRow, ReactionRow};
use crate::paths::Paths;
use crate::workspace::{mint_ulid, now_secs, EventAudience, PostOrigin, StoreEvent, Workspace};
use bisa_core::kind::{
    KIND_CHANNEL, KIND_CONVERSATION, KIND_GOAL, KIND_MESSAGE, KIND_REACTION, KIND_RETRACTION,
};
use bisa_core::{
    ActivityConcept, ActivityFact, ActivitySource, ActivitySourceKind, AgentId, AttachmentRef,
    ChannelId, GoalId, MembershipEvent, MessageBody, PrincipalId, ScopeKind, TeamId,
    MAX_ATTACHMENT_BYTES,
};
use nostr::event::{Event, EventBuilder, FinalizeEvent, Kind, Tag};
use nostr::types::Timestamp;
use serde::{Deserialize, Serialize};
use sha2::Digest as _;
use std::path::PathBuf;

/// An attachment as a reader sees it: what it is, and whether it is here.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MessageAttachment {
    #[serde(flatten)]
    pub file: AttachmentRef,
    /// Whether the bytes are on **this** disk — answered at read time.
    pub present: bool,
}

/// An artifact as a reader sees it: the descriptor from the body, and
/// whether its bytes are here.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MessageArtifact {
    #[serde(flatten)]
    pub artifact: bisa_core::ArtifactRef,
    pub present: bool,
}

/// A resolved message scope: its kind, its addressable coordinate (for the
/// `a` tag) and its audience.
#[derive(Clone, Debug)]
pub struct ScopeRef {
    pub id: String,
    pub kind: ScopeKind,
    pub coordinate: String,
    pub audience: EventAudience,
}

fn tag_value<'a>(event: &'a Event, name: &str) -> Option<&'a str> {
    event.tags.iter().find_map(|t| {
        let s = t.as_slice();
        (s.len() >= 2 && s[0] == name).then(|| s[1].as_str())
    })
}

/// First `e` tag value (the reply parent / reaction target).
fn e_tag(event: &Event) -> Option<&str> {
    tag_value(event, "e")
}

/// Every `p` tag value — the pubkeys a message addresses.
pub(crate) fn p_tags(event: &Event) -> Vec<String> {
    event
        .tags
        .iter()
        .filter_map(|t| {
            let s = t.as_slice();
            (s.len() >= 2 && s[0] == "p").then(|| s[1].to_string())
        })
        .collect()
}

/// The tag that makes a post one of a kind: a ULID minted when it is said.
/// An event's id is a hash over its author, its second, its tags and its
/// content, so the same words from the same author within one second would
/// otherwise be one event — a reminder that fires again, a loop that posts
/// for every item, a person who says "yes" twice, all read as said once.
pub const ONCE_TAG: &str = "once";

/// What makes a conversation fact the fact it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Identity {
    /// Its own: a post. Said twice, it is two.
    ItsOwn,
    /// What it says, and nothing else: a membership event, a reaction, a
    /// retraction. Made twice, it is the same fact and one row
    /// (05 — Channels, *Idempotency*).
    OfWhatItSays,
}

/// What a conversation fact is built from.
struct Fact<'a> {
    kind: u16,
    content: &'a str,
    /// The event it answers and how: `(id, marker)`, the marker `""` for plain.
    e_target: Option<(&'a str, &'a str)>,
    mentions: &'a [PrincipalId],
    attachments: &'a [AttachmentRef],
    /// The agent that signs; the owner when none is named.
    signer: Option<&'a AgentId>,
    identity: Identity,
}

/// How much of a message the feed carries.
pub const SNIPPET_CHARS: usize = 280;

/// A message as the activity feed stores it: the concept is *channels*
/// whatever the scope, the source is the scope, and the event is
/// `{type: "message", message_id, body_kind, snippet}` — never the whole
/// body, which the conversation itself serves.
fn message_activity(
    scope: &ScopeRef,
    event: &Event,
    body: &MessageBody,
    at: u64,
    author: &str,
) -> ActivityFact {
    let source_kind = match scope.kind {
        ScopeKind::Channel => ActivitySourceKind::Channel,
        ScopeKind::Goal => ActivitySourceKind::Goal,
        ScopeKind::Conversation => ActivitySourceKind::Conversation,
    };
    let text = body_content(body);
    let snippet: String = text.chars().take(SNIPPET_CHARS).collect();
    ActivityFact {
        at,
        concept: ActivityConcept::Channels,
        kind: "message".into(),
        source: ActivitySource::new(source_kind, scope.id.clone()),
        author: Some(author.to_string()),
        event: serde_json::json!({
            "type": "message",
            "message_id": event.id.to_hex(),
            "body_kind": body.kind(),
            "snippet": snippet,
        }),
    }
}

fn body_content(body: &MessageBody) -> String {
    match body {
        MessageBody::Post { text, .. } => text.clone(),
        MessageBody::Membership(ev) => format!(
            "{} {} {} ({})",
            ev.member.kind(),
            ev.member.id(),
            match ev.change {
                bisa_core::MembershipChange::Joined => "joined",
                bisa_core::MembershipChange::Left => "left",
            },
            match ev.cause {
                bisa_core::MembershipCause::Enabled => "enabled",
                bisa_core::MembershipCause::Disabled => "disabled",
            }
        ),
    }
}

/// Where a page continues from: the moment of the oldest row shown, and —
/// so no row is lost — that row's id. Moments are whole seconds, and two
/// messages posted within one share theirs; a page cut by the moment alone
/// drops the rest of that second. With the id the cut is exact: everything
/// older than that row, whatever its second. Without one, `at` alone is the
/// bound, and it is exclusive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageBefore {
    pub at: u64,
    pub id: Option<String>,
}

impl PageBefore {
    pub fn at(at: u64) -> Self {
        Self { at, id: None }
    }

    pub fn row(at: u64, id: impl Into<String>) -> Self {
        Self {
            at,
            id: Some(id.into()),
        }
    }
}

impl Workspace {
    // ------------------------------------------------------------------
    // Scopes
    // ------------------------------------------------------------------

    /// Resolve a scope id to channel / goal / conversation, in that order.
    /// A project's or a workstream's id is no message scope: a checkout has
    /// conversations, not a thread, and its messages live in one of those.
    pub fn resolve_scope(&self, scope: &str) -> Result<ScopeRef, StoreError> {
        if let Ok(id) = ChannelId::new(scope) {
            if let Some((channel, ev)) = self.snapshots.get::<bisa_core::Channel>(
                Paths::NS_CHANNELS,
                KIND_CHANNEL,
                id.as_str(),
            )? {
                return Ok(ScopeRef {
                    id: scope.to_string(),
                    kind: ScopeKind::Channel,
                    coordinate: format!("{KIND_CHANNEL}:{}:{scope}", ev.pubkey.to_hex()),
                    audience: EventAudience::from(&channel.audience),
                });
            }
        }
        if let Ok(id) = scope.parse::<GoalId>() {
            if let Ok(goal) = self.get_goal(id) {
                return Ok(ScopeRef {
                    id: scope.to_string(),
                    kind: ScopeKind::Goal,
                    coordinate: format!("{KIND_GOAL}:{}:{scope}", goal.author.as_hex()),
                    audience: EventAudience::Workspace,
                });
            }
        }
        if let Ok(id) = scope.parse::<bisa_core::ConversationId>() {
            if let Some((_, ev)) = self.snapshots.get::<bisa_core::Conversation>(
                Paths::NS_CONVERSATIONS,
                KIND_CONVERSATION,
                &id.to_string(),
            )? {
                return Ok(ScopeRef {
                    id: scope.to_string(),
                    kind: ScopeKind::Conversation,
                    coordinate: format!("{KIND_CONVERSATION}:{}:{scope}", ev.pubkey.to_hex()),
                    audience: EventAudience::Workspace,
                });
            }
        }
        Err(StoreError::Invalid(bisa_core::text!(
            "error-store-invalid-unknown-scope",
            scope = scope.to_string()
        )))
    }

    /// Resolve mention tokens for one scope into principals.
    ///
    /// **The one place a mention becomes a pubkey.** A token is a 64-hex
    /// principal, an agent id, a team id (expands to its enabled agents), or
    /// the scope's own channel id — the **channel handle**, which expands to
    /// the roster. Unknown tokens are an error rather than a silent drop.
    pub fn resolve_mentions(
        &self,
        scope: &str,
        tokens: &[String],
    ) -> Result<Vec<PrincipalId>, StoreError> {
        let mut out: Vec<PrincipalId> = Vec::new();
        for token in tokens {
            if token == scope {
                if let Ok(id) = ChannelId::new(scope) {
                    out.extend(self.channel_roster_pubkeys(&id)?);
                    continue;
                }
            }
            if let Ok(pk) = PrincipalId::new(token.clone()) {
                out.push(pk);
                continue;
            }
            if let Ok(id) = AgentId::new(token) {
                match self.get_agent(&id) {
                    // A disabled agent is refused: addressing it posts a `p`
                    // tag nothing answers, and makes the message *addressed*,
                    // which stops triage from picking it up.
                    Ok(def) if !def.enabled => {
                        return Err(StoreError::Invalid(bisa_core::text!(
                            "error-store-invalid-cannot-address-agent-disabled-so-would-answer",
                            token = format!("{token:?}")
                        )))
                    }
                    Ok(def) => {
                        out.push(def.pubkey);
                        continue;
                    }
                    Err(_) => {}
                }
            }
            if let Ok(id) = TeamId::new(token) {
                if let Ok(team) = self.get_team(&id) {
                    if !team.enabled {
                        return Err(StoreError::Invalid(bisa_core::text!(
                            "error-store-invalid-cannot-address-team-disabled",
                            token = format!("{token:?}")
                        )));
                    }
                    for a in self.team_agents(&id)? {
                        if let Ok(def) = self.get_agent(&a) {
                            if def.enabled {
                                out.push(def.pubkey);
                            }
                        }
                    }
                    continue;
                }
            }
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-cannot-address-not-pubkey-agent-id-team",
                token = format!("{token:?}")
            )));
        }
        let mut seen = std::collections::HashSet::new();
        out.retain(|p| seen.insert(p.as_hex().to_string()));
        Ok(out)
    }

    // ------------------------------------------------------------------
    // Messages / reactions / retractions
    // ------------------------------------------------------------------

    /// Append a conversation fact event to its scope log + index it. `emit` is
    /// `None` for a fact that must not reach the engine (the ingest path).
    pub(crate) fn apply_conversation_fact(
        &self,
        scope: &ScopeRef,
        event: &Event,
        emit: Option<PostOrigin>,
    ) -> Result<(), StoreError> {
        // Index first: a fact the cache refuses (a reply to a parent this
        // node has not seen, a reaction to nothing) must not land in the log
        // as if it had been accepted.
        self.index_conversation_fact(scope, event)?;
        let path = self.paths.conversation_log(&scope.id)?;
        crate::paths::append_line(&path, &serde_json::to_string(event)?)?;
        if let Some(origin) = emit {
            self.emit_store_event(StoreEvent::ConversationAppended {
                scope: scope.id.clone(),
                event: event.clone(),
                audience: scope.audience.clone(),
                origin,
            });
        }
        Ok(())
    }

    /// Index side effects of one conversation fact (used on write, ingest, rebuild).
    fn index_conversation_fact(&self, scope: &ScopeRef, event: &Event) -> Result<(), StoreError> {
        let author = event.pubkey.to_hex();
        let at = event.created_at.as_secs();
        match event.kind.as_u16() {
            k if k == KIND_MESSAGE => {
                let body: MessageBody = serde_json::from_str(&event.content).map_err(|e| {
                    StoreError::Invalid(bisa_core::text!(
                        "error-store-invalid-message-content-not-body",
                        e = e.to_string()
                    ))
                })?;
                body.validate()?;
                let context_json = match &body {
                    MessageBody::Post { context, .. } => serde_json::to_string(context)?,
                    MessageBody::Membership(_) => "[]".to_string(),
                };
                let idx = self.idx();
                idx.upsert_message(
                    &event.id.to_hex(),
                    scope.kind.as_str(),
                    &scope.id,
                    &author,
                    body.kind(),
                    &body_content(&body),
                    body.thinking(),
                    body.sentence(),
                    &context_json,
                    e_tag(event),
                    at,
                )?;
                idx.index_text(&event.id.to_hex(), &scope.id, &body_content(&body))?;
                for pk in p_tags(event) {
                    idx.upsert_mention(&event.id.to_hex(), &pk, &scope.id, at)?;
                }
                for (ordinal, a) in imeta_tags(event).iter().enumerate() {
                    idx.upsert_attachment(&event.id.to_hex(), ordinal, a)?;
                }
                // The one arm every artifact passes through: a local post, a
                // rebuild from truth and a peer's ingest all index here.
                for (ordinal, a) in body.artifacts().iter().enumerate() {
                    idx.upsert_artifact(&event.id.to_hex(), ordinal, a)?;
                }
                // The feed's row, through the same one arm: a post or a
                // membership change in a channel, a direct message or a
                // conversation, with its excerpt.
                idx.record_activity(&message_activity(scope, event, &body, at, &author))?;
                // A conversation keeps facts beside its record: the counts,
                // the first post's first line, who took part.
                if scope.kind == ScopeKind::Conversation {
                    let mut agents: Vec<String> = Vec::new();
                    if let Some(agent) = idx.agent_id_for_pubkey(&author)? {
                        agents.push(agent);
                    }
                    for pk in p_tags(event) {
                        if let Some(agent) = idx.agent_id_for_pubkey(&pk)? {
                            if !agents.contains(&agent) {
                                agents.push(agent);
                            }
                        }
                    }
                    let first_line = match &body {
                        MessageBody::Post { text, .. } => {
                            text.lines().map(str::trim).find(|l| !l.is_empty())
                        }
                        _ => None,
                    };
                    idx.note_conversation_message(&scope.id, at, first_line, &agents)?;
                }
            }
            k if k == KIND_REACTION => {
                let Some(target) = e_tag(event) else {
                    return Err(StoreError::Invalid(bisa_core::text!(
                        "error-store-invalid-reaction-has-no-e-tag"
                    )));
                };
                let idx = self.idx();
                if !idx.has_reaction(target, &author, &event.content)? {
                    idx.upsert_reaction(
                        &event.id.to_hex(),
                        target,
                        &scope.id,
                        &author,
                        &event.content,
                        at,
                    )?;
                }
            }
            k if k == KIND_RETRACTION => {
                let Some(target) = e_tag(event) else {
                    return Err(StoreError::Invalid(bisa_core::text!(
                        "error-store-invalid-retraction-has-no-e-tag"
                    )));
                };
                let idx = self.idx();
                if let Some(target_author) = idx.fact_author(target)? {
                    if target_author == author {
                        idx.retract_fact(target)?;
                        idx.index_text(target, &scope.id, "")?;
                    } else {
                        return Err(StoreError::Invalid(bisa_core::text!(
                            "error-store-invalid-retraction-author-does-not-match-target-author"
                        )));
                    }
                }
            }
            other => {
                return Err(StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-kind-not-conversation-fact",
                    other = other.to_string()
                )))
            }
        }
        Ok(())
    }

    fn build_fact(&self, scope: &ScopeRef, fact: Fact<'_>) -> Result<Event, StoreError> {
        let Fact {
            kind,
            content,
            e_target,
            mentions,
            attachments,
            signer,
            identity,
        } = fact;
        let (keys, attestation) = match signer {
            Some(agent_id) => self.signer_for(agent_id)?,
            None => (self.owner.clone(), None),
        };
        let mut tags = vec![Tag::parse(["a", &scope.coordinate]).map_err(StoreError::nostr)?];
        if identity == Identity::ItsOwn {
            let once = mint_ulid().to_string();
            tags.push(Tag::parse([ONCE_TAG, once.as_str()]).map_err(StoreError::nostr)?);
        }
        if let Some((id, marker)) = e_target {
            let tag = if marker.is_empty() {
                Tag::parse(["e", id]).map_err(StoreError::nostr)?
            } else {
                Tag::parse(["e", id, "", marker]).map_err(StoreError::nostr)?
            };
            tags.push(tag);
        }
        for m in mentions {
            tags.push(Tag::parse(["p", m.as_hex()]).map_err(StoreError::nostr)?);
        }
        // One `imeta` tag per file, NIP-92/94's shape. The descriptor rides the
        // wire; the bytes stay in the content-addressed store.
        for a in attachments {
            tags.push(
                Tag::parse([
                    "imeta",
                    &format!("x {}", a.sha256),
                    &format!("m {}", a.mime),
                    &format!("size {}", a.size),
                    // Last, because a filename is the one field that can hold a
                    // space.
                    &format!("name {}", a.name),
                ])
                .map_err(StoreError::nostr)?,
            );
        }
        if let Some(auth) = attestation {
            tags.push(auth);
        }
        EventBuilder::new(Kind::from(kind), content)
            .tags(tags)
            .custom_created_at(Timestamp::from_secs(now_secs()))
            .finalize(&keys)
            .map_err(StoreError::nostr)
    }

    /// Post a message into a scope. `signer` = an agent to author as (attested);
    /// `None` = the owner. Returns the event id (hex).
    #[allow(clippy::too_many_arguments)]
    pub fn post_message(
        &self,
        scope: &str,
        body: MessageBody,
        reply_to: Option<String>,
        mentions: &[PrincipalId],
        attachments: &[AttachmentRef],
        signer: Option<&AgentId>,
        origin: PostOrigin,
    ) -> Result<String, StoreError> {
        body.validate()?;
        if let MessageBody::Post {
            text,
            context,
            artifacts,
            ..
        } = &body
        {
            if text.trim().is_empty()
                && attachments.is_empty()
                && context.is_empty()
                && artifacts.is_empty()
            {
                return Err(StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-empty-message"
                )));
            }
        }
        for a in attachments {
            if self.attachment_path(&a.sha256).is_none() {
                return Err(StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-no-attachment-workspace",
                    a0 = (a.sha256).to_string()
                )));
            }
        }
        // An artifact's bytes are an attachment's: posted only once they are
        // here, so a reader on this machine is never shown a title with
        // nothing behind it.
        for a in body.artifacts() {
            if self.attachment_path(&a.sha256).is_none() {
                return Err(StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-no-artifact-bytes-workspace",
                    a0 = (a.sha256).to_string()
                )));
            }
        }
        let scope = self.resolve_scope(scope)?;
        if let Some(parent) = &reply_to {
            let known = self.idx().fact_scope(parent)?;
            if known.as_deref() != Some(scope.id.as_str()) {
                return Err(StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-cannot-reply-not-message-conversation",
                    parent = parent.to_string()
                )));
            }
        }
        if let EventAudience::Restricted(aud) = &scope.audience {
            let author = match signer {
                Some(agent_id) => self.get_agent(agent_id)?.pubkey,
                None => self.owner_principal(),
            };
            if !aud.contains(&author) {
                return Err(StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-author-not-participant-conversation"
                )));
            }
        }
        let content = serde_json::to_string(&body)?;
        let event = self.build_fact(
            &scope,
            Fact {
                kind: KIND_MESSAGE,
                content: &content,
                e_target: reply_to.as_deref().map(|id| (id, "reply")),
                mentions,
                attachments,
                signer,
                identity: match body {
                    MessageBody::Post { .. } => Identity::ItsOwn,
                    MessageBody::Membership(_) => Identity::OfWhatItSays,
                },
            },
        )?;
        self.apply_conversation_fact(&scope, &event, Some(origin))?;
        self.mark_seen(&event)?;
        Ok(event.id.to_hex())
    }

    /// Record a membership change in its channel, as an owner-signed
    /// announcement (never triaged).
    pub fn post_membership(&self, event: MembershipEvent) -> Result<String, StoreError> {
        let scope = event.channel.to_string();
        self.post_message(
            &scope,
            MessageBody::Membership(event),
            None,
            &[],
            &[],
            None,
            PostOrigin::Announced,
        )
    }

    /// The transcript a fresh session is given: the last `limit` messages,
    /// oldest first. The harness holds the rest as its own context and
    /// compacts it itself; the platform summarises nothing.
    pub fn transcript_tail(
        &self,
        scope: &str,
        limit: usize,
    ) -> Result<Vec<MessageRow>, StoreError> {
        let rows = self.idx().transcript_tail(scope, limit)?;
        self.with_attachments(rows)
    }

    /// Messages of a scope, newest-last, paginated backwards from `before`.
    pub fn messages(
        &self,
        scope: &str,
        before: Option<PageBefore>,
        limit: usize,
    ) -> Result<Vec<MessageRow>, StoreError> {
        let rows = self.idx().messages_for(scope, before, limit)?;
        self.with_attachments(rows)
    }

    fn with_attachments(&self, mut rows: Vec<MessageRow>) -> Result<Vec<MessageRow>, StoreError> {
        let ids: Vec<String> = rows.iter().map(|m| m.id.clone()).collect();
        let mut by_message = self.idx().attachments_for(&ids)?;
        let mut artifacts_by_message = self.idx().artifacts_for(&ids)?;
        for row in &mut rows {
            row.attachments = by_message
                .remove(&row.id)
                .unwrap_or_default()
                .into_iter()
                .map(|file| MessageAttachment {
                    present: self.attachment_path(&file.sha256).is_some(),
                    file,
                })
                .collect();
            row.artifacts = artifacts_by_message
                .remove(&row.id)
                .unwrap_or_default()
                .into_iter()
                .map(|artifact| MessageArtifact {
                    present: self.attachment_path(&artifact.sha256).is_some(),
                    artifact,
                })
                .collect();
        }
        Ok(rows)
    }

    /// One message by id, with its files and artifacts and whether each is
    /// here; `None` for an id the index does not know.
    /// One conversation fact as it was signed, by scope and id — the event
    /// a release hands back to dispatch, exactly as it arrived.
    pub fn conversation_event(
        &self,
        scope: &str,
        event_id: &str,
    ) -> Result<Option<nostr::event::Event>, StoreError> {
        let path = self.paths.conversation_log(scope)?;
        let body = match std::fs::read_to_string(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(StoreError::io(path.display().to_string(), e)),
        };
        Ok(body
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| nostr::event::Event::from_json(l).ok())
            .find(|ev| ev.id.to_hex() == event_id))
    }

    pub fn get_message(&self, id: &str) -> Result<Option<MessageRow>, StoreError> {
        let Some(row) = self.idx().message_by_id(id)? else {
            return Ok(None);
        };
        Ok(self.with_attachments(vec![row])?.into_iter().next())
    }

    /// A conversation's artifacts, newest first — its gallery.
    pub fn list_artifacts(
        &self,
        scope: &str,
        limit: usize,
    ) -> Result<Vec<ArtifactListRow>, StoreError> {
        let scope = self.resolve_scope(scope)?;
        let mut rows = self.idx().artifacts_for_scope(&scope.id, limit)?;
        for row in &mut rows {
            row.present = self.attachment_path(&row.artifact.sha256).is_some();
        }
        Ok(rows)
    }

    /// Messages that `p`-tag this pubkey, newest-last.
    pub fn mentions_of(&self, pubkey: &str, limit: usize) -> Result<Vec<MessageRow>, StoreError> {
        let rows = self.idx().mentions_for(pubkey, limit)?;
        self.with_attachments(rows)
    }

    /// Scopes in which this pubkey is mentioned by a live message.
    pub fn mentioned_scopes(&self, pubkey: &str) -> Result<Vec<String>, StoreError> {
        self.idx().scopes_mentioning(pubkey)
    }

    pub fn list_reactions(&self, scope: &str) -> Result<Vec<ReactionRow>, StoreError> {
        self.idx().reactions_for(scope)
    }

    /// Retract one of your own facts (message or reaction). Owner-signed.
    pub fn retract(&self, event_id: &str) -> Result<String, StoreError> {
        let (scope_id, target_author) = {
            let idx = self.idx();
            let scope = idx.fact_scope(event_id)?.ok_or_else(|| {
                StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-unknown-event",
                    event_id = event_id.to_string()
                ))
            })?;
            let author = idx.fact_author(event_id)?.unwrap_or_default();
            (scope, author)
        };
        if target_author != self.owner_principal().as_hex() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-only-author-may-retract-event"
            )));
        }
        let scope = self.resolve_scope(&scope_id)?;
        let event = self.build_fact(
            &scope,
            Fact {
                kind: KIND_RETRACTION,
                content: "",
                e_target: Some((event_id, "")),
                mentions: &[],
                attachments: &[],
                signer: None,
                identity: Identity::OfWhatItSays,
            },
        )?;
        self.apply_conversation_fact(&scope, &event, Some(PostOrigin::Asked))?;
        self.mark_seen(&event)?;
        Ok(event.id.to_hex())
    }

    /// React to a message with an emoji. One live reaction per
    /// `(author, target, emoji)`: reacting again is the same reaction, and
    /// answers its id — a double click writes nothing, and what it answers
    /// can be retracted. `signer` as in [`Self::post_message`].
    pub fn react(
        &self,
        target_event: &str,
        emoji: &str,
        signer: Option<&AgentId>,
    ) -> Result<String, StoreError> {
        if emoji.is_empty() || emoji.len() > 64 {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-emoji-must-be-1-64-bytes"
            )));
        }
        let scope_id = self.idx().fact_scope(target_event)?.ok_or_else(|| {
            StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-unknown-event-2",
                target_event = target_event.to_string()
            ))
        })?;
        let author = match signer {
            Some(agent_id) => self.signer_for(agent_id)?.0.public_key().to_hex(),
            None => self.owner_principal().as_hex().to_string(),
        };
        let standing = self.idx().reaction_id(target_event, &author, emoji)?;
        if let Some(standing) = standing {
            return Ok(standing);
        }
        let scope = self.resolve_scope(&scope_id)?;
        let event = self.build_fact(
            &scope,
            Fact {
                kind: KIND_REACTION,
                content: emoji,
                e_target: Some((target_event, "")),
                mentions: &[],
                attachments: &[],
                signer,
                identity: Identity::OfWhatItSays,
            },
        )?;
        self.apply_conversation_fact(&scope, &event, Some(PostOrigin::Asked))?;
        self.mark_seen(&event)?;
        Ok(event.id.to_hex())
    }

    // ------------------------------------------------------------------
    // Attachments
    // ------------------------------------------------------------------

    /// Take bytes into the content-addressed store and describe them.
    /// Idempotent by content. The size ceiling is checked here so every door
    /// passes through one limit.
    pub fn put_attachment(
        &self,
        bytes: &[u8],
        name: &str,
        mime: &str,
    ) -> Result<AttachmentRef, StoreError> {
        let size = bytes.len() as u64;
        if size == 0 {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-empty-attachment"
            )));
        }
        if size > MAX_ATTACHMENT_BYTES {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-attachment-bytes-limit",
                size = size.to_string(),
                max_attachment_bytes = (MAX_ATTACHMENT_BYTES).to_string()
            )));
        }
        let name = name.trim();
        if name.is_empty() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-attachment-needs-name"
            )));
        }
        let sha256 = hex::encode(sha2::Sha256::digest(bytes));
        let path = self.paths().attachment(&sha256).ok_or_else(|| {
            StoreError::Invalid(bisa_core::text!("error-store-invalid-not-digest"))
        })?;
        if !path.exists() {
            crate::paths::write_atomic(&path, bytes)?;
        }
        Ok(AttachmentRef {
            sha256,
            name: name.to_string(),
            mime: normalize_mime(mime),
            size,
        })
    }

    /// Where a blob is, when this machine has it. `None` is the ordinary state
    /// for a peer's attachment.
    pub fn attachment_path(&self, sha256: &str) -> Option<PathBuf> {
        self.paths().attachment(sha256).filter(|p| p.is_file())
    }

    pub fn attachment_bytes(&self, sha256: &str) -> Result<Option<Vec<u8>>, StoreError> {
        match self.attachment_path(sha256) {
            None => Ok(None),
            Some(path) => std::fs::read(&path)
                .map(Some)
                .map_err(|e| StoreError::io(path.display().to_string(), e)),
        }
    }

    /// The blob under the name its maker gave it, made on demand: a hard link
    /// beside the store, a copy where the filesystem refuses one. Idempotent.
    /// This is the file a file manager reveals and the default application
    /// opens; the blob itself has no extension. Refused for bytes this
    /// machine does not hold and for a name nothing safe is left of.
    pub fn put_attachment_named(&self, sha256: &str, name: &str) -> Result<PathBuf, StoreError> {
        let blob = self.attachment_path(sha256).ok_or_else(|| {
            StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-no-attachment-workspace-2",
                sha256 = sha256.to_string()
            ))
        })?;
        let named = self.paths().attachment_named(sha256, name).ok_or_else(|| {
            StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-not-file-name",
                name = format!("{name:?}")
            ))
        })?;
        if named.is_file() {
            return Ok(named);
        }
        if let Some(dir) = named.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| StoreError::io(dir.display().to_string(), e))?;
        }
        if std::fs::hard_link(&blob, &named).is_err() {
            std::fs::copy(&blob, &named)
                .map_err(|e| StoreError::io(named.display().to_string(), e))?;
        }
        Ok(named)
    }

    /// Store bytes a peer handed us, refusing anything that is not what we
    /// asked for — the check that makes content addressing a security property.
    pub fn accept_attachment(&self, expected_sha256: &str, bytes: &[u8]) -> Result<(), StoreError> {
        let actual = hex::encode(sha2::Sha256::digest(bytes));
        if actual != expected_sha256 {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-attachment-does-not-match-address-asked-got",
                expected_sha256 = expected_sha256.to_string(),
                actual = actual.to_string()
            )));
        }
        let path = self.paths().attachment(&actual).ok_or_else(|| {
            StoreError::Invalid(bisa_core::text!("error-store-invalid-not-digest"))
        })?;
        if !path.exists() {
            crate::paths::write_atomic(&path, bytes)?;
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Read markers (local-only)
    // ------------------------------------------------------------------

    pub fn mark_read(&self, scope: &str) -> Result<(), StoreError> {
        self.idx().set_read_marker(scope, now_secs(), false)?;
        self.emit_store_event(StoreEvent::ReadMarkerSet {
            scope: scope.to_string(),
        });
        Ok(())
    }

    pub fn mark_unread(&self, scope: &str) -> Result<(), StoreError> {
        self.idx().set_read_marker(scope, now_secs(), true)?;
        self.emit_store_event(StoreEvent::ReadMarkerSet {
            scope: scope.to_string(),
        });
        Ok(())
    }

    /// Unread message counts per scope.
    pub fn unread_counts(&self) -> Result<Vec<(String, u64)>, StoreError> {
        self.idx().unread_counts(self.owner_principal().as_hex())
    }

    /// The newest live post of every scope that has one, keyed by scope —
    /// what a list of rooms says of each (`GET /channels`, `GET /dms`). A
    /// room nobody wrote in is absent; its attachments are not read.
    pub fn latest_messages(
        &self,
    ) -> Result<std::collections::HashMap<String, MessageRow>, StoreError> {
        Ok(self
            .idx()
            .latest_messages()?
            .into_iter()
            .map(|m| (m.scope_id.clone(), m))
            .collect())
    }

    /// One scope's latest live post, by the same rule — `None` when nothing
    /// stands as its last words.
    pub fn latest_message(&self, scope: &str) -> Result<Option<MessageRow>, StoreError> {
        self.idx().latest_message(scope)
    }

    // ------------------------------------------------------------------
    // Rebuild support
    // ------------------------------------------------------------------

    /// Re-derive messages and reactions from truth files. Channels were
    /// indexed earlier in the rebuild order; read markers are lost.
    pub(crate) fn reindex_conversation(&self) -> Result<(), StoreError> {
        let dir = self.paths.conversation_dir();
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(StoreError::io(dir.display().to_string(), e)),
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(scope_id) = name.strip_suffix(".jsonl") else {
                continue;
            };
            let Ok(scope) = self.resolve_scope(scope_id) else {
                tracing::warn!("conversation/{name}: unknown scope, skipping");
                continue;
            };
            let content = std::fs::read_to_string(entry.path())
                .map_err(|e| StoreError::io(entry.path().display().to_string(), e))?;
            for line in content.lines().filter(|l| !l.trim().is_empty()) {
                let event: Event = match serde_json::from_str(line) {
                    Ok(ev) => ev,
                    Err(e) => {
                        tracing::warn!("conversation/{name}: bad line, skipping: {e}");
                        continue;
                    }
                };
                if event.verify().is_err() {
                    tracing::warn!("conversation/{name}: unverifiable event, skipping");
                    continue;
                }
                if let Err(e) = self.index_conversation_fact(&scope, &event) {
                    tracing::warn!("conversation/{name}: {e}");
                }
            }
        }
        Ok(())
    }
}

/// Every `imeta` tag on an event, as descriptors. A malformed one is dropped,
/// not guessed at.
fn imeta_tags(event: &Event) -> Vec<AttachmentRef> {
    let mut out = Vec::new();
    for tag in event.tags.iter() {
        let parts = tag.as_slice();
        if parts.first().map(String::as_str) != Some("imeta") {
            continue;
        }
        let (mut sha256, mut mime, mut size, mut name) = (None, None, None, None);
        for field in &parts[1..] {
            let Some((key, value)) = field.split_once(' ') else {
                continue;
            };
            match key {
                "x" => sha256 = Some(value.to_string()),
                "m" => mime = Some(value.to_string()),
                "size" => size = value.parse::<u64>().ok(),
                "name" => name = Some(value.to_string()),
                _ => {}
            }
        }
        let (Some(sha256), Some(mime), Some(size), Some(name)) = (sha256, mime, size, name) else {
            tracing::debug!("dropping a malformed imeta tag on {}", event.id.to_hex());
            continue;
        };
        if !AttachmentRef::is_valid_hash(&sha256) {
            continue;
        }
        out.push(AttachmentRef {
            sha256,
            name,
            mime,
            size,
        });
    }
    out
}

/// The media type as it will be stored: lowercased, parameters dropped, and
/// falling back to the one type that promises nothing.
fn normalize_mime(mime: &str) -> String {
    let head = mime.split(';').next().unwrap_or_default().trim();
    if head.is_empty() || !head.contains('/') {
        return "application/octet-stream".to_string();
    }
    head.to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::MemoryKeyStore;
    use bisa_core::{ContextRef, LineRange, RelPath, RosterPolicy, Tags};

    fn ws() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let ws =
            Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
        (dir, ws)
    }

    #[test]
    fn a_post_carries_its_chips_and_a_conversation_in_a_checkout_is_a_scope() {
        let (_dir, ws) = ws();
        let p = ws
            .create_project(crate::projects::NewProject::managed("web").unwrap())
            .unwrap();
        let conversation = ws
            .create_conversation(crate::conversations::NewConversation {
                origin: bisa_core::ConversationOrigin::Workstream {
                    id: bisa_core::WorkstreamId::from_ulid(p.id.0),
                    project: p.id,
                },
                title: None,
                mode: bisa_core::ConversationMode::Auto,
            })
            .unwrap();
        let scope = conversation.id.to_string();
        let body = MessageBody::Post {
            text: "look at this".into(),
            context: vec![
                ContextRef::Selection {
                    path: RelPath::new("src/main.rs").unwrap(),
                    range: LineRange::new(3, 4).unwrap(),
                    text: "fn main() {}".into(),
                },
                ContextRef::Annotation {
                    page: bisa_core::PageRef::File {
                        path: RelPath::new("www/index.html").unwrap(),
                    },
                    selector: "#save".into(),
                    excerpt: "<button id=\"save\">Save</button>".into(),
                    note: "make it blue".into(),
                },
            ],
            artifacts: vec![],
            thinking: None,
            said: None,
        };
        let id = ws
            .post_message(&scope, body, None, &[], &[], None, PostOrigin::Asked)
            .unwrap();
        let rows = ws.messages(&scope, None, 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, id);
        assert_eq!(rows[0].body_kind, "post");
        assert_eq!(rows[0].content, "look at this");
        assert_eq!(rows[0].context.len(), 2);
        assert_eq!(rows[0].context[0].label(), "src/main.rs:3-4");
        assert_eq!(
            rows[0].context[1].label(),
            "www/index.html · #save",
            "an annotated element rides the same wire"
        );
        assert_eq!(
            ws.resolve_scope(&scope).unwrap().kind,
            ScopeKind::Conversation,
            "a conversation's id is a scope of its own"
        );
        assert!(
            ws.resolve_scope(&p.id.to_string()).is_err(),
            "a project's or a workstream's id is no message scope"
        );
        // Chips alone are a message; nothing at all is not.
        assert!(ws
            .post_message(
                &scope,
                MessageBody::post("  "),
                None,
                &[],
                &[],
                None,
                PostOrigin::Asked
            )
            .is_err());
        // A reply must point into the same conversation.
        assert!(ws
            .post_message(
                &scope,
                MessageBody::post("re"),
                Some("nope".into()),
                &[],
                &[],
                None,
                PostOrigin::Asked
            )
            .is_err());
        ws.post_message(
            &scope,
            MessageBody::post("re"),
            Some(id.clone()),
            &[],
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap();
        assert_eq!(
            ws.messages(&scope, None, 10).unwrap()[1]
                .reply_to
                .as_deref(),
            Some(id.as_str())
        );
    }

    /// A note the platform authors rides its row as the message of the
    /// catalog, so a reader in another language renders the sentence and not
    /// its English; what a person or an agent said carries none — and a
    /// rebuild reads it back from the event.
    #[test]
    fn a_platform_sentence_rides_its_row_and_survives_a_rebuild() {
        let (_dir, ws) = ws();
        let scope = ChannelId::general().to_string();
        let note = bisa_core::text!("guided-say-cycle-failed");
        let said = ws
            .post_message(
                &scope,
                MessageBody::said(note.clone()),
                None,
                &[],
                &[],
                None,
                PostOrigin::Announced,
            )
            .unwrap();
        let spoken = ws
            .post_message(
                &scope,
                MessageBody::post("my own words"),
                None,
                &[],
                &[],
                None,
                PostOrigin::Asked,
            )
            .unwrap();
        let read = |ws: &Workspace| {
            let rows = ws.messages(&scope, None, 10).unwrap();
            let of = |id: &str| rows.iter().find(|m| m.id == id).unwrap().clone();
            (of(&said), of(&spoken))
        };
        let (a, b) = read(&ws);
        assert_eq!(a.said, Some(note.clone()));
        assert_eq!(a.content, note.to_string(), "its English is the content");
        assert_eq!(b.said, None);
        ws.rebuild_index().unwrap();
        let (a, b) = read(&ws);
        assert_eq!(a.said, Some(note));
        assert_eq!(b.said, None);
    }

    #[test]
    fn membership_events_are_announced_in_their_channel() {
        let (_dir, ws) = ws();
        let dev = ws
            .add_agent(crate::agents::NewAgent {
                name: "Dev".into(),
                harness: "mock".into(),
                system_prompt: "x".into(),
                ..Default::default()
            })
            .unwrap();
        let ev = MembershipEvent {
            channel: ChannelId::general(),
            member: bisa_core::Member::Agent(dev.id.clone()),
            change: bisa_core::MembershipChange::Joined,
            cause: bisa_core::MembershipCause::Enabled,
            at: now_secs(),
        };
        let mut rx = ws.subscribe_store_events();
        ws.post_membership(ev).unwrap();
        let rows = ws.messages("general", None, 10).unwrap();
        assert_eq!(rows[0].body_kind, "membership");
        assert!(rows[0].content.contains("joined"));
        let announced = std::iter::from_fn(|| rx.try_recv().ok()).any(|e| {
            matches!(
                e,
                StoreEvent::ConversationAppended {
                    origin: PostOrigin::Announced,
                    ..
                }
            )
        });
        assert!(announced, "a membership event never triages");
    }

    #[test]
    fn mentions_resolve_agents_teams_and_the_channel_handle() {
        let (_dir, ws) = ws();
        let dev = ws
            .add_agent(crate::agents::NewAgent {
                name: "Dev".into(),
                harness: "mock".into(),
                system_prompt: "x".into(),
                ..Default::default()
            })
            .unwrap();
        let team = ws
            .create_team(
                "Eng",
                None,
                vec![bisa_core::Assignee::Agent(dev.id.to_string())],
                Tags::default(),
            )
            .unwrap();
        let c = ws
            .create_channel(
                "room",
                None,
                RosterPolicy::Listed {
                    agents: vec![],
                    teams: vec![team.id.clone()],
                    humans: vec![],
                },
                Tags::default(),
            )
            .unwrap();
        let by_agent = ws.resolve_mentions("room", &[dev.id.to_string()]).unwrap();
        assert_eq!(by_agent, vec![dev.pubkey.clone()]);
        let by_team = ws.resolve_mentions("room", &[team.id.to_string()]).unwrap();
        assert_eq!(by_team, vec![dev.pubkey.clone()]);
        let by_handle = ws.resolve_mentions("room", &[c.id.to_string()]).unwrap();
        assert_eq!(
            by_handle.len(),
            3,
            "dev via the team, and the General Agent and the Workflow Agent, which stand in every room"
        );
        assert!(ws.resolve_mentions("room", &["nobody".into()]).is_err());
        ws.set_agent_enabled(&dev.id, false).unwrap();
        assert!(ws.resolve_mentions("room", &[dev.id.to_string()]).is_err());
    }

    #[test]
    fn attachments_are_content_addressed_and_verified() {
        let (_dir, ws) = ws();
        let a = ws
            .put_attachment(b"hello", "hi.txt", "text/plain; charset=utf-8")
            .unwrap();
        assert_eq!(a.mime, "text/plain");
        let b = ws
            .put_attachment(b"hello", "other.txt", "TEXT/PLAIN")
            .unwrap();
        assert_eq!(a.sha256, b.sha256, "one blob, two descriptors");
        assert_eq!(ws.attachment_bytes(&a.sha256).unwrap().unwrap(), b"hello");
        assert!(ws.accept_attachment(&a.sha256, b"evil").is_err());
        assert!(ws.put_attachment(b"", "x", "text/plain").is_err());
        let id = ws
            .post_message(
                "general",
                MessageBody::post("see"),
                None,
                &[],
                std::slice::from_ref(&a),
                None,
                PostOrigin::Asked,
            )
            .unwrap();
        let rows = ws.messages("general", None, 10).unwrap();
        assert_eq!(rows[0].id, id);
        assert_eq!(rows[0].attachments.len(), 1);
        assert!(rows[0].attachments[0].present);
        assert_eq!(normalize_mime("junk"), "application/octet-stream");
    }

    #[test]
    fn rebuild_restores_conversations() {
        let (_dir, ws) = ws();
        let id = ws
            .post_message(
                "general",
                MessageBody::post("hello"),
                None,
                &[],
                &[],
                None,
                PostOrigin::Asked,
            )
            .unwrap();
        ws.react(&id, "👍", None).unwrap();
        ws.rebuild_index().unwrap();
        assert_eq!(ws.messages("general", None, 10).unwrap().len(), 1);
        assert_eq!(ws.list_reactions("general").unwrap().len(), 1);
        ws.retract(&id).unwrap();
        assert!(ws.messages("general", None, 10).unwrap()[0].retracted);
    }
}
