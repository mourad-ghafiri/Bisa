//! Scheduling preflight + concurrency caps.
//!
//! The preflight runs BEFORE any resource is allocated (omp's spawn-policy
//! order); every rejection is typed, never silent.
//!
//! A work item exists because an `agent` step of a run asked for it, so the
//! preflight's central question is whether that step is still `Running` and
//! still names this item: a step that was cancelled, amended away or retried
//! onto a fresh item has nothing for the old one to do.

use crate::config::EngineConfig;
use bisa_core::workitem::{WorkItemSpec, WorkItemState};
use bisa_core::{Home, StepState};
use bisa_store::Workspace;
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::Semaphore;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ScheduleRejection {
    #[error("work item is not open (state: {0})")]
    NotOpen(String),
    /// The item's home — `goal:<id>` or `run:<id>` — spent its ceiling.
    #[error("budget exhausted for {0}")]
    BudgetExhausted(String),
    #[error("goal not found: {0}")]
    GoalMissing(String),
    /// The run the item serves is not there to run it.
    #[error("run not found: {0}")]
    RunMissing(String),
    #[error("harness {0} is disabled by configuration")]
    HarnessDisabled(String),
    #[error("spawn depth exhausted")]
    DepthExhausted,
    #[error("agent {agent:?} not in spawn allowlist {allowlist:?}")]
    SpawnNotAllowed {
        agent: String,
        allowlist: Vec<String>,
    },
    #[error("goal {0} is closed")]
    GoalClosed(String),
    /// A run of the workspace that is over runs nothing more.
    #[error("run {0} is over")]
    RunFinished(String),
    /// The item names no run step. Nothing schedules work a workflow did
    /// not ask for.
    #[error("work item is bound to no run step")]
    NoStep,
    #[error("step `{step}` is {state}, not running this item")]
    StepNotRunning { step: String, state: String },
    /// The concurrency caps were closed under the executor — the engine is
    /// going away; nothing launches any more. A typed refusal the item
    /// settles with, never a panic inside the executor's task.
    #[error("the concurrency caps are closed; nothing launches any more")]
    CapsClosed,
}

/// Preflight for a top-level work-item. Checks the item's state, its home
/// — an open goal, or an unfinished run of the workspace — the run step that
/// asked for it, the home's budget, and the configured disabled-harness list.
/// Why an item is being launched: fresh, or resumed after the process that
/// drove its session died. A fresh launch takes an `Open` item only; a
/// resume takes the item where the crash left it — claimed, in progress or
/// blocked by the interruption — since the session is not the work and the
/// work is in the item's checkout (`mark_in_progress` walks each of those
/// states forward).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Launch {
    Fresh,
    Resume,
}

pub fn preflight(
    ws: &Workspace,
    config: &EngineConfig,
    spec: &WorkItemSpec,
    launch: Launch,
) -> Result<(), ScheduleRejection> {
    let admitted = match launch {
        Launch::Fresh => matches!(spec.state, WorkItemState::Open),
        Launch::Resume => matches!(
            spec.state,
            WorkItemState::Open
                | WorkItemState::Claimed { .. }
                | WorkItemState::InProgress { .. }
                | WorkItemState::Blocked { .. }
        ),
    };
    if !admitted {
        return Err(ScheduleRejection::NotOpen(format!("{:?}", spec.state)));
    }
    // A step that named no harness runs on its agent's, or the default:
    // which, is known once the item is taken, and the launch walk refuses a
    // disabled one by name. What the step did name is judged here.
    if !spec.harness_candidates.is_empty()
        && spec
            .harness_candidates
            .iter()
            .all(|h| config.disabled_harnesses.contains(h))
    {
        return Err(ScheduleRejection::HarnessDisabled(
            spec.harness_candidates.join(","),
        ));
    }
    match spec.home {
        Home::Goal { goal } => {
            let goal = ws
                .get_goal(goal)
                .map_err(|_| ScheduleRejection::GoalMissing(goal.to_string()))?;
            if goal.is_closed() {
                return Err(ScheduleRejection::GoalClosed(goal.id.to_string()));
            }
        }
        Home::Run { run } => {
            let run = ws
                .get_run(run)
                .map_err(|_| ScheduleRejection::RunMissing(run.to_string()))?;
            if run.is_finished() {
                return Err(ScheduleRejection::RunFinished(run.id.to_string()));
            }
        }
    }
    let (Some(run_id), Some(step)) = (spec.run, spec.step.as_ref()) else {
        return Err(ScheduleRejection::NoStep);
    };
    let run = ws
        .get_run(run_id)
        .map_err(|_| ScheduleRejection::RunMissing(run_id.to_string()))?;
    match run.steps.get(step) {
        Some(record) if record.state == StepState::Running && record.work_item == Some(spec.id) => {
        }
        Some(record) => {
            return Err(ScheduleRejection::StepNotRunning {
                step: step.to_string(),
                state: if record.state == StepState::Running {
                    "running another item".to_string()
                } else {
                    record.state.as_str().to_string()
                },
            })
        }
        None => {
            return Err(ScheduleRejection::StepNotRunning {
                step: step.to_string(),
                state: "not in the run".to_string(),
            })
        }
    }
    if !ws.budget_allows(&spec.home).unwrap_or(false) {
        return Err(ScheduleRejection::BudgetExhausted(spec.home.to_string()));
    }
    Ok(())
}

/// Preflight for a *spawned* child work-item (from a running session).
/// The parent's spec constrains the child (depth, allowlist), then the
/// regular preflight applies.
pub fn preflight_spawn(
    ws: &Workspace,
    config: &EngineConfig,
    parent: &WorkItemSpec,
    child_agent: &str,
    child: &WorkItemSpec,
) -> Result<(), ScheduleRejection> {
    if parent.depth_budget == 0 {
        return Err(ScheduleRejection::DepthExhausted);
    }
    let allowed = parent
        .spawn_allowlist
        .iter()
        .any(|a| a == "*" || a == child_agent);
    if !allowed {
        return Err(ScheduleRejection::SpawnNotAllowed {
            agent: child_agent.to_string(),
            allowlist: parent.spawn_allowlist.clone(),
        });
    }
    preflight(ws, config, child, Launch::Fresh)
}

/// Global + per-adapter concurrency limits.
pub struct ConcurrencyCaps {
    global: Arc<Semaphore>,
    per_adapter: DashMap<String, Arc<Semaphore>>,
    per_adapter_limit: usize,
}

impl ConcurrencyCaps {
    pub fn new(global: usize, per_adapter: usize) -> Self {
        Self {
            global: Arc::new(Semaphore::new(global.max(1))),
            per_adapter: DashMap::new(),
            per_adapter_limit: per_adapter.max(1),
        }
    }

    /// Acquire both permits (global then adapter); held for the run's life.
    /// `CapsClosed` when a semaphore was closed — the one way this fails.
    pub async fn acquire(
        &self,
        adapter_id: &str,
    ) -> Result<
        (
            tokio::sync::OwnedSemaphorePermit,
            tokio::sync::OwnedSemaphorePermit,
        ),
        ScheduleRejection,
    > {
        let global = Arc::clone(&self.global)
            .acquire_owned()
            .await
            .map_err(|_| ScheduleRejection::CapsClosed)?;
        let adapter_sem = self
            .per_adapter
            .entry(adapter_id.to_string())
            .or_insert_with(|| Arc::new(Semaphore::new(self.per_adapter_limit)))
            .clone();
        let adapter = adapter_sem
            .acquire_owned()
            .await
            .map_err(|_| ScheduleRejection::CapsClosed)?;
        Ok((global, adapter))
    }
}
