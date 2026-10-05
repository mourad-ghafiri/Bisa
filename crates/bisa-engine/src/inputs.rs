//! One answerer for everything a harness stops on.
//!
//! An adapter raises [`InputRequest`]s — a permission, a question, a sign-in
//! — and never decides them. Every session driver (the executor, the chat
//! pump, the guided wake, a note's ask, `ask_agent_once`) hands the request
//! here, and this module decides once, the same way for all of them:
//!
//! 1. A permission goes to the **Tool & Commands Guard** first
//!    ([`crate::security::decide_tool`]): its placeholders are restored, the
//!    rules are tried in order, the classifier is asked where a rule says so.
//!    A rule's `deny` is the answer; a rule's `allow` is the answer, with the
//!    restored input; `ask` becomes an Escalation gate a person answers in the
//!    Inbox, whose question shows the redacted command.
//! 2. A permission no rule had an opinion on falls to the tier ceiling, inside
//!    the same funnel: within it, allowed at once (restored input included);
//!    above it, in a guided or manual goal an escalation like the guard's
//!    `ask` — answered once per goal and remembered — and in an auto goal
//!    the classifier's reading first (`goals.auto.permissions`), since the
//!    run has nobody watching it: safe runs, harmful asks or is refused as
//!    the classifier settings say, no verdict asks.
//! 3. A question and a sign-in go to the owner of the session's home — its
//!    goal, or its run of the workspace — as before.
//!
//! The session's presence says `waiting` *before* the gate opens so a screen
//! never shows the gate without the row. The answer goes back through
//! [`HarnessSession::answer`] — the one channel — and a refusal is a refusal,
//! not an abort: the agent hears *no* and goes on. A session outside any goal
//! and any run has nobody to escalate to and is refused.

use std::path::PathBuf;

use bisa_core::{AskKind, AskOption, Gate, GoalId, Home, ToolTier, WorkItemId};
use bisa_harness::{HarnessError, HarnessSession, InputAnswer, InputKind, InputRequest};

use crate::events::EnginePayload;
use crate::presence::{waiting_on, WaitingOn};
use crate::registry::LiveRunId;
use crate::security::{self, AboveCeiling, Judge, Outcome, Reach, CEILING_RULE};
use crate::{warn_on_err, Inner};

/// `goals.auto.permissions`: what an auto goal does above a step's ceiling.
pub const AUTO_PERMISSIONS_KEY: &str = "goals.auto.permissions";

/// Who is asking, and how far it may go on its own.
pub struct InputContext {
    pub live_run: LiveRunId,
    /// Where the session's questions are asked: its goal, or its run of the
    /// workspace. `None` for a session about neither.
    pub home: Option<Home>,
    pub work_item: Option<WorkItemId>,
    pub tier_ceiling: ToolTier,
    /// The agent definition behind the session, for the record.
    pub agent: Option<String>,
    /// The session's working directory — what a relative path in a command
    /// is resolved against before the guard's path rules see it.
    pub cwd: Option<PathBuf>,
    /// Whether this session's `classify` rules may ask the classifier. Off
    /// for a one-shot ask — the classifier's own session included — so a
    /// verdict never waits on another verdict.
    pub classifier: bool,
    /// The person on another node whose message woke this session, with
    /// their role (14-collaboration): what the guard reads to put every
    /// tool beyond reading to the owner. `None` for the owner's own.
    pub on_behalf_of: Option<(bisa_core::PrincipalId, bisa_core::MemberRole)>,
    /// What a permission above the ceiling does when no rule decided it —
    /// [`above_ceiling`] from the goal's mode; `Ask` where there is no goal
    /// (a run of the workspace is attended).
    pub above: AboveCeiling,
    /// The conversation about a checkout this session is a turn of, with
    /// its mode (ide/20): where an ask is answered in place rather than
    /// refused for want of a goal. `None` for every other session.
    pub conversation: Option<ConversationReach>,
}

/// A turn of a conversation about a checkout: which, and how far its agent
/// goes on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConversationReach {
    pub id: bisa_core::ConversationId,
    pub mode: bisa_core::ConversationMode,
}

/// What an agent hears when a plan is asked to change a file.
pub const PLAN_REFUSAL: &str = "this conversation is in plan mode: read, ask, and reply with the plan — nothing is changed until the person builds it";

