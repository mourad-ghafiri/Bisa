//! Direct QUIC transport over iroh (feature `iroh`): the host's door for a
//! hosted member's client that dials it — the same pairwise gift wraps a
//! relay would carry, and attachment bytes, over authenticated
//! point-to-point QUIC.
//!
//! Local-only by default: the endpoint binds with `RelayMode::Disabled` and
//! no address-lookup services. The `sync.iroh.n0_relays` setting opts into
//! n0's public relay/DNS infrastructure for NAT traversal — that DOES
//! contact third-party servers, hence off by default.
//!
//! ## Protocol (`ALPN = bisa/sync/1`)
//!
//! One bidirectional stream per session; frames are `u32-BE length` +
//! JSON, capped at 1 MiB:
//!
//! - `hello {member_pubkey, sig}` — first frame both ways. `sig` is a Schnorr
//!   signature by the member's nostr key over
//!   `SHA256("bisa-iroh-hello:" || <sender's 32-byte endpoint id>)`,
//!   binding the nostr identity to the TLS-authenticated iroh endpoint.
//!   The receiver verifies the peer is a hosted member (store) and the
//!   signature, else closes.
//! - `have {ids}` — batches (≤500) of **inner GEP event ids** of the
//!   conversation facts the peer reaches. Wrap ids are unstable (ephemeral
//!   keys, randomized timestamps), so reconciliation keys on the inner event
//!   id — the same identifier the store dedupes on.
//! - `want {ids}` — the subset the receiver lacks.
//! - `events {wraps}` — freshly-created gift wraps for wanted events,
//!   pairwise to the peer.
//! - `live {wrap}` — push of a newly-authored wrap while the session is open.
//!
//! Received wraps flow through the exact relay path
//! ([`crate::host::apply_wrap`]): unwrap, then the store's ladder — iroh is
//! a second pipe, not a second trust model. `<data_dir>/iroh_seen.jsonl`
//! records inner ids already processed via iroh so `want` lists converge
//! (e.g. events that ingest rejects as stale would otherwise be re-requested
//! forever).

use crate::config::NetConfig;
use crate::host::Inner;
use crate::NetError;
use iroh::endpoint::{presets, Builder, Connection, RelayMode};
use iroh::{Endpoint, EndpointAddr, EndpointId, SecretKey, TransportAddr};
use nostr::event::Event;
use nostr::key::Keys;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex, Notify};

pub const ALPN: &[u8] = b"bisa/sync/1";
const MAX_FRAME_BYTES: usize = 1 << 20; // 1 MiB
const HAVE_BATCH: usize = 500;
/// Soft byte budget per `events` frame (well under MAX_FRAME_BYTES).
const EVENTS_BATCH_BYTES: usize = 700 * 1024;
const HELLO_DOMAIN: &str = "bisa-iroh-hello:";

fn reconnect_interval() -> Duration {
    std::env::var("BISA_IROH_RECONNECT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(15))
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "t")]
enum Frame {
    Hello {
        member_pubkey: String,
        sig: String,
    },
    Have {
        ids: Vec<String>,
    },
    Want {
        ids: Vec<String>,
    },
    Events {
        wraps: Vec<serde_json::Value>,
    },
    Live {
        wrap: serde_json::Value,
    },
    /// "Do you have this attachment?"
    ///
    /// A message's files do not ride the event — every event is gift-wrapped
    /// once per member and re-wrapped in full when somebody joins, so a photo
    /// in an event would be re-encrypted per person per join, and would not fit
    /// under [`MAX_FRAME_BYTES`] anyway. What syncs is the descriptor; the
    /// bytes come down this pipe, on demand, from whoever has them.
    WantBlob {
        sha256: String,
    },
    /// One chunk of an attachment. `ordinal` counts from zero and `total` is
    /// how many to expect, so a receiver knows when it is done without a
    /// sentinel — and knows a transfer was cut short rather than finished.
    ///
    /// Chunked because the frame cap is 1 MiB and a file may be 25 MB. The
    /// receiver verifies the assembled bytes against `sha256` before storing:
    /// content addressing is only a security property if somebody checks.
    Blob {
        sha256: String,
        ordinal: usize,
        total: usize,
        /// base64, because a frame is JSON.
        chunk: String,
    },
    /// The peer does not have it. Said out loud rather than by silence, so a
    /// waiting request fails fast instead of on a timeout.
    NoBlob {
        sha256: String,
    },
}

