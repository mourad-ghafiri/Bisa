//! Conversational agents: talk to an agent and it answers.
//!
//! A message stream is a channel, a goal's thread or a **conversation** — a
//! saved exchange with an origin ([`bisa_core::Conversation`]). An agent's
//! turn in any of them is a *role over the stream*, exactly as the guided
//! agent is a role over a goal: its memory is the messages themselves, so a
//! fresh session reconstructs its context by reading the transcript — the
//! last [`TRANSCRIPT_WINDOW`] messages; the harness holds the rest as its
//! own context and compacts it itself, the platform summarises nothing. A
//! live session is kept for follow-up turns (fast, keeps the harness's own
//! context), and disposed once idle — the next message simply re-reads truth.
//!
//! The turn is a session of kind `conversation` on the roster, reached only
//! through what it answers in: it names its conversation, and the checkout
//! it runs in never draws it as a row of its own.
//!
//! The reply is whatever the agent says at the end of its turn: this module
//! accumulates the session's text and posts it into the scope **signed by
//! the agent's own key** (NIP-OA attested), with the thinking that led to it
//! beside the words (`MessageBody::Post { thinking }`). While the turn runs,
//! every word and thought the harness writes streams to the bus as
//! `AgentStreamed` — the deltas since the last frame, coalesced — and the
//! turn in flight is readable (`live_turns`) by a reader that joins late.
//! An agent that instead spoke via the `post_message` tool is not
//! double-posted.
//!
//! Two rules decide who wakes, and both live in [`dispatch`]:
//!
//! > **An unaddressed message is addressed to the platform.**
//! >
//! > **Only a core agent's message may wake another agent, and never
//! > itself.** The General Agent may wake anyone, the Workflow Agent
//! > included; the Workflow Agent may wake anyone but the General Agent.
//!
//! Together they make a message that named nobody reach the one agent that is
//! in every room, and bound what that agent can start: human → General Agent
//! → Workflow Agent or a worker → stop.

use crate::events::{EngineEvent, EnginePayload};
use crate::executor::{self, Attempts};
use crate::registry::{AgentRef, AgentStatus, LiveRunId, SessionKind};
use crate::{debug_on_err, warn_on_err, Inner};
use bisa_core::{
    Agent, AgentId, ChannelId, ChannelKind, Conversation, ConversationId, ConversationOrigin,
    GoalId, MessageBody, RespondPolicy, SessionId, WorkspaceMember,
};
use bisa_harness::{
    HarnessSession, LifecycleEvent, Outcome, ProgressEvent, SessionEvent, SessionSpec,
};
use bisa_store::{PostOrigin, SessionRow, SessionStatus, StoreEvent};
use dashmap::DashMap;
use futures::StreamExt;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

/// How much of the conversation a fresh session is given.
const TRANSCRIPT_WINDOW: usize = 50;

pub const CONVERSATION_FRAMING: &str = "\
You are in a live conversation inside this workspace. Everything below is the \
recent transcript; the last message is what you are answering.

How replying works: whatever you say at the end of your turn is posted into \
the conversation as your message. Do not call post_message for your own \
answer (that tool is for speaking into a *different* conversation). Be brief \
and human — this is chat, not a report.

You have your harness's full tools. If you are asked to do something, do it, \
then say what you did. If a request deserves tracked work, call spawn_sub_goal \
and mention that you started it; the Workflow Agent proposes how it runs.

Where what you make goes: your working directory is your own scratch folder. \
It is shared with every other conversation you have, nothing ever commits it, \
and it is not a repository — do not run git init in it. An answer, an \
analysis or a page you post as an artifact belongs here. Files that must be \
kept belong in a project: get_goal lists this goal's projects with the \
absolute path of each; work inside one of those paths. When it lists none and \
the person asked for files, create_project makes one — the only way a project \
is ever made — attaches it to this goal and initialises it as a git \
repository. In a channel or a direct message there is no goal to attach a \
project to, so there your scratch folder is all there is — say plainly that \
what you produced is untracked, and offer to move it into a project once \
there is a goal for it.

A message may carry files. Each one is listed under it with an absolute path — \
read it with your own tools. Images are also handed to you directly when your \
harness can take them, so you may already be looking at one.

When you make something for the person to look at — a page, a chart, a \
diagram, a report, a sheet, a deck, an image — post it as an artifact: \
post_message with artifacts [{path, title}], the path in your scratch folder, \
this checkout or the goal's scratch. It renders live where they read, titled; \
post a revision under the same title and they see the versions. A file handed \
over as a file goes in attachments instead.";

/// The framing as a session reads it: the transcript's rules, then what every
/// session is told about the embedded browser (`bisa_core::browser`), so
/// a conversation and a work item read the same sentence.
pub fn conversation_framing() -> String {
    format!(
        "{CONVERSATION_FRAMING}\n\n{}\n\n{}",
        bisa_core::browser::BROWSER_NOTE,
        bisa_core::draw::DRAW_NOTE
    )
}

/// How often a turn's words and thinking go out while it runs: the pump
/// gathers deltas and emits one `AgentStreamed` frame per interval — a
/// timeline reads a stream, the bus does not carry every token.
const STREAM_FLUSH: Duration = Duration::from_millis(120);

/// The most of one turn's words a conversation keeps: eight messages' worth.
/// What a harness says past it is not gathered — a turn that never stops
/// talking costs the node a bounded memory — and the person is told the
/// reply was cut.
pub const MAX_REPLY_BYTES: usize = 8 * bisa_core::MAX_TEXT_BYTES;

/// Words gathered into a turn's, up to the most a reply keeps. Answers
/// whether any were left out.
fn gather_words(kept: &mut String, more: &str) -> bool {
    let room = MAX_REPLY_BYTES.saturating_sub(kept.len());
    if more.len() <= room {
        kept.push_str(more);
        return false;
    }
    let mut end = room;
    while !more.is_char_boundary(end) {
        end -= 1;
    }
    kept.push_str(&more[..end]);
    true
}

/// Thinking gathered into a turn's: its tail, never more than twice what a
/// reply keeps of it — the end is what led to the words.
fn gather_thinking(kept: &mut String, more: &str) {
    kept.push_str(more);
    if kept.len() > 2 * bisa_core::MAX_THINKING_BYTES {
        *kept = keep_tail(kept, bisa_core::MAX_THINKING_BYTES).to_string();
    }
}

/// What a turn has said so far, as its pump keeps it for the turn's end.
#[derive(Default)]
struct Said {
    words: String,
    thinking: String,
    /// Words were said past the most a reply keeps, and left out.
    cut: bool,
}

impl Said {
    fn clear(&mut self) {
        self.words.clear();
        self.thinking.clear();
        self.cut = false;
    }

    fn hear(&mut self, thinking: bool, text: &str) {
        if thinking {
            gather_thinking(&mut self.thinking, text);
        } else {
            self.cut |= gather_words(&mut self.words, text);
        }
    }
}

/// A turn in flight: what the agent has said and thought so far, kept per
/// `(scope, agent)` for a reader that joins mid-turn — within the bounds the
/// reply itself is kept to.
#[derive(Clone, Debug, Default)]
pub struct LiveTurn {
    pub text: String,
    pub thinking: String,
    /// Unix seconds the turn began.
    pub since: u64,
    /// The tool the agent runs right now — `Read src/app.ts` — while its
    /// words wait; `None` between tools. One line for the timeline's live row,
    /// never a transcript (ide/09).
    pub working: Option<String>,
}

/// The deltas gathered since the last frame, when that was, which kind the
/// last delta was, and whether the tool line moved since the last frame.
struct Streamer {
    text: String,
    thinking: String,
    last: std::time::Instant,
    /// `Some(true)` after a thinking delta, `Some(false)` after words —
    /// a switch is a frame boundary of its own.
    last_kind: Option<bool>,
    /// The tool line changed since the last frame: news even with no words.
    working_moved: bool,
}

/// One frame's worth: the deltas since the last, and whether it is due.
struct Gathered {
    text: String,
    thinking: String,
}

impl Streamer {
    fn new() -> Self {
        Self {
            text: String::new(),
            thinking: String::new(),
            last: std::time::Instant::now(),
            last_kind: None,
            working_moved: false,
        }
    }

    fn reset(&mut self) {
        self.text.clear();
        self.thinking.clear();
        self.last = std::time::Instant::now();
        self.last_kind = None;
        self.working_moved = false;
    }

    /// An interval has passed since the last frame.
    fn due(&self) -> bool {
        self.last.elapsed() >= STREAM_FLUSH
    }

    /// Something waits for a frame: words or thinking gathered, or a tool
    /// line that moved. What arms the pump's timer — a delta followed by
    /// silence (a harness thinking, a tool running, a turn that stops
    /// without saying so) goes out an interval later, not when the next
    /// event happens to arrive.
    fn pending(&self) -> bool {
        !self.text.is_empty() || !self.thinking.is_empty() || self.working_moved
    }

    /// When the frame in gathering is due, on tokio's clock.
    fn deadline(&self) -> tokio::time::Instant {
        tokio::time::Instant::from_std(self.last + STREAM_FLUSH)
    }

    /// Whether a delta of this kind switches kinds — thinking to words or
    /// back. Thinking to words is the moment a timeline folds the thinking:
    /// what gathered goes out *before* the new kind is pushed, so the words
    /// ride a frame of their own and the fold lands with them, not a beat
    /// later.
    fn switches(&self, thinking: bool) -> bool {
        self.last_kind.is_some_and(|was| was != thinking)
    }

    /// One delta gathered. Answers whether a frame is due now: the interval
    /// passed since the last.
    fn push(&mut self, thinking: bool, text: &str) -> bool {
        self.last_kind = Some(thinking);
        if thinking {
            self.thinking.push_str(text);
        } else {
            self.text.push_str(text);
        }
        self.due()
    }

    /// The tool line moved: the next frame goes out even with nothing said.
    fn working_moved(&mut self) {
        self.working_moved = true;
    }

    /// What gathered, if anything — and the clock restarts. A moved tool
    /// line alone is something.
    fn take(&mut self) -> Option<Gathered> {
        self.last = std::time::Instant::now();
        let moved = std::mem::take(&mut self.working_moved);
        if self.text.is_empty() && self.thinking.is_empty() && !moved {
            return None;
        }
        Some(Gathered {
            text: std::mem::take(&mut self.text),
            thinking: std::mem::take(&mut self.thinking),
        })
    }
}

/// The last `max` bytes of `s`, on a character boundary — what is kept of
/// a long thinking: the end is what led to the words.
fn keep_tail(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut start = s.len() - max;
    while !s.is_char_boundary(start) {
        start += 1;
    }
    &s[start..]
}