/// What a session on this goal does above its step's ceiling: the
/// classifier reads it when the goal runs unattended and
/// `goals.auto.permissions` says `classify`; a person answers otherwise —
/// a guided or manual goal, no goal at all, or the setting on `ask`.
pub fn above_ceiling(inner: &Inner, goal: Option<GoalId>) -> AboveCeiling {
    let unattended = goal
        .and_then(|g| inner.ws.get_goal(g).ok())
        .is_some_and(|g| g.mode.unattended());
    if !unattended {
        return AboveCeiling::Ask;
    }
    let classify = inner
        .ws
        .setting(AUTO_PERMISSIONS_KEY, None)
        .ok()
        .and_then(|r| r.value.as_str().map(|s| s == "classify"))
        .unwrap_or(true);
    if classify {
        AboveCeiling::Classify
    } else {
        AboveCeiling::Ask
    }
}

/// Decide and answer one request. Never fails the caller: a session that
/// cannot take an answer is aborted and the reason logged.
pub async fn answer_request(
    inner: &Inner,
    ctx: InputContext,
    session: &dyn HarnessSession,
    request: &InputRequest,
) {
    let answer = decide(inner, &ctx, request).await;
    deliver(inner, &ctx, session, request, answer).await;
}

/// The answer alone — for a driver that must not hold its session lock while
/// a person or the classifier is being asked.
pub async fn decide(inner: &Inner, ctx: &InputContext, request: &InputRequest) -> InputAnswer {
    inner.pause.wait_running().await;
    match &request.kind {
        InputKind::Permission {
            tool_name,
            tier,
            input,
            ..
        } => decide_permission(inner, ctx, request, tool_name, *tier, input).await,
        InputKind::Question { text, options } => {
            let Some(home) = ctx.home else {
                return no_goal();
            };
            let subject = format!("question:{}", request.id);
            let expects = AskKind::Answer {
                options: options.iter().map(|o| AskOption::new(o, o)).collect(),
                multi: false,
            };
            let resolution =
                escalate(inner, ctx, home, request, subject, text.clone(), expects).await;
            match resolution
                .answer
                .and_then(|a| a.text.or_else(|| a.selected.into_iter().next()))
            {
                Some(text) => InputAnswer::Text { text },
                None => InputAnswer::Deny {
                    reason: "the person declined to answer".into(),
                },
            }
        }
        InputKind::Auth { provider, url } => {
            let Some(home) = ctx.home else {
                return no_goal();
            };
            let question = match url {
                Some(url) => format!("The agent needs you to sign in to {provider}: {url} — done?"),
                None => format!("The agent needs you to sign in to {provider}. Done?"),
            };
            let resolution = escalate(
                inner,
                ctx,
                home,
                request,
                format!("auth:{provider}"),
                question,
                AskKind::Decision,
            )
            .await;
            if resolution.approve {
                InputAnswer::ALLOW
            } else {
                refused_by_person()
            }
        }
    }
}

/// Hand the answer to the session and move its presence on.
pub async fn deliver(
    inner: &Inner,
    ctx: &InputContext,
    session: &dyn HarnessSession,
    request: &InputRequest,
    answer: InputAnswer,
) {
    match session.answer(&request.id, answer).await {
        Ok(()) => inner.presence.resumed(inner, ctx.live_run),
        Err(HarnessError::NotSupported(_)) => {
            tracing::warn!(
                live_run = %ctx.live_run,
                "the session raised an input request it cannot take an answer to; aborting"
            );
            warn_on_err(
                session.abort().await,
                "aborting a session that cannot be answered",
            );
        }
        Err(e) => {
            tracing::warn!(live_run = %ctx.live_run, "answering an input request failed: {e}")
        }
    }
}

fn no_goal() -> InputAnswer {
    InputAnswer::Deny {
        reason: "this session runs outside any goal and any run, so there is nobody to escalate to"
            .into(),
    }
}

fn refused_by_person() -> InputAnswer {
    InputAnswer::Deny {
        reason: "refused by the person who owns the work".into(),
    }
}

