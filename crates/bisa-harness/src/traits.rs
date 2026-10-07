//! The two core traits, plus the broadcast-backed subscribe helper.

use crate::error::HarnessError;
use crate::event::{InputAnswer, LifecycleEvent, Outcome, SessionEvent};
use crate::types::{
    Effort, ModelPlan, ProbeResult, PromptInput, ResumeToken, SessionSnapshot, SessionSpec, Steer,
};
use bisa_core::sync::Locked;
use bisa_core::HarnessCaps;
use futures::stream::BoxStream;
use futures::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

pub type BoxEventStream = BoxStream<'static, SessionEvent>;

/// How to find, probe, launch, and re-attach one kind of harness.
#[async_trait::async_trait]
pub trait HarnessAdapter: Send + Sync {
    /// Stable id ("claude-code", "codex", "acp:goose", ...).
    fn id(&self) -> &str;

    fn display_name(&self) -> &str;

    fn caps(&self) -> HarnessCaps;

    /// Host-level availability: binary on PATH, version, auth state.
    /// `launch` may still fail `Unavailable` for a specific spec.
    async fn probe(&self) -> ProbeResult;

    /// Models this harness can run, discovered from the harness itself
    /// (CLI probe, protocol metadata, or a curated list). Empty = unknown —
    /// callers must keep free-text model entry available.
    async fn models(&self) -> Vec<crate::types::ModelInfo> {
        Vec::new()
    }

    /// The efforts this harness takes for `model`, lowest first — what the
    /// engine clamps a resolved level to before it launches. `None` asks
    /// about the harness's own default model, and an id the harness does not
    /// list still gets an answer: the levels it is safe to send a model
    /// nobody here knows. Synchronous, and it spawns nothing: the answer is
    /// the adapter's own table, read from the harness's documentation.
    ///
    /// Empty means nothing is sent, whatever the capability flag says: the
    /// harness has no control, or this model takes no effort. An adapter
    /// whose harness fits a level itself — pi to its model, an ACP agent
    /// among what its session advertises — lists every level it may be
    /// asked for.
    fn efforts(&self, model: Option<&str>) -> Vec<Effort> {
        let _ = model;
        Vec::new()
    }

    /// The model plan this harness recommends for an agent — what the setup
    /// gate's fix writes on an agent that has none. `None` is the honest
    /// answer for a harness with no opinion, and the fix then takes the
    /// models it lists first.
    fn recommended_plan(&self) -> Option<ModelPlan> {
        None
    }

    /// The model this harness recommends for judging — what the setup
    /// gate's fix writes for the Decision-Making Agent. `None` as above.
    fn recommended_judge(&self) -> Option<String> {
        None
    }

    /// What this harness's **account** has left — its usage limits, read
    /// from the harness's own source (its provider's usage endpoint with its
    /// own sign-in, its app server, its CLI). The default is the honest
    /// answer for a harness that exposes none, in its own name; an adapter
    /// that reads one declares [`HarnessCaps::USAGE_REPORTING`]. A report
    /// never carries a credential.
    async fn usage(&self) -> crate::usage::UsageState {
        crate::usage::UsageState::unsupported(self.display_name())
    }

    /// How to run this harness in a terminal a person is sitting at, or `None`
    /// when it has no such form.
    ///
    /// **`None` is the safe answer and therefore the default.** `launch` above
    /// starts a *protocol* session — JSON on stdio, driven by the engine — and
    /// an adapter whose whole job is to put a binary into that mode (every
    /// `acp:*`) or which has no local process at all (`a2a:*`) has nothing
    /// interactive to offer. Returning its protocol command here would hand
    /// somebody a PTY full of JSON-RPC frames.
    ///
    /// An adapter that does implement it must name the same program it probes,
    /// or the menu offers something the availability check was never about.
    fn interactive(&self) -> Option<crate::types::InteractiveLaunch> {
        None
    }

