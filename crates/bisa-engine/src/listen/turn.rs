//! Turning a host on and off: the one door a library workflow or a goal
//! begins or stops hearing its start events through.
//!
//! **Turning on** refuses what could not listen as it stands — a workflow
//! with problems, archived, only a goal's design, one that reads its goal
//! when the workspace would run it, one with no start on an event — and
//! what it would listen with: every input its events need and no default
//! fills ([`bisa_core::Workflow::listening_needs`]) must be given, each
//! given one must be the workflow's and fit its kind, every start must
//! resolve against them, and a check start's command must pass the guard's
//! rules. Then the record is written (a workflow's beside its definition, a
//! goal's on the goal), every public hook start is minted its secret — shown
//! here, once, to whoever turned it on: the listening door's caller, the
//! person who started the goal, the one who adopted its design
//! ([`crate::ops::DecideOutcome`]) — and the bus hears it. A person turns a
//! host on; auto adoption arms only what nobody needs to see
//! (`StartOn::arms_unattended`).
//!
//! **Turning off** removes the record, and what its events had queued
//! settles *not listening*. A secret is kept across Off and On.

use super::now_secs;
use crate::events::{EngineEvent, EnginePayload};
use crate::{EngineError, Inner};
use bisa_core::goal::Budget;
use bisa_core::{
    GoalId, ListenerHost, ListenerKey, Listening, RunScope, StartOn, StepId, StepKind, Workflow,
};
use bisa_store::SignalState;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

/// A public hook's secret, as it is shown: once.
#[derive(
    Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct HookSecret {
    pub step: StepId,
    /// The route a caller outside posts to.
    pub path: String,
    /// Hex.
    pub secret: String,
}

/// What turning a host on did.
#[derive(Clone, Debug, PartialEq)]
pub struct TurnedOn {
    pub listening: Listening,
    /// The secrets minted by this turn — none when every public hook already
    /// had its own.
    pub secrets: Vec<HookSecret>,
}

/// The public route a hook start of `host` answers on.
pub fn public_path(host: &ListenerHost, step: &StepId) -> String {
    format!("/hooks/{host}/{step}")
}

/// The workflow a host listens with, for turning it on.
fn host_workflow(inner: &Inner, host: &ListenerHost) -> Result<Workflow, EngineError> {
    match host {
        ListenerHost::Workspace { workflow } => Ok(inner.ws.get_workflow(*workflow)?),
        ListenerHost::Goal { goal } => {
            let g = inner.ws.get_goal(*goal)?;
            if g.is_closed() {
                return Err(EngineError::Conflict(bisa_core::text!(
                    "error-engine-invalid-goal-closed",
                    goal_id = goal.to_string()
                )));
            }
            let wf = g.workflow.ok_or_else(|| {
                EngineError::Invalid(bisa_core::text!(
                    "error-engine-invalid-goal-has-no-workflow-pick-one-let",
                    goal_id = goal.to_string()
                ))
            })?;
            Ok(inner.ws.get_workflow(wf)?)
        }
    }
}

/// Every check turning `host` on with `inputs` makes, before anything is
/// written.
pub(crate) fn check(
    inner: &Inner,
    host: &ListenerHost,
    wf: &Workflow,
    inputs: &BTreeMap<String, Value>,
) -> Result<(), EngineError> {
    if wf.is_archived() {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-workflow-archived-unarchive-before-running",
            a0 = wf.name.clone()
        )));
    }
    let problems = inner.ws.validate_workflow(wf)?;
    if !problems.is_empty() {
        return Err(bisa_store::StoreError::WorkflowInvalid(problems).into());
    }
    if wf.event_starts().is_empty() {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-listening-no-event-start",
            workflow = wf.name.clone()
        )));
    }
    if let ListenerHost::Workspace { .. } = host {
        let scope = RunScope::Workspace {
            budget: Budget::default(),
        };
        let problems = wf.scope_problems(&scope);
        if !problems.is_empty() {
            return Err(bisa_store::StoreError::WorkflowInvalid(problems).into());
        }
    }
    for (name, value) in inputs {
        let def = wf
            .inputs
            .iter()
            .find(|d| d.name.as_str() == name)
            .ok_or_else(|| {
                EngineError::Invalid(bisa_core::text!(
                    "error-engine-invalid-listening-unknown-input",
                    name = name.clone(),
                    workflow = wf.name.clone()
                ))
            })?;
        if !def.kind.accepts(value) {
            return Err(EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-listening-input-kind",
                name = name.clone(),
                want = def.kind.as_str().to_string(),
                got = value.to_string()
            )));
        }
    }
    let missing: Vec<String> = wf
        .listening_needs()
        .into_iter()
        .filter(|n| !inputs.contains_key(n.as_str()))
        .map(|n| n.to_string())
        .collect();
    if !missing.is_empty() {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-listening-needs-inputs",
            names = missing.join(", ")
        )));
    }
    // An input nobody gave is read at its default, as a run reads it.
    let inputs = &wf.with_defaults(inputs);
    for (step, on) in wf.event_starts() {
        let resolved = on.resolve(inputs).map_err(|e| {
            EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-listening-start-unresolved",
                step = step.id.to_string(),
                e = e.to_string()
            ))
        })?;
        if let StartOn::Check { command, .. } = &resolved {
            if let Some(reason) = crate::security::refuse_by_rules(inner, command) {
                return Err(EngineError::Security(format!(
                    "the check start `{}` runs a command the guard refuses: {reason}",
                    step.id
                )));
            }
        }
    }
    Ok(())
}

