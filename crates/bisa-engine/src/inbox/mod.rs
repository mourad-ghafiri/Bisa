//! The Inbox: a list you keep, not a query that forgets you — what is owed
//! to you, and what happened to what you asked for.
//!
//! Rows are **things** — a goal, a channel, a direct channel, a conversation,
//! a workstream, a project, a workflow (`InboxKind`) — keyed by their own id
//! and mutated in place; never an event. A row carries two kinds of fact:
//! its **asks** (`needs_action` — a gate or a question, answered here) and
//! its **notices** (`notices` — a run finished or failed, a step blocked, a
//! budget spent, a design stalled, a listener that could not be armed or
//! could not start its run, a committer wanted, a script that failed, a pull
//! request opened or merged, a command refused, a folder a step made, a
//! workflow the Workflow Agent designed or proposed or one put away —
//! `bisa_engine::notices` names the list once). A listener has no row of its
//! own: its trouble is a notice on its host's — the workflow's, or the
//! goal's. Notices are read from the activity index the Pulse keeps
//! (one bounded read, `NOTICE_WINDOW`, the newest `NOTICES_PER_ROW` kept per
//! row), classified, never stored twice; the read watermark that covers a
//! conversation covers them. What is **owed** is the asks alone: a notice
//! never moves the holder, the sidebar's count or a goal card's. Two rules
//! decide what is on the screen.
//!
//! # A row is earned once and kept
//!
//! This used to be `if actions.is_empty() && n_unread == 0 { continue; }`: a
//! conversation had a row *while* it had an open gate or an unread message,
//! and lost it the instant either went away. Reading a thread deleted it.
//! Deciding a gate deleted it. The one screen whose whole promise is "you can
//! come back to this" was the one screen that threw a row away for being
//! attended to — and the selection then silently landed on somebody else's
//! conversation.
//!
//! So membership is derived from facts that outlive both reading and an index
//! rebuild. A scope belongs here once it has **ever** needed a human:
//!
//! - a goal that has ever had a gate or a question, read back from the
//!   durable decision record ([`Workspace::decisions`]) or from a gate that
//!   is pending right now;
//! - a scope that named you (`mentioned_scopes`, the `p`-tag index);
//! - a DM you are in — a direct message is addressed to you by construction;
//! - a thing with a notice within the window — a workstream whose script
//!   failed, a project a step made, a workflow whose listener failed, a goal
//!   whose run finished;
//!
//! plus, always, anything currently unread. Nothing new is stored to make
//! that work: the journal already *is* the record of "this needed you", which
//! is why a row survives a discarded index while the read markers behind
//! `read` do not.
//!
//! # Read is mine; handled is everyone's
//!
//! Whether *I* have read a row is per-person and local — the `read_markers`
//! watermark, deliberately not synced. Whether a gate was approved is a fact
//! about the workspace, so `handled` and `decided` read the same for
//! everybody who can see the row. Collapsing the two would mean either
//! broadcasting what I have looked at or pretending a decision is personal.
//!
//! `read` is not `unread_count == 0`. The count comes from the `messages`
//! table, so a conversation you have read and a conversation that never had a
//! message both report zero — and a row whose only content is a pending gate
//! has no messages at all. `read` compares the watermark to `latest_at`,
//! which is what makes a gate-only row go from bold to dim when you open it.
//!
//! # Cost
//!
//! Membership widened the row set, so the per-row work had to narrow. The
//! grouped facts are read once per read — unread counts, the newest message
//! per scope and the newest by somebody else, read markers, the decision
//! record (capped at [`bisa_store::inbox::MAX_DECISIONS`]), one bounded
//! window of the feed for the notices — and the per-row message fetch
//! happens **only for scopes that have a message**. A row that exists
//! because a gate is open and nobody has spoken costs no message query.
//!
//! A journal is read, and verified signature by signature, **only for a
//! home that can owe something from it** ([`Asked`]): one that ever
//! journaled a question — the feed says which, in one indexed read — and,
//! for a goal, one still open. A waiting step is read from its run, never
//! from a journal. A frame ([`delta`]) reads the same grouped facts and the
//! asks of its own thing alone.
//!
//! # One builder
//!
//! [`rows`] is the list; [`row`] and [`delta`] are one thing's row and its
//! frame; [`needs_actions_for`] and [`needs_actions_for_run`] are a goal's
//! and a run's asks. Each row is made by one function (`build`) over one
//! set of facts, whoever asks — the node's route, its goal and run pages,
//! its frames, the command line with no node — so they cannot disagree.

mod types;

pub use types::*;

use crate::gates::GateEntry;
use crate::notices::{self, InboxKind, InboxTarget};
use crate::presence::SessionPresence;
use crate::registry::LiveRunId;
use crate::Inner;
use bisa_core::WaitFor;
use bisa_core::{
    render_template, AskKind, ChannelKind, Gate, Goal, GoalId, GoalText, Home, RunId, StepKind,
    StepState, TemplateCtx, WorkflowRun,
};
use bisa_store::{approval_subject, MessageRow, StoreError, Workspace, WorkstreamFilter};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;

/// How the builder says a sentence of the catalogue: the engine renders
/// none itself, so whoever reads the Inbox — the node, the command line —
/// hands its renderer in.
pub type Say<'a> = &'a dyn Fn(&bisa_core::Text) -> String;

/// What only a running engine knows: the gates open right now, and the
/// harnesses a person opened in a terminal. With no engine — the command
/// line on its own — there are none of either, and the Inbox is what the
/// workspace alone says: every durable ask, every notice, every mark.
#[derive(Clone, Debug, Default)]
pub struct LiveView {
    /// The pending gates, oldest first.
    pub gates: Vec<GateEntry>,
    /// Every terminal harness on the roster, waiting or not.
    pub terminals: Vec<SessionPresence>,
}

impl LiveView {
    /// No engine runs: nothing is live.
    pub fn none() -> Self {
        Self::default()
    }

    /// The engine's own view, read once.
    pub fn of(inner: &Inner) -> Self {
        Self {
            gates: inner.gates.pending(),
            terminals: inner.presence.terminals(),
        }
    }

    /// One terminal harness by its session, when it is on the roster.
    fn terminal(&self, id: LiveRunId) -> Option<&SessionPresence> {
        self.terminals.iter().find(|p| p.id == id)
    }

    /// The terminal harnesses waiting on their person right now, the newest
    /// wait first.
    fn waiting_terminals(&self) -> Vec<SessionPresence> {
        let mut rows: Vec<SessionPresence> = self
            .terminals
            .iter()
            .filter(|p| p.wait().is_some())
            .cloned()
            .collect();
        crate::presence::newest_wait_first(&mut rows);
        rows
    }
}

/// Everything a read of the Inbox is made from.
pub struct Reader<'a> {
    pub ws: &'a Workspace,
    pub live: &'a LiveView,
    pub say: Say<'a>,
}

/// The goal's current run, or none of it with the reason in the log: a run
/// this node cannot read costs the goal its run's asks, never the Inbox.
fn current_run_of(ws: &Workspace, goal: &Goal) -> Option<WorkflowRun> {
    let id = goal.run?;
    match ws.get_run(id) {
        Ok(run) => Some(run),
        Err(e) => {
            tracing::error!(
                target: "bisa_engine",
                goal = %goal.id,
                run = %id,
                "the goal's current run cannot be read; drawing the goal without it: {e}"
            );
            None
        }
    }
}

/// What a conversation is about, from its indexed parts; parts that make no
/// origin read as the workspace's, said at `warn`.
fn origin_of(row: &bisa_store::ConversationRow) -> bisa_core::ConversationOrigin {
    match bisa_core::ConversationOrigin::from_parts(
        &row.origin_kind,
        row.origin_id.as_deref(),
        row.project.as_deref(),
    ) {
        Some(origin) => origin,
        // LCOV_EXCL_START the index writes these parts from an origin, so they always make one; a row edited by hand on disk (disk-only)
        None => {
            tracing::warn!(
                target: "bisa_engine",
                conversation = %row.id,
                kind = %row.origin_kind,
                "an indexed conversation's parts make no origin; read as the workspace's"
            );
            bisa_core::ConversationOrigin::Workspace
        } // LCOV_EXCL_STOP
    }
}

/// How far back a row reads to pick its representative.
///
/// The representative is the *oldest unread*, so the window has to cover the
/// unread tail; past that it is only ever the last message. 200 is what this
/// route has always used, kept here as a name so the bound is stated once.
const MESSAGE_WINDOW: usize = 200;

/// How many of the newest activity rows under the notice tags are read per
/// request: the one bounded scan behind every row's notices. A notice older
/// than the window ages out of the Inbox and stays in the Pulse.
const NOTICE_WINDOW: usize = 1000;

/// The guard's decisions are read in a window of their own. Most of them
/// are calls it let through, which are no notice: counted with the rest
/// they would push a failed run or a blocked step out of the window above
/// for nothing. What a rule or the classifier refused among the newest of
/// them is a notice; an older refusal stays in the Pulse.
const GUARD_TAG: &str = "guard_decided";
const GUARD_WINDOW: usize = 200;

/// The newest notices a row carries.
const NOTICES_PER_ROW: usize = 20;

/// Whether a row is owed something: an ask, a join to admit, a harness waiting.
pub fn owes(r: &InboxRow) -> bool {
    !r.needs_action.is_empty() || r.join.is_some() || r.waiting.is_some()
}

/// Representative message: the oldest of the trailing `unread` messages
/// authored by others (falls back to the latest message).
fn representative(msgs: &[MessageRow], own: &str, unread: u64) -> Option<Representative> {
    let others: Vec<&MessageRow> = msgs
        .iter()
        .filter(|m| m.author != own && !m.retracted)
        .collect();
    let pick: Option<&MessageRow> = if unread > 0 && !others.is_empty() {
        let idx = others.len().saturating_sub(unread as usize);
        others.get(idx).copied().or_else(|| others.last().copied())
    } else {
        others.last().copied().or_else(|| msgs.last())
    };
    pick.map(preview_of)
}

/// How many characters of a message a list shows of it.
const PREVIEW_CHARS: usize = 120;

/// What a list says of one message — who, the first words, when: the
/// Inbox row's representative, and a channel's or a direct channel's latest
/// (`GET /channels`, `GET /dms`).
pub fn preview_of(m: &MessageRow) -> Representative {
    Representative {
        author: m.author.clone(),
        snippet: m.content.chars().take(PREVIEW_CHARS).collect(),
        at: m.created_at,
    }
}

