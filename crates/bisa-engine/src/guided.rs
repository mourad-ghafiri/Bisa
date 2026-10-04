//! The design driver: the Workflow Agent's role over a goal it designs for
//! (an auto or a guided goal) — design the workflow the goal will run, and
//! propose the repair when a run fails. On a guided goal the person adopts;
//! on an auto goal the platform does (`ops::propose_workflow`); a manual goal
//! never wakes it, and asks it in the conversation instead.
//!
//! It is a *role, not a session*: it wakes as a fresh harness session at a
//! phase boundary and after each human answer, and its memory is the record:
//! the wake's prompt carries the goal, the staff, the connectors and the
//! templates (`wake_prompt`, built once per wake from truth), so the agent
//! fetches nothing it was handed and starts designing on its first turn.
//! This works on every harness — no resume-fidelity dependency. When the
//! previous wake's session is still live and supports follow-up, the answer
//! is delivered into it instead of a fresh wake (a transparent optimization
//! via the park/revive lifecycle).
//!
//! The driver **proposes**; who adopts is the goal's mode, decided in
//! `ops`. Nothing here records a decision or starts a run.

use crate::events::{EngineEvent, EnginePayload};
use crate::registry::{AgentRef, AgentStatus, LiveRunId, SessionKind};
use crate::{debug_on_err, executor, ops, warn_on_err, Inner};
use bisa_core::event::JournalPayload;
use bisa_core::{
    AgentId, Answer, Goal, GoalId, GuidancePhase, GuidanceStatus, Home, JournalEvent, MessageBody,
    RunOutcome, SessionId, StepId, StepState, Workflow, WorkflowRun,
};
use bisa_harness::{LifecycleEvent, Outcome, SessionEvent, SessionSpec};
use bisa_store::{PostOrigin, SessionRow, SessionStatus};
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// What the Workflow Agent is told on a fresh guided goal: the phase, and
/// what this wake's prompt already carries. The method itself — how to
/// design, staff, wire and place — is the agent's definition
/// (`library/core/workflow-agent.toml`), said once.
pub const DESIGN_DIRECTIVE: &str = "\
Phase: DESIGN. The blocks below — GOAL, STAFF, CONNECTORS, TEMPLATES — are \
this wake's facts, read from the record as you were launched; do not fetch \
them again. Start from the template that fits (get_workflow reads it), \
validate_workflow until it is clean, then propose_workflow. Say how the work \
begins: a `start` step for each way in — the one a person runs by hand \
always, and an event start only when GOAL says the work recurs or waits for \
something to happen (every Monday, whenever someone posts, when a run \
fails); the event is read in that start's `inputs` mapping and nowhere else. \
If an earlier question of yours was just answered, the answer is the last of \
the thread in GOAL; continue from there.";

/// What the Workflow Agent is told when a run failed: a finished run is not
/// amended — the repair is a new proposal.
pub const REPAIR_DIRECTIVE: &str = "\
Phase: REPAIR. A run of this goal's workflow failed at the step named below; \
the run is finished, so the repair is a new proposal with propose_workflow — \
change what caused the failure and keep the rest, and carry the failure into \
the corrected step's instructions so the next session does not repeat it. If \
the error says a step names no project and the goal has several, add an \
input of kind `project` and name it on that step. If the error begins \
`stalled:`, the graph itself is wrong — a step waited on a flow that could \
never settle — so rewire the flows; do not retry the same shape. If the error \
says a placeholder has no value, the shape is wrong, not the session: the \
reading step must run where the producer is sure to have run (a loop's first \
pass has nothing; give the second pass its own step), and the producer's \
output_schema must require every field a later step reads. Keep every \
assignee unless the failure was that nobody could take the step; then name \
another from STAFF. Ask nothing unless the failure cannot be understood from \
the record.";

/// What an auto goal adds: the run starts with nobody present, so nothing may
/// wait for a person that the workflow does not deliberately hand to one.
pub const AUTO_ADDENDUM: &str = "\
Mode: AUTO. Nobody adopts this design and nobody answers for it — the \
platform adopts what you propose and begins at once on the inputs' \
defaults: it runs a design that begins by hand, and listens for the events \
of one that begins on them. It arms alone only what nobody needs to see — a \
schedule, a signal, a run's end, a platform topic, a message; a hook, a \
check, a poll or a project start waits for a person's Adopt. Ask nothing: \
decide with sensible defaults and say your assumptions in the description. \
Give every input a default; put an `approval` step \
before a connector operation that writes — the validator refuses a write \
with no `approval` or `human` step upstream unless the step says \
`unattended: true`, which is the person's word to give, not yours — and a \
`human` step only where a person's judgement is genuinely the work. A step whose work runs commands \
gets `tier_ceiling: exec`: above the ceiling the classifier reads each call, \
not a person, so a low ceiling only slows the run.";

/// What a guided goal adds: the person adopts, so a question is affordable
/// but still rare — and this is the one place the wake teaches asking.
pub const GUIDED_ADDENDUM: &str = "\
Mode: GUIDED. The person reviews every step on a card and adopts it, asks \
you for changes, or edits it. Ask at most one round with ask_human, and only \
if the shape of the work is genuinely unclear — expects=\"answer\" (the \
default), with `options` when the answer is one of a few.";

/// Where the Workflow Agent itself stands: appended to every design or repair
/// prompt, because the session's cwd is the one folder in the goal that must
/// never receive a deliverable — and the one place a project comes from.
pub const DESIGN_PLACEMENT: &str = "\
You are running in this goal's scratch folder. It is not a repository and \
nothing commits it: you design here, you do not produce files here. A \
project for this goal is made only by create_project, once, and only when \
the goal's outcome is files and GOAL lists none.";

/// How many of the thread's latest posts the GOAL block carries.
const THREAD_TAIL: usize = 6;

#[derive(Debug, Clone)]
pub enum GuidedPhase {
    /// Propose the workflow for a goal that has none.
    Design,
    /// A run failed at `step`; propose the fix.
    Repair { step: StepId, error: String },
}

impl GuidedPhase {
    pub fn name(&self) -> &'static str {
        self.tag().as_str()
    }

    /// The phase as the journal records it.
    pub fn tag(&self) -> GuidancePhase {
        match self {
            GuidedPhase::Design => GuidancePhase::Design,
            GuidedPhase::Repair { .. } => GuidancePhase::Repair,
        }
    }
}

/// Per-goal guided coordination: one wake in flight, pending re-wakes,
/// the live-session handle for the follow-up optimization, and the last
/// standing this process recorded — what a settling wake reads to tell "the
/// turn ended after proposing" from "the turn ended with nothing".
#[derive(Default)]
pub struct GuidedState {
    waking: DashMap<GoalId, ()>,
    pending: DashMap<GoalId, GuidedPhase>,
    sessions: DashMap<GoalId, LiveRunId>,
    last: DashMap<GoalId, (GuidancePhase, GuidanceStatus)>,
    /// The stop of each wake in flight, by its run: what ending the run's
    /// row tells the wake's driver by ([`stop_run`]).
    driving: DashMap<LiveRunId, Arc<tokio::sync::Notify>>,
}

/// A wake's driver, reachable by its run for as long as it drives. Dropped
/// — the wake settled, walked to another model, or panicked — the run is
/// told nothing any more.
struct Driving {
    inner: Arc<Inner>,
    run: LiveRunId,
    stop: Arc<tokio::sync::Notify>,
}

impl Driving {
    fn begin(inner: &Arc<Inner>, run: LiveRunId) -> Self {
        let stop = Arc::new(tokio::sync::Notify::new());
        inner.guided.driving.insert(run, Arc::clone(&stop));
        Self {
            inner: Arc::clone(inner),
            run,
            stop,
        }
    }

    /// Resolves once the run was stopped; a stop that came before the driver
    /// listened is kept for it (`Notify` holds one permit).
    async fn stopped(&self) {
        self.stop.notified().await;
    }
}

impl Drop for Driving {
    fn drop(&mut self) {
        self.inner.guided.driving.remove(&self.run);
    }
}

