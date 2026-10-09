//! One door for ending a goal's or a run's work — what a stop, a restart,
//! a close, a retirement and the engine's own stop go through, in one order,
//! so nothing of the thing is left running and the verb says what it ended:
//!
//! 1. listening off and the queue withdrawn — a stop; a restart keeps the
//!    queue in its place; a close did both in its record;
//! 2. no re-wake: the Workflow Agent's pending cycle and the thread's queued
//!    message go, and the goal is marked as being ended, so the stop is not
//!    answered by the cycle it stopped;
//! 3. the live run cancelled — the run first, the sessions after: an aborted
//!    worker finds its item cancelled and nothing retries; the cancel disarms
//!    a `spawn` step's wait, so a child's end below cannot advance a run that
//!    is being ended;
//! 4. the goals it spawned, recursively — stopped with a stop or a restart,
//!    closed with a close or a retirement (`EndCause::for_child`);
//! 5. the run's calls out to platforms and its check and judge tasks aborted;
//! 6. every session told: the in-flight marks of the home first — a worker
//!    still launching has a mark and no row yet — then every row of the scope
//!    (`sessions::stop_for`);
//! 7. the wait (`sessions::await_stopped`): until nothing of the scope
//!    stands, or the deadline — then what still has a process of ours is
//!    terminated and said, and what has none is reported;
//! 8. then what the thing held is released: a closed or retired goal's
//!    workstreams and gates, the mark, and what the Workflow Agent and the
//!    conversations remembered of it.
//!
//! A close has a synchronous half (`ops::close_goal`, for a caller that
//! cannot wait): the record and the signal (steps 2, 5 and 6) happen at
//! once, and the rest follows on a task of its own; `ops::close_goal_settled`
//! waits for all of it and answers what was ended.

use crate::sessions::{self, EndCause, Ended, Scope};
use crate::{effects, ops, EngineError, Inner};
use bisa_core::{
    CancelCause, ClosureReason, GoalId, Home, ListenerHost, RunEvent, RunId, WorkflowRun,
    WorkstreamId,
};
use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// What ending a goal's or a run's work did: the run it cancelled, the
/// queued runs it withdrew, and the sessions, processes and spawned goals it
/// ended ([`Ended`]).
#[derive(Clone, Debug, Default)]
pub struct WorkEnded {
    pub run: Option<RunId>,
    pub withdrawn: Vec<RunId>,
    pub ended: Ended,
}

/// How deep a cascade goes: a corrupt chain of origins ends here, never in
/// a stop that never returns.
const MAX_DEPTH: usize = 32;

/// What the synchronous half of an end told, for the wait that follows it.
#[derive(Clone, Debug, Default)]
pub struct Signalled {
    pub run: Option<RunId>,
    pub withdrawn: Vec<RunId>,
    /// The rows told to stop.
    pub told: usize,
    /// The goal's workstreams, and the ones the caller named besides — what
    /// the wait watches beside the goal's own rows.
    pub workstreams: HashSet<WorkstreamId>,
}

/// End a goal's work for `cause`, its spawned goals with it, and wait for
/// every process to be gone (module doc). `extra` names workstreams whose
/// sessions go with the goal's — the ones a retirement touches.
pub async fn end_goal_work(
    inner: &Arc<Inner>,
    goal: GoalId,
    cause: EndCause,
    rationale: Option<String>,
    extra: &HashSet<WorkstreamId>,
) -> Result<WorkEnded, EngineError> {
    let mut visited = HashSet::new();
    end_goal_work_in(inner, goal, cause, rationale, extra, &mut visited, 0).await
}

fn end_goal_work_in<'a>(
    inner: &'a Arc<Inner>,
    goal: GoalId,
    cause: EndCause,
    rationale: Option<String>,
    extra: &'a HashSet<WorkstreamId>,
    visited: &'a mut HashSet<GoalId>,
    depth: usize,
) -> Pin<Box<dyn Future<Output = Result<WorkEnded, EngineError>> + Send + 'a>> {
    Box::pin(async move {
        if !visited.insert(goal) || depth > MAX_DEPTH {
            // LCOV_EXCL_START: a child is its parent's once and the chain is shallow; the set and the depth keep a cycle from recursing
            return Ok(WorkEnded::default());
            // LCOV_EXCL_STOP
        }
        let mut signalled = signal_goal_work(inner, goal, cause, rationale)?;
        signalled.workstreams.extend(extra.iter().copied());
        let mut done = WorkEnded {
            run: signalled.run,
            withdrawn: signalled.withdrawn.clone(),
            ended: Ended::default(),
        };
        // 4. The goals it spawned go with it.
        for child in open_children_of(inner, |g| g.origin.parent() == Some(goal))? {
            let child_done = end_child(
                inner,
                child,
                cause,
                child_rationale(cause, goal),
                visited,
                depth,
            )
            .await?;
            done.ended.children.push(child);
            done.ended.add(child_done.ended);
        }
        // 7. The wait, and 8. the release.
        let settled = sessions::await_stopped(
            inner,
            Scope::Goal(goal),
            &signalled.workstreams,
            signalled.told,
            inner.config.stop_deadline(),
        )
        .await;
        done.ended.add(Ended::of(settled));
        finish_goal_work(inner, goal, cause);
        Ok(done)
    })
}

