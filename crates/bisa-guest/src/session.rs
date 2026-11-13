//! A guest session: this person, on this node, as a member of one workspace
//! hosted elsewhere — and the supervisor that keeps every such membership.
//!
//! The session speaks the protocol (`bisa-collab`) and nothing else: a
//! `Join` out, the host's words in, this person's signed facts out over the
//! relay pool, the host's relayed facts into the replica ([`GuestStore`]).
//! What this person may do is what the host said they reach — the channels
//! in the replica are the channels the role reaches, and a post into any
//! other is refused here before the host would refuse it there.
//!
//! Two security seams live on this side too: the built-in redactor runs
//! over every text before it is signed (a secret pasted into a hosted
//! channel leaves this machine as a placeholder), and a fact is kept only
//! when the host's seal carried it and its author is someone the host's
//! directory names.

use crate::faces::FaceStore;
use crate::store::{scope_of, GuestStore, Hosted, HostedMessage, HostedState};
use crate::GuestError;
use bisa_collab::{
    wrap_control, wrap_for_member, Control, Directory, Face, HostCard, Incoming, InviteCode, Relays,
};
use bisa_core::kind::{KIND_CHANNEL, KIND_MESSAGE, KIND_REACTION, KIND_RETRACTION};
use bisa_core::sync::Locked;
use bisa_core::{Channel, ChannelKind, MemberRole, MessageBody, Permission, PrincipalId};
use bisa_security::redact::{Redactor, Vault};
use nostr::event::{Event, EventBuilder, FinalizeEvent, Kind, Tag};
use nostr::key::{Keys, PublicKey};
use nostr::types::Timestamp;
use std::collections::{HashMap, HashSet};

/// The fact an event points at: its first `e` tag.
fn e_target(event: &Event) -> Option<&str> {
    event.tags.iter().find_map(|t| {
        let s = t.as_slice();
        (s.len() >= 2 && s[0] == "e").then(|| s[1].as_str())
    })
}
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{broadcast, RwLock};

/// How long a join waits for the host's first word before it is reported
/// as still waiting. The membership stays `Requested` either way; a
/// `Welcome` that arrives later is applied like any other.
pub fn join_wait() -> Duration {
    std::env::var("BISA_JOIN_WAIT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(30))
}

/// The name of the directory under the data dir holding every replica.
pub const HOSTS_DIR: &str = "hosts";

/// What a session says changed, for the screens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostedChange {
    /// The membership's state or role moved (welcomed, waiting, refused,
    /// removed, promoted).
    Membership,
    Channels,
    Members,
    /// A fact landed in a scope.
    Message {
        scope: String,
        id: String,
    },
}

/// A fact this person signed and sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Posted {
    pub id: String,
    pub scope: String,
    /// Secrets the redactor replaced before signing.
    pub redacted: usize,
    /// No relay took it yet: it is kept in this person's copy, marked as
    /// not sent, and sent on its own when one answers.
    pub queued: bool,
}

/// This person as a member of one host.
pub struct GuestSession {
    keys: Keys,
    host: PublicKey,
    store: GuestStore,
    relays: Arc<Relays>,
    redactor: Redactor,
    vault: Vault,
    changed: broadcast::Sender<HostedChange>,
    /// Where faces land and the owner's profile is read (`FaceStore`).
    faces: Arc<dyn FaceStore>,
    /// The faces asked of the host and not yet here, so a welcome and the
    /// directory that follows do not ask twice. An ask no relay took is
    /// taken off again ([`Self::not_asked`]), and each new directory starts
    /// the set afresh: a face asked for and never answered is asked again.
    asked: Mutex<HashSet<String>>,
    /// When a leave was last said again to a host that keeps talking to a
    /// membership this person left ([`Self::leave_again`]).
    left_said: Mutex<Option<std::time::Instant>>,
}

/// How often a leave is said again to a host that has not heard it.
const LEAVE_AGAIN_EVERY: Duration = Duration::from_secs(60);
/// A word stamped further ahead than this is no word of now: a clock that
/// far off would pin its mark and shut out every word after it.
const AHEAD_SLACK_SECS: u64 = 24 * 3600;
/// How often the unsent are tried again while there are any.
const UNSENT_EVERY: Duration = Duration::from_secs(15);

