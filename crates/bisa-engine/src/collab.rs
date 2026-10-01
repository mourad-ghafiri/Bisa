//! Collaboration on the host's side (14-collaboration): what the engine does
//! about the people on other nodes this workspace hosts.
//!
//! Three things, each a seam the security features hook:
//!
//! - **A message from outside is read before an agent hears it.** The store
//!   applies it and says so (`StoreEvent::RemoteMessageArrived`); this module
//!   holds it, puts its redacted text to the classifier with the message
//!   brief, and either releases it into the ordinary dispatch — every session
//!   it wakes working *for* that person — or keeps it held with the reason,
//!   for the owner to release by hand. Off (`security.collaboration.classify`),
//!   it dispatches at once.
//! - **A message that leaves the machine is redacted before it is signed**
//!   ([`scope_leaves_the_machine`], read by `messaging::post_as_person`).
//! - **Invitations and people** are the owner's acts through the store —
//!   made here with the code the person carries, admitted or refused by hand
//!   in `ask` mode, promoted, demoted, removed — and every change is said on
//!   the bus for the screens. The host's pump (`bisa-net`) hears the same
//!   store events and tells the people.

use crate::conversation::dispatch;
use crate::events::{EngineEvent, EnginePayload};
use crate::{EngineError, Inner};
use bisa_collab::{hash_secret, mint_secret, InviteCode};
use bisa_core::{
    reaches, ChannelId, CollabSettings, Invite, InviteId, InviteState, MemberRole, MessageBody,
    PrincipalId, SyncSettings, WorkspaceMember,
};
use bisa_security::classify::MessageSubject;
use bisa_security::ClassifierVerdict;
use bisa_store::{Admission, HeldReason, PostOrigin, StoreEvent};
use std::sync::Arc;

/// The most of a message the classifier is shown.
pub const MAX_CLASSIFIED_CHARS: usize = 4000;

/// Whether a post into `scope` reaches a person on another node — a
/// standing channel some person reaches, or a direct message with one.
pub fn scope_leaves_the_machine(inner: &Arc<Inner>, scope: &str) -> bool {
    let Ok(people) = inner.ws.people() else {
        return false;
    };
    if people.is_empty() {
        return false;
    }
    let Some(channel) = ChannelId::new(scope)
        .ok()
        .and_then(|id| inner.ws.get_channel(&id).ok())
    else {
        return false;
    };
    people.iter().any(|p| reaches(&channel, p.role, &p.pubkey))
}

/// The words the classifier and a log line use for a person: the key's
/// first letters and the label, never the key whole.
fn person_words(inner: &Inner, pubkey: &PrincipalId) -> String {
    let short = format!("{}…", &pubkey.as_hex()[..8]);
    match inner.ws.member(pubkey).ok().flatten().and_then(|m| m.label) {
        Some(label) => format!("{short} ({label})"),
        None => short,
    }
}

/// A message from a person on another node landed: read it first, then let
/// the agents hear it — or not.
pub async fn on_remote_message(
    inner: &Arc<Inner>,
    scope: String,
    event: nostr::event::Event,
    author: PrincipalId,
    role: MemberRole,
) {
    let policy = inner.security.policy();
    if !policy.collaboration.classify {
        dispatch(
            inner,
            &scope,
            &event,
            PostOrigin::Asked,
            Some((author, role)),
        );
        return;
    }
    let id = event.id.to_hex();
    if let Err(e) = inner
        .ws
        .hold_message(&id, &scope, author.clone(), HeldReason::Pending)
    {
        tracing::warn!("cannot hold {id}: {e}");
        return;
    }
    announce_held(inner, &scope, &id, &author, HeldReason::Pending);
    let text = serde_json::from_str::<MessageBody>(&event.content)
        .ok()
        .and_then(|b| match b {
            MessageBody::Post { text, .. } => Some(text),
            _ => None,
        })
        .unwrap_or_default();
    let bounded: String = text.chars().take(MAX_CLASSIFIED_CHARS).collect();
    let subject = MessageSubject {
        author: person_words(inner, &author),
        role: role.as_str().to_string(),
        scope: scope.clone(),
        text: inner.security.redact(&bounded).text,
    };
    match classify_message(inner, &subject).await {
        Ok(ClassifierVerdict::Safe) => {
            // The verdict took time; the person may have been removed in it.
            // Their removal dropped the held row, and a member who is gone
            // wakes nobody — the role they hold *now* is what a dispatch
            // carries, never the one they arrived with.
            match inner.ws.release_message(&id) {
                Ok(Some(_)) => {}
                Ok(None) => {
                    tracing::info!(target: "bisa_engine::collab", event = %id, author = %person_words(inner, &author), "a held message was judged safe after its author was removed; nobody hears it");
                    return;
                }
                Err(e) => {
                    tracing::warn!(target: "bisa_engine::collab", event = %id, "cannot release: {e}");
                    return;
                }
            }
            let Some(role) = current_role(inner, &author, &id) else {
                return;
            };
            inner.emit(EngineEvent::global(EnginePayload::MessageReleased {
                scope: scope.clone(),
                event: id,
            }));
            dispatch(
                inner,
                &scope,
                &event,
                PostOrigin::Asked,
                Some((author, role)),
            );
        }
        Ok(ClassifierVerdict::Harmful { reason }) => {
            keep_held(
                inner,
                &scope,
                &id,
                author,
                HeldReason::Harmful { why: reason },
            );
        }
        Err(e) => {
            keep_held(
                inner,
                &scope,
                &id,
                author,
                HeldReason::NoVerdict { why: e.to_string() },
            );
        }
    }
}