/// A spawned goal, ended as its parent's cause says: stopped, or closed —
/// the record half of a close first, as `ops::close_goal` writes it.
async fn end_child(
    inner: &Arc<Inner>,
    child: GoalId,
    cause: EndCause,
    rationale: String,
    visited: &mut HashSet<GoalId>,
    depth: usize,
) -> Result<WorkEnded, EngineError> {
    match cause.for_child() {
        EndCause::Close => {
            if !inner.ws.get_goal(child)?.is_closed() {
                ops::close_record(
                    inner,
                    child,
                    ClosureReason::Abandoned {
                        rationale: Some(rationale.clone()),
                    },
                )?;
            }
            let done = end_goal_work_in(
                inner,
                child,
                EndCause::Close,
                Some(rationale),
                &HashSet::new(),
                visited,
                depth + 1,
            )
            .await?;
            inner.emit(crate::events::EngineEvent::scoped(
                child,
                None,
                crate::events::EnginePayload::GoalClosed {
                    reason: ClosureReason::Abandoned { rationale: None },
                },
            ));
            Ok(done)
        }
        _ => {
            end_goal_work_in(
                inner,
                child,
                EndCause::Stop,
                Some(rationale),
                &HashSet::new(),
                visited,
                depth + 1,
            )
            .await
        }
    }
}

/// Why a spawned goal is ended: its parent's end, in words.
fn child_rationale(cause: EndCause, parent: GoalId) -> String {
    match cause.for_child() {
        EndCause::Close => bisa_core::text!(
            "engine-rationale-parent-goal-closed",
            parent = parent.to_string()
        )
        .to_string(),
        _ => bisa_core::text!(
            "engine-rationale-parent-goal-stopped",
            parent = parent.to_string()
        )
        .to_string(),
    }
}

/// The open goals `born_of` picks out of the workspace's goals. One scan,
/// tolerant as the listing is: a goal whose record cannot be read is not in
/// it, never a reason to fail the end.
fn open_children_of(
    inner: &Inner,
    born_of: impl Fn(&bisa_core::Goal) -> bool,
) -> Result<Vec<GoalId>, EngineError> {
    Ok(inner
        .ws
        .list_goals(None)?
        .into_iter()
        .filter(|g| !g.is_closed() && born_of(g))
        .map(|g| g.id)
        .collect())
}

/// The synchronous half of ending a goal's work: steps 1 to 3, 5 and 6 of
/// the module doc. Everything here happens before this returns; the wait
/// and the release follow (`finish_goal_work`).
pub(crate) fn signal_goal_work(
    inner: &Arc<Inner>,
    goal: GoalId,
    cause: EndCause,
    rationale: Option<String>,
) -> Result<Signalled, EngineError> {
    let mut out = Signalled::default();
    // 1. Listening off, the queue withdrawn — a stop's; a restart keeps its
    // queue, and a close did both in its record.
    if cause == EndCause::Stop {
        crate::listen::turn::turn_off(inner, ListenerHost::Goal { goal })?;
        for queued in inner.ws.queued_runs(goal)? {
            if ops::withdraw_if_queued(inner, queued.id)? {
                out.withdrawn.push(queued.id);
            }
        }
    }
    // 2. No re-wake while the work is ended.
    inner.guided.begin_stopping(goal);
    crate::conversation::forget_pending_of_goal(inner, goal);
    // Every live session of the goal enters the ledger now, before the run
    // is cancelled: the cancel's effects end the rows through their drivers,
    // racing the stop's own pass in 6, and a row ended first was never
    // waited for — its harness, if it ignored the abort, nobody's to
    // terminate at the deadline. Entered first, the deadline has it.
    out.workstreams = ops::workstreams_of_goal(inner, goal)?;
    let noted = sessions::note_stopping(inner, Scope::Goal(goal), &out.workstreams, cause);
    // 3. The run first, the sessions after.
    if matches!(cause, EndCause::Stop | EndCause::Restart) {
        if let Some(live) = inner.ws.live_run(goal)? {
            let cancel = match cause {
                EndCause::Stop => CancelCause::Stopped { rationale },
                _ => CancelCause::Restarted,
            };
            if ops::cancel_unless_over(inner, live.id, cancel)? {
                out.run = Some(live.id);
            }
        }
    }
    // 5. Nothing of the run waits on a platform or a shell any more.
    let current = match out.run {
        Some(run) => Some(run),
        None => inner.ws.get_current_run(goal)?.map(|r| r.id),
    };
    if let Some(run) = current {
        crate::connectors::abort_calls(inner, run);
        effects::abort_step_tasks(inner, run);
    }
    // 6. Every session told: the marks first, then the rows.
    let home = Home::Goal { goal };
    for mark in inner.inflight.iter() {
        if mark.home == home {
            mark.stop();
        }
    }
    out.told = noted.max(sessions::stop_for(
        inner,
        Scope::Goal(goal),
        &out.workstreams,
        cause,
    ));
    Ok(out)
}