/// Raw bytes per `Blob` frame.
///
/// base64 inflates by 4/3, so this lands under [`EVENTS_BATCH_BYTES`] once
/// encoded and comfortably under the 1 MiB frame cap with the JSON envelope
/// on top.
const BLOB_CHUNK_BYTES: usize = 480 * 1024;

/// The most chunks one attachment takes: the ceiling
/// ([`bisa_core::attachment::MAX_ATTACHMENT_BYTES`]) in [`BLOB_CHUNK_BYTES`]
/// pieces. A transfer announced as longer is refused before its first byte
/// is held.
const MAX_BLOB_CHUNKS: usize =
    (bisa_core::attachment::MAX_ATTACHMENT_BYTES as usize).div_ceil(BLOB_CHUNK_BYTES);

/// A known peer's reachability info (from `iroh_addr` announcements or the
/// `sync.iroh.peers` setting), persisted at `<data_dir>/iroh_peers.json`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PeerAddr {
    pub node_id: String,
    pub addrs: Vec<String>,
}

struct Session {
    /// Member pubkey hex (verified via hello).
    member: String,
    tx: mpsc::Sender<Frame>,
    id: u64,
}

pub struct IrohState {
    endpoint: Endpoint,
    data_dir: PathBuf,
    sessions: Mutex<Vec<Session>>,
    session_seq: AtomicU64,
    /// member pubkey hex -> reachability.
    peers: Mutex<HashMap<String, PeerAddr>>,
    /// Inner GEP event ids already processed via iroh (persisted).
    seen: Mutex<HashSet<String>>,
    connect_notify: Notify,
    /// Attachment transfers in flight, by hash: the chunks received so far.
    ///
    /// In memory and per-session for a reason — there is **no resume**. A
    /// dropped connection loses the partial, and the next request starts over.
    /// Persisting half a file would mean reasoning about which half, which is
    /// a lot of machinery for a 25 MB ceiling.
    blobs: Mutex<HashMap<String, Vec<u8>>>,
    /// The hashes this node asked its peers for and has not received. A
    /// chunk for any other hash is dropped unread: a peer chooses what to
    /// answer, never what this node holds in memory.
    wanted_blobs: Mutex<HashSet<String>>,
    /// Woken when a transfer finishes or is refused, so a waiting request
    /// returns as soon as there is an answer rather than on a timer.
    blob_notify: Notify,
}

fn peers_path(data_dir: &Path) -> PathBuf {
    data_dir.join("iroh_peers.json")
}
fn seen_path(data_dir: &Path) -> PathBuf {
    data_dir.join("iroh_seen.jsonl")
}
fn key_path(data_dir: &Path) -> PathBuf {
    data_dir.join("iroh.key")
}

fn load_or_create_secret(data_dir: &Path) -> Result<SecretKey, NetError> {
    let path = key_path(data_dir);
    match std::fs::read_to_string(&path) {
        Ok(s) => {
            let bytes =
                hex::decode(s.trim()).map_err(|e| NetError::Config(format!("iroh.key: {e}")))?;
            let arr: [u8; 32] = bytes
                .try_into()
                .map_err(|_| NetError::Config("iroh.key: expected 32 bytes".into()))?;
            Ok(SecretKey::from_bytes(&arr))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let sk = SecretKey::from_bytes(&rand::random());
            std::fs::write(&path, hex::encode(sk.to_bytes()))?;
            // The key is the endpoint's identity: a file the machine's other
            // users could read is not one to start on.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
            }
            Ok(sk)
        }
        Err(e) => Err(e.into()),
    }
}

fn hello_digest(endpoint_id: &EndpointId) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(HELLO_DOMAIN.as_bytes());
    h.update(endpoint_id.as_bytes());
    h.finalize().into()
}

fn verify_ctx() -> &'static secp256k1::Secp256k1<secp256k1::VerifyOnly> {
    static CTX: std::sync::OnceLock<secp256k1::Secp256k1<secp256k1::VerifyOnly>> =
        std::sync::OnceLock::new();
    CTX.get_or_init(secp256k1::Secp256k1::verification_only)
}

fn hello_frame(keys: &Keys, my_endpoint: &EndpointId) -> Frame {
    let sig = keys.sign_schnorr(hello_digest(my_endpoint));
    Frame::Hello {
        member_pubkey: keys.public_key().to_hex(),
        sig: sig.to_string(),
    }
}