/// Stop the Workflow Agent's session one run names — what ending its row
/// does. A wake in flight is told, and its driver aborts the harness on the
/// spot and records why the design ended; a session kept for a follow-up is
/// let go of. The roster's row is the caller's to end.
pub(crate) fn stop_run(inner: &Arc<Inner>, run: LiveRunId) {
    if let Some(stop) = inner.guided.driving.get(&run) {
        stop.notify_one();
        return;
    }
    inner.guided.sessions.retain(|_, held| *held != run);
    let inner = Arc::clone(inner);
    tokio::spawn(async move {
        inner.lifecycle.release(&inner, run).await;
    });
}

impl GuidedState {
    /// The goal is over: what this process remembered of its guidance —
    /// a wake in flight, a re-wake pending, the live session, the last
    /// standing — goes with it, so the maps are the live goals' alone.
    pub fn forget_goal(&self, goal: GoalId) {
        self.waking.remove(&goal);
        self.pending.remove(&goal);
        self.sessions.remove(&goal);
        self.last.remove(&goal);
    }

    /// Is a guided turn running on this goal *right now*?
    ///
    /// The conversation responder asks before waking the Workflow Agent in a
    /// goal thread: the two subsystems file their sessions under different
    /// keys and cannot see each other, so without this a chat wake would start
    /// a second session of the same agent on the same goal, both holding
    /// tools over the same state.
    ///
    /// `waking` and not `sessions`: the latter holds a session *parked* for a
    /// follow-up under an idle TTL, and treating parked as busy would defer a
    /// person's question until that TTL expired.
    pub fn busy(&self, goal: GoalId) -> bool {
        self.waking.contains_key(&goal)
    }

    /// A guided context owns the goal: a wake in flight, or a session parked
    /// for a follow-up. What decides whether a proposal or a question landing
    /// on the goal is the Workflow Agent's own guided work.
    pub fn owns(&self, goal: GoalId) -> bool {
        self.busy(goal) || self.sessions.contains_key(&goal)
    }

    /// The phase this process last recorded for the goal, if any.
    pub fn phase_of(&self, goal: GoalId) -> Option<GuidancePhase> {
        self.last.get(&goal).map(|e| e.0)
    }

    fn last_status(&self, goal: GoalId) -> Option<GuidanceStatus> {
        self.last.get(&goal).map(|e| e.1)
    }
}

// ---------------------------------------------------------------------------
// The lifecycle funnel
// ---------------------------------------------------------------------------

/// One transition of the Workflow Agent's standing on a goal.
pub(crate) struct Transition {
    pub phase: GuidancePhase,
    pub status: GuidanceStatus,
    pub detail: Option<String>,
    pub session: Option<SessionId>,
    /// What the Workflow Agent says in the goal's thread for this
    /// transition — a message of `locales/en/engine.ftl` (`guided-say-…`),
    /// posted as a note the reader renders (`MessageBody::said`); `None` is
    /// silence.
    pub say: Option<bisa_core::Text>,
}

/// **The one writer of the guided lifecycle.** A journal fact signed by the
/// Workflow Agent (durable, synced, replayable after a restart); the same
/// fact on the bus scoped to the goal (`Guided`); the presence every surface
/// already reads (`AgentThinking` while working, `AgentReplied` when the turn
/// is over); and, when the transition has words, a post into the goal's
/// thread as the agent — announced, so nothing triages it.
pub(crate) fn record(inner: &Arc<Inner>, goal: GoalId, t: Transition) {
    // What is written about the Workflow Agent's standing, and what it says
    // in the thread, carry the world's words — a harness's own, when a wake
    // failed; a name and a step list an agent wrote. Each is redacted once
    // here, before the journal and before the post: the sentence is the
    // catalog's, its arguments are not.
    let t = Transition {
        detail: t
            .detail
            .map(|d| inner.security.redact_inbound(&d, "guided_detail")),
        say: t
            .say
            .map(|said| inner.security.redact_said(said, "guided_say")),
        ..t
    };
    let (signer, attestation) = ops::signer_for(&inner.ws, Some(AgentId::WORKFLOW));
    warn_on_err(
        inner.ws.append_journal(
            &Home::Goal { goal },
            JournalPayload::Guidance {
                phase: t.phase,
                status: t.status,
                detail: t.detail.clone(),
                session: t.session,
            },
            &signer,
            attestation,
        ),
        "journaling the Workflow Agent's standing",
    );
    inner.emit(EngineEvent::scoped(
        goal,
        None,
        EnginePayload::Guided {
            phase: t.phase,
            status: t.status,
            detail: t.detail.clone(),
            session: t.session,
        },
    ));
    let scope = goal.to_string();
    match t.status {
        GuidanceStatus::Working => inner.emit(EngineEvent::scoped(
            goal,
            None,
            EnginePayload::AgentThinking {
                scope: scope.clone(),
                agent: AgentId::WORKFLOW.to_string(),
            },
        )),
        GuidanceStatus::Asking
        | GuidanceStatus::Proposed
        | GuidanceStatus::Stalled
        | GuidanceStatus::Failed
        | GuidanceStatus::Off => inner.emit(EngineEvent::scoped(
            goal,
            None,
            EnginePayload::AgentReplied {
                scope: scope.clone(),
                agent: AgentId::WORKFLOW.to_string(),
                posted: t.say.is_some(),
                message: None,
            },
        )),
        GuidanceStatus::Scheduled => {}
    }
    if let Some(text) = t.say {
        // The agent speaks for itself. When its definition cannot sign — the
        // very failure this line may be about — the platform says it in its
        // own name rather than leaving the thread silent, as `signer_for`
        // did for the journal fact above.
        let agent = AgentId::workflow();
        let speaker = inner.ws.signer_for(&agent).is_ok().then_some(&agent);
        if speaker.is_none() {
            tracing::warn!(%goal, "the Workflow Agent cannot sign; speaking as the owner");
        }
        if let Err(e) = inner.ws.post_message(
            &scope,
            MessageBody::said(text),
            None,
            &[],
            &[],
            speaker,
            PostOrigin::Announced,
        ) {
            tracing::warn!(%goal, "the Workflow Agent could not speak in the goal's thread: {e}");
        }
    }
    inner.guided.last.insert(goal, (t.phase, t.status));
}

/// What became of a proposal — the goal's mode decided it in `ops`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProposalFate {
    /// An Adopt or amend gate waits for the person; `why` is an auto goal's
    /// reason for asking after all.
    Gated { why: Option<String> },
    /// The platform adopted it and the run started, or the amendment applied.
    Adopted,
    /// The platform adopted it and the goal listens: its workflow begins on
    /// events, so nothing runs until one happens.
    Listening,
    /// Recorded as the person's own draft on the Workflow tab.
    Drafted,
}