fn keep_held(inner: &Arc<Inner>, scope: &str, id: &str, author: PrincipalId, reason: HeldReason) {
    // A person removed while the verdict was pending took their held row
    // with them; holding it again would leave a row nobody can act on.
    if current_role(inner, &author, id).is_none() {
        return;
    }
    if let Err(e) = inner
        .ws
        .hold_message(id, scope, author.clone(), reason.clone())
    {
        tracing::warn!(target: "bisa_engine::collab", event = %id, "cannot hold: {e}");
        return;
    }
    // Read once more after the hold: a removal that ran between the check
    // above and the write (`remove_person` takes the member row first) left
    // this row behind, and it is dropped here rather than kept for nobody.
    if current_role(inner, &author, id).is_none() {
        if let Err(e) = inner.ws.drop_held_of(&author) {
            tracing::warn!(target: "bisa_engine::collab", event = %id, "cannot drop a held row of a person who left: {e}");
        }
        return;
    }
    announce_held(inner, scope, id, &author, reason);
}

/// The role a person holds now, or `None` — said in the log — when they
/// are no longer a member: what was theirs to say is nobody's to deliver.
fn current_role(inner: &Inner, author: &PrincipalId, event: &str) -> Option<MemberRole> {
    match inner.ws.member_role(author) {
        Ok(Some(role)) => Some(role),
        Ok(None) => {
            tracing::info!(target: "bisa_engine::collab", event, author = %person_words(inner, author), "the author is no longer a member; the message is not delivered");
            None
        }
        Err(e) => {
            tracing::warn!(target: "bisa_engine::collab", event, "the author's role could not be read; the message is not delivered: {e}");
            None
        }
    }
}

fn announce_held(
    inner: &Arc<Inner>,
    scope: &str,
    id: &str,
    author: &PrincipalId,
    reason: HeldReason,
) {
    inner.emit(EngineEvent::global(EnginePayload::MessageHeld {
        scope: scope.to_string(),
        event: id.to_string(),
        author: author.clone(),
        reason,
    }));
}

/// Ask the classifier about a message from outside — the classifier's own
/// agent, deadline and cache, with the message brief.
pub async fn classify_message(
    inner: &Inner,
    subject: &MessageSubject,
) -> Result<ClassifierVerdict, EngineError> {
    let policy = inner.security.policy();
    if !policy.classifier.enabled {
        return Err(EngineError::Security("the classifier is off".into()));
    }
    crate::classifier::classify_message(inner, subject, &policy.classifier).await
}

/// The owner lets a held message through: the agents it addressed hear it
/// now, as if it had just arrived.
pub fn release_message(inner: &Arc<Inner>, event_id: &str) -> Result<(), EngineError> {
    let Some(held) = inner.ws.release_message(event_id)? else {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-no-held-message",
            event_id = event_id.to_string()
        )));
    };
    inner.emit(EngineEvent::global(EnginePayload::MessageReleased {
        scope: held.scope.clone(),
        event: event_id.to_string(),
    }));
    // The held row is gone either way; a person who left since is told
    // about in a sentence, and nothing is delivered for them.
    let Some(role) = inner.ws.member_role(&held.author)? else {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-author-no-longer-member-message-not-delivered",
            event_id = event_id.to_string()
        )));
    };
    if let Some(event) = inner.ws.conversation_event(&held.scope, event_id)? {
        dispatch(
            inner,
            &held.scope,
            &event,
            PostOrigin::Asked,
            Some((held.author, role)),
        );
    }
    Ok(())
}

// -- invitations ---------------------------------------------------------