fn verify_hello(member_pubkey: &str, sig: &str, remote: &EndpointId) -> bool {
    let (Ok(pk), Ok(sig)) = (
        secp256k1::XOnlyPublicKey::from_str(member_pubkey),
        secp256k1::schnorr::Signature::from_str(sig),
    ) else {
        return false;
    };
    verify_ctx()
        .verify_schnorr(&sig, &hello_digest(remote), &pk)
        .is_ok()
}

impl IrohState {
    /// Bind the endpoint (local-only unless `sync.iroh.n0_relays`).
    pub(crate) async fn bind(data_dir: &Path, cfg: &NetConfig) -> Result<Self, NetError> {
        let secret = load_or_create_secret(data_dir)?;
        let endpoint = if cfg.iroh_n0_relays {
            // Opt-in: n0's public relay + DNS lookup infrastructure.
            Endpoint::builder(presets::N0)
                .secret_key(secret)
                .alpns(vec![ALPN.to_vec()])
                .bind()
                .await
        } else {
            Builder::empty()
                .crypto_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .relay_mode(RelayMode::Disabled)
                .secret_key(secret)
                .alpns(vec![ALPN.to_vec()])
                .bind()
                .await
        }
        .map_err(|e| NetError::Config(format!("iroh bind: {e}")))?;

        let mut peers: HashMap<String, PeerAddr> = HashMap::new();
        if let Ok(body) = std::fs::read_to_string(peers_path(data_dir)) {
            if let Ok(map) = serde_json::from_str::<HashMap<String, PeerAddr>>(&body) {
                peers = map;
            }
        }
        for p in &cfg.iroh_peers {
            peers.insert(
                p.member_pubkey.clone(),
                PeerAddr {
                    node_id: p.node_id.clone(),
                    addrs: p.addrs.clone(),
                },
            );
        }
        let mut seen = HashSet::new();
        if let Ok(body) = std::fs::read_to_string(seen_path(data_dir)) {
            seen.extend(
                body.lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| l.trim().to_string()),
            );
        }
        Ok(Self {
            endpoint,
            data_dir: data_dir.to_path_buf(),
            sessions: Mutex::new(Vec::new()),
            session_seq: AtomicU64::new(1),
            peers: Mutex::new(peers),
            seen: Mutex::new(seen),
            connect_notify: Notify::new(),
            blobs: Mutex::new(HashMap::new()),
            wanted_blobs: Mutex::new(HashSet::new()),
            blob_notify: Notify::new(),
        })
    }

    /// A transfer finished — wake anybody waiting on it.
    async fn blob_arrived(&self, sha256: &str) {
        self.wanted_blobs.lock().await.remove(sha256);
        self.blob_notify.notify_waiters();
    }

    /// A peer said it does not have it. Same wake, so the waiter re-checks the
    /// store, finds nothing, and asks the next peer instead of timing out.
    async fn blob_absent(&self, sha256: &str) {
        self.blobs.lock().await.remove(sha256);
        self.blob_notify.notify_waiters();
    }

    /// Whether this node asked for the hash and is still waiting on it.
    async fn wants_blob(&self, sha256: &str) -> bool {
        self.wanted_blobs.lock().await.contains(sha256)
    }

    /// Ask every connected peer for an attachment, and wait for the first that
    /// answers with the bytes.
    ///
    /// Broadcast rather than addressed: nothing records *which* member has a
    /// blob, and the sender of the message that named it may well be offline
    /// while somebody else who received it is not. Asking everyone costs one
    /// small frame per session.
    ///
    /// `false` means nobody answered in time, which the route reports as a 404
    /// — an honest "not here and not obtainable right now" rather than an error.
    pub async fn request_blob(
        &self,
        ws: &bisa_store::Workspace,
        sha256: &str,
        wait: Duration,
    ) -> bool {
        if ws.attachment_path(sha256).is_some() {
            return true;
        }
        {
            let sessions = self.sessions.lock().await;
            if sessions.is_empty() {
                return false;
            }
            self.wanted_blobs.lock().await.insert(sha256.to_string());
            for s in sessions.iter() {
                send_or_end(
                    &s.tx,
                    Frame::WantBlob {
                        sha256: sha256.to_string(),
                    },
                    &s.member,
                )
                .await;
            }
        }
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            // Subscribe before checking, so a transfer that lands between the
            // check and the wait is not missed.
            let woken = self.blob_notify.notified();
            if ws.attachment_path(sha256).is_some() {
                return true;
            }
            if tokio::time::timeout_at(deadline, woken).await.is_err() {
                // Nobody answered in time: the want is withdrawn, so a late
                // chunk is dropped instead of assembled for no one.
                self.wanted_blobs.lock().await.remove(sha256);
                self.blobs.lock().await.remove(sha256);
                return ws.attachment_path(sha256).is_some();
            }
        }
    }

    pub fn node_id(&self) -> String {
        self.endpoint.id().to_string()
    }

    /// Direct socket addresses to advertise (0.0.0.0 rewritten to loopback —
    /// LAN users advertise their real interface via manual peers or the n0
    /// opt-in; announcements are hints, not the only path).
    pub fn direct_addrs(&self) -> Vec<String> {
        self.endpoint
            .bound_sockets()
            .into_iter()
            .map(|sa| {
                if sa.ip().is_unspecified() {
                    format!("127.0.0.1:{}", sa.port())
                } else {
                    sa.to_string()
                }
            })
            .collect()
    }

    pub fn peers_connected(&self) -> usize {
        self.sessions
            .try_lock()
            .map(|s| {
                s.iter()
                    .map(|x| x.member.clone())
                    .collect::<HashSet<_>>()
                    .len()
            })
            .unwrap_or(0)
    }

    pub(crate) async fn learn_peer(&self, member_hex: &str, node_id: &str, addrs: &[String]) {
        {
            let mut peers = self.peers.lock().await;
            peers.insert(
                member_hex.to_string(),
                PeerAddr {
                    node_id: node_id.to_string(),
                    addrs: addrs.to_vec(),
                },
            );
            self.persist_peers(&peers);
        }
        self.connect_notify.notify_waiters();
    }

    /// A member left or was removed: their address is forgotten, on disk
    /// too, and their live sessions end — the connector never dials them
    /// again, and a frame of theirs still in flight reaches nothing.
    pub(crate) async fn forget_peer(&self, member_hex: &str) {
        {
            let mut peers = self.peers.lock().await;
            if peers.remove(member_hex).is_some() {
                self.persist_peers(&peers);
            }
        }
        let mut sessions = self.sessions.lock().await;
        let before = sessions.len();
        sessions.retain(|s| s.member != member_hex);
        if sessions.len() != before {
            tracing::info!("iroh sessions with {member_hex} ended: no longer hosted");
        }
    }

    /// The peers, whole, to `iroh_peers.json` through a rename so a reader
    /// never sees half a file. A failure is said: the addresses stay in
    /// memory and are learnt again from the next announcement.
    fn persist_peers(&self, peers: &HashMap<String, PeerAddr>) {
        let path = peers_path(&self.data_dir);
        let body = match serde_json::to_string_pretty(peers) {
            Ok(body) => body,
            Err(e) => {
                tracing::warn!("iroh peers not written: {e}");
                return;
            }
        };
        let tmp = path.with_extension(format!("json.tmp.{}", std::process::id()));
        let written = std::fs::write(&tmp, body).and_then(|()| std::fs::rename(&tmp, &path));
        if let Err(e) = written {
            tracing::warn!(path = %path.display(), "iroh peers not written: {e}");
        }
    }

    /// One id to the seen ledger. A line that does not land is said: the
    /// fact is applied once more after a restart, which the store ignores as
    /// a duplicate.
    async fn mark_seen(&self, inner_id: &str) {
        let mut seen = self.seen.lock().await;
        if seen.insert(inner_id.to_string()) {
            use std::io::Write;
            let appended = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(seen_path(&self.data_dir))
                .and_then(|mut f| writeln!(f, "{inner_id}"));
            if let Err(e) = appended {
                tracing::warn!("iroh seen ledger not appended: {e}");
            }
        }
    }
}