/// A proposal landed. Recorded whether it came from a design wake or a chat
/// wake; spoken only when the design wake owns the goal — a chat wake's
/// reply pump already posted the agent's own words. One short line per
/// fate, the steps beneath: the card says the rest.
pub(crate) fn note_proposed(
    inner: &Arc<Inner>,
    goal: GoalId,
    wf: &Workflow,
    phase: GuidancePhase,
    fate: &ProposalFate,
) {
    let detail = proposal_detail(wf);
    // The lead is a message per phase and fate (`guided-say-proposed-…`); the
    // proposal's own words — its description and its steps — ride as one
    // argument, content the card shows in full anyway.
    let say = inner.guided.owns(goal).then(|| {
        let name = wf.name.clone();
        let steps = proposal_words(wf);
        match (phase, fate) {
            (GuidancePhase::Design, ProposalFate::Gated { why: None }) => {
                bisa_core::text!(
                    "guided-say-proposed-design-gated",
                    name = name,
                    steps = steps
                )
            }
            (GuidancePhase::Design, ProposalFate::Gated { why: Some(why) }) => bisa_core::text!(
                "guided-say-proposed-design-gated-why",
                name = name,
                why = why.clone(),
                steps = steps
            ),
            (GuidancePhase::Design, ProposalFate::Adopted) => {
                bisa_core::text!(
                    "guided-say-proposed-design-adopted",
                    name = name,
                    steps = steps
                )
            }
            (GuidancePhase::Design, ProposalFate::Listening) => {
                bisa_core::text!(
                    "guided-say-proposed-design-listening",
                    name = name,
                    steps = steps
                )
            }
            (GuidancePhase::Design, ProposalFate::Drafted) => {
                bisa_core::text!(
                    "guided-say-proposed-design-drafted",
                    name = name,
                    steps = steps
                )
            }
            (GuidancePhase::Repair, ProposalFate::Gated { why: None }) => {
                bisa_core::text!(
                    "guided-say-proposed-repair-gated",
                    name = name,
                    steps = steps
                )
            }
            (GuidancePhase::Repair, ProposalFate::Gated { why: Some(why) }) => bisa_core::text!(
                "guided-say-proposed-repair-gated-why",
                name = name,
                why = why.clone(),
                steps = steps
            ),
            (GuidancePhase::Repair, ProposalFate::Adopted) => {
                bisa_core::text!(
                    "guided-say-proposed-repair-adopted",
                    name = name,
                    steps = steps
                )
            }
            (GuidancePhase::Repair, ProposalFate::Listening) => {
                bisa_core::text!(
                    "guided-say-proposed-repair-listening",
                    name = name,
                    steps = steps
                )
            }
            (GuidancePhase::Repair, ProposalFate::Drafted) => {
                bisa_core::text!(
                    "guided-say-proposed-repair-drafted",
                    name = name,
                    steps = steps
                )
            }
        }
    });
    record(
        inner,
        goal,
        Transition {
            phase,
            status: GuidanceStatus::Proposed,
            detail: Some(detail),
            session: None,
            say,
        },
    );
}

/// The design in one line for the guidance card: the name, then the steps in
/// order — `Plan → Build → Review → Ship (4 steps)`.
pub(crate) fn proposal_detail(wf: &Workflow) -> String {
    let steps = wf.steps.len();
    format!(
        "{}: {} ({steps} step{})",
        wf.name,
        wf.steps
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>()
            .join(" → "),
        if steps == 1 { "" } else { "s" }
    )
}

/// What the agent says about its proposal in the goal's thread: the
/// description's first sentence, then every step with its one-line summary.
fn proposal_words(wf: &Workflow) -> String {
    let mut out = String::new();
    let about = bisa_core::workflow::first_sentence(&wf.description, 200);
    if !about.is_empty() {
        out.push_str(&about);
        out.push('\n');
    }
    for (i, s) in wf.steps.iter().enumerate() {
        out.push_str(&format!(
            "\n{}. **{}** ({}) — {}",
            i + 1,
            s.name,
            s.kind.as_str(),
            s.summary()
        ));
    }
    out
}

/// The Workflow Agent asked the person something during a guided wake.
pub(crate) fn note_asked(inner: &Arc<Inner>, goal: GoalId, question: &str) {
    if !inner.guided.owns(goal) {
        return;
    }
    let phase = inner.guided.phase_of(goal).unwrap_or(GuidancePhase::Design);
    let mut detail: String = question.chars().take(140).collect();
    if question.chars().count() > 140 {
        detail.push('…');
    }
    record(
        inner,
        goal,
        Transition {
            phase,
            status: GuidanceStatus::Asking,
            detail: Some(detail),
            session: None,
            say: None,
        },
    );
}

/// Designing is off on this node: say so on the goal, once, at capture.
pub(crate) fn note_off(inner: &Arc<Inner>, goal: GoalId) {
    record(
        inner,
        goal,
        Transition {
            phase: GuidancePhase::Design,
            status: GuidanceStatus::Off,
            detail: Some("designing is off on this node".into()),
            session: None,
            say: Some(bisa_core::text!("guided-say-design-off")),
        },
    );
}

/// Why a design cannot be asked for right now.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DesignRefusal {
    #[error(
        "goal {0} is manual; design it on the Workflow tab, or ask the Workflow Agent in the conversation"
    )]
    ManualGoal(GoalId),
    #[error("goal {0} is closed")]
    Closed(GoalId),
    #[error("goal {0} already has a workflow; adopt it, edit it, or clear it first")]
    HasWorkflow(GoalId),
    #[error(
        "goal {0} has a run; a workflow is designed before a run, and repaired after a failed one"
    )]
    HasRun(GoalId),
    #[error("the Workflow Agent is already working on goal {0}")]
    Busy(GoalId),
    #[error("designing is off on this node")]
    DesignOff,
}

/// A person asks for the design again — after a stall, a failure, or a
/// restart. Refused by name when there is nothing to design or someone is
/// already at it; otherwise a fresh design wake is scheduled.
pub fn request_design(inner: &Arc<Inner>, goal_id: GoalId) -> Result<(), crate::EngineError> {
    let goal = inner.ws.get_goal(goal_id)?;
    // Closed outranks the mode: a closed goal is refused as closed whatever
    // it was, because that is the one thing about it a person can still act on.
    if goal.is_closed() {
        return Err(DesignRefusal::Closed(goal_id).into());
    }
    if !goal.mode.designs() {
        return Err(DesignRefusal::ManualGoal(goal_id).into());
    }
    if goal.run.is_some() {
        return Err(DesignRefusal::HasRun(goal_id).into());
    }
    if goal.workflow.is_some() {
        return Err(DesignRefusal::HasWorkflow(goal_id).into());
    }
    if inner.guided.busy(goal_id) {
        return Err(DesignRefusal::Busy(goal_id).into());
    }
    if !inner.config.design_enabled {
        return Err(DesignRefusal::DesignOff.into());
    }
    schedule(inner, goal_id, GuidedPhase::Design);
    Ok(())
}

/// Where the Workflow Agent stands on a goal, for a screen: the last guidance
/// fact that still applies, and whether a wake is behind it right now.
#[derive(Clone, Debug, serde::Serialize, schemars::JsonSchema)]
pub struct DesignStatus {
    pub phase: GuidancePhase,
    pub status: GuidanceStatus,
    /// Unix seconds the fact was recorded.
    pub since: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<SessionId>,
    /// A wake is in flight on this node right now.
    pub live: bool,
}

/// The last guidance fact, if its phase still applies: a design while the
/// goal has no run, a repair while the current run has failed. A fact that
/// claims work in progress with no wake behind it reads as `Stalled` — the
/// safety net under [`resume_interrupted`].
pub fn design_status(
    inner: &Inner,
    goal: &Goal,
    run: Option<&WorkflowRun>,
    journal: &[JournalEvent],
) -> Option<DesignStatus> {
    design_status_from_journal(goal, run, journal, inner.guided.busy(goal.id))
}

/// [`design_status`] for a surface with no engine at hand (the CLI reading
/// the store): `live` is what that surface knows about a wake in flight —
/// nothing, so `false`, and a live-looking fact reads as stalled.
pub fn design_status_from_journal(
    goal: &Goal,
    run: Option<&WorkflowRun>,
    journal: &[JournalEvent],
    live: bool,
) -> Option<DesignStatus> {
    let last = journal.iter().rev().find_map(|e| match &e.payload {
        JournalPayload::Guidance {
            phase,
            status,
            detail,
            session,
        } => Some((e.at, *phase, *status, detail.clone(), *session)),
        _ => None,
    })?;
    let (since, phase, status, detail, session) = last;
    let applies = match phase {
        GuidancePhase::Design => goal.run.is_none() && !goal.is_closed(),
        GuidancePhase::Repair => run.is_some_and(|r| r.outcome == Some(RunOutcome::Failed)),
    };
    if !applies {
        return None;
    }
    let (status, detail) = if status.is_live() && !live {
        (
            GuidanceStatus::Stalled,
            Some("the Workflow Agent is not working on it right now".to_string()),
        )
    } else {
        (status, detail)
    };
    Some(DesignStatus {
        phase,
        status,
        since,
        detail,
        session,
        live,
    })
}

