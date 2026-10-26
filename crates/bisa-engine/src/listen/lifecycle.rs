//! The lifecycle readings: what the bus says of a goal, a workflow
//! definition, a run and a harness session, read as the `goal`, `workflow`,
//! `run` and `agent` events — and a checkout's move as a `project` change.
//!
//! Each reading is a pure fold of one [`EngineEvent`] into what the filters
//! hear ([`Heard`]), its dedupe key and the run it is causally behind;
//! what a payload still needs from the store — a goal's title, a workflow's
//! name, a checkout's project, a reply's — is read here, once, and only
//! when a listener or a wait wants the source at all. A goal's or a
//! workflow's move is the workspace's news about it, heard by every host
//! and narrowed by the filter; a run's and a session's belong to their goal
//! when they have one, as a run's end does.

use super::ear::{run_payload, RunMove};
use crate::events::{EngineEvent, EnginePayload};
use crate::registry::LiveRunId;
use crate::Inner;
use bisa_core::{
    Assignee, GoalId, GoalOrigin, Heard, RunId, SessionOrigin, SignalScope, SignalSource,
    WorkflowId, WorkstreamState,
};
use serde_json::{json, Value};
use std::sync::Arc;

/// One occurrence read off a bus event: what the filters hear, the key that
/// makes a redelivery the same occurrence, and the run whose causal chain it
/// carries — the envelope's, a session's, a goal's origin's.
pub(crate) struct Reading {
    pub heard: Heard,
    pub dedupe: Option<String>,
    pub run: Option<RunId>,
}

fn reading(
    source: SignalSource,
    scope: SignalScope,
    payload: Value,
    dedupe: Option<String>,
    run: Option<RunId>,
) -> Reading {
    Reading {
        heard: Heard {
            source,
            name: None,
            scope,
            payload,
            chain: bisa_core::Chain::default(),
        },
        dedupe,
        run,
    }
}

/// Whether anyone — an armed listener while events are on, a run's wait —
/// could hear `source`: the cheap question before anything is read.
fn wanted(inner: &Arc<Inner>, source: SignalSource) -> bool {
    (inner.listen.settings().enabled && super::armed(inner).wants(source))
        || crate::waits::wants(inner, source)
}

/// Where a bus event belongs: its goal's, or nobody's in particular.
fn scope_of(event: &EngineEvent) -> SignalScope {
    match event.goal {
        Some(goal) => SignalScope::Goal { goal },
        None => SignalScope::Workspace,
    }
}

/// Every reading one bus event yields, for the sources somebody hears.
pub(crate) fn readings(inner: &Arc<Inner>, event: &EngineEvent) -> Vec<Reading> {
    let mut out = Vec::new();
    // The session memory moves on every frame, heard or not: a transition
    // nobody listens for is still the one the next listener counts from.
    let session = session_move(inner, event);
    if wanted(inner, SignalSource::Goal) {
        out.extend(goal_reading(inner, event));
    }
    if wanted(inner, SignalSource::Workflow) {
        out.extend(workflow_reading(inner, event));
    }
    if wanted(inner, SignalSource::Run) {
        out.extend(run_reading(inner, event));
    }
    if wanted(inner, SignalSource::Agent) {
        out.extend(agent_reading(inner, event, session));
    }
    if wanted(inner, SignalSource::Project) {
        out.extend(workstream_reading(inner, event));
    }
    out
}

// ---------------------------------------------------------------------------
// A goal's move
// ---------------------------------------------------------------------------