/// The `GateKind` behind the label `add_approval` wrote (`format!("{gate:?}")`).
///
/// Unrecognised labels are `None` rather than a plausible default: a row that
/// says "delivery signed off" when the journal said something else is worse
/// than a row that shows a generic decision.
fn gate_kind_from_label(label: &str) -> Option<Gate> {
    Gate::ALL
        .iter()
        .copied()
        .find(|g| g.as_str().eq_ignore_ascii_case(label))
}

/// What a goal is waiting on a person for, read from durable state alone —
/// for a gate this daemon did not open (another process, or before it
/// started).
///
/// Three shapes, and every one of them is rebuilt from truth rather than from
/// a flag: a `human` step that is `Waiting` (its prompt and options are on
/// the frozen workflow), an `approval` step that is `Waiting`, and a proposed
/// workflow nobody has adopted yet (the journaled `Question` under its
/// `adopt:` subject, with no `Decision` on it). Nothing is invented: a row
/// that offered choices the asker never made would produce an answer nobody
/// gave.
/// Everything waiting on a person for one goal: the live gates naming it, else
/// the asks rebuilt from the run and the journal. **The goal page and the
/// inbox both read this**, so a question rebuilt after a restart shows on both
/// or on neither — never on one.
pub fn needs_actions_for(
    r: &Reader<'_>,
    goal: &Goal,
    run: Option<&WorkflowRun>,
) -> Vec<NeedsAction> {
    let home = Home::Goal { goal: goal.id };
    let live: Vec<NeedsAction> = r
        .live
        .gates
        .clone()
        .into_iter()
        .filter(|g| g.home == home)
        .map(NeedsAction::from_gate)
        .collect();
    build_needs(r.ws, r.say, &Asked::read(r.ws), goal, run, live)
}

/// Everything one run of the workspace owes a person: its live gates, else
/// the asks rebuilt from the run and its own journal. The Inbox files them
/// on the run's workflow's row, and `GET /runs/{rid}` shows them on the run
/// — the same builder, so the two agree.
pub fn needs_actions_for_run(r: &Reader<'_>, run: &WorkflowRun) -> Vec<NeedsAction> {
    let home = run.home();
    let live: Vec<NeedsAction> = r
        .live
        .gates
        .clone()
        .into_iter()
        .filter(|g| g.home == home)
        .map(NeedsAction::from_gate)
        .collect();
    build_run_needs(r.ws, r.say, &Asked::read(r.ws), run, live)
}

/// One run of the workspace's actions from its already-filtered live gates:
/// the live ones, and the asks rebuilt from durable state they do not
/// carry. A run of the workspace owes no adoption and no amendment — only a
/// goal does.
fn build_run_needs(
    ws: &Workspace,
    say: Say<'_>,
    asked: &Asked,
    run: &WorkflowRun,
    live: Vec<NeedsAction>,
) -> Vec<NeedsAction> {
    merged(live, durable_run_actions(ws, say, asked, run))
}

/// The live asks first, then every durable ask they do not already carry —
/// one row for one question, told apart by the step it is about or by its
/// subject. A live gate on one step hides no other step's question rebuilt
/// from the run (after a restart every old question is durable, and the
/// first re-ask opens a live gate beside them); a question with both a live
/// gate and a journal fact is one row, the live one.
fn merged(live: Vec<NeedsAction>, durable: Vec<NeedsAction>) -> Vec<NeedsAction> {
    fn key(a: &NeedsAction) -> String {
        match (&a.run, &a.step) {
            (Some(run), Some(step)) => format!("{run}/{step}"),
            _ => a.subject.clone().unwrap_or_default(),
        }
    }
    let seen: HashSet<String> = live.iter().map(key).collect();
    let mut out = live;
    out.extend(durable.into_iter().filter(|a| !seen.contains(&key(a))));
    out
}

/// One goal's actions from its already-filtered live gates: the live ones,
/// and the asks rebuilt from the run and the journal they do not carry
/// (`merged`), with the `adopt:` proposals attached. The shared tail of
/// `needs_actions_for` (one goal, live gates re-scanned) and `Facts::gather`
/// (all goals, gates scanned once) — so both produce the identical card and
/// neither re-scans per goal.
pub(crate) fn build_needs(
    ws: &Workspace,
    say: Say<'_>,
    asked: &Asked,
    goal: &Goal,
    run: Option<&WorkflowRun>,
    live: Vec<NeedsAction>,
) -> Vec<NeedsAction> {
    let actions = merged(live, durable_actions(ws, say, asked, goal, run));
    with_proposals(ws, actions)
}

/// A decision about a workflow carries the workflow it proposes: the card
/// shows the description, every step summarised, the edges and the inputs —
/// and binds the inputs before approving, so the gate is never burnt on a
/// missing one. An adoption's subject names the design, an amendment's the
/// held copy (`proposed_workflow`).
fn with_proposals(ws: &Workspace, mut actions: Vec<NeedsAction>) -> Vec<NeedsAction> {
    for a in &mut actions {
        let Some(id) = a.subject.as_deref().and_then(proposed_workflow) else {
            continue;
        };
        if let Ok(wf) = ws.get_workflow(id) {
            a.proposal = Some(ProposalView::of(&wf));
        }
    }
    actions
}

/// The workflow a decision subject proposes: `adopt:<workflow>@<revision>`
/// names the design before the `@`, `amend:<run>@<workflow>` the held copy
/// after it. Anything else proposes no workflow.
fn proposed_workflow(subject: &str) -> Option<bisa_core::WorkflowId> {
    if let Some(rest) = subject.strip_prefix("adopt:") {
        return rest.split('@').next()?.parse().ok();
    }
    if let Some(rest) = subject.strip_prefix("amend:") {
        return rest.split_once('@')?.1.parse().ok();
    }
    None
}

/// A home's journal, or nothing of it with the reason in the log: the
/// durable asks of a journal this node cannot read are not shown — said at
/// `error`, never dropped in silence, and never the whole Inbox lost.
fn journal_or_empty(ws: &Workspace, home: &Home) -> Vec<bisa_core::event::JournalEvent> {
    ws.journal(home).unwrap_or_else(|e| {
        tracing::error!(
            target: "bisa_engine",
            %home,
            "the journal cannot be read; its durable asks are not shown: {e}"
        );
        Vec::new()
    })
}

/// The homes that ever journaled a question: `(source kind, id)` as the feed
/// files them — a goal by its id, a run of the workspace under its workflow.
///
/// Every journal fact is a row of the activity index, recorded on append and
/// replayed by a rebuild (the footing the `approvals` table stands on), so a
/// home that is not here has no question in its journal, and its journal —
/// read and verified whole, signature by signature — need not be opened to
/// find none. `None` is "could not be asked": every journal is read, as
/// before.
pub(crate) struct Asked(Option<HashSet<(String, String)>>);

impl Asked {
    fn read(ws: &Workspace) -> Self {
        match ws.activity_sources_of_kind("question") {
            Ok(homes) => Self(Some(homes)),
            // LCOV_EXCL_START the index answers every other read of the request; this one failing alone is the disk (disk-only)
            Err(e) => {
                tracing::warn!(target: "bisa_engine", "the feed cannot say which homes asked; every journal is read: {e}");
                Self(None)
            } // LCOV_EXCL_STOP
        }
    }

    /// Whether a goal's journal can hold a question still open: it asked
    /// one, and it is not closed — closing withdraws every question of a
    /// goal's own.
    fn goal(&self, goal: &Goal) -> bool {
        !goal.is_closed() && self.holds("goal", &goal.id.to_string())
    }

    /// Whether a run of the workspace's journal can: its workflow's rows
    /// hold a question — coarser than the run, and still sound.
    fn run(&self, run: &WorkflowRun) -> bool {
        self.holds("workflow", &run.workflow.id.to_string())
    }

    fn holds(&self, kind: &str, id: &str) -> bool {
        self.0
            .as_ref()
            .is_none_or(|asked| asked.contains(&(kind.to_string(), id.to_string())))
    }
}

/// The newest question of one family (`prefix`) that nothing settled —
/// judged per subject by the engine's one reader
/// (`recovery::open_questions`), the same the durable decide finds its
/// question with: a decision on one subject of a family hides no other
/// subject of it still open.
fn newest_open(
    journal: &[bisa_core::event::JournalEvent],
    prefix: &str,
) -> Option<crate::recovery::OpenQuestion> {
    crate::recovery::open_questions(journal, |subject| subject.starts_with(prefix))
        .into_iter()
        .next()
}

/// An agent's own question (`ask_human:`), rebuilt as a card at `home`.
fn asked_by_an_agent(
    home: Home,
    run: Option<RunId>,
    q: crate::recovery::OpenQuestion,
) -> NeedsAction {
    NeedsAction {
        home,
        gate_id: None,
        gate_kind: Gate::Escalation,
        run: run.map(|r| r.to_string()),
        step: None,
        question: q.text,
        expects: q.expects,
        durable: true,
        subject: Some(q.subject),
        work_item: q.work_item.map(|w| w.to_string()),
        opened_at: Some(q.at),
        proposal: None,
    }
}

