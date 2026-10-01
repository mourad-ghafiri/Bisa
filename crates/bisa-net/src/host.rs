//! The host pump: what this workspace says to the people it hosts, and
//! what it does with what they say.
//!
//! **Outbound.** Every store event is read for who reaches it: a
//! conversation fact goes pairwise to each hosted member whose role reaches
//! its channel ([`crate::truth::reaches_fact`]) — never back to its author;
//! a channel change sends each person the channels they now reach, whole,
//! and the backlog of any channel new to them; a person joining is
//! welcomed with the host's card, their role, the directory and their
//! channels, then handed the backlog; a role change, a removal and a
//! refusal are said to the one they concern, and the directory to everyone
//! else. A `(member, event)` ledger on disk keeps each fact delivered once
//! across restarts and one-shot processes.
//!
//! **Inbound.** A fact sealed by a hosted member goes to the store's ingest
//! ladder, which admits a human's acts in the channels they reach and
//! refuses everything else by role — nothing here decides that. A `Join`
//! claims an invite (the store's single-use, expiring, constant-time
//! claim) and admits at once or, under `collab.join = ask`, records the
//! request for the owner's Inbox. `Leave` removes; `OpenDm` opens a direct
//! channel through the store's own door.
//!
//! The pump owns no relay: it sits on the pool the node opened
//! ([`bisa_collab::Relays`]) and shares it with the guest sessions.

use crate::config::NetConfig;
use crate::truth::{conversation_facts, reaches_fact};
use crate::NetError;
use bisa_collab::{
    hash_secret, wrap_control, wrap_for_member, Control, Directory, Face, HostCard, Incoming,
    RelayHealth, Relays, CLIENT_CLI, CLIENT_DESKTOP, CLIENT_MOBILE,
};
use bisa_core::kind::KIND_CHANNEL;
use bisa_core::{
    clean_label, CollabSettings, InviteState, JoinPolicy, MemberRole, Permission, PrincipalId,
};
use bisa_store::{IngestOutcome, PeopleChange, StoreEvent, Workspace};
use nostr::event::Event;
use nostr::key::{Keys, PublicKey};
use nostr::types::Timestamp;
use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

/// What the node reports about the wire.
#[derive(Clone, Debug, Default)]
pub struct SyncStatus {
    pub relays: Vec<RelayHealth>,
    pub connected_relays: usize,
    pub last_catchup: Option<u64>,
    pub published: u64,
    pub ingested: u64,
    /// People hosted here.
    pub people: usize,
    /// This node's iroh endpoint id (present when the direct transport runs).
    pub iroh_node_id: Option<String>,
    /// Members with a live authenticated iroh session.
    pub iroh_peers_connected: usize,
}

pub(crate) struct Inner {
    pub(crate) ws: Arc<Workspace>,
    pub(crate) keys: Keys,
    pub(crate) relays: Arc<Relays>,
    pub(crate) data_dir: PathBuf,
    pub(crate) cfg: NetConfig,
    interval_secs: AtomicU64,
    published: AtomicU64,
    ingested: AtomicU64,
    last_catchup: AtomicU64, // 0 = never
    outbox: Mutex<VecDeque<OutboxItem>>,
    /// (member hex, inner event id) pairs already delivered. Persisted
    /// append-only at `net_published.jsonl`.
    ledger: Mutex<HashSet<(String, String)>>,
    /// Direct QUIC state (None: feature off, disabled, or bind failed).
    #[cfg(feature = "iroh")]
    pub(crate) iroh: Option<Arc<crate::iroh_sync::IrohState>>,
}

struct OutboxItem {
    wrap: Event,
    member: String,
    inner_id: String,
}

fn ledger_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("net_published.jsonl")
}

fn load_ledger(data_dir: &std::path::Path) -> HashSet<(String, String)> {
    let mut set = HashSet::new();
    if let Ok(body) = std::fs::read_to_string(ledger_path(data_dir)) {
        for line in body.lines().filter(|l| !l.trim().is_empty()) {
            if let Some((m, e)) = line.split_once(' ') {
                set.insert((m.to_string(), e.to_string()));
            }
        }
    }
    set
}

