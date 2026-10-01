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
    /// directory that follows do not ask twice.
    asked: Mutex<HashSet<String>>,
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
        self.store.set_hosted(&Hosted {
            host: HostCard {
                pubkey: code.host.clone(),
                name: String::new(),
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
        self.send_control(&Control::Join {
            secret: code.secret.clone(),
            label,
            client: client.to_string(),
        })
        .await?;
        let deadline = tokio::time::Instant::now() + join_wait();
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            match tokio::time::timeout(remaining, heard.recv()).await {
                Ok(Ok(HostedChange::Membership)) => {
                    if let Some(h) = self.store.hosted()? {
                        if !matches!(h.state, HostedState::Requested) {
                            return Ok(h.state);
                        }
                    }
                }
                Ok(Ok(_)) => {}
                Ok(Err(broadcast::error::RecvError::Lagged(_))) => {}
                Ok(Err(broadcast::error::RecvError::Closed)) | Err(_) => break,
            }
        }
        Ok(self
            .store
            .hosted()?
            .map(|h| h.state)
            .unwrap_or(HostedState::Requested))
    }

    /// Leave: tell the host, keep the replica marked as left.
    pub async fn leave(&self) -> Result<(), GuestError> {
        let Some(mut hosted) = self.store.hosted()? else {
            return Err(GuestError::NotHosted);
        };
        if hosted.state.is_member() || matches!(hosted.state, HostedState::Requested) {
            // The host may be unreachable right now; leaving is this
            // person's act and stands either way.
            if let Err(e) = self.send_control(&Control::Leave).await {
                tracing::debug!("the host did not hear the leave: {e}");
            }
        }
        hosted.state = HostedState::Left;
        self.store.set_hosted(&hosted)?;
        self.say(HostedChange::Membership);
        Ok(())
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

    /// Keep a fact of this person's, then send it to the host.
    async fn commit(&self, event: Event, redacted: usize) -> Result<Posted, GuestError> {
        let scope = scope_of(&event).unwrap_or_default();
        self.store.append(&event)?;
        self.say(HostedChange::Message {
            scope: scope.clone(),
            id: event.id.to_hex(),
        });
        let wrap = wrap_for_member(&self.keys, self.host, &event)?;
        self.relays.publish(&wrap).await?;
        Ok(Posted {
            id: event.id.to_hex(),
            scope,
            redacted,
        })
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
            return Err(GuestError::Refused(format!(
                "a {} may not post here",
                hosted.role.as_str()
            )));
        }
        self.reachable(scope)?;
        let redaction = self.redactor.redact(&self.vault, text.trim());
        if redaction.text.is_empty() {
            return Err(GuestError::Refused("an empty message".into()));
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
        let event = self.build_fact(KIND_REACTION, emoji, &scope, Some((event_id, "")), &[])?;
        self.commit(event, 0).await
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
            return Err(GuestError::Refused(format!(
                "a {} may not open direct messages",
                hosted.role.as_str()
            )));
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
    /// the directory names and this node lacks — for the router to send.
    pub fn on_incoming(&self, incoming: &Incoming) -> Result<Vec<Control>, GuestError> {
        match incoming {
            Incoming::Control { sender, control } if *sender == self.host => {
                self.on_control(control)
            }
            Incoming::Gep { sender, event } if *sender == self.host => {
                self.on_fact(event)?;
                Ok(Vec::new())
            }
            _ => Ok(Vec::new()),
        }
    }

    fn on_control(&self, control: &Control) -> Result<Vec<Control>, GuestError> {
        let mut hosted = match self.store.hosted()? {
            Some(h) => h,
            None => return Ok(Vec::new()),
        };
        let mut replies = Vec::new();
        match control {
            Control::Welcome {
                workspace,
                role,
                members,
                channels,
            } => {
                hosted.host = workspace.clone();
                hosted.role = *role;
                hosted.state = HostedState::Member;
                hosted.joined_at = Some(now_secs());
                self.store.set_hosted(&hosted)?;
                self.store.set_members(members)?;
                self.store.set_channels(channels)?;
                self.say(HostedChange::Membership);
                self.say(HostedChange::Members);
                self.say(HostedChange::Channels);
                // A member now: the host hears who this person is, and is
                // asked for the faces it named that this node lacks.
                replies.push(self.profile_control());
                replies.extend(self.faces_wanted(members));
            }
            Control::Waiting => {
                hosted.state = HostedState::Requested;
                self.store.set_hosted(&hosted)?;
                self.say(HostedChange::Membership);
            }
            // A refusal answers a claim; a member made none, so a refusal
            // that reaches one changes nothing.
            Control::Refused { .. } if matches!(hosted.state, HostedState::Member) => {}
            Control::Refused { reason } => {
                hosted.state = HostedState::Refused {
                    reason: reason.clone(),
                };
                self.store.set_hosted(&hosted)?;
                self.say(HostedChange::Membership);
            }
            // A removal that answers this person's own leave is the host's
            // housekeeping, not a new fact: the person left, and the record
            // keeps saying so whichever word reaches it first.
            Control::Removed { .. } if matches!(hosted.state, HostedState::Left) => {}
            Control::Removed { reason } => {
                hosted.state = HostedState::Removed {
                    reason: reason.clone(),
                };
                self.store.set_hosted(&hosted)?;
                self.say(HostedChange::Membership);
            }
            Control::RoleChanged { role } => {
                hosted.role = *role;
                self.store.set_hosted(&hosted)?;
                self.say(HostedChange::Membership);
            }
            Control::ChannelsChanged { channels } => {
                self.store.set_channels(channels)?;
                self.say(HostedChange::Channels);
            }
            Control::MembersChanged { members } => {
                self.store.set_members(members)?;
                self.say(HostedChange::Members);
                replies.extend(self.faces_wanted(members));
            }
            Control::Face { face } => match face.decode() {
                Ok(bytes) => {
                    self.asked.locked().remove(&face.sha256);
                    match self.faces.hold(&face.sha256, &bytes) {
                        Ok(()) => self.say(HostedChange::Members),
                        Err(e) => tracing::warn!("keeping a face from the host: {e}"),
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

    /// A fact the host relayed: verified, by an author the directory names
    /// (the host itself, this person, or a listed member — a host's agent's
    /// key is listed under the host by attestation, which the host already
    /// checked), in a scope this person reaches.
    fn on_fact(&self, event: &Event) -> Result<(), GuestError> {
        if !matches!(
            event.kind.as_u16(),
            k if k == KIND_MESSAGE || k == KIND_REACTION || k == KIND_RETRACTION
        ) {
            return Ok(());
        }
        if event.verify().is_err() {
            tracing::debug!("a relayed fact with a bad signature — dropped");
            return Ok(());
        }
        let Some(scope) = scope_of(event) else {
            return Ok(());
        };
        if self.store.channel(&scope)?.is_none() {
            tracing::debug!(
                "a relayed fact in {scope}, which this person does not reach — dropped"
            );
            return Ok(());
        }
        let author = event.pubkey;
        let listed = author == self.host
            || author == self.keys.public_key()
            || self
                .store
                .members()?
                .iter()
                .any(|m| m.pubkey.as_hex() == author.to_hex())
            || bisa_store_free_attestation_names(event, &self.host);
        if !listed {
            tracing::debug!(
                "a relayed fact by {author}, whom the directory does not name — dropped"
            );
            return Ok(());
        }
        if self.store.append(event)? {
            self.say(HostedChange::Message {
                scope,
                id: event.id.to_hex(),
            });
        }
        Ok(())
    }
}

/// Whether an event carries the host's attestation of its author — the
/// NIP-OA `auth` tag the host's agents sign under: `["auth", owner_hex,
/// conditions, sig]`. The host checked the signature before relaying; here
/// it is enough that the tag names the host.
fn bisa_store_free_attestation_names(event: &Event, host: &PublicKey) -> bool {
    event.tags.iter().any(|t| {
        let s = t.as_slice();
        s.len() >= 4 && s[0] == "auth" && s[1] == host.to_hex()
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
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
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
                continue;
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
                Err(e) => tracing::warn!("cannot open the replica of {host_hex}: {e}"),
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
                tracing::debug!(
                    "the host {} did not hear the profile: {e}",
                    session.host_hex()
                );
            }
        }
    }

    /// Forward one session's changes onto the supervisor's stream, keyed by
    /// host.
    fn relay_changes(&self, session: &Arc<GuestSession>) {
        forward_changes(self.changed.clone(), session);
    }

    /// Route the relay pool's inbound stream to the sessions, for as long
    /// as the task lives.
    pub fn spawn(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let this = Arc::clone(self);
        let mut rx = this.relays.subscribe();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(incoming) => this.route(&incoming).await,
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("guest inbound lagged by {n}; the next catch-up brings it");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        })
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
                        }
                    }
                }
                Err(e) => tracing::warn!("applying the host {sender}'s word: {e}"),
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

    pub async fn count(&self) -> usize {
        self.sessions.read().await.len()
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
        let state = session.join(code, label, client).await?;
        Ok((session, state))
    }

    /// Leave one host. The session stays open, marked as left, so its
    /// section can say so; the replica stays on disk.
    pub async fn leave(&self, host_hex: &str) -> Result<(), GuestError> {
        let session = self.session(host_hex).await.ok_or(GuestError::NotHosted)?;
        session.leave().await
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

        // A fact from the host in a reached channel lands; a stranger's does not.
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
        assert_eq!(now.len(), 2, "{now:?}");
        assert!(now.iter().any(|m| m.text == "welcome bob"));
        assert!(!now.iter().any(|m| m.text == "boo"));
        assert_eq!(session.unread("general").unwrap(), 1);
        session.mark_read("general").unwrap();
        assert_eq!(session.unread("general").unwrap(), 0);

        // A reaction and a retraction of one's own, and never of the host's.
        let reacted = session.react(&hosts.id.to_hex(), "👍").await.unwrap();
        assert_eq!(reacted.scope, "general");
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
            })
            .unwrap();
        assert_eq!(session.hosted().unwrap().unwrap().role, MemberRole::Guest);
        session
            .on_incoming(&Incoming::Control {
                sender: host.public_key(),
                control: Control::Removed {
                    reason: Some("done".into()),
                },
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
            })
            .unwrap();
        assert_eq!(
            session.hosted().unwrap().unwrap().state,
            HostedState::Left,
            "a removal that answers a leave is the host's housekeeping, not a new fact"
        );
        relays.stop().await;
    }

    #[test]
    fn the_host_s_attestation_tag_names_the_host() {
        let host = Keys::generate();
        let agent = Keys::generate();
        let ev = EventBuilder::new(Kind::Custom(KIND_MESSAGE), "{}")
            .tags([Tag::parse(["auth", &host.public_key().to_hex(), "", "sig"]).unwrap()])
            .finalize(&agent)
            .unwrap();
        assert!(bisa_store_free_attestation_names(&ev, &host.public_key()));
        assert!(!bisa_store_free_attestation_names(&ev, &agent.public_key()));
    }
}