/// The asks a live run's waiting steps owe a person, rebuilt from the run
/// alone: a `human` step's question, an `approval` step's decision, a held
/// `wait` step's release — each filed at the run's home. `goal` renders a
/// goal's run's prompts; a run of the workspace has no goal to read.
fn waiting_step_actions(say: Say<'_>, run: &WorkflowRun, goal: Option<&Goal>) -> Vec<NeedsAction> {
    let mut out = Vec::new();
    if run.is_finished() {
        return out;
    }
    let home = run.home();
    for (id, record) in &run.steps {
        if record.state != StepState::Waiting {
            continue;
        }
        let Some(step) = run.workflow.step(id) else {
            // LCOV_EXCL_START: a gate naming a step its run's workflow no longer has; an amendment's window
            continue;
            // LCOV_EXCL_STOP
        };
        // A step reads the run's inputs, never the event that began it: the
        // start's mapping read that, once.
        let ctx = TemplateCtx {
            inputs: &run.inputs,
            steps: &run.steps,
            event: None,
            goal: goal.map(|g| GoalText {
                statement: &g.statement,
                title: g.title.as_deref(),
            }),
            params: bisa_core::no_values(),
            account: bisa_core::no_values(),
        };
        let (gate_kind, prompt, expects, subject): (Gate, String, AskKind, String) =
            match &step.kind {
                StepKind::Human {
                    prompt,
                    options,
                    multi,
                    ..
                } => (
                    Gate::Escalation,
                    prompt.clone(),
                    AskKind::Answer {
                        options: options.clone(),
                        multi: *multi,
                    },
                    format!("step:{}/{id}", run.id),
                ),
                StepKind::Approval { prompt } => (
                    Gate::Approval,
                    prompt.clone(),
                    AskKind::Decision,
                    approval_subject(run.id, id),
                ),
                // A `wait` step a person is holding: owed until released.
                // Its one verb is *Release*; `release:` is what the decide
                // route reads it back as.
                StepKind::Wait {
                    until: WaitFor::Release,
                } => (
                    Gate::Escalation,
                    say(&bisa_core::text!(
                        "error-node-inbox-release-step-when-ready",
                        step = step.name.clone()
                    )),
                    AskKind::Decision,
                    format!("release:{}/{id}", run.id),
                ),
                _ => continue,
            };
        // A prompt that cannot render is never shown as written — a
        // placeholder is not a question — but with the reason, which is the
        // honest thing the person can act on.
        let question = match render_template(&prompt, &ctx) {
            Ok(rendered) => rendered,
            Err(e) => say(&bisa_core::text!(
                "error-node-inbox-question-could-not-render",
                step = step.name.clone(),
                e = e.to_string()
            )),
        };
        out.push(NeedsAction {
            home,
            gate_id: None,
            gate_kind,
            run: Some(run.id.to_string()),
            step: Some(id.to_string()),
            question,
            expects,
            durable: true,
            subject: Some(subject),
            work_item: record.work_item.map(|w| w.to_string()),
            opened_at: record.started_at,
            proposal: None,
        });
    }
    out
}

/// What a run of the workspace owes a person from durable state alone: its
/// waiting steps, and the newest question an agent of it asked
/// (`ask_human:`) that nothing has settled — while the run still goes.
fn durable_run_actions(
    ws: &Workspace,
    say: Say<'_>,
    asked: &Asked,
    run: &WorkflowRun,
) -> Vec<NeedsAction> {
    let mut out = waiting_step_actions(say, run, None);
    if run.is_finished() {
        return out;
    }
    // Every question an agent of the run asked that nothing settled, each
    // its own card: two work items' questions are two, and answering one
    // leaves the other owed.
    if !asked.run(run) {
        return out;
    }
    let journal = journal_or_empty(ws, &run.home());
    let mut asked = crate::recovery::open_questions(&journal, |s| s.starts_with("ask_human:"));
    asked.reverse();
    out.extend(
        asked
            .into_iter()
            .map(|q| asked_by_an_agent(run.home(), Some(run.id), q)),
    );
    out
}

pub(crate) fn durable_actions(
    ws: &Workspace,
    say: Say<'_>,
    asked: &Asked,
    goal: &Goal,
    run: Option<&WorkflowRun>,
) -> Vec<NeedsAction> {
    let home = Home::Goal { goal: goal.id };
    let mut out = run
        .map(|r| waiting_step_actions(say, r, Some(goal)))
        .unwrap_or_default();
    // What follows is read from the journal: a goal that never asked, or
    // one closed since, has nothing there to find.
    if !asked.goal(goal) {
        return out;
    }
    let journal = journal_or_empty(ws, &home);
    // A held amendment still owed while the run goes on: the run names the
    // subject, so an earlier run's amendment is never rebuilt for this one.
    if let Some(run) = run.filter(|r| !r.is_finished()) {
        if let Some(q) = newest_open(&journal, &format!("amend:{}@", run.id)) {
            out.push(NeedsAction {
                home,
                gate_id: None,
                gate_kind: Gate::Approval,
                run: Some(run.id.to_string()),
                step: None,
                question: q.text,
                expects: AskKind::Decision,
                durable: true,
                subject: Some(q.subject),
                work_item: None,
                opened_at: Some(q.at),
                proposal: None,
            });
        }
    }
    // An adoption still owed: the goal points at a workflow, nothing runs,
    // and the newest adoption question has no decision.
    if goal.workflow.is_some() && run.is_none_or(|r| r.is_finished()) {
        if let Some(q) = newest_open(&journal, "adopt:") {
            out.push(NeedsAction {
                home,
                gate_id: None,
                gate_kind: Gate::Approval,
                run: None,
                step: None,
                question: q.text,
                expects: AskKind::Decision,
                durable: true,
                subject: Some(q.subject),
                work_item: None,
                opened_at: Some(q.at),
                proposal: None,
            });
        }
    }
    // The Workflow Agent's own question, still unanswered: a goal it designs
    // for, with no workflow and no run, whose newest `ask_human:` question
    // has no decision after it. Before this, a daemon restart lost the
    // question on both screens and the goal was parked for good.
    if goal.mode.designs() && goal.workflow.is_none() && run.is_none() {
        if let Some(q) = newest_open(&journal, &format!("ask_human:{}", goal.id)) {
            out.push(asked_by_an_agent(home, None, q));
        }
    }
    // A work item's question while the goal's run goes on — asked by a
    // process that is gone, before the next one withdraws it: owed like a
    // run of the workspace's, each its own card.
    if let Some(run) = run.filter(|r| !r.is_finished()) {
        let own = format!("ask_human:{}", goal.id);
        let mut asked =
            crate::recovery::open_questions(&journal, |s| s.starts_with("ask_human:") && s != own);
        asked.reverse();
        out.extend(
            asked
                .into_iter()
                .map(|q| asked_by_an_agent(home, Some(run.id), q)),
        );
    }
    out
}

/// Which things one read is about: every row, or the one a frame names.
/// A frame reads the same grouped facts the list does and the asks of its
/// own thing alone — never every goal's.
#[derive(Clone, Copy)]
enum Scope<'a> {
    All,
    Key(&'a str),
}

/// A thing a row can be about, read from the workspace.
enum Thing {
    Goal(Box<Goal>),
    /// A live gate names a goal the index no longer lists: the decision is
    /// still owed, on a row named by nothing but the key.
    OrphanGoal(String),
    Channel(Box<bisa_core::Channel>),
    Conversation(Box<bisa_store::ConversationRow>),
    Workstream(Box<bisa_core::Workstream>),
    Project(Box<bisa_core::Project>),
    Workflow {
        id: String,
        name: String,
        created_at: u64,
    },
    Person {
        key: String,
        label: Option<String>,
    },
    Session(Box<SessionPresence>),
}

impl Thing {
    fn kind(&self) -> InboxKind {
        match self {
            Thing::Goal(_) | Thing::OrphanGoal(_) => InboxKind::Goal,
            Thing::Channel(c) if matches!(c.kind, ChannelKind::Direct) => InboxKind::Dm,
            Thing::Channel(_) => InboxKind::Channel,
            Thing::Conversation(_) => InboxKind::Conversation,
            Thing::Workstream(_) => InboxKind::Workstream,
            Thing::Project(_) => InboxKind::Project,
            Thing::Workflow { .. } => InboxKind::Workflow,
            Thing::Person { .. } => InboxKind::People,
            Thing::Session(_) => InboxKind::Session,
        }
    }
}

/// The persons a row can be about: each key with the label it goes by.
type People = Vec<(String, Option<String>)>;

/// Everything the row builders read once per read.
struct Facts {
    /// The goals the read is about: every one the index lists, or the one a
    /// frame names. Each `list_goals` is a snapshot read per goal on disk.
    goals: Vec<Goal>,
    /// goal → its current run, read once beside the goal.
    runs: HashMap<String, WorkflowRun>,
    own: String,
    unread: HashMap<String, u64>,
    /// scope → newest live message. Absent means nothing was ever said.
    activity: HashMap<String, u64>,
    /// scope → newest live message by somebody else: what a read mark is
    /// compared with — your own post is nothing you have yet to read.
    others: HashMap<String, u64>,
    /// scope → (watermark, forced unread).
    markers: HashMap<String, (u64, bool)>,
    mentioned: HashSet<String>,
    /// goal → its most recent decision.
    decided: HashMap<String, Decided>,
    /// goal → what is waiting on a human right now.
    needs: HashMap<String, Vec<NeedsAction>>,
    /// workflow → what its runs of the workspace are waiting on a human for.
    workflow_needs: HashMap<String, Vec<NeedsAction>>,
    /// workflow → the name its runs carry: what a row is titled by when the
    /// workflow's own file can no longer be read.
    workflow_names: HashMap<String, String>,
    /// workflow → the most recent decision on one of its runs of the
    /// workspace: what keeps its row once the ask is answered.
    workflow_decided: HashMap<String, Decided>,
    /// thing → what happened to it that concerns a person, newest first,
    /// the newest `NOTICES_PER_ROW` — from one read of the activity index.
    notices: HashMap<InboxTarget, Vec<NoticeDto>>,
    /// The persons hosted here and the ones asking to be.
    people: People,
    /// key → the join it is waiting on.
    joins: HashMap<String, JoinRequest>,
}

