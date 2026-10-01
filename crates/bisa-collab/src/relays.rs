//! The relay pool: one `nostr_sdk::Client` over the relays a node reads and
//! writes, with the health of each relay in words, a check before a relay is
//! added, a live reconnect, and one inbound stream of everything addressed
//! to this node's key — unwrapped once, here, and broadcast as [`Incoming`].
//!
//! Every real relay is `wss://`: the pool makes sure the process has a TLS
//! crypto provider before it makes a client ([`crate::tls`]) — without one the
//! first connection panicked inside the relay's own task and the relay sat at
//! *connecting* for ever, saying nothing.
//!
//! The pool is transport and nothing else: it knows no store, no membership
//! and no host. A host pump and a guest session both sit on top of it, and a
//! mobile client embeds it as it is. Relays see kind-1059 ciphertext and a
//! recipient; the pool sees the plaintext once and hands it on.
//!
//! Resilience is the relay layer's: every relay reconnects on its own with a
//! growing retry interval; a relay that never answers shows as such in
//! [`RelayHealth`] rather than failing the pool. A pool with no relays is a
//! pool that reaches nobody, and says so through [`Relays::connected_count`].

use crate::wrap::{unwrap_incoming, Incoming};
use crate::CollabError;
use futures::StreamExt;
use nostr::event::{Event, EventBuilder, FinalizeEvent, Kind, Tag};
use nostr::key::{Keys, PublicKey};
use nostr::types::Timestamp;
use nostr_sdk::client::{Client, ClientNotification};
use nostr_sdk::prelude::{Filter, RelayStatus, SubscriptionId};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, Mutex};

/// Gift-wrap `created_at` is randomized up to two days into the past
/// (NIP-59), so a since-window fetch over-fetches by at least that much.
pub const WRAP_TIMESTAMP_SLACK_SECS: u64 = 3 * 24 * 3600;
/// How long a first connection is given before the pool goes on without it.
const CONNECT_WAIT: Duration = Duration::from_secs(5);
/// How long a catch-up fetch may take.
const FETCH_WAIT: Duration = Duration::from_secs(10);
/// The first retry after a drop; the relay layer grows it from here.
const RETRY_INTERVAL: Duration = Duration::from_secs(10);
/// How many wrap ids the pool remembers, so a catch-up never hands the same
/// wrap on twice. Bounded: a long-running node forgets the oldest.
const SEEN_WRAPS: usize = 4096;
/// The one subscription the pool holds: wraps to this key. Re-subscribing
/// under the same id replaces it rather than stacking a second one.
const SUBSCRIPTION: &str = "bisa-wraps";

/// A relay's state, in the relay layer's words.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RelayStatusWord {
    /// Configured, and not talked to: the wire is off.
    Off,
    Initialized,
    Pending,
    Connecting,
    Connected,
    Disconnected,
    Terminated,
    Banned,
    Sleeping,
    Shutdown,
}

impl From<RelayStatus> for RelayStatusWord {
    fn from(s: RelayStatus) -> Self {
        match s {
            RelayStatus::Initialized => Self::Initialized,
            RelayStatus::Pending => Self::Pending,
            RelayStatus::Connecting => Self::Connecting,
            RelayStatus::Connected => Self::Connected,
            RelayStatus::Disconnected => Self::Disconnected,
            RelayStatus::Terminated => Self::Terminated,
            RelayStatus::Banned => Self::Banned,
            RelayStatus::Sleeping => Self::Sleeping,
            RelayStatus::Shutdown => Self::Shutdown,
        }
    }
}

/// One relay as it stands: what the settings panel's row shows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RelayHealth {
    pub url: String,
    pub status: RelayStatusWord,
    pub connected: bool,
    /// Connection attempts since the pool started.
    pub attempts: usize,
    /// Attempts that connected.
    pub success: usize,
    /// `success / attempts`, 0 when never tried.
    pub success_rate: f64,
    /// The last measured round trip, when the relay answered a ping.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    /// Unix seconds of the current connection's start; absent while down.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connected_at: Option<u64>,
    pub bytes_sent: usize,
    pub bytes_received: usize,
    /// Why a relay that was tried is not connected, in a sentence
    /// ([`relay_problem`]); absent while it is connected, off, or not tried yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