/// What a message scope is, resolved once per decision: a channel (which
/// `dispatch` reads on its own), a goal's thread, or a conversation with its
/// origin — the checkout its turns run in, the goal it is about.
#[derive(Clone, Debug, Default)]
pub(crate) struct ScopeFacts {
    pub conversation: Option<Conversation>,
    /// The goal the scope is about: a goal's thread, or a conversation with a
    /// goal origin. What a permission escalates to and `get_goal` answers.
    pub goal: Option<GoalId>,
    /// The checkout a turn runs in, with its project and the titles of the
    /// goals the project is attached to: a conversation about a workstream
    /// or a project. Everything else runs in the agent's scratch.
    pub checkout: Option<(bisa_core::Workstream, bisa_core::Project, Vec<String>)>,
}

impl ScopeFacts {
    /// The Workflow Agent designs workflows and shapes goals, and a checkout
    /// has neither: a conversation about a project or a workstream never
    /// offers, resolves or triages to it. A channel and a goal's thread do.
    pub fn reaches_workflow_agent(&self) -> bool {
        self.conversation
            .as_ref()
            .is_none_or(|c| c.origin.reaches_workflow_agent())
    }

    pub fn project(&self) -> Option<bisa_core::ProjectId> {
        self.checkout.as_ref().map(|(_, p, _)| p.id)
    }

    pub fn workstream(&self) -> Option<bisa_core::WorkstreamId> {
        self.checkout.as_ref().map(|(w, _, _)| w.id)
    }

    pub fn conversation_id(&self) -> Option<ConversationId> {
        self.conversation.as_ref().map(|c| c.id)
    }
}

/// The facts of a scope, from the store. A goal thread's goal is checked to
/// exist — a channel id is a ULID too, and would otherwise pass for one.
pub(crate) fn scope_facts(inner: &Inner, scope: &str) -> ScopeFacts {
    if let Some(conversation) = inner.ws.conversation_of_scope(scope) {
        let checkout = checkout_of_origin(inner, &conversation.origin);
        return ScopeFacts {
            goal: conversation.origin.goal(),
            checkout,
            conversation: Some(conversation),
        };
    }
    ScopeFacts {
        conversation: None,
        goal: scope
            .parse::<GoalId>()
            .ok()
            .filter(|g| inner.ws.get_goal(*g).is_ok()),
        checkout: None,
    }
}

/// The checkout an origin runs in: a workstream's own, a project's primary.
pub(crate) fn checkout_of_origin(
    inner: &Inner,
    origin: &ConversationOrigin,
) -> Option<(bisa_core::Workstream, bisa_core::Project, Vec<String>)> {
    let workstream = match origin {
        ConversationOrigin::Workstream { id, .. } => inner.ws.get_workstream(*id).ok()?,
        ConversationOrigin::Project { id } => inner.ws.primary_workstream(*id).ok()?,
        _ => return None,
    };
    let project = inner.ws.get_project(workstream.project).ok()?;
    let goals = inner
        .ws
        .goals_for_project(project.id)
        .unwrap_or_default()
        .into_iter()
        .map(|g| {
            g.title.clone().unwrap_or_else(|| {
                g.statement
                    .lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(60)
                    .collect()
            })
        })
        .collect();
    Some((workstream, project, goals))
}

/// One live chat session for a `(scope, agent)` pair.
struct ConvSession {
    /// The person on another node whose message woke the latest turn, with
    /// their role — what the session's tool calls are judged for
    /// (14-collaboration). Set on every wake; the owner's own is `None`.
    woken_by: std::sync::Mutex<Option<(bisa_core::PrincipalId, bisa_core::MemberRole)>>,
    agent_id: LiveRunId,
    /// The durable row's id, so parking and ending reach the store.
    session_id: SessionId,
    /// The goal the scope is about, for what a permission escalates to.
    goal: Option<GoalId>,
    session: Arc<Mutex<Option<Box<dyn HarnessSession>>>>,
    /// Bumped on every turn; a pending idle-TTL task only disposes when its
    /// epoch is still current (same race resolution as `Lifecycle`).
    epoch: Arc<std::sync::atomic::AtomicU64>,
    /// Unix seconds when the caller last handed this session a new turn.
    /// Harnesses that do not announce `TurnStarted` on a follow-up (most
    /// one-shot adapters) would otherwise leave the pump folding the new
    /// reply into the previous turn — and the duplicate check would then
    /// swallow it, so the human's second message would go unanswered.
    turn_mark: Arc<std::sync::atomic::AtomicU64>,
    supports_follow_up: bool,
    /// Where the session runs — the guard judges a relative path against it.
    cwd: std::path::PathBuf,
    /// The conversation about a checkout this session is a turn of, and the
    /// tracker of what it changes there (ide/20). `None` for every other
    /// scope: a turn in the agent's own folder changes nothing reviewed.
    review: Option<(
        bisa_core::ConversationId,
        Arc<crate::changes::tracker::ChangeTracker>,
    )>,
}

impl ConvSession {
    /// The conversation and its mode as it stands now — a person changes it
    /// between turns, and the next call is judged by the new one.
    fn reach(&self, inner: &Inner) -> Option<crate::inputs::ConversationReach> {
        let (id, _) = self.review.as_ref()?;
        let mode = inner.ws.get_conversation(*id).ok()?.mode;
        Some(crate::inputs::ConversationReach { id: *id, mode })
    }
}

/// The newest message in a scope: what woke the turn beginning now.
fn newest_message_id(inner: &Inner, scope: &str) -> Option<String> {
    inner
        .ws
        .messages(scope, None, 1)
        .unwrap_or_default()
        .into_iter()
        .next_back()
        .map(|m| m.id)
}

/// A turn begins: fold away what others wrote, open the turn's record.
async fn begin_tracked_turn(
    inner: &Arc<Inner>,
    scope: &str,
    tracker: &Arc<crate::changes::tracker::ChangeTracker>,
    mode: bisa_core::ConversationMode,
) {
    let prompt = newest_message_id(inner, scope);
    let tracker = Arc::clone(tracker);
    let begun =
        crate::changes::off_thread(inner, move |inner| tracker.begin_turn(inner, prompt, mode))
            .await;
    if let Some(Err(e)) = begun {
        tracing::warn!(scope = %scope, "a turn's changes are not tracked: {e}");
    }
}

/// Per-conversation coordination: one wake in flight per `(scope, agent)`,
/// one queued re-wake, and the live sessions.
#[derive(Default)]
pub struct ConversationState {
    waking: DashMap<(String, String), ()>,
    /// The one queued follow-up per (scope, agent), and whom it is for.
    pending: DashMap<(String, String), Option<(bisa_core::PrincipalId, bisa_core::MemberRole)>>,
    sessions: DashMap<(String, String), Arc<ConvSession>>,
    /// The turn in flight per (scope, agent): its words and thinking so far.
    live: DashMap<(String, String), LiveTurn>,
}

/// The turns in flight in a scope, by agent id, oldest first — what a reader
/// that joins mid-turn is given before the next frame reaches it.
pub fn live_turns(inner: &Inner, scope: &str) -> Vec<(String, LiveTurn)> {
    let mut out: Vec<(String, LiveTurn)> = inner
        .conversation
        .live
        .iter()
        .filter(|e| e.key().0 == scope)
        .map(|e| (e.key().1.clone(), e.value().clone()))
        .collect();
    out.sort_by_key(|(_, t)| t.since);
    out
}

impl ConvSession {
    fn set_woken_by(&self, who: Option<(bisa_core::PrincipalId, bisa_core::MemberRole)>) {
        *self.woken_by.lock().unwrap_or_else(|e| e.into_inner()) = who;
    }

    fn woken_by(&self) -> Option<(bisa_core::PrincipalId, bisa_core::MemberRole)> {
        self.woken_by
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Watch locally-authored conversation writes and answer as any agent addressed.
pub fn spawn_listener(inner: &Arc<Inner>) -> tokio::task::JoinHandle<()> {
    // Subscribe *before* spawning: a broadcast channel only delivers to
    // receivers that already exist, so subscribing inside the task would
    // silently drop anything posted in the moments after startup.
    let mut rx = inner.ws.subscribe_store_events();
    let inner = Arc::clone(inner);
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(StoreEvent::ConversationAppended {
                    scope,
                    event,
                    origin,
                    ..
                }) => {
                    // Only messages. A reaction or a retraction appends
                    // and emits exactly as a message does, so this filter is
                    // what keeps an agent's own mark (below) from re-entering
                    // `dispatch` and waking anything.
                    if event.kind.as_u16() != bisa_core::kind::KIND_MESSAGE {
                        continue;
                    }
                    // One message that breaks the dispatch is one line in
                    // the log, never the end of this loop — nobody would be
                    // woken by any message after it.
                    crate::contain("conversation dispatch", || {
                        dispatch(&inner, &scope, &event, origin, None)
                    });
                }
                // A person on another node spoke: the collaboration module
                // reads it first and dispatches when it may.
                Ok(StoreEvent::RemoteMessageArrived {
                    scope,
                    event,
                    author,
                    role,
                    ..
                }) => {
                    let inner = Arc::clone(&inner);
                    tokio::spawn(crate::survive("read of a remote message", async move {
                        crate::collab::on_remote_message(&inner, scope, event, author, role).await;
                    }));
                }
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("conversation listener lagged by {n} events");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    })
}

/// Decide which agents (if any) owe a reply to this message. `on_behalf_of`
/// names the person on another node who wrote it, with their role: every
/// session it wakes works for them (14-collaboration).
///
/// This is also where a message is **heard** — by a `message` start, wait or
/// boundary ([`crate::listen::ear::on_message`]): here, once, after the hold
/// a message from another node waits in, and never for an announcement.
pub(crate) fn dispatch(
    inner: &Arc<Inner>,
    scope: &str,
    event: &nostr::event::Event,
    origin: PostOrigin,
    on_behalf_of: Option<(bisa_core::PrincipalId, bisa_core::MemberRole)>,
) {
    crate::listen::ear::on_message(inner, scope, event, origin);
    dispatch_to(inner, scope, event, origin, on_behalf_of, None);
}

/// The one question a triage asks the Decision-Making Agent.
const TRIAGE_QUESTION: &str = "agent";