impl Facts {
    /// One bounded read of the feed, classified: the newest rows under the
    /// notice tags, each a notice or not (`notices::notice_of`), landed on
    /// the row it concerns (`notices::row_of`), typed as the Pulse types a
    /// row — a record this build cannot type is skipped there, never here.
    ///
    /// A project and its primary workstream are one thing under one id, so
    /// they are one row: what happened in the primary checkout — a script,
    /// a pull request, a committer wanted — lands on the project's row,
    /// beside the notice that the project was made.
    fn gather_notices(
        ws: &Workspace,
        projects: &HashSet<String>,
    ) -> Result<HashMap<InboxTarget, Vec<NoticeDto>>, StoreError> {
        let mut titles = crate::feed::Titles::new(ws);
        let mut out: HashMap<InboxTarget, Vec<NoticeDto>> = HashMap::new();
        let others: Vec<&str> = notices::NOTICE_TAGS
            .iter()
            .copied()
            .filter(|tag| *tag != GUARD_TAG)
            .collect();
        let mut records = ws.activity_by_kinds(&others, NOTICE_WINDOW)?;
        records.extend(ws.activity_by_kinds(&[GUARD_TAG], GUARD_WINDOW)?);
        // Newest first across both reads: a row keeps its newest notices.
        records.sort_by_key(|record| std::cmp::Reverse((record.at, record.seq)));
        for record in records {
            let event = match serde_json::from_str::<serde_json::Value>(&record.event) {
                Ok(event) => event,
                // LCOV_EXCL_START: the index holds what serde wrote; a row that is no JSON is a corrupted database
                Err(e) => {
                    // As the Pulse says it (`feed::typed`): once, with the row's `seq`.
                    tracing::warn!(target: "bisa_engine", seq = record.seq, kind = %record.kind, "skipping an activity row the Inbox cannot read: {e}");
                    continue;
                    // LCOV_EXCL_STOP
                }
            };
            let Some(notice) = notices::notice_of(&record.kind, &event) else {
                continue;
            };
            let Some(mut target) =
                notices::row_of(&record.kind, &record.source_kind, &record.source_id, &event)
            else {
                continue; // LCOV_EXCL_LINE: a notice from a source no row is keyed by
            };
            if target.kind == InboxKind::Workstream && projects.contains(&target.id) {
                target.kind = InboxKind::Project;
            }
            let rows = out.entry(target).or_default();
            if rows.len() >= NOTICES_PER_ROW {
                continue;
            }
            if let Some(row) = crate::feed::typed(record, &mut titles) {
                rows.push(NoticeDto { notice, row });
            }
        }
        Ok(out)
    }

    /// The persons a row can be about and the joins waiting on them. A
    /// member file or an invitation list this node cannot read costs the
    /// rows read from it — said at `warn` — never the Inbox: a person's row
    /// is still earned by a notice.
    fn gather_people(ws: &Workspace) -> (People, HashMap<String, JoinRequest>) {
        let mut people: Vec<(String, Option<String>)> = match ws.people() {
            Ok(people) => people
                .into_iter()
                .map(|m| (m.pubkey.as_hex().to_string(), m.label))
                .collect(),
            Err(e) => {
                tracing::warn!(target: "bisa_engine", "the members cannot be read; the Inbox shows the persons its notices name: {e}");
                Vec::new()
            }
        };
        let invites = ws.invites().unwrap_or_else(|e| {
            tracing::warn!(target: "bisa_engine", "the invitations cannot be read; the Inbox shows no join waiting: {e}");
            Vec::new()
        });
        let mut joins = HashMap::new();
        for invite in invites {
            let bisa_core::InviteState::Requested { by, label, at } = &invite.state else {
                continue;
            };
            let key = by.as_hex().to_string();
            if !people.iter().any(|(k, _)| *k == key) {
                people.push((key.clone(), label.clone()));
            }
            joins.entry(key).or_insert(JoinRequest {
                invite: invite.id.to_string(),
                role: invite.role,
                label: label.clone(),
                at: *at,
            });
        }
        (people, joins)
    }

    fn gather(r: &Reader<'_>, scope: Scope<'_>) -> Result<Self, StoreError> {
        let ws = r.ws;
        let own = ws.owner_principal().as_hex().to_string();

        // The live gates, scanned once and grouped by home. Each goal is
        // then handed its own slice through `build_needs`, the same tail the
        // goal page uses, so the `adopt:` proposal card is identical wherever
        // it is mounted; each run of the workspace its own through
        // `build_run_needs`.
        let mut live_by_goal: HashMap<String, Vec<NeedsAction>> = HashMap::new();
        let mut live_by_run: HashMap<RunId, Vec<NeedsAction>> = HashMap::new();
        for gate in r.live.gates.clone() {
            match gate.home {
                Home::Goal { goal } => live_by_goal
                    .entry(goal.to_string())
                    .or_default()
                    .push(NeedsAction::from_gate(gate)),
                Home::Run { run } => live_by_run
                    .entry(run)
                    .or_default()
                    .push(NeedsAction::from_gate(gate)),
            }
        }
        let asked = Asked::read(ws);
        // The goals the read is about. A put-away goal has no row — the
        // list leaves it out, and so does a frame that names it.
        let goals = match scope {
            Scope::All => ws.list_goals(None)?,
            Scope::Key(key) => GoalId::from_str(key)
                .ok()
                .and_then(|id| ws.get_goal(id).ok())
                .filter(|goal| !goal.is_archived())
                .into_iter()
                .collect(),
        };
        let about = |key: &str| match scope {
            Scope::All => true,
            Scope::Key(named) => named == key,
        };
        let mut needs: HashMap<String, Vec<NeedsAction>> = HashMap::new();
        let mut runs: HashMap<String, WorkflowRun> = HashMap::new();
        for goal in &goals {
            let key = goal.id.to_string();
            let run = current_run_of(ws, goal);
            let live = live_by_goal.remove(&key).unwrap_or_default();
            let actions = build_needs(ws, r.say, &asked, goal, run.as_ref(), live);
            if !actions.is_empty() {
                needs.insert(key.clone(), actions);
            }
            if let Some(run) = run {
                runs.insert(key, run);
            }
        }
        // A live gate can name a goal the index no longer lists — a record
        // removed behind the engine's back, or one the index lost. Its
        // decision is still owed: the key keeps a row of its own, named by
        // nothing but the key, rather than losing the gate.
        for (key, live) in live_by_goal {
            if about(&key) {
                needs.entry(key).or_default().extend(live);
            }
        }

        // Runs of the workspace: what each owes a person is its workflow's
        // row's — no goal holds it (I52).
        let mut workflow_needs: HashMap<String, Vec<NeedsAction>> = HashMap::new();
        let mut workflow_names: HashMap<String, String> = HashMap::new();
        let of_workflow = match scope {
            Scope::All => Some(None),
            Scope::Key(key) => key.parse::<bisa_core::WorkflowId>().ok().map(Some),
        };
        if let Some(of_workflow) = of_workflow {
            for run in ws.live_workspace_runs(of_workflow)? {
                let live = live_by_run.remove(&run.id).unwrap_or_default();
                let actions = build_run_needs(ws, r.say, &asked, &run, live);
                if !actions.is_empty() {
                    let workflow = run.workflow.id.to_string();
                    workflow_names.insert(workflow.clone(), run.workflow.name.clone());
                    workflow_needs.entry(workflow).or_default().extend(actions);
                }
            }
        }
        // A live gate on a run the index no longer lists as live stays
        // visible on its workflow's row rather than lost.
        for (run, actions) in live_by_run {
            // LCOV_EXCL_START: a live gate on a run the index no longer lists; the record half's window
            if let Ok(run) = ws.get_run(run) {
                let workflow = run.workflow.id.to_string();
                if about(&workflow) {
                    workflow_names.insert(workflow.clone(), run.workflow.name.clone());
                    workflow_needs.entry(workflow).or_default().extend(actions);
                }
            }
            // LCOV_EXCL_STOP
        }

        // `decisions` is newest-first, so the first one seen per goal — or
        // per workflow, through its run — wins.
        let mut decided: HashMap<String, Decided> = HashMap::new();
        let mut workflow_decided: HashMap<String, Decided> = HashMap::new();
        let mut workflow_of_run: HashMap<String, Option<String>> = HashMap::new();
        for d in ws.decisions()? {
            let entry = Decided {
                gate_kind: gate_kind_from_label(&d.gate),
                approve: d.approve,
                subject: d.subject,
                actor: d.actor,
                at: d.at,
            };
            match (d.goal_id, d.run_id) {
                (Some(goal), _) => {
                    decided.entry(goal).or_insert(entry);
                }
                (None, Some(run)) => {
                    // One snapshot read per run with a decision, at most.
                    let workflow = workflow_of_run
                        .entry(run.clone())
                        .or_insert_with(|| {
                            run.parse::<RunId>()
                                .ok()
                                .and_then(|id| ws.get_run(id).ok())
                                .map(|r| r.workflow.id.to_string())
                        })
                        .clone();
                    if let Some(workflow) = workflow {
                        workflow_decided.entry(workflow).or_insert(entry);
                    }
                }
                // LCOV_EXCL_START: a decision's home is a goal or a run; by construction
                (None, None) => {} // LCOV_EXCL_STOP
            }
        }

        let projects: HashSet<String> = ws
            .list_projects()?
            .into_iter()
            .map(|p| p.id.to_string())
            .collect();
        let (people, joins) = Self::gather_people(ws);
        let mentioned = ws.mentioned_scopes(&own)?.into_iter().collect();
        Ok(Self {
            goals,
            runs,
            own,
            unread: ws.unread_counts()?.into_iter().collect(),
            activity: ws.scope_activity()?.into_iter().collect(),
            others: ws.scope_activity_of_others()?.into_iter().collect(),
            markers: ws
                .read_markers()?
                .into_iter()
                .map(|m| (m.scope_id, (m.last_read_at, m.forced_unread)))
                .collect(),
            mentioned,
            decided,
            needs,
            workflow_needs,
            workflow_names,
            workflow_decided,
            notices: Self::gather_notices(ws, &projects)?,
            people,
            joins,
        })
    }

    fn unread_of(&self, key: &str) -> u64 {
        self.unread.get(key).copied().unwrap_or(0)
    }

    /// The notices on one thing, taken once.
    fn take_notices(&mut self, kind: InboxKind, key: &str) -> Vec<NoticeDto> {
        self.notices
            .remove(&InboxTarget {
                kind,
                id: key.to_string(),
            })
            .unwrap_or_default()
    }

    /// How many of a row's notices are after your watermark — all of them
    /// on a scope you put back deliberately.
    fn unread_notices(&self, key: &str, notices: &[NoticeDto]) -> usize {
        unread_notices_after(self.markers.get(key).copied(), notices)
    }

    /// Whether the local watermark covers everything this row has to show
    /// you. A forced-unread scope is never read, whatever the arithmetic
    /// says — putting a row back is a deliberate act and a clock may not
    /// undo it.
    fn read_at(&self, key: &str, latest_at: u64) -> bool {
        let (last_read_at, forced) = self.markers.get(key).copied().unwrap_or((0, false));
        !forced && last_read_at >= latest_at
    }

    /// The newest messages of a scope that has any, for its row's preview.
    /// A read that fails is said and costs the preview, never the row.
    fn messages_of(&self, ws: &Workspace, key: &str) -> Vec<MessageRow> {
        if !self.activity.contains_key(key) {
            return Vec::new();
        }
        match ws.messages(key, None, MESSAGE_WINDOW) {
            Ok(messages) => messages,
            // LCOV_EXCL_START the index answered the grouped reads a moment ago; a message read that fails now is the disk (disk-only)
            Err(e) => {
                tracing::warn!(target: "bisa_engine", scope = key, "a row's messages cannot be read; it is shown without its preview: {e}");
                Vec::new()
            } // LCOV_EXCL_STOP
        }
    }
}