    /// How one interactive session of this harness reports what it is doing
    /// — its own hooks, an extension, its event server — as a plan the engine
    /// materialises. The default is the empty plan: the session opens as a
    /// plain terminal and is never a roster row. Only meaningful for an
    /// adapter with an [`Self::interactive`] form.
    fn interactive_reporting(
        &self,
        ctx: &crate::types::ReportingContext,
    ) -> crate::types::ReportingPlan {
        let _ = ctx;
        crate::types::ReportingPlan::default()
    }

    /// What one payload the harness reported means, in the engine's
    /// vocabulary. Pure: the reporter CLI, the engine's event puller and a
    /// test all call it. Empty when the payload says nothing a person reads.
    fn translate_report(&self, payload: &serde_json::Value) -> Vec<SessionEvent> {
        let _ = payload;
        Vec::new()
    }

    async fn launch(&self, spec: SessionSpec) -> Result<Box<dyn HarnessSession>, HarnessError>;

    /// Re-attach to a parked/suspended session. Requires
    /// [`HarnessCaps::RESUME`]; adapters without it return
    /// `Err(NotSupported("resume"))`.
    async fn attach(&self, token: &ResumeToken) -> Result<Box<dyn HarnessSession>, HarnessError>;
}

/// One running (or resumable) session.
///
/// Implementations must never panic the engine: internal failures surface as
/// `Lifecycle(Ended { outcome: Failed, .. })` on the event stream, and the
/// session moves to `Phase::Ended`.
#[async_trait::async_trait]
pub trait HarnessSession: Send + Sync {
    /// Authoritative state with a monotonic revision.
    fn snapshot(&self) -> SessionSnapshot;

    fn phase(&self) -> crate::types::Phase;

    /// Start a turn. Errors with [`HarnessError::Busy`] while a turn is
    /// streaming — use `steer`/`follow_up` instead.
    async fn prompt(&self, input: PromptInput) -> Result<(), HarnessError>;

    /// Inject input into the running turn (before the next model call).
    /// Requires [`HarnessCaps::STEER`].
    async fn steer(&self, msg: Steer) -> Result<(), HarnessError>;

    /// Queue input delivered when the agent would otherwise stop.
    /// Requires [`HarnessCaps::FOLLOW_UP`].
    async fn follow_up(&self, msg: Steer) -> Result<(), HarnessError>;

    async fn abort(&self) -> Result<(), HarnessError>;

    /// Answer an [`crate::InputRequest`] the session raised and stopped on.
    /// Requires [`HarnessCaps::INPUT_REQUESTS`]; the default refuses, and a
    /// session without the capability never emits the request.
    async fn answer(&self, request_id: &str, answer: InputAnswer) -> Result<(), HarnessError> {
        let _ = (request_id, answer);
        Err(HarnessError::NotSupported("answer"))
    }

    /// Subscribe to the event stream. Multiple subscribers allowed; each gets
    /// every event from subscription time onward.
    fn subscribe(&self) -> BoxEventStream;

    /// Handle for re-attaching later, when the adapter supports RESUME.
    fn resume_token(&self) -> Option<ResumeToken>;

    /// Release resources. Does not abort a running turn first — callers
    /// abort explicitly when that is the goal.
    async fn dispose(self: Box<Self>) -> Result<(), HarnessError>;
}