/// At boot: a guided goal whose last fact claims work in progress lost that
/// work with the process. Say so — `Stalled: interrupted by a restart` — and
/// wake again, so a restart is a delay and never a silent death.
pub fn resume_interrupted(inner: &Arc<Inner>) {
    let goals = match inner.ws.list_goals(None) {
        Ok(g) => g,
        Err(e) => {
            tracing::warn!("cannot list goals to resume guided work: {e}");
            return;
        }
    };
    for goal in goals
        .into_iter()
        .filter(|g| g.mode.designs() && !g.is_closed())
    {
        let Ok(journal) = inner.ws.journal(&Home::Goal { goal: goal.id }) else {
            continue;
        };
        let Some((phase, status)) = journal.iter().rev().find_map(|e| match &e.payload {
            JournalPayload::Guidance { phase, status, .. } => Some((*phase, *status)),
            _ => None,
        }) else {
            continue;
        };
        if !status.is_live() {
            continue;
        }
        let run = goal.run.and_then(|r| inner.ws.get_run(r).ok());
        let applies = match phase {
            GuidancePhase::Design => goal.workflow.is_none() && goal.run.is_none(),
            GuidancePhase::Repair => run
                .as_ref()
                .is_some_and(|r| r.outcome == Some(RunOutcome::Failed)),
        };
        if !applies {
            continue;
        }
        record(
            inner,
            goal.id,
            Transition {
                phase,
                status: GuidanceStatus::Stalled,
                detail: Some("interrupted by a restart".into()),
                session: None,
                say: None,
            },
        );
        match (phase, run) {
            (GuidancePhase::Design, _) => schedule(inner, goal.id, GuidedPhase::Design),
            (GuidancePhase::Repair, Some(run)) => notify_run_failed(inner, &run),
            (GuidancePhase::Repair, None) => {}
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Is the Workflow Agent this goal's to drive at all: designing is on, the
/// goal's mode designs, and the goal is open.
fn designing(inner: &Arc<Inner>, goal: &Goal) -> bool {
    inner.config.design_enabled && goal.mode.designs() && !goal.is_closed()
}

/// Does this phase still apply to the goal as it stands? Design is for a
/// goal with no workflow and no run — a workflow picked by hand, or a run
/// started, while the wake was queued makes the wake moot; repair is for a
/// goal whose current run failed. The goal is read once by the caller and
/// threaded through, so a wake reads its goal one time, not five.
fn applies(inner: &Arc<Inner>, goal: &Goal, phase: &GuidedPhase) -> bool {
    if !designing(inner, goal) {
        return false;
    }
    match phase {
        GuidedPhase::Design => goal.workflow.is_none() && goal.run.is_none(),
        GuidedPhase::Repair { .. } => goal
            .run
            .and_then(|r| inner.ws.get_run(r).ok())
            .is_some_and(|r| r.outcome == Some(RunOutcome::Failed)),
    }
}

/// A guided goal was captured without a workflow — wake the Workflow Agent to
/// design one.
pub fn notify_captured(inner: &Arc<Inner>, goal_id: GoalId) {
    schedule(inner, goal_id, GuidedPhase::Design);
}

/// A run finished `Failed` — wake the Workflow Agent to propose the repair.
/// Called by the effect interpreter; a manual goal ignores it.
pub fn notify_run_failed(inner: &Arc<Inner>, run: &WorkflowRun) {
    // A run of the workspace has no goal to repair: its workflow is the
    // person's to mend in the designer.
    let Some(goal) = run.scope.goal() else {
        return;
    };
    if let Some(phase) = repair_phase(run) {
        schedule(inner, goal, phase);
    }
}

/// The repair a failed run calls for: the step that failed last, with its
/// error. `None` for a run that did not fail.
fn repair_phase(run: &WorkflowRun) -> Option<GuidedPhase> {
    if run.outcome != Some(RunOutcome::Failed) {
        return None;
    }
    let (step, record) = run
        .steps
        .iter()
        .filter(|(_, r)| r.state == StepState::Failed)
        .max_by_key(|(_, r)| r.seq)?;
    Some(GuidedPhase::Repair {
        step: step.clone(),
        error: record
            .error
            .clone()
            .unwrap_or_else(|| "no error recorded".into()),
    })
}

/// A human answered the Workflow Agent's question. Deliver into the live
/// session when possible; otherwise wake fresh (the answer is in the journal).
///
/// `clarify_rounds_left` is present when the human said they were not sure
/// and a re-ask is still within the clarify budget (`Some(0)` is the last),
/// and it is what turns that from a dead end into a next move: the agent is
/// steered to ask something narrower, or — on the last round, or past the
/// budget (`None` beside an unsure answer) — to proceed on its own
/// recommendation and journal the assumption.
pub fn notify_answered(
    inner: &Arc<Inner>,
    goal_id: GoalId,
    answer: Option<Answer>,
    clarify_rounds_left: Option<u8>,
) {
    let Ok(goal) = inner.ws.get_goal(goal_id) else {
        return;
    };
    if !designing(inner, &goal) {
        return;
    }
    let inner = Arc::clone(inner);
    tokio::spawn(async move {
        if let Some(agent_id) = inner.guided.sessions.get(&goal_id).map(|a| *a) {
            let text = answer_steer(answer.as_ref(), clarify_rounds_left);
            if inner.lifecycle.follow_up(agent_id, &text).await {
                // Delivered into the live session: it is working again.
                let phase = inner
                    .guided
                    .phase_of(goal_id)
                    .unwrap_or(GuidancePhase::Design);
                record(
                    &inner,
                    goal_id,
                    Transition {
                        phase,
                        status: GuidanceStatus::Working,
                        detail: Some("the person answered".into()),
                        session: None,
                        say: None,
                    },
                );
                return;
            }
        }
        // A fresh wake in the phase the goal is in: an answer given during a
        // repair resumes the repair, not a design the goal has moved past.
        let repair = match inner.guided.phase_of(goal_id) {
            Some(GuidancePhase::Repair) => goal
                .run
                .and_then(|r| inner.ws.get_run(r).ok())
                .and_then(|run| repair_phase(&run)),
            _ => None,
        };
        schedule(&inner, goal_id, repair.unwrap_or(GuidedPhase::Design));
    });
}

/// What the Workflow Agent is told when its question comes back.
///
/// Shares its wording with the polling path in `bisa-mcp`'s
/// `ToolCore::await_human` on purpose: an agent that asked through the live
/// session and one that asked through a gate poll must be told the same thing,
/// or "I'm not sure" means two different things depending on how you waited.
pub(crate) fn answer_steer(answer: Option<&Answer>, clarify_rounds_left: Option<u8>) -> String {
    let Some(a) = answer else {
        return "The human decided your pending question — read the journal.".into();
    };
    if a.unsure {
        let said = a
            .text
            .as_deref()
            .map(|t| format!(" They added: {t}"))
            .unwrap_or_default();
        return match clarify_rounds_left {
            Some(0) | None => format!(
                "The human is not sure, and there are no clarification rounds \
                 left.{said} Proceed on your own best recommendation, and \
                 record the assumption you are making with add_note so it can \
                 be reviewed."
            ),
            Some(left) => format!(
                "The human is not sure — this is not a refusal.{said} Ask one \
                 narrower question, offering concrete options rather than \
                 asking them to reconsider the same thing. {left} \
                 clarification round(s) left before you must proceed on your \
                 own recommendation."
            ),
        };
    }
    let mut parts: Vec<String> = Vec::new();
    if !a.selected.is_empty() {
        parts.push(format!("They chose: {}.", a.selected.join(", ")));
    }
    if let Some(t) = a.text.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        parts.push(format!("They said: {t}"));
    }
    format!(
        "The human answered your question. {}",
        parts.join(" ").trim()
    )
}

/// Why the Workflow Agent could not be resolved: a workspace that is open has
/// the General Agent and the Workflow Agent by construction, so reaching this means its truth file is
/// unreadable. The sentence names the repair, because the consequence is the
/// owner's: this guided goal is not being driven until the file is fixed.
fn unresolved_agent_detail(agent_id: &str) -> String {
    format!(
        "the platform's own agent {agent_id:?} is missing or disabled — every workspace \
         creates it when it opens; if it is gone, its truth file under `agents/` is \
         unreadable. Repair it, or pick a workflow by hand."
    )
}

/// One wake in flight per goal; a notification during a wake queues exactly
/// one re-wake (debounce: later phases replace the pending one). Scheduling
/// is the first fact of the lifecycle: the goal's screen learns at once that
/// somebody is on it.
fn schedule(inner: &Arc<Inner>, goal_id: GoalId, phase: GuidedPhase) {
    // A phase that no longer applies — a design for a goal that already has
    // a workflow — is not scheduled at all: a `Scheduled` fact with no wake
    // behind it would read as *Stalled* on the goal.
    let Ok(goal) = inner.ws.get_goal(goal_id) else {
        return;
    };
    if !applies(inner, &goal, &phase) {
        return;
    }
    let Some(waking) = Waking::take(inner, goal_id) else {
        inner.guided.pending.insert(goal_id, phase);
        return;
    };
    record(
        inner,
        goal_id,
        Transition {
            phase: phase.tag(),
            status: GuidanceStatus::Scheduled,
            detail: None,
            session: None,
            say: None,
        },
    );
    let inner = Arc::clone(inner);
    tokio::spawn(async move {
        let tag = phase.tag();
        let woke = futures::FutureExt::catch_unwind(std::panic::AssertUnwindSafe(wake(
            &inner, goal_id, phase, waking,
        )))
        .await;
        if let Err(panic) = woke {
            // The wake is over either way; the goal must not read as
            // *working* on a cycle that is gone, and the person must be
            // told to ask again.
            let message = bisa_log::panic_message(&*panic);
            tracing::error!(target: "bisa_engine", goal = %goal_id, "the guided wake panicked: {message}");
            inner.guided.sessions.remove(&goal_id);
            record(
                &inner,
                goal_id,
                Transition {
                    phase: tag,
                    status: GuidanceStatus::Failed,
                    detail: Some(format!("the wake panicked: {message}")),
                    session: None,
                    say: Some(bisa_core::text!("guided-say-cycle-failed")),
                },
            );
        }
        // The mark went before the terminal fact was recorded (`wake` drops
        // it), or with the unwinding on a panic; the next cycle is scheduled
        // only now, whichever way this one ended.
        if let Some((_, next)) = inner.guided.pending.remove(&goal_id) {
            schedule(&inner, goal_id, next);
        }
        // A chat message that arrived while this cycle held the goal was
        // queued rather than woken, so that two sessions of the Workflow Agent
        // could not run on it at once. Nothing else will come along to release
        // it: this is the release. See `conversation::drain_deferred`.
        crate::conversation::drain_deferred(&inner, goal_id);
    });
}

/// One wake in flight per goal — the mark, held by the wake's task and
/// released when that task ends, however it ends. A mark that outlived a
/// panicked wake parked the goal for good and stranded every chat message
/// deferred behind it.
struct Waking {
    inner: Arc<Inner>,
    goal: GoalId,
}

impl Waking {
    /// `None` when a wake already holds the goal.
    fn take(inner: &Arc<Inner>, goal: GoalId) -> Option<Self> {
        if inner.guided.waking.insert(goal, ()).is_some() {
            return None;
        }
        Some(Self {
            inner: Arc::clone(inner),
            goal,
        })
    }
}

impl Drop for Waking {
    fn drop(&mut self) {
        self.inner.guided.waking.remove(&self.goal);
    }
}

/// Everything the Workflow Agent used to fetch, read once per wake from the
/// record: the agent's own definition, the phase and the goal's mode, the
/// GOAL block, the staff and connector rosters, the TEMPLATES block, where
/// the session stands, and the documents the person gave the goal. Built
/// before the launch walk, so every model attempt sends the same words.
fn wake_prompt(
    inner: &Arc<Inner>,
    goal: &Goal,
    phase: &GuidedPhase,
    system_prompt: &str,
) -> String {
    let addendum = match goal.mode {
        bisa_core::GoalMode::Auto => AUTO_ADDENDUM,
        bisa_core::GoalMode::Guided => GUIDED_ADDENDUM,
        bisa_core::GoalMode::Manual => "",
    };
    let directive = match phase {
        GuidedPhase::Design => format!("{DESIGN_DIRECTIVE}\n\n{addendum}"),
        GuidedPhase::Repair { step, error } => {
            format!("{REPAIR_DIRECTIVE}\n\n{addendum}\n\nFailed step: `{step}`\nError:\n{error}")
        }
    };
    // Who may be named on a step, and what a step may reach: read once per
    // wake, so the directive and `list_staff` / `list_connectors` cannot
    // disagree. The staff is the goal's — the agents and teams it names to
    // carry it, when it names any. A roster that cannot be read is said so,
    // not left blank.
    let roster = match crate::staff::StaffRoster::for_goal(&inner.ws, goal.id) {
        Ok(r) => r.render(),
        Err(e) => {
            tracing::warn!(goal = %goal.id, "guided wake: the staff roster is unavailable: {e}");
            "STAFF — unavailable; call list_staff.".to_string()
        }
    };
    let connectors = match crate::connectors::ConnectorRoster::of(&inner.ws) {
        Ok(r) => r.render(),
        Err(e) => {
            tracing::warn!(goal = %goal.id, "guided wake: the connector roster is unavailable: {e}");
            "CONNECTORS — unavailable; call list_connectors.".to_string()
        }
    };
    let mut blocks = vec![
        system_prompt.to_string(),
        directive,
        goal_brief(inner, goal),
        roster,
        connectors,
        templates_index(inner),
        DESIGN_PLACEMENT.to_string(),
    ];
    // The documents the person gave the goal are the design's brief when
    // there are any: the designer is told where they are.
    if let Some(note) = crate::documents::note(inner, goal.id) {
        blocks.push(note);
    }
    // The embedded browser, as every session is told of it (ide/18) — and
    // that its tabs are out of sight when the goal runs unattended.
    blocks.push(bisa_core::browser::browser_note(goal.mode.unattended()));
    blocks.join("\n\n")
}

/// The GOAL block: the title, the statement, the mode, the projects attached
/// with their paths — or that there are none, and what that means for a
/// step whose outcome is files — and the last few posts of the thread, so
/// an answered question is read here rather than fetched.
fn goal_brief(inner: &Arc<Inner>, goal: &Goal) -> String {
    let mut out = String::from("GOAL\n");
    if let Some(title) = goal.title.as_deref().filter(|t| !t.trim().is_empty()) {
        out.push_str(&format!("Title: {title}\n"));
    }
    out.push_str(&format!("Statement: {}\n", goal.statement.trim()));
    out.push_str(&format!("Mode: {}\n", goal.mode.as_str()));
    // A fact beside the mode when this machine develops for phones — never
    // a rule: the designer stays universal and reads what is here.
    if let Some(line) = crate::mobile_development::brief_line(inner) {
        out.push_str(&line);
        out.push('\n');
    }
    match inner.ws.projects_for(goal.id) {
        Ok(projects) if projects.is_empty() => out.push_str(
            "Projects: none — a step whose outcome is files needs one; create_project makes it.\n",
        ),
        Ok(projects) => {
            out.push_str("Projects:\n");
            for p in projects {
                let path = inner.ws.project_root_path(&p);
                out.push_str(&format!("- {} ({})\n", p.slug, path.display()));
            }
        }
        Err(e) => {
            tracing::warn!(goal = %goal.id, "guided wake: the goal's projects are unreadable: {e}");
            out.push_str("Projects: unreadable — get_goal lists them.\n");
        }
    }
    let names: std::collections::HashMap<String, String> = inner
        .ws
        .list_agents()
        .unwrap_or_default()
        .into_iter()
        .map(|a| (a.pubkey.as_hex().to_string(), a.name))
        .collect();
    let mut tail: Vec<bisa_store::MessageRow> = inner
        .ws
        .messages(&goal.id.to_string(), None, THREAD_TAIL)
        .unwrap_or_default()
        .into_iter()
        .filter(|m| m.body_kind == "post" && !m.retracted)
        .collect();
    tail.sort_by_key(|m| m.created_at);
    if !tail.is_empty() {
        out.push_str(&format!("Thread (last {}):\n", tail.len()));
        for m in tail {
            let who = names
                .get(&m.author)
                .cloned()
                .unwrap_or_else(|| "the person".to_string());
            // The person's own words entering a prompt: redacted like any
            // text that leaves for an agent, and cut so a pasted page does
            // not become the brief.
            let text = inner
                .security
                .redact_inbound(m.content.trim(), "guided_thread");
            let text: String = text.chars().take(300).collect();
            out.push_str(&format!("- {who}: {}\n", text.replace('\n', " ")));
        }
    }
    out.trim_end().to_string()
}

/// The TEMPLATES block: the catalog's workflow templates and this
/// workspace's own workflows, one line each — the shapes to adapt, read by
/// slug or id with get_workflow — and what each begins on.
fn templates_index(inner: &Arc<Inner>) -> String {
    let (templates, workflows) = match crate::intake::workflow_templates(inner) {
        Ok(x) => x,
        Err(e) => {
            tracing::warn!("guided wake: the templates are unreadable: {e}");
            return "TEMPLATES — unavailable; call list_workflow_templates.".to_string();
        }
    };
    // What each catalog template begins on, from the one rule the catalog
    // page reads (`starts_on`).
    let starts: std::collections::BTreeMap<String, Vec<String>> = bisa_store::CATALOG
        .describe()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|d| match d.detail {
            bisa_store::CatalogDetail::Workflow { starts_on, .. } => Some((d.slug, starts_on)),
            _ => None,
        })
        .collect();
    let mut out =
        String::from("TEMPLATES — adapt the closest; get_workflow reads one by slug or id.\n");
    for t in &templates {
        let on = starts
            .get(&t.slug)
            .map(|on| on.join(", "))
            .unwrap_or_else(|| "manual".to_string());
        out.push_str(&format!(
            "- catalog {} — {}: {} (starts on: {on})\n",
            t.slug,
            t.name,
            bisa_core::workflow::first_sentence(&t.description, 160)
        ));
    }
    for w in &workflows {
        let mut on: Vec<&str> = w
            .steps
            .iter()
            .filter_map(|s| s.start_on())
            .map(|o| o.as_str())
            .collect();
        if on.is_empty() {
            on.push("manual");
        }
        out.push_str(&format!(
            "- workflow {} — {}: {} ({} steps; starts on: {})\n",
            w.id,
            w.name,
            bisa_core::workflow::first_sentence(&w.description, 160),
            w.steps.len(),
            on.join(", ")
        ));
    }
    out.trim_end().to_string()
}

/// Build the guided session's spec, or say why not.
///
/// The one thing here that can fail is the placement. The Workflow Agent runs
/// in the goal's own `scratch/` — it designs there and produces no files; the
/// design session used to run in the
/// workspace root, which put the journal it is supposed to append to, the
/// index and the key files inside the directory it was told to work in. If
/// that directory cannot be created, the honest answer is not to launch
/// somewhere else: refusing the wake costs one cycle, and widening the
/// placement costs whatever the session decides to touch.
fn build_guided_spec(inner: &Inner, goal_id: GoalId) -> std::io::Result<SessionSpec> {
    let command = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "bisa".into());
    let paths = inner.ws.paths().goal(goal_id);
    let (scratch, tmp) = (paths.scratch(), paths.tmp());
    std::fs::create_dir_all(&tmp)?;
    // Placement, not a sandbox: a tool that honours `TMPDIR` scratches
    // inside the goal rather than in the system temp directory — and the
    // proxy the platform follows rides beside it.
    let session_env = crate::network::session_env(
        inner,
        std::collections::BTreeMap::from([("TMPDIR".to_string(), tmp.display().to_string())]),
    );
    Ok(SessionSpec {
        work_item: None,
        cwd: scratch,
        prompt: String::new(),
        // Chosen per attempt by the executor's launch walk, from the Workflow
        // Agent's own plan against the live health ledger.
        model: None,
        // And the effort with it, fitted to that model.
        effort: None,
        mcp_servers: vec![bisa_harness::McpMount::platform(
            bisa_harness::McpServerConfig::Stdio {
                name: "bisa".into(),
                command,
                args: vec![
                    "mcp".into(),
                    "--socket".into(),
                    inner.socket_path.display().to_string(),
                    "--goal".into(),
                    goal_id.to_string(),
                    // The session says who it is. Without this the MCP server
                    // cannot tell the Workflow Agent from a worker, so it would
                    // offer neither the designing tool set nor the ops that are
                    // the Workflow Agent's alone — and the intake socket, which any
                    // session knowing its path can reach, would be the only check
                    // left.
                    "--agent".into(),
                    AgentId::WORKFLOW.to_string(),
                ],
                // Where the MCP server writes its own log: the parent's folder,
                // named — it is spawned without `--data-dir` and must not guess.
                env: crate::logging::child_env(inner),
                cwd: None,
            },
        )],
        env: session_env.0,
        env_remove: session_env.1,
        tier_ceiling: bisa_core::ToolTier::Read,
        output_schema: None,
        skills: vec![],
    })
}