/// Make an invite: the record the host keeps and the code the person
/// carries — the secret exists in the code and nowhere else.
pub fn create_invite(
    inner: &Arc<Inner>,
    role: MemberRole,
    channels: Vec<ChannelId>,
    label: Option<String>,
) -> Result<(Invite, InviteCode), EngineError> {
    let resolved = inner.ws.settings(None)?;
    let collab = CollabSettings::from_resolved(&resolved);
    let sync = SyncSettings::from_resolved(&resolved);
    let label = label.map(|l| inner.security.redact_inbound(&l, "invite"));
    let secret = mint_secret();
    let invite = inner.ws.create_invite(
        role,
        channels,
        label,
        collab.invite_ttl_hours.saturating_mul(3600),
        hash_secret(&secret),
    )?;
    let code = InviteCode::new(inner.ws.owner_keys().public_key(), &sync.relays, secret);
    Ok((invite, code))
}

pub fn revoke_invite(inner: &Arc<Inner>, id: InviteId) -> Result<Invite, EngineError> {
    Ok(inner.ws.revoke_invite(id)?)
}

/// The owner's word on a claim that waited: admit the person at the
/// invite's role and channels, or refuse them. The host's pump tells them.
pub fn settle_invite(inner: &Arc<Inner>, id: InviteId, admit: bool) -> Result<Invite, EngineError> {
    let invite = inner.ws.invite(id)?;
    let (by, label) = match &invite.state {
        InviteState::Requested { by, label, .. } => (by.clone(), label.clone()),
        other => {
            return Err(EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-invite-not-waiting-you",
                id = id.to_string(),
                a0 = (other.as_str()).to_string()
            )))
        }
    };
    let settled = inner.ws.settle_invite(id, admit)?;
    if admit {
        inner.ws.admit_claimed(&settled, &by, label, None)?;
    }
    Ok(settled)
}

// -- people --------------------------------------------------------------

pub fn set_role(
    inner: &Arc<Inner>,
    pubkey: &PrincipalId,
    role: MemberRole,
) -> Result<WorkspaceMember, EngineError> {
    Ok(inner.ws.set_role(pubkey, role)?)
}

/// Remove a person: off every roster, their held messages dropped, their
/// row gone. What they already hold of this workspace cannot be un-sent.
pub fn remove_person(inner: &Arc<Inner>, pubkey: &PrincipalId) -> Result<(), EngineError> {
    // The member row goes first, the held rows after: a verdict landing in
    // between (`keep_held`) reads the membership again after it holds, and a
    // row held for a person already gone is dropped there — whichever side
    // of `drop_held_of` it landed on.
    inner.ws.remove_member(pubkey)?;
    inner.ws.drop_held_of(pubkey)?;
    inner.ws.unroster_human_everywhere(pubkey)?;
    Ok(())
}

/// The owner's own profile — their label, their face — as `PUT /workspace/me`
/// sets it (14-collaboration): each part kept when `None`, cleared when
/// `Some(None)`; the label redacted as an invite's is. The store says
/// `ProfileChanged`, which the pump carries to every host this node is a
/// guest of.
pub fn set_profile(
    inner: &Arc<Inner>,
    label: Option<Option<String>>,
    photo: Option<Option<bisa_core::AttachmentRef>>,
) -> Result<WorkspaceMember, EngineError> {
    let label = label.map(|l| l.map(|l| inner.security.redact_inbound(&l, "profile")));
    Ok(inner
        .ws
        .set_member_profile(&inner.ws.owner_principal(), label, photo)?)
}

/// Admit a person by hand, without an invite — the owner typed a pubkey.
pub fn add_person(
    inner: &Arc<Inner>,
    pubkey: PrincipalId,
    role: MemberRole,
    label: Option<String>,
) -> Result<WorkspaceMember, EngineError> {
    let label = label.map(|l| inner.security.redact_inbound(&l, "invite"));
    inner.ws.add_member(
        pubkey.clone(),
        role,
        Admission {
            label,
            photo: None,
            invited_by: Some(inner.ws.owner_principal()),
            client: None,
        },
    )?;
    inner.ws.member(&pubkey)?.ok_or_else(|| {
        EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-was-not-admitted",
            pubkey = pubkey.to_string()
        ))
    })
}

// -- the bus -------------------------------------------------------------

/// Relay the store's word about people and invitations to the screens.
pub fn spawn_listener(inner: &Arc<Inner>) -> tokio::task::JoinHandle<()> {
    let mut rx = inner.ws.subscribe_store_events();
    let inner = Arc::clone(inner);
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(StoreEvent::PeopleChanged { pubkey, change }) => {
                    let label = inner
                        .ws
                        .member(&pubkey)
                        .ok()
                        .flatten()
                        .and_then(|m| m.label);
                    inner.emit(EngineEvent::global(EnginePayload::PeopleChanged {
                        pubkey,
                        change,
                        label,
                    }));
                }
                Ok(StoreEvent::InviteChanged { invite }) => {
                    inner.emit(EngineEvent::global(EnginePayload::InviteChanged { invite }));
                }
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("collaboration listener lagged by {n} events");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    })
}
