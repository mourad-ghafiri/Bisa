//! Idle -> parked session lifecycle: an adopted session is kept live for a
//! follow-up while it is touched, and parked — the live object disposed, its
//! durable row `parked` — once it idled its time to live. **Nothing revives a
//! parked session**: the next turn, or the Workflow Agent's next wake,
//! launches afresh and reads the truth again (the door that revived from a
//! resume token was called by nothing and went). The adapters' `attach` and
//! the token a session row keeps are each harness's own to resume a session
//! on its side.

use crate::registry::AgentStatus;
use crate::registry::LiveRunId;
use crate::Inner;
use bisa_harness::HarnessSession;
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

struct Slot {
    session: Option<Box<dyn HarnessSession>>,
    session_row_id: String,
    /// Bumped on every touch; a pending TTL task only parks when its epoch is
    /// still current (this is the park/revive race resolution).
    epoch: u64,
    /// How long the session idles before it parks — re-armed by every touch.
    idle_ttl: Duration,
}

/// Owns live sessions for adopted agents.
#[derive(Default)]
pub struct Lifecycle {
    slots: DashMap<LiveRunId, Arc<Mutex<Slot>>>,
}

impl Lifecycle {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adopt a finished-but-idle agent's session; parks it after `idle_ttl`
    /// unless touched or revived first.
    pub fn adopt(
        &self,
        inner: &Arc<Inner>,
        agent_id: LiveRunId,
        session: Box<dyn HarnessSession>,
        session_row_id: String,
        idle_ttl: Duration,
    ) {
        let slot = Arc::new(Mutex::new(Slot {
            session: Some(session),
            session_row_id,
            epoch: 0,
            idle_ttl,
        }));
        self.slots.insert(agent_id, Arc::clone(&slot));
        self.arm_ttl(inner, agent_id, slot, 0, idle_ttl);
    }

    fn arm_ttl(
        &self,
        inner: &Arc<Inner>,
        agent_id: LiveRunId,
        slot: Arc<Mutex<Slot>>,
        epoch: u64,
        idle_ttl: Duration,
    ) {
        let inner = Arc::clone(inner);
        tokio::spawn(async move {
            tokio::time::sleep(idle_ttl).await;
            let mut guard = slot.lock().await;
            if guard.epoch != epoch || guard.session.is_none() {
                return; // touched or already parked/revived — TTL is stale
            }
            let session = guard.session.take().expect("checked above");
            let row_id = guard.session_row_id.clone();
            drop(guard);
            crate::warn_on_err(session.dispose().await, "disposing an idle session");
            crate::sessions::ended(&inner, &row_id);
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            crate::warn_on_err(inner.ws.park_session(&row_id, now), "parking a session");
            if let Some(agent) = inner.registry.get(agent_id) {
                crate::debug_on_err(
                    inner.registry.mutate(agent_id, agent.generation, |a| {
                        a.status = AgentStatus::Parked
                    }),
                    "parking a run",
                );
            }
            inner.presence.parked(&inner, agent_id);
            inner.emit(crate::events::EngineEvent::global(
                crate::events::EnginePayload::Session {
                    event: bisa_harness::SessionEvent::Lifecycle(
                        bisa_harness::LifecycleEvent::Parked,
                    ),
                },
            ));
        });
    }

    /// Deliver a follow-up message into the agent's live session, if it is
    /// live and the session accepts follow-ups. Returns whether delivery
    /// happened (false = caller should wake fresh instead). A delivered
    /// follow-up is a touch: the idle clock starts again from it, so a
    /// session a person keeps talking to parks once they stop, never never.
    pub async fn follow_up(&self, inner: &Arc<Inner>, agent_id: LiveRunId, text: &str) -> bool {
        let Some(slot) = self.slots.get(&agent_id).map(|s| Arc::clone(&s)) else {
            return false;
        };
        let mut guard = slot.lock().await;
        guard.epoch += 1; // touched: invalidate any pending TTL
        let delivered = match &guard.session {
            Some(session) => session
                .follow_up(bisa_harness::Steer::from(text.to_string()))
                .await
                .is_ok(),
            None => false,
        };
        let (epoch, idle_ttl) = (guard.epoch, guard.idle_ttl);
        drop(guard);
        if delivered {
            self.arm_ttl(inner, agent_id, slot, epoch, idle_ttl);
        }
        delivered
    }

    /// Abort an adopted session and let go of it for good — what stopping
    /// its row does: the slot leaves, the harness is aborted, then disposed,
    /// and the durable row ended. Answers whether a slot was held.
    pub async fn abort(&self, inner: &Inner, agent_id: LiveRunId) -> bool {
        let Some((_, slot)) = self.slots.remove(&agent_id) else {
            return false;
        };
        let mut guard = slot.lock().await;
        guard.epoch += 1; // a pending TTL finds its epoch stale
        if let Some(session) = guard.session.take() {
            crate::warn_on_err(session.abort().await, "aborting a stopped session");
            crate::warn_on_err(session.dispose().await, "disposing a stopped session");
        }
        crate::sessions::ended(inner, &guard.session_row_id);
        true
    }

    /// Whether a slot holds the agent's session — adopted, not yet parked.
    pub fn holds(&self, agent_id: LiveRunId) -> bool {
        self.slots.contains_key(&agent_id)
    }

    /// Every adopted session aborted and let go of — the engine's own stop.
    pub async fn abort_all(&self, inner: &Inner) {
        let held: Vec<LiveRunId> = self.slots.iter().map(|s| *s.key()).collect();
        for agent_id in held {
            self.abort(inner, agent_id).await;
        }
    }

    /// Let go of an adopted session once it idled its time to live — the
    /// park: the slot leaves, the session is disposed and its durable row
    /// ended. Answers whether a slot was held.
    // LCOV_EXCL_START: release runs when a stop lands as a design session is being kept for a follow-up, a race no test can stage
    pub async fn release(&self, inner: &Inner, agent_id: LiveRunId) -> bool {
        let Some((_, slot)) = self.slots.remove(&agent_id) else {
            return false;
        };
        let mut guard = slot.lock().await;
        guard.epoch += 1; // a pending TTL finds its epoch stale
        if let Some(session) = guard.session.take() {
            crate::warn_on_err(session.dispose().await, "disposing a stopped session");
        }
        crate::sessions::ended(inner, &guard.session_row_id);
        true
    }
    // LCOV_EXCL_STOP

    /// Whether the agent currently holds a live session object.
    pub async fn is_live(&self, agent_id: LiveRunId) -> bool {
        match self.slots.get(&agent_id).map(|s| Arc::clone(&s)) {
            Some(slot) => slot.lock().await.session.is_some(),
            None => false,
        }
    }
}