/// When a row was last news, and when it was last news **to you**.
struct Dated {
    /// The newest thing the row has to show — what it is dated and ordered
    /// by: a message, an ask, a notice, a decision.
    latest_at: u64,
    /// The newest of those somebody else brought: what your read mark is
    /// compared with. Your own post and your own decision are nothing you
    /// have yet to read.
    news_at: u64,
}

/// The two clocks of a row. A gate's clock is the moment it **opened**, not
/// the moment the page was rendered: a gate waiting nine days used to date
/// itself `now` on every request and so read as brand new forever, hiding
/// precisely the staleness worth surfacing. A decision dates its row too —
/// one taken by somebody else while you were away is news — and `floor` is
/// when the thing came to be.
fn dated(
    f: &Facts,
    key: &str,
    actions: &[NeedsAction],
    notices: &[NoticeDto],
    decided: Option<&Decided>,
    floor: u64,
) -> Dated {
    let gate_at = actions
        .iter()
        .filter_map(|a| a.opened_at)
        .max()
        .unwrap_or(0);
    let notice_at = notices.iter().map(|n| n.row.at).max().unwrap_or(0);
    let owed = gate_at.max(notice_at).max(floor);
    let decided_at = decided.map_or(0, |d| d.at);
    let decided_by_another = decided.filter(|d| d.actor != f.own).map_or(0, |d| d.at);
    Dated {
        latest_at: owed
            .max(f.activity.get(key).copied().unwrap_or(0))
            .max(decided_at),
        news_at: owed
            .max(f.others.get(key).copied().unwrap_or(0))
            .max(decided_by_another),
    }
}

/// The notices after a watermark `(last_read_at, forced_unread)`. A scope
/// put back deliberately keeps its watermark (`Workspace::mark_unread`), so
/// it counts what is truly after it — and one when nothing is, as its
/// messages do: the row was put back, and says so.
fn unread_notices_after(marker: Option<(u64, bool)>, notices: &[NoticeDto]) -> usize {
    let (last_read_at, forced) = marker.unwrap_or((0, false));
    let after = notices.iter().filter(|n| n.row.at > last_read_at).count();
    if forced {
        return after.max(notices.len().min(1));
    }
    after
}

/// The one place a row is made: whether the thing has earned one, what it
/// owes, what happened to it, when it was last news and whether you have
/// read it. The list builds every thing's; a frame builds one — the same
/// function, so the two cannot disagree.
fn build(thing: &Thing, f: &mut Facts, r: &Reader<'_>) -> Option<InboxRow> {
    let ws = r.ws;
    let say = r.say;
    let kind = thing.kind();
    match thing {
        Thing::Goal(goal) => {
            let key = goal.id.to_string();
            let actions = f.needs.remove(&key).unwrap_or_default();
            let notices = f.take_notices(InboxKind::Goal, &key);
            let n_unread = f.unread_of(&key);
            let decided = f.decided.get(&key).cloned();
            let mentioned = f.mentioned.contains(&key);
            // Earned once and kept: a live gate, a decision on record, a
            // mention, a notice, or anything unread right now. Reading and
            // deciding both leave the first four untouched, which is the
            // whole point.
            if actions.is_empty()
                && decided.is_none()
                && !mentioned
                && n_unread == 0
                && notices.is_empty()
            {
                return None;
            }
            let msgs = f.messages_of(ws, &key);
            let at = dated(
                f,
                &key,
                &actions,
                &notices,
                decided.as_ref(),
                goal.created_at,
            );
            Some(InboxRow {
                key: key.clone(),
                kind,
                source: InboxSource::of(kind, None),
                origin: None,
                title: goal
                    .title
                    .clone()
                    .unwrap_or_else(|| goal.statement.chars().take(80).collect()),
                latest_at: at.latest_at,
                unread_count: n_unread,
                read: f.read_at(&key, at.news_at),
                handled: actions.is_empty() && decided.is_some(),
                mentioned,
                needs_action: actions,
                unread_notices: f.unread_notices(&key, &notices),
                notices,
                decided,
                representative: representative(&msgs, &f.own, n_unread),
                status: Some(goal.status(f.runs.get(&key))),
                join: None,
                waiting: None,
            })
        }
        Thing::OrphanGoal(key) => {
            let actions = f.needs.remove(key)?;
            let notices = f.take_notices(InboxKind::Goal, key);
            let at = dated(f, key, &actions, &notices, None, 0);
            Some(InboxRow {
                key: key.clone(),
                kind,
                source: InboxSource::of(kind, None),
                origin: None,
                title: key.clone(),
                latest_at: at.latest_at,
                unread_count: 0,
                read: f.read_at(key, at.news_at),
                handled: false,
                mentioned: f.mentioned.contains(key),
                needs_action: actions,
                unread_notices: f.unread_notices(key, &notices),
                notices,
                decided: None,
                representative: None,
                status: None,
                join: None,
                waiting: None,
            })
        }
        Thing::Channel(channel) => {
            let key = channel.id.to_string();
            let n_unread = f.unread_of(&key);
            let mentioned = f.mentioned.contains(&key);
            // A DM is addressed to you by construction, so it keeps its row.
            // Standing-channel chatter is not: it earns one by naming you or
            // by being unread, and otherwise belongs in the Channels view.
            if kind != InboxKind::Dm && !mentioned && n_unread == 0 {
                return None;
            }
            let msgs = f.messages_of(ws, &key);
            let at = dated(f, &key, &[], &[], None, channel.created_at);
            Some(InboxRow {
                key: key.clone(),
                kind,
                source: InboxSource::of(kind, None),
                origin: None,
                title: channel.name.clone(),
                latest_at: at.latest_at,
                unread_count: n_unread,
                read: f.read_at(&key, at.news_at),
                handled: false,
                mentioned,
                needs_action: vec![],
                notices: vec![],
                unread_notices: 0,
                decided: None,
                representative: representative(&msgs, &f.own, n_unread),
                status: None,
                join: None,
                waiting: None,
            })
        }
        // A conversation: one names you or is unread. Archived ones stay
        // out — a person put them away.
        Thing::Conversation(c) => {
            let key = c.id.clone();
            let n_unread = f.unread_of(&key);
            let mentioned = f.mentioned.contains(&key);
            if c.archived || (!mentioned && n_unread == 0) {
                return None;
            }
            let msgs = f.messages_of(ws, &key);
            let at = dated(f, &key, &[], &[], None, c.created_at);
            let origin = origin_of(c);
            Some(InboxRow {
                key: key.clone(),
                kind,
                source: InboxSource::of(kind, Some(&origin)),
                origin: Some(origin),
                title: c
                    .title
                    .clone()
                    .or_else(|| c.first_line.clone())
                    .unwrap_or_else(|| say(&bisa_core::text!("error-node-inbox-new-conversation"))),
                latest_at: at.latest_at,
                unread_count: n_unread,
                read: f.read_at(&key, at.news_at),
                handled: false,
                mentioned,
                needs_action: vec![],
                notices: vec![],
                unread_notices: 0,
                decided: None,
                representative: representative(&msgs, &f.own, n_unread),
                status: None,
                join: None,
                waiting: None,
            })
        }
        // A workstream or a project: something happened in it — a script
        // failed, a pull request opened or merged, a committer is wanted, a
        // step made the folder.
        Thing::Workstream(_) | Thing::Project(_) => {
            let (key, title, created_at) = match thing {
                Thing::Workstream(w) => (w.id.to_string(), w.label(None), w.created_at),
                Thing::Project(p) => (p.id.to_string(), p.name.clone(), p.created_at),
                // LCOV_EXCL_START the outer arm matched a workstream or a project, so no other thing reaches here; by construction
                _ => unreachable!("the arm holds these two alone"),
                // LCOV_EXCL_STOP
            };
            let notices = f.take_notices(kind, &key);
            if notices.is_empty() {
                return None;
            }
            let at = dated(f, &key, &[], &notices, None, created_at);
            Some(InboxRow {
                key: key.clone(),
                kind,
                source: InboxSource::of(kind, None),
                origin: None,
                title,
                latest_at: at.latest_at,
                unread_count: 0,
                read: f.read_at(&key, at.news_at),
                handled: false,
                mentioned: false,
                needs_action: vec![],
                unread_notices: f.unread_notices(&key, &notices),
                notices,
                decided: None,
                representative: None,
                status: None,
                join: None,
                waiting: None,
            })
        }
        // A workflow: what the Workflow Agent designed or proposed, one put
        // away, a listener of it that failed, and what its runs of the
        // workspace ask and did — no goal holds those. No conversation; the
        // door is the designer, or the run the ask names.
        Thing::Workflow {
            id,
            name,
            created_at,
        } => {
            let key = id.clone();
            let notices = f.take_notices(InboxKind::Workflow, &key);
            let actions = f.workflow_needs.remove(&key).unwrap_or_default();
            let decided = f.workflow_decided.get(&key).cloned();
            if notices.is_empty() && actions.is_empty() && decided.is_none() {
                return None;
            }
            let at = dated(f, &key, &actions, &notices, decided.as_ref(), *created_at);
            Some(InboxRow {
                key: key.clone(),
                kind,
                source: InboxSource::of(kind, None),
                origin: None,
                title: name.clone(),
                latest_at: at.latest_at,
                unread_count: 0,
                read: f.read_at(&key, at.news_at),
                handled: actions.is_empty() && decided.is_some(),
                mentioned: false,
                needs_action: actions,
                unread_notices: f.unread_notices(&key, &notices),
                notices,
                decided,
                representative: None,
                status: None,
                join: None,
                waiting: None,
            })
        }
        // People (14-collaboration): somebody waiting to be admitted, a
        // person who joined or left, a message the classifier held. Keyed
        // by pubkey; the door is Settings › People.
        Thing::Person { key, label } => {
            let notices = f.take_notices(InboxKind::People, key);
            let join = f.joins.get(key).cloned();
            if notices.is_empty() && join.is_none() {
                return None; // LCOV_EXCL_LINE a member with no notice in hand — the notice of their joining a bus hop behind, or aged out of the window: a race and an age no test holds still; out by decision
            }
            let floor = join.as_ref().map(|j| j.at).unwrap_or(0);
            let at = dated(f, key, &[], &notices, None, floor);
            Some(InboxRow {
                key: key.clone(),
                kind,
                source: InboxSource::of(kind, None),
                origin: None,
                title: label
                    .clone()
                    .unwrap_or_else(|| format!("{}…", key.chars().take(8).collect::<String>())),
                latest_at: at.latest_at,
                unread_count: 0,
                read: f.read_at(key, at.news_at),
                handled: false,
                mentioned: false,
                needs_action: vec![],
                unread_notices: f.unread_notices(key, &notices),
                notices,
                decided: None,
                representative: None,
                status: None,
                join,
                waiting: None,
            })
        }
        // A harness a person opened in a terminal, waiting on them at its
        // own prompt (ide/06 §Reporting): a row while the wait is, keyed by
        // the session, titled by the harness and where it stands. Owed like
        // an ask; answered in the terminal, never here.
        Thing::Session(p) => {
            // The session's own wait, else a sub-agent's: one row either
            // way, dated from the wait it names.
            let (on, child) = p.wait()?;
            let since = p.wait_since();
            let key = p.id.to_string();
            let place = p
                .workstream
                .and_then(|w| ws.get_workstream(w).ok().map(|w| w.label(None)))
                .or_else(|| {
                    p.project
                        .and_then(|id| ws.get_project(id).ok().map(|pr| pr.name))
                })
                .or_else(|| p.goal.map(|g| format!("goal {g}")));
            let title = match place {
                Some(place) => format!("{} · {place}", p.harness),
                None => p.harness.clone(), // LCOV_EXCL_LINE a harness row with no place, which no inbox journey stages; out by decision
            };
            Some(InboxRow {
                key: key.clone(),
                kind,
                source: InboxSource::of(kind, None),
                origin: None,
                title,
                latest_at: since,
                unread_count: 0,
                read: f.read_at(&key, since),
                handled: false,
                mentioned: false,
                needs_action: vec![],
                unread_notices: 0,
                notices: vec![],
                decided: None,
                representative: None,
                status: None,
                join: None,
                waiting: Some(WaitingSession {
                    session: key,
                    harness: p.harness.clone(),
                    workstream: p.workstream.map(|w| w.to_string()),
                    project: p.project.map(|id| id.to_string()),
                    goal: p.goal.map(|g| g.to_string()),
                    on: on.clone(),
                    words: on.words(),
                    subagent: child.map(|c| c.name.clone()),
                    since,
                }),
            })
        }
    }
}