/// What a bus event says of a goal: the goal, the change word, the fields
/// the change carries, and its dedupe key. Pure.
pub(crate) fn goal_move(
    event: &EngineEvent,
) -> Option<(GoalId, &'static str, Value, Option<String>)> {
    let on = event.goal;
    Some(match &event.payload {
        EnginePayload::GoalCreated { goal, origin } => (
            *goal,
            "created",
            json!({ "origin": origin.as_str(), "parent": origin.parent().map(|p| p.to_string()) }),
            Some(format!("goal:{goal}:created")),
        ),
        EnginePayload::GoalUpdated { goal } => (*goal, "updated", json!({}), None),
        EnginePayload::GoalClosed { reason } => {
            let by = match reason {
                bisa_core::ClosureReason::Superseded { by } => Some(by.to_string()),
                bisa_core::ClosureReason::Abandoned { .. } => None,
            };
            let goal = on?;
            (
                goal,
                "closed",
                json!({ "reason": reason.as_str(), "by": by }),
                Some(format!("goal:{goal}:closed")),
            )
        }
        EnginePayload::GoalArchived { goal, archived } => (
            *goal,
            if *archived { "archived" } else { "restored" },
            json!({}),
            None,
        ),
        EnginePayload::GoalDeleted { goal } => (
            *goal,
            "deleted",
            json!({}),
            Some(format!("goal:{goal}:deleted")),
        ),
        EnginePayload::AttachmentChanged { project, attached } => (
            on?,
            if *attached { "attached" } else { "detached" },
            json!({ "project": project.to_string() }),
            None,
        ),
        EnginePayload::WorkflowProposed {
            workflow,
            revision,
            gate_id,
        } => {
            let goal = on?;
            (
                goal,
                "proposed",
                json!({ "workflow": workflow.to_string(), "revision": revision, "gate_id": gate_id }),
                Some(format!("goal:{goal}:proposed:{workflow}@{revision}")),
            )
        }
        EnginePayload::GateOpened {
            gate_id,
            gate,
            question,
        } => {
            let goal = on?;
            (
                goal,
                "asked",
                json!({ "gate": format!("{gate:?}").to_lowercase(), "gate_id": gate_id, "question": question }),
                Some(format!("goal:{goal}:asked:{gate_id}")),
            )
        }
        EnginePayload::GateDecided {
            gate_id,
            gate,
            approve,
        } => {
            let goal = on?;
            (
                goal,
                "decided",
                json!({ "gate": format!("{gate:?}").to_lowercase(), "gate_id": gate_id, "approve": approve }),
                Some(format!("goal:{goal}:decided:{gate_id}")),
            )
        }
        _ => return None,
    })
}

/// The goal payload, every key present — the table the guide and the
/// designer's form copy: `{ goal, change, title, status, origin, parent,
/// reason, by, project, workflow, revision, gate, gate_id, question,
/// approve }`.
fn goal_payload(
    goal: GoalId,
    change: &str,
    title: Option<String>,
    status: Option<&str>,
    extra: &Value,
) -> Value {
    let field = |k: &str| extra.get(k).cloned().unwrap_or(Value::Null);
    json!({
        "goal": goal.to_string(),
        "change": change,
        "title": title,
        "status": status,
        "origin": field("origin"),
        "parent": field("parent"),
        "reason": field("reason"),
        "by": field("by"),
        "project": field("project"),
        "workflow": field("workflow"),
        "revision": field("revision"),
        "gate": field("gate"),
        "gate_id": field("gate_id"),
        "question": field("question"),
        "approve": field("approve"),
    })
}

fn goal_reading(inner: &Arc<Inner>, event: &EngineEvent) -> Option<Reading> {
    let (goal, change, extra, dedupe) = goal_move(event)?;
    // The record as it stands — gone for a deleted goal, which reads with
    // no title and no status; its origin says which run the move is
    // causally behind, so a run that spawns goals never feeds its own
    // `goal` start.
    let record = inner.ws.get_goal(goal).ok();
    let status = record.as_ref().map(|g| {
        let run = inner.ws.get_current_run(goal).ok().flatten();
        g.status(run.as_ref()).as_str()
    });
    let title = record.as_ref().and_then(|g| g.title.clone());
    let run = record.as_ref().and_then(|g| match &g.origin {
        GoalOrigin::Run { run, .. } => Some(*run),
        GoalOrigin::Spawned { parent, .. } => {
            inner.ws.live_run(*parent).ok().flatten().map(|r| r.id)
        }
        GoalOrigin::Captured => None,
    });
    Some(reading(
        SignalSource::Goal,
        SignalScope::Workspace,
        goal_payload(goal, change, title, status, &extra),
        dedupe,
        run,
    ))
}

// ---------------------------------------------------------------------------
// A workflow definition's move
// ---------------------------------------------------------------------------

/// What a bus event says of a workflow definition: the workflow, the change
/// word, its revision and whose hand, and the dedupe key.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct WorkflowMove {
    pub workflow: WorkflowId,
    pub change: &'static str,
    pub revision: Option<u64>,
    pub designed: Option<bool>,
    pub dedupe: Option<String>,
}