/// The rank of a membership word among words of one second: the order a
/// host says them in (`Heard::takes_membership`) — a claim waits, is
/// refused or welcomed, and a member is removed. A refusal ranks under a
/// welcome: a claim refused and made again is answered twice in a second,
/// the welcome last.
fn membership_rank(control: &Control) -> u8 {
    match control {
        Control::Waiting => 0,
        Control::Refused { .. } => 1,
        Control::Welcome { .. } => 2,
        _ => 3,
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl GuestSession {
    /// Open the replica of `host` under `<data_dir>/hosts/` for `keys`.
    pub fn open(
        keys: Keys,
        host: PublicKey,
        data_dir: &Path,
        relays: Arc<Relays>,
        faces: Arc<dyn FaceStore>,
    ) -> Result<Arc<Self>, GuestError> {
        let store = GuestStore::open(&data_dir.join(HOSTS_DIR), &host)?;
        let (redactor, _) = Redactor::compile(&bisa_security::builtin::redact_rules(), &|_| None);
        let (changed, _) = broadcast::channel(256);
        Ok(Arc::new(Self {
            keys,
            host,
            store,
            relays,
            redactor,
            vault: Vault::new(format!("guest:{}", host.to_hex())),
            changed,
            faces,
            asked: Mutex::new(HashSet::new()),
            left_said: Mutex::new(None),
        }))
    }

    pub fn host(&self) -> PublicKey {
        self.host
    }

    pub fn host_hex(&self) -> String {
        self.host.to_hex()
    }

    pub fn me(&self) -> PrincipalId {
        PrincipalId::new(self.keys.public_key().to_hex()).expect("a pubkey is a principal")
    }

    pub fn store(&self) -> &GuestStore {
        &self.store
    }

    /// What changed, as it changes.
    pub fn subscribe(&self) -> broadcast::Receiver<HostedChange> {
        self.changed.subscribe()
    }

    fn say(&self, change: HostedChange) {
        // Nobody listening is not an error: a screen reads the replica when
        // it opens.
        if self.changed.send(change).is_err() {
            tracing::trace!("a change with no listener");
        }
    }

    // -- membership ---------------------------------------------------------

    pub fn hosted(&self) -> Result<Option<Hosted>, GuestError> {
        self.store.hosted()
    }

    fn membership(&self) -> Result<Hosted, GuestError> {
        match self.store.hosted()? {
            Some(h) if h.state.is_member() => Ok(h),
            Some(h) => Err(GuestError::Refused(format!(
                "you are not a member of this workspace ({})",
                h.state.words()
            ))),
            None => Err(GuestError::NotHosted),
        }
    }

    async fn send_control(&self, control: &Control) -> Result<(), GuestError> {
        let wrap = wrap_control(&self.keys, self.host, control)?;
        self.relays.publish(&wrap).await?;
        Ok(())
    }

    /// This person's profile as a word to the host: the label, the face by
    /// reference, and its bytes when this node holds them.
    pub fn profile_control(&self) -> Control {
        let profile = self.faces.profile();
        let face = profile
            .photo
            .as_ref()
            .and_then(|p| self.faces.bytes(&p.sha256))
            .map(|bytes| Face::from_bytes(&bytes));
        Control::Profile {
            label: profile.label,
            photo: profile.photo,
            face,
        }
    }

    /// Say this person's profile to the host — after a welcome, and whenever
    /// it changes on this node. Nothing while not a member.
    pub async fn send_profile(&self) -> Result<(), GuestError> {
        self.membership()?;
        self.send_control(&self.profile_control()).await
    }

    /// The faces the directory names and this node lacks, each asked once.
    fn faces_wanted(&self, members: &[Directory]) -> Vec<Control> {
        let mut asked = self.asked.locked();
        // A directory is the host's word on who is here now: what was asked
        // under the last one and never came is asked again under this one.
        asked.clear();
        members
            .iter()
            .filter_map(|m| m.photo.as_ref())
            .filter(|p| self.faces.bytes(&p.sha256).is_none())
            .filter(|p| asked.insert(p.sha256.clone()))
            .map(|p| Control::WantFace {
                sha256: p.sha256.clone(),
            })
            .collect()
    }

    /// An ask the host did not hear is no ask: the face is wanted still.
    pub fn not_asked(&self, sha256: &str) {
        self.asked.locked().remove(sha256);
    }

    /// Claim `code` with this key: record the request, send the `Join`, and
    /// wait up to [`join_wait`] for the host's first word. The state
    /// returned is the membership's as it stands then — `Member` on a
    /// welcome, `Requested` while the host has not answered or admits by
    /// hand, `Refused` with the reason.
    pub async fn join(
        &self,
        code: &InviteCode,
        label: Option<String>,
        client: &str,
    ) -> Result<HostedState, GuestError> {
        if code.host.as_hex() != self.host.to_hex() {
            return Err(GuestError::Refused("this code is for another host".into()));
        }
        // A person is no guest of their own workspace: the code they made
        // is somebody else's to paste. Sent, it reached their own host pump
        // and left a copy of their own workspace listed among the hosts.
        if self.host == self.keys.public_key() {
            return Err(GuestError::Refused(
                "this is your own workspace's invitation — it is for the person you invite".into(),
            ));
        }
        // A member has nothing to claim: the membership stands as it is, and
        // no claim goes out that a spent code would have answered with a
        // refusal — which, written over the record, unseated the member on
        // their own node while the host still counted them.
        if let Some(standing) = self.store.hosted()? {
            if matches!(standing.state, HostedState::Member) {
                return Ok(standing.state);
            }
        }
        let label = label
            .map(|l| self.redactor.redact(&self.vault, l.trim()).text)
            .filter(|l| !l.is_empty())
            .map(|l| l.chars().take(64).collect::<String>());
        // The record first, then the claim: the host's answer may land
        // before this returns, and a word for a membership nobody recorded
        // is dropped. What stood is kept beside it — the host's name among
        // it — and put back when no relay takes the claim: *waiting for the
        // host* over a claim that never left waited on an answer to nothing.
        let stood = self.store.hosted()?;
        self.store.set_hosted(&Hosted {
            host: HostCard {
                pubkey: code.host.clone(),
                name: stood
                    .as_ref()
                    .map(|h| h.host.name.clone())
                    .unwrap_or_default(),
                relays: code.relays.clone(),
            },
            role: MemberRole::Guest,
            state: HostedState::Requested,
            requested_at: now_secs(),
            joined_at: None,
            label: label.clone(),
        })?;
        self.say(HostedChange::Membership);
        let mut heard = self.subscribe();
        if let Err(not_sent) = self
            .send_control(&Control::Join {
                secret: code.secret.clone(),
                label,
                client: client.to_string(),
            })
            .await
        {
            match &stood {
                Some(before) => self.store.set_hosted(before)?,
                None => self.store.unset_hosted()?,
            }
            self.say(HostedChange::Membership);
            return Err(not_sent);
        }
        let deadline = tokio::time::Instant::now() + join_wait();
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break; // LCOV_EXCL_LINE the wait already spent at its first check; a race
            }
            match tokio::time::timeout(remaining, heard.recv()).await {
                Ok(Ok(HostedChange::Membership)) => {
                    if let Some(h) = self.store.hosted()? {
                        if !matches!(h.state, HostedState::Requested) {
                            return Ok(h.state);
                        }
                    } // LCOV_EXCL_LINE a membership word that leaves the claim waiting; a race
                }
                Ok(Ok(_)) => {} // LCOV_EXCL_LINE a change of another kind arriving under the wait; a race
                Ok(Err(broadcast::error::RecvError::Lagged(_))) => {} // LCOV_EXCL_LINE the stream lagged under the wait; a race
                Ok(Err(broadcast::error::RecvError::Closed)) | Err(_) => break,
            }
        }
        Ok(self
            .store
            .hosted()?
            .map(|h| h.state)
            .unwrap_or(HostedState::Requested))
    }

    /// Leave: tell the host, keep the replica marked as left. The host may
    /// be unreachable right now; leaving is this person's act and stands
    /// either way — and a host that did not hear it is told again whenever
    /// it next speaks to this membership ([`Self::leave_again`]).
    pub async fn leave(&self) -> Result<(), GuestError> {
        let Some(mut hosted) = self.store.hosted()? else {
            return Err(GuestError::NotHosted);
        };
        if hosted.state.is_member() || matches!(hosted.state, HostedState::Requested) {
            match self.send_control(&Control::Leave).await {
                Ok(()) => *self.left_said.locked() = Some(std::time::Instant::now()),
                // Not said: the host's next word to this membership is
                // answered with it, with no minute to wait out.
                Err(e) => tracing::debug!("the host did not hear the leave: {e}"),
            }
        } // LCOV_EXCL_LINE the other edge of the branch; by construction
        hosted.state = HostedState::Left;
        self.store.set_hosted(&hosted)?;
        self.say(HostedChange::Membership);
        Ok(())
    }

    /// The leave, said again — at most once in [`LEAVE_AGAIN_EVERY`] — when
    /// the host speaks to a membership this person left: it did not hear the
    /// first, and keeps relaying to somebody who is gone. `removed` is its
    /// answer, and the one word that is not answered.
    fn leave_again(&self) -> Vec<Control> {
        let mut last = self.left_said.locked();
        if last.is_some_and(|at| at.elapsed() < LEAVE_AGAIN_EVERY) {
            return Vec::new();
        }
        *last = Some(std::time::Instant::now());
        vec![Control::Leave]
    }

    // -- reads --------------------------------------------------------------

    pub fn channels(&self) -> Result<Vec<Channel>, GuestError> {
        self.store.channels_of_kind(ChannelKind::Standing)
    }

    pub fn dms(&self) -> Result<Vec<Channel>, GuestError> {
        self.store.channels_of_kind(ChannelKind::Direct)
    }

    pub fn members(&self) -> Result<Vec<Directory>, GuestError> {
        self.store.members()
    }

    /// A channel's messages — one the person reaches: a channel the host
    /// never named is refused as a post into it is, never answered as an
    /// empty room that hides what was not sent.
    pub fn messages(
        &self,
        scope: &str,
        before: Option<u64>,
        limit: usize,
    ) -> Result<Vec<HostedMessage>, GuestError> {
        self.reachable(scope)?;
        self.store.messages(scope, before, limit)
    }

    pub fn unread(&self, scope: &str) -> Result<usize, GuestError> {
        self.store.unread(scope, &self.me())
    }

    pub fn mark_read(&self, scope: &str) -> Result<(), GuestError> {
        let at = self.store.latest_at(scope)?.unwrap_or_else(now_secs);
        self.store.mark_read(scope, at)
    }

    // -- writes -------------------------------------------------------------

    fn coordinate(&self, scope: &str) -> String {
        format!("{KIND_CHANNEL}:{}:{scope}", self.host.to_hex())
    }

    /// A channel this person reaches, by the host's word.
    fn reachable(&self, scope: &str) -> Result<Channel, GuestError> {
        self.store
            .channel(scope)?
            .ok_or_else(|| GuestError::Refused(format!("you do not reach {scope} on this host")))
    }

    fn build_fact(
        &self,
        kind: u16,
        content: &str,
        scope: &str,
        e_target: Option<(&str, &str)>,
        mentions: &[PrincipalId],
    ) -> Result<Event, GuestError> {
        let mut tags = vec![Tag::parse(["a", &self.coordinate(scope)])
            .map_err(|e| GuestError::Store(e.to_string()))?];
        if let Some((id, marker)) = e_target {
            let tag = if marker.is_empty() {
                Tag::parse(["e", id])
            } else {
                Tag::parse(["e", id, "", marker])
            };
            tags.push(tag.map_err(|e| GuestError::Store(e.to_string()))?);
        }
        for m in mentions {
            tags.push(Tag::parse(["p", m.as_hex()]).map_err(|e| GuestError::Store(e.to_string()))?);
        }
        EventBuilder::new(Kind::from(kind), content)
            .tags(tags)
            .custom_created_at(Timestamp::from_secs(now_secs()))
            .finalize(&self.keys)
            .map_err(|e| GuestError::Store(e.to_string()))
    }

    /// Keep a fact of this person's, then send it to the host. Kept first:
    /// it is theirs the moment it is signed. A fact no relay takes is noted
    /// as not sent and answered as *queued* — it stays in the thread, marked,
    /// and is sent on its own when a relay answers ([`Self::flush_unsent`]).
    /// Answered as an error it stood in the thread as sent, was never sent
    /// again, and sending again made a second.
    async fn commit(&self, event: Event, redacted: usize) -> Result<Posted, GuestError> {
        let scope = scope_of(&event).unwrap_or_default();
        let id = event.id.to_hex();
        self.store.append(&event)?;
        self.store.mark_unsent(&id)?;
        self.say(HostedChange::Message {
            scope: scope.clone(),
            id: id.clone(),
        });
        // In the order they were said: what waited goes first.
        let queued = self.flush_unsent().await? > 0;
        Ok(Posted {
            id,
            scope,
            redacted,
            queued,
        })
    }

    /// Send what no relay has taken, oldest first, stopping at the first
    /// that still finds none. Answers how many wait on.
    pub async fn flush_unsent(&self) -> Result<usize, GuestError> {
        let waiting = self.store.unsent()?;
        for (i, id) in waiting.iter().enumerate() {
            let Some(event) = self.store.event(id)? else {
                // Its room is gone from the copy: nothing is owed for it.
                self.store.clear_unsent(id)?;
                continue;
            };
            let wrap = wrap_for_member(&self.keys, self.host, &event)?;
            if let Err(e) = self.relays.publish(&wrap).await {
                tracing::debug!("{id} is not sent yet: {e}");
                return Ok(waiting.len() - i);
            }
            self.store.clear_unsent(id)?;
            self.say(HostedChange::Message {
                scope: scope_of(&event).unwrap_or_default(),
                id: id.clone(),
            });
        }
        Ok(0)
    }

    /// Whether anything of this person's waits for a relay.
    pub fn has_unsent(&self) -> bool {
        self.store.unsent().map(|u| !u.is_empty()).unwrap_or(false)
    }

    /// Post text into a channel this person reaches. Mentions are pubkeys
    /// the host's directory names (or the host's agents, whose keys the
    /// screen learned from the host's roster words). The text passes the
    /// built-in redactor before it is signed.
    pub async fn post(
        &self,
        scope: &str,
        text: &str,
        mentions: &[PrincipalId],
        reply_to: Option<&str>,
    ) -> Result<Posted, GuestError> {
        let hosted = self.membership()?;
        if !hosted.role.may(Permission::PostInChannels) {
            // LCOV_EXCL_START no role lacks posting today; by construction
            return Err(GuestError::Refused(format!(
                "a {} may not post here",
                hosted.role.as_str()
            )));
            // LCOV_EXCL_STOP
        }
        self.reachable(scope)?;
        let redaction = self.redactor.redact(&self.vault, text.trim());
        if redaction.text.is_empty() {
            return Err(GuestError::Refused("an empty message".into()));
        }
        // The host's bound, held before anything is kept or sent: a message
        // past it was written into this copy as sent, and the host — which
        // refuses it — never had it.
        if redaction.text.len() > bisa_core::MAX_TEXT_BYTES {
            return Err(GuestError::Refused(format!(
                "a message is at most {} KiB; send a longer text as a file",
                bisa_core::MAX_TEXT_BYTES / 1024
            )));
        }
        if let Some(parent) = reply_to {
            if !self.store.has_seen(parent) {
                return Err(GuestError::Refused(format!(
                    "cannot reply to {parent}: it is not a message you hold"
                )));
            }
        }
        let body = MessageBody::post(redaction.text);
        let content = serde_json::to_string(&body)?;
        let event = self.build_fact(
            KIND_MESSAGE,
            &content,
            scope,
            reply_to.map(|id| (id, "reply")),
            mentions,
        )?;
        self.commit(event, redaction.count).await
    }

    /// React to a message this person holds.
    pub async fn react(&self, event_id: &str, emoji: &str) -> Result<Posted, GuestError> {
        self.membership()?;
        if emoji.is_empty() || emoji.len() > 64 {
            return Err(GuestError::Refused("emoji must be 1..=64 bytes".into()));
        }
        let target = self
            .store
            .event(event_id)?
            .ok_or_else(|| GuestError::Refused(format!("unknown message {event_id}")))?;
        let scope = scope_of(&target).unwrap_or_default();
        // One live reaction per person, message and emoji, as on the host:
        // pressing it again answers the one that stands — which is what can
        // be taken back — and writes nothing.
        if let Some(standing) = self.standing_reaction(&scope, event_id, emoji)? {
            return Ok(Posted {
                id: standing,
                scope,
                redacted: 0,
                queued: false,
            });
        }
        let event = self.build_fact(KIND_REACTION, emoji, &scope, Some((event_id, "")), &[])?;
        self.commit(event, 0).await
    }

    /// This person's live reaction to a message with one emoji, by its id.
    fn standing_reaction(
        &self,
        scope: &str,
        target: &str,
        emoji: &str,
    ) -> Result<Option<String>, GuestError> {
        let me = self.keys.public_key();
        let events = self.store.events(scope)?;
        let taken_back: HashSet<&str> = events
            .iter()
            .filter(|e| e.kind.as_u16() == KIND_RETRACTION)
            .filter_map(|e| e_target(e))
            .collect();
        Ok(events
            .iter()
            .filter(|e| e.kind.as_u16() == KIND_REACTION && e.pubkey == me && e.content == emoji)
            .filter(|e| e_target(e) == Some(target))
            .map(|e| e.id.to_hex())
            .find(|id| !taken_back.contains(id.as_str())))
    }

    /// Take back one of this person's own facts.
    pub async fn retract(&self, event_id: &str) -> Result<Posted, GuestError> {
        self.membership()?;
        let target = self
            .store
            .event(event_id)?
            .ok_or_else(|| GuestError::Refused(format!("unknown message {event_id}")))?;
        if target.pubkey != self.keys.public_key() {
            return Err(GuestError::Refused(
                "only the author may retract a message".into(),
            ));
        }
        let scope = scope_of(&target).unwrap_or_default();
        let event = self.build_fact(KIND_RETRACTION, "", &scope, Some((event_id, "")), &[])?;
        self.commit(event, 0).await
    }

    /// Ask the host for a direct channel with these people; the host answers
    /// with the channels this person reaches, the new one among them.
    pub async fn open_dm(&self, participants: &[PrincipalId]) -> Result<(), GuestError> {
        let hosted = self.membership()?;
        if !hosted.role.may(Permission::OpenDms) {
            // LCOV_EXCL_START no role lacks direct messages today; by construction
            return Err(GuestError::Refused(format!(
                "a {} may not open direct messages",
                hosted.role.as_str()
            )));
            // LCOV_EXCL_STOP
        }
        let known = self.store.members()?;
        let me = self.me();
        let mut wanted: Vec<PrincipalId> = Vec::new();
        for p in participants {
            if *p == me || wanted.contains(p) {
                continue;
            }
            if !known.iter().any(|m| &m.pubkey == p) {
                return Err(GuestError::Refused(format!(
                    "{p} is not a person of this workspace"
                )));
            }
            wanted.push(p.clone());
        }
        if wanted.is_empty() {
            return Err(GuestError::Refused(
                "a direct message needs at least one other person".into(),
            ));
        }
        self.send_control(&Control::OpenDm {
            participants: wanted,
        })
        .await
    }

    // -- inbound ------------------------------------------------------------

    /// Apply one thing the relay pool heard, if it is this host's; answers
    /// what to say back — this person's profile after a welcome, the faces
    /// the directory names and this node lacks, a leave the host did not
    /// hear — for the router to send.
    pub fn on_incoming(&self, incoming: &Incoming) -> Result<Vec<Control>, GuestError> {
        match incoming {
            Incoming::Control {
                sender,
                control,
                at,
            } if *sender == self.host => self.on_control(control, *at),
            Incoming::Gep { sender, event } if *sender == self.host => self.on_fact(event),
            _ => Ok(Vec::new()),
        }
    }

    /// One of the host's words, said at `at` by the host's clock.
    ///
    /// **The newest word of its kind stands.** A relay hands words back in
    /// any order — after a restart, every one it kept — so each is weighed
    /// against when the newest of its kind was said ([`Heard`]) and an older
    /// one changes nothing: an old welcome re-seated nobody who left or was
    /// removed, an old *waiting* unseats no member (C10), an old role, list
    /// of rooms or directory puts nothing back. A welcome speaks to all
    /// four kinds, each part against its own mark. The record is written
    /// first, the mark second: a stop between the two costs a word applied
    /// twice, never one lost.
    fn on_control(&self, control: &Control, at: u64) -> Result<Vec<Control>, GuestError> {
        let mut hosted = match self.store.hosted()? {
            Some(h) => h,
            None => return Ok(Vec::new()),
        };
        if at > now_secs().saturating_add(AHEAD_SLACK_SECS) {
            // LCOV_EXCL_START a tracing event's field, evaluated under a subscriber only
            tracing::warn!(
                "the host's {} is stamped {at}, more than a day ahead of this clock — dropped",
                control.kind()
            );
            // LCOV_EXCL_STOP
            return Ok(Vec::new());
        }
        let left = matches!(hosted.state, HostedState::Left);
        // The host's answer to a leave, and the one word a membership this
        // person left does not answer.
        if left && matches!(control, Control::Removed { .. }) {
            return Ok(Vec::new());
        }
        // Whatever else it says to somebody who left, it did not hear the
        // leave: it is told again, and nothing of the word is applied.
        if left {
            return Ok(self.leave_again());
        }
        let mut heard = self.store.heard()?;
        let mut replies = Vec::new();
        let rank = membership_rank(control);
        match control {
            Control::Welcome {
                workspace,
                role,
                members,
                channels,
            } => {
                let was_member = hosted.state.is_member();
                // Everything is written before anything is said: a join that
                // waits on *membership* reads the rooms the moment it hears.
                let mut said = Vec::new();
                if heard.takes_membership(at, rank) {
                    hosted.host = workspace.clone();
                    hosted.state = HostedState::Member;
                    // When they were first let in; a welcome said again is
                    // the host's standing word, not a second joining.
                    hosted.joined_at = hosted.joined_at.or(Some(now_secs()));
                    if at >= heard.role {
                        hosted.role = *role;
                        heard.role = at;
                    }
                    self.store.set_hosted(&hosted)?;
                    heard.membership = (at, rank);
                    said.push(HostedChange::Membership);
                }
                // A welcome that is not the newest membership word carries
                // no newer role either: a member's role mark is at least
                // its welcome's.
                if hosted.state.is_member() {
                    if at >= heard.members {
                        self.store.set_members(members)?;
                        heard.members = at;
                        said.push(HostedChange::Members);
                        replies.extend(self.faces_wanted(members));
                    }
                    if at >= heard.channels {
                        self.rooms_named(channels)?;
                        heard.channels = at;
                    }
                }
                self.store.set_heard(&heard)?;
                for change in said {
                    self.say(change);
                }
                // A member now, and not before: the host hears who this
                // person is. A welcome said again to a member asks nothing.
                if !was_member && hosted.state.is_member() {
                    replies.insert(0, self.profile_control());
                }
            }
            Control::Waiting => {
                self.stands(&mut hosted, &mut heard, (at, rank), HostedState::Requested)?;
            }
            // A refusal answers a claim; a member made none, so a refusal
            // that reaches one changes nothing.
            Control::Refused { .. } if matches!(hosted.state, HostedState::Member) => {}
            Control::Refused { reason } => {
                let refused = HostedState::Refused {
                    reason: reason.clone(),
                };
                self.stands(&mut hosted, &mut heard, (at, rank), refused)?;
            }
            Control::Removed { reason } => {
                let removed = HostedState::Removed {
                    reason: reason.clone(),
                };
                self.stands(&mut hosted, &mut heard, (at, rank), removed)?;
            }
            Control::RoleChanged { role } => {
                if at >= heard.role {
                    hosted.role = *role;
                    self.store.set_hosted(&hosted)?;
                    heard.role = at;
                    self.store.set_heard(&heard)?;
                    self.say(HostedChange::Membership);
                }
            }
            Control::ChannelsChanged { channels } => {
                if at >= heard.channels {
                    self.rooms_named(channels)?;
                    heard.channels = at;
                    self.store.set_heard(&heard)?;
                }
            }
            Control::MembersChanged { members } => {
                if at >= heard.members {
                    self.store.set_members(members)?;
                    heard.members = at;
                    self.store.set_heard(&heard)?;
                    self.say(HostedChange::Members);
                    replies.extend(self.faces_wanted(members));
                }
            }
            Control::Face { face } => match face.decode() {
                Ok(bytes) => {
                    self.asked.locked().remove(&face.sha256);
                    match self.faces.hold(&face.sha256, &bytes) {
                        Ok(()) => self.say(HostedChange::Members),
                        Err(e) => tracing::warn!("keeping a face from the host: {e}"), // LCOV_EXCL_LINE the memory faces never refuse a hold; by construction
                    }
                }
                Err(refusal) => tracing::info!("the host sent a face that is not one: {refusal}"),
            },
            // A member's words, or nothing a guest acts on.
            Control::Join { .. }
            | Control::OpenDm { .. }
            | Control::Leave
            | Control::IrohAddr { .. }
            | Control::Profile { .. }
            | Control::WantFace { .. } => {}
        }
        Ok(replies)
    }

    /// A membership word that is the newest of its kind stands: the state
    /// is written, then its mark, then the screens hear. An older one
    /// changes nothing.
    fn stands(
        &self,
        hosted: &mut Hosted,
        heard: &mut crate::store::Heard,
        said: (u64, u8),
        state: HostedState,
    ) -> Result<(), GuestError> {
        if !heard.takes_membership(said.0, said.1) {
            return Ok(());
        }
        hosted.state = state;
        self.store.set_hosted(hosted)?;
        heard.membership = said;
        self.store.set_heard(heard)?;
        self.say(HostedChange::Membership);
        Ok(())
    }

    /// The rooms as the host names them now — and the facts that arrived
    /// before their room was named, applied now that it is.
    fn rooms_named(&self, channels: &[Channel]) -> Result<(), GuestError> {
        self.store.set_channels(channels)?;
        self.say(HostedChange::Channels);
        for event in self.store.take_parked()? {
            self.keep(&event)?;
        }
        Ok(())
    }

    /// A fact the host relayed: verified, of this host's workspace, in a
    /// room this person reaches — whoever wrote it. The host's seal is the
    /// authority (C8): it admitted the author before it relayed a word of
    /// theirs, and the fact's own signature is checked here. Held to the
    /// directory besides, a newcomer never saw what somebody who has since
    /// left had written.
    fn on_fact(&self, event: &Event) -> Result<Vec<Control>, GuestError> {
        if !matches!(
            event.kind.as_u16(),
            k if k == KIND_MESSAGE || k == KIND_REACTION || k == KIND_RETRACTION
        ) {
            return Ok(Vec::new());
        }
        match self.store.hosted()?.map(|h| h.state) {
            Some(HostedState::Member) => {}
            // The host relays to somebody who left: it did not hear it.
            Some(HostedState::Left) => return Ok(self.leave_again()),
            _ => return Ok(Vec::new()),
        }
        if event.verify().is_err() {
            // LCOV_EXCL_START a fact the wire did not sign; out by decision
            tracing::debug!("a relayed fact with a bad signature — dropped");
            return Ok(Vec::new());
            // LCOV_EXCL_STOP
        }
        // A fact names the workspace it was said in. One sealed by this
        // host that names another — what its owner said, as a person, in a
        // workspace that hosts them — is not this host's room's.
        if !names_workspace(event, &self.host) {
            tracing::debug!("a relayed fact of another workspace — not this host's; dropped");
            return Ok(Vec::new());
        }
        let Some(scope) = scope_of(event) else {
            return Ok(Vec::new());
        };
        if self.store.channel(&scope)?.is_none() {
            // Before the word that names its room: kept for it.
            self.store.park(event)?;
            return Ok(Vec::new());
        }
        self.keep(event)?;
        Ok(Vec::new())
    }

    fn keep(&self, event: &Event) -> Result<(), GuestError> {
        if self.store.append(event)? {
            self.say(HostedChange::Message {
                scope: scope_of(event).unwrap_or_default(),
                id: event.id.to_hex(),
            });
        }
        Ok(())
    }
}

