//! The host pump: what this workspace says to the people it hosts, and
//! what it does with what they say.
//!
//! **Outbound.** Every store event is read for who reaches it: a
//! conversation fact goes pairwise to each hosted member whose role reaches
//! its channel ([`crate::truth::reaches_fact`]) — never back to its author.
//! What a person is *told* — that they are welcome, their role, their
//! rooms, the directory, that they were removed, that a request was
//! refused — has one rule ([`reconcile`]): the pump keeps what a relay last
//! took from it for each person (`net_people.json`, [`crate::told`]) and
//! sends whatever differs from what stands, at start, on every change of
//! the people or the channels, and on every round while something is owed.
//! So a word no relay took is said again, and a change made while no pump
//! ran is said when one starts. A `(member, event)` ledger on disk keeps
//! each fact delivered once across restarts and one-shot processes; it
//! forgets a person who was removed, or whose node lost its copy, so the
//! history follows them back.
//!
//! **Inbound.** A fact sealed by a hosted member goes to the store's ingest
//! ladder, which admits a human's acts in the channels they reach and
//! refuses everything else by role — nothing here decides that. A `Join`
//! claims an invite (the store's single-use, expiring, constant-time
//! claim) and admits at once or, under `collab.join = ask`, records the
//! request for the owner's Inbox. `Leave` removes; `OpenDm` opens a direct
//! channel through the store's own door. A member's words carry the time
//! they were said, and the newest join or leave, and the newest profile,
//! stand: a relay hands every word back after a restart, in any order.
//!
//! The pump owns no relay: it sits on the pool the node opened
//! ([`bisa_collab::Relays`]) and shares it with the guest sessions.

use crate::config::NetConfig;
use crate::told::{self, PeopleFile, Told, RANK_JOIN, RANK_LEAVE};
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
use bisa_store::{ClaimFailure, ClaimOutcome, IngestOutcome, StoreEvent, Workspace};
use nostr::event::Event;
use nostr::key::{Keys, PublicKey};
use nostr::types::Timestamp;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Notify};

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
    outbox: Mutex<Outbox>,
    /// What each person said last and was told last (`net_people.json`).
    people: Mutex<PeopleFile>,
    /// One reconcile at a time.
    reconciling: Mutex<()>,
    /// Wakes the reconcile task: the people or the channels moved.
    moved: Notify,
    /// Something a person is owed found no relay: the rounds try again.
    owed: AtomicBool,
    /// When a standing member's node last asked to join again — it lost its
    /// copy — and was sent the history again.
    resent: Mutex<HashMap<String, Instant>>,
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

/// The wraps waiting for a relay, one for each `(member, fact)`: asked
/// again every round while no relay took it, a fact was queued once more
/// each time — memory without bound, then the fact published once for every
/// round that had passed.
#[derive(Default)]
struct Outbox {
    items: VecDeque<OutboxItem>,
    queued: HashSet<(String, String)>,
}

/// How often a standing member's node that asks to join again — it lost
/// what it held — is sent the history again.
const RESEND_EVERY: Duration = Duration::from_secs(600);

fn ledger_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("net_published.jsonl")
}

/// The ledger's line that forgets a person: everything above it that was
/// delivered to them. It has no space, so a build that knows only
/// `<member> <event>` lines passes over it.
fn forgetting_line(member: &str) -> String {
    format!("-{member}")
}

fn load_ledger(data_dir: &std::path::Path) -> HashSet<(String, String)> {
    let mut set: HashSet<(String, String)> = HashSet::new();
    if let Ok(body) = std::fs::read_to_string(ledger_path(data_dir)) {
        for line in body.lines().filter(|l| !l.trim().is_empty()) {
            if let Some((m, e)) = line.split_once(' ') {
                set.insert((m.to_string(), e.to_string()));
            } else if let Some(gone) = line.strip_prefix('-') {
                set.retain(|(m, _)| m != gone);
            }
        }
    }
    set
}

fn ledger_line(inner: &Inner, line: &str) {
    use std::io::Write;
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(ledger_path(&inner.data_dir))
    {
        Ok(mut f) => {
            if let Err(e) = writeln!(f, "{line}") {
                // LCOV_EXCL_START: the ledger file unwritable; a disk arm
                tracing::warn!("the ledger did not take a line: {e}");
                // LCOV_EXCL_STOP
            }
        }
        // LCOV_EXCL_START: the ledger file unopenable; a disk arm
        Err(e) => tracing::warn!("the ledger could not be opened: {e}"),
        // LCOV_EXCL_STOP
    }
}

/// Forget what was delivered to one person — they were removed, or their
/// node lost its copy: invited again, or asking again, the history is owed
/// to them anew. What still waits for a relay for them is dropped with it.
async fn ledger_forget(inner: &Arc<Inner>, member: &str) {
    {
        let mut ledger = inner.ledger.lock().await;
        let before = ledger.len();
        ledger.retain(|(m, _)| m != member);
        if ledger.len() != before {
            ledger_line(inner, &forgetting_line(member));
        }
    }
    let mut outbox = inner.outbox.lock().await;
    outbox.items.retain(|item| item.member != member);
    outbox.queued.retain(|(m, _)| m != member);
}

async fn ledger_record(inner: &Arc<Inner>, member: &str, inner_id: &str) {
    let mut ledger = inner.ledger.lock().await;
    if ledger.insert((member.to_string(), inner_id.to_string())) {
        ledger_line(inner, &format!("{member} {inner_id}"));
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
                // LCOV_EXCL_START: the direct transport's fetch; out by decision
                return st
                    .request_blob(&self.inner.ws, sha256, ATTACHMENT_FETCH_TIMEOUT)
                    .await;
                // LCOV_EXCL_STOP
            }
        }
        let _ = sha256;
        false
    }
}