/// How a wake ended, for the driver to record. `Proposed` and `Asked` were
/// recorded where they happened (the proposal, the question); `Moot` records
/// nothing, because a hand-picked workflow already changed the page.
enum WakeEnd {
    Moot,
    Proposed,
    Asked,
    Stalled(String),
    Failed(String),
}

/// How a session that ended short of a turn says so — in the harness's own
/// words where it gave any, never as a value printed for a developer.
fn ended_words(outcome: &Outcome) -> String {
    let why = match outcome {
        Outcome::Failed { error } => error.trim().trim_end_matches('.').to_string(),
        Outcome::Suspended { reason } => {
            format!("suspended — {}", reason.trim().trim_end_matches('.'))
        }
        Outcome::Aborted => "it was stopped".to_string(),
        Outcome::Completed | Outcome::ModelUnavailable { .. } => {
            "it ended without a turn".to_string()
        }
    };
    format!("the session ended: {why}.")
}

/// The words for a wake that did not end in a proposal.
fn end_words(phase: GuidancePhase, end: &WakeEnd) -> Option<bisa_core::Text> {
    let phase = match phase {
        GuidancePhase::Design => "design",
        GuidancePhase::Repair => "repair",
    };
    match end {
        WakeEnd::Stalled(detail) => Some(bisa_core::text!(
            "guided-say-stalled",
            phase = phase,
            detail = detail.clone()
        )),
        WakeEnd::Failed(detail) => Some(bisa_core::text!(
            "guided-say-failed",
            phase = phase,
            detail = detail.clone()
        )),
        WakeEnd::Moot | WakeEnd::Proposed | WakeEnd::Asked => None,
    }
}

