//! Ask one question — of an agent, or of a harness with one of its models —
//! and get the answer back as a string.
//!
//! Everything else in the engine that runs an agent runs it *for its own
//! sake*: a work item settles into a result, a guided wake settles into a
//! journal entry, a chat turn settles into a posted message. Each of those
//! owns its session for the session's whole life and reports through the bus,
//! because nobody is waiting on the other end of an HTTP request.
//!
//! This is the shape that was missing: a caller holding a request open, that
//! wants **one string** and then wants the session gone. It is
//! [`crate::conversation`]'s reply pump with the pump taken out — the same
//! `TextDelta` accumulation, settled on the first turn end and handed back
//! rather than posted.
//!
//! Three properties make it safe to reach for from a route:
//!
//! 1. **It cannot act.** No MCP servers are injected and the tier ceiling is
//!    [`ToolTier::Read`], so the session that suggests a commit message has
//!    no way to make one. A suggestion that could commit would not be a
//!    suggestion.
//! 2. **It always ends.** One deadline covers launch, prompt and turn; on
//!    expiry the session is aborted and disposed, and the caller gets an
//!    error. The stream is awaited inline rather than in a spawned task
//!    precisely so that disposal cannot outlive a caller that gave up.
//! 3. **It fails out loud.** No harness, no model, an empty turn or a timeout
//!    are each an `Err`. There is no path here that returns a plausible
//!    string the agent did not say.
//!
//! It is deliberately not on the bus. A work item's session events belong to
//! a work item and a chat's to a scope; this one has neither, and a `Session`
//! envelope with nothing to attach it to is noise on every surface that
//! renders one.

use crate::executor::{self, Attempts, LaunchPlan};
use crate::{warn_on_err, EngineError, Inner};
use bisa_core::{AgentId, Effort, EffortChoice, ModelPlan, ToolTier};
use bisa_harness::{LifecycleEvent, Outcome, ProgressEvent, SessionEvent, SessionSpec};
use std::collections::HashSet;
use std::time::{Duration, Instant};

/// Whom one question is put to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Whom {
    /// An agent, on its own harness and through its own model plan.
    Agent(String),
    /// A harness on this node with exactly this model — never substituted —
    /// or its own default when none is named. No agent stands in between.
    Harness {
        harness: String,
        model: Option<String>,
        /// How hard the model works: the asker's own setting, fitted at
        /// launch to what the model takes. `None` leaves it to
        /// `agents.effort`.
        effort: Option<Effort>,
    },
}

impl std::fmt::Display for Whom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Whom::Agent(agent) => f.write_str(agent),
            Whom::Harness {
                harness,
                model: Some(model),
                ..
            } => write!(f, "{harness} ({model})"),
            Whom::Harness {
                harness,
                model: None,
                ..
            } => f.write_str(harness),
        }
    }
}

/// One answer, and the model that gave it as the ledger names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asked {
    pub text: String,
    pub model: String,
}

/// Launch `agent`, put one prompt to it, and return the text of its first
/// turn. [`ask_once`] with an agent and no schema.
pub async fn ask_agent_once(
    inner: &Inner,
    agent: &str,
    prompt: &str,
    deadline: Duration,
) -> Result<String, EngineError> {
    ask_once(
        inner,
        &Whom::Agent(agent.to_string()),
        prompt,
        None,
        deadline,
    )
    .await
    .map(|asked| asked.text)
}