/// Why a relay is not connected, from what the relay layer keeps about it:
/// its status word and how its attempts went. `None` while it is connected,
/// while the wire is off, and before it was ever tried — a row says a problem
/// only once there is one to say. It names what is known and guesses no
/// cause: the layer does not keep the last error, and a sentence that
/// invented one would send a person looking in the wrong place.
pub fn relay_problem(status: RelayStatusWord, attempts: usize, success: usize) -> Option<String> {
    use RelayStatusWord as W;
    match status {
        W::Connected | W::Off => None,
        W::Banned => Some("the relay banned this node — it refuses every connection from it".into()),
        W::Terminated | W::Shutdown => {
            Some("the connection was shut down and is not retried — reconnect to try again".into())
        }
        _ if attempts == 0 => None,
        _ if success == 0 => Some(format!(
            "never connected in {attempts} {} — the address may be wrong, the relay down, or this network may not reach it (relays are dialled directly, never through a proxy)",
            if attempts == 1 { "attempt" } else { "attempts" }
        )),
        W::Sleeping => Some("idle — the relay layer put the connection to sleep and wakes it when there is something to send".into()),
        _ => Some(format!(
            "the connection dropped after {success} of {attempts} attempts connected — it is retried on its own"
        )),
    }
}

impl RelayHealth {
    /// A configured relay the node is not talking to — the wire is off. No
    /// attempts, no connection, nothing moved.
    pub fn off(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            status: RelayStatusWord::Off,
            connected: false,
            attempts: 0,
            success: 0,
            success_rate: 0.0,
            latency_ms: None,
            connected_at: None,
            bytes_sent: 0,
            bytes_received: 0,
            problem: None,
        }
    }
}

/// What a relay answered when tried once, before it is added.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RelayCheck {
    pub url: String,
    pub ok: bool,
    /// How long the handshake took, when it succeeded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Wrap ids already handed on, bounded.
#[derive(Default)]
struct SeenWraps {
    order: VecDeque<String>,
    set: HashSet<String>,
}

impl SeenWraps {
    /// `true` the first time an id is seen.
    fn note(&mut self, id: String) -> bool {
        if self.set.contains(&id) {
            return false;
        }
        if self.order.len() >= SEEN_WRAPS {
            if let Some(old) = self.order.pop_front() {
                self.set.remove(&old);
            }
        }
        self.order.push_back(id.clone());
        self.set.insert(id);
        true
    }
}

/// The pool.
pub struct Relays {
    keys: Keys,
    client: Client,
    urls: Mutex<Vec<String>>,
    incoming: broadcast::Sender<Incoming>,
    seen: Mutex<SeenWraps>,
    inbound: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

fn relay_filter(me: PublicKey) -> Filter {
    Filter::new().kind(Kind::GiftWrap).pubkey(me)
}

impl Relays {
    /// Open the pool on `urls` and start hearing wraps to `keys`. Never
    /// fails on a relay that does not answer: the relay reconnects on its
    /// own and its row says so. Returns once every relay was given
    /// [`CONNECT_WAIT`] to connect.
    pub async fn start(keys: Keys, urls: &[String]) -> Arc<Self> {
        // Before the client exists: its first `wss://` connection needs the
        // process's crypto provider, and panics without one.
        crate::tls::ensure_crypto_provider();
        let client = Client::new();
        let (incoming, _) = broadcast::channel(1024);
        let pool = Arc::new(Self {
            keys,
            client,
            urls: Mutex::new(Vec::new()),
            incoming,
            seen: Mutex::new(SeenWraps::default()),
            inbound: Mutex::new(None),
        });
        pool.set_relays(urls).await;
        let task = {
            let pool = Arc::clone(&pool);
            tokio::spawn(async move {
                let mut stream = pool.client.notifications();
                while let Some(n) = stream.next().await {
                    if let ClientNotification::Event { event, .. } = n {
                        if event.kind == Kind::GiftWrap {
                            pool.take_wrap(&event).await;
                        }
                    }
                }
            })
        };
        *pool.inbound.lock().await = Some(task);
        pool
    }