async fn ledger_record(inner: &Arc<Inner>, member: &str, inner_id: &str) {
    let mut ledger = inner.ledger.lock().await;
    if ledger.insert((member.to_string(), inner_id.to_string())) {
        use std::io::Write;
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(ledger_path(&inner.data_dir))
        {
            Ok(mut f) => {
                if let Err(e) = writeln!(f, "{member} {inner_id}") {
                    tracing::warn!("the ledger did not take a line: {e}");
                }
            }
            Err(e) => tracing::warn!("the ledger could not be opened: {e}"),
        }
    }
}

/// How long a peer gets to answer a request for an attachment. Long enough
/// for 25 MB over a home connection, short enough that a request aimed at a
/// peer that has gone quiet does not hold an HTTP handler open.
#[cfg(feature = "iroh")]
const ATTACHMENT_FETCH_TIMEOUT: Duration = Duration::from_secs(60);

/// The one capability the node borrows from a running pump.
#[derive(Clone)]
pub struct AttachmentFetch {
    #[cfg_attr(not(feature = "iroh"), allow(dead_code))]
    inner: Arc<Inner>,
}

impl AttachmentFetch {
    /// `false` when nobody answered — including when the direct transport
    /// is not running, the ordinary case for a relay-only node.
    pub async fn fetch(&self, sha256: &str) -> bool {
        #[cfg(feature = "iroh")]
        {
            if let Some(st) = self.inner.iroh.as_ref() {
                return st
                    .request_blob(&self.inner.ws, sha256, ATTACHMENT_FETCH_TIMEOUT)
                    .await;
            }
        }
        let _ = sha256;
        false
    }
}

/// The running pump.
pub struct Host {
    inner: Arc<Inner>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl Host {
    /// Start the pump on `relays` for `ws`: the inbound reader, the outbound
    /// store-event pump and the periodic catch-up. Returns after a first
    /// catch-up and backlog pass, so a one-shot process sees history.
    pub async fn start(
        ws: Arc<Workspace>,
        relays: Arc<Relays>,
        cfg: NetConfig,
        data_dir: PathBuf,
    ) -> Result<Self, NetError> {
        let keys = ws.owner_keys().clone();
        let ledger_init = load_ledger(&data_dir);
        #[cfg(feature = "iroh")]
        let iroh_state = if cfg.iroh {
            match crate::iroh_sync::IrohState::bind(&data_dir, &cfg).await {
                Ok(st) => Some(Arc::new(st)),
                Err(e) => {
                    tracing::warn!("iroh endpoint bind failed ({e}); continuing relay-only");
                    None
                }
            }
        } else {
            None
        };
        let inner = Arc::new(Inner {
            ws,
            keys,
            relays,
            data_dir,
            interval_secs: AtomicU64::new(cfg.sync_interval_secs.max(1)),
            cfg,
            published: AtomicU64::new(0),
            ingested: AtomicU64::new(0),
            last_catchup: AtomicU64::new(0),
            outbox: Mutex::new(VecDeque::new()),
            ledger: Mutex::new(ledger_init),
            #[cfg(feature = "iroh")]
            iroh: iroh_state,
        });

        if inner.cfg.publish_relay_list {
            inner.relays.publish_relay_list().await;
        }

        let mut tasks = Vec::new();

        // Inbound: what the pool heard for this key.
        {
            let inner = Arc::clone(&inner);
            let mut rx = inner.relays.subscribe();
            tasks.push(tokio::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok(incoming) => on_incoming(&inner, &incoming).await,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(
                                "host inbound lagged by {n}; the next catch-up brings it"
                            );
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            }));
        }