/// Launch a session for `whom`, put one prompt to it, and return the text of
/// its first turn. `output_schema` is handed to the harness, for the ones that
/// can hold a model to a shape; the caller still reads the answer strictly.
///
/// `deadline` is the budget for the **whole** call, not per attempt: a model
/// wall that costs one relaunch spends the same clock the turn would have,
/// because from the caller's side there is one question and one wait.
///
/// The session is disposed on every exit — answered, timed out, ended empty
/// or refused — before this returns.
pub async fn ask_once(
    inner: &Inner,
    whom: &Whom,
    prompt: &str,
    output_schema: Option<serde_json::Value>,
    deadline: Duration,
) -> Result<Asked, EngineError> {
    let agent = whom.to_string();
    let agent = agent.as_str();
    let (cwd, candidates, plan, pin, effort_pin, judged_as) = match whom {
        Whom::Agent(name) => {
            let agent_id = AgentId::new(name.as_str())?;
            let Some(info) = crate::ops::agent_info(&inner.ws, &agent_id) else {
                return Err(EngineError::Invalid(bisa_core::text!(
                    "error-engine-invalid-agent-missing-disabled-so-there-nobody-ask",
                    name = format!("{name:?}")
                )));
            };
            // The agent's own folder, like a chat instance: a session with no
            // work of its own must not be handed the workspace root to look
            // around in.
            let cwd = inner.ws.paths().agent(&agent_id).scratch();
            (
                cwd,
                info.harness,
                info.models,
                None,
                None,
                Some(name.clone()),
            )
        }
        Whom::Harness {
            harness,
            model,
            effort,
        } => {
            if harness.trim().is_empty() {
                return Err(EngineError::Invalid(bisa_core::text!(
                    "error-engine-invalid-no-harness-named-so-there-nobody-ask"
                )));
            }
            // A folder of its own, with nothing in it to look around in.
            let cwd = inner.ws.paths().run_dir().join("ask");
            (
                cwd,
                vec![harness.clone()],
                ModelPlan::default(),
                model.clone(),
                // The asker's level stands where a step's pin would: the
                // first link of the chain.
                effort.map(EffortChoice::from),
                None,
            )
        }
    };
    std::fs::create_dir_all(&cwd)?;

    let session_env = crate::network::session_env(inner, Default::default());
    let spec = SessionSpec {
        work_item: None,
        cwd,
        prompt: String::new(),
        // Chosen per attempt by the executor's launch walk, from the agent's
        // own plan against the live health ledger.
        model: None,
        // And the effort with it, fitted to that model.
        effort: None,
        // Nothing. This session answers a question; it does not touch the
        // platform, and the surest way to guarantee that is to give it no
        // door to reach the platform through.
        mcp_servers: vec![],
        env: session_env.0,
        env_remove: session_env.1,
        tier_ceiling: ToolTier::Read,
        output_schema,
        skills: vec![],
    };

    let launch = LaunchPlan {
        candidates: &candidates,
        plan: &plan,
        pin: pin.as_deref(),
        // A one-shot answer is never routed: the Decision-Making Agent is
        // asked through here, and a route that asked it would never end.
        lead: None,
        effort_pin,
        effort_setting: crate::effort::setting(inner, None),
        // Nor asked how hard to work, for the same reason: an `auto` in the
        // chain takes its fallback here.
        judged_effort: None,
        rotation: inner.models.next_rotation(),
        skills_in_prompt: false,
    };
    let mut tried = HashSet::new();
    let mut attempts = Attempts::new(inner.config.max_model_attempts);
    let started = Instant::now();
    let left = || deadline.saturating_sub(started.elapsed());

    loop {
        let launched =
            executor::resolve_and_launch(inner, &launch, &mut tried, &mut attempts, &spec)
                .await
                .map_err(|f| {
                    EngineError::Invalid(bisa_core::text!(
                        "error-engine-invalid-no-session-agent",
                        agent = agent.to_string(),
                        a0 = (f.message).to_string()
                    ))
                })?;
        let (harness, model_key) = (launched.harness.clone(), launched.model_key.clone());
        let session = launched.session;
        let in_flight = launched.in_flight;
        let mut events = session.subscribe();

        // Skills the harness cannot host natively ride the first prompt, the
        // way every other launch site delivers them.
        let text = match &launched.skill_appendix {
            Some(appendix) => format!("{prompt}\n\n{appendix}"),
            None => prompt.to_string(),
        };
        if let Err(e) = session.prompt(text.as_str().into()).await {
            warn_on_err(session.dispose().await, "disposing a failed ask session");
            drop(in_flight);
            return Err(EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-asking-failed",
                agent = agent.to_string(),
                e = e.to_string()
            )));
        }

        let mut buf = String::new();
        let mut wall = None;
        let mut timed_out = false;
        loop {
            let event =
                match tokio::time::timeout(left(), futures::StreamExt::next(&mut events)).await {
                    Err(_) => {
                        timed_out = true;
                        break;
                    }
                    Ok(None) => break,
                    Ok(Some(event)) => event,
                };
            match &event {
                SessionEvent::Progress(ProgressEvent::TurnStarted) => buf.clear(),
                SessionEvent::Progress(ProgressEvent::TextDelta { text }) => buf.push_str(text),
                // A harness that asks before every tool — Claude Code does,
                // for a `Read` too — is answered like every other driver:
                // the guard first, then the `Read` ceiling. There is no goal
                // here, so anything above the ceiling is refused, never
                // escalated, and the session goes on.
                SessionEvent::Lifecycle(LifecycleEvent::InputRequested { request }) => {
                    crate::inputs::answer_request(
                        inner,
                        crate::inputs::InputContext {
                            live_run: crate::registry::LiveRunId::mint(),
                            home: None,
                            work_item: None,
                            tier_ceiling: ToolTier::Read,
                            agent: judged_as.clone(),
                            cwd: Some(spec.cwd.clone()),
                            classifier: false,
                            on_behalf_of: None,
                            above: crate::security::AboveCeiling::Ask,
                            conversation: None,
                        },
                        session.as_ref(),
                        request,
                    )
                    .await;
                }
                // One question, one turn: the first end is the answer.
                SessionEvent::Progress(ProgressEvent::TurnEnded) => break,
                SessionEvent::Lifecycle(LifecycleEvent::Ended { outcome, .. }) => {
                    // A turn that ended without a `TurnEnded` still answered.
                    if let Outcome::ModelUnavailable {
                        reason,
                        retry_after,
                        ..
                    } = outcome
                    {
                        let until =
                            inner
                                .models
                                .note_unavailable(&harness, &model_key, *retry_after);
                        wall = Some(executor::ModelWall {
                            harness: harness.clone(),
                            model: model_key.clone(),
                            reason: reason.clone(),
                            retry_after: *retry_after,
                            cooldown_until: until,
                            after_progress: !buf.is_empty(),
                        });
                    }
                    break;
                }
                _ => {}
            }
        }

        // Abort before dispose so a harness that is still generating stops
        // generating, rather than being detached and left running.
        if timed_out {
            warn_on_err(session.abort().await, "aborting a timed-out ask session");
        }
        warn_on_err(session.dispose().await, "disposing an ask session");
        drop(in_flight);

        if let Some(wall) = wall {
            // The model died, not the question. Come round on the next one,
            // on whatever clock is left.
            tracing::info!(agent = %agent, "{} unavailable ({})", wall.model, wall.reason);
            if !attempts.spent() && !left().is_zero() {
                continue;
            }
            return Err(EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-no-model-left-unavailable",
                agent = agent.to_string(),
                a0 = (wall.model).to_string(),
                a1 = (wall.reason).to_string()
            )));
        }

        if timed_out {
            return Err(EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-did-not-answer-within-s",
                agent = agent.to_string(),
                a0 = (deadline.as_secs()).to_string()
            )));
        }
        let answer = buf.trim().to_string();
        if answer.is_empty() {
            return Err(EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-ended-turn-without-saying-anything",
                agent = agent.to_string()
            )));
        }
        inner.models.note_success(&harness, &model_key);
        return Ok(Asked {
            text: answer,
            model: model_key,
        });
    }
}