    pub fn public_key(&self) -> PublicKey {
        self.keys.public_key()
    }

    pub fn keys(&self) -> &Keys {
        &self.keys
    }

    /// The relays as configured, in order.
    pub async fn urls(&self) -> Vec<String> {
        self.urls.lock().await.clone()
    }

    /// Make the pool match `urls`: relays no longer named are dropped,
    /// new ones added and connected, the rest left as they are. The live
    /// subscription is renewed so a new relay hears wraps at once.
    pub async fn set_relays(&self, urls: &[String]) {
        let wanted: Vec<String> = urls
            .iter()
            .map(|u| u.trim().to_string())
            .filter(|u| !u.is_empty())
            .fold(Vec::new(), |mut acc, u| {
                if !acc.contains(&u) {
                    acc.push(u);
                }
                acc
            });
        let current = self.urls.lock().await.clone();
        for gone in current.iter().filter(|u| !wanted.contains(u)) {
            if let Err(e) = self.client.remove_relay(gone.as_str()).force().await {
                tracing::debug!("removing relay {gone}: {e}");
            }
        }
        for url in wanted.iter().filter(|u| !current.contains(u)) {
            let added = self
                .client
                .add_relay(url.as_str())
                .reconnect(true)
                .retry_interval(RETRY_INTERVAL)
                .adjust_retry_interval(true)
                .ping(true)
                .await;
            match added {
                Ok(_) => {
                    if let Err(e) = self.client.connect_relay(url.as_str()).await {
                        tracing::debug!("connecting relay {url}: {e}");
                    }
                }
                Err(e) => tracing::warn!("relay {url} refused by the pool: {e}"),
            }
        }
        *self.urls.lock().await = wanted.clone();
        if wanted.is_empty() {
            return;
        }
        self.client.connect().and_wait(CONNECT_WAIT).await;
        self.resubscribe().await;
    }

    async fn resubscribe(&self) {
        let filter = relay_filter(self.keys.public_key());
        if let Err(e) = self
            .client
            .subscribe(filter)
            .with_id(SubscriptionId::new(SUBSCRIPTION))
            .await
        {
            tracing::warn!("subscribing for wraps: {e}");
        }
    }

    /// Every relay's row.
    pub async fn health(&self) -> Vec<RelayHealth> {
        let urls = self.urls.lock().await.clone();
        let relays = self.client.relays().await;
        urls.into_iter()
            .map(|url| {
                let relay = relays
                    .iter()
                    .find(|(u, _)| u.to_string() == url)
                    .map(|(_, r)| r);
                match relay {
                    Some(r) => {
                        let status = r.status();
                        let stats = r.stats();
                        let connected = status.is_connected();
                        let connected_at = stats.connected_at().as_secs();
                        let word: RelayStatusWord = status.into();
                        RelayHealth {
                            url,
                            status: word,
                            connected,
                            problem: relay_problem(word, stats.attempts(), stats.success()),
                            attempts: stats.attempts(),
                            success: stats.success(),
                            success_rate: stats.success_rate(),
                            latency_ms: stats.latency().map(|d| d.as_millis() as u64),
                            connected_at: (connected && connected_at > 0).then_some(connected_at),
                            bytes_sent: stats.bytes_sent(),
                            bytes_received: stats.bytes_received(),
                        }
                    }
                    None => RelayHealth {
                        url,
                        status: RelayStatusWord::Initialized,
                        connected: false,
                        attempts: 0,
                        success: 0,
                        success_rate: 0.0,
                        latency_ms: None,
                        connected_at: None,
                        bytes_sent: 0,
                        bytes_received: 0,
                        problem: None,
                    },
                }
            })
            .collect()
    }

    /// How many relays are connected right now.
    pub async fn connected_count(&self) -> usize {
        self.health().await.iter().filter(|h| h.connected).count()
    }

    /// How many relays are configured.
    pub async fn count(&self) -> usize {
        self.urls.lock().await.len()
    }