        // Outbound: what the store said.
        {
            let inner = Arc::clone(&inner);
            let mut rx = inner.ws.subscribe_store_events();
            tasks.push(tokio::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok(ev) => on_store_event(&inner, ev).await,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!("store event bus lagged by {n}; next catch-up resyncs");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            }));
        }

        // Periodic catch-up + backlog + flush. The interval is read each
        // round, so a settings change takes effect without a restart.
        {
            let inner = Arc::clone(&inner);
            tasks.push(tokio::spawn(async move {
                loop {
                    let secs = inner.interval_secs.load(Ordering::Relaxed).max(1);
                    tokio::time::sleep(Duration::from_secs(secs)).await;
                    catch_up(&inner).await;
                    publish_backlog(&inner).await;
                    flush_outbox(&inner).await;
                }
            }));
        }

        #[cfg(feature = "iroh")]
        if inner.iroh.is_some() {
            crate::iroh_sync::spawn_loops(&inner, &mut tasks);
            crate::iroh_sync::announce_addrs(&inner).await;
        }

        catch_up(&inner).await;
        publish_backlog(&inner).await;
        flush_outbox(&inner).await;

        Ok(Self { inner, tasks })
    }

    /// The wire as it stands.
    pub async fn status(&self) -> SyncStatus {
        let last = self.inner.last_catchup.load(Ordering::Relaxed);
        let relays = self.inner.relays.health().await;
        SyncStatus {
            connected_relays: relays.iter().filter(|r| r.connected).count(),
            relays,
            last_catchup: (last != 0).then_some(last),
            published: self.inner.published.load(Ordering::Relaxed),
            ingested: self.inner.ingested.load(Ordering::Relaxed),
            people: self.inner.ws.people().map(|p| p.len()).unwrap_or(0),
            #[cfg(feature = "iroh")]
            iroh_node_id: self.inner.iroh.as_ref().map(|st| st.node_id()),
            #[cfg(not(feature = "iroh"))]
            iroh_node_id: None,
            #[cfg(feature = "iroh")]
            iroh_peers_connected: self
                .inner
                .iroh
                .as_ref()
                .map(|st| st.peers_connected())
                .unwrap_or(0),
            #[cfg(not(feature = "iroh"))]
            iroh_peers_connected: 0,
        }
    }

    /// Take a changed `sync.*` reading: the relays the pool should match and
    /// the interval the next round sleeps. The direct transport's flags need
    /// a restart and are left as they were.
    pub async fn apply(&self, cfg: &NetConfig) {
        self.inner
            .interval_secs
            .store(cfg.sync_interval_secs.max(1), Ordering::Relaxed);
        self.inner.relays.set_relays(&cfg.relays).await;
        if cfg.publish_relay_list {
            self.inner.relays.publish_relay_list().await;
        }
    }

    /// This node's iroh endpoint identity and directly-dialable addresses.
    #[cfg(feature = "iroh")]
    pub fn iroh_endpoint(&self) -> Option<(String, Vec<String>)> {
        self.inner
            .iroh
            .as_ref()
            .map(|st| (st.node_id(), st.direct_addrs()))
    }

    /// A cheap, cloneable handle that can only fetch attachments.
    pub fn attachment_fetcher(&self) -> AttachmentFetch {
        AttachmentFetch {
            inner: Arc::clone(&self.inner),
        }
    }

    /// One immediate catch-up, backlog pass and flush.
    pub async fn catch_up_now(&self) {
        catch_up(&self.inner).await;
        publish_backlog(&self.inner).await;
        flush_outbox(&self.inner).await;
    }

    /// Stop the pumps. The relay pool is the node's and stays open.
    pub async fn stop(mut self) {
        for t in self.tasks.drain(..) {
            t.abort();
        }
    }
}

// -- who ---------------------------------------------------------------------

/// The role of a hosted member by pubkey hex; `None` for the owner, an
/// agent or a stranger.
pub(crate) fn hosted_role(inner: &Inner, hex: &str) -> Option<(PrincipalId, MemberRole)> {
    let person = PrincipalId::new(hex.to_string()).ok()?;
    let role = inner.ws.member_role(&person).ok().flatten()?;
    role.is_hosted().then_some((person, role))
}

fn public_key(person: &PrincipalId) -> Option<PublicKey> {
    PublicKey::from_hex(person.as_hex()).ok()
}

async fn host_card(inner: &Inner) -> HostCard {
    let name = inner
        .ws
        .settings(None)
        .map(|r| CollabSettings::from_resolved(&r))
        .unwrap_or_default()
        .display_name(&inner.keys.public_key().to_hex());
    HostCard {
        pubkey: inner.ws.owner_principal(),
        name,
        relays: inner.relays.urls().await,
    }
}

fn directory(inner: &Inner) -> Vec<Directory> {
    inner
        .ws
        .members()
        .unwrap_or_default()
        .into_iter()
        .map(|m| Directory {
            pubkey: m.pubkey,
            role: m.role,
            label: m.label,
            photo: m.photo,
        })
        .collect()
}

/// A member's face as they sent it, kept when it is one: the bytes decoded
/// under the cap, hashed as claimed, a picture (`Face::decode`), and put in
/// the attachment store under its hash. `None` — said in the log — when it
/// is not, so the label still lands and the row keeps its old face.
fn keep_face(inner: &Inner, sender: &PublicKey, face: &Face) -> bool {
    match face.decode() {
        Ok(bytes) => match inner.ws.accept_attachment(&face.sha256, &bytes) {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!("keeping {sender}'s face: {e}");
                false
            }
        },
        Err(refusal) => {
            tracing::info!("{sender} sent a face that is not one: {refusal}");
            false
        }
    }
}

