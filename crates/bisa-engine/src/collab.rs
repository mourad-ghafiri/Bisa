//! Collaboration on the host's side (14-collaboration): what the engine does
//! about the people on other nodes this workspace hosts.
//!
//! Three things, each a seam the security features hook:
//!
//! - **A message from outside is read before an agent hears it.** The store
//!   holds it as it applies it and says so
//!   (`StoreEvent::RemoteMessageArrived`); this module puts its redacted
//!   text to the classifier with the message brief, and either releases it
//!   into the ordinary dispatch — every session it wakes working *for* that
//!   person — or keeps it held with the reason, for the owner to release by
//!   hand. Off (`security.collaboration.classify`), it is let through at
//!   once. While it is held — and for good once refused — it is in nothing
//!   an agent reads (`conversation::heard_tail`). The owner's word on a
//!   message stands whatever the classifier says after it
//!   (`restate_held`), and a message left *being read* when the node
//!   stopped is read when it starts ([`resume_pending`]).
//! - **A message that leaves the machine is redacted before it is signed**
//!   ([`scope_leaves_the_machine`], read by `messaging::post_as_person`).
//! - **Invitations and people** are the owner's acts through the store —
//!   made here with the code the person carries, admitted or refused by hand
//!   in `ask` mode, promoted, demoted, removed — and every change is said on
//!   the bus for the screens. The host's pump (`bisa-net`) hears the same
//!   store events and tells the people.

use crate::conversation::{dispatch, dispatch_released};
use crate::events::{EngineEvent, EnginePayload};
use crate::{EngineError, Inner};
use bisa_collab::{hash_secret, mint_secret, InviteCode};
use bisa_core::{
    reaches, ChannelId, CollabSettings, Invite, InviteId, MemberRole, MessageBody, PrincipalId,
    SyncSettings, WorkspaceMember,
};
use bisa_security::classify::MessageSubject;
use bisa_security::ClassifierVerdict;
use bisa_store::HeldMessage;
use bisa_store::{Admission, HeldReason, PostOrigin, StoreEvent};
use std::sync::Arc;

/// The most of a message the classifier is shown — the words and every
/// context chip they carry, as the agent would read them: the content
/// screen's own bound. Longer is no verdict, held with the reason: what the
/// classifier did not read must not reach an agent as read.
pub const MAX_CLASSIFIED_CHARS: usize = crate::content::MAX_SCREENED_CHARS;