/// One frame to a session's writer. `false` when the writer is gone — the
/// session is ending and its reader will notice — so a caller stops
/// preparing frames nobody will read.
async fn send_or_end(tx: &mpsc::Sender<Frame>, frame: Frame, member: &str) -> bool {
    if tx.send(frame).await.is_err() {
        tracing::debug!("iroh session with {member} is gone; a frame was not sent");
        return false;
    }
    true
}

/// Spawn the accept loop and the connector loop.
pub(crate) fn spawn_loops(inner: &Arc<Inner>, tasks: &mut Vec<tokio::task::JoinHandle<()>>) {
    // Accept loop.
    {
        let inner = Arc::clone(inner);
        tasks.push(tokio::spawn(async move {
            let Some(st) = inner.iroh.clone() else { return };
            while let Some(incoming) = st.endpoint.accept().await {
                let inner = Arc::clone(&inner);
                tokio::spawn(async move {
                    match incoming.await {
                        Ok(conn) => handle_conn(&inner, conn, false).await,
                        Err(e) => tracing::debug!("iroh incoming failed: {e}"),
                    }
                });
            }
        }));
    }
    // Connector loop: dial known peers without a live session.
    {
        let inner = Arc::clone(inner);
        tasks.push(tokio::spawn(async move {
            let Some(st) = inner.iroh.clone() else { return };
            loop {
                let targets: Vec<(String, PeerAddr)> = {
                    let peers = st.peers.lock().await;
                    let sessions = st.sessions.lock().await;
                    let live: HashSet<&String> = sessions.iter().map(|s| &s.member).collect();
                    peers
                        .iter()
                        .filter(|(m, _)| !live.contains(m))
                        .map(|(m, p)| (m.clone(), p.clone()))
                        .collect()
                };
                for (member, peer) in targets {
                    // Only dial members (a removed member stays unreachable).
                    if crate::host::hosted_role(&inner, &member).is_none() {
                        continue;
                    }
                    let Ok(id) = EndpointId::from_str(&peer.node_id) else {
                        continue;
                    };
                    let addrs = peer
                        .addrs
                        .iter()
                        .filter_map(|a| a.parse().ok())
                        .map(TransportAddr::Ip);
                    let addr = EndpointAddr::from_parts(id, addrs);
                    match st.endpoint.connect(addr, ALPN).await {
                        Ok(conn) => {
                            let inner = Arc::clone(&inner);
                            tokio::spawn(async move {
                                handle_conn(&inner, conn, true).await;
                            });
                        }
                        Err(e) => tracing::debug!("iroh dial {member} failed: {e}"),
                    }
                }
                // Woken by a connect, or the interval elapsed: both mean try now.
                let _woken_or_elapsed =
                    tokio::time::timeout(reconnect_interval(), st.connect_notify.notified()).await;
            }
        }));
    }
}