/// The bytes of a face this workspace holds, for a member who lacks it —
/// a face is public to the workspace by construction; anything that is not
/// one (`bisa_core::is_face`) is answered with nothing.
fn face_held(inner: &Inner, sha256: &str) -> Option<Face> {
    if !bisa_core::AttachmentRef::is_valid_hash(sha256) {
        return None;
    }
    let bytes = inner.ws.attachment_bytes(sha256).ok().flatten()?;
    bisa_core::is_face(&bytes).then(|| Face::from_bytes(&bytes))
}

fn known_client(client: &str) -> Option<String> {
    [CLIENT_DESKTOP, CLIENT_MOBILE, CLIENT_CLI]
        .contains(&client)
        .then(|| client.to_string())
}

// -- outbound ------------------------------------------------------------------

/// Send one control message to one person over the pool (best effort).
pub(crate) async fn send_control(inner: &Arc<Inner>, member: PublicKey, control: &Control) {
    if inner.relays.count().await == 0 {
        return;
    }
    match wrap_control(&inner.keys, member, control) {
        Ok(w) => {
            if let Err(e) = inner.relays.publish(&w).await {
                tracing::debug!("control {} to {member} failed: {e}", control.kind());
            }
        }
        Err(e) => tracing::warn!("control wrap for {member} failed: {e}"),
    }
}

async fn send_channels(inner: &Arc<Inner>, person: &PrincipalId, role: MemberRole) {
    let Some(pk) = public_key(person) else {
        return;
    };
    let channels = inner
        .ws
        .channels_reached_by(person, role)
        .unwrap_or_default();
    send_control(inner, pk, &Control::ChannelsChanged { channels }).await;
}

/// The directory to every hosted member but `except`.
async fn send_directory(inner: &Arc<Inner>, except: Option<&PrincipalId>) {
    let members = directory(inner);
    for p in inner.ws.people().unwrap_or_default() {
        if except == Some(&p.pubkey) {
            continue;
        }
        if let Some(pk) = public_key(&p.pubkey) {
            send_control(
                inner,
                pk,
                &Control::MembersChanged {
                    members: members.clone(),
                },
            )
            .await;
        }
    }
}

async fn welcome(inner: &Arc<Inner>, person: &PrincipalId, role: MemberRole) {
    let Some(pk) = public_key(person) else {
        return;
    };
    let control = Control::Welcome {
        workspace: host_card(inner).await,
        role,
        members: directory(inner),
        channels: inner
            .ws
            .channels_reached_by(person, role)
            .unwrap_or_default(),
    };
    send_control(inner, pk, &control).await;
}

/// Wrap one fact pairwise to each recipient not yet in the ledger for it.
/// Returns how many wraps were queued.
async fn enqueue_pairwise(inner: &Arc<Inner>, event: &Event, recipients: &[PublicKey]) -> usize {
    let me = inner.keys.public_key();
    let inner_id = event.id.to_hex();
    let has_relays = inner.relays.count().await > 0;
    let ledger = inner.ledger.lock().await;
    let mut outbox = inner.outbox.lock().await;
    let mut queued = 0usize;
    for pk in recipients {
        if *pk == me {
            continue;
        }
        let member_hex = pk.to_hex();
        if ledger.contains(&(member_hex.clone(), inner_id.clone())) {
            continue;
        }
        match wrap_for_member(&inner.keys, *pk, event) {
            Ok(wrapped) => {
                #[cfg(feature = "iroh")]
                crate::iroh_sync::live_push(inner, Some(&member_hex), &wrapped).await;
                if has_relays {
                    outbox.push_back(OutboxItem {
                        wrap: wrapped,
                        member: member_hex,
                        inner_id: inner_id.clone(),
                    });
                }
                queued += 1;
            }
            Err(e) => tracing::warn!("pairwise wrap for {member_hex} failed: {e}"),
        }
    }
    queued
}