/// Whether a post into `scope` reaches a person on another node — a
/// standing channel some person reaches, or a direct message with one.
pub fn scope_leaves_the_machine(inner: &Arc<Inner>, scope: &str) -> bool {
    let Ok(people) = inner.ws.people() else {
        // LCOV_EXCL_START: the people file unreadable; a disk arm
        return false;
        // LCOV_EXCL_STOP
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
    let id = event.id.to_hex();
    if !policy.collaboration.classify {
        // Nothing reads it first: the row it arrived under goes, and it is
        // heard at once.
        if let Err(e) = inner.ws.release_message(&id) {
            // LCOV_EXCL_START: the held file unreadable; a disk arm
            tracing::warn!(target: "bisa_engine::collab", event = %id, "cannot let a message through: {e}");
            return;
            // LCOV_EXCL_STOP
        }
        dispatch(
            inner,
            &scope,
            &event,
            PostOrigin::Asked,
            Some((author, role)),
        );
        return;
    }
    match inner
        .ws
        .hold_message(&id, &scope, author.clone(), HeldReason::Pending)
    {
        Ok(Some(_)) => {}
        // The owner refused it already: nobody reads it, nobody hears it.
        Ok(None) => return,
        // LCOV_EXCL_START: the held file unwritable; a disk arm
        Err(e) => {
            // Not heard, and said so: logged alone, the message sat in its
            // room with no caution on it and no way to let it through.
            tracing::warn!(target: "bisa_engine::collab", event = %id, "cannot hold: {e}");
            announce_held(
                inner,
                &scope,
                &id,
                &author,
                HeldReason::NoVerdict { why: e.to_string() },
            );
            return;
        } // LCOV_EXCL_STOP
    }
    announce_held(inner, &scope, &id, &author, HeldReason::Pending);
    let (text, context) = serde_json::from_str::<MessageBody>(&event.content)
        .ok()
        .and_then(|b| match b {
            MessageBody::Post { text, context, .. } => Some((text, context)),
            _ => None,
        })
        .unwrap_or_default();
    // What an agent would read: the words and every chip they carry — a
    // selection's text, a diff, a terminal's tail — whole, or not at all:
    // a message past the bound is held as no verdict, never read in part.
    let whole = crate::framing::follow_up_text(text, &context);
    if whole.chars().count() > MAX_CLASSIFIED_CHARS {
        keep_held(
            inner,
            &scope,
            &id,
            author,
            HeldReason::NoVerdict {
                why: bisa_core::text!(
                    "engine-collab-held-longer-than-screen",
                    max = MAX_CLASSIFIED_CHARS as u64
                )
                .to_string(),
            },
        );
        return;
    }
    let subject = MessageSubject {
        author: person_words(inner, &author),
        role: role.as_str().to_string(),
        scope: scope.clone(),
        text: inner.security.redact(&whole).text,
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
                // LCOV_EXCL_START: the held file unreadable at the release; a disk arm
                Err(e) => {
                    tracing::warn!(target: "bisa_engine::collab", event = %id, "cannot release: {e}");
                    return;
                } // LCOV_EXCL_STOP
            }
            let Some(role) = current_role(inner, &author, &id) else {
                // LCOV_EXCL_START: the author removed between the judgement and the release; a race no test can time
                return;
                // LCOV_EXCL_STOP
            };
            inner.emit(EngineEvent::global(EnginePayload::MessageReleased {
                scope: scope.clone(),
                event: id,
            }));
            dispatch_released(inner, &scope, &event, (author, role));
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

/// The classifier's word on a message that waits for it. A row the owner
/// let through or refused meanwhile is theirs, and one whose author was
/// removed went with them: nothing is held again under the id
/// (`restate_held` never makes a row), and nothing is said.
fn keep_held(inner: &Arc<Inner>, scope: &str, id: &str, author: PrincipalId, reason: HeldReason) {
    match inner.ws.restate_held(id, reason.clone()) {
        Ok(Some(_)) => announce_held(inner, scope, id, &author, reason),
        Ok(None) => {
            tracing::info!(target: "bisa_engine::collab", event = %id, "a verdict landed on a message already answered, or whose author left; it changes nothing");
        }
        // LCOV_EXCL_START: the held file unwritable under the verdict; a disk arm
        Err(e) => {
            tracing::warn!(target: "bisa_engine::collab", event = %id, "cannot restate a hold: {e}")
        } // LCOV_EXCL_STOP
    }
}

/// The role a person holds now, or `None` — said in the log — when they
/// are no longer a member: what was theirs to say is nobody's to deliver.
fn current_role(inner: &Inner, author: &PrincipalId, event: &str) -> Option<MemberRole> {
    match inner.ws.member_role(author) {
        Ok(Some(role)) => Some(role),
        // LCOV_EXCL_START: the author removed between the judgement and the release; a race no test can time
        Ok(None) => {
            tracing::info!(target: "bisa_engine::collab", event, author = %person_words(inner, author), "the author is no longer a member; the message is not delivered");
            None
        }
        // LCOV_EXCL_STOP
        // LCOV_EXCL_START: the members file unreadable; a disk arm
        Err(e) => {
            tracing::warn!(target: "bisa_engine::collab", event, "the author's role could not be read; the message is not delivered: {e}");
            None
        } // LCOV_EXCL_STOP
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
        return Err(EngineError::Security(
            bisa_core::text!("engine-collab-classifier-off").to_string(),
        ));
    }
    crate::classifier::classify_message(inner, subject, &policy.classifier).await
}

/// The owner lets a held message through: the agents it addressed hear it
/// now, handed that message — not whatever is newest in its room.
///
/// A release that can deliver nothing says why, and never says *released*:
/// when its author is no longer a member, or its room is gone, the message
/// is refused instead — out of what any agent reads, for good.
pub fn release_message(inner: &Arc<Inner>, event_id: &str) -> Result<(), EngineError> {
    let no_such = || {
        EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-no-held-message",
            event_id = event_id.to_string()
        ))
    };
    let held = inner
        .ws
        .held_messages()?
        .into_iter()
        .find(|h| h.event_id == event_id)
        .ok_or_else(no_such)?;
    let role = inner.ws.member_role(&held.author)?;
    let event = inner.ws.conversation_event(&held.scope, event_id)?;
    let (Some(role), Some(event)) = (role, event) else {
        refuse_message(inner, event_id)?;
        return Err(EngineError::Invalid(if role.is_none() {
            bisa_core::text!(
                "error-engine-invalid-author-no-longer-member-message-not-delivered",
                event_id = event_id.to_string()
            )
        } else {
            bisa_core::text!(
                "error-engine-invalid-room-gone-message-not-delivered",
                event_id = event_id.to_string()
            )
        }));
    };
    // The row is the gate: whoever takes it delivers, once.
    if inner.ws.release_message(event_id)?.is_none() {
        // LCOV_EXCL_START: the row taken between the read and the release; a race no test can time
        return Err(no_such());
        // LCOV_EXCL_STOP
    }
    inner.emit(EngineEvent::global(EnginePayload::MessageReleased {
        scope: held.scope.clone(),
        event: event_id.to_string(),
    }));
    dispatch_released(inner, &held.scope, &event, (held.author, role));
    Ok(())
}