/// The label a people notice carries for its person — what a row is titled
/// by once the person is no longer a member.
fn label_in(notices: &[NoticeDto]) -> Option<String> {
    notices.iter().find_map(|n| match &n.row.event {
        crate::feed::PulseEvent::Engine(event) => event
            .0
            .get("label")
            .and_then(|l| l.as_str())
            .filter(|l| !l.is_empty())
            .map(str::to_string),
        _ => None, // LCOV_EXCL_LINE a people notice is read from an engine fact alone (`notices::notice_of`), so no other arm holds one; by construction
    })
}

/// Every thing a row may be about, in the order the list reads them. One
/// record this node cannot read costs its own row — said at `warn` — never
/// the list.
fn things(r: &Reader<'_>, f: &mut Facts) -> Result<Vec<Thing>, StoreError> {
    let ws = r.ws;
    let mut out: Vec<Thing> = Vec::new();
    let goals = std::mem::take(&mut f.goals);
    let listed: HashSet<String> = goals.iter().map(|g| g.id.to_string()).collect();
    out.extend(goals.into_iter().map(|g| Thing::Goal(Box::new(g))));
    // A gate can name a goal that `list_goals` no longer returns; keep
    // those visible rather than losing the decision they are waiting for.
    let mut orphans: Vec<String> = f
        .needs
        .keys()
        .filter(|key| !listed.contains(*key))
        .cloned()
        .collect();
    orphans.sort();
    out.extend(orphans.into_iter().map(Thing::OrphanGoal));
    out.extend(
        ws.list_channels()?
            .into_iter()
            .map(|c| Thing::Channel(Box::new(c))),
    );
    out.extend(
        ws.list_conversations(&bisa_store::ConversationFilter {
            archived: Some(false),
            limit: usize::MAX / 2,
            ..Default::default()
        })?
        .into_iter()
        .map(|c| Thing::Conversation(Box::new(c))),
    );
    // A project's primary workstream is the project itself, under its id:
    // one row, the project's.
    let (workstreams, unreadable) = ws.list_workstreams_readable(WorkstreamFilter::All)?;
    for e in unreadable {
        tracing::warn!(target: "bisa_engine", "a workstream record cannot be read; the Inbox shows the rest: {e}");
    }
    out.extend(
        workstreams
            .into_iter()
            .filter(|w| !w.is_primary())
            .map(|w| Thing::Workstream(Box::new(w))),
    );
    out.extend(
        ws.list_projects()?
            .into_iter()
            .map(|p| Thing::Project(Box::new(p))),
    );
    // Every workflow is asked — a goal's design and an archived one too,
    // since the archiving is the notice.
    let mut named: HashSet<String> = HashSet::new();
    for wf in ws.list_workflows()? {
        named.insert(wf.id.to_string());
        out.push(Thing::Workflow {
            id: wf.id.to_string(),
            name: wf.name,
            created_at: wf.created_at,
        });
    }
    // A run's ask outlives its workflow's file: the row is the workflow's
    // still, titled by the name the run carries.
    let mut unlisted: Vec<String> = f
        .workflow_needs
        .keys()
        .filter(|id| !named.contains(*id))
        .cloned()
        .collect();
    unlisted.sort();
    for id in unlisted {
        let name = f
            .workflow_names
            .get(&id)
            .cloned()
            .unwrap_or_else(|| id.clone());
        out.push(Thing::Workflow {
            id,
            name,
            created_at: 0,
        });
    }
    // The persons hosted here, the ones asking to be, and — a row is
    // earned by a notice — the ones a notice still names once they left.
    let mut people = f.people.clone();
    let mut left: Vec<String> = f
        .notices
        .keys()
        .filter(|t| t.kind == InboxKind::People && !people.iter().any(|(k, _)| *k == t.id))
        .map(|t| t.id.clone())
        .collect();
    left.sort();
    for key in left {
        let label = f
            .notices
            .get(&InboxTarget {
                kind: InboxKind::People,
                id: key.clone(),
            })
            .and_then(|notices| label_in(notices));
        people.push((key, label));
    }
    out.extend(
        people
            .into_iter()
            .map(|(key, label)| Thing::Person { key, label }),
    );
    out.extend(
        r.live
            .waiting_terminals()
            .into_iter()
            .map(|p| Thing::Session(Box::new(p))),
    );
    Ok(out)
}

/// Whether `key` is a string a row could be keyed by: a channel's handle or a
/// person's public key (both lower-case words), or the ULID a goal, a
/// conversation, a project, a workstream, a workflow and a session are named
/// by. Shape alone — the thing may be gone, and a mark for a row deleted
/// under its reader is still a mark; what this refuses is a string no row
/// ever had, which a read mark would otherwise be kept for.
pub fn is_key(key: &str) -> bool {
    bisa_core::ChannelId::new(key).is_ok() || key.parse::<LiveRunId>().is_ok()
}

/// The thing one key names, or nothing: what a frame is about. The order is
/// the one a key is unambiguous in — a project before its primary
/// workstream, which shares its id.
fn resolve(r: &Reader<'_>, f: &mut Facts, key: &str) -> Option<Thing> {
    let ws = r.ws;
    if let Ok(id) = key.parse::<LiveRunId>() {
        if let Some(p) = r.live.terminal(id) {
            return Some(Thing::Session(Box::new(p.clone())));
        }
    }
    if let Some(goal) = f.goals.pop() {
        return Some(Thing::Goal(Box::new(goal)));
    }
    if let Ok(id) = bisa_core::ChannelId::new(key) {
        if let Ok(c) = ws.get_channel(&id) {
            return Some(Thing::Channel(Box::new(c)));
        }
    }
    if let Ok(id) = key.parse::<bisa_core::ConversationId>() {
        if let Ok(c) = ws.conversation_row(id) {
            return Some(Thing::Conversation(Box::new(c)));
        }
    }
    if let Ok(id) = key.parse::<bisa_core::ProjectId>() {
        if let Ok(p) = ws.get_project(id) {
            return Some(Thing::Project(Box::new(p)));
        }
    }
    if let Ok(id) = key.parse::<bisa_core::WorkstreamId>() {
        if let Ok(w) = ws.get_workstream(id) {
            return Some(Thing::Workstream(Box::new(w)));
        }
    }
    if let Ok(id) = key.parse::<bisa_core::WorkflowId>() {
        if let Ok(wf) = ws.get_workflow(id) {
            return Some(Thing::Workflow {
                id: key.to_string(),
                name: wf.name,
                created_at: wf.created_at,
            });
        }
    }
    if f.workflow_needs.contains_key(key) {
        let name = f
            .workflow_names
            .get(key)
            .cloned()
            .unwrap_or_else(|| key.to_string());
        return Some(Thing::Workflow {
            id: key.to_string(),
            name,
            created_at: 0,
        });
    }
    let person = InboxTarget {
        kind: InboxKind::People,
        id: key.to_string(),
    };
    if let Some((_, label)) = f.people.iter().find(|(k, _)| k == key) {
        return Some(Thing::Person {
            key: key.to_string(),
            label: label.clone(),
        });
    }
    if let Some(notices) = f.notices.get(&person) {
        return Some(Thing::Person {
            key: key.to_string(),
            label: label_in(notices),
        });
    }
    f.needs
        .contains_key(key)
        .then(|| Thing::OrphanGoal(key.to_string()))
}

/// Every row of the Inbox, in order: what is owed first, then what is new,
/// then everything kept — the newest first within each, and by key where
/// two are of one second, so two reads agree. The node filters by bucket
/// and source (`GET /inbox`); the command line prints them with no node,
/// from the same builder.
pub fn rows(r: &Reader<'_>) -> Result<Vec<InboxRow>, StoreError> {
    let mut f = Facts::gather(r, Scope::All)?;
    let mut rows: Vec<InboxRow> = things(r, &mut f)?
        .iter()
        .filter_map(|thing| build(thing, &mut f, r))
        .collect();
    rows.sort_by(|a, b| {
        rank(a)
            .cmp(&rank(b))
            .then(b.latest_at.cmp(&a.latest_at))
            .then_with(|| a.key.cmp(&b.key))
    });
    Ok(rows)
}