/// The hosted members a fact reaches, its author left out.
fn reachers_of(inner: &Inner, event: &Event, only: Option<&PrincipalId>) -> Vec<PublicKey> {
    let author = event.pubkey.to_hex();
    inner
        .ws
        .people()
        .unwrap_or_default()
        .into_iter()
        .filter(|p| only.is_none_or(|o| o == &p.pubkey))
        .filter(|p| p.pubkey.as_hex() != author)
        .filter(|p| reaches_fact(&inner.ws, event, &p.pubkey, p.role))
        .filter_map(|p| public_key(&p.pubkey))
        .collect()
}

/// Relay one fact to everyone who reaches it.
async fn relay_fact(inner: &Arc<Inner>, event: &Event) {
    let recipients = reachers_of(inner, event, None);
    if recipients.is_empty() {
        return;
    }
    enqueue_pairwise(inner, event, &recipients).await;
    flush_outbox(inner).await;
}

/// Every fact one person reaches and has not been sent.
async fn backlog_for(inner: &Arc<Inner>, person: &PrincipalId) {
    let Ok(facts) = conversation_facts(&inner.ws) else {
        return;
    };
    for fact in &facts {
        let recipients = reachers_of(inner, fact, Some(person));
        if !recipients.is_empty() {
            enqueue_pairwise(inner, fact, &recipients).await;
        }
    }
    flush_outbox(inner).await;
}

/// Everything every person reaches and has not been sent — what a
/// one-shot process wrote while no pump ran.
async fn publish_backlog(inner: &Arc<Inner>) {
    if inner.relays.count().await == 0 {
        return;
    }
    let Ok(facts) = conversation_facts(&inner.ws) else {
        return;
    };
    for fact in &facts {
        let recipients = reachers_of(inner, fact, None);
        if !recipients.is_empty() {
            enqueue_pairwise(inner, fact, &recipients).await;
        }
    }
}

/// Try to publish everything queued; failures stay queued for the next flush.
async fn flush_outbox(inner: &Arc<Inner>) {
    let mut queued = {
        let mut outbox = inner.outbox.lock().await;
        std::mem::take(&mut *outbox)
    };
    let mut still = VecDeque::new();
    while let Some(item) = queued.pop_front() {
        match inner.relays.publish(&item.wrap).await {
            Ok(()) => {
                inner.published.fetch_add(1, Ordering::Relaxed);
                ledger_record(inner, &item.member, &item.inner_id).await;
            }
            Err(e) => {
                tracing::debug!("publish failed ({e}); will retry");
                still.push_back(item);
            }
        }
    }
    if !still.is_empty() {
        let mut outbox = inner.outbox.lock().await;
        still.append(&mut *outbox);
        *outbox = still;
    }
}

async fn catch_up(inner: &Arc<Inner>) {
    let last = inner.last_catchup.load(Ordering::Relaxed);
    inner.relays.catch_up((last != 0).then_some(last)).await;
    inner
        .last_catchup
        .store(Timestamp::now().as_secs(), Ordering::Relaxed);
}

async fn on_store_event(inner: &Arc<Inner>, ev: StoreEvent) {
    match ev {
        StoreEvent::ConversationAppended { event, .. }
        | StoreEvent::RemoteMessageArrived { event, .. } => relay_fact(inner, &event).await,
        StoreEvent::ConversationSnapshot { kind, .. } if kind == KIND_CHANNEL => {
            // A channel was made, changed or rostered: everyone's reach may
            // have moved, so everyone hears their channels whole, and the
            // backlog of any channel new to them.
            for p in inner.ws.people().unwrap_or_default() {
                send_channels(inner, &p.pubkey, p.role).await;
                backlog_for(inner, &p.pubkey).await;
            }
        }
        StoreEvent::PeopleChanged { pubkey, change } => match change {
            PeopleChange::Joined { role } => {
                welcome(inner, &pubkey, role).await;
                backlog_for(inner, &pubkey).await;
                send_directory(inner, Some(&pubkey)).await;
            }
            PeopleChange::RoleChanged { role } => {
                if let Some(pk) = public_key(&pubkey) {
                    send_control(inner, pk, &Control::RoleChanged { role }).await;
                }
                send_channels(inner, &pubkey, role).await;
                backlog_for(inner, &pubkey).await;
                send_directory(inner, Some(&pubkey)).await;
            }
            PeopleChange::Left => {
                if let Some(pk) = public_key(&pubkey) {
                    send_control(inner, pk, &Control::Removed { reason: None }).await;
                }
                send_directory(inner, Some(&pubkey)).await;
                #[cfg(feature = "iroh")]
                if let Some(st) = &inner.iroh {
                    st.forget_peer(pubkey.as_hex()).await;
                }
            }
            // A label or a face moved — a member's, or the owner's own: the
            // directory, whole, to everyone, the changer included.
            PeopleChange::ProfileChanged => send_directory(inner, None).await,
        },
        StoreEvent::InviteChanged { invite } => {
            if let InviteState::Refused { by, .. } = &invite.state {
                if let Some(pk) = public_key(by) {
                    send_control(
                        inner,
                        pk,
                        &Control::Refused {
                            reason: "the host declined".into(),
                        },
                    )
                    .await;
                }
            }
        }
        // Goals, journals, snapshots of anything but a channel, and read
        // markers stay on this node.
        StoreEvent::JournalAppended { .. }
        | StoreEvent::SnapshotWritten { .. }
        | StoreEvent::ConversationSnapshot { .. }
        | StoreEvent::ReadMarkerSet { .. } => {}
    }
}