/// Announce our endpoint id + direct addresses to every hosted member
/// (pairwise control messages over the relay pool; no-op without relays —
/// manual peers bootstrap the relay-less case).
pub(crate) async fn announce_addrs(inner: &Arc<Inner>) {
    let Some(st) = inner.iroh.clone() else { return };
    let control = bisa_collab::Control::IrohAddr {
        node_id: st.node_id(),
        addrs: st.direct_addrs(),
    };
    let Ok(people) = inner.ws.people() else {
        return;
    };
    for m in people {
        if let Ok(pk) = nostr::key::PublicKey::from_hex(m.pubkey.as_hex()) {
            crate::host::send_control(inner, pk, &control).await;
        }
    }
}

/// Push one freshly-created wrap to live sessions. `member` = the pairwise
/// audience (only that member's session); `None` = every session.
pub(crate) async fn live_push(inner: &Arc<Inner>, member: Option<&str>, wrap: &Event) {
    let Some(st) = inner.iroh.clone() else { return };
    let Ok(value) = serde_json::to_value(wrap) else {
        return;
    };
    let sessions = st.sessions.lock().await;
    for s in sessions.iter() {
        if member.is_none_or(|m| m == s.member) {
            let pushed = s.tx.try_send(Frame::Live {
                wrap: value.clone(),
            });
            if let Err(e) = pushed {
                // Not lost: the next reconciliation advertises it by id.
                tracing::debug!(
                    "live push to {} not taken ({e}); the next have/want round carries it",
                    s.member
                );
            }
        }
    }
}