async fn wake(inner: &Arc<Inner>, goal_id: GoalId, phase: GuidedPhase, waking: Waking) {
    let tag = phase.tag();
    let end = run_wake(inner, goal_id, &phase).await;
    // The wake is over before the fact that says so is written: a reader
    // must never see `status: failed` beside `live: true`.
    drop(waking);
    let status = match &end {
        WakeEnd::Moot | WakeEnd::Proposed | WakeEnd::Asked => return,
        WakeEnd::Stalled(_) => GuidanceStatus::Stalled,
        WakeEnd::Failed(_) => GuidanceStatus::Failed,
    };
    let detail = match &end {
        WakeEnd::Stalled(d) | WakeEnd::Failed(d) => Some(d.clone()),
        _ => None,
    };
    tracing::warn!(goal = %goal_id, "guided wake {}: {}", status.as_str(), detail.as_deref().unwrap_or(""));
    record(
        inner,
        goal_id,
        Transition {
            phase: tag,
            status,
            detail,
            session: None,
            say: end_words(tag, &end),
        },
    );
}

/// The wake itself: launch the Workflow Agent, prompt it, drive the turn to
/// its end. Every way out is a [`WakeEnd`] the driver records — no exit is a
/// log line alone.
async fn run_wake(inner: &Arc<Inner>, goal_id: GoalId, phase: &GuidedPhase) -> WakeEnd {
    // The goal, read once: the phase check, the mode and the GOAL block all
    // read this one record.
    let goal = match inner.ws.get_goal(goal_id) {
        Ok(g) => g,
        Err(e) => return WakeEnd::Failed(format!("the goal cannot be read: {e}")),
    };
    if !applies(inner, &goal, phase) {
        return WakeEnd::Moot;
    }
    inner.pause.wait_running().await;
    let tag = phase.tag();

    // The driver is the platform's Workflow Agent, and nothing else. There is
    // no hidden prompt behind it and no configurable substitute: if it cannot
    // be resolved, say so on the goal and leave it for the human. Substituting
    // an unattested prompt with no model plan is how this went unnoticed for
    // two milestones.
    let Some(info) = ops::agent_info(&inner.ws, &AgentId::workflow()) else {
        return WakeEnd::Failed(unresolved_agent_detail(AgentId::WORKFLOW));
    };
    let system_prompt = info.system_prompt;
    let models = info.models;
    // The agent's own harness, and only that. Where it runs is a property of
    // the agent — editable on its definition, which is where an owner looks
    // for it — rather than a second list in engine config that could disagree
    // with it.
    let candidates = info.harness;
    let spec = match build_guided_spec(inner, goal_id) {
        Ok(spec) => spec,
        Err(e) => {
            return WakeEnd::Failed(format!("cannot create the goal's work directory: {e}."));
        }
    };
    // Everything the agent used to fetch, read once per wake from the
    // record — and once for every model attempt, so a relaunch after a wall
    // sends the same words.
    let base_prompt = wake_prompt(inner, &goal, phase, &system_prompt);
    // A wake stands in the goal's scratch folder, which is no project's: the
    // workspace's setting.
    let effort_setting = crate::effort::setting(inner, None);
    let rotation = inner.models.next_rotation();
    // Asked once for the wake: the model that leads, and the level of every
    // attempt that comes to `auto`. The loop below keeps both across a wall.
    let judged = crate::decider::walk(
        inner,
        &crate::decider::WalkAsk {
            plan: &models,
            candidates: &candidates,
            model_pin: None,
            effort_pin: None,
            effort_setting,
            rotation,
            task: &goal.statement,
            agent: Some(AgentId::WORKFLOW),
        },
        &crate::decider::Standing {
            home: Some(Home::Goal { goal: goal_id }),
            agent: Some(AgentId::WORKFLOW.to_string()),
            ..Default::default()
        },
    )
    .await;
    let launch_plan = executor::LaunchPlan {
        candidates: &candidates,
        plan: &models,
        pin: None,
        lead: judged.lead.as_deref(),
        effort_pin: None,
        effort_setting,
        judged_effort: judged.effort,
        rotation,
        skills_in_prompt: false,
    };
    let mut tried = std::collections::HashSet::new();
    let mut attempts = executor::Attempts::new(inner.config.max_model_attempts);
    // The Workflow Agent signs its own wake, carrying the owner attestation, in
    // exactly the way `run_work_item` signs a work item. Signing as the owner
    // here is what made a guided note read as something the human wrote.
    let (signer, attestation) = ops::signer_for(&inner.ws, Some(AgentId::WORKFLOW));
    let mut pending_walls: Vec<executor::ModelWall> = Vec::new();

    // A guided wake walks the plan exactly the way a work item does. It has no
    // workstream and no result contract, so the loop is the bare shape: launch,
    // prompt, drive; a model wall relaunches, anything else settles the wake.
    let (harness_id, session, session_row, agent_id, end, aborted) = loop {
        let launched = match executor::resolve_and_launch(
            inner,
            &launch_plan,
            &mut tried,
            &mut attempts,
            &spec,
        )
        .await
        {
            Ok(ok) => ok,
            Err(failure) => {
                pending_walls.extend(failure.walls);
                executor::report_switches(
                    inner,
                    (
                        Home::Goal { goal: goal_id },
                        crate::events::EventScope::of_goal(goal_id),
                    ),
                    None,
                    &pending_walls,
                    None,
                    &signer,
                    attestation.clone(),
                );
                return WakeEnd::Failed(format!("{}.", failure.message.trim_end_matches('.')));
            }
        };
        pending_walls.extend(launched.walls.iter().cloned());
        executor::report_switches(
            inner,
            (
                Home::Goal { goal: goal_id },
                crate::events::EventScope::of_goal(goal_id),
            ),
            None,
            &pending_walls,
            Some(&launched.model_key),
            &signer,
            attestation.clone(),
        );
        pending_walls.clear();
        let harness_id = launched.harness.clone();
        let model_key = launched.model_key.clone();
        let effort = launched.effort;
        let skill_appendix = launched.skill_appendix;
        let in_flight = launched.in_flight;
        let session = launched.session;

        // Register the wake in the runtime roster, then say it is working —
        // with the session's id, so a screen can follow it.
        let agent_id = LiveRunId::mint();
        let driving = Driving::begin(inner, agent_id);
        let session_id = SessionId::from_ulid(ulid::Ulid::from_datetime(SystemTime::now()));
        let transcript = session.resume_token().and_then(|t| t.transcript_path);
        debug_on_err(
            inner.registry.register_if(
                AgentRef {
                    id: agent_id,
                    kind: SessionKind::Guided,
                    status: AgentStatus::Running,
                    generation: 1,
                    session_id: Some(session_id),
                    work_item: None,
                    conversation: None,
                    goal: Some(goal_id),
                    workstream: None,
                    transcript_path: transcript.clone(),
                    last_activity: now_secs(),
                },
                None,
            ),
            "registering a guided run",
        );
        inner.presence.register(
            inner,
            agent_id,
            crate::presence::SessionMeta {
                kind: SessionKind::Guided,
                harness: harness_id.clone(),
                model: Some(model_key.clone()),
                effort,
                agent: Some(AgentId::workflow()),
                session_id: Some(session_id),
                work_item: None,
                conversation: None,
                goal: Some(goal_id),
                run: None,
                workstream: None,
                project: None,
                transcript_path: transcript.as_ref().map(|p| p.display().to_string()),
            },
        );
        let session_row = SessionRow {
            id: session_id.to_string(),
            adapter: harness_id.clone(),
            kind: SessionKind::Guided,
            conversation: None,
            work_item: None,
            workstream: None,
            agent_id: Some(AgentId::WORKFLOW.to_string()),
            transcript_path: transcript.map(|p| p.display().to_string()),
            resume_token_json: session
                .resume_token()
                .and_then(|t| serde_json::to_string(&t).ok()),
            status: SessionStatus::Live,
            parked_at: None,
            pid: None,
            pid_seen_at: None,
            ended_at: None,
        };
        warn_on_err(
            inner.ws.record_session(&session_row),
            "recording a guided session",
        );
        // Silent: the card's *working* state says it, and a line here on
        // every goal was the chatter the person read past.
        record(
            inner,
            goal_id,
            Transition {
                phase: tag,
                status: GuidanceStatus::Working,
                detail: Some(format!("on {harness_id}")),
                session: Some(session_id),
                say: None,
            },
        );

        let mut events = session.subscribe();
        // Skills the harness cannot host natively ride the first prompt.
        let prompt = match &skill_appendix {
            Some(appendix) => format!("{base_prompt}\n\n{appendix}"),
            None => base_prompt.clone(),
        };
        if let Err(e) = session.prompt(prompt.as_str().into()).await {
            warn_on_err(session.dispose().await, "disposing a failed guided session");
            return WakeEnd::Failed(format!("the session refused the prompt: {e}."));
        }

        // Drive to settlement: a guided wake has no output contract, so the first
        // completed turn end (terminal or not) settles the wake.
        let timeout = Duration::from_secs(inner.config.guided_wake_timeout_secs);
        let started = SystemTime::now();
        let mut wall: Option<executor::ModelWall> = None;
        let mut progressed = false;
        let mut end: Option<WakeEnd> = None;
        // Whether the session was aborted here — by the clock, or by whoever
        // stopped its row: an aborted session takes no follow-up.
        let mut aborted = false;
        loop {
            let remaining = timeout
                .checked_sub(started.elapsed().unwrap_or_default())
                .unwrap_or(Duration::ZERO);
            let next = tokio::select! {
                biased;
                () = driving.stopped() => {
                    warn_on_err(session.abort().await, "aborting a stopped guided wake");
                    aborted = true;
                    end = Some(WakeEnd::Failed(ended_words(&Outcome::Aborted)));
                    break;
                }
                next = tokio::time::timeout(remaining, futures::StreamExt::next(&mut events)) => next,
            };
            let event = match next {
                Err(_) => {
                    warn_on_err(session.abort().await, "aborting a timed-out guided wake");
                    aborted = true;
                    end = Some(settle_end(
                        inner,
                        goal_id,
                        format!("no proposal after {}s", timeout.as_secs()),
                    ));
                    break;
                }
                Ok(None) => {
                    end = Some(settle_end(
                        inner,
                        goal_id,
                        "the session ended without proposing a workflow".into(),
                    ));
                    break;
                }
                Ok(Some(ev)) => ev,
            };
            inner.presence.apply(inner, agent_id, &event);
            inner.emit(EngineEvent::scoped(
                goal_id,
                None,
                EnginePayload::Session {
                    event: event.clone(),
                },
            ));
            if matches!(event, SessionEvent::Progress(_)) {
                progressed = true;
            }
            if let SessionEvent::Lifecycle(LifecycleEvent::InputRequested { request }) = &event {
                crate::inputs::answer_request(
                    inner,
                    crate::inputs::InputContext {
                        live_run: agent_id,
                        home: Some(Home::Goal { goal: goal_id }),
                        work_item: None,
                        tier_ceiling: bisa_core::ToolTier::Exec,
                        agent: Some(AgentId::WORKFLOW.to_string()),
                        cwd: Some(inner.ws.paths().goal(goal_id).scratch()),
                        classifier: true,
                        on_behalf_of: None,
                        above: crate::inputs::above_ceiling(inner, Some(goal_id)),
                        conversation: None,
                    },
                    session.as_ref(),
                    request,
                )
                .await;
            }
            if let SessionEvent::Lifecycle(LifecycleEvent::Ended { outcome, .. }) = &event {
                match outcome {
                    Outcome::Completed => {
                        end = Some(settle_end(
                            inner,
                            goal_id,
                            "the session ended its turn without proposing a workflow".into(),
                        ));
                    }
                    Outcome::ModelUnavailable {
                        reason,
                        retry_after,
                        ..
                    } => {
                        let until =
                            inner
                                .models
                                .note_unavailable(&harness_id, &model_key, *retry_after);
                        wall = Some(executor::ModelWall {
                            harness: harness_id.clone(),
                            model: model_key.clone(),
                            reason: reason.clone(),
                            retry_after: *retry_after,
                            cooldown_until: until,
                            after_progress: progressed,
                        });
                    }
                    other => {
                        end = Some(WakeEnd::Failed(ended_words(other)));
                    }
                }
                break;
            }
        }

        drop(in_flight);
        match wall {
            None => {
                inner.models.note_success(&harness_id, &model_key);
                let end = end.unwrap_or_else(|| {
                    settle_end(
                        inner,
                        goal_id,
                        "the session ended without proposing a workflow".into(),
                    )
                });
                break (harness_id, session, session_row, agent_id, end, aborted);
            }
            Some(wall) => {
                // The model died, not the wake. Drop this session and come round
                // again on whatever the plan yields next.
                if let Some(agent) = inner.registry.get(agent_id) {
                    debug_on_err(
                        inner
                            .registry
                            .mutate(agent_id, agent.generation, |a| a.status = AgentStatus::Idle),
                        "idling a guided run",
                    );
                }
                warn_on_err(session.dispose().await, "disposing a walled guided session");
                inner.presence.forget(inner, agent_id);
                pending_walls.push(wall);
                if attempts.spent() {
                    executor::report_switches(
                        inner,
                        (
                            Home::Goal { goal: goal_id },
                            crate::events::EventScope::of_goal(goal_id),
                        ),
                        None,
                        &pending_walls,
                        None,
                        &signer,
                        attestation.clone(),
                    );
                    return WakeEnd::Failed(
                        "every model in the Workflow Agent's plan is unavailable right now.".into(),
                    );
                }
            }
        }
    };

    // Keep the session available for the follow-up optimization when the
    // adapter supports it; otherwise dispose. NOTE: an adopted session has no
    // event consumer — its work still lands through the intake socket, and
    // the next fresh wake re-reads truth.
    if let Some(agent) = inner.registry.get(agent_id) {
        debug_on_err(
            inner
                .registry
                .mutate(agent_id, agent.generation, |a| a.status = AgentStatus::Idle),
            "idling a guided run",
        );
    }
    let supports_follow_up = inner
        .catalog
        .get(&harness_id)
        .map(|a| a.caps().contains(bisa_core::HarnessCaps::FOLLOW_UP))
        .unwrap_or(false);
    // A session is kept for the follow-up only while it can take one: not
    // once it was aborted, here or by whoever stopped its row.
    let adopted = supports_follow_up && !aborted && !inner.registry.is_aborted(agent_id);
    // Presence: an adopted session waits for the follow-up and reads as idle
    // (parked once the TTL runs out); a disposed one is over.
    match &end {
        WakeEnd::Stalled(d) | WakeEnd::Failed(d) => inner.presence.ended(
            inner,
            agent_id,
            &crate::events::ExecutionOutcome::Failed { reason: d.clone() },
        ),
        WakeEnd::Asked | WakeEnd::Moot | WakeEnd::Proposed => {
            if !adopted {
                inner
                    .presence
                    .ended(inner, agent_id, &crate::events::ExecutionOutcome::Completed);
            }
        }
    }
    if adopted {
        inner.guided.sessions.insert(goal_id, agent_id);
        inner.lifecycle.adopt(
            inner,
            agent_id,
            session,
            session_row.id.clone(),
            inner.config.idle_ttl,
        );
        // Stopped as it was being kept: a stop that found no slot yet is
        // honoured here, so no session outlives its row's end.
        if inner.registry.is_aborted(agent_id) {
            inner.guided.sessions.retain(|_, held| *held != agent_id);
            inner.lifecycle.release(inner, agent_id).await;
        }
    } else {
        warn_on_err(session.dispose().await, "disposing a guided session");
        crate::sessions::ended(inner, &session_row.id);
    }
    end
}