/// One thing's row, when it has earned one — what [`rows`] would hold for
/// that key, built by the same function.
pub fn row(r: &Reader<'_>, key: &str) -> Result<Option<InboxRow>, StoreError> {
    let mut f = Facts::gather(r, Scope::Key(key))?;
    Ok(resolve(r, &mut f, key).and_then(|thing| build(&thing, &mut f, r)))
}

/// Sort bucket: what is owed, then what is new, then everything kept. Within
/// a bucket the newest is first.
fn rank(r: &InboxRow) -> u8 {
    if owes(r) {
        0
    } else if !r.read {
        1
    } else {
        2
    }
}

/// One row's inbox state, for the SSE `inbox` channel.
///
/// This is what lets a client change a row *in place* — bold to dim, gate to
/// decided, a notice counted — instead of refetching the list and replacing
/// the array under a selection the reader is in the middle of using. It is
/// **a projection of the row** [`rows`] holds for the key — built by the one
/// function — so a frame and the list say the same of it: its clock, whether
/// it is read, what it owes, how many notices it carries and how many are
/// unread.
///
/// `listed` says whether the key has a row at all. A key with none — a
/// channel nobody named you in, a session no longer waiting, a key nothing
/// answers to — still gets its frame, with what is true of it (its unread
/// messages, its clock) and nothing owed: a client that holds no row for it
/// has nothing to read again.
pub fn delta(r: &Reader<'_>, key: &str) -> Option<serde_json::Value> {
    match try_delta(r, key) {
        Ok(frame) => Some(frame),
        // LCOV_EXCL_START: the row's reads refused mid-frame; a disk arm
        Err(e) => {
            // A frame that is not sent leaves a row stale with nothing on
            // screen to say why; the log says which row, and the next read of
            // the list corrects it.
            tracing::warn!(target: "bisa_engine", key, "no inbox frame for a row that moved: {e}");
            None
            // LCOV_EXCL_STOP
        }
    }
}

