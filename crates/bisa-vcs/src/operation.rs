//! The facts of an operation git has left half-done (ide/04 §Conflicts,
//! continued): what is being merged, rebased, picked or reverted, which
//! side of a conflict came from where, and how far a rebase or a pick has
//! got — read from the files git keeps under its directory, never from
//! prose, so an operation started in a terminal is described as well as one
//! started here. `Markers` is what the directory holds; `facts_of` is the
//! pure reading of it, so the rule is tested without a repository; the
//! [`crate::git::Git::operation_facts`] wrapper adds what only git can say —
//! a commit's subject, the branch a sha is the tip of.
//!
//! The sides are **git's** `ours` and `theirs`: under a rebase `ours` is
//! the branch rebased onto and `theirs` the commit replayed. The one place
//! that turns them into *mine* and *theirs* for a person is the desktop's
//! `conflictSidesModel`, so the swap is made once.

use crate::interactive::InProgress;
use std::path::Path;

/// What a side of the operation is: a branch, a remote-tracking branch, or
/// one commit (the one replayed, picked or reverted).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideRole {
    Branch,
    Upstream,
    Commit,
}

/// One side of the operation, named as far as git can name it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SideRef {
    pub role: SideRole,
    /// The branch or remote-tracking branch, when the side is one.
    pub name: Option<String>,
    /// The commit's full sha, when known.
    pub commit: Option<String>,
    /// The commit's subject line, when known.
    pub subject: Option<String>,
}

impl SideRef {
    fn branch(name: Option<String>) -> Self {
        Self {
            role: SideRole::Branch,
            name,
            commit: None,
            subject: None,
        }
    }
    fn commit(sha: Option<String>) -> Self {
        Self {
            role: SideRole::Commit,
            name: None,
            commit: sha,
            subject: None,
        }
    }
}

/// Where a rebase, a cherry-pick or a revert of several commits stands:
/// `done` is the step git stopped on, one-based, of `total`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Step {
    pub done: u32,
    pub total: u32,
}

/// The operation half-done in a checkout, as facts.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct OperationFacts {
    pub kind: InProgress,
    /// The branch the operation is on: the current branch, or under a rebase
    /// the branch being rebased (HEAD is detached while it runs).
    pub branch: Option<String>,
    /// git's `ours`: the current branch for a merge, a pick or a revert; the
    /// branch rebased onto for a rebase.
    pub ours: SideRef,
    /// git's `theirs`: the branch merged in; the commit replayed, picked or
    /// reverted.
    pub theirs: SideRef,
    pub step: Option<Step>,
}

/// What git's directory holds about the operation: each file read as it is,
/// absent when the file is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Markers {
    /// `rebase-merge/head-name` or `rebase-apply/head-name`: `refs/heads/<branch>`.
    pub head_name: Option<String>,
    /// `rebase-merge/onto` or `rebase-apply/onto`: the sha rebased onto.
    pub onto: Option<String>,
    /// `rebase-merge/msgnum` of `end`, or `rebase-apply/next` of `last`.
    pub step: Option<(u32, u32)>,
    /// The commit the operation stopped on: `REBASE_HEAD`, `MERGE_HEAD`'s first
    /// line, `CHERRY_PICK_HEAD` or `REVERT_HEAD`.
    pub stopped: Option<String>,
    /// The first line of `MERGE_MSG`.
    pub merge_msg: Option<String>,
    /// `sequencer/done` and `sequencer/todo` step counts, for a pick or a
    /// revert of several commits.
    pub sequencer: Option<(u32, u32)>,
}

impl Markers {
    /// Read the files for `kind` under `git_dir`.
    pub fn read(git_dir: &Path, kind: InProgress) -> Self {
        let text = |rel: &str| {
            std::fs::read_to_string(git_dir.join(rel))
                .ok()
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
        };
        let first_line = |rel: &str| text(rel).map(|t| t.lines().next().unwrap_or("").to_string());
        let number = |rel: &str| text(rel).and_then(|t| t.parse::<u32>().ok());
        let steps =
            |rel: &str| text(rel).map(|t| t.lines().filter(|l| is_step_line(l)).count() as u32);
        match kind {
            InProgress::Rebase => {
                let backend = if git_dir.join("rebase-merge").is_dir() {
                    "rebase-merge"
                } else {
                    "rebase-apply"
                };
                let (at, of) = if backend == "rebase-merge" {
                    ("rebase-merge/msgnum", "rebase-merge/end")
                } else {
                    ("rebase-apply/next", "rebase-apply/last")
                };
                Self {
                    head_name: text(&format!("{backend}/head-name")),
                    onto: text(&format!("{backend}/onto")),
                    step: number(at).zip(number(of)),
                    stopped: text("REBASE_HEAD")
                        .or_else(|| text("rebase-merge/stopped-sha"))
                        .or_else(|| text("rebase-apply/original-commit")),
                    ..Self::default()
                }
            }
            InProgress::Merge => Self {
                stopped: first_line("MERGE_HEAD"),
                merge_msg: first_line("MERGE_MSG"),
                ..Self::default()
            },
            InProgress::CherryPick | InProgress::Revert => Self {
                stopped: text(if kind == InProgress::CherryPick {
                    "CHERRY_PICK_HEAD"
                } else {
                    "REVERT_HEAD"
                }),
                // `done` is written after the first step lands; a pick that
                // stopped on its first has `todo` alone, and nothing done yet.
                sequencer: steps("sequencer/todo")
                    .map(|todo| (steps("sequencer/done").unwrap_or(0), todo)),
                ..Self::default()
            },
        }
    }
}