/// Whether a fact's coordinate, `33405:<owner>:<scope>`, names `owner`'s
/// workspace.
fn names_workspace(event: &Event, owner: &PublicKey) -> bool {
    event.tags.iter().any(|t| {
        let s = t.as_slice();
        s.len() >= 2 && s[0] == "a" && s[1].split(':').nth(1) == Some(owner.to_hex().as_str())
    })
}

/// Forward one session's changes onto a supervisor's stream, keyed by host.
fn forward_changes(tx: broadcast::Sender<(String, HostedChange)>, session: &Arc<GuestSession>) {
    let host = session.host_hex();
    let mut rx = session.subscribe();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(change) => {
                    if tx.send((host.clone(), change)).is_err() {
                        tracing::trace!("a hosted change with no listener");
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue, // LCOV_EXCL_LINE the stream lagged under the forward; a race
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

/// Every hosted membership on this node, and the one place the relay
/// pool's inbound stream is routed to them by the seal's sender.
pub struct Guests {
    keys: Keys,
    data_dir: PathBuf,
    relays: Arc<Relays>,
    faces: Arc<dyn FaceStore>,
    sessions: RwLock<HashMap<String, Arc<GuestSession>>>,
    changed: broadcast::Sender<(String, HostedChange)>,
}

impl Guests {
    /// Open every replica under `<data_dir>/hosts/`, every session keeping
    /// faces in and reading its profile from `faces`.
    pub fn open(
        keys: Keys,
        data_dir: &Path,
        relays: Arc<Relays>,
        faces: Arc<dyn FaceStore>,
    ) -> Result<Arc<Self>, GuestError> {
        let (changed, _) = broadcast::channel(256);
        let mut sessions = HashMap::new();
        for host_hex in GuestStore::list(&data_dir.join(HOSTS_DIR)) {
            let Ok(host) = PublicKey::from_hex(&host_hex) else {
                continue; // LCOV_EXCL_LINE the listing keeps keys alone; by construction
            };
            match GuestSession::open(
                keys.clone(),
                host,
                data_dir,
                Arc::clone(&relays),
                Arc::clone(&faces),
            ) {
                Ok(session) => {
                    forward_changes(changed.clone(), &session);
                    sessions.insert(host_hex, session);
                }
                Err(e) => tracing::warn!("cannot open the replica of {host_hex}: {e}"), // LCOV_EXCL_LINE a replica whose folder cannot be read; disk-only
            }
        }
        Ok(Arc::new(Self {
            keys,
            data_dir: data_dir.to_path_buf(),
            relays,
            faces,
            sessions: RwLock::new(sessions),
            changed,
        }))
    }

    /// This person's profile changed on this node: every host hears it.
    pub async fn announce_profile(&self) {
        let sessions: Vec<Arc<GuestSession>> =
            self.sessions.read().await.values().cloned().collect();
        for session in sessions {
            if let Err(e) = session.send_profile().await {
                // LCOV_EXCL_START a tracing event's field, evaluated under a subscriber only
                tracing::debug!(
                    "the host {} did not hear the profile: {e}",
                    session.host_hex()
                );
                // LCOV_EXCL_STOP
            }
        }
    }

    /// Forward one session's changes onto the supervisor's stream, keyed by
    /// host.
    fn relay_changes(&self, session: &Arc<GuestSession>) {
        forward_changes(self.changed.clone(), session);
    }

    /// Route the relay pool's inbound stream to the sessions, for as long
    /// as the task lives — and, between words, send what no relay had taken
    /// ([`GuestSession::flush_unsent`]).
    pub fn spawn(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let this = Arc::clone(self);
        let mut rx = this.relays.subscribe();
        tokio::spawn(async move {
            let mut retry = tokio::time::interval(UNSENT_EVERY);
            loop {
                tokio::select! {
                    heard = rx.recv() => match heard {
                        Ok(incoming) => this.route(&incoming).await,
                        // LCOV_EXCL_START the pool's stream lagged or closed under the router; a race
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!("guest inbound lagged by {n}: the pool forgets what it handed on and reads everything again");
                            // Safe: a fact is kept once by its id, a word
                            // applied by its time. Read in a task of its
                            // own — this reader must drain what it brings.
                            let relays = Arc::clone(&this.relays);
                            tokio::spawn(async move {
                                relays.forget_seen().await;
                                relays.catch_up(None).await;
                            });
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                        // LCOV_EXCL_STOP
                    },
                    _ = retry.tick() => this.flush_unsent().await,
                }
            }
        }) // LCOV_EXCL_LINE the other edge of the branch; by construction
    }

    /// Send, for every membership, what no relay had taken yet.
    pub async fn flush_unsent(&self) {
        let sessions: Vec<Arc<GuestSession>> =
            self.sessions.read().await.values().cloned().collect();
        for session in sessions.iter().filter(|s| s.has_unsent()) {
            if let Err(e) = session.flush_unsent().await {
                // LCOV_EXCL_START the copy unreadable under a retry; disk-only
                tracing::warn!("sending what waited for {}: {e}", session.host_hex());
                // LCOV_EXCL_STOP
            }
        }
    }

    /// Hand one incoming thing to the session of the host that sealed it.
    pub async fn route(&self, incoming: &Incoming) {
        let sender = match incoming {
            Incoming::Gep { sender, .. } | Incoming::Control { sender, .. } => sender.to_hex(),
            Incoming::Other => return,
        };
        let session = self.sessions.read().await.get(&sender).cloned();
        if let Some(session) = session {
            match session.on_incoming(incoming) {
                Ok(replies) => {
                    for reply in replies {
                        if let Err(e) = session.send_control(&reply).await {
                            tracing::debug!("the host {sender} did not hear {}: {e}", reply.kind());
                            // An ask nobody heard is no ask.
                            if let Control::WantFace { sha256 } = &reply {
                                session.not_asked(sha256);
                            }
                        }
                    }
                }
                Err(e) => tracing::warn!("applying the host {sender}'s word: {e}"), // LCOV_EXCL_LINE applying a word fails only unwritable (disk-only)
            }
        }
    }

    /// Changes across every session: `(host pubkey hex, change)`.
    pub fn subscribe(&self) -> broadcast::Receiver<(String, HostedChange)> {
        self.changed.subscribe()
    }

    pub async fn session(&self, host_hex: &str) -> Option<Arc<GuestSession>> {
        self.sessions.read().await.get(host_hex).cloned()
    }

    /// Every membership, as it stands.
    pub async fn list(&self) -> Vec<Hosted> {
        let sessions = self.sessions.read().await;
        let mut out: Vec<Hosted> = sessions
            .values()
            .filter_map(|s| s.hosted().ok().flatten())
            .collect();
        out.sort_by(|a, b| {
            a.host
                .name
                .cmp(&b.host.name)
                .then(a.host.pubkey.as_hex().cmp(b.host.pubkey.as_hex()))
        });
        out
    }

    /// How many memberships this node holds — a session with nothing
    /// recorded (its record moved aside as unreadable) is none.
    pub async fn count(&self) -> usize {
        self.sessions
            .read()
            .await
            .values()
            .filter(|s| matches!(s.hosted(), Ok(Some(_))))
            .count()
    }

    /// Join the workspace a code names: the code's relays are added to the
    /// pool by the caller (they are a setting); here the session is opened
    /// and the claim sent.
    pub async fn join(
        &self,
        code: &InviteCode,
        label: Option<String>,
        client: &str,
    ) -> Result<(Arc<GuestSession>, HostedState), GuestError> {
        let host = PublicKey::from_hex(code.host.as_hex())
            .map_err(|e| GuestError::Refused(format!("bad host key: {e}")))?;
        let session = {
            let existing = self.sessions.read().await.get(&host.to_hex()).cloned();
            match existing {
                Some(s) => s,
                None => {
                    let s = GuestSession::open(
                        self.keys.clone(),
                        host,
                        &self.data_dir,
                        Arc::clone(&self.relays),
                        Arc::clone(&self.faces),
                    )?;
                    self.relay_changes(&s);
                    self.sessions
                        .write()
                        .await
                        .insert(host.to_hex(), Arc::clone(&s));
                    s
                }
            }
        };
        match session.join(code, label, client).await {
            Ok(state) => Ok((session, state)),
            Err(not_joined) => {
                // A claim that never left changes nothing: a session opened
                // for it, with no membership recorded, is closed again —
                // or it counted as a workspace joined, listed by nobody.
                let nothing_stands = matches!(session.hosted(), Ok(None));
                let mut sessions = self.sessions.write().await;
                sessions.retain(|key, _| !(nothing_stands && *key == host.to_hex()));
                Err(not_joined)
            }
        }
    }

    /// Leave one host. The session stays open, marked as left, so its
    /// section can say so; the replica stays on disk.
    pub async fn leave(&self, host_hex: &str) -> Result<(), GuestError> {
        let session = self.session(host_hex).await.ok_or(GuestError::NotHosted)?;
        session.leave().await
    }

    /// Forget a membership that is over — left, removed from, or refused:
    /// its session is closed and its copy moved out of `hosts/`, kept whole
    /// under `hosts-forgotten/`. One that stands, or waits on the host, is
    /// left first: forgetting it would leave the host relaying to nobody.
    /// Answers where the copy went.
    pub async fn forget(&self, host_hex: &str) -> Result<PathBuf, GuestError> {
        let session = self.session(host_hex).await.ok_or(GuestError::NotHosted)?;
        let stands = session
            .hosted()?
            .is_some_and(|h| matches!(h.state, HostedState::Member | HostedState::Requested));
        if stands {
            return Err(GuestError::Refused(
                "leave this workspace first — it still counts you, or is still answering your request".into(),
            ));
        }
        self.sessions.write().await.remove(host_hex);
        drop(session);
        let to = GuestStore::forget(&self.data_dir.join(HOSTS_DIR), host_hex)?;
        if self
            .changed
            .send((host_hex.to_string(), HostedChange::Membership))
            .is_err()
        {
            tracing::trace!("a membership forgotten with no listener");
        }
        Ok(to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_collab::InviteCode;
    use nostr_relay_builder::builder::RelayBuilder;
    use nostr_relay_builder::local::LocalRelay;

    async fn local_relay() -> (LocalRelay, String) {
        let relay = LocalRelay::new(RelayBuilder::default().addr("127.0.0.1".parse().unwrap()));
        relay.run().await.expect("relay run");
        let url = relay.url().await.to_string();
        (relay, url)
    }

    /// The host's clock for a test's words: each said a second after the
    /// last, as a host says them.
    fn tick() -> u64 {
        static CLOCK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1_000);
        CLOCK.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    fn pid(pk: PublicKey) -> PrincipalId {
        PrincipalId::new(pk.to_hex()).unwrap()
    }

    /// Alice's face as the fake host holds it: a small PNG.
    fn alice_face() -> (Vec<u8>, bisa_core::AttachmentRef) {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.resize(600, 3);
        let face = Face::from_bytes(&png);
        let r = bisa_core::AttachmentRef {
            sha256: face.sha256,
            name: "alice-face.png".into(),
            mime: "image/png".into(),
            size: png.len() as u64,
        };
        (png, r)
    }

    /// A host that answers a `Join` with a `Welcome` — the owner's row wearing
    /// a face — a `WantFace` with the bytes, and records the profiles it hears:
    /// the smallest stand-in for the host's pump.
    async fn fake_host(
        host: Arc<Relays>,
        guest: PublicKey,
        channels: Vec<Channel>,
        heard: Arc<Mutex<Vec<Control>>>,
    ) {
        let mut rx = host.subscribe();
        tokio::spawn(async move {
            while let Ok(incoming) = rx.recv().await {
                match incoming {
                    Incoming::Control {
                        sender,
                        control: Control::Join { .. },
                        ..
                    } if sender == guest => {
                        let (_, alice) = alice_face();
                        let welcome = Control::Welcome {
                            workspace: HostCard {
                                pubkey: pid(host.public_key()),
                                name: "Acme".into(),
                                relays: vec![],
                            },
                            role: MemberRole::Guest,
                            members: vec![
                                Directory {
                                    pubkey: pid(host.public_key()),
                                    role: MemberRole::Owner,
                                    label: Some("Alice".into()),
                                    photo: Some(alice),
                                },
                                Directory {
                                    pubkey: pid(guest),
                                    role: MemberRole::Guest,
                                    label: Some("Bob".into()),
                                    photo: None,
                                },
                            ],
                            channels: channels.clone(),
                        };
                        let wrap = wrap_control(host.keys(), guest, &welcome).unwrap();
                        host.publish(&wrap).await.unwrap();
                    }
                    Incoming::Control {
                        sender,
                        control: Control::WantFace { sha256 },
                        ..
                    } if sender == guest => {
                        let (png, alice) = alice_face();
                        heard.locked().push(Control::WantFace {
                            sha256: sha256.clone(),
                        });
                        if sha256 == alice.sha256 {
                            let wrap = wrap_control(
                                host.keys(),
                                guest,
                                &Control::Face {
                                    face: Face::from_bytes(&png),
                                },
                            )
                            .unwrap();
                            host.publish(&wrap).await.unwrap();
                        }
                    }
                    Incoming::Control {
                        sender,
                        control: control @ Control::Profile { .. },
                        ..
                    } if sender == guest => heard.locked().push(control),
                    _ => {}
                }
            }
        });
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_guest_joins_is_welcomed_posts_a_redacted_message_and_refuses_an_unreached_channel() {
        std::env::set_var("BISA_JOIN_WAIT_SECS", "10");
        let (_relay, url) = local_relay().await;
        let host = Relays::start(Keys::generate(), std::slice::from_ref(&url)).await;
        let guest_keys = Keys::generate();
        let guest_relays = Relays::start(guest_keys.clone(), std::slice::from_ref(&url)).await;
        let dir = tempfile::tempdir().unwrap();
        let faces = Arc::new(crate::faces::MemoryFaces::new());
        faces.set_profile(crate::faces::OwnerProfile {
            label: Some("Bob from his node".into()),
            photo: None,
        });
        let guests = Guests::open(
            guest_keys.clone(),
            dir.path(),
            Arc::clone(&guest_relays),
            Arc::clone(&faces) as Arc<dyn FaceStore>,
        )
        .unwrap();
        let _router = guests.spawn();
        let heard = Arc::new(Mutex::new(Vec::new()));
        fake_host(
            Arc::clone(&host),
            guest_keys.public_key(),
            vec![Channel::general(0)],
            Arc::clone(&heard),
        )
        .await;

        let code = InviteCode::new(
            host.public_key(),
            std::slice::from_ref(&url),
            "ab".repeat(16),
        );
        let (session, state) = guests.join(&code, Some("Bob".into()), "cli").await.unwrap();
        assert_eq!(state, HostedState::Member, "welcomed");
        let hosted = session.hosted().unwrap().unwrap();
        assert_eq!(hosted.host.name, "Acme");
        assert_eq!(hosted.role, MemberRole::Guest);
        assert_eq!(session.channels().unwrap().len(), 1);
        assert_eq!(session.members().unwrap().len(), 2);
        assert_eq!(guests.list().await.len(), 1);
        // A channel the host never named is refused to read as to post:
        // never an empty room that hides what was not sent.
        assert!(
            matches!(
                session.messages("kitchen", None, 50),
                Err(GuestError::Refused(_))
            ),
            "a channel this person does not reach"
        );
        // The welcome named a face this node lacked: it was asked for once,
        // came back, and is held under its hash; the host heard who Bob is.
        let (_, alice) = alice_face();
        for _ in 0..100 {
            if faces.bytes(&alice.sha256).is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(
            faces.bytes(&alice.sha256).is_some(),
            "the owner's face landed"
        );
        assert_eq!(
            session.members().unwrap()[0]
                .photo
                .as_ref()
                .map(|p| &p.sha256),
            Some(&alice.sha256)
        );
        {
            let heard = heard.locked();
            assert_eq!(
                heard
                    .iter()
                    .filter(|c| matches!(c, Control::WantFace { .. }))
                    .count(),
                1,
                "asked once: {heard:?}"
            );
            assert!(heard.iter().any(|c| matches!(c, Control::Profile { label: Some(l), .. } if l == "Bob from his node")), "the profile follows the welcome: {heard:?}");
        }
        // A face that is not one is dropped; a directory naming a face already held asks nothing.
        let lying = Face {
            sha256: "0".repeat(64),
            b64: "AAAA".into(),
        };
        assert!(session
            .on_incoming(&Incoming::Control {
                sender: host.public_key(),
                control: Control::Face { face: lying },
                at: tick(),
            })
            .unwrap()
            .is_empty());
        assert_eq!(faces.count(), 1);
        let again = session
            .on_incoming(&Incoming::Control {
                sender: host.public_key(),
                control: Control::MembersChanged {
                    members: session.members().unwrap(),
                },
                at: tick(),
            })
            .unwrap();
        assert!(again.is_empty(), "nothing to ask: {again:?}");

        // The host hears the guest's post, sealed by the guest, with the
        // secret gone before it was signed.
        let mut heard = host.subscribe();
        let posted = session
            .post(
                "general",
                "token ghp_abcdefghijklmnopqrstuvwxyz0123456789 here",
                &[],
                None,
            )
            .await
            .unwrap();
        assert_eq!(posted.scope, "general");
        assert!(posted.redacted >= 1, "the redactor ran: {posted:?}");
        let got = tokio::time::timeout(Duration::from_secs(10), heard.recv())
            .await
            .unwrap()
            .unwrap();
        match got {
            Incoming::Gep { sender, event } => {
                assert_eq!(sender, guest_keys.public_key());
                assert_eq!(event.pubkey, guest_keys.public_key());
                assert!(!event
                    .content
                    .contains("ghp_abcdefghijklmnopqrstuvwxyz0123456789"));
                assert_eq!(scope_of(&event).as_deref(), Some("general"));
            }
            other => panic!("unexpected: {other:?}"),
        }
        let mine = session.messages("general", None, 10).unwrap();
        assert_eq!(mine.len(), 1);
        assert!(!mine[0].text.contains("ghp_"));

        let refused = session.post("design", "hi", &[], None).await;
        assert!(
            matches!(refused, Err(GuestError::Refused(_))),
            "{refused:?}"
        );

        // A fact the host's seal carried, in a reached channel, lands whoever
        // wrote it: the host admitted its author — somebody the directory no
        // longer names, whose words a newcomer is owed all the same.
        let stranger = Keys::generate();
        let coord = format!("33405:{}:general", host.public_key().to_hex());
        let strangers = EventBuilder::new(
            Kind::Custom(KIND_MESSAGE),
            serde_json::to_string(&MessageBody::post("boo")).unwrap(),
        )
        .tags([Tag::parse(["a", &coord]).unwrap()])
        .finalize(&stranger)
        .unwrap();
        let hosts = EventBuilder::new(
            Kind::Custom(KIND_MESSAGE),
            serde_json::to_string(&MessageBody::post("welcome bob")).unwrap(),
        )
        .tags([Tag::parse(["a", &coord]).unwrap()])
        .finalize(host.keys())
        .unwrap();
        session
            .on_incoming(&Incoming::Gep {
                sender: host.public_key(),
                event: strangers,
            })
            .unwrap();
        session
            .on_incoming(&Incoming::Gep {
                sender: host.public_key(),
                event: hosts.clone(),
            })
            .unwrap();
        let now = session.messages("general", None, 10).unwrap();
        assert_eq!(now.len(), 3, "{now:?}");
        assert!(now.iter().any(|m| m.text == "welcome bob"));
        assert!(now.iter().any(|m| m.text == "boo"));
        assert_eq!(session.unread("general").unwrap(), 2);
        session.mark_read("general").unwrap();
        assert_eq!(session.unread("general").unwrap(), 0);

        // A reaction and a retraction of one's own, and never of the host's.
        let reacted = session.react(&hosts.id.to_hex(), "👍").await.unwrap();
        assert_eq!(reacted.scope, "general");
        // Pressed again it is the same reaction — the one that can be taken
        // back — and another emoji is another.
        let again = session.react(&hosts.id.to_hex(), "👍").await.unwrap();
        assert_eq!(
            again.id, reacted.id,
            "one live reaction a person, a message and an emoji"
        );
        let other = session.react(&hosts.id.to_hex(), "🎉").await.unwrap();
        assert_ne!(other.id, reacted.id);
        let reactions = session
            .messages("general", None, 10)
            .unwrap()
            .into_iter()
            .find(|m| m.id == hosts.id.to_hex())
            .map(|m| m.reactions.len());
        assert_eq!(reactions, Some(2), "two emoji, two reactions");
        // A message past the host's bound is refused here, and kept nowhere.
        let before = session.messages("general", None, 50).unwrap().len();
        let long = "x".repeat(bisa_core::MAX_TEXT_BYTES + 1);
        let refused = session.post("general", &long, &[], None).await.unwrap_err();
        assert!(refused.to_string().contains("at most"), "{refused}");
        assert_eq!(session.messages("general", None, 50).unwrap().len(), before);
        assert!(session.retract(&hosts.id.to_hex()).await.is_err());
        session.retract(&posted.id).await.unwrap();
        let after = session.messages("general", None, 10).unwrap();
        assert!(after.iter().find(|m| m.id == posted.id).unwrap().retracted);

        // Leaving tells the host and marks the replica. The reaction and the
        // retraction above are still in flight to the host, so the leave is
        // awaited past whatever lands first.
        let mut heard = host.subscribe();
        guests.leave(&host.public_key().to_hex()).await.unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            let got = tokio::time::timeout_at(deadline, heard.recv())
                .await
                .expect("the leave reaches the host in time")
                .unwrap();
            match got {
                Incoming::Control {
                    control: Control::Leave,
                    sender,
                    ..
                } => {
                    assert_eq!(sender, guest_keys.public_key());
                    break;
                }
                Incoming::Gep { .. } => continue,
                other => panic!("unexpected before the leave: {other:?}"),
            }
        }
        assert_eq!(session.hosted().unwrap().unwrap().state, HostedState::Left);
        assert!(session
            .post("general", "still here?", &[], None)
            .await
            .is_err());

        guest_relays.stop().await;
        host.stop().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_refusal_and_a_removal_are_kept_in_words_and_a_reopen_finds_the_replica() {
        let (_relay, url) = local_relay().await;
        let host = Keys::generate();
        let guest_keys = Keys::generate();
        let relays = Relays::start(guest_keys.clone(), std::slice::from_ref(&url)).await;
        let dir = tempfile::tempdir().unwrap();
        let session = GuestSession::open(
            guest_keys.clone(),
            host.public_key(),
            dir.path(),
            Arc::clone(&relays),
            Arc::new(crate::faces::MemoryFaces::new()),
        )
        .unwrap();
        assert!(matches!(
            session.post("general", "x", &[], None).await,
            Err(GuestError::NotHosted)
        ));
        session
            .store()
            .set_hosted(&Hosted {
                host: HostCard {
                    pubkey: pid(host.public_key()),
                    name: String::new(),
                    relays: vec![],
                },
                role: MemberRole::Guest,
                state: HostedState::Requested,
                requested_at: 1,
                joined_at: None,
                label: None,
            })
            .unwrap();
        session
            .on_incoming(&Incoming::Control {
                sender: host.public_key(),
                control: Control::Refused {
                    reason: "expired".into(),
                },
                at: tick(),
            })
            .unwrap();
        assert_eq!(
            session.hosted().unwrap().unwrap().state,
            HostedState::Refused {
                reason: "expired".into()
            }
        );
        // Another key's word is not the host's.
        let other = Keys::generate();
        session
            .on_incoming(&Incoming::Control {
                sender: other.public_key(),
                control: Control::Welcome {
                    workspace: HostCard {
                        pubkey: pid(other.public_key()),
                        name: "x".into(),
                        relays: vec![],
                    },
                    role: MemberRole::Admin,
                    members: vec![],
                    channels: vec![],
                },
                at: tick(),
            })
            .unwrap();
        assert_eq!(session.hosted().unwrap().unwrap().role, MemberRole::Guest);
        session
            .on_incoming(&Incoming::Control {
                sender: host.public_key(),
                control: Control::Removed {
                    reason: Some("done".into()),
                },
                at: tick(),
            })
            .unwrap();
        assert_eq!(
            session.hosted().unwrap().unwrap().state.words(),
            "removed — done"
        );

        let guests = Guests::open(
            guest_keys,
            dir.path(),
            relays.clone(),
            Arc::new(crate::faces::MemoryFaces::new()),
        )
        .unwrap();
        assert_eq!(guests.count().await, 1);
        assert!(guests.session(&host.public_key().to_hex()).await.is_some());
        relays.stop().await;
    }

    /// A member claiming again — the same code, typed twice — stays a
    /// member: no claim goes out, and a refusal the host might say of a spent
    /// code never writes over a standing membership.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_members_second_claim_and_a_refusal_never_unseat_the_member() {
        let (_relay, url) = local_relay().await;
        let host = Keys::generate();
        let guest_keys = Keys::generate();
        let relays = Relays::start(guest_keys.clone(), std::slice::from_ref(&url)).await;
        let dir = tempfile::tempdir().unwrap();
        let session = GuestSession::open(
            guest_keys.clone(),
            host.public_key(),
            dir.path(),
            Arc::clone(&relays),
            Arc::new(crate::faces::MemoryFaces::new()),
        )
        .unwrap();
        let member = Hosted {
            host: HostCard {
                pubkey: pid(host.public_key()),
                name: "The shelf".into(),
                relays: vec![url.clone()],
            },
            role: MemberRole::Guest,
            state: HostedState::Member,
            requested_at: 1,
            joined_at: Some(2),
            label: Some("bob".into()),
        };
        session.store().set_hosted(&member).unwrap();

        let code = InviteCode {
            host: pid(host.public_key()),
            relays: vec![url.clone()],
            secret: bisa_collab::mint_secret(),
        };
        let state = session
            .join(&code, Some("bob again".into()), "test")
            .await
            .unwrap();
        assert_eq!(state, HostedState::Member, "nothing to claim");
        assert_eq!(
            session.hosted().unwrap().unwrap(),
            member,
            "the record is as it was — its label and its host's name too"
        );

        session
            .on_incoming(&Incoming::Control {
                sender: host.public_key(),
                control: Control::Refused {
                    reason: "this invite code is unknown or was already used".into(),
                },
                at: tick(),
            })
            .unwrap();
        assert_eq!(
            session.hosted().unwrap().unwrap().state,
            HostedState::Member,
            "a refusal answers a claim, and a member made none"
        );

        // The person leaves; the host's removal — its answer to the leave —
        // lands after: the record keeps saying *left*.
        session.leave().await.unwrap();
        assert_eq!(session.hosted().unwrap().unwrap().state, HostedState::Left);
        session
            .on_incoming(&Incoming::Control {
                sender: host.public_key(),
                control: Control::Removed { reason: None },
                at: tick(),
            })
            .unwrap();
        assert_eq!(
            session.hosted().unwrap().unwrap().state,
            HostedState::Left,
            "a removal that answers a leave is the host's housekeeping, not a new fact"
        );
        relays.stop().await;
    }

    /// With no relay to reach the host, every refusal of a session is said
    /// before anything leaves, the host's other words are nothing a guest
    /// acts on, and the router hands each thing to the session it is for.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_session_refuses_what_it_cannot_do_and_the_router_hands_each_word_on() {
        let host_keys = Keys::generate();
        let host = host_keys.public_key();
        let guest_keys = Keys::generate();
        let relays = Relays::start(guest_keys.clone(), &Vec::<String>::new()).await;
        let dir = tempfile::tempdir().unwrap();
        let faces = Arc::new(crate::faces::MemoryFaces::new());
        let guests = Guests::open(
            guest_keys.clone(),
            dir.path(),
            Arc::clone(&relays),
            Arc::clone(&faces) as Arc<dyn FaceStore>,
        )
        .unwrap();
        // Nothing hosted: nothing to leave.
        assert!(matches!(
            guests.leave(&host.to_hex()).await,
            Err(GuestError::NotHosted)
        ));
        // A claim nobody can carry is refused and changes nothing: no
        // membership is recorded, no session is left open, nothing is
        // listed — *waiting for the host* over a claim that never left
        // waited on an answer to nothing.
        let code = InviteCode::new(host, &Vec::<String>::new(), "ab".repeat(16));
        assert!(guests.join(&code, Some("Bob".into()), "cli").await.is_err());
        assert!(guests.session(&host.to_hex()).await.is_none());
        assert_eq!(guests.count().await, 0);
        assert!(guests.list().await.is_empty());
        let session = GuestSession::open(
            guest_keys.clone(),
            host,
            dir.path(),
            Arc::clone(&relays),
            Arc::clone(&faces) as Arc<dyn FaceStore>,
        )
        .unwrap();
        assert_eq!(session.host(), host);
        assert!(session.hosted().unwrap().is_none());
        // A code for another host is refused by this one's session, and
        // there is nothing to leave.
        let other = Keys::generate();
        assert!(matches!(
            session
                .join(
                    &InviteCode::new(other.public_key(), &Vec::<String>::new(), "cd".repeat(16)),
                    None,
                    "cli"
                )
                .await,
            Err(GuestError::Refused(_))
        ));
        assert!(matches!(session.leave().await, Err(GuestError::NotHosted)));
        // A claim that never left over a membership that is over leaves
        // what stood — the state and the host's name with it.
        session
            .store()
            .set_hosted(&Hosted {
                host: HostCard {
                    pubkey: pid(host),
                    name: "Alice's".into(),
                    relays: vec![],
                },
                role: MemberRole::Guest,
                state: HostedState::Left,
                requested_at: 1,
                joined_at: Some(1),
                label: None,
            })
            .unwrap();
        assert!(session.join(&code, None, "cli").await.is_err());
        let stood = session.hosted().unwrap().unwrap();
        assert_eq!(stood.state, HostedState::Left);
        assert_eq!(stood.host.name, "Alice's");
        // The host's word before any claim is nothing.
        let elsewhere = GuestSession::open(
            guest_keys.clone(),
            other.public_key(),
            dir.path(),
            Arc::clone(&relays),
            Arc::clone(&faces) as Arc<dyn FaceStore>,
        )
        .unwrap();
        assert!(elsewhere
            .on_incoming(&Incoming::Control {
                sender: other.public_key(),
                control: Control::Leave,
                at: tick(),
            })
            .unwrap()
            .is_empty());
        assert!(matches!(
            elsewhere.leave().await,
            Err(GuestError::NotHosted)
        ));

        // A member by the host's word: what a post, a reaction and a direct
        // message refuse before anything leaves.
        session
            .store()
            .set_hosted(&Hosted {
                host: HostCard {
                    pubkey: pid(host),
                    name: String::new(),
                    relays: vec![],
                },
                role: MemberRole::Guest,
                state: HostedState::Member,
                requested_at: tick(),
                joined_at: Some(0),
                label: None,
            })
            .unwrap();
        session
            .store()
            .set_channels(&[Channel::general(0)])
            .unwrap();
        let (_, alice) = alice_face();
        session
            .store()
            .set_members(&[
                Directory {
                    pubkey: pid(host),
                    role: MemberRole::Owner,
                    label: Some("Alice".into()),
                    photo: Some(alice),
                },
                Directory {
                    pubkey: pid(guest_keys.public_key()),
                    role: MemberRole::Guest,
                    label: None,
                    photo: None,
                },
            ])
            .unwrap();
        let me = pid(guest_keys.public_key());
        assert!(matches!(
            session.post("general", "   ", &[], None).await,
            Err(GuestError::Refused(_))
        ));
        assert!(matches!(
            session
                .post("general", "hi", &[], Some(&"0".repeat(64)))
                .await,
            Err(GuestError::Refused(_))
        ));
        assert!(matches!(
            session.react(&"0".repeat(64), "").await,
            Err(GuestError::Refused(_))
        ));
        assert!(matches!(
            session.open_dm(&[]).await,
            Err(GuestError::Refused(_))
        ));
        assert!(matches!(
            session.open_dm(&[me.clone(), me.clone()]).await,
            Err(GuestError::Refused(_))
        ));
        assert!(matches!(
            session.open_dm(&[pid(other.public_key())]).await,
            Err(GuestError::Refused(_))
        ));

        // A reply with a mention to a message held is built and kept, and
        // waits for a relay — said so, not refused; facts of another kind, with no scope
        // or in a channel this person does not reach are dropped; the
        // host's other words are nothing a guest acts on.
        let coord = format!("33405:{}:general", host.to_hex());
        let held = EventBuilder::new(
            Kind::Custom(KIND_MESSAGE),
            serde_json::to_string(&MessageBody::post("from the host")).unwrap(),
        )
        .tags([Tag::parse(["a", &coord]).unwrap()])
        .finalize(&host_keys)
        .unwrap();
        session
            .on_incoming(&Incoming::Gep {
                sender: host,
                event: held.clone(),
            })
            .unwrap();
        let sent = session
            .post("general", "noted", &[pid(host)], Some(&held.id.to_hex()))
            .await;
        let sent = sent.expect("kept for the next relay");
        assert!(sent.queued, "nothing carried it to the host yet");
        assert!(session.has_unsent());
        let mine = session.messages("general", None, 10).unwrap();
        let noted = mine.iter().find(|m| m.text == "noted").expect("kept");
        assert!(noted.unsent, "{mine:?}");
        assert!(!mine.iter().any(|m| m.text == "from the host" && m.unsent));
        // The retry finds no relay still and keeps it.
        guests.flush_unsent().await;
        assert_eq!(session.flush_unsent().await.unwrap(), 1, "it waits on");
        assert!(session.has_unsent());
        let other_kind = EventBuilder::new(Kind::Custom(1), "x")
            .tags([Tag::parse(["a", &coord]).unwrap()])
            .finalize(&host_keys)
            .unwrap();
        let no_scope = EventBuilder::new(Kind::Custom(KIND_MESSAGE), "{}")
            .finalize(&host_keys)
            .unwrap();
        let kitchen = EventBuilder::new(Kind::Custom(KIND_MESSAGE), "{}")
            .tags([Tag::parse(["a", &format!("33405:{}:kitchen", host.to_hex())]).unwrap()])
            .finalize(&host_keys)
            .unwrap();
        for event in [other_kind, no_scope, kitchen] {
            session
                .on_incoming(&Incoming::Gep {
                    sender: host,
                    event,
                })
                .unwrap();
        }
        assert_eq!(session.messages("general", None, 10).unwrap().len(), 2);
        assert!(session
            .on_incoming(&Incoming::Control {
                sender: host,
                control: Control::Leave,
                at: tick(),
            })
            .unwrap()
            .is_empty());

        // The router: a thing that is no word, a directory naming a face this
        // node lacks whose ask the host cannot hear, a profile nobody hears.
        let guests = Guests::open(
            guest_keys.clone(),
            dir.path(),
            Arc::clone(&relays),
            Arc::clone(&faces) as Arc<dyn FaceStore>,
        )
        .unwrap();
        assert_eq!(guests.count().await, 1);
        guests.route(&Incoming::Other).await;
        // A word sealed by a host this node is no guest of goes nowhere.
        guests
            .route(&Incoming::Control {
                sender: Keys::generate().public_key(),
                control: Control::Waiting,
                at: tick(),
            })
            .await;
        guests
            .route(&Incoming::Control {
                sender: host,
                control: Control::MembersChanged {
                    members: session.members().unwrap(),
                },
                at: tick(),
            })
            .await;
        guests.announce_profile().await;
        // A second host whose claim never left lists nowhere; a claim that
        // never left for a membership that is over leaves it listed as it was.
        let second = InviteCode::new(other.public_key(), &Vec::<String>::new(), "ef".repeat(16));
        assert!(guests.join(&second, None, "cli").await.is_err());
        assert_eq!(guests.count().await, 1);
        guests.leave(&host.to_hex()).await.unwrap();
        assert!(guests.join(&code, None, "cli").await.is_err());
        assert_eq!(guests.count().await, 1);
        assert_eq!(guests.list().await[0].state, HostedState::Left);
        // A second membership lists beside the first, by key when unnamed.
        let second_host = Keys::generate();
        drop(waiting_on(&second_host, &guest_keys, dir.path(), &relays));
        let both = Guests::open(
            guest_keys.clone(),
            dir.path(),
            Arc::clone(&relays),
            Arc::clone(&faces) as Arc<dyn FaceStore>,
        )
        .unwrap();
        let listed = both.list().await;
        assert_eq!(listed.len(), 2);
        assert!(
            (listed[0].host.name.as_str(), listed[0].host.pubkey.as_hex())
                <= (listed[1].host.name.as_str(), listed[1].host.pubkey.as_hex())
        );
        relays.stop().await;
    }

    /// A fact names the workspace it was said in, by its coordinate's owner.
    #[test]
    fn a_fact_names_its_workspace_by_its_coordinates_owner() {
        let host = Keys::generate();
        let other = Keys::generate();
        let ev = EventBuilder::new(Kind::Custom(KIND_MESSAGE), "{}")
            .tags([Tag::parse([
                "a",
                &format!("33405:{}:general", host.public_key().to_hex()),
            ])
            .unwrap()])
            .finalize(&other)
            .unwrap();
        assert!(names_workspace(&ev, &host.public_key()));
        assert!(!names_workspace(&ev, &other.public_key()));
        let bare = EventBuilder::new(Kind::Custom(KIND_MESSAGE), "{}")
            .finalize(&host)
            .unwrap();
        assert!(!names_workspace(&bare, &host.public_key()));
    }

    // -- the guest's side, hardened: order, retry, a way out ----------------

    fn room(id: &str, born: u64) -> Channel {
        Channel {
            id: bisa_core::ChannelId::new(id).unwrap(),
            name: id.into(),
            ..Channel::general(born)
        }
    }

    fn welcome_as(host: &Keys, role: MemberRole, channels: Vec<Channel>) -> Control {
        Control::Welcome {
            workspace: HostCard {
                pubkey: pid(host.public_key()),
                name: "Acme".into(),
                relays: vec![],
            },
            role,
            members: vec![Directory {
                pubkey: pid(host.public_key()),
                role: MemberRole::Owner,
                label: Some("Alice".into()),
                photo: None,
            }],
            channels,
        }
    }

    fn said(host: &Keys, control: Control, at: u64) -> Incoming {
        Incoming::Control {
            sender: host.public_key(),
            control,
            at,
        }
    }

    fn fact_in(author: &Keys, owner: &Keys, scope: &str, text: &str) -> Event {
        EventBuilder::new(
            Kind::Custom(KIND_MESSAGE),
            serde_json::to_string(&MessageBody::post(text)).unwrap(),
        )
        .tags([Tag::parse([
            "a",
            &format!("33405:{}:{scope}", owner.public_key().to_hex()),
        ])
        .unwrap()])
        .finalize(author)
        .unwrap()
    }

    /// A session of `guest`'s on `host`, its claim recorded and waiting.
    fn waiting_on(
        host: &Keys,
        guest: &Keys,
        dir: &Path,
        relays: &Arc<Relays>,
    ) -> Arc<GuestSession> {
        let session = GuestSession::open(
            guest.clone(),
            host.public_key(),
            dir,
            Arc::clone(relays),
            Arc::new(crate::faces::MemoryFaces::new()),
        )
        .unwrap();
        session
            .store()
            .set_hosted(&Hosted {
                host: HostCard {
                    pubkey: pid(host.public_key()),
                    name: String::new(),
                    relays: vec![],
                },
                role: MemberRole::Guest,
                state: HostedState::Requested,
                requested_at: 1,
                joined_at: None,
                label: None,
            })
            .unwrap();
        session
    }

    /// The same, welcomed at `at` into `general`.
    fn member_of(
        host: &Keys,
        guest: &Keys,
        dir: &Path,
        relays: &Arc<Relays>,
        at: u64,
    ) -> Arc<GuestSession> {
        let session = waiting_on(host, guest, dir, relays);
        session
            .on_incoming(&said(
                host,
                welcome_as(host, MemberRole::Guest, vec![room("general", 1)]),
                at,
            ))
            .unwrap();
        assert!(session.hosted().unwrap().unwrap().state.is_member());
        session
    }

    fn orders(n: usize) -> Vec<Vec<usize>> {
        fn go(k: usize, a: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
            if k <= 1 {
                out.push(a.clone());
                return;
            }
            for i in 0..k {
                go(k - 1, a, out);
                let j = if k % 2 == 0 { i } else { 0 };
                a.swap(j, k - 1);
            }
        }
        let mut out = Vec::new();
        go(n, &mut (0..n).collect(), &mut out);
        out
    }

    /// A relay hands the host's words back in any order — every one it kept,
    /// after a restart. Whatever the order, a copy ends where the newest say.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_newest_word_stands_whatever_order_they_arrive_in() {
        let host = Keys::generate();
        let guest = Keys::generate();
        let relays = Relays::start(guest.clone(), &Vec::<String>::new()).await;
        let words = [
            (Control::Waiting, 10),
            (
                welcome_as(&host, MemberRole::Guest, vec![room("general", 1)]),
                20,
            ),
            (Control::Removed { reason: None }, 30),
            (
                welcome_as(
                    &host,
                    MemberRole::Member,
                    vec![room("general", 1), room("design", 2)],
                ),
                40,
            ),
            (
                Control::RoleChanged {
                    role: MemberRole::Admin,
                },
                50,
            ),
        ];
        let all = orders(words.len());
        assert_eq!(all.len(), 120);
        for order in all {
            let dir = tempfile::tempdir().unwrap();
            let session = waiting_on(&host, &guest, dir.path(), &relays);
            for i in &order {
                let (control, at) = &words[*i];
                session
                    .on_incoming(&said(&host, control.clone(), *at))
                    .unwrap();
            }
            let stands = session.hosted().unwrap().unwrap();
            assert_eq!(stands.state, HostedState::Member, "{order:?}");
            assert_eq!(stands.role, MemberRole::Admin, "{order:?}");
            assert_eq!(stands.host.name, "Acme", "{order:?}");
            assert_eq!(session.channels().unwrap().len(), 2, "{order:?}");
            assert_eq!(session.members().unwrap().len(), 1, "{order:?}");
        }
        relays.stop().await;
    }

    /// C10: a *waiting* the relay hands back — said before the welcome, or
    /// in its very second — unseats nobody; nor does an old refusal or an
    /// old removal unseat somebody invited again.
    #[tokio::test(flavor = "multi_thread")]
    async fn an_old_waiting_never_unseats_a_member() {
        let host = Keys::generate();
        let guest = Keys::generate();
        let relays = Relays::start(guest.clone(), &Vec::<String>::new()).await;
        let dir = tempfile::tempdir().unwrap();
        let session = member_of(&host, &guest, dir.path(), &relays, 20);
        for (old, at) in [
            (Control::Waiting, 10),
            (Control::Waiting, 20),
            (
                Control::Refused {
                    reason: "try it again".into(),
                },
                20,
            ),
            (Control::Removed { reason: None }, 19),
            (
                Control::Refused {
                    reason: "used".into(),
                },
                25,
            ),
        ] {
            session.on_incoming(&said(&host, old, at)).unwrap();
            assert_eq!(
                session.hosted().unwrap().unwrap().state,
                HostedState::Member
            );
        }
        // The marks are the copy's, not the process's: a reopen keeps them.
        drop(session);
        let again = GuestSession::open(
            guest.clone(),
            host.public_key(),
            dir.path(),
            Arc::clone(&relays),
            Arc::new(crate::faces::MemoryFaces::new()),
        )
        .unwrap();
        again
            .on_incoming(&said(&host, Control::Waiting, 10))
            .unwrap();
        assert!(again.hosted().unwrap().unwrap().state.is_member());
        // A removal in the welcome's own second is the later word.
        again
            .on_incoming(&said(&host, Control::Removed { reason: None }, 20))
            .unwrap();
        assert!(matches!(
            again.hosted().unwrap().unwrap().state,
            HostedState::Removed { .. }
        ));
        relays.stop().await;
    }

    /// A claim the host could not admit, made again at once: the refusal and
    /// the welcome are said in one second, and the welcome is the later word
    /// whichever the relay hands over first.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_claim_refused_and_made_again_in_one_second_is_welcomed() {
        let host = Keys::generate();
        let guest = Keys::generate();
        let relays = Relays::start(guest.clone(), &Vec::<String>::new()).await;
        let refusal = Control::Refused {
            reason: "try it again".into(),
        };
        let welcome = welcome_as(&host, MemberRole::Guest, vec![room("general", 1)]);
        for words in [
            [refusal.clone(), welcome.clone()],
            [welcome.clone(), refusal.clone()],
        ] {
            let dir = tempfile::tempdir().unwrap();
            let session = waiting_on(&host, &guest, dir.path(), &relays);
            for word in words {
                session.on_incoming(&said(&host, word, 20)).unwrap();
            }
            assert!(session.hosted().unwrap().unwrap().state.is_member());
        }
        relays.stop().await;
    }

    /// A welcome said again — the host's standing word, or a replay — puts
    /// nothing back: not an older role, not an older list of rooms, not the
    /// day this person joined; and it asks for no second introduction.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_welcome_said_again_puts_nothing_back() {
        let host = Keys::generate();
        let guest = Keys::generate();
        let relays = Relays::start(guest.clone(), &Vec::<String>::new()).await;
        let dir = tempfile::tempdir().unwrap();
        let session = waiting_on(&host, &guest, dir.path(), &relays);
        let first = welcome_as(&host, MemberRole::Guest, vec![room("general", 1)]);
        let replies = session
            .on_incoming(&said(&host, first.clone(), 20))
            .unwrap();
        assert!(
            matches!(replies.first(), Some(Control::Profile { .. })),
            "a member now: the host hears who this is"
        );
        let mut stood = session.hosted().unwrap().unwrap();
        stood.joined_at = Some(7);
        session.store().set_hosted(&stood).unwrap();
        session
            .on_incoming(&said(
                &host,
                Control::RoleChanged {
                    role: MemberRole::Member,
                },
                30,
            ))
            .unwrap();
        session
            .on_incoming(&said(
                &host,
                Control::ChannelsChanged {
                    channels: vec![room("general", 1), room("design", 2)],
                },
                40,
            ))
            .unwrap();
        let replies = session.on_incoming(&said(&host, first, 20)).unwrap();
        assert!(replies.is_empty(), "{replies:?}");
        for old in [
            Control::RoleChanged {
                role: MemberRole::Guest,
            },
            Control::ChannelsChanged { channels: vec![] },
        ] {
            session.on_incoming(&said(&host, old, 25)).unwrap();
        }
        let stands = session.hosted().unwrap().unwrap();
        assert_eq!(stands.role, MemberRole::Member);
        assert_eq!(stands.joined_at, Some(7));
        assert_eq!(session.channels().unwrap().len(), 2);
        // A newer welcome is the host's word on all of it; the joining day stays.
        session
            .on_incoming(&said(
                &host,
                welcome_as(&host, MemberRole::Admin, vec![room("general", 1)]),
                50,
            ))
            .unwrap();
        let stands = session.hosted().unwrap().unwrap();
        assert_eq!(stands.role, MemberRole::Admin);
        assert_eq!(stands.joined_at, Some(7));
        assert_eq!(session.channels().unwrap().len(), 1);
        // A word stamped past tomorrow is no word: applied, nothing newer
        // could ever follow it.
        session
            .on_incoming(&said(
                &host,
                Control::Removed { reason: None },
                now_secs() + 3 * 24 * 60 * 60,
            ))
            .unwrap();
        assert!(session.hosted().unwrap().unwrap().state.is_member());
        relays.stop().await;
    }

    /// Somebody who left is seated by nothing the host says after — and the
    /// host, which plainly did not hear the leave, is told again: at its
    /// next word, then no more than once a minute. *Removed* is its answer.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_welcome_never_reseats_someone_who_left() {
        let host = Keys::generate();
        let guest = Keys::generate();
        let relays = Relays::start(guest.clone(), &Vec::<String>::new()).await;
        let dir = tempfile::tempdir().unwrap();
        let session = member_of(&host, &guest, dir.path(), &relays, 20);
        // No relay: the leave is not heard.
        session.leave().await.unwrap();
        let replies = session
            .on_incoming(&said(
                &host,
                welcome_as(&host, MemberRole::Admin, vec![room("general", 1)]),
                90,
            ))
            .unwrap();
        assert_eq!(replies, vec![Control::Leave], "the host is told again");
        let stands = session.hosted().unwrap().unwrap();
        assert_eq!(stands.state, HostedState::Left);
        assert_eq!(stands.role, MemberRole::Guest, "nothing of it is applied");
        // A fact it relays meanwhile is neither kept nor answered so soon.
        let fact = fact_in(&host, &host, "general", "still here?");
        let replies = session
            .on_incoming(&Incoming::Gep {
                sender: host.public_key(),
                event: fact,
            })
            .unwrap();
        assert!(replies.is_empty(), "said a moment ago: {replies:?}");
        assert!(session.store().events("general").unwrap().is_empty());
        // Its answer to the leave is not answered, and the copy says *left*.
        let replies = session
            .on_incoming(&said(&host, Control::Removed { reason: None }, 95))
            .unwrap();
        assert!(replies.is_empty());
        assert_eq!(session.hosted().unwrap().unwrap().state, HostedState::Left);
        // A request withdrawn while waiting is not seated by the welcome
        // that follows it either.
        let other_dir = tempfile::tempdir().unwrap();
        let asked = waiting_on(&host, &guest, other_dir.path(), &relays);
        asked.leave().await.unwrap();
        let replies = asked
            .on_incoming(&said(
                &host,
                welcome_as(&host, MemberRole::Guest, vec![room("general", 1)]),
                30,
            ))
            .unwrap();
        assert_eq!(replies, vec![Control::Leave]);
        assert_eq!(asked.hosted().unwrap().unwrap().state, HostedState::Left);
        relays.stop().await;
    }

    /// The whole way: a leave no relay took, a relay back, the host's next
    /// word — and the host hears the leave.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_leave_the_host_missed_is_said_again_when_the_host_speaks() {
        let (_relay, url) = local_relay().await;
        let host = Keys::generate();
        let guest = Keys::generate();
        let host_pool = Relays::start(host.clone(), std::slice::from_ref(&url)).await;
        let mut at_host = host_pool.subscribe();
        let relays = Relays::start(guest.clone(), &Vec::<String>::new()).await;
        let dir = tempfile::tempdir().unwrap();
        let session = member_of(&host, &guest, dir.path(), &relays, 20);
        session.leave().await.unwrap();
        drop(session);
        let guests = Guests::open(
            guest.clone(),
            dir.path(),
            Arc::clone(&relays),
            Arc::new(crate::faces::MemoryFaces::new()),
        )
        .unwrap();
        relays.set_relays(std::slice::from_ref(&url)).await;
        guests
            .route(&said(
                &host,
                Control::RoleChanged {
                    role: MemberRole::Member,
                },
                60,
            ))
            .await;
        let heard = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let Ok(Incoming::Control {
                    sender, control, ..
                }) = at_host.recv().await
                {
                    if sender == guest.public_key() {
                        break control;
                    }
                }
            }
        })
        .await
        .expect("the host hears");
        assert_eq!(heard, Control::Leave);
        relays.stop().await;
        host_pool.stop().await;
    }

    /// Decision 4: a post no relay took is kept, said as *not sent yet*, and
    /// sent — in the order said — when a relay answers.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_post_no_relay_took_is_kept_and_sent_when_one_answers() {
        let (_relay, url) = local_relay().await;
        let host = Keys::generate();
        let guest = Keys::generate();
        let host_pool = Relays::start(host.clone(), std::slice::from_ref(&url)).await;
        let mut at_host = host_pool.subscribe();
        let relays = Relays::start(guest.clone(), &Vec::<String>::new()).await;
        let dir = tempfile::tempdir().unwrap();
        let session = member_of(&host, &guest, dir.path(), &relays, 20);

        let first = session.post("general", "one", &[], None).await.unwrap();
        let second = session.post("general", "two", &[], None).await.unwrap();
        assert!(first.queued && second.queued);
        assert_eq!(
            session.store().unsent().unwrap(),
            vec![first.id.clone(), second.id.clone()]
        );
        let shown = session.messages("general", None, 10).unwrap();
        assert_eq!(shown.len(), 2);
        assert!(shown.iter().all(|m| m.unsent), "{shown:?}");

        // A relay answers: the next thing said carries what waited, first —
        // and a note of a fact the copy no longer holds is owed nothing.
        session.store().mark_unsent(&"0".repeat(64)).unwrap();
        relays.set_relays(std::slice::from_ref(&url)).await;
        let third = session.post("general", "three", &[], None).await.unwrap();
        assert!(!third.queued, "a relay took it");
        assert!(!session.has_unsent());
        let shown = session.messages("general", None, 10).unwrap();
        assert!(shown.iter().all(|m| !m.unsent), "{shown:?}");
        let mut order = Vec::new();
        tokio::time::timeout(Duration::from_secs(10), async {
            while order.len() < 3 {
                if let Ok(Incoming::Gep { sender, event }) = at_host.recv().await {
                    assert_eq!(sender, guest.public_key());
                    order.push(event.id.to_hex());
                }
            }
        })
        .await
        .expect("the host hears all three");
        assert_eq!(
            {
                let mut sorted = order.clone();
                sorted.sort();
                sorted
            },
            {
                let mut ids = vec![first.id, second.id, third.id];
                ids.sort();
                ids
            },
            "each of them, once"
        );
        relays.stop().await;
        host_pool.stop().await;
    }

    /// The timer's door: the router's retry sends what waited with nothing
    /// new said, and a restart does not forget what waits.
    #[tokio::test(flavor = "multi_thread")]
    async fn what_waited_is_sent_by_the_retry_after_a_restart() {
        let (_relay, url) = local_relay().await;
        let host = Keys::generate();
        let guest = Keys::generate();
        let host_pool = Relays::start(host.clone(), std::slice::from_ref(&url)).await;
        let mut at_host = host_pool.subscribe();
        let relays = Relays::start(guest.clone(), &Vec::<String>::new()).await;
        let dir = tempfile::tempdir().unwrap();
        let session = member_of(&host, &guest, dir.path(), &relays, 20);
        let kept = session.post("general", "later", &[], None).await.unwrap();
        assert!(kept.queued);
        drop(session);

        let guests = Guests::open(
            guest.clone(),
            dir.path(),
            Arc::clone(&relays),
            Arc::new(crate::faces::MemoryFaces::new()),
        )
        .unwrap();
        let session = guests
            .session(&host.public_key().to_hex())
            .await
            .expect("reopened");
        assert!(session.has_unsent(), "the copy remembers");
        relays.set_relays(std::slice::from_ref(&url)).await;
        guests.flush_unsent().await;
        assert!(!session.has_unsent());
        let heard = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let Ok(Incoming::Gep { event, .. }) = at_host.recv().await {
                    break event.id.to_hex();
                }
            }
        })
        .await
        .expect("the host hears");
        assert_eq!(heard, kept.id);
        relays.stop().await;
        host_pool.stop().await;
    }

    /// A claim no relay took changes nothing — nothing recorded, nothing
    /// listed, no session left open; and a person's own code is refused
    /// before anything is written.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_claim_that_never_left_changes_nothing() {
        let host = Keys::generate();
        let guest = Keys::generate();
        let relays = Relays::start(guest.clone(), &Vec::<String>::new()).await;
        let dir = tempfile::tempdir().unwrap();
        let guests = Guests::open(
            guest.clone(),
            dir.path(),
            Arc::clone(&relays),
            Arc::new(crate::faces::MemoryFaces::new()),
        )
        .unwrap();
        let code = InviteCode::new(host.public_key(), &Vec::<String>::new(), "ab".repeat(16));
        assert!(guests.join(&code, None, "cli").await.is_err());
        assert_eq!(guests.count().await, 0);
        assert!(guests.session(&host.public_key().to_hex()).await.is_none());
        assert!(GuestStore::list(&dir.path().join(HOSTS_DIR)).is_empty());

        let own = InviteCode::new(guest.public_key(), &Vec::<String>::new(), "cd".repeat(16));
        let refused = guests.join(&own, None, "cli").await;
        match refused {
            Err(GuestError::Refused(why)) => assert!(why.contains("your own workspace"), "{why}"),
            other => panic!("unexpected: {:?}", other.map(|(_, state)| state)),
        }
        assert_eq!(guests.count().await, 0);
        assert!(GuestStore::list(&dir.path().join(HOSTS_DIR)).is_empty());
        relays.stop().await;
    }

    /// Decision 3: a membership that is over is forgotten — its session
    /// closed, its copy moved whole to `hosts-forgotten/` — and one that
    /// stands or waits is not.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_membership_that_is_over_is_forgotten_and_its_copy_kept_aside() {
        let host = Keys::generate();
        let guest = Keys::generate();
        let relays = Relays::start(guest.clone(), &Vec::<String>::new()).await;
        let dir = tempfile::tempdir().unwrap();
        let host_hex = host.public_key().to_hex();
        let session = member_of(&host, &guest, dir.path(), &relays, 20);
        session
            .on_incoming(&Incoming::Gep {
                sender: host.public_key(),
                event: fact_in(&host, &host, "general", "kept"),
            })
            .unwrap();
        drop(session);
        let guests = Guests::open(
            guest.clone(),
            dir.path(),
            Arc::clone(&relays),
            Arc::new(crate::faces::MemoryFaces::new()),
        )
        .unwrap();
        let mut changes = guests.subscribe();
        assert!(matches!(
            guests.forget(&"0".repeat(64)).await,
            Err(GuestError::NotHosted)
        ));
        match guests.forget(&host_hex).await {
            Err(GuestError::Refused(why)) => assert!(why.contains("leave"), "{why}"),
            other => panic!("unexpected: {other:?}"),
        }
        guests
            .route(&said(&host, Control::Removed { reason: None }, 30))
            .await;
        let aside = guests.forget(&host_hex).await.unwrap();
        assert!(aside.starts_with(dir.path().join(crate::store::FORGOTTEN_DIR)));
        assert!(aside.join("host.json").is_file());
        assert!(aside.join("conversations").join("general.jsonl").is_file());
        assert!(!dir.path().join(HOSTS_DIR).join(&host_hex).exists());
        assert!(guests.session(&host_hex).await.is_none());
        assert_eq!(guests.count().await, 0);
        assert!(guests.list().await.is_empty());
        let told = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Ok((who, HostedChange::Membership)) = changes.recv().await {
                    break who;
                }
            }
        })
        .await
        .expect("the screens hear");
        assert_eq!(told, host_hex);

        // Joined, over and forgotten a second time in the same second: a
        // second copy beside the first, neither written over.
        let session = member_of(&host, &guest, dir.path(), &relays, 40);
        session.leave().await.unwrap();
        drop(session);
        let guests = Guests::open(
            guest.clone(),
            dir.path(),
            Arc::clone(&relays),
            Arc::new(crate::faces::MemoryFaces::new()),
        )
        .unwrap();
        let second = guests.forget(&host_hex).await.unwrap();
        assert_ne!(second, aside);
        assert!(aside.join("host.json").is_file() && second.join("host.json").is_file());
        relays.stop().await;
    }

    /// The host's seal is the authority: what it relays in a room this
    /// person reaches is kept whoever wrote it — somebody the directory no
    /// longer names among them — and the same fact under another seal is not.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_former_members_words_reach_a_newcomer() {
        let host = Keys::generate();
        let guest = Keys::generate();
        let former = Keys::generate();
        let relays = Relays::start(guest.clone(), &Vec::<String>::new()).await;
        let dir = tempfile::tempdir().unwrap();
        let session = member_of(&host, &guest, dir.path(), &relays, 20);
        let theirs = fact_in(&former, &host, "general", "before your time");
        session
            .on_incoming(&Incoming::Gep {
                sender: former.public_key(),
                event: theirs.clone(),
            })
            .unwrap();
        assert!(session.messages("general", None, 10).unwrap().is_empty());
        session
            .on_incoming(&Incoming::Gep {
                sender: host.public_key(),
                event: theirs.clone(),
            })
            .unwrap();
        let shown = session.messages("general", None, 10).unwrap();
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].author, pid(former.public_key()));
        // Handed back by the relay, it is the one fact.
        session
            .on_incoming(&Incoming::Gep {
                sender: host.public_key(),
                event: theirs,
            })
            .unwrap();
        assert_eq!(session.messages("general", None, 10).unwrap().len(), 1);
        relays.stop().await;
    }

    /// A fact that arrives before the word naming its room waits for it,
    /// and is in the room the moment the room is named.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_fact_before_its_room_is_kept_for_it() {
        let host = Keys::generate();
        let guest = Keys::generate();
        let relays = Relays::start(guest.clone(), &Vec::<String>::new()).await;
        let dir = tempfile::tempdir().unwrap();
        let session = member_of(&host, &guest, dir.path(), &relays, 20);
        let early = fact_in(&host, &host, "design", "first thing said");
        let elsewhere = fact_in(&host, &host, "kitchen", "never named");
        for event in [early.clone(), early.clone(), elsewhere] {
            session
                .on_incoming(&Incoming::Gep {
                    sender: host.public_key(),
                    event,
                })
                .unwrap();
        }
        assert!(session.store().events("design").unwrap().is_empty());
        let mut changes = session.subscribe();
        session
            .on_incoming(&said(
                &host,
                Control::ChannelsChanged {
                    channels: vec![room("general", 1), room("design", 2)],
                },
                30,
            ))
            .unwrap();
        let shown = session.messages("design", None, 10).unwrap();
        assert_eq!(shown.len(), 1, "{shown:?}");
        assert_eq!(shown[0].text, "first thing said");
        let mut told = Vec::new();
        while let Ok(change) = changes.try_recv() {
            told.push(change);
        }
        assert!(
            told.iter()
                .any(|c| matches!(c, HostedChange::Message { scope, id }
                if scope == "design" && *id == early.id.to_hex())),
            "{told:?}"
        );
        // The other still waits, for a room that may yet be named.
        assert_eq!(session.store().take_parked().unwrap().len(), 0);
        session
            .on_incoming(&said(
                &host,
                Control::ChannelsChanged {
                    channels: vec![room("general", 1), room("design", 2), room("kitchen", 3)],
                },
                40,
            ))
            .unwrap();
        assert_eq!(session.messages("kitchen", None, 10).unwrap().len(), 1);
        relays.stop().await;
    }

    /// Two nodes that host each other: what the host's owner said as a
    /// person in *another* workspace's `general` is sealed by the same key,
    /// and is not this host's room's.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_fact_of_another_workspace_is_not_this_hosts() {
        let host = Keys::generate();
        let guest = Keys::generate();
        let relays = Relays::start(guest.clone(), &Vec::<String>::new()).await;
        let dir = tempfile::tempdir().unwrap();
        let session = member_of(&host, &guest, dir.path(), &relays, 20);
        // A coordinate that names the workspace and no room is nobody's.
        let roomless = EventBuilder::new(Kind::Custom(KIND_MESSAGE), "{}")
            .tags([Tag::parse(["a", &format!("33405:{}", host.public_key().to_hex())]).unwrap()])
            .finalize(&host)
            .unwrap();
        session
            .on_incoming(&Incoming::Gep {
                sender: host.public_key(),
                event: roomless,
            })
            .unwrap();
        // Somebody still waiting on the host is sent no facts, and keeps none.
        let other_dir = tempfile::tempdir().unwrap();
        let waiting = waiting_on(&host, &guest, other_dir.path(), &relays);
        waiting.store().set_channels(&[room("general", 1)]).unwrap();
        waiting
            .on_incoming(&Incoming::Gep {
                sender: host.public_key(),
                event: fact_in(&host, &host, "general", "not yet yours"),
            })
            .unwrap();
        assert!(waiting.store().events("general").unwrap().is_empty());
        // The host, writing in the guest's own workspace's `general`.
        let crossing = fact_in(&host, &guest, "general", "said in yours");
        session
            .on_incoming(&Incoming::Gep {
                sender: host.public_key(),
                event: crossing,
            })
            .unwrap();
        assert!(session.messages("general", None, 10).unwrap().is_empty());
        assert!(session.store().take_parked().unwrap().is_empty());
        let here = fact_in(&host, &host, "general", "said in mine");
        session
            .on_incoming(&Incoming::Gep {
                sender: host.public_key(),
                event: here,
            })
            .unwrap();
        assert_eq!(session.messages("general", None, 10).unwrap().len(), 1);
        relays.stop().await;
    }

    /// A record that does not read is moved aside and the membership reads
    /// as none — so the workspace can be joined again; one that cannot be
    /// read at all stays an error, never taken for a broken file.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_copy_whose_record_does_not_read_can_be_joined_again() {
        let host = Keys::generate();
        let guest = Keys::generate();
        let relays = Relays::start(guest.clone(), &Vec::<String>::new()).await;
        let dir = tempfile::tempdir().unwrap();
        let session = member_of(&host, &guest, dir.path(), &relays, 20);
        let replica = session.store().dir().to_path_buf();
        std::fs::write(replica.join("host.json"), b"{ not json").unwrap();
        std::fs::write(replica.join("read.json"), b"\x00\x01").unwrap();
        assert!(session.hosted().unwrap().is_none(), "read as absent");
        assert_eq!(session.unread("general").unwrap(), 0);
        let aside: Vec<String> = std::fs::read_dir(&replica)
            .unwrap()
            .flatten()
            .filter_map(|e| e.file_name().to_str().map(str::to_string))
            .filter(|n| n.contains(".unreadable."))
            .collect();
        assert!(
            aside.iter().any(|n| n.starts_with("host."))
                && aside.iter().any(|n| n.starts_with("read.")),
            "{aside:?}"
        );
        // The host's words find no membership and change nothing; a claim
        // is made as on a fresh copy (and fails here only for want of a relay).
        assert!(session
            .on_incoming(&said(&host, Control::Removed { reason: None }, 30))
            .unwrap()
            .is_empty());
        let code = InviteCode::new(host.public_key(), &Vec::<String>::new(), "ab".repeat(16));
        let claim = session.join(&code, None, "cli").await;
        assert!(
            !matches!(claim, Err(GuestError::Json(_))),
            "the record is no longer in the way: {claim:?}"
        );
        // Unreadable is not broken.
        std::fs::create_dir(replica.join("host.json")).unwrap();
        assert!(session.hosted().is_err());
        relays.stop().await;
    }

    /// A face asked for and never sent is asked for again under the next
    /// directory — once a process, an unanswered ask left a key's first
    /// letters where a face should be until the node restarted.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_face_never_sent_is_asked_for_again() {
        let host = Keys::generate();
        let guest = Keys::generate();
        let relays = Relays::start(guest.clone(), &Vec::<String>::new()).await;
        let dir = tempfile::tempdir().unwrap();
        let session = member_of(&host, &guest, dir.path(), &relays, 20);
        let (_, alice) = alice_face();
        let directory = vec![Directory {
            pubkey: pid(host.public_key()),
            role: MemberRole::Owner,
            label: Some("Alice".into()),
            photo: Some(alice.clone()),
        }];
        for at in [30, 40] {
            let replies = session
                .on_incoming(&said(
                    &host,
                    Control::MembersChanged {
                        members: directory.clone(),
                    },
                    at,
                ))
                .unwrap();
            assert_eq!(
                replies,
                vec![Control::WantFace {
                    sha256: alice.sha256.clone()
                }],
                "asked under the directory said at {at}"
            );
        }
        // An older directory is no word at all: nothing kept, nothing asked.
        let replies = session
            .on_incoming(&said(
                &host,
                Control::MembersChanged { members: vec![] },
                35,
            ))
            .unwrap();
        assert!(replies.is_empty());
        assert_eq!(session.members().unwrap().len(), 1);
        relays.stop().await;
    }
}