/// Turn `host` on, listening with `inputs` under `budget`.
pub fn turn_on(
    inner: &Arc<Inner>,
    host: ListenerHost,
    inputs: BTreeMap<String, Value>,
    budget: Option<Budget>,
) -> Result<TurnedOn, EngineError> {
    let wf = host_workflow(inner, &host)?;
    check(inner, &host, &wf, &inputs)?;
    let listening = Listening {
        inputs,
        // A goal's runs spend against the goal's budget.
        budget: match host {
            ListenerHost::Workspace { .. } => budget,
            ListenerHost::Goal { .. } => None,
        },
        since: now_secs(),
        paused: None,
    };
    inner.ws.set_listening(&host, Some(listening.clone()))?;
    // A host turned on starts afresh: what was said of its listeners before
    // is said again if it is still true, and kept by none until then.
    inner.listen.forget_host(&host);
    for (step, _) in wf.event_starts() {
        let key = ListenerKey {
            host,
            step: step.id.clone(),
        };
        super::keep_trouble(inner, &key);
    }
    let secrets = mint_secrets(inner, &host, &wf);
    super::invalidate(inner);
    announce(inner, host, true);
    Ok(TurnedOn { listening, secrets })
}

/// Mint a secret for every public hook start that has none; answer the ones
/// minted, to be shown once.
fn mint_secrets(inner: &Inner, host: &ListenerHost, wf: &Workflow) -> Vec<HookSecret> {
    let mut out = Vec::new();
    for step in &wf.steps {
        let StepKind::Start {
            on: StartOn::Hook { public: true },
            ..
        } = &step.kind
        else {
            continue;
        };
        let key = ListenerKey {
            host: *host,
            step: step.id.clone(),
        };
        match inner.ws.ensure_hook_secret(&key) {
            Ok(Some(secret)) => out.push(HookSecret {
                step: step.id.clone(),
                path: public_path(host, &step.id),
                secret: hex::encode(secret),
            }),
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(target: "bisa_engine::listen", listener = %key, "a hook's secret could not be minted: {e}")
            }
        }
    }
    out
}

/// Turn `host` off: its record goes, and what its events queued settles.
/// Off already is nothing to do.
pub fn turn_off(inner: &Arc<Inner>, host: ListenerHost) -> Result<(), EngineError> {
    if inner.ws.listening(&host)?.is_none() {
        return Ok(());
    }
    inner.ws.set_listening(&host, None)?;
    inner.listen.forget_host(&host);
    crate::warn_on_err(
        inner
            .ws
            .settle_pending_signals(&host, SignalState::Skipped, "not listening"),
        "settling the signals of a host turned off",
    );
    super::invalidate(inner);
    announce(inner, host, false);
    Ok(())
}

/// A paused goal hears again: its listening as it was — or with new inputs
/// — unpaused. A goal that never listened listens with nothing but what is
/// given.
pub fn listen_again(
    inner: &Arc<Inner>,
    goal: GoalId,
    inputs: Option<BTreeMap<String, Value>>,
) -> Result<TurnedOn, EngineError> {
    let host = ListenerHost::Goal { goal };
    let inputs = match inputs {
        Some(given) => given,
        None => kept_inputs(inner, &host)?,
    };
    turn_on(inner, host, inputs, None)
}

/// What a host listened with before, as far as its workflow still reads it.
/// An input the design no longer has, or whose kind no longer takes the
/// value, is left behind — a repair may have redrawn the inputs — and what
/// is then missing is asked for by name when the host is turned on.
fn kept_inputs(inner: &Inner, host: &ListenerHost) -> Result<BTreeMap<String, Value>, EngineError> {
    let Some(listening) = inner.ws.listening(host)? else {
        return Ok(BTreeMap::new());
    };
    let wf = host_workflow(inner, host)?;
    Ok(listening
        .inputs
        .into_iter()
        .filter(|(name, value)| {
            wf.inputs
                .iter()
                .any(|def| def.name.as_str() == name && def.kind.accepts(value))
        })
        .collect())
}

/// Mint a public hook start's secret anew: the old one stops verifying at
/// once, the new one is shown this once.
pub fn rotate_secret(inner: &Arc<Inner>, key: &ListenerKey) -> Result<HookSecret, EngineError> {
    let wf = host_workflow(inner, &key.host)?;
    let public = wf.step(&key.step).is_some_and(|s| {
        matches!(
            s.kind,
            StepKind::Start {
                on: StartOn::Hook { public: true },
                ..
            }
        )
    });
    if !public {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-listening-not-public-hook",
            step = key.step.to_string()
        )));
    }
    let secret = inner.ws.rotate_hook_secret(key)?;
    Ok(HookSecret {
        step: key.step.clone(),
        path: public_path(&key.host, &key.step),
        secret: hex::encode(secret),
    })
}

/// `ListeningChanged`, on the host's own row.
pub(crate) fn announce(inner: &Inner, host: ListenerHost, on: bool) {
    let payload = EnginePayload::ListeningChanged { host, on };
    inner.emit(match host {
        ListenerHost::Goal { goal } => EngineEvent::scoped(goal, None, payload),
        ListenerHost::Workspace { workflow } => crate::events::EventScope {
            goal: None,
            workflow: Some(workflow),
            run: None,
        }
        .event(None, payload),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_public_hook_answers_on_its_hosts_route() {
        let host = ListenerHost::Workspace {
            workflow: bisa_core::WorkflowId::from_ulid(ulid::Ulid::from_parts(1, 1)),
        };
        let path = public_path(&host, &StepId::new("ticket").unwrap());
        assert_eq!(path, format!("/hooks/{host}/ticket"));
        assert!(path.starts_with("/hooks/workspace:"));
    }
}