/// The turn is over: what did it leave behind? A proposal already recorded
/// is a proposal; a question already recorded is a wake parked on the
/// person — not a stall; anything else is a stall with `otherwise` as the
/// reason.
fn settle_end(inner: &Arc<Inner>, goal_id: GoalId, otherwise: String) -> WakeEnd {
    match inner.guided.last_status(goal_id) {
        Some(GuidanceStatus::Proposed) => WakeEnd::Proposed,
        Some(GuidanceStatus::Asking) => WakeEnd::Asked,
        _ => WakeEnd::Stalled(otherwise),
    }
}

#[cfg(test)]
mod state_tests {
    use super::*;

    #[test]
    fn a_closed_goal_leaves_nothing_in_the_guided_state() {
        let state = GuidedState::default();
        let goal = GoalId::from_ulid(ulid::Ulid::from_parts(5, 5));
        let other = GoalId::from_ulid(ulid::Ulid::from_parts(6, 6));
        state.waking.insert(goal, ());
        state.pending.insert(goal, GuidedPhase::Design);
        state
            .last
            .insert(goal, (GuidancePhase::Design, GuidanceStatus::Working));
        state
            .last
            .insert(other, (GuidancePhase::Design, GuidanceStatus::Proposed));
        state.forget_goal(goal);
        assert!(!state.waking.contains_key(&goal));
        assert!(!state.pending.contains_key(&goal));
        assert!(!state.sessions.contains_key(&goal));
        assert!(!state.last.contains_key(&goal));
        assert!(
            state.last.contains_key(&other),
            "another goal's standing stays"
        );
    }
}