    /// Try one relay once, on a throwaway client, within `budget`. What the
    /// settings panel shows before a relay is added.
    pub async fn check(url: &str, budget: Duration) -> RelayCheck {
        let url = url.trim().to_string();
        crate::tls::ensure_crypto_provider();
        let client = Client::new();
        let started = std::time::Instant::now();
        let outcome = match client.add_relay(url.as_str()).await {
            Ok(_) => client
                .try_connect_relay(url.as_str(), budget)
                .await
                .map_err(|e| e.to_string()),
            Err(e) => Err(e.to_string()),
        };
        client.shutdown().await;
        match outcome {
            Ok(()) => RelayCheck {
                url,
                ok: true,
                latency_ms: Some(started.elapsed().as_millis() as u64),
                error: None,
            },
            Err(error) => RelayCheck {
                url,
                ok: false,
                latency_ms: None,
                error: Some(error),
            },
        }
    }

    /// Drop every connection and open them again.
    pub async fn reconnect(&self) {
        self.client.disconnect().await;
        self.client.connect().and_wait(CONNECT_WAIT).await;
        self.resubscribe().await;
    }

    /// Send one event to every relay. An error means no relay took it.
    pub async fn publish(&self, event: &Event) -> Result<(), CollabError> {
        self.client
            .send_event(event)
            .await
            .map(|_| ())
            .map_err(|e| CollabError::Relay(e.to_string()))
    }

    /// Say which relays this key reads (NIP-65), so a host's relay hints
    /// stay discoverable. Best effort.
    pub async fn publish_relay_list(&self) {
        let urls = self.urls.lock().await.clone();
        let tags: Vec<Tag> = urls
            .iter()
            .filter_map(|u| Tag::parse(["r", u.as_str()]).ok())
            .collect();
        if tags.is_empty() {
            return;
        }
        match EventBuilder::new(Kind::RelayList, "")
            .tags(tags)
            .finalize(&self.keys)
        {
            Ok(ev) => {
                if let Err(e) = self.client.send_event(&ev).await {
                    tracing::debug!("relay list publish failed: {e}");
                }
            }
            Err(e) => tracing::debug!("relay list build failed: {e}"),
        }
    }

    /// Everything addressed to this key, unwrapped: live wraps as they
    /// arrive and the ones a catch-up finds. A slow reader that lags is
    /// told so by the channel; the next catch-up brings what it missed.
    pub fn subscribe(&self) -> broadcast::Receiver<Incoming> {
        self.incoming.subscribe()
    }

    /// Reconcile (NIP-77, when a relay speaks it) and fetch every wrap to
    /// this key since `since` (unix seconds; `None` = everything), handing
    /// each unseen one on. Called on a timer by whoever owns the pool.
    pub async fn catch_up(&self, since: Option<u64>) {
        if self.count().await == 0 {
            return;
        }
        let filter = relay_filter(self.keys.public_key());
        if let Err(e) = self.client.sync(filter.clone()).await {
            tracing::debug!("negentropy sync unavailable ({e}); fetching instead");
        }
        let fetch_filter = match since {
            Some(t) if t > WRAP_TIMESTAMP_SLACK_SECS => {
                filter.since(Timestamp::from_secs(t - WRAP_TIMESTAMP_SLACK_SECS))
            }
            _ => filter,
        };
        match self
            .client
            .fetch_events(fetch_filter)
            .timeout(FETCH_WAIT)
            .await
        {
            Ok(events) => {
                for ev in events {
                    if ev.kind == Kind::GiftWrap {
                        self.take_wrap(&ev).await;
                    }
                }
            }
            Err(e) => {
                let relays = self.count().await;
                tracing::debug!(relays, "catch-up fetch failed across the pool: {e}")
            }
        }
    }

    /// Hand one wrap on, once.
    async fn take_wrap(&self, wrap: &Event) {
        if !self.seen.lock().await.note(wrap.id.to_hex()) {
            return;
        }
        match unwrap_incoming(&self.keys, wrap) {
            Ok(Incoming::Other) => {}
            Ok(incoming) => {
                // Nobody subscribed yet is not an error: the next catch-up
                // brings what a late reader missed.
                if self.incoming.send(incoming).is_err() {
                    tracing::trace!("a wrap arrived before anyone listened");
                }
            }
            Err(e) => tracing::debug!("cannot unwrap gift {}: {e}", wrap.id),
        }
    }