/// One authenticated session over one bi-stream.
async fn handle_conn(inner: &Arc<Inner>, conn: Connection, initiated: bool) {
    let Some(st) = inner.iroh.clone() else { return };
    let remote = conn.remote_id();

    let (mut send, mut recv) = if initiated {
        match conn.open_bi().await {
            Ok(p) => p,
            Err(e) => {
                tracing::debug!("iroh open_bi failed: {e}");
                return;
            }
        }
    } else {
        match conn.accept_bi().await {
            Ok(p) => p,
            Err(e) => {
                tracing::debug!("iroh accept_bi failed: {e}");
                return;
            }
        }
    };

    // Mutual hello: initiator first, acceptor answers after verifying.
    let my_hello = hello_frame(&inner.keys, &st.endpoint.id());
    if initiated && write_frame(&mut send, &my_hello).await.is_err() {
        return;
    }
    let member = match read_frame(&mut recv).await {
        Ok(Frame::Hello { member_pubkey, sig }) => {
            let is_hosted = crate::host::hosted_role(inner, &member_pubkey).is_some();
            if !is_hosted || !verify_hello(&member_pubkey, &sig, &remote) {
                tracing::warn!("iroh hello rejected from {remote} ({member_pubkey})");
                conn.close(1u8.into(), b"auth");
                return;
            }
            member_pubkey
        }
        _ => {
            conn.close(1u8.into(), b"protocol");
            return;
        }
    };
    if !initiated && write_frame(&mut send, &my_hello).await.is_err() {
        return;
    }

    tracing::info!("iroh session established with member {member}");
    let (tx, mut rx) = mpsc::channel::<Frame>(256);
    let session_id = st.session_seq.fetch_add(1, Ordering::Relaxed);
    st.sessions.lock().await.push(Session {
        member: member.clone(),
        tx: tx.clone(),
        id: session_id,
    });

    // Writer: everything leaves through this task.
    let writer = tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            if write_frame(&mut send, &frame).await.is_err() {
                break;
            }
        }
    });

    // Kick off reconciliation: advertise the facts this peer reaches. A
    // fact outside their reach is never advertised — the ids alone would
    // leak who is talking to whom.
    {
        let ids: Vec<String> = crate::truth::conversation_facts(&inner.ws)
            .map(|evs| {
                evs.iter()
                    .filter(|e| visible_to_peer(inner, e, &member))
                    .map(|e| e.id.to_hex())
                    .collect()
            })
            .unwrap_or_default();
        for chunk in ids.chunks(HAVE_BATCH) {
            let have = Frame::Have {
                ids: chunk.to_vec(),
            };
            if !send_or_end(&tx, have, &member).await {
                break;
            }
        }
    }

    // Reader loop.
    loop {
        let frame = match read_frame(&mut recv).await {
            Ok(f) => f,
            Err(e) => {
                tracing::debug!("iroh session with {member} ended: {e}");
                break;
            }
        };
        match frame {
            Frame::Hello { .. } => {} // already authenticated; ignore
            Frame::Have { ids } => {
                let known: HashSet<String> = crate::truth::conversation_facts(&inner.ws)
                    .map(|evs| evs.iter().map(|e| e.id.to_hex()).collect())
                    .unwrap_or_default();
                let seen = st.seen.lock().await;
                let want: Vec<String> = ids
                    .into_iter()
                    .filter(|id| !known.contains(id) && !seen.contains(id))
                    .collect();
                drop(seen);
                for chunk in want.chunks(HAVE_BATCH) {
                    let want = Frame::Want {
                        ids: chunk.to_vec(),
                    };
                    if !send_or_end(&tx, want, &member).await {
                        break;
                    }
                }
            }
            Frame::Want { ids } => send_wanted(inner, &member, &tx, &ids).await,
            Frame::WantBlob { sha256 } => send_blob(inner, &member, &tx, &sha256).await,
            Frame::Blob {
                sha256,
                ordinal,
                total,
                chunk,
            } => receive_blob_chunk(inner, &st, &sha256, ordinal, total, &chunk).await,
            Frame::NoBlob { sha256 } => {
                // Said out loud so a waiter fails now rather than on a timeout.
                st.blob_absent(&sha256).await;
            }
            Frame::Events { wraps } => {
                for w in wraps {
                    apply_wrap(inner, &st, w).await;
                }
            }
            Frame::Live { wrap } => apply_wrap(inner, &st, wrap).await,
        }
    }

    // Teardown.
    st.sessions.lock().await.retain(|s| s.id != session_id);
    writer.abort();
    st.connect_notify.notify_waiters();
}