/// Pure: the move one bus event says of a workflow definition, when it says one.
pub(crate) fn workflow_move(event: &EngineEvent) -> Option<WorkflowMove> {
    Some(match &event.payload {
        EnginePayload::WorkflowChanged {
            workflow,
            revision,
            designed,
        } => WorkflowMove {
            workflow: *workflow,
            change: "saved",
            revision: Some(*revision),
            designed: Some(*designed),
            dedupe: Some(format!("workflow:{workflow}:saved@{revision}")),
        },
        EnginePayload::WorkflowProposed {
            workflow, revision, ..
        } => WorkflowMove {
            workflow: *workflow,
            change: "proposed",
            revision: Some(*revision),
            designed: None,
            dedupe: Some(format!("workflow:{workflow}:proposed@{revision}")),
        },
        EnginePayload::WorkflowArchived { workflow, archived } => WorkflowMove {
            workflow: *workflow,
            change: if *archived { "archived" } else { "restored" },
            revision: None,
            designed: None,
            dedupe: None,
        },
        EnginePayload::WorkflowDeleted { workflow } => WorkflowMove {
            workflow: *workflow,
            change: "deleted",
            revision: None,
            designed: None,
            dedupe: Some(format!("workflow:{workflow}:deleted")),
        },
        _ => return None,
    })
}

fn workflow_reading(inner: &Arc<Inner>, event: &EngineEvent) -> Option<Reading> {
    let m = workflow_move(event)?;
    let name = inner.ws.get_workflow(m.workflow).ok().map(|w| w.name);
    Some(reading(
        SignalSource::Workflow,
        SignalScope::Workspace,
        json!({
            "workflow": m.workflow.to_string(),
            "change": m.change,
            "name": name,
            "revision": m.revision,
            "designed": m.designed,
            "goal": event.goal.map(|g| g.to_string()),
        }),
        m.dedupe,
        event.run,
    ))
}

// ---------------------------------------------------------------------------
// A run's move — but its end, which the ear reads as it always did
// ---------------------------------------------------------------------------

fn run_reading(inner: &Arc<Inner>, event: &EngineEvent) -> Option<Reading> {
    let (payload, dedupe) = match &event.payload {
        EnginePayload::RunQueued {
            run,
            workflow,
            position,
        } => (
            run_payload(&RunMove {
                run: *run,
                workflow: *workflow,
                change: "queued",
                outcome: None,
                position: Some(*position),
                start: None,
                step: None,
                state: None,
                kind: None,
                goal: event.goal,
            }),
            format!("run:{run}:queued"),
        ),
        EnginePayload::RunStarted { run, workflow } => {
            let start = inner
                .ws
                .get_run(*run)
                .ok()
                .and_then(|r| r.entry_step().map(|s| s.id.to_string()));
            (
                run_payload(&RunMove {
                    run: *run,
                    workflow: *workflow,
                    change: "started",
                    outcome: None,
                    position: None,
                    start: start.as_deref(),
                    step: None,
                    state: None,
                    kind: None,
                    goal: event.goal,
                }),
                format!("run:{run}:started"),
            )
        }
        EnginePayload::StepChanged {
            run,
            workflow,
            step,
            state,
            kind,
        } => {
            // The visit's entry tells one visit of a looping step from the
            // next; a snapshot that cannot be read leaves the key off.
            let entered = inner
                .ws
                .get_run(*run)
                .ok()
                .and_then(|r| r.steps.get(step).and_then(|record| record.started_at));
            let payload = run_payload(&RunMove {
                run: *run,
                workflow: *workflow,
                change: "step",
                outcome: None,
                position: None,
                start: None,
                step: Some(step.as_str()),
                state: Some(state.as_str()),
                kind: Some(kind.as_str()),
                goal: event.goal,
            });
            match entered {
                Some(at) => (payload, format!("run:{run}:step:{step}:{state}@{at}")),
                None => {
                    return Some(reading(
                        SignalSource::Run,
                        scope_of(event),
                        payload,
                        None,
                        event.run,
                    ))
                }
            }
        }
        _ => return None,
    };
    Some(reading(
        SignalSource::Run,
        scope_of(event),
        payload,
        Some(dedupe),
        event.run,
    ))
}

// ---------------------------------------------------------------------------
// A harness session's move
// ---------------------------------------------------------------------------

