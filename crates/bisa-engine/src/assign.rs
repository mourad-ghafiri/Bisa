//! Assignment resolves in **one** place.
//!
//! Goals, projects and workflow steps all carry
//! `Vec<Assignee>`. Two questions get asked of that data — *who may take this
//! work?* and *who may sign for this goal?* — and every surface that ever
//! needs an answer calls [`workers`] or [`approvers`]. No caller
//! re-implements the union; that is the design rule this module exists to
//! enforce.
//!
//! ## The union, in precedence order
//!
//! ```text
//! spec.assignees              the request; nearest, and it wins alone
//! ∪ project.assignees         the project the item works in
//! ∪ goal.assignees          the goal that owns the item, when one does
//! ∪ ancestors' assignees      walking `Goal.parent`, visited-set guarded
//! ```
//!
//! A run of the workspace's item has no goal: its union is the request and
//! its project's, and an empty one falls to the routing race like any other.
//!
//! Nearest wins: a project assignment is *closer* to the work than the
//! goal's, and a goal's is closer than its parent's. Order is preserved
//! rather than sorted, because the order **is** the precedence and a caller
//! that takes `first()` should get the nearest answer. Duplicates are dropped
//! at their nearest position.
//!
//! Teams expand to their members at this boundary — `Assignee::Team` never
//! escapes into a caller. Nesting is not modelled (see
//! `bisa_core::assignee`), so the expansion is one level and needs no
//! cycle detection; the *goal* walk does need one, because `Goal.parent`
//! is user-editable and a goal can end up its own ancestor.
//!
//! ## The core agent
//!
//! **The core agent is an implicit member of every team and every channel, and
//! it is never stored in a members list or a roster — a list you can edit is
//! a list you can empty.**
//!
//! That membership is an addressing fact, not a routing one, and this module
//! is where the two part company. [`principals`] includes it, through
//! `Workspace::team_addressed`: naming who is in the room is the question
//! it answers. [`workers`] excludes it from the union, because the pool it
//! returns is who *executes* a work item, and the core agent guides and
//! delegates — an agent that could win the rotation for an implementation
//! item would be doing the one thing its role forbids. The exception is
//! [`WorkItemSpec::assignees`]: naming the core agent there is a deliberate
//! request rather than the implicit membership the exclusion exists to
//! cancel, so an explicit request reaches it.
//!
//! ## What `spec.agent` is not
//!
//! [`WorkItemSpec::agent`] is the **answer**, written back by the executor
//! once [`pick`] has chosen. It is never an input to this module: reading it
//! here would make the second resolution of an item agree with the first by
//! construction and hide any disagreement, and accepting it from a caller
//! would be a route around the union entirely. The request is `assignees`.

use crate::Inner;
use bisa_core::workitem::WorkItemSpec;
use bisa_core::{AgentId, Assignee, GoalId, PrincipalId, ProjectId, WorkItemId};

/// The union, from its single implementation.
///
/// The walk itself lives in `bisa_store::Workspace::assignees_for` —
/// governance needs the same answer and the store is the lowest layer both
/// callers reach, so there is one implementation rather than two that drift.
/// This module is where the *filters* live.
fn union(inner: &Inner, spec: &WorkItemSpec) -> Vec<Assignee> {
    // Nearest wins, and nothing is nearer to the work than the item itself —
    // so a non-empty request is the whole union. Widening it with the goal's
    // assignees would make naming somebody a suggestion, which is the one
    // thing this field must not be.
    if !spec.assignees.is_empty() {
        return inner.ws.expand_assignees(spec.assignees.clone());
    }
    let projects: Vec<ProjectId> = spec.project.into_iter().collect();
    inner.ws.assignees_for(spec.home.goal(), &projects)
}

/// Agents that may take this work item, best-first.
///
/// The union above, filtered down to agents that are **enabled** and whose
/// harness **probes available** here: an agent that cannot launch on this host
/// is no use, and a disabled one was turned off on purpose. Both filters ask
/// "can this actually run", which is a fact about the host rather than a
/// preference, so they apply to an explicit request too — an item whose named
/// agent cannot launch falls back like any other rather than waiting for a
/// harness that is not installed.
///
/// Humans are dropped. They decide gates and answer questions; they do not
/// take work items. That is what makes a human-only assignment harmless — it
/// yields an empty pool, and the caller falls back to
/// `spec.harness_candidates` exactly as an unassigned item does, instead of
/// blocking forever on a person who was never going to run a session.
///
/// The core agent is dropped from the *resolved* union for the same reason and
/// a stronger one: it is in every team implicitly, so leaving it in would make
/// it a candidate for essentially every item in the workspace, and it delegates
/// rather than implements. `spec.assignees` is the exception, because there the
/// membership is not implicit — somebody typed the id.
///
/// An empty result therefore means "nothing here to route to", never "this
/// item is not runnable".
pub async fn workers(inner: &Inner, spec: &WorkItemSpec) -> Vec<String> {
    let requested = !spec.assignees.is_empty();
    let mut pool = Vec::new();
    for assignee in union(inner, spec) {
        let Assignee::Agent(id) = assignee else {
            continue;
        };
        if AgentId::is_core_str(&id) && !requested {
            continue;
        }
        let Ok(agent_id) = AgentId::new(&id) else {
            continue;
        };
        let Ok(def) = inner.ws.get_agent(&agent_id) else {
            continue;
        };
        if !def.enabled {
            continue;
        }
        let available = match inner.catalog.get(&def.harness) {
            Some(adapter) => adapter.probe().await.available,
            None => false,
        };
        if available {
            pool.push(id);
        }
    }
    pool
}