/// [`dispatch`], with the agent an unaddressed message is triaged to already
/// chosen — by the Decision-Making Agent, which was asked off this path and
/// came back here. `None` is the first pass: the rule decides, or asks.
fn dispatch_to(
    inner: &Arc<Inner>,
    scope: &str,
    event: &nostr::event::Event,
    origin: PostOrigin,
    on_behalf_of: Option<(bisa_core::PrincipalId, bisa_core::MemberRole)>,
    triaged: Option<AgentId>,
) {
    let author = event.pubkey.to_hex();
    let Ok(agents) = inner.ws.list_agents() else {
        return;
    };
    let facts = scope_facts(inner, scope);

    // Loop guard, first — and it guards on *shape*, never on a counter:
    //
    // > **Only the core agent's message may wake another agent, and never
    // > itself.**
    //
    // An agent's own reply re-emits this event, so without a guard two agents
    // in one conversation would talk forever. Refusing *every* agent-authored
    // message stopped that and stopped the hand-off with it: a triage post
    // naming `@developer` could never reach the developer, so the core agent
    // could route in prose and nothing would happen.
    //
    // Admitting exactly the core agent bounds the chain by construction.
    // Human → triage → worker → stop: the worker's reply is agent-authored and
    // is not the core agent, and the core agent's own post is refused a wake
    // of itself in the loop below. Two sessions per human message, maximum,
    // and that bound is a property of who may speak rather than of a count
    // somebody has to remember to decrement.
    //
    // One more thing may speak: **the platform, announcing** — a workflow's
    // `notify` step, signed by the agent it names or by the Workflow Agent.
    // An announcement carries the owner's authority whoever signs it: it
    // addresses exactly whom it mentions, none by default, never triages,
    // and cannot chain — the woken agent's reply is asked, agent-authored
    // and not core, so it is dropped here as any worker's is.
    let announced = origin == PostOrigin::Announced;
    let authoring_agent = agents.iter().find(|a| a.pubkey.as_hex() == author);
    let author_is_agent = authoring_agent.is_some();
    let author_is_core = match authoring_agent {
        Some(a) if a.id.is_core_id() => true,
        Some(_) if announced => false,
        Some(_) => return,
        None => false,
    };
    let author_id = authoring_agent.map(|a| a.id.clone());

    let channel = ChannelId::new(scope)
        .ok()
        .and_then(|id| inner.ws.get_channel(&id).ok());
    let audience: Vec<String> = channel
        .as_ref()
        .map(|c| {
            c.audience
                .principals()
                .iter()
                .map(|p| p.as_hex().to_string())
                .collect()
        })
        .unwrap_or_default();
    // The Workflow Agent is a goal's: it designs workflows and shapes goals,
    // and a checkout has neither for it to act on. A mention of it in a
    // conversation about a project or a workstream names nobody — the
    // message is unaddressed, and triage takes it to the General Agent —
    // so the rule holds here, at the engine, and no surface can route
    // around it (13 — Conversations).
    let unreachable_workflow_agent = (!facts.reaches_workflow_agent())
        .then(|| agents.iter().find(|a| a.id.is_workflow()))
        .flatten()
        .map(|a| a.pubkey.as_hex().to_string());
    let mentioned: Vec<String> = event
        .tags
        .iter()
        .filter_map(|t| {
            let s = t.as_slice();
            (s.len() >= 2 && s[0] == "p").then(|| s[1].to_string())
        })
        .filter(|pk| unreachable_workflow_agent.as_ref() != Some(pk))
        .collect();

    // > **An unaddressed message is addressed to the platform.**
    //
    // A standing channel has an empty audience — empty means the whole
    // workspace — so a message naming nobody matched no agent and this
    // function returned having done nothing at all: no reply, no log line,
    // nothing in the window to say the workspace had heard you. One agent
    // takes it: the **default agent** (`agents.default`), which is the general
    // agent unless a project says otherwise. It either answers or names the
    // agent that should. Nothing in code scores agents — `assign::pick` is a
    // rotation and there is deliberately no semantic selection anywhere — so
    // the routing is that session's judgement, which is what it is for.
    //
    // A **non-core** default answers but does not route: the loop guard above
    // admits only the core agent's post as a wake, so its `@developer` reaches
    // nobody. That is the bound, kept: a workstream whose default is the
    // Reviewer gets the Reviewer, and a hand-off still belongs to the core.
    //
    // Which rooms have nobody to answer for them.
    //
    // A **standing channel** — an empty audience means the whole workspace, so
    // nobody in particular. And a **thread**: a goal's or a workstream's,
    // where `get_channel` finds nothing at all. Both are rooms you can speak
    // into and be met with silence.
    //
    // A **DM** is excluded, and that exclusion is the reason clearing the
    // address tray in one does not silence it: a DM's audience already names
    // who is listening, so its agent matches on `audience.contains` below
    // without any mention and never needed triage.
    let unaddressed_room = match channel.as_ref().map(|c| &c.kind) {
        Some(ChannelKind::Standing) => true,
        Some(_) => false,
        None => true,
    };

    // Three narrowings, each of them a reason this rule must not fire:
    //
    // - only a room with **nobody to answer for it**, above;
    // - only when **nobody at all** was named, because a message that named
    //   somebody was answered by naming them;
    // - only a message the core agent did **not** write, because its own
    //   unaddressed post summoning itself is the one place this rule could
    //   have made the loop the guard above exists to prevent;
    // - and only a message somebody **asked**. A workflow's `notify` step
    //   posts as an agent and *announces*: it addresses exactly whom it
    //   mentions — none by default — and, with no scope, lands on the goal
    //   thread as a line nobody asked. Without [`PostOrigin`] every cron
    //   tick, probe and webhook would start a harness session, which is
    //   exactly why this rule was narrowed to standing channels when it was
    //   written. It applies to channels too, closing the same hole there.
    let triage = !author_is_core
        && unaddressed_room
        && origin == PostOrigin::Asked
        && audience.is_empty()
        && mentioned.is_empty();
    // Who an unaddressed message reaches: the project's `agents.default` for a
    // conversation in a checkout, the workspace's elsewhere — resolved once,
    // and only when it is going to be used.
    let default_id = match (triage, triaged) {
        (true, Some(chosen)) => chosen,
        (true, None) => {
            let default_id = default_agent(inner, &agents, &facts, &author);
            // The Decision-Making Agent, when it is on here, reads the message
            // and names the agent; the default agent is what it falls back to.
            // It is asked off this path — a dispatch never waits on a model —
            // and the wake comes back through here with the agent chosen.
            let standing = crate::decider::Standing {
                home: facts.goal.map(bisa_core::Home::from),
                project: facts.project(),
                switched_on: agents
                    .iter()
                    .any(|a| a.id == default_id && a.decision_making),
                ..Default::default()
            };
            let candidates: Vec<(String, String)> = agents
                .iter()
                .filter(|a| a.enabled && author_id.as_ref() != Some(&a.id))
                .filter(|a| !a.id.is_workflow() || facts.reaches_workflow_agent())
                .filter(|a| may_respond(inner, a, &author))
                .map(|a| {
                    let what = a
                        .description
                        .clone()
                        .filter(|d| !d.trim().is_empty())
                        .unwrap_or_else(|| a.name.clone());
                    (a.id.to_string(), what)
                })
                .collect();
            if candidates.len() >= 2
                && crate::decider::is_on(inner, bisa_core::DecisionPoint::DispatchTriage, &standing)
            {
                let inner = Arc::clone(inner);
                let scope = scope.to_string();
                let event = event.clone();
                let asked = event.content.clone();
                tokio::spawn(async move {
                    let request = bisa_core::DecisionRequest::one(
                        serde_json::json!({ "message": asked }),
                        TRIAGE_QUESTION,
                        bisa_core::DecisionQuestion::choice(
                            "Nobody was named in this message. Which agent should answer it?",
                            candidates,
                        ),
                    );
                    let chosen = crate::decider::judge(
                        &inner,
                        bisa_core::DecisionPoint::DispatchTriage,
                        &standing,
                        request,
                    )
                    .await
                    .answered()
                    .and_then(|r| r.answer(TRIAGE_QUESTION)?.chosen().map(str::to_string))
                    .and_then(|id| AgentId::new(id.as_str()).ok())
                    .unwrap_or(default_id);
                    dispatch_to(&inner, &scope, &event, origin, on_behalf_of, Some(chosen));
                });
                return;
            }
            default_id
        }
        (false, _) => AgentId::general(),
    };

    for agent in agents {
        if !agent.enabled {
            continue;
        }
        let is_default = agent.id == default_id;
        // Never itself — and the Workflow Agent never wakes the General Agent,
        // which is what bounds the General Agent and the Workflow Agent to one hand-off rather than
        // a conversation with each other. The other half of the loop guard.
        if author_id.as_ref() == Some(&agent.id) {
            continue;
        }
        if !announced
            && author_id.as_ref().is_some_and(|a| a.is_workflow())
            && agent.id.is_general()
        {
            continue;
        }
        // The Workflow Agent where a checkout is the subject: its mention
        // was dropped above and `default_agent` never names it, so this is
        // the belt to those braces.
        if agent.id.is_workflow() && !facts.reaches_workflow_agent() {
            continue;
        }
        let pk = agent.pubkey.as_hex().to_string();
        // Addressed by being in the conversation (a DM), by name, or — the
        // default agent alone — by nobody having been named at all.
        if !(audience.contains(&pk) || mentioned.contains(&pk) || (triage && is_default)) {
            continue;
        }
        // An announcement carries the owner's authority: the respond policy
        // is the person's to satisfy, and the workflow speaks for the person.
        if !announced && !may_respond(inner, &agent, &author) {
            tracing::debug!(
                agent = %agent.id,
                "message from {author} does not satisfy the agent's respond policy"
            );
            continue;
        }
        if triage && is_default {
            tracing::info!(scope = %scope, "message named nobody: triaging to {}", agent.id);
        }

        // **Mark the message, here, not in the wake.**
        //
        // This is the only point that knows all three of: which message, which
        // agent, and that a wake is actually going to happen. `schedule` below
        // collapses a burst into one wake plus one queued follow-up and never
        // receives the triggering event, so marking there would mark one
        // message out of three. Marking at the decision marks every message
        // that causes a wake, and costs nothing when the wake coalesces —
        // `has_reaction` dedupes on `(target, author, emoji)`.
        //
        // Only a human's message. An agent reacting to the core agent's
        // hand-off would be a mark on a message you did not write.
        if !author_is_agent {
            if let Err(e) = inner
                .ws
                .react(&event.id.to_hex(), TAKEN_MARK, Some(&agent.id))
            {
                // Never fatal: the mark is a courtesy, and failing to draw it
                // must not cost somebody their answer.
                tracing::warn!(agent = %agent.id, "could not mark the message taken: {e}");
            }
        }

        // One Workflow Agent session per goal, ever.
        //
        // The guided cycle runs the *same agent* on this goal and files its
        // session under `GoalId` in `guided`, while this module files under
        // `(scope, agent)` — two maps with disjoint key types, so
        // `wake_attempt` cannot see a live guided session and would launch a
        // concurrent second one, both at `ToolTier::Exec` with tools on the
        // same goal. Queue instead, into the same slot `schedule` uses for
        // its own follow-up; `guided` drains it when the cycle ends.
        if agent.id.is_workflow() && guided_holds_this_scope(inner, scope) {
            tracing::info!(
                scope = %scope,
                "the guided cycle holds this goal: queueing the chat wake"
            );
            inner.conversation.pending.insert(
                (scope.to_string(), agent.id.to_string()),
                on_behalf_of.clone(),
            );
            continue;
        }

        schedule(
            inner,
            scope.to_string(),
            agent.id.to_string(),
            on_behalf_of.clone(),
        );
    }
}