/// The change an agent event counts a session's new state as, when it
/// counts one: into `starting` is `started`; into `waiting`, `done`,
/// `failed` or `aborted` their own word; the churn between — thinking, a
/// tool running, idle, parked — none.
pub(crate) fn session_change(state: &str) -> Option<&'static str> {
    Some(match state {
        "starting" => "started",
        "waiting" => "waiting",
        "done" => "done",
        "failed" => "failed",
        "aborted" => "aborted",
        _ => return None,
    })
}

/// Whether a frame in `now` after `before` is a transition an agent event
/// hears: the first frame counts only when it is `starting` (a session the
/// ear first hears mid-turn — the roster rebuilt after a restart — makes
/// nothing), and every later frame only when the word moved.
pub(crate) fn session_transition(before: Option<&str>, now: &str) -> Option<&'static str> {
    match before {
        None if now == "starting" => Some("started"),
        None => None,
        Some(was) if was == now => None,
        Some(_) => session_change(now),
    }
}

/// Keep the session memory on every frame, and answer the transition this
/// frame is, when it is one. A session gone from the roster is forgotten,
/// and a `gone` whose last word was no end says nothing: the end frame a
/// lagging bus dropped is not invented.
fn session_move(inner: &Arc<Inner>, event: &EngineEvent) -> Option<(LiveRunId, &'static str)> {
    match &event.payload {
        EnginePayload::SessionState { live_run, presence } => {
            let now = presence.state.as_str();
            let before = inner
                .listen
                .session_states
                .insert(*live_run, now.to_string());
            session_transition(before.as_deref(), now).map(|change| (*live_run, change))
        }
        EnginePayload::SessionGone { live_run } => {
            inner.listen.session_states.remove(live_run);
            None
        }
        _ => None,
    }
}

/// The teams `agent` is a member of, by id — what a team in an agent
/// filter counts.
fn teams_of(inner: &Inner, agent: Option<&str>) -> Vec<String> {
    let Some(agent) = agent else {
        return Vec::new();
    };
    let who = Assignee::Agent(agent.to_string());
    inner
        .ws
        .list_teams()
        .unwrap_or_default()
        .into_iter()
        .filter(|t| t.members.contains(&who))
        .map(|t| t.id.to_string())
        .collect()
}