/// Answer a peer's request for an attachment, in chunks or with a refusal.
///
/// No visibility check, deliberately, and it is worth saying why: a peer only
/// learns a hash by receiving a message that names it, and a message reaches a
/// peer only through [`visible_to_peer`]. The hash *is* the capability. Adding
/// a second check here would mean re-deriving which messages a peer may see
/// from the blob alone, which is the same authority expressed twice.
async fn send_blob(inner: &Arc<Inner>, member: &str, tx: &mpsc::Sender<Frame>, sha256: &str) {
    let Ok(Some(bytes)) = inner.ws.attachment_bytes(sha256) else {
        let absent = Frame::NoBlob {
            sha256: sha256.to_string(),
        };
        send_or_end(tx, absent, member).await;
        return;
    };
    let total = bytes.len().div_ceil(BLOB_CHUNK_BYTES).max(1);
    for (ordinal, part) in bytes.chunks(BLOB_CHUNK_BYTES).enumerate() {
        let chunk = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, part);
        let piece = Frame::Blob {
            sha256: sha256.to_string(),
            ordinal,
            total,
            chunk,
        };
        if !send_or_end(tx, piece, member).await {
            return; // the session went away mid-transfer
        }
    }
}

/// Take one chunk, and store the file once the last one lands.
///
/// **The hash is checked before anything is written.** A peer that answered a
/// request for one file with another is caught here and nowhere else, which is
/// what makes the address a proof rather than a filing convention.
async fn receive_blob_chunk(
    inner: &Arc<Inner>,
    st: &Arc<IrohState>,
    sha256: &str,
    ordinal: usize,
    total: usize,
    chunk: &str,
) {
    if !st.wants_blob(sha256).await {
        tracing::debug!("a blob chunk for {sha256} nobody here asked for; dropped");
        return;
    }
    if total == 0 || total > MAX_BLOB_CHUNKS {
        tracing::warn!(
            "a blob announced as {total} chunks is outside the attachment ceiling; dropped"
        );
        st.blob_absent(sha256).await;
        return;
    }
    let Ok(part) = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, chunk) else {
        tracing::warn!("a blob chunk was not base64");
        return;
    };
    if part.len() > BLOB_CHUNK_BYTES {
        tracing::warn!(
            "a blob chunk of {} bytes is over the chunk size; dropping the transfer",
            part.len()
        );
        st.blob_absent(sha256).await;
        return;
    }
    let assembled = {
        let mut partials = st.blobs.lock().await;
        let slot = partials.entry(sha256.to_string()).or_default();
        // Out of order, or a repeat, means the sender is not one of ours or is
        // confused. Either way the transfer cannot be trusted to assemble.
        if ordinal != slot.len() {
            tracing::warn!(
                "blob chunk {ordinal} arrived for {sha256} with {} already held; dropping the transfer",
                slot.len()
            );
            partials.remove(sha256);
            return;
        }
        slot.extend_from_slice(&part);
        if ordinal + 1 < total {
            return;
        }
        partials.remove(sha256).unwrap_or_default()
    };

    match inner.ws.accept_attachment(sha256, &assembled) {
        Ok(()) => tracing::info!("fetched attachment {sha256} from a peer"),
        // The one failure worth a warning rather than a debug: it means a peer
        // handed us something other than what we asked for.
        Err(e) => tracing::warn!("refusing a peer's attachment: {e}"),
    }
    st.blob_arrived(sha256).await;
}

/// Whether `peer_hex` (an authenticated hosted member) reaches this fact —
/// the one rule, [`crate::truth::reaches_fact`], by their role.
fn visible_to_peer(inner: &Arc<Inner>, event: &nostr::event::Event, peer_hex: &str) -> bool {
    match crate::host::hosted_role(inner, peer_hex) {
        Some((person, role)) => crate::truth::reaches_fact(&inner.ws, event, &person, role),
        None => false,
    }
}