/// The owner refuses a held message: the row is gone and no agent ever
/// hears or reads it — the message stays in its scope's log, as every
/// message does. Nothing is dispatched; the fact is said so the Pulse and
/// the People panel move. A message nobody holds is refused by name.
pub fn refuse_message(inner: &Arc<Inner>, event_id: &str) -> Result<HeldMessage, EngineError> {
    let Some(held) = inner.ws.refuse_held(event_id)? else {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-no-held-message",
            event_id = event_id.to_string()
        )));
    };
    inner.emit(EngineEvent::global(EnginePayload::MessageRefused {
        scope: held.scope.clone(),
        event: event_id.to_string(),
        author: held.author.clone(),
    }));
    Ok(held)
}

/// At start: every message left *being read* when the node stopped — or
/// taken in by a pump with no engine beside it — is read now, one at a
/// time. Nothing read them again: the row said *being read* for ever, the
/// owner's *Let through* waited on a verdict nobody was fetching, and the
/// message was heard by no one. One whose author has left since, or whose
/// room is gone, is refused. With the classifier off for messages from
/// outside they are let through, as they would have been.
pub async fn resume_pending(inner: Arc<Inner>) {
    let waiting: Vec<HeldMessage> = match inner.ws.held_messages() {
        Ok(rows) => rows
            .into_iter()
            .filter(|h| h.reason == HeldReason::Pending)
            .collect(),
        // LCOV_EXCL_START: the held file unreadable at start; a disk arm
        Err(e) => {
            tracing::warn!(target: "bisa_engine::collab", "the held messages could not be read at start: {e}");
            return;
        } // LCOV_EXCL_STOP
    };
    for held in waiting {
        let role = inner.ws.member_role(&held.author).ok().flatten();
        let event = inner
            .ws
            .conversation_event(&held.scope, &held.event_id)
            .ok()
            .flatten();
        match (role, event) {
            (Some(role), Some(event)) => {
                on_remote_message(&inner, held.scope, event, held.author, role).await;
            }
            _ => {
                if let Err(e) = refuse_message(&inner, &held.event_id) {
                    // LCOV_EXCL_START: the row gone or the file unwritable under the start; a race, a disk arm
                    tracing::debug!(target: "bisa_engine::collab", event = %held.event_id, "a row nobody can answer was not dropped: {e}");
                    // LCOV_EXCL_STOP
                }
            }
        }
    }
}