/// Wake the Workflow Agent for a chat message that was queued while the
/// guided cycle held this goal.
///
/// Called by `guided` when a cycle ends. It reads the same
/// `(scope, agent)` slot `schedule` queues its own follow-up into, so
/// "exactly one queued follow-up" stays true whichever side was busy — and a
/// scope with nothing waiting costs one map lookup.
pub(crate) fn drain_deferred(inner: &Arc<Inner>, goal: bisa_core::GoalId) {
    let key = (goal.to_string(), AgentId::WORKFLOW.to_string());
    if let Some((_, for_whom)) = inner.conversation.pending.remove(&key) {
        tracing::info!(
            scope = %key.0,
            "the guided cycle finished: waking the chat message it was holding"
        );
        schedule(inner, key.0, key.1, for_whom);
    }
}

/// The emoji an agent puts on the message it has taken.
///
/// 👀 rather than a bespoke marker: it is already in the desktop's quick-react
/// row, so it renders through the pill the timeline already draws, and it means
/// in every chat product what it means here.
const TAKEN_MARK: &str = "👀";

/// Is the guided driver mid-cycle on the goal this scope names?
///
/// `waking` only, deliberately — not `sessions`. That map holds a session
/// *parked* for follow-up with an idle TTL, and deferring on a parked one would
/// defer until the TTL expired, which is a long time to leave somebody without
/// an answer. What must not overlap is two turns running at once.
fn guided_holds_this_scope(inner: &Arc<Inner>, scope: &str) -> bool {
    scope
        .parse::<bisa_core::GoalId>()
        .is_ok_and(|goal| inner.guided.busy(goal))
}

/// The one place `RespondPolicy` is enforced: who an agent will act on.
/// Public so the policy can be asserted directly — it decides whether an
/// agent is reachable at all.
pub fn may_respond(inner: &Arc<Inner>, agent: &Agent, author_hex: &str) -> bool {
    let admitted = match agent.respond {
        RespondPolicy::OwnerOnly => inner.ws.owner_principal().as_hex() == author_hex,
        // A member whose role may address agents — a guest's mention names
        // nobody, whatever the agent's policy (14-collaboration).
        RespondPolicy::Members => match bisa_core::PrincipalId::new(author_hex.to_string()) {
            Ok(p) => inner
                .ws
                .member_role(&p)
                .ok()
                .flatten()
                .is_some_and(|r| r.may(bisa_core::Permission::MentionAgents)),
            Err(_) => false,
        },
    };
    if admitted {
        return true;
    }
    // The core agent's hand-off carries the owner's authority, so every
    // policy admits it. It speaks here only because the owner said something
    // first, and every message it posts is NIP-OA attested to the owner: a
    // hand-off is the owner's question arriving by way of the agent that read
    // it. Without this the routing would be decorative — an agent is not a
    // *member*, and `owner_only` is the default, so both policies would refuse
    // the hand-off and the agent named would sit there silent while the chat
    // window showed it had been asked.
    //
    // Checked only after the policy has already said no, so the ordinary path
    // — a human's message — never pays for it.
    inner
        .ws
        .get_agent(&AgentId::general())
        .is_ok_and(|core| core.pubkey.as_hex() == author_hex)
}

/// One wake in flight per `(scope, agent)`; a message arriving during a wake
/// queues exactly one follow-up, so a burst produces one reply, then one more.
fn schedule(
    inner: &Arc<Inner>,
    scope: String,
    agent_id: String,
    on_behalf_of: Option<(bisa_core::PrincipalId, bisa_core::MemberRole)>,
) {
    let key = (scope.clone(), agent_id.clone());
    if inner.conversation.waking.insert(key.clone(), ()).is_some() {
        inner.conversation.pending.insert(key, on_behalf_of);
        return;
    }
    let inner = Arc::clone(inner);
    tokio::spawn(async move {
        // The mark is lifted whatever became of the wake: one that unwound
        // — an adapter that broke at launch — costs its turn, and the next
        // message wakes the agent again. Left standing, the mark made every
        // later message wait behind a wake that was gone.
        crate::survive(
            "conversation wake",
            wake(&inner, &scope, &agent_id, on_behalf_of),
        )
        .await;
        inner.conversation.waking.remove(&key);
        if let Some((_, next)) = inner.conversation.pending.remove(&key) {
            schedule(&inner, scope, agent_id, next);
        }
    });
}

/// Names an author the way an agent should read them — never a pubkey.
///
/// A type rather than a closure because two callers need it and one of them is
/// the fifty-message transcript: the roster, the owner and the member list are
/// fetched **once** here and reused per line, where a free function would have
/// gone back to the store for every row.
struct Labeller {
    agents: Vec<Agent>,
    members: Vec<WorkspaceMember>,
    owner: String,
    me: String,
}

impl Labeller {
    fn new(inner: &Arc<Inner>, me: &str) -> Self {
        Self {
            agents: inner.ws.list_agents().unwrap_or_default(),
            members: inner.ws.members().unwrap_or_default(),
            owner: inner.ws.owner_principal().as_hex().to_string(),
            me: me.to_string(),
        }
    }

    fn of(&self, author: &str) -> String {
        if author == self.owner {
            return "The owner".into();
        }
        if let Some(a) = self.agents.iter().find(|a| a.pubkey.as_hex() == author) {
            return if a.id.as_str() == self.me {
                format!("{} (you)", a.name)
            } else {
                a.name.clone()
            };
        }
        self.members
            .iter()
            .find(|m| m.pubkey.as_hex() == author)
            .and_then(|m| m.label.clone())
            .unwrap_or_else(|| format!("{}…", &author[..8.min(author.len())]))
    }
}

/// One message as an agent reads it: who said it, what they said, and one line
/// per file with the path to open.
///
/// **The universal half of "the harness processes the file".** Every adapter
/// can read a path — `Read`/`Grep`/`Glob` are allowed at every tier, and the
/// four non-claude adapters restrict nothing — so this is the floor that works
/// everywhere. Handing an image over natively is the ceiling, and only one
/// adapter has it.
///
/// A file whose bytes have not arrived is *said to be absent* rather than given
/// a path that would fail to open. An agent told the truth can say so; an agent
/// handed a broken path reports a tool error.
fn render_message(inner: &Arc<Inner>, m: &bisa_store::MessageRow, who: &Labeller) -> String {
    let mut line = format!("{}: {}", who.of(&m.author), m.content);
    for c in crate::framing::context_lines(&m.context) {
        line.push('\n');
        line.push_str(&c);
    }
    // A capture's picture is a file the agent opens: its named copy's path,
    // beside the line that names the device.
    for r in &m.context {
        if let bisa_core::ContextRef::Capture { label, shot, .. } = r {
            line.push_str(&format!(
                "\n  [capture file] {label} · {}",
                match crate::mobile_development::capture_path(inner, shot) {
                    Some(p) => p.display().to_string(),
                    None => "not on this machine yet".to_string(),
                }
            ));
        }
    }
    for a in &m.attachments {
        line.push_str(&format!(
            "\n  [file] {} · {} · {}",
            a.file.name,
            a.file.mime,
            match inner.ws.attachment_path(&a.file.sha256) {
                Some(p) => p.display().to_string(),
                None => "not on this machine yet".to_string(),
            }
        ));
    }
    // The blob's path has no extension, so the name is said beside it.
    for a in &m.artifacts {
        line.push_str(&format!(
            "\n  [artifact] {} · {} · {} ({})",
            a.artifact.title,
            a.artifact.kind,
            match inner.ws.attachment_path(&a.artifact.sha256) {
                Some(p) => p.display().to_string(),
                None => "not on this machine yet".to_string(),
            },
            a.artifact.name
        ));
    }
    line
}