// -- inbound -------------------------------------------------------------------

/// Unwrap one raw gift wrap with the owner's key and apply it. Returns the
/// inner event id when it carried one — the iroh path marks it seen.
#[cfg_attr(not(feature = "iroh"), allow(dead_code))]
pub(crate) async fn apply_wrap(inner: &Arc<Inner>, wrap: &Event) -> Option<String> {
    match bisa_collab::unwrap_incoming(&inner.keys, wrap) {
        Ok(incoming) => {
            let id = match &incoming {
                Incoming::Gep { event, .. } => Some(event.id.to_hex()),
                _ => None,
            };
            on_incoming(inner, &incoming).await;
            id
        }
        Err(e) => {
            tracing::debug!("cannot unwrap gift {}: {e}", wrap.id);
            None
        }
    }
}

/// Apply one thing the pool heard for the owner's key.
pub(crate) async fn on_incoming(inner: &Arc<Inner>, incoming: &Incoming) {
    match incoming {
        Incoming::Gep { sender, event } => {
            if hosted_role(inner, &sender.to_hex()).is_none() {
                tracing::debug!("a fact sealed by {sender}, who is not hosted here — ignored");
                return;
            }
            let ws = Arc::clone(&inner.ws);
            let event_id = event.id.to_hex();
            let event = event.clone();
            let outcome = tokio::task::spawn_blocking(move || ws.ingest_remote_event(&event)).await;
            match outcome {
                Ok(Ok(IngestOutcome::AppliedConversation { scope })) => {
                    inner.ingested.fetch_add(1, Ordering::Relaxed);
                    tracing::info!("a person's fact landed in {scope}");
                }
                Ok(Ok(IngestOutcome::AppliedJournal { home })) => {
                    inner.ingested.fetch_add(1, Ordering::Relaxed);
                    tracing::info!("a person's decision landed on {home}");
                }
                Ok(Ok(IngestOutcome::AppliedSnapshot { kind, .. })) => {
                    inner.ingested.fetch_add(1, Ordering::Relaxed);
                    tracing::info!("a person's {kind} snapshot landed");
                }
                Ok(Ok(IngestOutcome::Duplicate)) => {}
                Ok(Ok(IngestOutcome::Rejected { reason })) if reason == "stale" => {}
                Ok(Ok(IngestOutcome::Rejected { reason })) => {
                    tracing::warn!("ingest refused a fact from {sender}: {reason}");
                }
                Ok(Err(e)) => {
                    tracing::warn!(event = %event_id, "ingest of a fact from {sender} failed: {e}")
                }
                Err(e) => {
                    tracing::warn!(event = %event_id, "the ingest task for a fact from {sender} did not finish: {e}")
                }
            }
        }
        Incoming::Control { sender, control } => on_control(inner, *sender, control).await,
        Incoming::Other => {}
    }
}