/// A room deleted, or a message taken back by its author: what was held
/// there, or of it, is nobody's to let through any more.
fn refuse_what_is_moot(inner: &Arc<Inner>, moot: impl Fn(&HeldMessage) -> bool) {
    let held = match inner.ws.held_messages() {
        Ok(held) => held,
        Err(_) => return, // LCOV_EXCL_LINE: the held file unreadable under the listener; a disk arm
    };
    for row in held.into_iter().filter(|h| moot(h)) {
        if let Err(e) = refuse_message(inner, &row.event_id) {
            // LCOV_EXCL_START: the row answered meanwhile; a race no test can time
            tracing::debug!(target: "bisa_engine::collab", event = %row.event_id, "a moot hold was not dropped: {e}");
            // LCOV_EXCL_STOP
        }
    }
}

// -- invitations ---------------------------------------------------------

/// Make an invite: the record the host keeps and the code the person
/// carries — the secret exists in the code and nowhere else.
///
/// A role that is not named is the workspace's (`collab.default_role`) —
/// read here, so the route, the verb and the dialog agree. Each said *guest*
/// for itself, and the setting was read by nothing.
pub fn create_invite(
    inner: &Arc<Inner>,
    role: Option<MemberRole>,
    channels: Vec<ChannelId>,
    label: Option<String>,
) -> Result<(Invite, InviteCode), EngineError> {
    let resolved = inner.ws.settings(None)?;
    let collab = CollabSettings::from_resolved(&resolved);
    let role = role.unwrap_or(collab.default_role);
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

/// Every invitation, newest first. One whose time ran out since it was last
/// read is marked expired here — and said, once, so the screens and the
/// Pulse hear an expiry that otherwise happened to nobody.
pub fn invites(inner: &Arc<Inner>) -> Result<Vec<Invite>, EngineError> {
    inner.ws.mark_invites_expired()?;
    Ok(inner.ws.invites()?)
}

pub fn revoke_invite(inner: &Arc<Inner>, id: InviteId) -> Result<Invite, EngineError> {
    Ok(inner.ws.revoke_invite(id)?)
}

/// The owner's word on a claim that waited: admit the person at the
/// invite's role and channels, or refuse them — the store's one admission,
/// whole under the people's writer. The host's pump tells them.
pub fn settle_invite(inner: &Arc<Inner>, id: InviteId, admit: bool) -> Result<Invite, EngineError> {
    Ok(if admit {
        inner.ws.admit_request(id)?.0
    } else {
        inner.ws.refuse_request(id)?
    })
}

// -- people --------------------------------------------------------------

pub fn set_role(
    inner: &Arc<Inner>,
    pubkey: &PrincipalId,
    role: MemberRole,
) -> Result<WorkspaceMember, EngineError> {
    Ok(inner.ws.set_role(pubkey, role)?)
}

/// Remove a person: their row gone, their held messages dropped, off every
/// roster — the store's one removal, the row first, so a verdict landing
/// meanwhile (`keep_held`) finds nobody to hold a message for. What they
/// already hold of this workspace cannot be un-sent.
pub fn remove_person(inner: &Arc<Inner>, pubkey: &PrincipalId) -> Result<(), EngineError> {
    let theirs: Vec<HeldMessage> = inner
        .ws
        .held_messages()
        .unwrap_or_default()
        .into_iter()
        .filter(|h| &h.author == pubkey)
        .collect();
    inner.ws.remove_person(pubkey)?;
    // What was held of theirs went with them — said, so a list of held
    // messages on screen does not keep rows nobody can answer.
    for held in theirs {
        inner.emit(EngineEvent::global(EnginePayload::MessageRefused {
            scope: held.scope,
            event: held.event_id,
            author: held.author,
        }));
    }
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
        // LCOV_EXCL_START: `add_member` above wrote the row this reads; by construction
        EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-was-not-admitted",
            pubkey = pubkey.to_string()
        ))
    })
    // LCOV_EXCL_STOP
}