    /// Close every connection and stop hearing.
    pub async fn stop(&self) {
        if let Some(task) = self.inbound.lock().await.take() {
            task.abort();
        }
        self.client.shutdown().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::Control;
    use crate::wrap::wrap_control;
    use nostr_relay_builder::builder::RelayBuilder;
    use nostr_relay_builder::local::LocalRelay;

    async fn local_relay() -> (LocalRelay, String) {
        let relay = LocalRelay::new(RelayBuilder::default().addr("127.0.0.1".parse().unwrap()));
        relay.run().await.expect("relay run");
        let url = relay.url().await.to_string();
        (relay, url)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_wrap_published_by_one_pool_reaches_the_other_unwrapped() {
        let (_relay, url) = local_relay().await;
        let alice = Relays::start(Keys::generate(), std::slice::from_ref(&url)).await;
        let bob = Relays::start(Keys::generate(), std::slice::from_ref(&url)).await;
        let mut heard = bob.subscribe();
        let wrap = wrap_control(alice.keys(), bob.public_key(), &Control::Waiting).unwrap();
        alice.publish(&wrap).await.expect("published");
        let got = tokio::time::timeout(Duration::from_secs(10), heard.recv())
            .await
            .expect("heard in time")
            .expect("channel open");
        match got {
            Incoming::Control { sender, control } => {
                assert_eq!(sender, alice.public_key());
                assert_eq!(control, Control::Waiting);
            }
            other => panic!("unexpected: {other:?}"),
        }
        let health = bob.health().await;
        assert_eq!(health.len(), 1);
        assert!(health[0].connected, "{:?}", health[0]);
        assert_eq!(health[0].status, RelayStatusWord::Connected);
        assert_eq!(bob.connected_count().await, 1);
        bob.stop().await;
        alice.stop().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_catch_up_hands_a_wrap_on_once_and_a_relay_change_is_live() {
        let (_relay, url) = local_relay().await;
        let alice = Relays::start(Keys::generate(), std::slice::from_ref(&url)).await;
        let bob_keys = Keys::generate();
        // Bob is not listening yet: the wrap waits on the relay.
        let wrap = wrap_control(alice.keys(), bob_keys.public_key(), &Control::Leave).unwrap();
        alice.publish(&wrap).await.expect("published");
        let bob = Relays::start(bob_keys, &[]).await;
        assert_eq!(bob.count().await, 0);
        let mut heard = bob.subscribe();
        bob.set_relays(std::slice::from_ref(&url)).await;
        assert_eq!(bob.urls().await, vec![url.clone()]);
        bob.catch_up(None).await;
        bob.catch_up(None).await;
        let first = tokio::time::timeout(Duration::from_secs(10), heard.recv())
            .await
            .expect("heard in time")
            .expect("channel open");
        assert!(matches!(
            first,
            Incoming::Control {
                control: Control::Leave,
                ..
            }
        ));
        let again = tokio::time::timeout(Duration::from_millis(500), heard.recv()).await;
        assert!(again.is_err(), "the same wrap is handed on once: {again:?}");
        bob.set_relays(&[]).await;
        assert!(bob.health().await.is_empty());
        bob.stop().await;
        alice.stop().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_check_says_yes_to_a_live_relay_and_no_to_a_dead_port() {
        let (_relay, url) = local_relay().await;
        let ok = Relays::check(&url, Duration::from_secs(5)).await;
        assert!(ok.ok, "{ok:?}");
        assert!(ok.latency_ms.is_some());
        let dead = Relays::check("ws://127.0.0.1:1", Duration::from_secs(2)).await;
        assert!(!dead.ok);
        assert!(dead.error.is_some());
        let junk = Relays::check("not a url", Duration::from_secs(1)).await;
        assert!(!junk.ok);
    }

    /// Every real relay is `wss://`. With both of rustls' backends compiled
    /// in and no provider installed, the websocket stack's TLS client
    /// **panicked** on its first connection; a check of a TLS relay must
    /// answer in words whatever is behind the address. Here a plain `ws://`
    /// relay stands behind a `wss://` address on this machine — a TLS
    /// handshake that cannot succeed, and nothing leaves the loopback.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_tls_relay_that_cannot_be_reached_is_an_error_in_words_never_a_panic() {
        let (_relay, url) = local_relay().await;
        let tls = url.replacen("ws://", "wss://", 1);
        let answered = Relays::check(&tls, Duration::from_secs(3)).await;
        assert!(!answered.ok, "{answered:?}");
        assert!(answered.error.as_deref().is_some_and(|e| !e.is_empty()));
        assert!(crate::tls::crypto_provider_ready());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn opening_a_pool_gives_the_process_its_tls_provider_so_no_embedder_can_forget() {
        let pool = Relays::start(Keys::generate(), &[]).await;
        assert!(crate::tls::crypto_provider_ready());
        pool.stop().await;
    }

    #[test]
    fn a_row_says_why_a_relay_is_not_connected_and_nothing_before_there_is_a_why() {
        use RelayStatusWord as W;
        assert_eq!(relay_problem(W::Connected, 3, 3), None);
        assert_eq!(
            relay_problem(W::Off, 0, 0),
            None,
            "the wire is off: not a fault"
        );
        for untried in [W::Initialized, W::Pending, W::Connecting, W::Disconnected] {
            assert_eq!(
                relay_problem(untried, 0, 0),
                None,
                "{untried:?}: not tried yet"
            );
        }
        let never = relay_problem(W::Connecting, 4, 0).unwrap();
        assert!(
            never.starts_with("never connected in 4 attempts"),
            "{never}"
        );
        assert!(
            never.contains("never through a proxy"),
            "the one bound a person can act on: {never}"
        );
        assert!(relay_problem(W::Disconnected, 1, 0)
            .unwrap()
            .contains("1 attempt —"));
        let dropped = relay_problem(W::Disconnected, 5, 2).unwrap();
        assert!(
            dropped.contains("2 of 5") && dropped.contains("retried on its own"),
            "{dropped}"
        );
        assert!(relay_problem(W::Banned, 0, 0).unwrap().contains("banned"));
        assert!(relay_problem(W::Terminated, 2, 1)
            .unwrap()
            .contains("not retried"));
        assert!(relay_problem(W::Shutdown, 2, 1)
            .unwrap()
            .contains("not retried"));
        assert!(relay_problem(W::Sleeping, 2, 1)
            .unwrap()
            .starts_with("idle"));
    }

    #[test]
    fn an_off_row_says_so_on_the_wire_and_proves_nothing() {
        let row = RelayHealth::off("wss://relay.example");
        let json = serde_json::to_value(&row).unwrap();
        assert_eq!(json["status"], "off");
        assert_eq!(json["connected"], false);
        assert_eq!(json["attempts"], 0);
        assert!(json.get("latency_ms").is_none());
        assert!(json.get("problem").is_none(), "off is not a problem");
        let back: RelayHealth = serde_json::from_value(json).unwrap();
        assert_eq!(back, row);
        let failing = RelayHealth {
            status: RelayStatusWord::Connecting,
            attempts: 2,
            problem: relay_problem(RelayStatusWord::Connecting, 2, 0),
            ..RelayHealth::off("wss://relay.example")
        };
        let json = serde_json::to_value(&failing).unwrap();
        assert!(json["problem"]
            .as_str()
            .unwrap()
            .starts_with("never connected"));
        assert_eq!(
            serde_json::from_value::<RelayHealth>(json).unwrap(),
            failing
        );
    }

    #[test]
    fn seen_wraps_are_bounded() {
        let mut seen = SeenWraps::default();
        for i in 0..(SEEN_WRAPS + 10) {
            assert!(seen.note(format!("id{i}")));
        }
        assert!(!seen.note(format!("id{}", SEEN_WRAPS + 5)));
        assert!(seen.note("id0".into()), "the oldest was forgotten");
        assert_eq!(seen.set.len(), SEEN_WRAPS);
    }
}