/// A line of the sequencer's todo or done file that is a step — `pick`,
/// `revert`, and the rest — not a comment or a blank.
fn is_step_line(line: &str) -> bool {
    let l = line.trim_start();
    !l.is_empty() && !l.starts_with('#')
}

/// The facts as the markers alone tell them. `branch` is the current branch
/// (none while a rebase runs detached — the markers name it then).
pub fn facts_of(kind: InProgress, markers: &Markers, branch: Option<String>) -> OperationFacts {
    match kind {
        InProgress::Merge => OperationFacts {
            kind,
            branch: branch.clone(),
            ours: SideRef::branch(branch),
            theirs: merged_side(markers),
            step: None,
        },
        InProgress::Rebase => {
            let rebased = markers
                .head_name
                .as_deref()
                .map(|n| n.strip_prefix("refs/heads/").unwrap_or(n).to_string())
                .or(branch);
            OperationFacts {
                kind,
                branch: rebased,
                ours: SideRef {
                    role: SideRole::Branch,
                    name: None,
                    commit: markers.onto.clone(),
                    subject: None,
                },
                theirs: SideRef::commit(markers.stopped.clone()),
                step: markers.step.map(|(done, total)| Step { done, total }),
            }
        }
        InProgress::CherryPick | InProgress::Revert => OperationFacts {
            kind,
            branch: branch.clone(),
            ours: SideRef::branch(branch),
            theirs: SideRef::commit(markers.stopped.clone()),
            // The stopped step is still at the top of `todo` until it lands.
            step: markers
                .sequencer
                .filter(|(_, todo)| *todo > 0)
                .map(|(done, todo)| Step {
                    done: done + 1,
                    total: done + todo,
                }),
        },
    }
}