/// The running pump.
pub struct Host {
    inner: Arc<Inner>,
    tasks: std::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl Host {
    /// Start the pump on `relays` for `ws`: the inbound reader, the outbound
    /// store-event pump, the reconcile task and the periodic round. Returns
    /// after a first pass ([`Self::catch_up_now`]), so a one-shot process
    /// sees history.
    ///
    /// The readers are attached before anything is fetched. A pool that is
    /// given its relays only after this returns — the node's, which must
    /// have every reader listening before a relay streams what it kept —
    /// finds them here, and its owner asks for the first pass again.
    pub async fn start(
        ws: Arc<Workspace>,
        relays: Arc<Relays>,
        cfg: NetConfig,
        data_dir: PathBuf,
    ) -> Result<Self, NetError> {
        let keys = ws.owner_keys().clone();
        let ledger_init = load_ledger(&data_dir);
        let people_init = told::load(&data_dir);
        #[cfg(feature = "iroh")]
        let iroh_state = if cfg.iroh {
            match crate::iroh_sync::IrohState::bind(&data_dir, &cfg).await {
                Ok(st) => Some(Arc::new(st)),
                // LCOV_EXCL_START: the direct transport's bind failing; out by decision
                Err(e) => {
                    tracing::warn!("iroh endpoint bind failed ({e}); continuing relay-only");
                    None
                } // LCOV_EXCL_STOP
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
            outbox: Mutex::new(Outbox::default()),
            people: Mutex::new(PeopleFile::default()),
            reconciling: Mutex::new(()),
            moved: Notify::new(),
            owed: AtomicBool::new(false),
            resent: Mutex::new(HashMap::new()),
            ledger: Mutex::new(ledger_init),
            #[cfg(feature = "iroh")]
            iroh: iroh_state,
        });

        // What each person was told: as kept — or, the first time this
        // build serves the workspace, taken from what stands.
        let people = match people_init {
            Some(kept) => kept,
            None => {
                let seeded = seed_people(&inner).await;
                told::save(&inner.data_dir, &seeded);
                seeded
            }
        };
        *inner.people.lock().await = people;

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
                        // LCOV_EXCL_START: the pool's bus lagging or closing under the reader; a race no test can time
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(
                                "host inbound lagged by {n}: the pool forgets what it handed on, and the next round reads everything again"
                            );
                            // Safe: a fact is taken once by its id, a word
                            // by its time. The next round's catch-up is whole.
                            inner.relays.forget_seen().await;
                            inner.last_catchup.store(0, Ordering::Relaxed);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        // LCOV_EXCL_STOP
                    }
                }
            })); // LCOV_EXCL_LINE: the task ends with the bus; a race no test can time
        }

        // Outbound: what the store said.
        {
            let inner = Arc::clone(&inner);
            let mut rx = inner.ws.subscribe_store_events();
            tasks.push(tokio::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok(ev) => on_store_event(&inner, ev).await,
                        // LCOV_EXCL_START: the store's bus lagging or closing under the reader; a race no test can time
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!("store event bus lagged by {n}; what each person is told is weighed again");
                            inner.moved.notify_one();
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        // LCOV_EXCL_STOP
                    }
                }
            })); // LCOV_EXCL_LINE: the task ends with the bus; a race no test can time
        }

        // What each person is told, weighed again whenever the people or
        // the channels moved — in a task of its own, so a relay that is slow
        // to take a word never holds the store's events back.
        {
            let inner = Arc::clone(&inner);
            tasks.push(tokio::spawn(async move {
                loop {
                    inner.moved.notified().await;
                    reconcile(&inner).await;
                }
            }));
        }

        // Periodic catch-up + backlog + flush — and what is still owed to a
        // person, which is the retry. The interval is read each round, so a
        // settings change takes effect without a restart.
        {
            let inner = Arc::clone(&inner);
            tasks.push(tokio::spawn(async move {
                loop {
                    let secs = inner.interval_secs.load(Ordering::Relaxed).max(1);
                    tokio::time::sleep(Duration::from_secs(secs)).await;
                    catch_up(&inner).await;
                    if inner.owed.load(Ordering::Relaxed) {
                        reconcile(&inner).await;
                    }
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

        let host = Self {
            inner,
            tasks: std::sync::Mutex::new(tasks),
        };
        host.catch_up_now().await;
        Ok(host)
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
        // A pump started on a pool that had no relay yet said where it can
        // be dialled to nobody.
        #[cfg(feature = "iroh")]
        if self.inner.iroh.is_some() {
            crate::iroh_sync::announce_addrs(&self.inner).await; // LCOV_EXCL_LINE: the direct transport's announcement; out by decision
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

    /// One immediate pass: a catch-up, what each person is owed, the
    /// backlog and a flush.
    pub async fn catch_up_now(&self) {
        catch_up(&self.inner).await;
        reconcile(&self.inner).await;
        publish_backlog(&self.inner).await;
        flush_outbox(&self.inner).await;
    }

    /// Stop the pumps. The relay pool is the node's and stays open.
    pub async fn stop(&self) {
        let tasks: Vec<_> = match self.tasks.lock() {
            Ok(mut tasks) => tasks.drain(..).collect(),
            Err(poisoned) => poisoned.into_inner().drain(..).collect(), // LCOV_EXCL_LINE: a task list nobody panics under; by construction
        };
        for t in tasks {
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

/// The directory as it stands; `None` when the members could not be read
/// — never an empty one, which told everybody that nobody is here.
fn directory(inner: &Inner) -> Option<Vec<Directory>> {
    let members = inner.ws.members().ok()?;
    Some(
        members
            .into_iter()
            .map(|m| Directory {
                pubkey: m.pubkey,
                role: m.role,
                label: m.label,
                photo: m.photo,
            })
            .collect(),
    )
}

/// A member's face as they sent it, kept when it is one: the bytes decoded
/// under the cap, hashed as claimed, a picture (`Face::decode`), and put in
/// the attachment store under its hash. `None` — said in the log — when it
/// is not, so the label still lands and the row keeps its old face.
fn keep_face(inner: &Inner, sender: &PublicKey, face: &Face) -> bool {
    match face.decode() {
        Ok(bytes) => match inner.ws.accept_attachment(&face.sha256, &bytes) {
            Ok(()) => true,
            // LCOV_EXCL_START: bytes `decode` proved against their hash, refused by the store; a disk arm
            Err(e) => {
                tracing::warn!("keeping {sender}'s face: {e}");
                false
            } // LCOV_EXCL_STOP
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
        // LCOV_EXCL_START: a guest asks for the hash a directory named; by construction
        return None;
        // LCOV_EXCL_STOP
    }
    let bytes = inner.ws.attachment_bytes(sha256).ok().flatten()?;
    bisa_core::is_face(&bytes).then(|| Face::from_bytes(&bytes))
}

/// What a joiner reads when their code was good and the host could not
/// write the admission: the code is theirs to try again.
const NOT_ADMITTED: &str =
    "the host could not admit you just now — the code is still good; try it again";

fn known_client(client: &str) -> Option<String> {
    [CLIENT_DESKTOP, CLIENT_MOBILE, CLIENT_CLI]
        .contains(&client)
        .then(|| client.to_string())
}

// -- outbound ------------------------------------------------------------------

/// Send one control message to one person over the pool. Answers whether
/// a relay took it — what [`reconcile`] waits on before it notes a word as
/// told.
pub(crate) async fn send_control(inner: &Arc<Inner>, member: PublicKey, control: &Control) -> bool {
    if inner.relays.count().await == 0 {
        return false;
    }
    match wrap_control(&inner.keys, member, control) {
        Ok(w) => match inner.relays.publish(&w).await {
            Ok(()) => true,
            Err(e) => {
                tracing::debug!("control {} to {member} failed: {e}", control.kind());
                false
            }
        },
        // LCOV_EXCL_START: wrapping to a member's key never fails; by construction
        Err(e) => {
            tracing::warn!("control wrap for {member} failed: {e}");
            false
        } // LCOV_EXCL_STOP
    }
}

/// A person's rooms, said to them now whatever they were last told — the
/// answer to an ask for a direct channel that already stood.
async fn send_channels(inner: &Arc<Inner>, person: &PrincipalId, role: MemberRole) {
    let Some(pk) = public_key(person) else {
        // LCOV_EXCL_START: a member's id is a key on the curve; by construction
        return;
        // LCOV_EXCL_STOP
    };
    // A list that could not be read is never sent as an empty one: told
    // they reach nothing, a person's copy dropped every room it held.
    let channels = match inner.ws.channels_reached_by(person, role) {
        Ok(channels) => channels,
        // LCOV_EXCL_START: the rooms unreadable under an ask; a disk arm
        Err(e) => {
            tracing::warn!("the channels {person} reaches could not be read; nothing is sent: {e}");
            return;
        } // LCOV_EXCL_STOP
    };
    send_control(inner, pk, &Control::ChannelsChanged { channels }).await;
}

/// Welcome one person — the host's card, their role, the directory and
/// their rooms — and note what a relay took. Answers whether one did.
async fn welcome(inner: &Arc<Inner>, person: &PrincipalId, role: MemberRole) -> bool {
    let Some(pk) = public_key(person) else {
        // LCOV_EXCL_START: a member's id is a key on the curve; by construction
        return false;
        // LCOV_EXCL_STOP
    };
    // Somebody just admitted holds no room yet, so a list that could not
    // be read costs them nothing here: they are welcomed, and hear their
    // rooms when the read answers — an unread list is never noted as told.
    let rooms = inner.ws.channels_reached_by(person, role).ok();
    let members = directory(inner);
    let control = Control::Welcome {
        workspace: host_card(inner).await,
        role,
        members: members.clone().unwrap_or_default(),
        channels: rooms.clone().unwrap_or_default(),
    };
    if !send_control(inner, pk, &control).await {
        return false;
    }
    let mut people = inner.people.lock().await;
    people.person(person.as_hex()).told = Some(Told {
        role,
        rooms: rooms.as_ref().map(told::digest).unwrap_or_default(),
        directory: members.as_ref().map(told::digest).unwrap_or_default(),
    });
    told::save(&inner.data_dir, &people);
    true
}

/// What stands, taken as told — the first time this build serves a
/// workspace an older pump served. Whoever the ledger shows a delivery to
/// was welcomed by it; they are welcomed by nothing again and sent no
/// history twice. Somebody it shows nothing for — admitted while no pump
/// ran — is owed their welcome. The refusals already made were said.
async fn seed_people(inner: &Arc<Inner>) -> PeopleFile {
    let mut file = PeopleFile::default();
    let served: HashSet<String> = inner
        .ledger
        .lock()
        .await
        .iter()
        .map(|(member, _)| member.clone())
        .collect();
    let members = directory(inner);
    for p in inner.ws.people().unwrap_or_default() {
        if !served.contains(p.pubkey.as_hex()) {
            continue;
        }
        let rooms = inner.ws.channels_reached_by(&p.pubkey, p.role).ok();
        file.person(p.pubkey.as_hex()).told = Some(Told {
            role: p.role,
            rooms: rooms.as_ref().map(told::digest).unwrap_or_default(),
            directory: members.as_ref().map(told::digest).unwrap_or_default(),
        });
    }
    for invite in inner.ws.invites().unwrap_or_default() {
        if matches!(invite.state, InviteState::Refused { .. }) {
            file.refusals.insert(invite.id.to_string());
        }
    }
    file
}

/// **What each person is told.** One rule, run whole: for everyone hosted,
/// what stands — a welcome if they were never welcomed, else their role,
/// their rooms, the directory — against what a relay last took for them,
/// and the difference sent; *removed* to whoever was welcomed and is here
/// no more; *refused* for a request the owner declined. A word is noted as
/// told only when a relay took it, so one that found none is owed, and the
/// rounds come back here until it is said.
///
/// A read that fails skips its part: nobody is told they reach nothing, or
/// that they were removed, because a file would not read.
async fn reconcile(inner: &Arc<Inner>) {
    let _one = inner.reconciling.lock().await;
    let people = match inner.ws.people() {
        Ok(people) => people,
        // LCOV_EXCL_START: the members unreadable under the pump; a disk arm
        Err(e) => {
            tracing::warn!("the people could not be read; nobody is told anything: {e}");
            inner.owed.store(true, Ordering::Relaxed);
            return;
        } // LCOV_EXCL_STOP
    };
    let members = directory(inner);
    let members_digest = members.as_ref().map(told::digest);
    let mut owed = false;
    let mut history: Vec<PrincipalId> = Vec::new();

    for p in &people {
        let Some(pk) = public_key(&p.pubkey) else {
            // LCOV_EXCL_START: a member's id is a key on the curve; by construction
            continue;
            // LCOV_EXCL_STOP
        };
        let hex = p.pubkey.as_hex();
        let standing = inner.people.lock().await.told(hex).cloned();
        let Some(mut said) = standing else {
            if welcome(inner, &p.pubkey, p.role).await {
                // A fact relayed to them before the welcome found a node
                // that keeps nothing until it is a member's: whatever the
                // ledger says of them, the history is theirs from here.
                ledger_forget(inner, hex).await;
                history.push(p.pubkey.clone());
            } else {
                owed = true;
            }
            continue;
        };
        let before = said.clone();
        if said.role != p.role {
            if send_control(inner, pk, &Control::RoleChanged { role: p.role }).await {
                said.role = p.role;
            } else {
                owed = true;
            }
        }
        match inner.ws.channels_reached_by(&p.pubkey, p.role) {
            Ok(channels) => {
                let rooms = told::digest(&channels);
                if rooms != said.rooms {
                    if send_control(inner, pk, &Control::ChannelsChanged { channels }).await {
                        said.rooms = rooms;
                        history.push(p.pubkey.clone());
                    } else {
                        owed = true;
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    "the channels {} reaches could not be read; nothing is sent: {e}",
                    p.pubkey
                );
                owed = true;
            }
        }
        if let (Some(members), Some(digest)) = (&members, &members_digest) {
            if *digest != said.directory {
                let word = Control::MembersChanged {
                    members: members.clone(),
                };
                if send_control(inner, pk, &word).await {
                    said.directory = digest.clone();
                } else {
                    owed = true;
                }
            }
        } // LCOV_EXCL_LINE: the members unreadable under the pump; a disk arm
        if said != before {
            let mut kept = inner.people.lock().await;
            kept.person(hex).told = Some(said);
            told::save(&inner.data_dir, &kept);
        }
    }

    // Whoever was welcomed and is here no more: what was delivered to them
    // is forgotten at once — invited again, the history is theirs anew —
    // and they are told until a relay takes it.
    let here: HashSet<&str> = people.iter().map(|p| p.pubkey.as_hex()).collect();
    let gone: Vec<String> = inner
        .people
        .lock()
        .await
        .people
        .iter()
        .filter(|(hex, p)| p.told.is_some() && !here.contains(hex.as_str()))
        .map(|(hex, _)| hex.clone())
        .collect();
    for hex in gone {
        ledger_forget(inner, &hex).await;
        let Ok(pk) = PublicKey::from_hex(&hex) else {
            // LCOV_EXCL_START: the record is keyed by members' keys; by construction
            continue;
            // LCOV_EXCL_STOP
        };
        if send_control(inner, pk, &Control::Removed { reason: None }).await {
            let mut kept = inner.people.lock().await;
            kept.person(&hex).told = None;
            told::save(&inner.data_dir, &kept);
        } else {
            owed = true;
        }
    }

    // A request the owner declined, said once a relay takes it — and not at
    // all to somebody who is a member now, or who asked again since.
    if let Ok(invites) = inner.ws.invites() {
        let asking: HashSet<&PrincipalId> = invites
            .iter()
            .filter_map(|i| match &i.state {
                InviteState::Requested { by, .. } => Some(by),
                _ => None,
            })
            .collect();
        for invite in &invites {
            let InviteState::Refused { by, .. } = &invite.state else {
                continue;
            };
            let id = invite.id.to_string();
            if inner.people.lock().await.refusals.contains(&id) {
                continue;
            }
            let moot = here.contains(by.as_hex()) || asking.contains(by);
            let said = moot
                || match public_key(by) {
                    Some(pk) => {
                        let word = Control::Refused {
                            reason: "the host declined".into(),
                        };
                        send_control(inner, pk, &word).await
                    }
                    None => true, // LCOV_EXCL_LINE: a claimer's id is a key on the curve; by construction
                };
            if said {
                let mut kept = inner.people.lock().await;
                kept.refusals.insert(id);
                told::save(&inner.data_dir, &kept);
            } else {
                owed = true;
            }
        }
    } // LCOV_EXCL_LINE: the invitations unreadable under the pump; a disk arm

    inner.owed.store(owed, Ordering::Relaxed);
    for person in history {
        backlog_for(inner, &person).await;
    }
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
            // LCOV_EXCL_START: the owner is no person of their own workspace; by construction
            continue;
            // LCOV_EXCL_STOP
        }
        let member_hex = pk.to_hex();
        let pair = (member_hex.clone(), inner_id.clone());
        if ledger.contains(&pair) || outbox.queued.contains(&pair) {
            continue;
        }
        match wrap_for_member(&inner.keys, *pk, event) {
            Ok(wrapped) => {
                #[cfg(feature = "iroh")]
                crate::iroh_sync::live_push(inner, Some(&member_hex), &wrapped).await;
                if has_relays {
                    outbox.queued.insert(pair);
                    outbox.items.push_back(OutboxItem {
                        wrap: wrapped,
                        member: member_hex,
                        inner_id: inner_id.clone(),
                    });
                }
                queued += 1;
            }
            // LCOV_EXCL_START: wrapping to a member's key never fails; by construction
            Err(e) => tracing::warn!("pairwise wrap for {member_hex} failed: {e}"),
            // LCOV_EXCL_STOP
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
        // LCOV_EXCL_START: the scan passes over what it cannot read and never fails; by construction
        return;
        // LCOV_EXCL_STOP
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
        // LCOV_EXCL_START: the scan passes over what it cannot read and never fails; by construction
        return;
        // LCOV_EXCL_STOP
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
        std::mem::take(&mut outbox.items)
    };
    let mut still = VecDeque::new();
    while let Some(item) = queued.pop_front() {
        let pair = (item.member.clone(), item.inner_id.clone());
        // Forgotten while it waited — its person was removed: not sent.
        if !inner.outbox.lock().await.queued.contains(&pair) {
            continue; // LCOV_EXCL_LINE: a person forgotten while a flush holds their wrap; a race no test can time
        }
        match inner.relays.publish(&item.wrap).await {
            Ok(()) => {
                inner.published.fetch_add(1, Ordering::Relaxed);
                ledger_record(inner, &item.member, &item.inner_id).await;
                inner.outbox.lock().await.queued.remove(&pair);
            }
            Err(e) => {
                tracing::debug!("publish failed ({e}); will retry");
                still.push_back(item);
            }
        }
    }
    if !still.is_empty() {
        let mut outbox = inner.outbox.lock().await;
        still.append(&mut outbox.items);
        outbox.items = still;
    }
}

/// One catch-up from where the last one that landed left off. The cursor
/// moves only when a relay was there to answer: moved after a fetch that
/// reached nobody, an outage longer than the window lost what was published
/// in it.
async fn catch_up(inner: &Arc<Inner>) {
    let last = inner.last_catchup.load(Ordering::Relaxed);
    let asked_at = Timestamp::now().as_secs();
    if inner.relays.catch_up((last != 0).then_some(last)).await {
        inner.last_catchup.store(asked_at, Ordering::Relaxed);
    }
}

async fn on_store_event(inner: &Arc<Inner>, ev: StoreEvent) {
    match ev {
        // A peer's drawing landed here: the engine tells the canvas; the host
        // relays facts by its own pump, never by echo.
        StoreEvent::RemoteDrawingArrived { .. } => {} // LCOV_EXCL_LINE: the pump does nothing with a drawing; out by decision
        StoreEvent::ConversationAppended { event, .. }
        | StoreEvent::RemoteMessageArrived { event, .. }
        // A person's reaction or retraction, relayed as it lands.
        | StoreEvent::RemoteFactArrived { event, .. } => relay_fact(inner, &event).await,
        // A channel was made, changed, rostered or deleted — here or by a
        // record that landed: everyone's reach may have moved.
        StoreEvent::ConversationSnapshot { kind, .. } if kind == KIND_CHANNEL => {
            inner.moved.notify_one();
        }
        StoreEvent::RemoteChannelArrived { .. } => inner.moved.notify_one(),
        StoreEvent::DefinitionRemoved { kind, .. } if kind == KIND_CHANNEL => {
            inner.moved.notify_one();
        }
        // Somebody joined, changed role, left, or said who they are: what
        // each person is told is weighed again — the one it concerns, and
        // the directory for everyone else.
        StoreEvent::PeopleChanged { pubkey, change } => {
            #[cfg(feature = "iroh")]
            if matches!(change, bisa_store::PeopleChange::Left) {
                if let Some(st) = &inner.iroh {
                    st.forget_peer(pubkey.as_hex()).await; // LCOV_EXCL_LINE: the direct transport's forgetting; out by decision
                }
            }
            #[cfg(not(feature = "iroh"))]
            let _ = (pubkey, change);
            inner.moved.notify_one();
        }
        // A request refused is said to its asker by the same rule.
        StoreEvent::InviteChanged { .. } => inner.moved.notify_one(),
        // Goals, journals, snapshots of anything but a channel, and read
        // markers stay on this node.
        StoreEvent::JournalAppended { .. }
        | StoreEvent::SnapshotWritten { .. }
        | StoreEvent::ConversationSnapshot { .. }
        | StoreEvent::ReadMarkerSet { .. }
        | StoreEvent::DefinitionRemoved { .. } => {}
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
                _ => None, // LCOV_EXCL_LINE: the direct transport's wire; out by decision
            };
            on_incoming(inner, &incoming).await;
            id
        }
        // LCOV_EXCL_START: the direct transport's wire; out by decision
        Err(e) => {
            tracing::debug!("cannot unwrap gift {}: {e}", wrap.id);
            None
        } // LCOV_EXCL_STOP
    }
}

/// Apply one thing the pool heard for the owner's key.
pub(crate) async fn on_incoming(inner: &Arc<Inner>, incoming: &Incoming) {
    match incoming {
        Incoming::Gep { sender, event } => {
            if hosted_role(inner, &sender.to_hex()).is_none() {
                // LCOV_EXCL_START: a fact from a key the host does not host; out by decision
                tracing::debug!("a fact sealed by {sender}, who is not hosted here — ignored");
                return;
                // LCOV_EXCL_STOP
            }
            // A person's seal carries that person's own fact. Only the host
            // relays what somebody else wrote; taken under a member's seal,
            // another author's fact was judged by its author alone — the
            // owner's own words, sealed back by a workspace that hosts them,
            // among them.
            if event.pubkey != *sender {
                tracing::debug!(
                    "a fact by {} under {sender}'s seal — a person relays nobody's fact but their own; ignored",
                    event.pubkey
                );
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
                // LCOV_EXCL_START: a hosted member writes no journal; by construction
                Ok(Ok(IngestOutcome::AppliedJournal { home })) => {
                    inner.ingested.fetch_add(1, Ordering::Relaxed);
                    tracing::info!("a person's decision landed on {home}");
                }
                // LCOV_EXCL_STOP
                // LCOV_EXCL_START: a hosted admin's channel definition lands as a conversation; by construction
                Ok(Ok(IngestOutcome::AppliedSnapshot { kind, .. })) => {
                    inner.ingested.fetch_add(1, Ordering::Relaxed);
                    tracing::info!("a person's {kind} snapshot landed");
                }
                // LCOV_EXCL_STOP
                Ok(Ok(IngestOutcome::Duplicate)) => {} // LCOV_EXCL_LINE: the pool hands each wrap on once; by construction
                Ok(Ok(IngestOutcome::Rejected { reason })) if reason == "stale" => {}
                Ok(Ok(IngestOutcome::Rejected { reason })) => {
                    tracing::warn!("ingest refused a fact from {sender}: {reason}");
                }
                // LCOV_EXCL_START: the index unwritable under the ingest, a disk arm; the ingest task unwinding, by construction
                Ok(Err(e)) => {
                    tracing::warn!(event = %event_id, "ingest of a fact from {sender} failed: {e}")
                }
                Err(e) => {
                    tracing::warn!(event = %event_id, "the ingest task for a fact from {sender} did not finish: {e}")
                } // LCOV_EXCL_STOP
            }
        }
        Incoming::Control {
            sender,
            control,
            at,
        } => on_control(inner, *sender, control, *at).await,
        Incoming::Other => {} // LCOV_EXCL_LINE: an envelope the wire did not make; out by decision
    }
}

/// Note a member's join or leave as the newest heard from them.
async fn heard(inner: &Arc<Inner>, hex: &str, at: u64, rank: u8) {
    let mut people = inner.people.lock().await;
    people.person(hex).said = (at, rank);
    told::save(&inner.data_dir, &people);
}

/// One of a member's words, said at `at` by their clock. The newest join
/// or leave, and the newest profile, stand ([`crate::told`]): an older one
/// — handed back by a relay after a restart — changes nothing.
async fn on_control(inner: &Arc<Inner>, sender: PublicKey, control: &Control, at: u64) {
    if !control.is_members_word() {
        // LCOV_EXCL_START: a host's word arriving from a member; out by decision
        tracing::debug!("{sender} sent a host's word ({}) — ignored", control.kind());
        return;
        // LCOV_EXCL_STOP
    }
    let Ok(person) = PrincipalId::new(sender.to_hex()) else {
        // LCOV_EXCL_START: a key's hex is a principal; by construction
        return;
        // LCOV_EXCL_STOP
    };
    match control {
        Control::Join {
            secret,
            label,
            client,
        } => {
            let hex = sender.to_hex();
            if !inner.people.lock().await.takes(&hex, at, RANK_JOIN) {
                tracing::debug!("an old join from {sender}, handed back — ignored");
                return;
            }
            // Settings that will not read never admit: a host that asked to
            // admit by hand, read as its default, admitted at once.
            let waits = match inner.ws.settings(None) {
                Ok(settings) => CollabSettings::from_resolved(&settings).join == JoinPolicy::Ask,
                // LCOV_EXCL_START: the settings unreadable under a join; a disk arm
                Err(e) => {
                    tracing::warn!(
                        "the settings could not be read; {sender}'s claim waits on the owner: {e}"
                    );
                    true
                } // LCOV_EXCL_STOP
            };
            let label = clean_label(label.clone());
            // The store's one admission, whole under the people's writer.
            match inner.ws.claim_and_admit(
                &hash_secret(secret),
                &person,
                label,
                known_client(client),
                waits,
            ) {
                Ok(ClaimOutcome::Admitted { member, .. }) => {
                    tracing::info!("admitted {sender} as {}", member.role.as_str());
                    // Admitted, they are owed a welcome whatever the record
                    // says of an earlier membership — one whose *removed*
                    // no relay had taken yet read as somebody still told.
                    inner.people.lock().await.person(&hex).told = None;
                    heard(inner, &hex, at, RANK_JOIN).await;
                    // The welcome rides the store's `PeopleChanged`.
                }
                Ok(ClaimOutcome::Waiting { .. }) => {
                    tracing::info!("{sender} asked to join; the owner admits by hand");
                    heard(inner, &hex, at, RANK_JOIN).await;
                    send_control(inner, sender, &Control::Waiting).await;
                }
                // A member's own node claims only when it lost what it held
                // of this workspace: they are told where they stand again,
                // and the code is kept for whoever it was made for. The
                // owner's own key is no person of its workspace: nothing.
                Ok(ClaimOutcome::AlreadyMember { member }) => {
                    if !member.role.is_hosted() {
                        return;
                    }
                    tracing::info!("{sender} is a member already; welcomed again");
                    heard(inner, &hex, at, RANK_JOIN).await;
                    // Their node holds nothing of this workspace: what the
                    // ledger says was delivered to them is not there. The
                    // history is sent again — not on every ask.
                    let again = {
                        let mut resent = inner.resent.lock().await;
                        let due = resent
                            .get(&hex)
                            .is_none_or(|last| last.elapsed() >= RESEND_EVERY);
                        if due {
                            resent.insert(hex.clone(), Instant::now());
                        }
                        due
                    };
                    if again {
                        ledger_forget(inner, &hex).await;
                    }
                    if welcome(inner, &person, member.role).await {
                        if again {
                            backlog_for(inner, &person).await;
                        }
                    } else {
                        // Their standing is owed to them: the rounds say it.
                        inner.people.lock().await.person(&hex).told = None;
                        inner.owed.store(true, Ordering::Relaxed);
                    }
                }
                Err(ClaimFailure::Refused(refusal)) => {
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
                // The host's own trouble is the host's to read: what it
                // names — a file on this disk — is nothing a joiner is sent.
                Err(ClaimFailure::NotAdmitted(e)) => {
                    tracing::warn!("{sender} held a good code and could not be admitted: {e}");
                    send_control(
                        inner,
                        sender,
                        &Control::Refused {
                            reason: NOT_ADMITTED.into(),
                        },
                    )
                    .await;
                }
            }
        }
        Control::Leave => {
            let hex = sender.to_hex();
            if hosted_role(inner, &hex).is_none() {
                // LCOV_EXCL_START: a word from a key the host does not host; out by decision
                return;
                // LCOV_EXCL_STOP
            }
            // An old leave, handed back, removes nobody invited again. With
            // nothing heard from them yet — admitted by key, or before this
            // was kept — the day they were admitted is the floor.
            let newest = {
                let people = inner.people.lock().await;
                if people.heard_from(&hex) {
                    people.takes(&hex, at, RANK_LEAVE)
                } else {
                    let admitted = inner
                        .ws
                        .people()
                        .unwrap_or_default()
                        .into_iter()
                        .find(|p| p.pubkey == person)
                        .map(|p| p.added_at)
                        .unwrap_or(0);
                    at >= admitted
                }
            };
            if !newest {
                tracing::debug!("an old leave from {sender}, handed back — ignored");
                return;
            }
            heard(inner, &hex, at, RANK_LEAVE).await;
            // The store's one removal: the row first, then what is held of
            // theirs and their places on rosters.
            match inner.ws.remove_person(&person) {
                Ok(()) => tracing::info!("{sender} left"),
                // LCOV_EXCL_START: the people's files unwritable under a leave; a disk arm
                Err(e) => tracing::warn!("removing {sender} who left: {e}"),
                // LCOV_EXCL_STOP
            }
        }
        Control::OpenDm { participants } => {
            let Some((_, role)) = hosted_role(inner, &sender.to_hex()) else {
                // LCOV_EXCL_START: a word from a key the host does not host; out by decision
                return;
                // LCOV_EXCL_STOP
            };
            if !role.may(Permission::OpenDms) {
                // LCOV_EXCL_START: every hosted role may open direct messages; by construction
                tracing::info!(
                    "{sender} may not open direct messages as a {}",
                    role.as_str()
                );
                return;
                // LCOV_EXCL_STOP
            }
            let mut all = participants.clone();
            all.push(person.clone());
            match inner.ws.open_dm(&all) {
                // The channel's snapshot, when new, reaches everyone through
                // the store's bus; an existing one is said to the asker here.
                Ok(_) => send_channels(inner, &person, role).await,
                // LCOV_EXCL_START: a participant the directory named and the store refuses; a race no test can time
                Err(e) => tracing::info!("{sender} asked for a direct message: {e}"),
                // LCOV_EXCL_STOP
            }
        }
        Control::Profile { label, photo, face } => {
            // A profile is a hosted member's word about themselves; a stranger's
            // is ignored before a byte of it is decoded.
            if hosted_role(inner, &sender.to_hex()).is_none() {
                tracing::debug!("profile from {sender}, not hosted — ignored");
                return;
            }
            // An old profile, handed back, puts no name back and clears no face.
            {
                let hex = sender.to_hex();
                let mut people = inner.people.lock().await;
                if !people.takes_profile(&hex, at) {
                    tracing::debug!("an old profile from {sender}, handed back — ignored");
                    return;
                }
                people.person(&hex).profile = at;
                told::save(&inner.data_dir, &people);
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
                // LCOV_EXCL_START: the members file unwritable; a disk arm
                Err(e) => tracing::warn!("profile from {sender}: {e}"),
                // LCOV_EXCL_STOP
            }
        }
        Control::WantFace { sha256 } => {
            if hosted_role(inner, &sender.to_hex()).is_none() {
                // LCOV_EXCL_START: a word from a key the host does not host; out by decision
                return;
                // LCOV_EXCL_STOP
            }
            if let Some(face) = face_held(inner, sha256) {
                send_control(inner, sender, &Control::Face { face }).await;
            } // LCOV_EXCL_LINE: a face the directory names is one the host holds; by construction
        }
        // LCOV_EXCL_START: the direct transport's word; out by decision
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
        // LCOV_EXCL_STOP
        _ => {} // LCOV_EXCL_LINE: every members' word has an arm; by construction
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

    /// A profile from a sender nobody hosts is ignored before a byte of it
    /// is decoded: no member row appears and nothing is kept — the control
    /// a member removed a moment ago may still send.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_profile_from_a_sender_nobody_hosts_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Arc::new(
            Workspace::open_with_keystore(
                dir.path(),
                Box::new(bisa_store::MemoryKeyStore::default()),
            )
            .unwrap(),
        );
        let relays = Relays::start(ws.owner_keys().clone(), &[]).await;
        let host = Host::start(
            Arc::clone(&ws),
            relays,
            NetConfig {
                relays: vec![],
                sync_interval_secs: 1,
                publish_relay_list: false,
                iroh: false,
                ..NetConfig::default()
            },
            dir.path().to_path_buf(),
        )
        .await
        .unwrap();
        let stranger = Keys::generate();
        on_control(
            &host.inner,
            stranger.public_key(),
            &Control::Profile {
                label: Some("Eve".into()),
                photo: None,
                face: None,
            },
            1,
        )
        .await;
        let who = bisa_core::PrincipalId::new(stranger.public_key().to_hex()).unwrap();
        assert!(
            ws.member(&who).unwrap().is_none(),
            "nobody was admitted by a profile"
        );
        assert!(ws.people().unwrap().is_empty());
        host.stop().await;
    }

    /// The owner's own key claiming one of its own codes — a link pasted
    /// into the node that made it — claims nothing: the code stands, and the
    /// owner is no person of their own workspace.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_owners_own_claim_of_their_own_code_claims_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Arc::new(
            Workspace::open_with_keystore(
                dir.path(),
                Box::new(bisa_store::MemoryKeyStore::default()),
            )
            .unwrap(),
        );
        let relays = Relays::start(ws.owner_keys().clone(), &[]).await;
        let host = Host::start(
            Arc::clone(&ws),
            relays,
            NetConfig {
                relays: vec![],
                sync_interval_secs: 1,
                publish_relay_list: false,
                iroh: false,
                ..NetConfig::default()
            },
            dir.path().to_path_buf(),
        )
        .await
        .unwrap();
        let secret = "ab".repeat(16);
        ws.create_invite(MemberRole::Member, vec![], None, 3600, hash_secret(&secret))
            .unwrap();
        on_control(
            &host.inner,
            ws.owner_keys().public_key(),
            &Control::Join {
                secret,
                label: None,
                client: "desktop".into(),
            },
            1,
        )
        .await;
        assert_eq!(ws.invites().unwrap()[0].state, InviteState::Pending);
        assert!(ws.people().unwrap().is_empty());
        host.stop().await;
    }

    /// A person's seal carries that person's own fact, and nobody else's:
    /// the owner's own words, handed back under a member's seal, land
    /// nowhere — a fact is relayed by its workspace's host alone.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_person_relays_nobodys_fact_but_their_own() {
        use nostr::event::{EventBuilder, FinalizeEvent, Kind, Tag};
        let dir = tempfile::tempdir().unwrap();
        let ws = Arc::new(
            Workspace::open_with_keystore(
                dir.path(),
                Box::new(bisa_store::MemoryKeyStore::default()),
            )
            .unwrap(),
        );
        let relays = Relays::start(ws.owner_keys().clone(), &[]).await;
        let host = Host::start(
            Arc::clone(&ws),
            relays,
            NetConfig {
                relays: vec![],
                sync_interval_secs: 3600,
                publish_relay_list: false,
                iroh: false,
                ..NetConfig::default()
            },
            dir.path().to_path_buf(),
        )
        .await
        .unwrap();
        let bob = Keys::generate();
        ws.add_member(
            PrincipalId::new(bob.public_key().to_hex()).unwrap(),
            MemberRole::Member,
            bisa_store::Admission::default(),
        )
        .unwrap();
        let said = |keys: &Keys, text: &str| {
            EventBuilder::new(
                Kind::from(bisa_core::kind::KIND_MESSAGE),
                serde_json::to_string(&bisa_core::MessageBody::post(text)).unwrap(),
            )
            .tags([Tag::parse([
                "a",
                &format!(
                    "{KIND_CHANNEL}:{}:general",
                    ws.owner_keys().public_key().to_hex()
                ),
            ])
            .unwrap()])
            .finalize(keys)
            .unwrap()
        };
        let owners = said(ws.owner_keys(), "the owner's own words, handed back");
        let bobs = said(&bob, "bob's own");
        for event in [owners, bobs] {
            on_incoming(
                &host.inner,
                &Incoming::Gep {
                    sender: bob.public_key(),
                    event,
                },
            )
            .await;
        }
        let texts: Vec<String> = ws
            .messages("general", None, 10)
            .unwrap()
            .into_iter()
            .map(|m| m.content)
            .collect();
        assert_eq!(texts, ["bob's own"]);
        host.stop().await;
    }

    // -- a member's words in order; what waits; what the ledger forgets -----

    fn open_ws(dir: &std::path::Path) -> Arc<Workspace> {
        Arc::new(
            Workspace::open_with_keystore(dir, Box::new(bisa_store::MemoryKeyStore::default()))
                .unwrap(),
        )
    }

    /// A pump on a pool of `urls`, its rounds an hour apart.
    async fn pump(ws: &Arc<Workspace>, dir: &std::path::Path, urls: &[String]) -> Host {
        let relays = Relays::start(ws.owner_keys().clone(), urls).await;
        Host::start(
            Arc::clone(ws),
            relays,
            NetConfig {
                relays: urls.to_vec(),
                sync_interval_secs: 3600,
                publish_relay_list: false,
                iroh: false,
                ..NetConfig::default()
            },
            dir.to_path_buf(),
        )
        .await
        .unwrap()
    }

    fn pid(keys: &Keys) -> PrincipalId {
        PrincipalId::new(keys.public_key().to_hex()).unwrap()
    }

    fn a_code(ws: &Workspace) -> String {
        let secret = bisa_collab::mint_secret();
        ws.create_invite(MemberRole::Member, vec![], None, 3600, hash_secret(&secret))
            .unwrap();
        secret
    }

    async fn says(host: &Host, who: &Keys, control: Control, at: u64) {
        on_incoming(
            &host.inner,
            &Incoming::Control {
                sender: who.public_key(),
                control,
                at,
            },
        )
        .await;
    }

    fn join(secret: &str) -> Control {
        Control::Join {
            secret: secret.to_string(),
            label: None,
            client: "cli".into(),
        }
    }

    /// A relay hands a member's words back after a restart, every one it
    /// kept: the leave of somebody who has since been invited again removes
    /// nobody, and their first join — its code long spent — is answered by
    /// nothing.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_member_who_left_and_came_back_is_not_removed_by_the_old_leave() {
        let dir = tempfile::tempdir().unwrap();
        let ws = open_ws(dir.path());
        let host = pump(&ws, dir.path(), &[]).await;
        let bob = Keys::generate();
        let first = a_code(&ws);
        says(&host, &bob, join(&first), 100).await;
        assert!(ws.member(&pid(&bob)).unwrap().is_some(), "admitted");
        says(&host, &bob, Control::Leave, 200).await;
        assert!(ws.member(&pid(&bob)).unwrap().is_none(), "left");
        let second = a_code(&ws);
        says(&host, &bob, join(&second), 300).await;
        assert!(ws.member(&pid(&bob)).unwrap().is_some(), "invited again");

        // Handed back, in the order a relay happens to keep them.
        for (old, at) in [
            (Control::Leave, 200),
            (join(&first), 100),
            (join(&second), 300),
        ] {
            says(&host, &bob, old, at).await;
            assert!(
                ws.member(&pid(&bob)).unwrap().is_some(),
                "an old word removed somebody who came back"
            );
        }
        // The marks are the pump's record, not the process's.
        host.stop().await;
        let again = pump(&ws, dir.path(), &[]).await;
        says(&again, &bob, Control::Leave, 200).await;
        assert!(ws.member(&pid(&bob)).unwrap().is_some());
        // The leave of the join's own second is the later word; a newer one
        // is a leave.
        says(&again, &bob, Control::Leave, 300).await;
        assert!(ws.member(&pid(&bob)).unwrap().is_none());
        again.stop().await;
    }

    /// Somebody the pump has heard nothing from — admitted by key — has the
    /// day they were admitted as the floor: a leave said before it is of an
    /// earlier membership.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_leave_older_than_the_admission_is_of_an_earlier_membership() {
        let dir = tempfile::tempdir().unwrap();
        let ws = open_ws(dir.path());
        let host = pump(&ws, dir.path(), &[]).await;
        let bob = Keys::generate();
        ws.add_member(pid(&bob), MemberRole::Member, Default::default())
            .unwrap();
        let admitted = ws.member(&pid(&bob)).unwrap().unwrap().added_at;
        says(&host, &bob, Control::Leave, admitted - 60).await;
        assert!(ws.member(&pid(&bob)).unwrap().is_some());
        says(&host, &bob, Control::Leave, admitted + 1).await;
        assert!(ws.member(&pid(&bob)).unwrap().is_none());
        host.stop().await;
    }

    /// An old profile, handed back, puts no name back.
    #[tokio::test(flavor = "multi_thread")]
    async fn an_old_profile_never_puts_a_name_back() {
        let dir = tempfile::tempdir().unwrap();
        let ws = open_ws(dir.path());
        let host = pump(&ws, dir.path(), &[]).await;
        let bob = Keys::generate();
        ws.add_member(pid(&bob), MemberRole::Member, Default::default())
            .unwrap();
        let named = |label: &str| Control::Profile {
            label: Some(label.into()),
            photo: None,
            face: None,
        };
        let label = || ws.member(&pid(&bob)).unwrap().unwrap().label;
        says(&host, &bob, named("Bob"), 10).await;
        says(&host, &bob, named("Robert"), 20).await;
        assert_eq!(label().as_deref(), Some("Robert"));
        says(&host, &bob, named("Bob"), 10).await;
        assert_eq!(label().as_deref(), Some("Robert"), "the old name came back");
        says(&host, &bob, named("Rob"), 21).await;
        assert_eq!(label().as_deref(), Some("Rob"));
        host.stop().await;
    }

    /// While no relay takes it, a fact waits once for each person — asked
    /// again every round, it was queued once more each time.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_outbox_holds_one_of_each_while_the_relay_is_down() {
        let dir = tempfile::tempdir().unwrap();
        let ws = open_ws(dir.path());
        let host = pump(&ws, dir.path(), &["ws://127.0.0.1:1".to_string()]).await;
        let (bob, carol) = (Keys::generate(), Keys::generate());
        for who in [&bob, &carol] {
            ws.add_member(pid(who), MemberRole::Member, Default::default())
                .unwrap();
        }
        ws.post_message(
            "general",
            bisa_core::MessageBody::post("while the relay is down"),
            None,
            &[],
            &[],
            None,
            bisa_store::PostOrigin::Asked,
        )
        .unwrap();
        for _ in 0..4 {
            host.catch_up_now().await;
        }
        {
            let outbox = host.inner.outbox.lock().await;
            assert_eq!(outbox.items.len(), 2, "one for each person");
            assert_eq!(outbox.queued.len(), 2);
        }
        assert!(
            host.inner.owed.load(Ordering::Relaxed),
            "their welcomes are owed"
        );
        // Removed while it waited: nothing of theirs is sent.
        ws.remove_person(&pid(&carol)).unwrap();
        ledger_forget(&host.inner, &carol.public_key().to_hex()).await;
        let outbox = host.inner.outbox.lock().await;
        assert_eq!(outbox.items.len(), 1);
        assert_eq!(outbox.items[0].member, bob.public_key().to_hex());
        drop(outbox);
        host.stop().await;
    }

    /// The ledger forgets a person by a line of its own, which a build that
    /// knows only `<member> <event>` passes over; what is delivered after
    /// it counts again.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_ledger_forgets_a_person_and_keeps_the_others() {
        let dir = tempfile::tempdir().unwrap();
        let ws = open_ws(dir.path());
        let host = pump(&ws, dir.path(), &[]).await;
        ledger_record(&host.inner, "aa", "e1").await;
        ledger_record(&host.inner, "aa", "e2").await;
        ledger_record(&host.inner, "bb", "e1").await;
        ledger_forget(&host.inner, "aa").await;
        ledger_forget(&host.inner, "aa").await;
        ledger_forget(&host.inner, "cc").await;
        ledger_record(&host.inner, "aa", "e3").await;
        let lines = std::fs::read_to_string(ledger_path(dir.path())).unwrap();
        assert_eq!(
            lines.lines().collect::<Vec<_>>(),
            ["aa e1", "aa e2", "bb e1", "-aa", "aa e3"],
            "one forgetting line, and only for somebody the ledger knew"
        );
        assert!(lines.lines().all(|l| l != "-aa" || !l.contains(' ')));
        ledger_line(&host.inner, "neither");
        let read = load_ledger(dir.path());
        let expected: HashSet<(String, String)> = [("bb", "e1"), ("aa", "e3")]
            .into_iter()
            .map(|(m, e)| (m.to_string(), e.to_string()))
            .collect();
        assert_eq!(read, expected);
        host.stop().await;
    }

    /// The first time this build serves a workspace an older pump served:
    /// whoever the ledger shows a delivery to was welcomed by it and is
    /// told nothing again; somebody admitted while no pump ran is owed
    /// their welcome; a refusal already made is not said twice.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_pump_that_served_before_takes_what_stands_as_told() {
        let dir = tempfile::tempdir().unwrap();
        let ws = open_ws(dir.path());
        let (served, unserved) = (Keys::generate(), Keys::generate());
        for who in [&served, &unserved] {
            ws.add_member(pid(who), MemberRole::Member, Default::default())
                .unwrap();
        }
        std::fs::write(
            ledger_path(dir.path()),
            format!("{} {}\n", served.public_key().to_hex(), "0".repeat(64)),
        )
        .unwrap();
        assert!(!told::path(dir.path()).exists());
        let host = pump(&ws, dir.path(), &[]).await;
        {
            let people = host.inner.people.lock().await;
            let was = people
                .told(&served.public_key().to_hex())
                .expect("taken as told");
            assert_eq!(was.role, MemberRole::Member);
            assert!(!was.rooms.is_empty() && !was.directory.is_empty());
            assert!(people.told(&unserved.public_key().to_hex()).is_none());
        }
        assert!(told::path(dir.path()).exists(), "kept, so it is taken once");
        assert!(
            host.inner.owed.load(Ordering::Relaxed),
            "one welcome is owed"
        );
        // What stands is what was told: a role that moves is owed to them.
        ws.set_role(&pid(&served), MemberRole::Admin).unwrap();
        host.catch_up_now().await;
        let people = host.inner.people.lock().await;
        assert_eq!(
            people.told(&served.public_key().to_hex()).unwrap().role,
            MemberRole::Member,
            "not noted as told while no relay took it"
        );
        drop(people);
        host.stop().await;
    }

    /// A welcome no relay took is owed, and the rounds say it until one
    /// does — with nothing else happening to prompt it.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_welcome_no_relay_took_is_said_again() {
        use nostr_relay_builder::builder::RelayBuilder;
        use nostr_relay_builder::local::LocalRelay;
        let every_second = |url: &str| NetConfig {
            relays: vec![url.to_string()],
            sync_interval_secs: 1,
            publish_relay_list: false,
            iroh: false,
            ..NetConfig::default()
        };
        let dead = "ws://127.0.0.1:1";
        let dir = tempfile::tempdir().unwrap();
        let ws = open_ws(dir.path());
        let relays = Relays::start(ws.owner_keys().clone(), &[dead.to_string()]).await;
        let host = Host::start(
            Arc::clone(&ws),
            relays,
            every_second(dead),
            dir.path().to_path_buf(),
        )
        .await
        .unwrap();
        let bob = Keys::generate();
        let hex = bob.public_key().to_hex();
        ws.add_member(pid(&bob), MemberRole::Member, Default::default())
            .unwrap();
        // The admission wakes the pump; its welcome finds no relay.
        let mut owed = false;
        for _ in 0..300 {
            owed = host.inner.owed.load(Ordering::Relaxed);
            if owed {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(owed, "the welcome is owed");
        assert!(host.inner.people.lock().await.told(&hex).is_none());

        let relay = LocalRelay::new(RelayBuilder::default().addr("127.0.0.1".parse().unwrap()));
        relay.run().await.unwrap();
        host.apply(&every_second(&relay.url().await.to_string()))
            .await;
        let mut told = false;
        for _ in 0..300 {
            told = host.inner.people.lock().await.told(&hex).is_some();
            if told {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(told, "the welcome was said once a relay took it");
        host.stop().await;
    }

    fn a_standing(role: MemberRole) -> Told {
        Told {
            role,
            rooms: "rooms".into(),
            directory: "directory".into(),
        }
    }

    /// With no relay to take it, a removal and a refusal are owed and stay
    /// so; a refusal is moot once its asker asks again; and a pump that
    /// served before takes the refusals already made as said.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_removal_and_a_refusal_are_owed_until_a_relay_takes_them() {
        let dir = tempfile::tempdir().unwrap();
        let ws = open_ws(dir.path());
        ws.set_setting(
            bisa_core::settings::Scope::Workspace,
            None,
            "collab.join",
            serde_json::json!("ask"),
        )
        .unwrap();
        let host = pump(&ws, dir.path(), &[]).await;
        let (bob, carol) = (Keys::generate(), Keys::generate());

        // Somebody welcomed once, removed since.
        ws.add_member(pid(&bob), MemberRole::Member, Default::default())
            .unwrap();
        let bob_hex = bob.public_key().to_hex();
        host.inner.people.lock().await.person(&bob_hex).told = Some(a_standing(MemberRole::Member));
        ledger_record(&host.inner, &bob_hex, "e1").await;
        ws.remove_person(&pid(&bob)).unwrap();
        host.catch_up_now().await;
        assert!(host.inner.owed.load(Ordering::Relaxed));
        assert!(
            host.inner.people.lock().await.told(&bob_hex).is_some(),
            "told to nobody yet"
        );
        assert!(load_ledger(dir.path()).is_empty(), "forgotten at once");
        // Invited again before the removal was ever said: admitted, they are
        // owed a welcome, not the difference from an earlier membership.
        ws.set_setting(
            bisa_core::settings::Scope::Workspace,
            None,
            "collab.join",
            serde_json::json!("admit"),
        )
        .unwrap();
        says(&host, &bob, join(&a_code(&ws)), 50).await;
        assert!(ws.member(&pid(&bob)).unwrap().is_some());
        assert!(host.inner.people.lock().await.told(&bob_hex).is_none());
        ws.set_setting(
            bisa_core::settings::Scope::Workspace,
            None,
            "collab.join",
            serde_json::json!("ask"),
        )
        .unwrap();

        // A request the owner declines.
        says(&host, &carol, join(&a_code(&ws)), 10).await;
        let asked = ws
            .invites()
            .unwrap()
            .into_iter()
            .find(|i| matches!(i.state, InviteState::Requested { .. }))
            .expect("carol waits");
        ws.refuse_request(asked.id).unwrap();
        host.catch_up_now().await;
        assert!(host.inner.people.lock().await.refusals.is_empty(), "owed");

        // A pump that finds no record takes the refusal as said.
        host.stop().await;
        std::fs::remove_file(told::path(dir.path())).unwrap();
        let host = pump(&ws, dir.path(), &[]).await;
        assert!(host
            .inner
            .people
            .lock()
            .await
            .refusals
            .contains(&asked.id.to_string()));
        host.catch_up_now().await;

        // Another, moot before it is said: its asker asked again.
        says(&host, &carol, join(&a_code(&ws)), 20).await;
        let second = ws
            .invites()
            .unwrap()
            .into_iter()
            .find(|i| matches!(i.state, InviteState::Requested { .. }))
            .expect("carol waits again");
        ws.refuse_request(second.id).unwrap();
        says(&host, &carol, join(&a_code(&ws)), 30).await;
        host.catch_up_now().await;
        assert!(host
            .inner
            .people
            .lock()
            .await
            .refusals
            .contains(&second.id.to_string()));
        host.stop().await;
    }

    /// A member's node that asks to join again lost what it held: it is
    /// welcomed as it stands, and sent the history again — once, not on
    /// every ask. With no relay, its standing is owed.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_members_node_that_asks_again_is_sent_the_history_once() {
        use nostr_relay_builder::builder::RelayBuilder;
        use nostr_relay_builder::local::LocalRelay;
        let relay = LocalRelay::new(RelayBuilder::default().addr("127.0.0.1".parse().unwrap()));
        relay.run().await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let ws = open_ws(dir.path());
        let host = pump(&ws, dir.path(), &[relay.url().await.to_string()]).await;
        let bob = Keys::generate();
        let hex = bob.public_key().to_hex();
        says(&host, &bob, join(&a_code(&ws)), 100).await;
        ws.post_message(
            "general",
            bisa_core::MessageBody::post("the history"),
            None,
            &[],
            &[],
            None,
            bisa_store::PostOrigin::Asked,
        )
        .unwrap();
        host.catch_up_now().await;
        assert_eq!(load_ledger(dir.path()).len(), 1, "delivered once");
        let lines = || {
            std::fs::read_to_string(ledger_path(dir.path()))
                .unwrap()
                .lines()
                .map(str::to_string)
                .collect::<Vec<_>>()
        };
        let before = lines().len();

        says(&host, &bob, join(&a_code(&ws)), 200).await;
        let after = lines();
        assert!(after.contains(&format!("-{hex}")), "forgotten: {after:?}");
        assert_eq!(after.len(), before + 2, "and delivered again: {after:?}");
        assert_eq!(load_ledger(dir.path()).len(), 1);

        says(&host, &bob, join(&a_code(&ws)), 300).await;
        assert_eq!(lines().len(), before + 2, "not on every ask");
        assert!(host.inner.people.lock().await.told(&hex).is_some());
        host.stop().await;

        // The same with no relay: their standing is owed.
        let offline = tempfile::tempdir().unwrap();
        let ws = open_ws(offline.path());
        let host = pump(&ws, offline.path(), &[]).await;
        says(&host, &bob, join(&a_code(&ws)), 100).await;
        host.inner.people.lock().await.person(&hex).told = Some(a_standing(MemberRole::Member));
        host.inner.owed.store(false, Ordering::Relaxed);
        says(&host, &bob, join(&a_code(&ws)), 200).await;
        assert!(host.inner.people.lock().await.told(&hex).is_none());
        assert!(host.inner.owed.load(Ordering::Relaxed));
        host.stop().await;
    }
}
