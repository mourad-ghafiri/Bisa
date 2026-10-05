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
//! Membership widened the row set, so the per-row work had to narrow. Four
//! grouped queries are read once per request — unread counts, newest message
//! per scope, read markers, and the decision record (capped at
//! [`bisa_store::inbox::MAX_DECISIONS`]) — and the per-row message fetch
//! now happens **only for scopes that have a message**, which
//! `scope_activity` answers for nothing. A row that exists because a gate is
//! open and nobody has spoken costs no message query at all; before, dating a
//! row meant a `LIMIT 200` fetch per conversation whether or not it had one.
//! No journal is scanned: the `approvals` table is already the journal's
//! `Decision` events, re-derived by `rebuild_index`.

use crate::dto::*;
use crate::route_docs::RouteDoc;
use crate::Query;
use crate::{ApiError, Shared};
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use bisa_core::event::JournalPayload;
use bisa_core::WaitFor;
use bisa_core::{
    render_template, AskKind, ChannelKind, Gate, Goal, GoalId, GoalText, Home, RunId, StepKind,
    StepState, TemplateCtx, WorkflowRun,
};
use bisa_engine::notices::{self, InboxKind, InboxTarget};
use bisa_store::{approval_subject, MessageRow, Workspace, WorkstreamFilter};
use serde::Deserialize;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;

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

/// The newest notices a row carries.
const NOTICES_PER_ROW: usize = 20;

pub(crate) fn routes() -> Router<Shared> {
    Router::new().route("/inbox", get(inbox))
}

#[derive(Deserialize)]
struct InboxQuery {
    /// The primary bucket: all (default) | needs_you | unread.
    #[serde(default)]
    filter: Option<String>,
    /// The source, orthogonal to the bucket: goals | projects (workstreams
    /// and projects) | workflows (a workflow the Workflow Agent wrote, one
    /// put away, one whose listener failed) | messages (channels and direct
    /// channels) | people (persons hosted here); absent for every source.
    #[serde(default)]
    source: Option<String>,
}

/// The primary bucket of `GET /inbox`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bucket {
    All,
    NeedsYou,
    Unread,
}

impl Bucket {
    const NAMES: &'static [&'static str] = &["all", "needs_you", "unread"];

    /// Absent is `all`; a word that is no bucket is refused by name — a
    /// misspelt filter that answered every row would read as a full Inbox.
    fn parse(raw: Option<&str>) -> Result<Self, ApiError> {
        match raw.unwrap_or("all") {
            "all" => Ok(Self::All),
            "needs_you" => Ok(Self::NeedsYou),
            "unread" => Ok(Self::Unread),
            s => Err(crate::bad_request(bisa_core::text!(
                "error-node-inbox-unknown-filter-use-one",
                s = format!("{s:?}"),
                a0 = (Self::NAMES.join(", ")).to_string()
            ))),
        }
    }

    fn holds(self, r: &InboxRow) -> bool {
        match self {
            Self::All => true,
            Self::NeedsYou => owes(r),
            Self::Unread => !r.read,
        }
    }
}

/// Whether a row is owed something: an ask, a join to admit, a harness waiting.
fn owes(r: &InboxRow) -> bool {
    !r.needs_action.is_empty() || r.join.is_some() || r.waiting.is_some()
}

/// The sources a row can be asked by — `InboxSource::ALL`'s words, in the tabs' order.
const SOURCES: &[&str] = &["messages", "projects", "workflows", "goals", "people"];

