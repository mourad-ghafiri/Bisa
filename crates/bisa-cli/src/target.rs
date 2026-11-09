//! What an id on the command line names, for the verbs that take either
//! kind of run: a **goal** — its current run, decided through the goal — or
//! a **run** by its own id, a goal's or a run of the workspace, decided
//! through the run's home.
//!
//! Both are ULIDs, so the id is looked up rather than guessed at: a goal
//! first, then a run. The workspace answers the look-up directly, with or
//! without a daemon — it is a read.

use anyhow::{anyhow, Result};
use bisa_core::{GoalId, Home, RunId};
use bisa_store::Workspace;
use std::str::FromStr;

/// The id a command printed under an inbox row takes for one of its
/// decisions (`needs_action`): the run's when the decision is filed on a run
/// of the workspace — its row is keyed by the workflow, which names neither
/// a goal nor a run — else the row's own key.
pub fn hint_id<'a>(row_key: &'a str, action: &'a serde_json::Value) -> &'a str {
    match action["home"]["home"].as_str() {
        Some("run") => action["home"]["run"].as_str().unwrap_or(row_key),
        _ => row_key,
    }
}

/// A goal, or one run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Goal(GoalId),
    Run(RunId),
}

impl Target {
    /// The goal or the run `raw` names; refused in a sentence when it names
    /// neither.
    pub fn resolve(ws: &Workspace, raw: &str) -> Result<Self> {
        let raw = raw.trim();
        let refused = || {
            anyhow!(bisa_core::text!(
                "cli-target-names-no-goal-or-run",
                id = format!("{raw:?}")
            ))
        };
        if let Ok(goal) = GoalId::from_str(raw) {
            if ws.get_goal(goal).is_ok() {
                return Ok(Target::Goal(goal));
            }
        }
        let run = RunId::from_str(raw).map_err(|_| refused())?;
        ws.get_run(run).map_err(|_| refused())?;
        Ok(Target::Run(run))
    }

    /// The run a step verb acts on: the goal's current run, or the run
    /// itself. A goal that never ran has none, and says so.
    pub fn run(&self, ws: &Workspace) -> Result<RunId> {
        match self {
            Target::Run(run) => Ok(*run),
            Target::Goal(goal) => ws.get_current_run(*goal)?.map(|r| r.id).ok_or_else(|| {
                anyhow!(bisa_core::text!(
                    "cli-run-goal-has-no-run-yet",
                    id = goal.to_string()
                ))
            }),
        }
    }

    /// Where a decision about it is filed and made: the goal, or the run's
    /// home — its goal for a goal's run, itself for a run of the workspace.
    pub fn home(&self, ws: &Workspace) -> Result<Home> {
        match self {
            Target::Goal(goal) => Ok(Home::Goal { goal: *goal }),
            Target::Run(run) => Ok(ws.get_run(*run)?.home()),
        }
    }
}

/// The node route a decision about `home` goes to.
pub fn decide_route(home: &Home) -> String {
    match home {
        Home::Goal { goal } => format!("/goals/{goal}/decide"),
        Home::Run { run } => format!("/runs/{run}/decide"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The command printed under an inbox row names an id the verb takes: a
    /// goal row's own key, and — under a workspace run's row, keyed by its
    /// workflow — the run the decision is filed on.
    #[test]
    fn the_inbox_hint_under_a_workspace_runs_row_is_a_command_that_works() {
        let goal = json!({"home": {"home": "goal", "goal": "01GOAL"}});
        assert_eq!(hint_id("01GOAL", &goal), "01GOAL");
        let run = json!({"home": {"home": "run", "run": "01RUN"}});
        assert_eq!(hint_id("01WORKFLOW", &run), "01RUN");
        assert_eq!(
            hint_id("01ROW", &json!({})),
            "01ROW",
            "a row with no home keeps its key"
        );
    }
}