/// Humans who may decide this goal's gates.
///
/// The same union, filtered to people. `governance.rs` holds the *policy*
/// question (an explicitly configured policy is authoritative and is never
/// widened by an assignment); this answers the *membership* one, and is what
/// surfaces show when they list who a gate is waiting on.
pub fn approvers(inner: &Inner, goal: GoalId, project: Option<ProjectId>) -> Vec<PrincipalId> {
    let projects: Vec<ProjectId> = project.into_iter().collect();
    inner
        .ws
        .assignees_for(Some(goal), &projects)
        .into_iter()
        .filter_map(|a| match a {
            Assignee::Human(pk) => Some(pk),
            _ => None,
        })
        .collect()
}

/// The principals one assignee names, for addressing a message to it.
///
/// An agent is its own attested pubkey, a human is theirs, and a team is its
/// participants' — the same one-level expansion as everywhere else in this
/// module, which is why a workflow's `notify` does not get to do its own.
///
/// Unlike [`workers`] this keeps humans **and** agents: the caller is naming a
/// conversation's participants, not choosing who runs a session, and a message
/// addressed to a person is the point of asking a person. For the same reason
/// a team resolves through `team_addressed` rather than `team_agents`, so
/// the core agent is reachable in every room it implicitly belongs to — the
/// other half of the split this module's docs describe. A team stood down
/// names nobody, as a disabled agent does.
pub fn principals(inner: &Inner, assignee: &Assignee) -> Vec<PrincipalId> {
    let mut out = Vec::new();
    let push_agent = |id: &str, out: &mut Vec<PrincipalId>| match AgentId::new(id)
        .map_err(bisa_store::StoreError::from)
        .and_then(|id| inner.ws.get_agent(&id))
    {
        Ok(def) if def.enabled => out.push(def.pubkey),
        Ok(_) => tracing::debug!("agent {id} is disabled; not addressing it"),
        Err(e) => tracing::warn!("agent {id} unresolvable: {e}"),
    };
    match assignee {
        Assignee::Agent(id) => push_agent(id, &mut out),
        Assignee::Human(pk) => out.push(pk.clone()),
        Assignee::Team(id) => {
            // Whom the team reaches, which is nobody while it is stood down.
            let addressed = bisa_core::TeamId::new(id)
                .ok()
                .and_then(|id| inner.ws.team_addressed(&id).ok())
                .unwrap_or_default();
            for participant in addressed {
                match participant {
                    Assignee::Agent(agent) => push_agent(&agent, &mut out),
                    Assignee::Human(pk) => out.push(pk),
                    // Nesting is rejected at validation; a team inside a team
                    // is data that cannot exist.
                    Assignee::Team(_) => {}
                }
            }
        }
    }
    out.dedup();
    out
}

/// The one question a pick asks the Decision-Making Agent.
const PICK_QUESTION: &str = "agent";

/// Choose who of `pool` takes `spec`'s work item: the one the Decision-Making
/// Agent picks by what each agent is for, when it is on here and sure; the lot
/// ([`pick`]) otherwise. On here means the workspace's switch, the run's
/// workflow's, or that of any agent in the pool.
pub async fn choose(
    inner: &crate::Inner,
    pool: &[String],
    spec: &bisa_core::workitem::WorkItemSpec,
) -> Option<String> {
    let by_lot = || pick(pool, spec.id);
    if pool.len() < 2 {
        return by_lot();
    }
    let members: Vec<bisa_core::Agent> = pool
        .iter()
        .filter_map(|id| bisa_core::AgentId::new(id.as_str()).ok())
        .filter_map(|id| inner.ws.get_agent(&id).ok())
        .collect();
    let workflow_on = spec
        .run
        .and_then(|run| inner.ws.get_run(run).ok())
        .is_some_and(|run| run.workflow.decision_making);
    let standing = crate::decider::Standing {
        home: Some(spec.home),
        run: spec.run,
        step: spec.step.clone(),
        project: spec.project,
        switched_on: workflow_on || members.iter().any(|a| a.decision_making),
        ..Default::default()
    };
    if !crate::decider::is_on(inner, bisa_core::DecisionPoint::AssignPick, &standing) {
        return by_lot();
    }
    let options = members.iter().map(|a| {
        let what = a
            .description
            .clone()
            .filter(|d| !d.trim().is_empty())
            .unwrap_or_else(|| a.name.clone());
        (a.id.to_string(), what)
    });
    let request = bisa_core::DecisionRequest::one(
        serde_json::json!({ "work": spec.instructions }),
        PICK_QUESTION,
        bisa_core::DecisionQuestion::choice(
            "Which agent is the right one to do this work?",
            options,
        ),
    );
    crate::decider::judge(
        inner,
        bisa_core::DecisionPoint::AssignPick,
        &standing,
        request,
    )
    .await
    .answered()
    .and_then(|r| r.answer(PICK_QUESTION)?.chosen().map(str::to_string))
    // A pick outside the pool cannot pass the contract; this is the belt.
    .filter(|agent| pool.contains(agent))
    .or_else(by_lot)
}

/// Pick one agent out of a pool for `item`.
///
/// The rotation is the work item's own ULID randomness, which makes the
/// choice deterministic (the same item always lands on the same agent, so a
/// re-run is not a reshuffle) while consecutive items of one plan spread
/// across the pool instead of pinning a single agent.
pub fn pick(pool: &[String], item: WorkItemId) -> Option<String> {
    if pool.is_empty() {
        return None;
    }
    let slot = (item.0.random() as usize) % pool.len();
    pool.get(slot).cloned()
}