fn agent_reading(
    inner: &Arc<Inner>,
    event: &EngineEvent,
    session: Option<(LiveRunId, &'static str)>,
) -> Option<Reading> {
    match &event.payload {
        EnginePayload::SessionState { presence, .. } => {
            let (live_run, change) = session?;
            let agent = presence.agent.as_ref().map(|a| a.to_string());
            let scope = match presence.goal {
                Some(goal) => SignalScope::Goal { goal },
                None => SignalScope::Workspace,
            };
            let turn_scope = match &presence.origin {
                SessionOrigin::Turn { scope, .. } => Some(scope.clone()),
                _ => None,
            };
            let dedupe = match change {
                "waiting" => format!("session:{live_run}:waiting:{}", presence.since),
                other => format!("session:{live_run}:{other}"),
            };
            Some(reading(
                SignalSource::Agent,
                scope,
                json!({
                    "agent": agent,
                    "teams": teams_of(inner, agent.as_deref()),
                    "harness": presence.harness,
                    "kind": format!("{:?}", presence.kind).to_lowercase(),
                    "change": change,
                    "state": presence.state.as_str(),
                    "live_run": live_run.to_string(),
                    "project": presence.project.map(|p| p.to_string()),
                    "workstream": presence.workstream.map(|w| w.to_string()),
                    "goal": presence.goal.map(|g| g.to_string()),
                    "run": presence.run.map(|r| r.to_string()),
                    "work_item": presence.work_item.map(|w| w.to_string()),
                    "scope": turn_scope,
                    "message": null,
                }),
                Some(dedupe),
                presence.run,
            ))
        }
        EnginePayload::AgentReplied {
            scope,
            agent,
            posted: true,
            message: Some(message),
        } => {
            let facts = crate::conversation::scope_facts(inner, scope);
            Some(reading(
                SignalSource::Agent,
                match facts.goal {
                    Some(goal) => SignalScope::Goal { goal },
                    None => SignalScope::Workspace,
                },
                json!({
                    "agent": agent,
                    "teams": teams_of(inner, Some(agent)),
                    "harness": null,
                    "kind": "conversation",
                    "change": "replied",
                    "state": "replied",
                    "live_run": null,
                    "project": facts.project().map(|p| p.to_string()),
                    "workstream": facts.workstream().map(|w| w.to_string()),
                    "goal": facts.goal.map(|g| g.to_string()),
                    "run": null,
                    "work_item": null,
                    "scope": scope,
                    "message": message,
                }),
                Some(format!("reply:{message}")),
                None,
            ))
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// A checkout's move, as a project change
// ---------------------------------------------------------------------------

/// A pull request a workstream state names, when it names one.
fn pull_request_of(state: &WorkstreamState) -> Value {
    match state {
        WorkstreamState::PrOpen { number, url } | WorkstreamState::Merged { number, url } => {
            json!({ "number": number, "url": url })
        }
        _ => Value::Null,
    }
}

fn workstream_reading(inner: &Arc<Inner>, event: &EngineEvent) -> Option<Reading> {
    let (workstream, state, branch, pull_request) = match &event.payload {
        EnginePayload::WorkstreamOpened {
            workstream, branch, ..
        } => (*workstream, "open".to_string(), branch.clone(), Value::Null),
        EnginePayload::WorkstreamChanged { workstream, state } => (
            *workstream,
            state.as_str().to_string(),
            None,
            pull_request_of(state),
        ),
        _ => return None,
    };
    let record = inner.ws.get_workstream(workstream).ok()?;
    let project = inner.ws.get_project(record.project).ok()?;
    let branch = branch.or_else(|| record.branch().map(str::to_string));
    Some(reading(
        SignalSource::Project,
        SignalScope::Workspace,
        json!({
            "project": project.id.to_string(),
            "slug": project.slug.as_str(),
            "change": "workstream",
            "workstream": workstream.to_string(),
            "state": state,
            "branch": branch,
            "pull_request": pull_request,
        }),
        Some(format!("workstream:{workstream}:{state}")),
        event.run,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_core::{ClosureReason, Gate, GoalId, WorkflowId};

    fn goal() -> GoalId {
        GoalId::from_ulid(ulid::Ulid::from_parts(1, 1))
    }

    fn wf() -> WorkflowId {
        WorkflowId::from_ulid(ulid::Ulid::from_parts(2, 2))
    }

    #[test]
    fn a_goals_move_is_read_with_its_change_its_fields_and_its_key() {
        let g = goal();
        let other = GoalId::from_ulid(ulid::Ulid::from_parts(1, 2));
        let scoped = |payload: EnginePayload| EngineEvent::scoped(g, None, payload);
        let (who, change, fields, key) = goal_move(&scoped(EnginePayload::GoalCreated {
            goal: g,
            origin: GoalOrigin::Spawned {
                parent: other,
                step: None,
            },
        }))
        .unwrap();
        assert_eq!((who, change), (g, "created"));
        assert_eq!(fields["origin"], "spawned");
        assert_eq!(fields["parent"], other.to_string());
        assert_eq!(key.as_deref(), Some(format!("goal:{g}:created").as_str()));
        let (_, change, fields, key) = goal_move(&scoped(EnginePayload::GoalClosed {
            reason: ClosureReason::Superseded { by: other },
        }))
        .unwrap();
        assert_eq!(change, "closed");
        assert_eq!(fields["reason"], "superseded");
        assert_eq!(fields["by"], other.to_string());
        assert!(key.is_some());
        assert!(
            goal_move(&EngineEvent::global(EnginePayload::GoalClosed {
                reason: ClosureReason::Abandoned { rationale: None }
            }))
            .is_none(),
            "a closing with no goal on its envelope names nobody"
        );
        let (_, change, _, key) =
            goal_move(&scoped(EnginePayload::GoalUpdated { goal: g })).unwrap();
        assert_eq!((change, key), ("updated", None));
        let (_, change, _, _) = goal_move(&scoped(EnginePayload::GoalArchived {
            goal: g,
            archived: false,
        }))
        .unwrap();
        assert_eq!(change, "restored");
        let (_, change, _, key) =
            goal_move(&scoped(EnginePayload::GoalDeleted { goal: g })).unwrap();
        assert_eq!(change, "deleted");
        assert_eq!(key.as_deref(), Some(format!("goal:{g}:deleted").as_str()));
        let (_, change, fields, _) = goal_move(&scoped(EnginePayload::AttachmentChanged {
            project: bisa_core::ProjectId::from_ulid(ulid::Ulid::from_parts(3, 3)),
            attached: false,
        }))
        .unwrap();
        assert_eq!(change, "detached");
        assert!(fields["project"].is_string());
        let (_, change, fields, key) = goal_move(&scoped(EnginePayload::GateOpened {
            gate_id: "g1".into(),
            gate: Gate::Approval,
            question: "Adopt?".into(),
        }))
        .unwrap();
        assert_eq!(change, "asked");
        assert_eq!(fields["gate"], "approval");
        assert_eq!(key.as_deref(), Some(format!("goal:{g}:asked:g1").as_str()));
        let (_, change, fields, _) = goal_move(&scoped(EnginePayload::GateDecided {
            gate_id: "g1".into(),
            gate: Gate::Approval,
            approve: true,
        }))
        .unwrap();
        assert_eq!(
            (change, fields["approve"].as_bool()),
            ("decided", Some(true))
        );
        let (_, change, fields, key) = goal_move(&scoped(EnginePayload::WorkflowProposed {
            workflow: wf(),
            revision: 2,
            gate_id: None,
        }))
        .unwrap();
        assert_eq!((change, fields["revision"].as_u64()), ("proposed", Some(2)));
        assert_eq!(
            key.as_deref(),
            Some(format!("goal:{g}:proposed:{}@2", wf()).as_str())
        );
        assert!(goal_move(&scoped(EnginePayload::Paused)).is_none());
        let payload = goal_payload(g, "closed", Some("Ship it".into()), Some("closed"), &fields);
        assert_eq!(payload["title"], "Ship it");
        assert_eq!(payload["revision"], 2);
        assert!(
            payload["reason"].is_null(),
            "a key the change lacks is there, null"
        );
    }

    #[test]
    fn a_workflows_move_is_read_with_its_revision_and_its_hand() {
        let global = |payload: EnginePayload| EngineEvent::global(payload);
        let saved = workflow_move(&global(EnginePayload::WorkflowChanged {
            workflow: wf(),
            revision: 4,
            designed: true,
        }))
        .unwrap();
        assert_eq!(
            saved,
            WorkflowMove {
                workflow: wf(),
                change: "saved",
                revision: Some(4),
                designed: Some(true),
                dedupe: Some(format!("workflow:{}:saved@4", wf())),
            }
        );
        let archived = workflow_move(&global(EnginePayload::WorkflowArchived {
            workflow: wf(),
            archived: true,
        }))
        .unwrap();
        assert_eq!((archived.change, archived.dedupe), ("archived", None));
        let deleted =
            workflow_move(&global(EnginePayload::WorkflowDeleted { workflow: wf() })).unwrap();
        assert_eq!(deleted.change, "deleted");
        assert!(deleted.dedupe.is_some());
        assert!(workflow_move(&global(EnginePayload::Resumed)).is_none());
    }

    #[test]
    fn a_sessions_transition_counts_a_start_a_wait_and_an_end_and_never_the_churn() {
        assert_eq!(session_transition(None, "starting"), Some("started"));
        assert_eq!(
            session_transition(None, "thinking"),
            None,
            "first heard mid-turn"
        );
        assert_eq!(
            session_transition(None, "done"),
            None,
            "an end nobody saw begin"
        );
        assert_eq!(session_transition(Some("starting"), "thinking"), None);
        assert_eq!(session_transition(Some("thinking"), "running"), None);
        assert_eq!(
            session_transition(Some("running"), "waiting"),
            Some("waiting")
        );
        assert_eq!(
            session_transition(Some("waiting"), "waiting"),
            None,
            "the same frame again"
        );
        assert_eq!(session_transition(Some("waiting"), "thinking"), None);
        assert_eq!(session_transition(Some("thinking"), "done"), Some("done"));
        assert_eq!(
            session_transition(Some("thinking"), "failed"),
            Some("failed")
        );
        assert_eq!(session_transition(Some("idle"), "aborted"), Some("aborted"));
        assert_eq!(session_transition(Some("done"), "parked"), None);
        assert_eq!(
            session_transition(Some("thinking"), "starting"),
            Some("started"),
            "a harness that starts again after a model fell over — one key, one occurrence"
        );
    }

    #[test]
    fn a_pull_request_is_read_off_the_states_that_carry_one() {
        assert_eq!(
            pull_request_of(&WorkstreamState::PrOpen {
                number: 7,
                url: "u".into()
            })["number"],
            7
        );
        assert!(pull_request_of(&WorkstreamState::Committed).is_null());
    }
}