/// Broadcast-backed event fan-out shared by session implementations.
///
/// Lag handling: a slow subscriber that overflows the ring misses events and
/// resumes at the live edge — correct under the "snapshots are authoritative,
/// events are advisory" rule.
///
/// **Two events are not advisory.** The session's **terminal end** is how
/// it says why it is over — a model it could not run, an agent that went
/// away — and a session may end before anybody listens: an agent that
/// refuses the model during its handshake is done before the engine has
/// subscribed. So the first terminal end is kept, and a subscriber that
/// comes after it is handed it first. Without that the engine waits on a
/// stream that will never say anything, and a model wall — which should
/// walk the agent's plan to its next model — reads as a session that hung.
/// The **process it announced** (`ProcessStarted`) is how a driver learns
/// the pid a boot after a crash has to end, and a long-lived adapter
/// announces it from its own task at launch, before the engine subscribes:
/// so the last one is kept too, and handed over first.
#[derive(Clone)]
pub struct EventBroadcaster {
    tx: tokio::sync::broadcast::Sender<SessionEvent>,
    /// What a late subscriber must still hear. Read and written under one
    /// lock with the send and the subscribe, so every subscriber hears each
    /// exactly once: from the channel when it was there first, from here
    /// when it was not.
    kept: std::sync::Arc<std::sync::Mutex<Kept>>,
}

/// The events kept for whoever subscribes after they were said.
#[derive(Clone, Default)]
struct Kept {
    /// The last process the session announced — a one-shot adapter announces
    /// one per turn, and the latest is the one alive.
    process: Option<SessionEvent>,
    /// The first terminal end.
    ended: Option<SessionEvent>,
}

impl Kept {
    /// What a late subscriber hears first, in the order it was said.
    fn said(&self) -> impl Iterator<Item = SessionEvent> {
        self.process.clone().into_iter().chain(self.ended.clone())
    }
}

impl EventBroadcaster {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = tokio::sync::broadcast::channel(capacity);
        Self {
            tx,
            kept: Default::default(),
        }
    }

    /// Send; nobody listening is not an error (events are advisory) — a
    /// terminal end and the last process announced are kept for whoever
    /// listens later.
    pub fn emit(&self, event: SessionEvent) {
        let mut kept = self.kept.locked();
        match &event {
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                is_terminal: true, ..
            }) if kept.ended.is_none() => kept.ended = Some(event.clone()),
            SessionEvent::Lifecycle(LifecycleEvent::ProcessStarted { .. }) => {
                kept.process = Some(event.clone())
            }
            _ => {}
        }
        if self.tx.send(event).is_err() {
            tracing::trace!("no session listens; events are advisory");
        }
    }

    /// Emit a terminal failure — the never-panic contract's last resort.
    pub fn emit_failure(&self, error: impl Into<String>) {
        self.emit(SessionEvent::Lifecycle(LifecycleEvent::Ended {
            outcome: Outcome::Failed {
                error: error.into(),
            },
            is_terminal: true,
        }));
    }

    /// Every event from now on — and first, what the session said before
    /// anybody was here: the process it announced, and its terminal end when
    /// it is already over.
    pub fn subscribe(&self) -> BoxEventStream {
        let (live, said) = {
            let kept = self.kept.locked();
            (self.tx.subscribe(), kept.said().collect::<Vec<_>>())
        };
        futures::stream::iter(said)
            .chain(
                BroadcastStream::new(live)
                    // A lagged subscriber skips to the live edge; snapshot carries truth.
                    .filter_map(|item| async move { item.ok() }),
            )
            .boxed()
    }
}

impl Default for EventBroadcaster {
    fn default() -> Self {
        Self::new(1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::ProgressEvent;

    fn wall() -> SessionEvent {
        SessionEvent::Lifecycle(LifecycleEvent::Ended {
            outcome: Outcome::ModelUnavailable {
                model: "a-model".into(),
                reason: "not offered by this session".into(),
                retry_after: None,
            },
            is_terminal: true,
        })
    }

    fn is_wall(event: &SessionEvent) -> bool {
        matches!(
            event,
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: Outcome::ModelUnavailable { .. },
                is_terminal: true,
            })
        )
    }

    /// What a stream holds right now, without waiting for more.
    async fn heard(stream: &mut BoxEventStream) -> Vec<SessionEvent> {
        let mut all = Vec::new();
        while let Ok(Some(event)) =
            tokio::time::timeout(std::time::Duration::from_millis(50), stream.next()).await
        {
            all.push(event);
        }
        all
    }

