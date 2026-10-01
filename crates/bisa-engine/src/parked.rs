//! A **desk** of requests parked for the desktop: what the browser bridge
//! (ide/18) and the drawing bridge (19) share below their own words.
//!
//! An agent's tool call reaches the engine, which has no browser and no
//! canvas of its own; the desktop has both. So the op parks the request
//! here, puts it on the bus, and waits; the desktop hears it, performs it,
//! and answers over the node; the waiting op returns what came back. A
//! desk knows when a desktop was last heard from — it read the list, or it
//! answered — so an op can say *nobody home* at once rather than after the
//! answer timeout, and a request nobody answers is forgotten with a word
//! the caller chooses.

use dashmap::DashMap;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::watch;

/// A request a desktop performs: it carries its own id and knows its word
/// for the log.
pub trait Parked: Clone + Serialize + Send + Sync + 'static {
    /// What the desktop answers it with.
    type Answer: Clone + Send + Sync + 'static;
    fn id(&self) -> &str;
    /// A word for the log line — the action, say.
    fn label(&self) -> String;
}

struct Slot<P: Parked> {
    pending: P,
    tx: watch::Sender<Option<P::Answer>>,
    /// When the request was parked, for the log's `elapsed_ms`.
    asked: Instant,
}

/// The parked requests, by id — and when a desktop was last heard from.
pub struct Desk<P: Parked> {
    slots: DashMap<String, Arc<Slot<P>>>,
    /// When a desktop last read the list or answered a request, in seconds;
    /// zero until one has.
    desktop_seen: AtomicU64,
    /// How recently a desktop must have been heard from to count as home.
    presence: Duration,
    /// The word the log calls these requests.
    what: &'static str,
}

impl<P: Parked> Desk<P> {
    pub fn new(what: &'static str, presence: Duration) -> Self {
        Self {
            slots: DashMap::new(),
            desktop_seen: AtomicU64::new(0),
            presence,
            what,
        }
    }

    /// A desktop is here: it read the list or answered a request just now.
    fn desktop_seen(&self) {
        self.desktop_seen.store(now_secs(), Ordering::Relaxed);
    }

    /// Whether a desktop was heard from within the presence window — what
    /// decides between parking a request and saying nobody is home.
    pub fn desktop_present(&self) -> bool {
        let seen = self.desktop_seen.load(Ordering::Relaxed);
        seen != 0 && now_secs().saturating_sub(seen) <= self.presence.as_secs()
    }

    /// A fresh id for a request: sortable, so a list reads oldest first.
    pub fn new_id() -> String {
        ulid::Ulid::from_datetime(SystemTime::now()).to_string()
    }

    /// Park a request; the receiver resolves when the desktop answers. The
    /// caller puts the request on the bus in its own words.
    pub fn park(&self, pending: P) -> watch::Receiver<Option<P::Answer>> {
        let (tx, rx) = watch::channel(None);
        let id = pending.id().to_string();
        tracing::debug!(id, what = self.what, action = %pending.label(), "request parked for the desktop");
        self.slots.insert(
            id,
            Arc::new(Slot {
                pending,
                tx,
                asked: Instant::now(),
            }),
        );
        rx
    }

    /// The desktop's answer; `false` when nothing waits under that id.
    pub fn answer(&self, id: &str, answer: P::Answer) -> bool {
        self.desktop_seen();
        let Some((_, slot)) = self.slots.remove(id) else {
            tracing::debug!(
                id,
                what = self.what,
                "an answer for a request nobody waits on"
            );
            return false;
        };
        tracing::debug!(
            id,
            what = self.what,
            action = %slot.pending.label(),
            elapsed_ms = slot.asked.elapsed().as_millis() as u64,
            "request answered"
        );
        // No receiver means the op gave up waiting, which is not a fault.
        if slot.tx.send(Some(answer)).is_err() {
            tracing::debug!(id, what = self.what, "request answered with no waiter");
        }
        true
    }

    /// Every request still waiting, oldest first — what a desktop that just
    /// opened reads to catch up.
    pub fn pending(&self) -> Vec<P> {
        self.desktop_seen();
        let mut out: Vec<_> = self.slots.iter().map(|s| s.pending.clone()).collect();
        out.sort_by(|a, b| a.id().cmp(b.id()));
        out
    }

    /// Wait for the answer, or say the desktop was silent after `patience`;
    /// a request that times out — or whose slot went away under the waiter
    /// — is forgotten and answered with `silent()`. A request parked here
    /// had a desktop to answer it, so a silence is never *nobody home*.
    pub async fn wait_for(
        &self,
        id: &str,
        mut rx: watch::Receiver<Option<P::Answer>>,
        patience: Duration,
        silent: impl FnOnce() -> P::Answer,
    ) -> P::Answer {
        let waited = tokio::time::timeout(patience, async {
            loop {
                if let Some(r) = rx.borrow().clone() {
                    return Some(r);
                }
                if rx.changed().await.is_err() {
                    return None;
                }
            }
        })
        .await;
        match waited {
            Ok(Some(r)) => r,
            Ok(None) => {
                let action = self.slots.remove(id).map(|(_, s)| s.pending.label());
                tracing::warn!(id, what = self.what, ?action, "request lost its waiter");
                silent()
            }
            Err(_) => {
                let (action, elapsed_ms) = self
                    .slots
                    .remove(id)
                    .map(|(_, s)| {
                        (
                            Some(s.pending.label()),
                            s.asked.elapsed().as_millis() as u64,
                        )
                    })
                    .unwrap_or((None, patience.as_millis() as u64));
                tracing::warn!(
                    id,
                    what = self.what,
                    ?action,
                    elapsed_ms,
                    "request timed out"
                );
                silent()
            }
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, Serialize)]
    struct Ping {
        id: String,
    }

    impl Parked for Ping {
        type Answer = &'static str;
        fn id(&self) -> &str {
            &self.id
        }
        fn label(&self) -> String {
            "ping".into()
        }
    }

    #[tokio::test]
    async fn a_desk_knows_who_is_home_and_answers_what_the_desktop_said() {
        let desk: Desk<Ping> = Desk::new("ping", Duration::from_secs(45));
        assert!(!desk.desktop_present(), "nobody has read the list");
        assert!(desk.pending().is_empty());
        assert!(desk.desktop_present(), "reading the list is being home");
        let rx = desk.park(Ping { id: "01".into() });
        let rx2 = desk.park(Ping { id: "00".into() });
        assert_eq!(
            desk.pending()
                .iter()
                .map(|p| p.id.as_str())
                .collect::<Vec<_>>(),
            ["00", "01"],
            "oldest first"
        );
        assert!(desk.answer("01", "pong"));
        assert!(!desk.answer("01", "again"), "answered once");
        assert_eq!(
            desk.wait_for("01", rx, Duration::from_secs(1), || "silent")
                .await,
            "pong"
        );
        assert_eq!(
            desk.wait_for("00", rx2, Duration::from_millis(20), || "silent")
                .await,
            "silent",
            "a request nobody answers is said to be silent"
        );
        assert!(
            desk.pending().is_empty(),
            "a timed-out request is forgotten"
        );
    }
}