/// Wrap + send the requested truth events, batched under the frame cap.
async fn send_wanted(
    inner: &Arc<Inner>,
    member_hex: &str,
    tx: &mpsc::Sender<Frame>,
    ids: &[String],
) {
    let wanted: HashSet<&String> = ids.iter().collect();
    let events = match crate::truth::conversation_facts(&inner.ws) {
        Ok(evs) => evs,
        Err(e) => {
            tracing::warn!("truth scan failed: {e}");
            return;
        }
    };
    let member_pk = match nostr::key::PublicKey::from_hex(member_hex) {
        Ok(pk) => pk,
        Err(_) => return,
    };

    let mut batch: Vec<serde_json::Value> = Vec::new();
    let mut batch_bytes = 0usize;
    for ev in events {
        if !wanted.contains(&ev.id.to_hex()) {
            continue;
        }
        // Only what they reach, wrapped to them alone.
        if !visible_to_peer(inner, &ev, member_hex) {
            continue;
        }
        match bisa_collab::wrap_for_member(&inner.keys, member_pk, &ev) {
            Ok(wrapped) => {
                let json = match serde_json::to_value(&wrapped) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let size = json.to_string().len();
                if !batch.is_empty() && batch_bytes + size > EVENTS_BATCH_BYTES {
                    let events = Frame::Events {
                        wraps: std::mem::take(&mut batch),
                    };
                    if !send_or_end(tx, events, member_hex).await {
                        return;
                    }
                    batch_bytes = 0;
                }
                batch_bytes += size;
                batch.push(json);
            }
            Err(e) => tracing::warn!("iroh wrap failed: {e}"),
        }
    }
    if !batch.is_empty() {
        send_or_end(tx, Frame::Events { wraps: batch }, member_hex).await;
    }
}

async fn apply_wrap(inner: &Arc<Inner>, st: &Arc<IrohState>, wrap: serde_json::Value) {
    let ev = match serde_json::from_value::<Event>(wrap) {
        Ok(ev) => ev,
        Err(e) => {
            tracing::debug!("iroh frame carried a non-event: {e}");
            return;
        }
    };
    if let Some(inner_id) = crate::host::apply_wrap(inner, &ev).await {
        st.mark_seen(&inner_id).await;
    }
}

async fn write_frame(send: &mut iroh::endpoint::SendStream, frame: &Frame) -> Result<(), NetError> {
    let body = serde_json::to_vec(frame).map_err(|e| NetError::Config(e.to_string()))?;
    if body.len() > MAX_FRAME_BYTES {
        return Err(NetError::Config("frame exceeds 1 MiB cap".into()));
    }
    let len = (body.len() as u32).to_be_bytes();
    send.write_all(&len)
        .await
        .map_err(|e| NetError::Relay(e.to_string()))?;
    send.write_all(&body)
        .await
        .map_err(|e| NetError::Relay(e.to_string()))?;
    Ok(())
}

async fn read_frame(recv: &mut iroh::endpoint::RecvStream) -> Result<Frame, NetError> {
    let mut len_buf = [0u8; 4];
    recv.read_exact(&mut len_buf)
        .await
        .map_err(|e| NetError::Relay(e.to_string()))?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_FRAME_BYTES {
        return Err(NetError::Config("frame exceeds 1 MiB cap".into()));
    }
    let mut body = vec![0u8; len];
    recv.read_exact(&mut body)
        .await
        .map_err(|e| NetError::Relay(e.to_string()))?;
    serde_json::from_slice(&body).map_err(|e| NetError::Config(format!("bad frame: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blob_is_at_most_the_attachment_ceiling_in_chunks() {
        assert_eq!(MAX_BLOB_CHUNKS, 54, "25 MiB in 480 KiB pieces");
        assert!(
            MAX_BLOB_CHUNKS * BLOB_CHUNK_BYTES
                >= bisa_core::attachment::MAX_ATTACHMENT_BYTES as usize
        );
        assert!(
            (MAX_BLOB_CHUNKS - 1) * BLOB_CHUNK_BYTES
                < bisa_core::attachment::MAX_ATTACHMENT_BYTES as usize
        );
    }

    #[test]
    fn a_hello_verifies_against_the_endpoint_it_was_signed_for_alone() {
        let keys = Keys::generate();
        let mine = SecretKey::from_bytes(&rand::random()).public();
        let other = SecretKey::from_bytes(&rand::random()).public();
        let Frame::Hello { member_pubkey, sig } = hello_frame(&keys, &mine) else {
            panic!("a hello frame");
        };
        assert_eq!(member_pubkey, keys.public_key().to_hex());
        assert!(verify_hello(&member_pubkey, &sig, &mine));
        assert!(
            !verify_hello(&member_pubkey, &sig, &other),
            "another endpoint's id"
        );
        assert!(
            !verify_hello(&Keys::generate().public_key().to_hex(), &sig, &mine),
            "another member's key"
        );
        assert!(!verify_hello("not a key", &sig, &mine));
    }
}