    /// A session that ended before anybody listened still says how: the one
    /// who comes after hears the end, once, and nothing that was advisory.
    #[tokio::test]
    async fn a_subscriber_that_comes_after_the_end_still_hears_it_once() {
        let session = EventBroadcaster::default();
        session.emit(SessionEvent::Lifecycle(LifecycleEvent::Started));
        session.emit(SessionEvent::Progress(ProgressEvent::TurnStarted));
        session.emit(wall());

        let mut late = session.subscribe();
        let all = heard(&mut late).await;
        assert_eq!(all.len(), 1, "the end and nothing else: {all:?}");
        assert!(is_wall(&all[0]), "{all:?}");
    }

    /// A process the session announced before anybody listened is still
    /// announced: a driver that subscribes after the launch records the pid
    /// a boot after a crash would have to end — the last one, once, and
    /// before the end. One who was there when it was announced hears it from
    /// the session and not again from what is kept.
    #[tokio::test]
    async fn a_process_started_before_anybody_listened_is_replayed_once_before_the_end() {
        let session = EventBroadcaster::default();
        session.emit(SessionEvent::Lifecycle(LifecycleEvent::ProcessStarted {
            pid: Some(41),
        }));
        session.emit(SessionEvent::Lifecycle(LifecycleEvent::ProcessStarted {
            pid: Some(42),
        }));
        session.emit(SessionEvent::Progress(ProgressEvent::TurnStarted));
        session.emit(wall());
        let mut late = session.subscribe();
        let all = heard(&mut late).await;
        assert_eq!(all.len(), 2, "the last process, then the end: {all:?}");
        assert!(
            matches!(
                &all[0],
                SessionEvent::Lifecycle(LifecycleEvent::ProcessStarted { pid: Some(42) })
            ),
            "{all:?}"
        );
        assert!(is_wall(&all[1]), "{all:?}");

        let session = EventBroadcaster::default();
        let mut early = session.subscribe();
        session.emit(SessionEvent::Lifecycle(LifecycleEvent::ProcessStarted {
            pid: Some(7),
        }));
        let all = heard(&mut early).await;
        assert_eq!(all.len(), 1, "heard once, from the session: {all:?}");
    }

    /// One who was there when it ended hears it from the session, once — not
    /// a second time from what is kept.
    #[tokio::test]
    async fn a_subscriber_that_was_there_hears_the_end_once() {
        let session = EventBroadcaster::default();
        let mut early = session.subscribe();
        session.emit(SessionEvent::Lifecycle(LifecycleEvent::Started));
        session.emit(wall());

        let all = heard(&mut early).await;
        assert_eq!(all.len(), 2, "{all:?}");
        assert!(matches!(
            all[0],
            SessionEvent::Lifecycle(LifecycleEvent::Started)
        ));
        assert!(is_wall(&all[1]), "{all:?}");
    }

    /// The end that is kept is the first the session said — a stop asked of
    /// a session already over changes nothing about why it ended — and a
    /// turn's end that is not the session's is kept for nobody.
    #[tokio::test]
    async fn what_is_kept_is_the_first_terminal_end_and_no_turns_end() {
        let session = EventBroadcaster::default();
        session.emit(SessionEvent::Lifecycle(LifecycleEvent::Ended {
            outcome: Outcome::Completed,
            is_terminal: false,
        }));
        assert!(
            heard(&mut session.subscribe()).await.is_empty(),
            "a turn that ended is not a session that ended"
        );

        session.emit(wall());
        session.emit(SessionEvent::Lifecycle(LifecycleEvent::Ended {
            outcome: Outcome::Aborted,
            is_terminal: true,
        }));
        let all = heard(&mut session.subscribe()).await;
        assert_eq!(all.len(), 1, "{all:?}");
        assert!(
            is_wall(&all[0]),
            "why it ended, not the stop after: {all:?}"
        );
    }
}