// -- the relays ----------------------------------------------------------

/// One change to the list of relays (`sync.relays`, machine scope).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelayEdit {
    Add(String),
    Remove(String),
    /// Back to the relays a fresh node is given.
    Reset,
}

/// What an edit of the relays did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelaysEdited {
    /// Whether the list moved: a relay already configured is added by
    /// nothing, one not configured removed by nothing.
    pub changed: bool,
    pub relays: Vec<String>,
}

/// One writer of the list at a time, in this process.
static RELAY_EDITS: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Add a relay, remove one, or go back to the defaults — on the list as it
/// stands, under one lock. Three places each read the list, changed their
/// copy and wrote it back whole: two adds at once kept one. A relay is one
/// relay whatever its spelling (`bisa_core::same_relay`); the spelling a
/// person typed is what is kept. The write is a setting's, so whoever
/// listens — the pump, the screens — hears it move.
pub fn edit_relays(inner: &Arc<Inner>, edit: RelayEdit) -> Result<RelaysEdited, EngineError> {
    use bisa_core::collab_settings::keys::SYNC_RELAYS;
    let _one_writer = RELAY_EDITS.lock().unwrap_or_else(|e| e.into_inner());
    let mut relays = SyncSettings::from_resolved(&inner.ws.settings(None)?).relays;
    let before = relays.clone();
    match edit {
        RelayEdit::Add(url) => {
            let url = url.trim().to_string();
            if !bisa_core::is_relay_url(&url) {
                return Err(EngineError::Invalid(bisa_core::text!(
                    "error-engine-invalid-relay-ws-wss-url"
                )));
            }
            if !relays.iter().any(|r| bisa_core::same_relay(r, &url)) {
                relays.push(url);
            }
        }
        RelayEdit::Remove(url) => relays.retain(|r| !bisa_core::same_relay(r, url.trim())),
        RelayEdit::Reset => {
            crate::settings::unset(inner, bisa_core::SettingScope::Machine, None, SYNC_RELAYS)?;
            let relays = SyncSettings::from_resolved(&inner.ws.settings(None)?).relays;
            return Ok(RelaysEdited {
                changed: relays != before,
                relays,
            });
        }
    }
    let changed = relays != before;
    if changed {
        crate::settings::set(
            inner,
            bisa_core::SettingScope::Machine,
            None,
            SYNC_RELAYS,
            serde_json::json!(relays),
        )?;
    }
    Ok(RelaysEdited { changed, relays })
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
                Ok(StoreEvent::DefinitionRemoved { kind, d: id })
                    if kind == bisa_core::kind::KIND_CHANNEL =>
                {
                    refuse_what_is_moot(&inner, |h| h.scope == id);
                }
                Ok(StoreEvent::RemoteFactArrived { event, .. })
                    if event.kind.as_u16() == bisa_core::kind::KIND_RETRACTION =>
                {
                    let taken_back: Vec<String> = event
                        .tags
                        .iter()
                        .map(|t| t.as_slice())
                        .filter(|t| t.len() >= 2 && t[0] == "e")
                        .map(|t| t[1].clone())
                        .collect();
                    refuse_what_is_moot(&inner, |h| taken_back.contains(&h.event_id));
                }
                Ok(_) => {}
                // LCOV_EXCL_START: the bus lagging or closing under the listener; a race no test can time
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("collaboration listener lagged by {n} events");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                // LCOV_EXCL_STOP
            }
        }
    }) // LCOV_EXCL_LINE: the task ends with the bus; a race no test can time
}