async fn decide_permission(
    inner: &Inner,
    ctx: &InputContext,
    request: &InputRequest,
    tool: &str,
    tier: ToolTier,
    input: &serde_json::Value,
) -> InputAnswer {
    let judge = Judge {
        home: ctx.home,
        session: Some(ctx.live_run),
        cwd: ctx.cwd.as_deref(),
        classifier: ctx.classifier,
        on_behalf_of: ctx.on_behalf_of.clone(),
    };
    // The tier ceiling is the owner's; a session woken by an outsider reads
    // at the ceiling and asks for anything beyond reading.
    let ceiling = match (
        &ctx.on_behalf_of,
        inner.security.policy().collaboration.agent_tools,
    ) {
        (Some(_), bisa_core::OutsiderTools::Ask) => std::cmp::min(ctx.tier_ceiling, ToolTier::Read),
        _ => ctx.tier_ceiling,
    };
    let reach = Reach {
        tier,
        ceiling,
        above: ctx.above,
    };
    match security::decide_tool(inner, tool, input, judge.clone(), Some(reach)).await {
        Outcome::Allow { input, .. } => InputAnswer::Allow { input: Some(input) },
        Outcome::Deny { reason, .. } => InputAnswer::Deny { reason },
        Outcome::Ask {
            question,
            input,
            rule,
            ..
        } => {
            let of_ceiling = rule.as_deref() == Some(CEILING_RULE);
            let home = match (ctx.home, ctx.conversation) {
                (Some(home), _) => home,
                (None, Some(reach)) => {
                    return ask_in_conversation(
                        inner,
                        ctx,
                        reach,
                        &judge,
                        &request.id,
                        tool,
                        tier,
                        input,
                        question,
                        of_ceiling,
                    )
                    .await
                }
                (None, None) => {
                    tracing::info!(live_run = %ctx.live_run, tool, "a tool call asked for a person, and the session is about no goal, no run and no conversation: refused");
                    return no_goal();
                }
            };
            // The ceiling's ask and a rule's wear different subjects: the
            // Inbox and the restart sweep know both.
            let subject = if rule.as_deref() == Some(CEILING_RULE) {
                format!("permission:{tool}")
            } else {
                format!("guard:{tool}")
            };
            let resolution = escalate(
                inner,
                ctx,
                home,
                request,
                subject,
                question,
                AskKind::Decision,
            )
            .await;
            security::record_person(inner, &judge, tool, &input, resolution.approve);
            if resolution.approve {
                InputAnswer::Allow { input: Some(input) }
            } else {
                refused_by_person()
            }
        }
        // A reach was given, so the funnel settled the ceiling itself; this
        // arm is the type's, not a path.
        Outcome::Fallthrough { input } => InputAnswer::Allow { input: Some(input) },
    }
}

/// An ask of a turn about a checkout, answered in the conversation itself.
/// A plan refuses an edit outright — the mode's word, not a question — and
/// every other ask waits for the person where they are reading.
#[allow(clippy::too_many_arguments)]
async fn ask_in_conversation(
    inner: &Inner,
    ctx: &InputContext,
    reach: ConversationReach,
    judge: &Judge<'_>,
    request_id: &str,
    tool: &str,
    tier: ToolTier,
    input: serde_json::Value,
    question: String,
    of_ceiling: bool,
) -> InputAnswer {
    if of_ceiling && tier == ToolTier::Write && !reach.mode.writes() {
        return InputAnswer::Deny {
            reason: PLAN_REFUSAL.into(),
        };
    }
    inner.presence.waiting(
        inner,
        ctx.live_run,
        request_id,
        WaitingOn::Permission {
            tool: tool.to_string(),
            gate_id: None,
        },
    );
    let said = crate::changes::asks::ask(
        inner,
        reach.id,
        ctx.agent.as_deref().unwrap_or_default(),
        crate::changes::asks::AskSubject::Tool {
            tool: tool.to_string(),
            tier,
        },
        question,
        of_ceiling,
    )
    .await;
    match said {
        crate::changes::asks::Said::Allow => {
            security::record_person(inner, judge, tool, &input, true);
            InputAnswer::Allow { input: Some(input) }
        }
        crate::changes::asks::Said::Deny(reason) => {
            security::record_person(inner, judge, tool, &input, false);
            InputAnswer::Deny { reason }
        }
    }
}

/// Open an Escalation gate in the session's home — its goal, or its run of
/// the workspace — say the session waits on it, and wait for the person.
async fn escalate(
    inner: &Inner,
    ctx: &InputContext,
    home: Home,
    request: &InputRequest,
    subject: String,
    question: String,
    expects: AskKind,
) -> crate::gates::GateResolution {
    let (gate_id, rx) = inner.gates.open(
        home,
        ctx.work_item,
        Gate::Escalation,
        subject.clone(),
        question.clone(),
        expects.clone(),
    );
    crate::ops::journal_question(inner, home, ctx.work_item, &subject, &question, expects);
    // Presence first, then the gate: a screen that hears the gate finds the
    // row already waiting on it.
    let on = match waiting_on(request) {
        WaitingOn::Permission { tool, .. } => WaitingOn::Permission {
            tool,
            gate_id: Some(gate_id.clone()),
        },
        WaitingOn::Question { text, .. } => WaitingOn::Question {
            text,
            gate_id: Some(gate_id.clone()),
        },
        other => other,
    };
    // Under the request's own id: the wait the fold already holds for it is
    // told its gate, never doubled.
    inner.presence.waiting(inner, ctx.live_run, &request.id, on);
    inner.emit(inner.home_scope(&home).event(
        ctx.work_item,
        EnginePayload::GateOpened {
            gate_id,
            gate: Gate::Escalation,
            question,
        },
    ));
    inner.gates.wait(rx).await
}