/// Step 8: what the goal held is released once nothing of it runs. A goal
/// that goes for good takes what was remembered of it; a stopped one may be
/// woken again — by a person, never by the stop.
pub(crate) fn finish_goal_work(inner: &Arc<Inner>, goal: GoalId, cause: EndCause) {
    if matches!(cause, EndCause::Close | EndCause::Retire) {
        effects::release_goal_workstreams(inner, goal);
        for gate in inner.gates.pending_for_goal(goal) {
            inner.gates.withdraw(&gate.id);
        }
        inner.guided.forget_goal(goal);
        crate::conversation::forget_pending_of_goal(inner, goal);
    }
    inner.guided.end_stopping(goal);
}

/// The wait and the release after a synchronous close signalled the work —
/// the children closed with it — for the caller that could not wait.
pub(crate) async fn finish_after_signal(
    inner: &Arc<Inner>,
    goal: GoalId,
    signalled: Signalled,
) -> WorkEnded {
    let mut done = WorkEnded {
        run: signalled.run,
        withdrawn: signalled.withdrawn.clone(),
        ended: Ended::default(),
    };
    let mut visited = HashSet::from([goal]);
    let children = open_children_of(inner, |g| g.origin.parent() == Some(goal)).unwrap_or_default();
    for child in children {
        match end_child(
            inner,
            child,
            EndCause::Close,
            child_rationale(EndCause::Close, goal),
            &mut visited,
            0,
        )
        .await
        {
            Ok(child_done) => {
                done.ended.children.push(child);
                done.ended.add(child_done.ended);
            }
            Err(e) => {
                tracing::warn!(target: "bisa_engine", %goal, %child, "a goal spawned by a closed goal could not be closed with it: {e}");
            }
        }
    }
    let settled = sessions::await_stopped(
        inner,
        Scope::Goal(goal),
        &signalled.workstreams,
        signalled.told,
        inner.config.stop_deadline(),
    )
    .await;
    done.ended.add(Ended::of(settled));
    finish_goal_work(inner, goal, EndCause::Close);
    done
}

/// End a run of the workspace's work: cancelled for `cancel`, the goals born
/// of it ended, its calls and tasks aborted, its sessions told and waited
/// for. Answers the run after and what was ended.
pub async fn end_run_work(
    inner: &Arc<Inner>,
    run_id: RunId,
    cancel: CancelCause,
) -> Result<(WorkflowRun, WorkEnded), EngineError> {
    let cause = EndCause::of_cancel(&cancel);
    // Every live session of the run enters the ledger before the cancel, for
    // the reason `signal_goal_work` gives.
    let workstreams = ops::workstreams_of_run(inner, run_id)?;
    let noted = sessions::note_stopping(inner, Scope::Run(run_id), &workstreams, cause);
    let run = ops::record_run_event(inner, run_id, RunEvent::Cancel { cause: cancel })?;
    let mut done = WorkEnded {
        run: Some(run_id),
        withdrawn: Vec::new(),
        ended: Ended::default(),
    };
    // 5. Nothing of the run waits on a platform or a shell any more.
    crate::connectors::abort_calls(inner, run_id);
    effects::abort_step_tasks(inner, run_id);
    // 4. The goals born of it go with it.
    let mut visited = HashSet::new();
    let born = open_children_of(
        inner,
        |g| matches!(&g.origin, bisa_core::GoalOrigin::Run { run, .. } if *run == run_id),
    )?;
    for child in born {
        let why = bisa_core::text!(
            "engine-rationale-parent-run-stopped",
            run = run_id.to_string()
        )
        .to_string();
        let child_done = end_child(inner, child, cause, why, &mut visited, 0).await?;
        done.ended.children.push(child);
        done.ended.add(child_done.ended);
    }
    // 6. Every session told, 7. and waited for.
    let home = Home::Run { run: run_id };
    for mark in inner.inflight.iter() {
        if mark.home == home {
            // LCOV_EXCL_START: an item reserved but not yet launched at the stop: a race between the launch and the stop
            mark.stop();
            // LCOV_EXCL_STOP
        }
    }
    let told = noted.max(sessions::stop_for(
        inner,
        Scope::Run(run_id),
        &workstreams,
        cause,
    ));
    let settled = sessions::await_stopped(
        inner,
        Scope::Run(run_id),
        &workstreams,
        told,
        inner.config.stop_deadline(),
    )
    .await;
    done.ended.add(Ended::of(settled));
    Ok((run, done))
}

impl EndCause {
    /// The cause a run's cancel carries, as what the end is for.
    pub fn of_cancel(cancel: &CancelCause) -> Self {
        match cancel {
            CancelCause::Stopped { .. } | CancelCause::Withdrawn => EndCause::Stop,
            CancelCause::Restarted => EndCause::Restart,
            // LCOV_EXCL_START: a close ends a goal's work through close_goal, which stops the runs itself; the arm keeps the match total
            CancelCause::Closed { .. } => EndCause::Close,
            // LCOV_EXCL_STOP
            CancelCause::Retired => EndCause::Retire,
        }
    }
}