/// The side a merge brings in, named from `MERGE_MSG`'s first line — git's
/// own *Merge branch 'x'*, *Merge remote-tracking branch 'origin/main'*,
/// *Merge tag 'v1'*, *Merge commit 'abc'* — else the commit alone.
fn merged_side(markers: &Markers) -> SideRef {
    let quoted = markers.merge_msg.as_deref().and_then(|m| {
        let (lead, rest) = m.split_once('\'')?;
        let (name, _) = rest.split_once('\'')?;
        Some((lead.trim().to_string(), name.to_string()))
    });
    match quoted {
        Some((lead, name)) if lead == "Merge branch" => SideRef {
            role: SideRole::Branch,
            name: Some(name),
            commit: markers.stopped.clone(),
            subject: None,
        },
        Some((lead, name)) if lead == "Merge remote-tracking branch" => SideRef {
            role: SideRole::Upstream,
            name: Some(name),
            commit: markers.stopped.clone(),
            subject: None,
        },
        Some((lead, name)) if lead.starts_with("Merge tag") => SideRef {
            role: SideRole::Commit,
            name: Some(name),
            commit: markers.stopped.clone(),
            subject: None,
        },
        _ => SideRef::commit(markers.stopped.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha(n: u8) -> Option<String> {
        Some(format!("{n:0>40}"))
    }

    #[test]
    fn a_merge_names_its_branch_and_the_branch_merged_in_from_the_prepared_message() {
        let m = Markers {
            stopped: sha(3),
            merge_msg: Some("Merge branch 'feature/login'".into()),
            ..Markers::default()
        };
        let f = facts_of(InProgress::Merge, &m, Some("main".into()));
        assert_eq!(f.branch.as_deref(), Some("main"));
        assert_eq!(f.ours, SideRef::branch(Some("main".into())));
        assert_eq!(f.theirs.role, SideRole::Branch);
        assert_eq!(f.theirs.name.as_deref(), Some("feature/login"));
        assert_eq!(f.theirs.commit, sha(3));
        assert_eq!(f.step, None, "a merge has no steps");

        let pull = Markers {
            stopped: sha(4),
            merge_msg: Some("Merge remote-tracking branch 'origin/main'".into()),
            ..Markers::default()
        };
        let f = facts_of(InProgress::Merge, &pull, Some("main".into()));
        assert_eq!(
            f.theirs.role,
            SideRole::Upstream,
            "a pull merges the upstream"
        );
        assert_eq!(f.theirs.name.as_deref(), Some("origin/main"));

        let bare = Markers {
            stopped: sha(5),
            ..Markers::default()
        };
        let f = facts_of(InProgress::Merge, &bare, None);
        assert_eq!(
            f.theirs,
            SideRef::commit(sha(5)),
            "no message: the commit alone"
        );
        assert_eq!(f.ours.name, None);
    }

    #[test]
    fn a_rebase_names_the_branch_replayed_the_commit_stopped_on_and_the_step() {
        let m = Markers {
            head_name: Some("refs/heads/feature".into()),
            onto: sha(9),
            step: Some((3, 7)),
            stopped: sha(2),
            ..Markers::default()
        };
        let f = facts_of(InProgress::Rebase, &m, None);
        assert_eq!(
            f.branch.as_deref(),
            Some("feature"),
            "HEAD is detached: the markers name it"
        );
        assert_eq!(f.ours.role, SideRole::Branch);
        assert_eq!(
            f.ours.commit,
            sha(9),
            "git's ours is the branch rebased onto"
        );
        assert_eq!(
            f.theirs,
            SideRef::commit(sha(2)),
            "git's theirs is the commit replayed"
        );
        assert_eq!(f.step, Some(Step { done: 3, total: 7 }));
    }

    #[test]
    fn a_pick_or_a_revert_of_several_commits_counts_its_step_from_the_sequencer() {
        let m = Markers {
            stopped: sha(6),
            sequencer: Some((1, 2)),
            ..Markers::default()
        };
        let f = facts_of(InProgress::CherryPick, &m, Some("main".into()));
        assert_eq!(f.theirs, SideRef::commit(sha(6)));
        assert_eq!(
            f.step,
            Some(Step { done: 2, total: 3 }),
            "one landed, the stopped one is second of three"
        );

        let single = Markers {
            stopped: sha(7),
            ..Markers::default()
        };
        let f = facts_of(InProgress::Revert, &single, Some("main".into()));
        assert_eq!(f.step, None, "one commit: no steps to count");
        assert_eq!(f.ours, SideRef::branch(Some("main".into())));
    }

    #[test]
    fn the_sequencer_files_are_read_step_by_step_and_a_rebase_by_its_backend() {
        let dir = tempfile::tempdir().unwrap();
        let g = dir.path();
        std::fs::create_dir_all(g.join("sequencer")).unwrap();
        std::fs::write(g.join("CHERRY_PICK_HEAD"), "aaaa\n").unwrap();
        std::fs::write(g.join("sequencer/done"), "pick 1111 one\n").unwrap();
        std::fs::write(
            g.join("sequencer/todo"),
            "pick 2222 two\n# a comment\n\npick 3333 three\n",
        )
        .unwrap();
        let m = Markers::read(g, InProgress::CherryPick);
        assert_eq!(m.stopped.as_deref(), Some("aaaa"));
        assert_eq!(
            m.sequencer,
            Some((1, 2)),
            "comments and blanks are not steps"
        );

        std::fs::create_dir_all(g.join("rebase-merge")).unwrap();
        std::fs::write(g.join("rebase-merge/head-name"), "refs/heads/topic\n").unwrap();
        std::fs::write(g.join("rebase-merge/onto"), "bbbb\n").unwrap();
        std::fs::write(g.join("rebase-merge/msgnum"), "2\n").unwrap();
        std::fs::write(g.join("rebase-merge/end"), "5\n").unwrap();
        std::fs::write(g.join("REBASE_HEAD"), "cccc\n").unwrap();
        let m = Markers::read(g, InProgress::Rebase);
        assert_eq!(m.head_name.as_deref(), Some("refs/heads/topic"));
        assert_eq!(m.onto.as_deref(), Some("bbbb"));
        assert_eq!(m.step, Some((2, 5)));
        assert_eq!(m.stopped.as_deref(), Some("cccc"));

        std::fs::write(g.join("MERGE_HEAD"), "dddd\neeee\n").unwrap();
        std::fs::write(g.join("MERGE_MSG"), "Merge branch 'x'\n\n# Conflicts:\n").unwrap();
        let m = Markers::read(g, InProgress::Merge);
        assert_eq!(
            m.stopped.as_deref(),
            Some("dddd"),
            "the first head of an octopus"
        );
        assert_eq!(m.merge_msg.as_deref(), Some("Merge branch 'x'"));
    }
}