/// [`delta`], with the read that failed kept.
fn try_delta(r: &Reader<'_>, key: &str) -> Result<serde_json::Value, StoreError> {
    let mut f = Facts::gather(r, Scope::Key(key))?;
    let thing = resolve(r, &mut f, key);
    let kind = thing.as_ref().map(Thing::kind);
    let row = thing.as_ref().and_then(|thing| build(thing, &mut f, r));
    let Some(row) = row else {
        // No row: what is true of the key all the same.
        let latest_at = f.activity.get(key).copied().unwrap_or(0);
        let news_at = f.others.get(key).copied().unwrap_or(0);
        let mut frame = json!({
            "key": key,
            "kind": kind,
            "unread_count": f.unread_of(key),
            "needs_action_count": 0,
            "latest_at": latest_at,
            "read": f.read_at(key, news_at),
            "handled": false,
            "notice_count": 0,
            "unread_notices": 0,
            "listed": false,
        });
        if kind == Some(InboxKind::Session) {
            frame["waiting"] = json!(false);
        }
        return Ok(frame);
    };
    let waiting = row.waiting.is_some();
    let mut frame = json!({
        "key": key,
        "kind": row.kind,
        "unread_count": row.unread_count,
        // A harness waiting at its prompt is owed like an ask.
        "needs_action_count": row.needs_action.len() + usize::from(waiting),
        "latest_at": row.latest_at,
        "read": row.read,
        "handled": row.handled,
        "notice_count": row.notices.len(),
        "unread_notices": row.unread_notices,
        "listed": true,
    });
    if waiting {
        frame["waiting"] = json!(true);
    }
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_core::event::JournalPayload;
    use bisa_core::AskKind;
    use bisa_store::{MemoryKeyStore, NewGoal};

    /// The guard lets most calls through, and each is a row of the feed:
    /// a thousand of them never push a failed run out of the Inbox, and a
    /// call it refused among the newest is a notice on its goal.
    #[test]
    fn allowed_decisions_never_crowd_out_a_failure() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open_with_keystore(
            dir.path(),
            Box::new(bisa_store::MemoryKeyStore::default()),
        )
        .unwrap();
        let goal = ws
            .create_goal(bisa_store::NewGoal::captured("guarded work"))
            .unwrap()
            .id
            .to_string();
        let row = |at: u64, kind: &str, event: serde_json::Value| bisa_core::ActivityFact {
            at,
            concept: bisa_core::ActivityConcept::Agents,
            kind: kind.to_string(),
            source: bisa_core::ActivitySource::new(
                bisa_core::ActivitySourceKind::Goal,
                goal.clone(),
            ),
            author: None,
            event,
        };
        let guard = |verdict: &str| json!({"type": "guard_decided", "verdict": verdict});
        ws.record_activity(
            &row(
                1,
                "run_finished",
                json!({"type": "run_finished", "outcome": "failed"}),
            ),
            false,
        )
        .unwrap();
        ws.record_activity(&row(2, GUARD_TAG, guard("denied")), false)
            .unwrap();
        for at in 0..(NOTICE_WINDOW as u64 + 50) {
            ws.record_activity(&row(10 + at, GUARD_TAG, guard("allowed")), false)
                .unwrap();
        }
        ws.record_activity(&row(5_000, GUARD_TAG, guard("denied")), false)
            .unwrap();

        let notices = Facts::gather_notices(&ws, &HashSet::new()).unwrap();
        let kinds: Vec<notices::NoticeKind> = notices
            .values()
            .flatten()
            .map(|notice| notice.notice)
            .collect();
        assert_eq!(
            kinds,
            [
                notices::NoticeKind::GuardRefused,
                notices::NoticeKind::RunFailed
            ],
            "the newest refusal, and the failure the decisions did not push out — \
             the refusal older than the guard's own window stays in the Pulse"
        );
    }

    fn workspace(dir: &tempfile::TempDir) -> Workspace {
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap()
    }

    /// The refusal of a result that had to be one — `ApiError` is not `Debug`.
    fn say(text: &bisa_core::Text) -> String {
        text.to_string()
    }

    /// A journal is read only for a home that can owe something from it: a
    /// goal that journaled a question and is still open, a run whose
    /// workflow's rows hold one. When the feed cannot say, every journal is
    /// read, as before.
    #[test]
    fn a_journal_is_read_only_for_a_home_that_asked_and_is_open() {
        let dir = tempfile::tempdir().unwrap();
        let ws = workspace(&dir);
        let asked_goal = ws.create_goal(NewGoal::captured("asked")).unwrap();
        let quiet = ws.create_goal(NewGoal::captured("never asked")).unwrap();
        let owner = ws.owner_keys().clone();
        ws.append_journal(
            &Home::Goal {
                goal: asked_goal.id,
            },
            JournalPayload::Question {
                work_item: None,
                gate: format!("ask_human:{}", asked_goal.id),
                text: "Which database?".into(),
                expects: AskKind::free_text(),
            },
            &owner,
            None,
        )
        .unwrap();
        let asked = Asked::read(&ws);
        assert!(asked.goal(&asked_goal));
        assert!(!asked.goal(&quiet), "nothing was ever asked there");
        // What the builder shows follows: the designer's question on the
        // one, nothing read on the other.
        assert_eq!(
            durable_actions(&ws, &say, &asked, &asked_goal, None).len(),
            1
        );
        assert!(durable_actions(&ws, &say, &asked, &quiet, None).is_empty());
        // Closing withdraws every question of a goal's own.
        let (closed, _) = ws
            .set_goal_closed(
                asked_goal.id,
                bisa_core::ClosureReason::Abandoned { rationale: None },
            )
            .unwrap();
        assert!(!asked.goal(&closed));
        // The feed could not say: every home is read.
        assert!(Asked(None).goal(&quiet));
        assert!(
            !Asked(None).goal(&closed),
            "a closed goal still owes nothing"
        );
    }

    /// A member file or an invitation list this node cannot read costs the
    /// people's rows read from it, never the Inbox — and with no engine
    /// there is nothing live, so the rows are what the workspace alone says.
    #[test]
    fn unreadable_people_files_never_cost_the_inbox() {
        let dir = tempfile::tempdir().unwrap();
        let ws = workspace(&dir);
        let goal = ws.create_goal(NewGoal::captured("still here")).unwrap();
        let owner = ws.owner_keys().clone();
        ws.append_journal(
            &Home::Goal { goal: goal.id },
            JournalPayload::Question {
                work_item: None,
                gate: format!("ask_human:{}", goal.id),
                text: "Which database?".into(),
                expects: AskKind::free_text(),
            },
            &owner,
            None,
        )
        .unwrap();
        std::fs::write(ws.paths().members_file(), b"{ not json").unwrap();
        std::fs::write(ws.paths().invites_file(), b"{ not json").unwrap();
        assert!(ws.people().is_err() && ws.invites().is_err(), "staged");

        let (people, joins) = Facts::gather_people(&ws);
        assert!(people.is_empty() && joins.is_empty());
        let live = LiveView::none();
        let reader = Reader {
            ws: &ws,
            live: &live,
            say: &say,
        };
        let listed = rows(&reader).expect("the Inbox is read all the same");
        assert_eq!(listed.len(), 1, "the goal's question is still owed");
        assert_eq!(listed[0].key, goal.id.to_string());
        assert_eq!(listed[0].needs_action.len(), 1);
        // One key read alone says the same, and its frame is that row's.
        let one = row(&reader, &listed[0].key).unwrap().expect("a row");
        assert_eq!(one.key, listed[0].key);
        let frame = delta(&reader, &one.key).expect("a frame");
        assert_eq!(frame["listed"], json!(true));
        assert_eq!(frame["needs_action_count"], json!(1));
    }

    /// A key is a channel's handle, a person's public key or a ULID — the
    /// shapes a row is keyed by — and nothing else.
    #[test]
    fn a_key_is_one_of_the_shapes_a_row_is_keyed_by() {
        for key in [
            "general",
            "dm-ab12",
            "01ARZ3NDEKTSV4RRFFQ69G5FAV",
            &"ab".repeat(32),
        ] {
            assert!(is_key(key), "{key}");
        }
        for key in ["", "Not A Key!", "../../etc", "UPPER-case", &"a".repeat(65)] {
            assert!(!is_key(key), "{key:?}");
        }
    }

    /// A held amendment's question is rebuilt under its subject while the
    /// run it names goes on, carries that run, and disappears once decided —
    /// so a restart mid-amendment leaves the goal answerable, as the docs
    /// promise.
    #[test]
    fn a_held_amendment_question_is_rebuilt_until_decided() {
        let dir = tempfile::tempdir().unwrap();
        let ws = workspace(&dir);
        let goal = ws.create_goal(NewGoal::captured("amend me")).unwrap();
        let run_id: bisa_core::RunId = "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap();
        let held: bisa_core::WorkflowId = "01BX5ZZKBKACTAV9WEVGEMMVRZ".parse().unwrap();
        let subject = format!("amend:{run_id}@{held}");
        let owner = ws.owner_keys().clone();
        ws.append_journal(
            &Home::Goal { goal: goal.id },
            JournalPayload::Question {
                work_item: None,
                gate: subject.clone(),
                text: "Amend the running workflow?".into(),
                expects: AskKind::Decision,
            },
            &owner,
            None,
        )
        .unwrap();

        let journal = ws.journal(&Home::Goal { goal: goal.id }).unwrap();
        let q = newest_open(&journal, &format!("amend:{run_id}@")).expect("pending");
        assert_eq!(q.subject, subject);
        assert_eq!(q.text, "Amend the running workflow?");
        assert!(
            newest_open(&journal, "amend:01OTHERRUN@").is_none(),
            "another run's prefix finds nothing"
        );
        assert_eq!(
            proposed_workflow(&subject),
            Some(held),
            "the held copy is after the @"
        );
        assert_eq!(
            proposed_workflow(&format!("adopt:{held}@3")),
            Some(held),
            "a design is before the @"
        );
        assert_eq!(proposed_workflow("approval:x/y"), None);

        ws.append_journal(
            &Home::Goal { goal: goal.id },
            JournalPayload::Decision {
                gate: Gate::Approval,
                approve: true,
                subject: subject.clone(),
                rationale: None,
                answer: None,
            },
            &owner,
            None,
        )
        .unwrap();
        let journal = ws.journal(&Home::Goal { goal: goal.id }).unwrap();
        assert!(
            newest_open(&journal, &format!("amend:{run_id}@")).is_none(),
            "decided"
        );
    }

    /// A waiting step whose prompt cannot render any more — an upstream
    /// output gone with a failure — is never shown as written: a placeholder
    /// is not a question. The row says the step and why.
    #[test]
    fn a_question_that_cannot_render_is_never_shown_verbatim() {
        use bisa_core::{CheckKind, Join, OnFail, RunEvent, Step, StepKind, Workflow, WorkflowRun};
        let dir = tempfile::tempdir().unwrap();
        let ws = workspace(&dir);
        let goal = ws.create_goal(NewGoal::captured("ask me")).unwrap();
        let sid = |s: &str| bisa_core::StepId::new(s).unwrap();
        let plain = |id: &str, name: &str, kind: StepKind, then: &[&str]| Step {
            id: sid(id),
            name: name.into(),
            kind,
            then: then.iter().map(|t| bisa_core::Flow::to(sid(t))).collect(),
            boundaries: vec![],
            join: Join::All,
            on_fail: OnFail::Fail,
            retries: 0,
            max_visits: 3,
            position: None,
        };
        let mut probe = plain(
            "probe",
            "Probe",
            StepKind::Check {
                check: CheckKind::Command {
                    command: "true".into(),
                    project: None,
                },
            },
            &["ask"],
        );
        probe.on_fail = OnFail::Skip;
        let ask = plain(
            "ask",
            "Ask",
            StepKind::Human {
                prompt: "Is {steps.probe.output.evidence} fine?".into(),
                options: vec![],
                multi: false,
                assignee: None,
            },
            &[],
        );
        let workflow = Workflow {
            id: "01BX5ZZKBKACTAV9WEVGEMMVRZ".parse().unwrap(),
            name: "Probing".into(),
            description: String::new(),
            inputs: vec![],
            steps: vec![probe, ask],
            origin: bisa_core::WorkflowOrigin::Workspace,
            author: ws.owner_principal(),
            tags: Default::default(),
            revision: 1,
            archived: None,
            decision_making: false,
            created_at: 0,
        };
        let mut run = WorkflowRun::new(
            "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
            bisa_core::RunScope::Goal { goal: goal.id },
            workflow,
            Default::default(),
            bisa_core::RunEntry::by_hand(),
            0,
        );
        run.apply(RunEvent::Start, 1).unwrap();
        run.apply(
            RunEvent::StepFailed {
                step: sid("probe"),
                error: "exit 1".into(),
            },
            2,
        )
        .unwrap();
        assert_eq!(run.steps[&sid("ask")].state, StepState::Waiting);
        let actions = durable_actions(&ws, &say, &Asked(None), &goal, Some(&run));
        assert_eq!(actions.len(), 1, "{actions:?}");
        let q = &actions[0].question;
        // Never the prompt as written: the reason names the placeholder that
        // had no value, but the question is not the unrendered sentence.
        assert!(
            !q.contains("Is {steps.probe.output.evidence} fine?"),
            "never verbatim: {q}"
        );
        assert_eq!(
            q,
            "Step Ask — its question could not be rendered: {steps.probe.output.evidence} has no value in this run: step `probe` failed — exit 1"
        );
    }

    /// The design question is rebuilt from the journal by its stable subject,
    /// and disappears once a decision names that subject — so a restart mid-
    /// question leaves the goal answerable.
    #[test]
    fn an_unanswered_design_question_is_rebuilt_from_the_journal_until_it_is_decided() {
        let dir = tempfile::tempdir().unwrap();
        let ws = workspace(&dir);
        let goal = ws.create_goal(NewGoal::captured("ask me")).unwrap();
        assert!(
            durable_actions(&ws, &say, &Asked(None), &goal, None).is_empty(),
            "nothing asked yet"
        );

        let owner = ws.owner_keys().clone();
        let expects = AskKind::free_text();
        ws.append_journal(
            &Home::Goal { goal: goal.id },
            JournalPayload::Question {
                work_item: None,
                gate: format!("ask_human:{}", goal.id),
                text: "Which tone?".into(),
                expects: expects.clone(),
            },
            &owner,
            None,
        )
        .unwrap();
        let actions = durable_actions(&ws, &say, &Asked(None), &goal, None);
        assert_eq!(actions.len(), 1, "{actions:?}");
        let a = &actions[0];
        assert_eq!(a.gate_kind, Gate::Escalation);
        assert_eq!(a.question, "Which tone?");
        assert_eq!(a.expects, expects);
        assert!(a.durable && a.gate_id.is_none());
        assert_eq!(
            a.subject.as_deref(),
            Some(format!("ask_human:{}", goal.id).as_str())
        );

        ws.append_journal(
            &Home::Goal { goal: goal.id },
            JournalPayload::Decision {
                gate: Gate::Escalation,
                approve: true,
                subject: format!("ask_human:{}", goal.id),
                rationale: None,
                answer: Some(bisa_core::Answer::text("warm")),
            },
            &owner,
            None,
        )
        .unwrap();
        assert!(
            durable_actions(&ws, &say, &Asked(None), &goal, None).is_empty(),
            "answered"
        );
    }

    /// A run of the workspace owes its asks on its own home: a waiting
    /// step's question, rendered with no goal to read, and an agent's
    /// question nothing has settled — gone once it is decided. No adoption
    /// and no amendment is ever rebuilt for it: only a goal owes those.
    #[test]
    fn a_workspace_runs_asks_are_rebuilt_on_its_own_home() {
        use bisa_core::{Join, OnFail, RunScope, Step, StepId};
        let dir = tempfile::tempdir().unwrap();
        let ws = workspace(&dir);
        let which = Step {
            id: StepId::new("which").unwrap(),
            name: "Which".into(),
            kind: StepKind::Human {
                prompt: "Which region?".into(),
                options: vec![],
                multi: false,
                assignee: None,
            },
            then: vec![],
            boundaries: vec![],
            join: Join::All,
            on_fail: OnFail::Fail,
            retries: 0,
            max_visits: 3,
            position: None,
        };
        let wf = ws
            .create_workflow(
                bisa_store::NewWorkflow {
                    name: "Asks".into(),
                    description: String::new(),
                    inputs: vec![],
                    steps: vec![which],
                    tags: Default::default(),
                    decision_making: false,
                },
                bisa_core::WorkflowOrigin::Workspace,
            )
            .unwrap();
        let (run, _) = ws
            .create_run(
                RunScope::Workspace {
                    budget: Default::default(),
                },
                wf.id,
                Default::default(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        let actions = durable_run_actions(&ws, &say, &Asked(None), &run);
        assert_eq!(actions.len(), 1, "{actions:?}");
        assert_eq!(actions[0].home, run.home());
        assert_eq!(actions[0].question, "Which region?");
        assert_eq!(actions[0].run, Some(run.id.to_string()));
        assert_eq!(actions[0].subject, Some(format!("step:{}/which", run.id)));

        let owner = ws.owner_keys().clone();
        let subject = "ask_human:01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string();
        ws.append_journal(
            &run.home(),
            JournalPayload::Question {
                work_item: None,
                gate: subject.clone(),
                text: "Which tone?".into(),
                expects: AskKind::free_text(),
            },
            &owner,
            None,
        )
        .unwrap();
        let actions = durable_run_actions(&ws, &say, &Asked(None), &run);
        assert_eq!(actions.len(), 2, "{actions:?}");
        let ask = actions
            .iter()
            .find(|a| a.subject.as_deref() == Some(subject.as_str()))
            .expect("the agent's question");
        assert_eq!(ask.home, run.home());
        assert_eq!(ask.question, "Which tone?");

        ws.append_journal(
            &run.home(),
            JournalPayload::Decision {
                gate: Gate::Escalation,
                approve: true,
                subject: subject.clone(),
                rationale: None,
                answer: Some(bisa_core::Answer::text("warm")),
            },
            &owner,
            None,
        )
        .unwrap();
        let actions = durable_run_actions(&ws, &say, &Asked(None), &run);
        assert_eq!(actions.len(), 1, "answered: only the step is owed");
    }
}