/// Absent or empty is every source; a word that is no source is refused.
fn parse_source(raw: Option<&str>) -> Result<Option<&str>, ApiError> {
    match raw.unwrap_or("") {
        "" => Ok(None),
        s if SOURCES.contains(&s) => Ok(Some(s)),
        s => Err(crate::bad_request(bisa_core::text!(
            "error-node-inbox-unknown-source-use-one",
            s = format!("{s:?}"),
            a0 = (SOURCES.join(", ")).to_string()
        ))),
    }
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
pub(crate) fn preview_of(m: &MessageRow) -> Representative {
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
pub(crate) fn needs_actions_for(
    state: &Shared,
    goal: &Goal,
    run: Option<&WorkflowRun>,
) -> Vec<NeedsAction> {
    let home = Home::Goal { goal: goal.id };
    let live: Vec<NeedsAction> = state
        .engine
        .inbox()
        .into_iter()
        .filter(|g| g.home == home)
        .map(NeedsAction::from_gate)
        .collect();
    build_needs(state.engine.workspace(), goal, run, live)
}

/// Everything one run of the workspace owes a person: its live gates, else
/// the asks rebuilt from the run and its own journal. The Inbox files them
/// on the run's workflow's row, and `GET /runs/{rid}` shows them on the run
/// — the same builder, so the two agree.
pub(crate) fn needs_actions_for_run(state: &Shared, run: &WorkflowRun) -> Vec<NeedsAction> {
    let home = run.home();
    let live: Vec<NeedsAction> = state
        .engine
        .inbox()
        .into_iter()
        .filter(|g| g.home == home)
        .map(NeedsAction::from_gate)
        .collect();
    build_run_needs(state.engine.workspace(), run, live)
}

/// One run of the workspace's actions from its already-filtered live gates:
/// the live ones, else the asks rebuilt from durable state. A run of the
/// workspace owes no adoption and no amendment — only a goal does.
fn build_run_needs(ws: &Workspace, run: &WorkflowRun, live: Vec<NeedsAction>) -> Vec<NeedsAction> {
    if !live.is_empty() {
        return live;
    }
    durable_run_actions(ws, run)
}

/// One goal's actions from its already-filtered live gates: the live ones, else
/// the asks rebuilt from the run and the journal, with the `adopt:` proposals
/// attached. The shared tail of `needs_actions_for` (one goal, live gates
/// re-scanned) and `Facts::gather` (all goals, gates scanned once) — so both
/// produce the identical card and neither re-scans per goal.
pub(crate) fn build_needs(
    ws: &Workspace,
    goal: &Goal,
    run: Option<&WorkflowRun>,
    live: Vec<NeedsAction>,
) -> Vec<NeedsAction> {
    let actions = if live.is_empty() {
        durable_actions(ws, goal, run)
    } else {
        live
    };
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

/// The newest journaled question whose gate starts with `prefix` and has no
/// decision on that gate after it — the one scan behind every durable
/// question the inbox rebuilds after a restart.
struct PendingQuestion {
    subject: String,
    text: String,
    expects: AskKind,
    at: u64,
}

fn pending_question(
    journal: &[bisa_core::event::JournalEvent],
    prefix: &str,
) -> Option<PendingQuestion> {
    for e in journal.iter().rev() {
        match &e.payload {
            JournalPayload::Question {
                gate,
                text,
                expects,
                ..
            } if gate.starts_with(prefix) => {
                return Some(PendingQuestion {
                    subject: gate.clone(),
                    text: text.clone(),
                    expects: expects.clone(),
                    at: e.at,
                });
            }
            // Decided, or taken back — by the asker, or by a restart that
            // found the asker dead: either way, nothing is owed.
            JournalPayload::Decision { subject, .. }
            | JournalPayload::Withdrawn { subject, .. }
                if subject.starts_with(prefix) =>
            {
                return None
            }
            _ => {}
        }
    }
    None
}

/// The asks a live run's waiting steps owe a person, rebuilt from the run
/// alone: a `human` step's question, an `approval` step's decision, a held
/// `wait` step's release — each filed at the run's home. `goal` renders a
/// goal's run's prompts; a run of the workspace has no goal to read.
fn waiting_step_actions(run: &WorkflowRun, goal: Option<&Goal>) -> Vec<NeedsAction> {
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
            continue;
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
                    bisa_i18n::english(&bisa_core::text!(
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
            Err(e) => bisa_i18n::english(&bisa_core::text!(
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
fn durable_run_actions(ws: &Workspace, run: &WorkflowRun) -> Vec<NeedsAction> {
    let mut out = waiting_step_actions(run, None);
    if run.is_finished() {
        return out;
    }
    let journal = ws.journal(&run.home()).unwrap_or_default();
    if let Some(q) = pending_question(&journal, "ask_human:") {
        out.push(NeedsAction {
            home: run.home(),
            gate_id: None,
            gate_kind: Gate::Escalation,
            run: Some(run.id.to_string()),
            step: None,
            question: q.text,
            expects: q.expects,
            durable: true,
            subject: Some(q.subject),
            work_item: None,
            opened_at: Some(q.at),
            proposal: None,
        });
    }
    out
}

pub(crate) fn durable_actions(
    ws: &Workspace,
    goal: &Goal,
    run: Option<&WorkflowRun>,
) -> Vec<NeedsAction> {
    let home = Home::Goal { goal: goal.id };
    let mut out = run
        .map(|r| waiting_step_actions(r, Some(goal)))
        .unwrap_or_default();
    let journal = ws.journal(&home).unwrap_or_default();
    // A held amendment still owed while the run goes on: the run names the
    // subject, so an earlier run's amendment is never rebuilt for this one.
    if let Some(run) = run.filter(|r| !r.is_finished()) {
        if let Some(q) = pending_question(&journal, &format!("amend:{}@", run.id)) {
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
        if let Some(q) = pending_question(&journal, "adopt:") {
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
        if let Some(q) = pending_question(&journal, &format!("ask_human:{}", goal.id)) {
            out.push(NeedsAction {
                home,
                gate_id: None,
                gate_kind: Gate::Escalation,
                run: None,
                step: None,
                question: q.text,
                expects: q.expects,
                durable: true,
                subject: Some(q.subject),
                work_item: None,
                opened_at: Some(q.at),
                proposal: None,
            });
        }
    }
    out
}

/// Everything the row builders read once per request.
struct Facts {
    /// Read once and carried: reconstructing the durable actions and
    /// building the rows are two passes over the same list, and each
    /// `list_goals` is a snapshot read per goal on disk.
    goals: Vec<Goal>,
    /// goal → its current run, read once beside the goal.
    runs: HashMap<String, WorkflowRun>,
    own: String,
    unread: HashMap<String, u64>,
    /// scope → newest live message. Absent means nothing was ever said.
    activity: HashMap<String, u64>,
    /// scope → (watermark, forced unread).
    markers: HashMap<String, (u64, bool)>,
    mentioned: HashSet<String>,
    /// goal → its most recent decision.
    decided: HashMap<String, Decided>,
    /// goal → what is waiting on a human right now.
    needs: HashMap<String, Vec<NeedsAction>>,
    /// workflow → what its runs of the workspace are waiting on a human for.
    workflow_needs: HashMap<String, Vec<NeedsAction>>,
    /// workflow → the most recent decision on one of its runs of the
    /// workspace: what keeps its row once the ask is answered.
    workflow_decided: HashMap<String, Decided>,
    /// thing → what happened to it that concerns a person, newest first,
    /// the newest `NOTICES_PER_ROW` — from one read of the activity index.
    notices: HashMap<InboxTarget, Vec<NoticeDto>>,
}

impl Facts {
    /// One bounded read of the feed, classified: the newest rows under the
    /// notice tags, each a notice or not (`notices::notice_of`), landed on
    /// the row it concerns (`notices::row_of`), typed as the Pulse types a
    /// row — a record this build cannot type is skipped there, never here.
    fn gather_notices(ws: &Workspace) -> Result<HashMap<InboxTarget, Vec<NoticeDto>>, ApiError> {
        let mut titles = crate::pulse::Titles::new(ws);
        let mut out: HashMap<InboxTarget, Vec<NoticeDto>> = HashMap::new();
        for record in ws.activity_by_kinds(notices::NOTICE_TAGS, NOTICE_WINDOW)? {
            let event = match serde_json::from_str::<serde_json::Value>(&record.event) {
                Ok(event) => event,
                Err(e) => {
                    // As the Pulse says it (`pulse::row_of`): once, with the row's `seq`.
                    tracing::warn!(target: "bisa_node", seq = record.seq, kind = %record.kind, "skipping an activity row the Inbox cannot read: {e}");
                    continue;
                }
            };
            let Some(notice) = notices::notice_of(&record.kind, &event) else {
                continue;
            };
            let Some(target) =
                notices::row_of(&record.kind, &record.source_kind, &record.source_id, &event)
            else {
                continue;
            };
            let rows = out.entry(target).or_default();
            if rows.len() >= NOTICES_PER_ROW {
                continue;
            }
            if let Some(row) = crate::pulse::row_of(record, &mut titles) {
                rows.push(NoticeDto { notice, row });
            }
        }
        Ok(out)
    }

    fn gather(state: &Shared) -> Result<Self, ApiError> {
        let ws = state.engine.workspace();
        let own = ws.owner_principal().as_hex().to_string();

        // The live gates, scanned ONCE and grouped by home — the
        // inbox once called the per-goal builder for every goal, and each call
        // cloned and sorted the whole pending-gate set, so a workspace with
        // many goals paid O(goals × gates). Each goal is then handed its own
        // slice through `build_needs`, the same tail the goal page uses, so the
        // `adopt:` proposal card is identical wherever it is mounted; each run
        // of the workspace its own through `build_run_needs`.
        let mut live_by_goal: HashMap<String, Vec<NeedsAction>> = HashMap::new();
        let mut live_by_run: HashMap<RunId, Vec<NeedsAction>> = HashMap::new();
        for gate in state.engine.inbox() {
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
        let goals = ws.list_goals(None)?;
        let mut needs: HashMap<String, Vec<NeedsAction>> = HashMap::new();
        let mut runs: HashMap<String, WorkflowRun> = HashMap::new();
        for goal in &goals {
            let key = goal.id.to_string();
            let run = crate::runs::current_run(ws, goal).into_run();
            let live = live_by_goal.remove(&key).unwrap_or_default();
            let actions = build_needs(ws, goal, run.as_ref(), live);
            if !actions.is_empty() {
                needs.insert(key.clone(), actions);
            }
            if let Some(run) = run {
                runs.insert(key, run);
            }
        }

        // Runs of the workspace: what each owes a person is its workflow's
        // row's — no goal holds it (I52).
        let mut workflow_needs: HashMap<String, Vec<NeedsAction>> = HashMap::new();
        for run in ws.live_workspace_runs(None)? {
            let live = live_by_run.remove(&run.id).unwrap_or_default();
            let actions = build_run_needs(ws, &run, live);
            if !actions.is_empty() {
                workflow_needs
                    .entry(run.workflow.id.to_string())
                    .or_default()
                    .extend(actions);
            }
        }
        // A live gate on a run the index no longer lists as live stays
        // visible on its workflow's row rather than lost.
        for (run, actions) in live_by_run {
            if let Ok(run) = ws.get_run(run) {
                workflow_needs
                    .entry(run.workflow.id.to_string())
                    .or_default()
                    .extend(actions);
            }
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
                (None, None) => {}
            }
        }

        let mentioned = ws.mentioned_scopes(&own)?.into_iter().collect();
        Ok(Self {
            goals,
            runs,
            own,
            unread: ws.unread_counts()?.into_iter().collect(),
            activity: ws.scope_activity()?.into_iter().collect(),
            markers: ws
                .read_markers()?
                .into_iter()
                .map(|m| (m.scope_id, (m.last_read_at, m.forced_unread)))
                .collect(),
            mentioned,
            decided,
            needs,
            workflow_needs,
            workflow_decided,
            notices: Self::gather_notices(ws)?,
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

    /// Whether the local watermark covers everything this row has to show.
    /// A forced-unread scope is never read, whatever the arithmetic says —
    /// putting a row back is a deliberate act and a clock may not undo it.
    fn read_at(&self, key: &str, latest_at: u64) -> bool {
        let (last_read_at, forced) = self.markers.get(key).copied().unwrap_or((0, false));
        !forced && last_read_at >= latest_at
    }
}

/// The newest thing this row has to show. A gate's clock is the moment it
/// **opened**, not the moment the page was rendered: a gate waiting nine days
/// used to date itself `now` on every request and so read as brand new
/// forever, hiding precisely the staleness worth surfacing.
fn latest_of(
    activity: Option<u64>,
    actions: &[NeedsAction],
    notices: &[NoticeDto],
    floor: u64,
) -> u64 {
    let gate_at = actions
        .iter()
        .filter_map(|a| a.opened_at)
        .max()
        .unwrap_or(0);
    let notice_at = notices.iter().map(|n| n.row.at).max().unwrap_or(0);
    activity.unwrap_or(0).max(gate_at).max(notice_at).max(floor)
}

/// The notices after a watermark `(last_read_at, forced_unread)` — every
/// one on a scope put back deliberately, none with no notices.
fn unread_notices_after(marker: Option<(u64, bool)>, notices: &[NoticeDto]) -> usize {
    let (last_read_at, forced) = marker.unwrap_or((0, false));
    if forced {
        return notices.len();
    }
    notices.iter().filter(|n| n.row.at > last_read_at).count()
}

async fn inbox(
    State(state): State<Shared>,
    Query(q): Query<InboxQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Refused before anything is read.
    let bucket = Bucket::parse(q.filter.as_deref())?;
    let source = parse_source(q.source.as_deref())?;
    let ws = state.engine.workspace();
    let mut f = Facts::gather(&state)?;
    let goals = std::mem::take(&mut f.goals);
    let mut rows: Vec<InboxRow> = Vec::new();

    // Goals.
    for goal in goals {
        let key = goal.id.to_string();
        let actions = f.needs.remove(&key).unwrap_or_default();
        let notices = f.take_notices(InboxKind::Goal, &key);
        let n_unread = f.unread_of(&key);
        let decided = f.decided.get(&key).cloned();
        let mentioned = f.mentioned.contains(&key);
        // Earned once and kept: a live gate, a decision on record, a mention,
        // a notice, or anything unread right now. Reading and deciding both
        // leave the first four untouched, which is the whole point.
        if actions.is_empty()
            && decided.is_none()
            && !mentioned
            && n_unread == 0
            && notices.is_empty()
        {
            continue;
        }
        let activity = f.activity.get(&key).copied();
        let msgs = if activity.is_some() {
            ws.messages(&key, None, MESSAGE_WINDOW).unwrap_or_default()
        } else {
            Vec::new()
        };
        let latest_at = latest_of(activity, &actions, &notices, goal.created_at);
        rows.push(InboxRow {
            key: key.clone(),
            kind: InboxKind::Goal,
            source: InboxSource::of(InboxKind::Goal, None),
            origin: None,
            title: goal
                .title
                .clone()
                .unwrap_or_else(|| goal.statement.chars().take(80).collect()),
            latest_at,
            unread_count: n_unread,
            read: f.read_at(&key, latest_at),
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
        });
    }
    // A gate can name a goal that `list_goals` no longer returns; keep
    // those visible rather than losing the decision they are waiting for.
    for (key, actions) in std::mem::take(&mut f.needs) {
        let notices = f.take_notices(InboxKind::Goal, &key);
        let latest_at = latest_of(f.activity.get(&key).copied(), &actions, &notices, 0);
        rows.push(InboxRow {
            key: key.clone(),
            kind: InboxKind::Goal,
            source: InboxSource::of(InboxKind::Goal, None),
            origin: None,
            title: key.clone(),
            latest_at,
            unread_count: 0,
            read: f.read_at(&key, latest_at),
            handled: false,
            mentioned: f.mentioned.contains(&key),
            needs_action: actions,
            unread_notices: f.unread_notices(&key, &notices),
            notices,
            decided: None,
            representative: None,
            status: None,
            join: None,
            waiting: None,
        });
    }

    // Channels and direct channels.
    for channel in ws.list_channels()? {
        let key = channel.id.to_string();
        let n_unread = f.unread_of(&key);
        let mentioned = f.mentioned.contains(&key);
        let is_dm = matches!(channel.kind, ChannelKind::Direct);
        // A DM is addressed to you by construction, so it keeps its row.
        // Standing-channel chatter is not: it earns one by naming you or by
        // being unread, and otherwise belongs in the Channels view.
        if !is_dm && !mentioned && n_unread == 0 {
            continue;
        }
        let activity = f.activity.get(&key).copied();
        let msgs = if activity.is_some() {
            ws.messages(&key, None, MESSAGE_WINDOW).unwrap_or_default()
        } else {
            Vec::new()
        };
        let latest_at = latest_of(activity, &[], &[], channel.created_at);
        let kind = if is_dm {
            InboxKind::Dm
        } else {
            InboxKind::Channel
        };
        rows.push(InboxRow {
            key: key.clone(),
            kind,
            source: InboxSource::of(kind, None),
            origin: None,
            title: channel.name.clone(),
            latest_at,
            unread_count: n_unread,
            read: f.read_at(&key, latest_at),
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
        });
    }

    // Conversations: one names you or is unread. Archived ones stay out —
    // a person put them away.
    for c in ws.list_conversations(&bisa_store::ConversationFilter {
        archived: Some(false),
        limit: usize::MAX / 2,
        ..Default::default()
    })? {
        let key = c.id.clone();
        let n_unread = f.unread_of(&key);
        let mentioned = f.mentioned.contains(&key);
        if !mentioned && n_unread == 0 {
            continue;
        }
        let activity = f.activity.get(&key).copied();
        let msgs = if activity.is_some() {
            ws.messages(&key, None, MESSAGE_WINDOW).unwrap_or_default()
        } else {
            Vec::new()
        };
        let latest_at = latest_of(activity, &[], &[], c.created_at);
        let origin = crate::dto::origin_of(&c);
        rows.push(InboxRow {
            key: key.clone(),
            kind: InboxKind::Conversation,
            source: InboxSource::of(InboxKind::Conversation, Some(&origin)),
            origin: Some(origin),
            title: c
                .title
                .clone()
                .or_else(|| c.first_line.clone())
                .unwrap_or_else(|| {
                    bisa_i18n::english(&bisa_core::text!("error-node-inbox-new-conversation"))
                }),
            latest_at,
            unread_count: n_unread,
            read: f.read_at(&key, latest_at),
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
        });
    }

    // Workstreams: something happened in them — a script failed, a pull
    // request opened or merged, a committer is wanted.
    for w in ws.list_workstreams(WorkstreamFilter::All)? {
        let key = w.id.to_string();
        let notices = f.take_notices(InboxKind::Workstream, &key);
        if notices.is_empty() {
            continue;
        }
        let latest_at = latest_of(None, &[], &notices, w.created_at);
        rows.push(InboxRow {
            key: key.clone(),
            kind: InboxKind::Workstream,
            source: InboxSource::of(InboxKind::Workstream, None),
            origin: None,
            title: w.label(None),
            latest_at,
            unread_count: 0,
            read: f.read_at(&key, latest_at),
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
        });
    }

    // Projects: a folder a workflow step made, and who commits in it.
    for p in ws.list_projects()? {
        let key = p.id.to_string();
        let notices = f.take_notices(InboxKind::Project, &key);
        if notices.is_empty() {
            continue;
        }
        let latest_at = latest_of(None, &[], &notices, p.created_at);
        rows.push(InboxRow {
            key: key.clone(),
            kind: InboxKind::Project,
            source: InboxSource::of(InboxKind::Project, None),
            origin: None,
            title: p.name.clone(),
            latest_at,
            unread_count: 0,
            read: f.read_at(&key, latest_at),
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
        });
    }

    // Workflows: what the Workflow Agent designed or proposed, one put away,
    // a listener of it that failed, and what its runs of the workspace ask
    // and did — no goal holds those. Every workflow is asked — a goal's
    // design and an archived one too, since the archiving is the notice. No
    // conversation; the door is the designer, or the run the ask names.
    for wf in ws.list_workflows()? {
        let key = wf.id.to_string();
        let notices = f.take_notices(InboxKind::Workflow, &key);
        let actions = f.workflow_needs.remove(&key).unwrap_or_default();
        let decided = f.workflow_decided.get(&key).cloned();
        if notices.is_empty() && actions.is_empty() && decided.is_none() {
            continue;
        }
        let latest_at = latest_of(None, &actions, &notices, wf.created_at);
        rows.push(InboxRow {
            key: key.clone(),
            kind: InboxKind::Workflow,
            source: InboxSource::of(InboxKind::Workflow, None),
            origin: None,
            title: wf.name.clone(),
            latest_at,
            unread_count: 0,
            read: f.read_at(&key, latest_at),
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
        });
    }

    // People (14-collaboration): somebody waiting to be admitted, a person
    // who joined or left, a message the classifier held. Keyed by pubkey;
    // the door is Settings › People.
    let waiting: Vec<bisa_core::Invite> = ws
        .invites()?
        .into_iter()
        .filter(|i| matches!(i.state, bisa_core::InviteState::Requested { .. }))
        .collect();
    let mut people_keys: Vec<(String, Option<String>)> = ws
        .people()?
        .into_iter()
        .map(|m| (m.pubkey.as_hex().to_string(), m.label))
        .collect();
    for invite in &waiting {
        if let bisa_core::InviteState::Requested { by, label, .. } = &invite.state {
            if !people_keys.iter().any(|(k, _)| k == by.as_hex()) {
                people_keys.push((by.as_hex().to_string(), label.clone()));
            }
        }
    }
    for (key, label) in people_keys {
        let notices = f.take_notices(InboxKind::People, &key);
        let join = waiting.iter().find_map(|i| match &i.state {
            bisa_core::InviteState::Requested { by, label, at } if by.as_hex() == key => {
                Some(JoinRequest {
                    invite: i.id.to_string(),
                    role: i.role,
                    label: label.clone(),
                    at: *at,
                })
            }
            _ => None,
        });
        if notices.is_empty() && join.is_none() {
            continue;
        }
        let latest_at = latest_of(
            None,
            &[],
            &notices,
            join.as_ref().map(|j| j.at).unwrap_or(0),
        );
        rows.push(InboxRow {
            key: key.clone(),
            kind: InboxKind::People,
            source: InboxSource::of(InboxKind::People, None),
            origin: None,
            title: label.unwrap_or_else(|| format!("{}…", &key[..8])),
            latest_at,
            unread_count: 0,
            read: f.read_at(&key, latest_at),
            handled: false,
            mentioned: false,
            needs_action: vec![],
            unread_notices: f.unread_notices(&key, &notices),
            notices,
            decided: None,
            representative: None,
            status: None,
            join,
            waiting: None,
        });
    }

    // Harnesses a person opened in a terminal, waiting on them at their own
    // prompt (ide/06 §Reporting): a row while the wait is, keyed by the
    // session, titled by the harness and where it stands. Owed like an
    // ask; answered in the terminal, never here.
    for p in state.engine.inner().presence.waiting_terminals() {
        // The session's own wait, else a sub-agent's: one row either way,
        // dated from the wait it names.
        let Some((on, child)) = p.wait() else {
            continue;
        };
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
            None => p.harness.clone(),
        };
        rows.push(InboxRow {
            key: key.clone(),
            kind: InboxKind::Session,
            source: InboxSource::of(InboxKind::Session, None),
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
        });
    }

    rows.retain(|r| bucket.holds(r) && source.is_none_or(|s| r.source.as_str() == s));
    rows.sort_by(|a, b| rank(a).cmp(&rank(b)).then(b.latest_at.cmp(&a.latest_at)));
    Ok(Json(json!({"rows": rows})))
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

/// The kind of thing a key names — a goal, a channel or direct channel, a
/// workstream, a project, a workflow, a terminal harness on the roster —
/// for a frame that arrives with the key alone (a message, a read marker, a
/// session's state).
fn kind_of_key(state: &Shared, key: &str) -> Option<InboxKind> {
    let ws = state.engine.workspace();
    if let Ok(id) = key.parse::<bisa_engine::LiveRunId>() {
        if state
            .engine
            .inner()
            .presence
            .get(id)
            .is_some_and(|p| p.kind == bisa_engine::registry::SessionKind::Terminal)
        {
            return Some(InboxKind::Session);
        }
    }
    if let Ok(id) = GoalId::from_str(key) {
        if ws.get_goal(id).is_ok() {
            return Some(InboxKind::Goal);
        }
    }
    if let Ok(id) = bisa_core::ChannelId::new(key) {
        if let Ok(c) = ws.get_channel(&id) {
            return Some(if matches!(c.kind, ChannelKind::Direct) {
                InboxKind::Dm
            } else {
                InboxKind::Channel
            });
        }
    }
    if let Ok(id) = key.parse::<bisa_core::ConversationId>() {
        if ws.get_conversation(id).is_ok() {
            return Some(InboxKind::Conversation);
        }
    }
    if let Ok(id) = key.parse::<bisa_core::WorkstreamId>() {
        if ws.get_workstream(id).is_ok() {
            return Some(InboxKind::Workstream);
        }
    }
    if let Ok(id) = key.parse::<bisa_core::ProjectId>() {
        if ws.get_project(id).is_ok() {
            return Some(InboxKind::Project);
        }
    }
    if let Ok(id) = key.parse::<bisa_core::WorkflowId>() {
        if ws.get_workflow(id).is_ok() {
            return Some(InboxKind::Workflow);
        }
    }
    None
}

/// One row's inbox state, for the SSE `inbox` channel.
///
/// This is what lets a client change a row *in place* — bold to dim, gate to
/// decided, a notice counted — instead of refetching the list and replacing
/// the array under a selection the reader is in the middle of using. It is
/// deliberately the same arithmetic the row above does, over one key.
///
/// `with_notices` says whether the cause could have moved the row's notices
/// (an engine fact, a read marker): then the feed is read for this one row
/// and the frame carries `notice_count` and `unread_notices`; a message
/// moves no notice, so its frame leaves them out and the client keeps what
/// it has.
pub(crate) fn delta(state: &Shared, key: &str, with_notices: bool) -> Option<serde_json::Value> {
    match try_delta(state, key, with_notices) {
        Ok(frame) => frame,
        Err(e) => {
            // A frame that is not sent leaves a row stale with nothing on
            // screen to say why; the log says which row, and the next read of
            // the list corrects it.
            tracing::warn!(target: "bisa_node", key, "no inbox frame for a row that moved: {}", e.text);
            None
        }
    }
}

/// [`delta`], with the read that failed kept: `Ok(None)` is a key that names
/// no row.
fn try_delta(
    state: &Shared,
    key: &str,
    with_notices: bool,
) -> Result<Option<serde_json::Value>, ApiError> {
    let ws = state.engine.workspace();
    let kind = kind_of_key(state, key);
    // A terminal harness's row is its wait: owed while it waits, gone when
    // it does not — the frame says which, and the client reads the list
    // again for the row itself.
    if kind == Some(InboxKind::Session) {
        let Ok(session) = key.parse() else {
            return Ok(None);
        };
        // Its own wait or a sub-agent's: the row is owed while either is.
        let waiting = state
            .engine
            .inner()
            .presence
            .get(session)
            .is_some_and(|p| p.wait().is_some());
        return Ok(Some(json!({
            "key": key,
            "kind": kind,
            "unread_count": 0,
            "needs_action_count": if waiting { 1 } else { 0 },
            "latest_at": 0,
            "read": true,
            "handled": false,
            "waiting": waiting,
        })));
    }
    let unread_count = ws
        .unread_counts()?
        .into_iter()
        .find(|(s, _)| s == key)
        .map(|(_, n)| n)
        .unwrap_or(0);

    // The same answer the goal page gives: live gates, else the
    // reconstruction — a goal's, or a workflow's runs of the workspace'.
    let workflow = match kind {
        Some(InboxKind::Workflow) => key.parse::<bisa_core::WorkflowId>().ok(),
        _ => None,
    };
    let actions: Vec<NeedsAction> = match (
        GoalId::from_str(key)
            .ok()
            .and_then(|id| ws.get_goal(id).ok()),
        workflow,
    ) {
        (Some(goal), _) => {
            let run = crate::runs::current_run(ws, &goal).into_run();
            needs_actions_for(state, &goal, run.as_ref())
        }
        (None, Some(workflow)) => ws
            .live_workspace_runs(Some(workflow))?
            .iter()
            .flat_map(|run| needs_actions_for_run(state, run))
            .collect(),
        (None, None) => state
            .engine
            .inbox()
            .into_iter()
            .filter(|g| g.home.id() == key)
            .map(NeedsAction::from_gate)
            .collect(),
    };
    let notices: Vec<NoticeDto> = match (with_notices, kind) {
        (true, Some(kind)) => Facts::gather_notices(ws)?
            .remove(&InboxTarget {
                kind,
                id: key.to_string(),
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    };

    let activity = ws
        .scope_activity()?
        .into_iter()
        .find(|(s, _)| s == key)
        .map(|(_, at)| at);
    let latest_at = latest_of(activity, &actions, &notices, 0);
    let marker = ws
        .read_markers()?
        .into_iter()
        .find(|m| m.scope_id == key)
        .map(|m| (m.last_read_at, m.forced_unread));
    let (last_read_at, forced) = marker.unwrap_or((0, false));
    let handled = actions.is_empty() && {
        let decisions = ws.decisions()?;
        match workflow {
            // A workflow's row is handled once a decision stands on one of
            // its runs of the workspace.
            Some(workflow) => {
                let runs: HashSet<String> = ws
                    .list_workflow_runs(workflow)?
                    .iter()
                    .map(|r| r.id.to_string())
                    .collect();
                decisions
                    .iter()
                    .any(|d| d.run_id.as_ref().is_some_and(|r| runs.contains(r)))
            }
            None => decisions.iter().any(|d| d.goal_id.as_deref() == Some(key)),
        }
    };

    let mut frame = json!({
        "key": key,
        "kind": kind,
        "unread_count": unread_count,
        "needs_action_count": actions.len(),
        "latest_at": latest_at,
        "read": !forced && last_read_at >= latest_at,
        "handled": handled,
    });
    if with_notices {
        frame["notice_count"] = json!(notices.len());
        frame["unread_notices"] = json!(unread_notices_after(marker, &notices));
    }
    Ok(Some(frame))
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test. Kept beside `routes()` so a route added here is added here.
pub const ROUTES: &[RouteDoc] = &[RouteDoc {
    method: "GET",
    path: "/inbox",
    summary: "What concerns you, one row per thing — a goal, a channel, a direct channel, a conversation, a workstream, a project, a workflow: its asks (`needs_action` — gates and questions, a held `wait` step as a `release:` ask), its notices (`notices` — what happened to it, from the activity index: a run done or failed, a step blocked, a budget spent, a design stalled, a listener that could not be armed or could not start its run (`listener_failed`, on its host's row — the workflow's, or the goal's), a committer wanted, a script that failed, a pull request opened or merged, a command refused, a folder a step made, a workflow the Workflow Agent designed or proposed, a workflow put away), `unread_notices`, unread messages, `read` (your watermark, covering notices), `handled`, `mentioned`. `?filter=needs_you|unread|all`, `?source=goals|projects|workflows|messages|people` — a word that is neither is `400` naming the ones that are, never every row (every row says its `source`; `messages` covers channels, direct channels and a conversation about the node or the workspace — one about a goal, a workflow, a project or a workstream sits under that thing's source and says what it is about in `origin`; `workflows` a workflow's row — the Workflow Agent's design or proposal, an archiving, a listener of it that failed, and its runs of the workspace: their asks, decided through the run's home, and what happened to them; a person's own save earns none; `people` the persons hosted here — a claim waiting to be admitted (`join`), a person who joined or left, a message the classifier held). A row is earned by an ask, a decision, a mention, a direct channel, an unread message, a join request or a notice, and kept — and a harness a person opened in the IDE's terminal has a `session` row (`?source=projects`, `waiting`: the harness, where it stands, what it waits on in words) while it waits on them at its own prompt, gone when the prompt is answered or the tab closes; its door is that terminal.",
}];

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_core::AskKind;
    use bisa_store::{MemoryKeyStore, NewGoal};

    fn workspace(dir: &tempfile::TempDir) -> Workspace {
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap()
    }

    /// The refusal of a result that had to be one — `ApiError` is not `Debug`.
    fn refusal<T>(result: Result<T, ApiError>) -> ApiError {
        match result {
            Err(e) => e,
            Ok(_) => panic!("taken, where a refusal was due"),
        }
    }

    #[test]
    fn a_filter_is_one_of_three_words_and_anything_else_is_refused_by_name() {
        assert_eq!(Bucket::parse(None).ok(), Some(Bucket::All));
        for (word, bucket) in
            Bucket::NAMES
                .iter()
                .zip([Bucket::All, Bucket::NeedsYou, Bucket::Unread])
        {
            assert_eq!(Bucket::parse(Some(word)).ok(), Some(bucket));
        }
        // The name the bucket had before it was `needs_you`: it answered
        // every row, and a test passed on it.
        for wrong in ["needs_action", "", "ALL", "unread "] {
            let refused = refusal(Bucket::parse(Some(wrong)));
            assert_eq!(refused.status, axum::http::StatusCode::BAD_REQUEST);
            assert!(
                refused.text.to_string().contains("needs_you"),
                "the refusal names what may be asked: {}",
                refused.text
            );
        }
    }

    #[test]
    fn a_source_is_every_source_when_absent_and_refused_when_unknown() {
        assert_eq!(parse_source(None).ok(), Some(None));
        assert_eq!(parse_source(Some("")).ok(), Some(None));
        assert_eq!(parse_source(Some("goals")).ok(), Some(Some("goals")));
        let refused = refusal(parse_source(Some("goal")));
        assert_eq!(refused.status, axum::http::StatusCode::BAD_REQUEST);
        assert!(
            refused.text.to_string().contains("workflows"),
            "{}",
            refused.text
        );
        assert_eq!(SOURCES.len(), 5);
        assert_eq!(
            SOURCES.to_vec(),
            InboxSource::ALL
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>(),
            "one order, the tabs'"
        );
        // Every kind a row can have is asked for by a source the route takes.
        for kind in InboxKind::ALL {
            assert!(
                SOURCES.contains(&InboxSource::of(*kind, None).as_str()),
                "{kind:?}"
            );
        }
        // A conversation sits under what it is about; one about nothing in
        // particular is a message like any other.
        use bisa_core::ConversationOrigin as O;
        let goal = GoalId::from_str("01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
        let at = |origin: O| InboxSource::of(InboxKind::Conversation, Some(&origin));
        assert_eq!(at(O::Goal { id: goal }), InboxSource::Goals);
        assert_eq!(at(O::Workspace), InboxSource::Messages);
        assert_eq!(at(O::Node), InboxSource::Messages);
        assert_eq!(
            InboxSource::of(InboxKind::Conversation, None),
            InboxSource::Messages,
            "no origin known: a message"
        );
        for source in SOURCES {
            assert!(
                matches!(parse_source(Some(source)), Ok(Some(_))),
                "{source} is taken"
            );
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
        let q = pending_question(&journal, &format!("amend:{run_id}@")).expect("pending");
        assert_eq!(q.subject, subject);
        assert_eq!(q.text, "Amend the running workflow?");
        assert!(
            pending_question(&journal, "amend:01OTHERRUN@").is_none(),
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
            pending_question(&journal, &format!("amend:{run_id}@")).is_none(),
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
        let actions = durable_actions(&ws, &goal, Some(&run));
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
            durable_actions(&ws, &goal, None).is_empty(),
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
        let actions = durable_actions(&ws, &goal, None);
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
        assert!(durable_actions(&ws, &goal, None).is_empty(), "answered");
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
        let actions = durable_run_actions(&ws, &run);
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
        let actions = durable_run_actions(&ws, &run);
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
        let actions = durable_run_actions(&ws, &run);
        assert_eq!(actions.len(), 1, "answered: only the step is owed");
    }
}