async fn on_control(inner: &Arc<Inner>, sender: PublicKey, control: &Control) {
    if !control.is_members_word() {
        tracing::debug!("{sender} sent a host's word ({}) — ignored", control.kind());
        return;
    }
    let Ok(person) = PrincipalId::new(sender.to_hex()) else {
        return;
    };
    match control {
        Control::Join {
            secret,
            label,
            client,
        } => {
            let settings = inner.ws.settings(None).unwrap_or_default();
            let collab = CollabSettings::from_resolved(&settings);
            let label = clean_label(label.clone());
            let waits = collab.join == JoinPolicy::Ask;
            match inner
                .ws
                .claim_invite(&hash_secret(secret), &person, label.clone(), waits)
            {
                Ok(claimed) if claimed.waits => {
                    tracing::info!("{sender} asked to join; the owner admits by hand");
                    send_control(inner, sender, &Control::Waiting).await;
                }
                Ok(claimed) => {
                    match inner.ws.admit_claimed(
                        &claimed.invite,
                        &person,
                        label,
                        known_client(client),
                    ) {
                        Ok(member) => {
                            tracing::info!("admitted {sender} as {}", member.role.as_str());
                            // The welcome rides the store's `PeopleChanged`.
                        }
                        Err(e) => {
                            tracing::warn!("admitting {sender}: {e}");
                            send_control(
                                inner,
                                sender,
                                &Control::Refused {
                                    reason: e.to_string(),
                                },
                            )
                            .await;
                        }
                    }
                }
                Err(refusal) => {
                    tracing::info!("join from {sender} refused: {refusal}");
                    send_control(
                        inner,
                        sender,
                        &Control::Refused {
                            reason: refusal.to_string(),
                        },
                    )
                    .await;
                }
            }
        }
        Control::Leave => {
            if hosted_role(inner, &sender.to_hex()).is_none() {
                return;
            }
            let ws = &inner.ws;
            if let Err(e) = ws
                .unroster_human_everywhere(&person)
                .and_then(|_| ws.drop_held_of(&person))
                .and_then(|_| ws.remove_member(&person))
            {
                tracing::warn!("removing {sender} who left: {e}");
            } else {
                tracing::info!("{sender} left");
            }
        }
        Control::OpenDm { participants } => {
            let Some((_, role)) = hosted_role(inner, &sender.to_hex()) else {
                return;
            };
            if !role.may(Permission::OpenDms) {
                tracing::info!(
                    "{sender} may not open direct messages as a {}",
                    role.as_str()
                );
                return;
            }
            let mut all = participants.clone();
            all.push(person.clone());
            match inner.ws.open_dm(&all) {
                // The channel's snapshot, when new, reaches everyone through
                // the store's bus; an existing one is said to the asker here.
                Ok(_) => send_channels(inner, &person, role).await,
                Err(e) => tracing::info!("{sender} asked for a direct message: {e}"),
            }
        }
        Control::Profile { label, photo, face } => {
            // A profile is a hosted member's word about themselves; a stranger's
            // is ignored before a byte of it is decoded.
            if hosted_role(inner, &sender.to_hex()).is_none() {
                tracing::debug!("profile from {sender}, not hosted — ignored");
                return;
            }
            // The face's ref is kept only with its bytes in hand and honest
            // (`keep_face`): a ref without bytes, or bytes that are not the
            // face named, changes nothing of the picture.
            let photo = match (photo, face) {
                (Some(r), Some(f)) if f.sha256 == r.sha256 && keep_face(inner, &sender, f) => {
                    Some(Some(r.clone()))
                }
                (Some(_), _) => None,
                (None, _) => Some(None),
            };
            let label = label.as_ref().map(|l| Some(l.clone()));
            match inner.ws.set_member_profile(&person, label, photo) {
                Ok(_) => tracing::info!("{sender} set their profile"),
                Err(e) => tracing::warn!("profile from {sender}: {e}"),
            }
        }
        Control::WantFace { sha256 } => {
            if hosted_role(inner, &sender.to_hex()).is_none() {
                return;
            }
            if let Some(face) = face_held(inner, sha256) {
                send_control(inner, sender, &Control::Face { face }).await;
            }
        }
        Control::IrohAddr { node_id, addrs } => {
            if hosted_role(inner, &sender.to_hex()).is_none() {
                tracing::debug!("iroh addr announcement from {sender}, not hosted — ignored");
                return;
            }
            #[cfg(feature = "iroh")]
            if let Some(st) = &inner.iroh {
                st.learn_peer(&sender.to_hex(), node_id, addrs).await;
            }
            #[cfg(not(feature = "iroh"))]
            {
                let _ = (node_id, addrs);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_is_the_core_s_rule_and_a_client_is_a_known_word() {
        assert_eq!(clean_label(Some("Bob\u{7}".into())), Some("Bob".into()));
        assert_eq!(known_client("mobile"), Some("mobile".into()));
        assert_eq!(known_client("toaster"), None);
    }
}