/// The transcript a fresh session is given: the last [`TRANSCRIPT_WINDOW`]
/// messages, oldest first. The harness holds the rest as its own context.
fn transcript(inner: &Arc<Inner>, scope: &str, me: &str) -> String {
    let who = Labeller::new(inner, me);
    inner
        .ws
        .transcript_tail(scope, TRANSCRIPT_WINDOW)
        .unwrap_or_default()
        .into_iter()
        .filter(|m| !m.retracted)
        .map(|m| render_message(inner, &m, &who))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The newest message's files, as the harness wants them.
///
/// **Only the newest.** The transcript names every attachment in the window by
/// path, which is what lets an agent go back and read an older one; handing the
/// harness every image in the last fifty messages would re-send the whole
/// conversation as pixels on every turn. What is carried natively is what is
/// being answered.
///
/// A file whose bytes are not on this machine is left out rather than pointed
/// at — the transcript already says it is absent, and an adapter given an
/// unreadable path could only log a failure.
fn latest_attachments(inner: &Arc<Inner>, scope: &str) -> Vec<bisa_harness::Attachment> {
    inner
        .ws
        .messages(scope, None, 1)
        .unwrap_or_default()
        .into_iter()
        .next_back()
        .map(|m| {
            // An artifact's bytes are an attachment's: a chart an agent made
            // reaches an image-taking harness the way a photo does.
            // A capture's screen reaches an image-taking harness the way a
            // photo does.
            let captures: Vec<bisa_core::AttachmentRef> = m
                .context
                .iter()
                .filter_map(|r| match r {
                    bisa_core::ContextRef::Capture { shot, .. } => Some(shot.clone()),
                    _ => None,
                })
                .collect();
            let files = m
                .attachments
                .into_iter()
                .map(|a| a.file)
                .chain(m.artifacts.into_iter().map(|a| a.artifact.file()))
                .chain(captures);
            files
                .filter_map(|file| {
                    inner
                        .ws
                        .attachment_path(&file.sha256)
                        .map(|path| bisa_harness::Attachment {
                            name: file.name,
                            mime: file.mime,
                            path,
                        })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The session's spec. A goal thread's turn, or a conversation about a
/// goal, is launched knowing the goal (`--goal` beside `--conversation`):
/// the MCP server hands the Workflow Agent the shaping tools on that scope,
/// so *Request changes* in the thread can propose.
fn build_conversation_spec(
    inner: &Inner,
    scope: &str,
    agent: &Agent,
    cwd: std::path::PathBuf,
    goal: Option<GoalId>,
) -> SessionSpec {
    let command = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "bisa".into());
    let mut args = vec![
        "mcp".into(),
        "--socket".into(),
        inner.socket_path.display().to_string(),
        "--conversation".into(),
        scope.to_string(),
        "--agent".into(),
        agent.id.to_string(),
    ];
    if let Some(goal) = goal {
        args.push("--goal".into());
        args.push(goal.to_string());
    }
    let mut mcp_servers = vec![bisa_harness::McpMount::platform(
        bisa_harness::McpServerConfig::Stdio {
            name: "bisa".into(),
            command,
            args,
            // Where the MCP server writes its own log: the parent's folder,
            // named — it is spawned without `--data-dir` and must not guess.
            env: crate::logging::child_env(inner),
            cwd: None,
        },
    )];
    // Registry lookups, not embedded blobs: an id that no longer resolves is
    // warned about inside the store and costs this session that one server.
    mcp_servers.extend(crate::executor::installed_mounts(
        inner,
        &agent.id,
        &agent.mcps,
    ));
    let skills = inner.ws.skill_payloads(&agent.id, &agent.skills);
    let session_env = crate::network::session_env(inner, Default::default());
    SessionSpec {
        work_item: None,
        cwd,
        prompt: String::new(),
        // The model is chosen per attempt by the executor's launch walk, from
        // the agent's plan against the live health ledger — a chat survives a
        // quota wall the same way a work item does.
        model: None,
        // And the effort with it, fitted to that model.
        effort: None,
        mcp_servers,
        env: session_env.0,
        env_remove: session_env.1,
        // Chat instances are full-capability: "do this for me" has to work.
        tier_ceiling: bisa_core::ToolTier::Exec,
        output_schema: None,
        skills,
    }
}

/// The agent an unaddressed message reaches: `agents.default`, resolved at
/// the project layer for a conversation in a checkout and at the workspace
/// layer elsewhere.
///
/// The general agent stands in whenever the named one cannot take it — unset,
/// unknown, disabled, refusing this author under its respond policy, or the
/// Workflow Agent where a checkout is the subject — so an unaddressed message
/// is never met with silence because of a stale or impossible setting.
fn default_agent(
    inner: &Arc<Inner>,
    agents: &[bisa_core::Agent],
    facts: &ScopeFacts,
    author: &str,
) -> AgentId {
    let named = inner
        .ws
        .setting("agents.default", facts.project())
        .ok()
        .and_then(|r| r.value.as_str().map(str::to_string))
        .and_then(|id| AgentId::new(&id).ok());
    let Some(id) = named else {
        return AgentId::general();
    };
    match agents.iter().find(|a| a.id == id) {
        Some(a)
            if a.enabled
                && may_respond(inner, a, author)
                && (facts.reaches_workflow_agent() || !a.id.is_workflow()) =>
        {
            id
        }
        _ => AgentId::general(),
    }
}

/// The agent's own folder — its chat sessions run here, so a chat instance
/// cannot trample workspace truth.
///
/// Failure is returned rather than absorbed. This used to fall back to the
/// workspace root, which is the widest directory there is: the one moment the
/// placement could not be honoured was the one moment a full-capability chat
/// session was handed the journal, the index and the key files. A wake that
/// does not happen is recoverable; a wake in the wrong place is not.
fn agent_scratch_dir(inner: &Inner, agent_id: &AgentId) -> std::io::Result<std::path::PathBuf> {
    let dir = inner.ws.paths().agent(agent_id).scratch();
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

async fn wake(
    inner: &Arc<Inner>,
    scope: &str,
    agent_id: &str,
    on_behalf_of: Option<(bisa_core::PrincipalId, bisa_core::MemberRole)>,
) {
    wake_attempt(
        inner,
        scope,
        agent_id,
        Attempts::new(inner.config.max_model_attempts),
        Vec::new(),
        None,
        on_behalf_of,
    )
    .await;
}

/// One launch of a chat session, carrying the run's remaining model-attempt
/// budget.
///
/// A chat has no workstream and no work item, so its failover is the simplest
/// possible shape: the reply pump sees a terminal `ModelUnavailable`, notes
/// the ledger, and calls back in here. The next pass through
/// [`executor::resolve_and_launch`] sees the cooldown and picks the next model
/// in the plan. `attempts` is what stops a plan whose every model is dead from
/// launching forever.
///
/// `judged` is what the Decision-Making Agent named for this wake — the model
/// that leads, the level to work at. A first launch has none and asks; a
/// relaunch after a wall carries what the first was told, so one wake asks
/// once and the next model runs at the same judged level, fitted to its own
/// list.
async fn wake_attempt(
    inner: &Arc<Inner>,
    scope: &str,
    agent_id: &str,
    mut attempts: Attempts,
    pending_walls: Vec<executor::ModelWall>,
    judged: Option<crate::decider::Walk>,
    on_behalf_of: Option<(bisa_core::PrincipalId, bisa_core::MemberRole)>,
) {
    inner.pause.wait_running().await;
    let Ok(agent_key) = AgentId::new(agent_id) else {
        return;
    };
    let Ok(agent) = inner.ws.get_agent(&agent_key) else {
        return;
    };
    if !agent.enabled {
        return;
    }
    let key = (scope.to_string(), agent_id.to_string());

    // A live session already holds the harness's own context: just hand it
    // the new turn. The pump task posts whatever it says.
    if let Some(conv) = inner
        .conversation
        .sessions
        .get(&key)
        .map(|c| Arc::clone(&c))
    {
        if conv.supports_follow_up {
            conv.set_woken_by(on_behalf_of.clone());
            let reach = conv.reach(inner);
            if let (Some(reach), Some((_, tracker))) = (reach, conv.review.as_ref()) {
                begin_tracked_turn(inner, scope, tracker, reach.mode).await;
            }
            let guard = conv.session.lock().await;
            if let Some(session) = guard.as_ref() {
                // The newest message rendered the way the transcript renders
                // it — its line, plus a line per file — **and its chips in
                // full** (`framing::follow_up_text`): a live session has no
                // prompt to carry the context block, so the turn carries it.
                // Deliberately **not** `transcript(..).lines().last()`: a
                // message that carries a file ends in an attachment line, and
                // that is what a live session would have been handed instead
                // of the question.
                let text = inner
                    .ws
                    .messages(scope, None, 1)
                    .unwrap_or_default()
                    .into_iter()
                    .next_back()
                    .map(|m| {
                        crate::framing::follow_up_text(
                            render_message(inner, &m, &Labeller::new(inner, agent_id)),
                            &m.context,
                        )
                    })
                    .unwrap_or_default();
                let text = match reach {
                    Some(reach) => {
                        format!("{}\n\n{text}", crate::framing::mode_note(reach.mode))
                    }
                    None => text,
                };
                if session
                    .follow_up(bisa_harness::Steer {
                        text,
                        attachments: latest_attachments(inner, scope),
                    })
                    .await
                    .is_ok()
                {
                    conv.turn_mark
                        .store(now_secs(), std::sync::atomic::Ordering::SeqCst);
                    conv.epoch.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    emit_thinking(inner, scope, agent_id);
                    return;
                }
            }
        }
        // Stale handle — drop it and launch fresh below.
        inner.conversation.sessions.remove(&key);
    }

    // A conversation about a workstream or a project runs **in the
    // checkout** (ide/09): the primary's is the project tree, a worktree's is
    // its own directory, and the frame below says which goals the project is
    // attached to. Every other scope keeps the agent's own scratch folder.
    let facts = scope_facts(inner, scope);
    let cwd = match &facts.checkout {
        Some((workstream, project, _)) => inner.ws.checkout_in(project, workstream),
        None => match agent_scratch_dir(inner, &agent_key) {
            Ok(dir) => dir,
            Err(e) => {
                tracing::warn!(
                    agent = %agent_id, scope = %scope,
                    "chat wake refused: cannot create the agent's work directory: {e}"
                );
                return;
            }
        },
    };
    if let Some((workstream, project, _)) = &facts.checkout {
        if !cwd.is_dir() {
            tracing::warn!(
                agent = %agent_id, scope = %scope,
                "chat wake refused: workstream {} of project {} has no checkout at {}",
                workstream.id, project.slug, cwd.display()
            );
            return;
        }
    }
    // How far the agent goes on its own here (ide/20) — a checkout's alone.
    let reviewed = facts
        .conversation
        .as_ref()
        .filter(|c| c.origin.is_checkout() && facts.checkout.is_some())
        .map(|c| (c.id, c.mode));
    let guarded = inner
        .catalog
        .get(&agent.harness)
        .map(|a| a.caps().contains(bisa_core::HarnessCaps::TOOL_GUARD))
        .unwrap_or(false);
    if let Some((_, mode)) = reviewed {
        if mode.needs_tool_guard() && !guarded {
            refuse_unguarded_plan(inner, scope, &agent);
            return;
        }
    }
    let spec = build_conversation_spec(inner, scope, &agent, cwd.clone(), facts.goal);
    let candidates = vec![agent.harness.clone()];
    // The setting is read for the project the conversation stands in.
    let effort_setting = crate::effort::setting(inner, facts.project());
    let rotation = inner.models.next_rotation();
    // What is being asked is the newest message: the task an `auto_route`
    // plan is routed by and an effort of `auto` is judged on.
    let judged = match judged {
        Some(judged) => judged,
        None => {
            let asked = inner
                .ws
                .messages(scope, None, 1)
                .unwrap_or_default()
                .into_iter()
                .next_back()
                .map(|m| m.content)
                .unwrap_or_default();
            crate::decider::walk(
                inner,
                &crate::decider::WalkAsk {
                    plan: &agent.models,
                    candidates: &candidates,
                    model_pin: None,
                    effort_pin: None,
                    effort_setting,
                    rotation,
                    task: &asked,
                    agent: Some(agent.id.as_str()),
                },
                &crate::decider::Standing {
                    home: facts.goal.map(bisa_core::Home::from),
                    agent: Some(agent.id.to_string()),
                    ..Default::default()
                },
            )
            .await
        }
    };
    let launch_plan = executor::LaunchPlan {
        candidates: &candidates,
        plan: &agent.models,
        pin: None,
        lead: judged.lead.as_deref(),
        effort_pin: None,
        effort_setting,
        judged_effort: judged.effort,
        rotation,
        skills_in_prompt: false,
    };
    let mut tried = std::collections::HashSet::new();
    let launched =
        match executor::resolve_and_launch(inner, &launch_plan, &mut tried, &mut attempts, &spec)
            .await
        {
            Ok(ok) => ok,
            Err(failure) => {
                let mut walls = pending_walls;
                walls.extend(failure.walls);
                report_chat_switches(inner, scope, agent_id, &walls, None);
                tracing::warn!(
                    agent = %agent_id, scope = %scope,
                    "chat wake failed to launch: {}", failure.message
                );
                return;
            }
        };
    let mut walls = pending_walls;
    walls.extend(launched.walls.iter().cloned());
    report_chat_switches(inner, scope, agent_id, &walls, Some(&launched.model_key));
    let harness_id = launched.harness.clone();
    let model_key = launched.model_key.clone();
    let effort = launched.effort;
    let skill_appendix = launched.skill_appendix;
    let in_flight = launched.in_flight;
    let session = launched.session;

    // Roster + session row: a chat instance is a first-class runtime agent.
    let agent_uid = LiveRunId::from_ulid(ulid::Ulid::from_datetime(SystemTime::now()));
    let session_id = SessionId::from_ulid(ulid::Ulid::from_datetime(SystemTime::now()));
    let transcript_path = session.resume_token().and_then(|t| t.transcript_path);
    debug_on_err(
        inner.registry.register_if(
            AgentRef {
                id: agent_uid,
                kind: SessionKind::Conversation,
                status: AgentStatus::Running,
                generation: 1,
                session_id: Some(session_id),
                work_item: None,
                conversation: facts.conversation_id(),
                goal: facts.goal,
                workstream: facts.workstream(),
                transcript_path: transcript_path.clone(),
                last_activity: now_secs(),
            },
            None,
        ),
        "registering a chat run",
    );
    inner.presence.register(
        inner,
        agent_uid,
        crate::presence::SessionMeta {
            kind: SessionKind::Conversation,
            harness: harness_id.clone(),
            model: Some(model_key.clone()),
            effort,
            agent: Some(agent.id.clone()),
            session_id: Some(session_id),
            work_item: None,
            conversation: facts.conversation_id(),
            goal: facts.goal,
            run: None,
            workstream: facts.workstream(),
            project: facts.project(),
            transcript_path: transcript_path.as_ref().map(|p| p.display().to_string()),
        },
    );
    warn_on_err(
        inner.ws.record_session(&SessionRow {
            id: session_id.to_string(),
            adapter: harness_id.clone(),
            kind: SessionKind::Conversation,
            conversation: facts.conversation_id().map(|c| c.to_string()),
            work_item: None,
            workstream: facts.workstream().map(|w| w.to_string()),
            agent_id: Some(agent.id.to_string()),
            transcript_path: transcript_path.map(|p| p.display().to_string()),
            resume_token_json: session
                .resume_token()
                .and_then(|t| serde_json::to_string(&t).ok()),
            status: SessionStatus::Live,
            parked_at: None,
            pid: None,
            pid_seen_at: None,
            ended_at: None,
        }),
        "recording a chat session",
    );

    let events = session.subscribe();
    let mut prompt = format!("{}\n\n{}", agent.system_prompt, conversation_framing());
    prompt.push_str(&crate::framing::mobile_development_note(
        inner,
        facts.project(),
    ));
    if let Some((_, mode)) = reviewed {
        prompt.push_str("\n\n");
        prompt.push_str(crate::framing::mode_note(mode));
    }
    // The frame: where the turn stands, said once. A checkout's says the
    // project and its goals; any other origin says what it is about.
    if let Some((workstream, project, goals)) = &facts.checkout {
        prompt.push_str("\n\n");
        prompt.push_str(&crate::framing::project_frame(
            project.slug.as_str(),
            goals,
            workstream.branch(),
        ));
    } else if let Some(conversation) = &facts.conversation {
        let subject = origin_subject(inner, &conversation.origin);
        let frame = crate::framing::origin_frame(&conversation.origin, subject.as_deref());
        if !frame.is_empty() {
            prompt.push_str("\n\n");
            prompt.push_str(&frame);
        }
    }
    prompt.push_str(&format!(
        "\n\n--- conversation ---\n{}",
        transcript(inner, scope, agent_id)
    ));
    // The newest message's chips, as a block: what the person attached to
    // the turn being answered, in chip order (ide/09).
    let latest_context = inner
        .ws
        .messages(scope, None, 1)
        .unwrap_or_default()
        .into_iter()
        .next_back()
        .map(|m| m.context)
        .unwrap_or_default();
    let block = crate::framing::context_block(&latest_context);
    if !block.is_empty() {
        prompt.push_str("\n\n");
        prompt.push_str(&block);
    }
    if let Some(appendix) = &skill_appendix {
        prompt.push_str("\n\n");
        prompt.push_str(appendix);
    }
    let first_turn = bisa_harness::PromptInput {
        text: prompt,
        attachments: latest_attachments(inner, scope),
    };
    let review = match (reviewed, &facts.checkout) {
        (Some((conversation, mode)), Some((workstream, _, _))) => {
            let tracker = Arc::new(crate::changes::tracker::ChangeTracker::new(
                inner,
                crate::changes::Checkout {
                    conversation,
                    workstream: workstream.id,
                    root: cwd.clone(),
                },
                agent.id.clone(),
                cwd.clone(),
                guarded,
            ));
            begin_tracked_turn(inner, scope, &tracker, mode).await;
            Some((conversation, tracker))
        }
        _ => None,
    };
    if let Err(e) = session.prompt(first_turn).await {
        tracing::warn!(agent = %agent_id, "chat prompt failed: {e}");
        warn_on_err(session.dispose().await, "disposing a failed chat session");
        return;
    }
    emit_thinking(inner, scope, agent_id);

    let supports_follow_up = inner
        .catalog
        .get(&harness_id)
        .map(|a| a.caps().contains(bisa_core::HarnessCaps::FOLLOW_UP))
        .unwrap_or(false);
    let conv = Arc::new(ConvSession {
        turn_mark: Arc::new(std::sync::atomic::AtomicU64::new(now_secs())),
        agent_id: agent_uid,
        session_id,
        goal: facts.goal,
        session: Arc::new(Mutex::new(Some(session))),
        epoch: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        supports_follow_up,
        cwd,
        woken_by: std::sync::Mutex::new(on_behalf_of),
        review,
    });
    inner
        .conversation
        .sessions
        .insert(key.clone(), Arc::clone(&conv));
    spawn_reply_pump(
        inner,
        key,
        conv,
        events,
        ChatModel {
            harness: harness_id,
            model: model_key,
            in_flight,
        },
        attempts,
        judged,
    );
    // Stopped while it was being made — its row stood before its session
    // did: the stop found nothing to let go of, and is honoured here.
    if inner.registry.is_aborted(agent_uid) {
        stop_run(inner, agent_uid);
    }
}

/// What an origin is about, by name: a goal's title, a workflow's name.
fn origin_subject(inner: &Inner, origin: &ConversationOrigin) -> Option<String> {
    match origin {
        ConversationOrigin::Goal { id } => inner.ws.get_goal(*id).ok().map(|g| {
            g.title.clone().unwrap_or_else(|| {
                g.statement
                    .lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(60)
                    .collect()
            })
        }),
        ConversationOrigin::Workflow { id } => inner.ws.get_workflow(*id).ok().map(|w| w.name),
        ConversationOrigin::Drawing { id } => inner.ws.get_drawing(*id).ok().map(|d| d.title),
        ConversationOrigin::Note { id } => inner.ws.get_note(*id).ok().map(|n| n.title),
        _ => None,
    }
}

/// The `(harness, model)` a chat session is running on, so its pump can tell
/// the ledger how it went.
struct ChatModel {
    harness: String,
    model: String,
    in_flight: crate::models::InFlight,
}

/// A chat has no goal journal to write into (its scope is a channel, a
/// workstream, a goal thread), so a switch is reported on the bus and in the
/// log. The `ModelSwitched` event is what a surface renders.
fn report_chat_switches(
    inner: &Inner,
    scope: &str,
    agent_id: &str,
    walls: &[executor::ModelWall],
    next: Option<&str>,
) {
    for (i, wall) in walls.iter().enumerate() {
        let to = walls.get(i + 1).map(|w| w.model.as_str()).or(next);
        tracing::info!(
            agent = %agent_id, scope = %scope,
            "{} unavailable ({}); {}",
            wall.model, wall.reason,
            to.map(|t| format!("retrying on {t}"))
                .unwrap_or_else(|| "no model left in the plan".into())
        );
        if let Some(to) = to {
            inner.emit(EngineEvent::global(EnginePayload::ModelSwitched {
                work_item: None,
                from: wall.model.clone(),
                to: to.to_string(),
                reason: wall.reason.clone(),
                retry_in_secs: wall.cooldown_until.saturating_sub(now_secs()),
                after_progress: wall.after_progress,
            }));
        }
    }
}

fn emit_thinking(inner: &Arc<Inner>, scope: &str, agent_id: &str) {
    inner.emit(EngineEvent::global(EnginePayload::AgentThinking {
        scope: scope.to_string(),
        agent: agent_id.to_string(),
    }));
}

/// Own the session's event stream for its whole life: accumulate the text of
/// each turn and post it as the agent's reply when the turn ends. `judged`
/// is what the wake was told — kept for the relaunch a model wall asks for.
fn spawn_reply_pump(
    inner: &Arc<Inner>,
    key: (String, String),
    conv: Arc<ConvSession>,
    mut events: futures::stream::BoxStream<'static, SessionEvent>,
    running: ChatModel,
    attempts: Attempts,
    judged: crate::decider::Walk,
) {
    // Idle disposal is armed first: the pump owns its clones below.
    arm_idle_ttl(inner, key.clone(), Arc::clone(&conv));
    // What the pump owes if it unwinds: a session nobody reads any more is
    // let go of, the turn in flight is over, and the row says why — never a
    // conversation whose next message goes to a session with no reader.
    let unwound = {
        let (inner, key, run) = (Arc::clone(inner), key.clone(), conv.agent_id);
        move |reason: String| {
            inner.conversation.live.remove(&key);
            stop_run(&inner, run);
            inner.presence.ended(
                &inner,
                run,
                &crate::events::ExecutionOutcome::Failed { reason },
            );
            inner.emit(EngineEvent::global(EnginePayload::AgentReplied {
                scope: key.0,
                agent: key.1,
                posted: false,
                message: None,
            }));
        }
    };
    let inner = Arc::clone(inner);
    let work = async move {
        let (scope, agent_id) = key.clone();
        let mut said = Said::default();
        let mut streamer = Streamer::new();
        let mut turn_started = now_secs();
        // What the agent had said before this turn: a reply is posted only
        // when the turn added nothing to it through the post_message tool.
        // A set of ids, not a clock — two turns can end within one second.
        let mut spoken_before = agent_posts(&inner, &scope, &agent_id);
        let mut wall: Option<executor::ModelWall> = None;
        let mut progressed = false;
        loop {
            // The next event — or, while something gathered waits for a
            // frame, the interval's end: a frame is at most `STREAM_FLUSH`
            // late whatever the harness does next (13 § The reply streams).
            let event = tokio::select! {
                next = events.next() => match next {
                    Some(event) => event,
                    None => break,
                },
                _ = tokio::time::sleep_until(streamer.deadline()), if streamer.pending() => {
                    flush_stream(&inner, &scope, &agent_id, &mut streamer);
                    continue;
                }
            };
            inner.presence.apply(&inner, conv.agent_id, &event);
            inner.emit(EngineEvent::global(EnginePayload::Session {
                event: event.clone(),
            }));
            match &event {
                SessionEvent::Progress(ProgressEvent::TurnStarted) => {
                    said.clear();
                    streamer.reset();
                    inner.conversation.live.remove(&key);
                    turn_started = now_secs();
                    spoken_before = agent_posts(&inner, &scope, &agent_id);
                }
                // The harness stopped for an answer. A chat runs at the Exec
                // ceiling; what is above it — a question, a sign-in — goes to
                // the goal's Inbox when the scope is a goal, and is refused
                // when there is nobody to ask (`inputs.rs`).
                SessionEvent::Lifecycle(LifecycleEvent::InputRequested { request }) => {
                    // Decide without the session lock held: a guard question
                    // or a classifier call may take a while, and the person's
                    // next message needs the lock to reach this session. The
                    // lock is taken only for the answer itself.
                    let reach = conv.reach(&inner);
                    let ctx = crate::inputs::InputContext {
                        live_run: conv.agent_id,
                        home: conv.goal.map(bisa_core::Home::from),
                        work_item: None,
                        tier_ceiling: reach
                            .map(|r| r.mode.ceiling())
                            .unwrap_or(bisa_core::ToolTier::Exec),
                        agent: Some(agent_id.clone()),
                        cwd: Some(conv.cwd.clone()),
                        classifier: true,
                        on_behalf_of: conv.woken_by(),
                        above: crate::inputs::above_ceiling(&inner, conv.goal),
                        conversation: reach,
                    };
                    let answer = crate::inputs::decide(&inner, &ctx, request).await;
                    // The call runs next: the tracker reads what it is about
                    // to change before it does.
                    if let (Some((_, tracker)), bisa_harness::InputAnswer::Allow { .. }) =
                        (conv.review.as_ref(), &answer)
                    {
                        let (tracker, request) = (Arc::clone(tracker), request.clone());
                        crate::changes::off_thread(&inner, move |_| tracker.allowed(&request))
                            .await;
                    }
                    let guard = conv.session.lock().await;
                    if let Some(session) = guard.as_ref() {
                        crate::inputs::deliver(&inner, &ctx, session.as_ref(), request, answer)
                            .await;
                    }
                }
                SessionEvent::Progress(ProgressEvent::TextDelta { text })
                | SessionEvent::Progress(ProgressEvent::ThinkingDelta { text }) => {
                    progressed = true;
                    // A follow-up delivered while the harness stayed silent
                    // about turn boundaries starts a new turn here.
                    let mark = conv.turn_mark.load(std::sync::atomic::Ordering::SeqCst);
                    if mark > turn_started {
                        said.clear();
                        streamer.reset();
                        inner.conversation.live.remove(&key);
                        turn_started = mark;
                        spoken_before = agent_posts(&inner, &scope, &agent_id);
                    }
                    let thinking = matches!(
                        &event,
                        SessionEvent::Progress(ProgressEvent::ThinkingDelta { .. })
                    );
                    let mut live =
                        inner
                            .conversation
                            .live
                            .entry(key.clone())
                            .or_insert_with(|| LiveTurn {
                                since: turn_started,
                                ..LiveTurn::default()
                            });
                    said.hear(thinking, text);
                    if thinking {
                        gather_thinking(&mut live.thinking, text);
                    } else {
                        gather_words(&mut live.text, text);
                    }
                    drop(live);
                    if streamer.switches(thinking) {
                        flush_stream(&inner, &scope, &agent_id, &mut streamer);
                    }
                    if streamer.push(thinking, text) {
                        flush_stream(&inner, &scope, &agent_id, &mut streamer);
                    }
                }
                // The tool the agent runs, named on the live row while its
                // words wait — one line, never a transcript (ide/09).
                SessionEvent::Progress(ProgressEvent::ToolStarted {
                    name, args_summary, ..
                }) => {
                    set_working(
                        &inner,
                        &key,
                        turn_started,
                        Some(format!("{name} {args_summary}").trim().to_string()),
                    );
                    streamer.working_moved();
                    flush_stream(&inner, &scope, &agent_id, &mut streamer);
                }
                SessionEvent::Progress(ProgressEvent::ToolEnded { name, .. }) => {
                    set_working(&inner, &key, turn_started, None);
                    streamer.working_moved();
                    flush_stream(&inner, &scope, &agent_id, &mut streamer);
                    if let Some((_, tracker)) = conv.review.as_ref() {
                        let (tracker, name) = (Arc::clone(tracker), name.clone());
                        let recorded = crate::changes::off_thread(&inner, move |inner| {
                            tracker.tool_ended(inner, &name)
                        })
                        .await;
                        if let Some(Err(e)) = recorded {
                            tracing::warn!(agent = %agent_id, "a tool's changes were not recorded: {e}");
                        }
                    }
                }
                SessionEvent::Progress(ProgressEvent::TurnEnded) => {
                    flush_stream(&inner, &scope, &agent_id, &mut streamer);
                    // The turn is over before its reply lands: a reader who
                    // sees the message never also sees it in flight.
                    inner.conversation.live.remove(&key);
                    finish_turn(&inner, &scope, &agent_id, &mut said, &spoken_before).await;
                    end_tracked_turn(&inner, &conv, &scope, &agent_id, &spoken_before).await;
                }
                SessionEvent::Lifecycle(LifecycleEvent::Ended {
                    outcome,
                    is_terminal,
                }) => {
                    // A turn that ends without a TurnEnded still owes a reply;
                    // the turn is over before it lands, as above.
                    flush_stream(&inner, &scope, &agent_id, &mut streamer);
                    inner.conversation.live.remove(&key);
                    finish_turn(&inner, &scope, &agent_id, &mut said, &spoken_before).await;
                    end_tracked_turn(&inner, &conv, &scope, &agent_id, &spoken_before).await;
                    match outcome {
                        Outcome::Completed => {}
                        // The model died, not the conversation. Bury the pair
                        // and relaunch below on whatever the plan yields next.
                        Outcome::ModelUnavailable {
                            reason,
                            retry_after,
                            ..
                        } if *is_terminal => {
                            let until = inner.models.note_unavailable(
                                &running.harness,
                                &running.model,
                                *retry_after,
                            );
                            wall = Some(executor::ModelWall {
                                harness: running.harness.clone(),
                                model: running.model.clone(),
                                reason: reason.clone(),
                                retry_after: *retry_after,
                                cooldown_until: until,
                                after_progress: progressed,
                            });
                        }
                        other => tracing::warn!(agent = %agent_id, "chat turn ended: {other:?}"),
                    }
                    if *is_terminal {
                        break;
                    }
                }
                // A boundary — a tool, a sub-agent, a cost — lets what has
                // gathered go out rather than wait for the next word.
                _ => flush_stream(&inner, &scope, &agent_id, &mut streamer),
            }
        }
        // Stream closed: the session is gone. Its row says so — unless the
        // idle TTL took the session and parked it, in which case the stream
        // closing is the parking's consequence, and the parked row stands.
        inner.conversation.live.remove(&key);
        forget_session(&inner, &key, &conv);
        // Nobody is left to hear an answer: the session's asks are a no.
        if let Some((conversation, _)) = conv.review.as_ref() {
            crate::changes::asks::drop_asks_of(&inner, *conversation, &agent_id);
        }
        let parked = conv.session.lock().await.is_none();
        if !parked {
            crate::sessions::ended(&inner, &conv.session_id.to_string());
            if let Some(a) = inner.registry.get(conv.agent_id) {
                debug_on_err(
                    inner.registry.mutate(conv.agent_id, a.generation, |a| {
                        a.status = AgentStatus::Idle
                    }),
                    "idling a chat run",
                );
            }
            inner.presence.ended(
                &inner,
                conv.agent_id,
                &match &wall {
                    None => crate::events::ExecutionOutcome::Completed,
                    Some(w) => crate::events::ExecutionOutcome::Failed {
                        reason: w.sentence(),
                    },
                },
            );
        }
        drop(running.in_flight);
        match wall {
            None => inner.models.note_success(&running.harness, &running.model),
            Some(wall) => {
                if attempts.spent() {
                    report_chat_switches(&inner, &scope, &agent_id, &[wall], None);
                } else {
                    let for_whom = conv.woken_by();
                    relaunch_after_wall(inner, scope, agent_id, wall, attempts, judged, for_whom);
                }
            }
        }
    };
    crate::spawn_settling("reply pump", work, unwound);
}

/// Close the turn's record under the reply it posted. Idempotent: a turn
/// that ends twice — a `TurnEnded`, then an `Ended` — closes once.
async fn end_tracked_turn(
    inner: &Arc<Inner>,
    conv: &Arc<ConvSession>,
    scope: &str,
    agent_id: &str,
    spoken_before: &HashSet<String>,
) {
    let Some((_, tracker)) = conv.review.as_ref() else {
        return;
    };
    let said = agent_posts(inner, scope, agent_id);
    let reply = inner
        .ws
        .messages(scope, None, 10)
        .unwrap_or_default()
        .into_iter()
        .rev()
        .map(|m| m.id)
        .find(|id| said.contains(id) && !spoken_before.contains(id));
    let tracker = Arc::clone(tracker);
    let ended =
        crate::changes::off_thread(inner, move |inner| tracker.end_turn(inner, reply)).await;
    if let Some(Err(e)) = ended {
        tracing::warn!(agent = %agent_id, "a turn's changes were not closed: {e}");
    }
}

/// A plan on a harness the guard cannot stop is no plan: say so in the
/// conversation rather than run a turn that could change the checkout.
fn refuse_unguarded_plan(inner: &Arc<Inner>, scope: &str, agent: &bisa_core::Agent) {
    let text = format!(
        "This conversation is in plan mode, and I run on {}, which the platform cannot hold to reading. Switch the conversation to manual or auto, or address an agent on a harness the guard can stop.",
        agent.harness
    );
    match inner.ws.post_message(
        scope,
        MessageBody::post(text),
        None,
        &[],
        &[],
        Some(&agent.id),
        PostOrigin::Asked,
    ) {
        Ok(id) => inner.emit(EngineEvent::global(EnginePayload::AgentReplied {
            scope: scope.to_string(),
            agent: agent.id.to_string(),
            posted: true,
            message: Some(id),
        })),
        Err(e) => tracing::warn!(agent = %agent.id, "posting the plan refusal failed: {e}"),
    }
}

/// Come back into [`wake_attempt`] on the next model, with what the wake was
/// told: the lead and the judged level travel, so nobody is asked twice.
///
/// The hop through a boxed future is what keeps the type finite: a wake spawns
/// a pump, and a pump can spawn a wake, so the two futures would otherwise be
/// defined in terms of each other.
fn relaunch_after_wall(
    inner: Arc<Inner>,
    scope: String,
    agent_id: String,
    wall: executor::ModelWall,
    attempts: Attempts,
    judged: crate::decider::Walk,
    for_whom: Option<(bisa_core::PrincipalId, bisa_core::MemberRole)>,
) {
    tokio::spawn(async move {
        // The wall rides along unreported: only the next launch knows what it
        // switched *to*.
        let fut: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> =
            Box::pin(async move {
                wake_attempt(
                    &inner,
                    &scope,
                    &agent_id,
                    attempts,
                    vec![wall],
                    Some(judged),
                    for_whom,
                )
                .await;
            });
        fut.await;
    });
}

/// The words and the thinking gathered since the last frame go out as one
/// `AgentStreamed`, with the tool line as it stands now; nothing when
/// nothing gathered and the tool line did not move.
fn flush_stream(inner: &Arc<Inner>, scope: &str, agent_id: &str, streamer: &mut Streamer) {
    if let Some(Gathered { text, thinking }) = streamer.take() {
        let working = inner
            .conversation
            .live
            .get(&(scope.to_string(), agent_id.to_string()))
            .and_then(|turn| turn.working.clone());
        inner.emit(EngineEvent::global(EnginePayload::AgentStreamed {
            scope: scope.to_string(),
            agent: agent_id.to_string(),
            text,
            thinking,
            working,
        }));
    }
}

/// The live row's tool line — set when a tool starts, cleared when it ends.
/// Opens the turn's live entry when a tool runs before a word is said.
fn set_working(
    inner: &Arc<Inner>,
    key: &(String, String),
    turn_started: u64,
    working: Option<String>,
) {
    let mut live = inner
        .conversation
        .live
        .entry(key.clone())
        .or_insert_with(|| LiveTurn {
            since: turn_started,
            ..LiveTurn::default()
        });
    live.working = working;
}

/// The turn's words become the conversation's. A reply longer than one
/// message holds is said in as many as it takes, in order, the thinking
/// beside the first (`bisa_core::split_text`); one that was cut at the most a
/// reply keeps says so after its words, in the platform's own sentence. The
/// bus names the message the reply ended on — or none, when nothing landed,
/// so no timeline holds a turn that will never settle.
async fn finish_turn(
    inner: &Arc<Inner>,
    scope: &str,
    agent_id: &str,
    said: &mut Said,
    spoken_before: &HashSet<String>,
) {
    // The agent's words, whole: a token a delta boundary split is caught
    // here, where the reply becomes a message. The thinking rides beside
    // them, its tail within the bound, redacted the same way.
    let text = inner
        .security
        .redact_inbound(said.words.trim(), "chat_reply");
    let thinking = inner.security.redact_inbound(
        keep_tail(said.thinking.trim(), bisa_core::MAX_THINKING_BYTES),
        "chat_thinking",
    );
    let cut = said.cut;
    said.clear();
    if text.is_empty() {
        return;
    }
    // Dedupe: the agent may have answered through the post_message tool.
    if spoke_already(inner, scope, agent_id, spoken_before) {
        inner.emit(EngineEvent::global(EnginePayload::AgentReplied {
            scope: scope.to_string(),
            agent: agent_id.to_string(),
            posted: false,
            message: None,
        }));
        return;
    }
    let Ok(agent_key) = AgentId::new(agent_id) else {
        return;
    };
    let mut landed: Option<String> = None;
    let mut thinking = Some(thinking);
    for part in bisa_core::split_text(&text, bisa_core::MAX_TEXT_BYTES) {
        let body = match thinking.take() {
            Some(thinking) => MessageBody::post_with_thinking(part, thinking),
            None => MessageBody::post(part),
        };
        match inner.ws.post_message(
            scope,
            body,
            None,
            &[],
            &[],
            Some(&agent_key),
            PostOrigin::Asked,
        ) {
            Ok(id) => landed = Some(id),
            Err(e) => {
                tracing::warn!(agent = %agent_id, "posting chat reply failed: {e}");
                break;
            }
        }
    }
    if cut {
        let note = bisa_core::text!(
            "engine-conversation-reply-cut",
            kept = format!("{} MiB", MAX_REPLY_BYTES / (1024 * 1024))
        );
        match inner.ws.post_message(
            scope,
            MessageBody::said(note),
            None,
            &[],
            &[],
            Some(&agent_key),
            PostOrigin::Announced,
        ) {
            Ok(id) => landed = Some(id),
            Err(e) => tracing::warn!(agent = %agent_id, "saying a reply was cut failed: {e}"),
        }
    }
    inner.emit(EngineEvent::global(EnginePayload::AgentReplied {
        scope: scope.to_string(),
        agent: agent_id.to_string(),
        posted: landed.is_some(),
        message: landed,
    }));
}

/// Dispose every live session of a scope, marking each parked: the next
/// turn launches fresh and reads truth.
pub(crate) async fn dispose_live_sessions(inner: &Arc<Inner>, scope: &str) {
    let keys: Vec<(String, String)> = inner
        .conversation
        .sessions
        .iter()
        .filter(|e| e.key().0 == scope)
        .map(|e| e.key().clone())
        .collect();
    for key in keys {
        let Some((_, conv)) = inner.conversation.sessions.remove(&key) else {
            continue;
        };
        let mut guard = conv.session.lock().await;
        if let Some(session) = guard.take() {
            warn_on_err(session.dispose().await, "disposing a chat session");
        }
        drop(guard);
        park(inner, &conv);
    }
}

/// Take this session's own entry out of the map — never the session that
/// took its place under the same scope and agent since.
fn forget_session(inner: &Inner, key: &(String, String), conv: &Arc<ConvSession>) {
    inner
        .conversation
        .sessions
        .remove_if(key, |_, held| Arc::ptr_eq(held, conv));
}

/// Let go of the live session one run holds, if a conversation holds it —
/// what stopping that run's row does. The entry leaves at once, so the next
/// turn launches afresh; the session is disposed on a task of its own and
/// its durable row ended, never parked: a stopped session is revived by
/// nothing. The roster's row is the caller's to end.
pub(crate) fn stop_run(inner: &Arc<Inner>, run: LiveRunId) {
    let held = inner
        .conversation
        .sessions
        .iter()
        .find(|entry| entry.value().agent_id == run)
        .map(|entry| (entry.key().clone(), Arc::clone(entry.value())));
    let Some((key, conv)) = held else {
        return;
    };
    forget_session(inner, &key, &conv);
    let inner = Arc::clone(inner);
    tokio::spawn(async move {
        if let Some(session) = conv.session.lock().await.take() {
            warn_on_err(session.dispose().await, "disposing a stopped chat session");
        }
        crate::sessions::ended(&inner, &conv.session_id.to_string());
    });
}

/// A chat session is parked: the registry, the roster and the row agree.
fn park(inner: &Arc<Inner>, conv: &ConvSession) {
    if let Some(a) = inner.registry.get(conv.agent_id) {
        debug_on_err(
            inner.registry.mutate(conv.agent_id, a.generation, |a| {
                a.status = AgentStatus::Parked
            }),
            "parking a chat run",
        );
    }
    inner.presence.parked(inner, conv.agent_id);
    if let Err(e) = inner
        .ws
        .park_session(&conv.session_id.to_string(), now_secs())
    {
        tracing::debug!(session = %conv.session_id, "could not park the session row: {e}");
    }
}

/// The ids of this agent's standing posts among the newest messages of the
/// scope — the snapshot a turn starts from.
fn agent_posts(inner: &Arc<Inner>, scope: &str, agent_id: &str) -> HashSet<String> {
    let Some(def) = AgentId::new(agent_id)
        .ok()
        .and_then(|id| inner.ws.get_agent(&id).ok())
    else {
        return HashSet::new();
    };
    let pk = def.pubkey.as_hex().to_string();
    inner
        .ws
        .messages(scope, None, 10)
        .unwrap_or_default()
        .into_iter()
        .filter(|m| m.author == pk && !m.retracted)
        .map(|m| m.id)
        .collect()
}

/// Did this agent post into the scope during this turn — anything of its
/// that was not there when the turn started?
fn spoke_already(
    inner: &Arc<Inner>,
    scope: &str,
    agent_id: &str,
    spoken_before: &HashSet<String>,
) -> bool {
    agent_posts(inner, scope, agent_id)
        .iter()
        .any(|id| !spoken_before.contains(id))
}

fn arm_idle_ttl(inner: &Arc<Inner>, key: (String, String), conv: Arc<ConvSession>) {
    let inner = Arc::clone(inner);
    let ttl = inner.config.idle_ttl;
    tokio::spawn(async move {
        let mut seen = conv.epoch.load(std::sync::atomic::Ordering::SeqCst);
        loop {
            tokio::time::sleep(ttl).await;
            let now = conv.epoch.load(std::sync::atomic::Ordering::SeqCst);
            if now != seen {
                seen = now; // touched by a follow-up — wait another interval
                continue;
            }
            // Already let go of — stopped by a person, parked with its
            // scope: whoever took it said what became of it.
            let Some(session) = conv.session.lock().await.take() else {
                return;
            };
            warn_on_err(session.dispose().await, "disposing an idle chat session");
            forget_session(&inner, &key, &conv);
            park(&inner, &conv);
            return;
        }
    });
}
