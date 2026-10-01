//! Where an agent step runs — the rule, with no I/O in it.
//!
//! A step names a project, or the goal has exactly one attached and that is
//! it, or there is none and the step runs in its home's scratch folder with
//! no workstream: its result is its deliverable, and nothing there is
//! committed. A run of the workspace has nothing attached, so its step runs
//! in the project it names or in the run's own scratch. A goal with several
//! projects and a step naming none is ambiguous, and ambiguity is refused
//! rather than guessed: the person names the project with a `project` input.
//! **No project is ever born of a step**: an agent that was asked for files
//! that must be kept makes one with `create_project`, once, and names it.
//!
//! The scratch folder is also a `check` command's cwd when the run has no
//! project, the Workflow Agent's design session, and every session's `TMPDIR`.

use crate::ProjectId;

/// The project an agent step runs in, resolved from what the step names and
/// what the run's goal has attached — nothing, for a run of the workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepProject {
    /// The step named it; on a goal it must be attached to the goal.
    Named(ProjectId),
    /// The step named none and the goal has exactly one.
    Single(ProjectId),
    /// The step named none and nothing is attached: the step runs in its
    /// home's scratch folder — the goal's, or the run of the workspace's —
    /// with no workstream; its result is its deliverable.
    Scratch,
    /// The step named none and the goal has several: refuse, by name.
    Ambiguous { count: usize },
}

/// The one rule. `named` wins whatever is attached.
pub fn resolve_step_project(named: Option<ProjectId>, attached: &[ProjectId]) -> StepProject {
    if let Some(p) = named {
        return StepProject::Named(p);
    }
    match attached {
        [] => StepProject::Scratch,
        [one] => StepProject::Single(*one),
        many => StepProject::Ambiguous { count: many.len() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pid(n: u8) -> ProjectId {
        ProjectId::from_ulid(ulid::Ulid::from_parts(n as u64 + 1, n as u128 + 1))
    }

    #[test]
    fn a_named_project_wins_over_attachments() {
        assert_eq!(
            resolve_step_project(Some(pid(1)), &[pid(2), pid(3)]),
            StepProject::Named(pid(1))
        );
        assert_eq!(
            resolve_step_project(Some(pid(1)), &[]),
            StepProject::Named(pid(1))
        );
    }

    #[test]
    fn no_attachment_means_scratch() {
        assert_eq!(resolve_step_project(None, &[]), StepProject::Scratch);
    }

    #[test]
    fn one_attachment_is_the_project() {
        assert_eq!(
            resolve_step_project(None, &[pid(4)]),
            StepProject::Single(pid(4))
        );
    }

    #[test]
    fn two_attachments_are_ambiguous() {
        assert_eq!(
            resolve_step_project(None, &[pid(1), pid(2)]),
            StepProject::Ambiguous { count: 2 }
        );
    }
}
