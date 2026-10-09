//! `Workstream`: one checkout occupant of a project — the place where terminals,
//! agent sessions and editors live. Modelled on the reference
//! implementation's *workspace*: the default realisation is a git worktree on
//! its own branch, a plain-folder project gets a copy, and **every
//! project has exactly one primary workstream, which is the repository's own
//! root checkout**.
//!
//! A workstream is **local state, never GEP**. It stands for a directory on
//! one machine, so publishing it to other nodes would be a lie; what *is* true
//! everywhere (the branch was pushed, PR #12 opened) is journaled as a
//! progress fact.
//!
//! **No path is stored.** The store resolves the checkout from the kind — the
//! primary is the project root, everything else lives under the project's
//! `workstreams/` — so a record can never point outside its project.
//!
//! Its lifecycle changes only through [`WorkstreamTransition`] and
//! [`Workstream::apply`], which guards the primary and then delegates to the
//! pure state table [`WorkstreamState::apply`] — the pattern the goal and the
//! work item use.

use crate::board::WorkstreamBoard;
use crate::id::{GoalId, ProjectId, WorkItemId, WorkstreamId};
use serde::{Deserialize, Serialize};

impl WorkstreamId {
    /// The primary workstream's id **is its project's ULID**. One id, one
    /// record: "exactly one primary per project" is true by construction, the
    /// primary is addressable without a field on the (synced) project record,
    /// and every `…/project/<id>` address becomes `…/workstream/<id>` by
    /// changing one word.
    pub fn primary_of(project: ProjectId) -> Self {
        WorkstreamId(project.0)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Workstream {
    pub id: WorkstreamId,
    /// Required — a workstream is always of a project, and lives under it.
    pub project: ProjectId,
    /// A label the person chose. `None` means "call me by my branch" (see
    /// [`Self::label`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Free text the person keeps beside the workstream — what it is for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pinned: bool,
    /// What stands behind the checkout, and therefore where it is.
    pub kind: WorkstreamKind,
    /// The goal it was created *for*. History, not a live association: it
    /// survives detaching the project from that goal. `None` for a branch
    /// made in the IDE and for the primary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal: Option<GoalId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_item: Option<WorkItemId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    pub state: WorkstreamState,
    pub created_at: u64,
    /// Its place on the Board — a column, a rank, a due date — nothing until
    /// a person sets something (`board.rs`). A view over the workstream,
    /// never its lifecycle: the column is not the state.
    #[serde(default, skip_serializing_if = "WorkstreamBoard::is_empty")]
    pub board: WorkstreamBoard,
}

impl Workstream {
    pub fn is_primary(&self) -> bool {
        matches!(self.kind, WorkstreamKind::Primary)
    }

    /// A repository to run git in, or the refusal: a copy of a non-git
    /// project has none, so committing, pushing and diffing it is a category
    /// error rather than a failure.
    pub fn require_repository(&self) -> Result<(), WorkstreamError> {
        match self.kind {
            WorkstreamKind::Copy => Err(WorkstreamError::NoRepository {
                id: self.id.to_string(),
            }),
            WorkstreamKind::Primary | WorkstreamKind::Worktree { .. } => Ok(()),
        }
    }

    /// The branch and its base — what only a worktree has, and what a pull
    /// request or a forced push is made of. The primary is on whatever the
    /// root is on; a copy has no repository at all.
    pub fn require_own_branch(&self) -> Result<(&str, &str), WorkstreamError> {
        match &self.kind {
            WorkstreamKind::Worktree { branch, base } => Ok((branch, base)),
            WorkstreamKind::Primary => Err(WorkstreamError::NoOwnBranch {
                id: self.id.to_string(),
            }),
            WorkstreamKind::Copy => Err(WorkstreamError::NoRepository {
                id: self.id.to_string(),
            }),
        }
    }

    /// The branch recorded on a worktree workstream. The primary's branch is
    /// whatever its checkout is on and is read live, never recorded.
    pub fn branch(&self) -> Option<&str> {
        match &self.kind {
            WorkstreamKind::Worktree { branch, .. } => Some(branch),
            WorkstreamKind::Primary | WorkstreamKind::Copy => None,
        }
    }

    /// The branch a worktree workstream was cut from.
    pub fn base(&self) -> Option<&str> {
        match &self.kind {
            WorkstreamKind::Worktree { base, .. } => Some(base),
            WorkstreamKind::Primary | WorkstreamKind::Copy => None,
        }
    }

    /// What to call this workstream: the person's name for it, else its
    /// recorded branch, else the live branch the caller read, else
    /// `"primary"` — a copy workstream with none of those is named by its id.
    pub fn label(&self, live_branch: Option<&str>) -> String {
        if let Some(n) = self
            .name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
        {
            return n.to_string();
        }
        if let Some(b) = self.branch() {
            return b.to_string();
        }
        if let Some(b) = live_branch.map(str::trim).filter(|b| !b.is_empty()) {
            return b.to_string();
        }
        match self.kind {
            WorkstreamKind::Primary => "primary".to_string(),
            _ => self.id.to_string(),
        }
    }

    /// The kind-aware transition — **the only thing the store's writer
    /// calls.** The primary is the project itself: it can be committed to and
    /// pushed like any checkout, but it is never closed and never *finishes*
    /// as a branch would (no pull request, no merge). Everything else is the
    /// pure state table, [`WorkstreamState::apply`].
    pub fn apply(&self, t: &WorkstreamTransition) -> Result<WorkstreamState, WorkstreamError> {
        if self.is_primary()
            && matches!(
                t,
                WorkstreamTransition::Close
                    | WorkstreamTransition::PrOpened { .. }
                    | WorkstreamTransition::PrAdopted { .. }
                    | WorkstreamTransition::Merged
            )
        {
            return Err(WorkstreamError::Primary {
                transition: t.name(),
            });
        }
        self.state.clone().apply(t)
    }
}

/// What stands behind a workstream's checkout. The path is not here on
/// purpose: the store derives it from the kind, so a record cannot name a
/// directory outside its project.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "kind")]
pub enum WorkstreamKind {
    /// The repository's own root checkout. Exactly one per project, born with
    /// it; its id is the project's ([`WorkstreamId::primary_of`]). It cannot
    /// be closed or deleted — removing the project is the substitute.
    Primary,
    /// A git worktree standing on `branch`, opened from a
    /// [`WorkstreamSource`]; `base` is the branch the work returns to — what
    /// the lifecycle diffs against and a pull request targets — and never a
    /// fiction: a checkout of an existing branch records the base it was
    /// given, not one it was cut from. The worktree *is* the isolation, it
    /// persists, and the artifact is a commit.
    Worktree { branch: String, base: String },
    /// A non-git project falls back to the isolation floor: a copy.
    Copy,
}

/// Where a workstream **starts** — what branch its checkout stands on and
/// where that branch's HEAD begins (ide/07 §Where a workstream starts). This
/// is a different fact from the base: the base is where the work *goes back
/// to*; the source is where it *comes from*. A workstream is always a branch
/// and a checkout, one-to-one, so a tag or a commit opens a new branch at
/// that point rather than a detached HEAD.
///
/// On the wire the variant is the `source` word, so a request reads
/// `{"source":"local_branch","name":"topic"}`; the CLI spells the same thing
/// as a spec — `branch:topic`, `remote:origin/topic`, `tag:v1`,
/// `new-tag:v1@main`, `pr:12`, or a bare name for a new branch
/// ([`FromStr`]).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "source")]
pub enum WorkstreamSource {
    /// A new branch: `name` as a person typed it (made safe, never refused)
    /// or, absent, one derived from the label; `start` is the ref HEAD begins
    /// at — the base when absent.
    NewBranch {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        start: Option<String>,
    },
    /// A branch that already exists in the repository, checked out as it is.
    LocalBranch { name: String },
    /// A branch on a remote: fetched, then a local branch of the same name
    /// tracking it.
    RemoteBranch { remote: String, name: String },
    /// A tag: a new branch at it, `branch` when a person named one, else
    /// `from/<tag>`. With `create_at`, the tag is made first, at that ref.
    Tag {
        name: String,
        #[serde(default)]
        branch: Option<String>,
        #[serde(default)]
        create_at: Option<String>,
    },
    /// An open pull request on the code host behind `origin`: its head branch
    /// fetched and tracked, its base as the base, the record born linked
    /// ([`WorkstreamTransition::PrAdopted`]).
    PullRequest { number: u64 },
}

impl Default for WorkstreamSource {
    /// A derived new branch at the base — what a work item opens.
    fn default() -> Self {
        WorkstreamSource::NewBranch {
            name: None,
            start: None,
        }
    }
}

impl WorkstreamSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkstreamSource::NewBranch { .. } => "new_branch",
            WorkstreamSource::LocalBranch { .. } => "local_branch",
            WorkstreamSource::RemoteBranch { .. } => "remote_branch",
            WorkstreamSource::Tag { .. } => "tag",
            WorkstreamSource::PullRequest { .. } => "pull_request",
        }
    }

    /// The branch the checkout will stand on, when the source names it
    /// before git runs: a typed new-branch name, an existing branch, a remote
    /// branch's local twin, a tag's branch. `None` when it is derived (from
    /// the label, from the tag, from the pull request).
    pub fn branch_hint(&self) -> Option<&str> {
        match self {
            WorkstreamSource::NewBranch { name, .. } => name.as_deref(),
            WorkstreamSource::LocalBranch { name }
            | WorkstreamSource::RemoteBranch { name, .. } => Some(name),
            WorkstreamSource::Tag { branch, .. } => branch.as_deref(),
            WorkstreamSource::PullRequest { .. } => None,
        }
    }

    /// One line for the journal and the opened event.
    pub fn describe(&self) -> String {
        match self {
            WorkstreamSource::NewBranch {
                name: None,
                start: None,
            } => "a new branch".to_string(),
            WorkstreamSource::NewBranch {
                name: Some(n),
                start: None,
            } => format!("a new branch {n}"),
            WorkstreamSource::NewBranch {
                name: None,
                start: Some(s),
            } => {
                format!("a new branch at {s}")
            }
            WorkstreamSource::NewBranch {
                name: Some(n),
                start: Some(s),
            } => {
                format!("a new branch {n} at {s}")
            }
            WorkstreamSource::LocalBranch { name } => format!("the branch {name}"),
            WorkstreamSource::RemoteBranch { remote, name } => {
                format!("the branch {remote}/{name}")
            }
            WorkstreamSource::Tag {
                name,
                create_at: None,
                ..
            } => format!("the tag {name}"),
            WorkstreamSource::Tag {
                name,
                create_at: Some(at),
                ..
            } => {
                format!("a new tag {name} at {at}")
            }
            WorkstreamSource::PullRequest { number } => format!("pull request #{number}"),
        }
    }
}

/// A source spec the CLI could not read, with the spelling it wanted.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct SourceSpecError(String);

impl std::str::FromStr for WorkstreamSource {
    type Err = SourceSpecError;

    /// `branch:<name>` · `remote:<remote>/<name>` · `tag:<name>` ·
    /// `new-tag:<name>@<ref>` · `pr:<number>` · `new:<name>[@<ref>]` — or a
    /// bare name, which is a new branch of that name.
    fn from_str(spec: &str) -> Result<Self, Self::Err> {
        let spec = spec.trim();
        if spec.is_empty() {
            return Err(SourceSpecError("a source is needed".into()));
        }
        let want = |what: &str| SourceSpecError(format!("{spec}: {what}"));
        let Some((kind, rest)) = spec.split_once(':') else {
            return Ok(WorkstreamSource::NewBranch {
                name: Some(spec.to_string()),
                start: None,
            });
        };
        let rest = rest.trim();
        if rest.is_empty() {
            return Err(want("a name is needed after the colon"));
        }
        match kind.trim() {
            "new" => {
                let (name, start) = match rest.split_once('@') {
                    Some((n, s)) => (n.trim().to_string(), Some(s.trim().to_string())),
                    None => (rest.to_string(), None),
                };
                Ok(WorkstreamSource::NewBranch {
                    name: Some(name),
                    start,
                })
            }
            "branch" => Ok(WorkstreamSource::LocalBranch {
                name: rest.to_string(),
            }),
            "remote" => match rest.split_once('/') {
                Some((remote, name)) if !remote.is_empty() && !name.is_empty() => {
                    Ok(WorkstreamSource::RemoteBranch {
                        remote: remote.to_string(),
                        name: name.to_string(),
                    })
                }
                _ => Err(want("a remote branch is spelled remote:<remote>/<branch>")),
            },
            "tag" => Ok(WorkstreamSource::Tag {
                name: rest.to_string(),
                branch: None,
                create_at: None,
            }),
            "new-tag" => match rest.split_once('@') {
                Some((name, at)) if !name.trim().is_empty() && !at.trim().is_empty() => {
                    Ok(WorkstreamSource::Tag {
                        name: name.trim().to_string(),
                        branch: None,
                        create_at: Some(at.trim().to_string()),
                    })
                }
                _ => Err(want("a new tag is spelled new-tag:<name>@<ref>")),
            },
            "pr" => rest
                .trim_start_matches('#')
                .parse::<u64>()
                .map(|number| WorkstreamSource::PullRequest { number })
                .map_err(|_| want("a pull request is spelled pr:<number>")),
            other => Err(want(&format!(
                "{other} is not a source — new, branch, remote, tag, new-tag or pr"
            ))),
        }
    }
}

impl WorkstreamKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkstreamKind::Primary => "primary",
            WorkstreamKind::Worktree { .. } => "worktree",
            WorkstreamKind::Copy => "copy",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "state")]
pub enum WorkstreamState {
    Open,
    /// A commit was attempted and refused; the tree is exactly as left.
    Dirty,
    Committed,
    Pushed,
    PrOpen {
        number: u64,
        url: String,
    },
    /// Merged, and still naming the pull request it went in through — a
    /// merged branch's PR is the record of how the work landed, and a surface
    /// that forgets it the moment it succeeds cannot show what happened.
    Merged {
        number: u64,
        url: String,
    },
    Closed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "transition")]
pub enum WorkstreamTransition {
    /// Open | Dirty | Committed | Pushed -> Dirty.
    CommitRefused,
    /// Open | Dirty | Committed | Pushed -> Committed. A commit on a pushed
    /// branch is unpushed work again.
    Committed,
    /// Committed | Pushed -> Pushed; PrOpen -> PrOpen (more commits reached
    /// the open pull request's branch).
    Pushed,
    /// Pushed -> PrOpen.
    PrOpened { number: u64, url: String },
    /// Open -> PrOpen: the workstream was opened *from* a pull request that
    /// already exists on the code host — its head fetched and checked out —
    /// so the record is born linked, never having pushed anything itself.
    PrAdopted { number: u64, url: String },
    /// PrOpen -> Merged, keeping the pull request's number and url.
    Merged,
    /// Any non-closed -> Closed.
    Close,
}

impl WorkstreamTransition {
    pub fn name(&self) -> &'static str {
        match self {
            WorkstreamTransition::CommitRefused => "commit_refused",
            WorkstreamTransition::Committed => "committed",
            WorkstreamTransition::Pushed => "pushed",
            WorkstreamTransition::PrOpened { .. } => "pr_opened",
            WorkstreamTransition::PrAdopted { .. } => "pr_adopted",
            WorkstreamTransition::Merged => "merged",
            WorkstreamTransition::Close => "close",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WorkstreamError {
    #[error("cannot {transition} a workstream that is {from}")]
    Illegal {
        from: &'static str,
        transition: &'static str,
    },
    #[error("the workstream is closed; nothing moves")]
    Terminal,
    #[error("cannot {transition} the primary workstream: it is the project itself — remove the project instead")]
    Primary { transition: &'static str },
    #[error("workstream {id} is a copy of a non-git project, not a git checkout")]
    NoRepository { id: String },
    #[error(
        "workstream {id} is the project's primary: it has no branch of its own to do that with"
    )]
    NoOwnBranch { id: String },
}

impl WorkstreamState {
    /// Whether the work has left this machine. Cleanup must never discard
    /// something only pushed locally.
    pub fn is_published(&self) -> bool {
        matches!(
            self,
            WorkstreamState::Pushed
                | WorkstreamState::PrOpen { .. }
                | WorkstreamState::Merged { .. }
        )
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, WorkstreamState::Closed)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            WorkstreamState::Open => "open",
            WorkstreamState::Dirty => "dirty",
            WorkstreamState::Committed => "committed",
            WorkstreamState::Pushed => "pushed",
            WorkstreamState::PrOpen { .. } => "pr_open",
            WorkstreamState::Merged { .. } => "merged",
            WorkstreamState::Closed => "closed",
        }
    }

    /// The one exhaustive match — the pure table, blind to the kind. Reach it
    /// through [`Workstream::apply`], which is what the store's one writer
    /// calls.
    pub fn apply(self, t: &WorkstreamTransition) -> Result<WorkstreamState, WorkstreamError> {
        use WorkstreamState::*;
        use WorkstreamTransition as T;
        if self.is_terminal() {
            return Err(WorkstreamError::Terminal);
        }
        let illegal = || WorkstreamError::Illegal {
            from: self.as_str(),
            transition: t.name(),
        };
        match (&self, t) {
            (Open | Dirty | Committed | Pushed, T::CommitRefused) => Ok(Dirty),
            (Open | Dirty | Committed | Pushed, T::Committed) => Ok(Committed),
            (Committed | Pushed, T::Pushed) => Ok(Pushed),
            (PrOpen { .. }, T::Pushed) => Ok(self.clone()),
            // Answering a review is a commit on a branch whose pull request is
            // open, and a branch that merged can still be committed on: the
            // commit is git's fact and has landed by the time this is asked,
            // and the record keeps the pull request it names — what is
            // unpushed is the live status's to say, never a state that would
            // forget the number.
            (PrOpen { .. } | Merged { .. }, T::Committed | T::CommitRefused) => Ok(self.clone()),
            (Pushed, T::PrOpened { number, url }) | (Open, T::PrAdopted { number, url }) => {
                Ok(PrOpen {
                    number: *number,
                    url: url.clone(),
                })
            }
            (PrOpen { number, url }, T::Merged) => Ok(Merged {
                number: *number,
                url: url.clone(),
            }),
            (_, T::Close) => Ok(Closed),
            _ => Err(illegal()),
        }
    }
}

/// Build a git branch name like `work/add-analytics-a3f9k2` from a kind and a
/// human-written slug. Sanitising rather than refusing is deliberate: the slug
/// comes from an instruction someone typed, and failing a whole run because
/// they wrote `cart total (v2)` would be absurd. Everything
/// `git check-ref-format` forbids is folded away.
pub fn branch_name_for(kind: &str, slug: &str) -> String {
    let kind = sanitize_ref_component(kind, "work");
    let slug = sanitize_ref_component(slug, "item");
    format!("{kind}/{slug}")
}

fn sanitize_ref_component(s: &str, fallback: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        let c = ch.to_ascii_lowercase();
        let keep = c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.';
        out.push(if keep { c } else { '-' });
    }
    while out.contains("..") {
        out = out.replace("..", ".");
    }
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    let trimmed = out.trim_matches(|c| c == '-' || c == '.' || c == '_');
    let mut out = trimmed.to_string();
    while let Some(rest) = out.strip_suffix(".lock") {
        out = rest.trim_end_matches(['-', '.', '_']).to_string();
    }
    if out.is_empty() {
        fallback.to_string()
    } else {
        out
    }
}

/// Is `name` something `git check-ref-format --branch` would accept?
pub fn is_valid_branch_name(name: &str) -> bool {
    if name.is_empty() || name.starts_with('/') || name.ends_with('/') || name.contains("//") {
        return false;
    }
    if name.contains("..") || name.contains("@{") || name == "@" {
        return false;
    }
    if name.ends_with('.') || name.ends_with(".lock") {
        return false;
    }
    if name.starts_with('-') || name.starts_with('.') {
        return false;
    }
    for component in name.split('/') {
        if component.is_empty() || component.starts_with('.') || component.ends_with(".lock") {
            return false;
        }
    }
    name.chars().all(|c| {
        !c.is_control()
            && !c.is_whitespace()
            && !matches!(c, '~' | '^' | ':' | '?' | '*' | '[' | '\\')
            && c.is_ascii()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One of every state, and one of every transition.
    fn every_state() -> Vec<WorkstreamState> {
        let pr = || (7u64, "https://code.example.test/pr/7".to_string());
        vec![
            WorkstreamState::Open,
            WorkstreamState::Dirty,
            WorkstreamState::Committed,
            WorkstreamState::Pushed,
            WorkstreamState::PrOpen {
                number: pr().0,
                url: pr().1,
            },
            WorkstreamState::Merged {
                number: pr().0,
                url: pr().1,
            },
            WorkstreamState::Closed,
        ]
    }

    fn every_transition() -> Vec<WorkstreamTransition> {
        let url = "https://code.example.test/pr/9".to_string();
        vec![
            WorkstreamTransition::CommitRefused,
            WorkstreamTransition::Committed,
            WorkstreamTransition::Pushed,
            WorkstreamTransition::PrOpened {
                number: 9,
                url: url.clone(),
            },
            WorkstreamTransition::PrAdopted { number: 9, url },
            WorkstreamTransition::Merged,
            WorkstreamTransition::Close,
        ]
    }

    /// The whole table, cell by cell: where each transition leads from each
    /// state, or that it is refused — and how.
    #[test]
    fn every_cell_of_the_lifecycle_is_a_state_or_a_refusal_in_words() {
        // (from, transition) → the state's name it leads to; absent is refused.
        let legal: &[(&str, &str, &str)] = &[
            ("open", "commit_refused", "dirty"),
            ("open", "committed", "committed"),
            ("open", "pr_adopted", "pr_open"),
            ("dirty", "commit_refused", "dirty"),
            ("dirty", "committed", "committed"),
            ("committed", "commit_refused", "dirty"),
            ("committed", "committed", "committed"),
            ("committed", "pushed", "pushed"),
            ("pushed", "commit_refused", "dirty"),
            ("pushed", "committed", "committed"),
            ("pushed", "pushed", "pushed"),
            ("pushed", "pr_opened", "pr_open"),
            ("pr_open", "commit_refused", "pr_open"),
            ("pr_open", "committed", "pr_open"),
            ("pr_open", "pushed", "pr_open"),
            ("pr_open", "merged", "merged"),
            ("merged", "commit_refused", "merged"),
            ("merged", "committed", "merged"),
        ];
        let mut cells = 0;
        for from in every_state() {
            for t in every_transition() {
                cells += 1;
                let (f, n) = (from.as_str(), t.name());
                let got = from.clone().apply(&t);
                if f == "closed" {
                    assert_eq!(got, Err(WorkstreamError::Terminal), "{f} · {n}");
                    continue;
                }
                if n == "close" {
                    assert_eq!(got, Ok(WorkstreamState::Closed), "{f} · {n}");
                    continue;
                }
                match legal.iter().find(|(lf, ln, _)| *lf == f && *ln == n) {
                    Some((_, _, to)) => {
                        let to_state = got.unwrap_or_else(|e| panic!("{f} · {n} is legal: {e}"));
                        assert_eq!(to_state.as_str(), *to, "{f} · {n}");
                    }
                    None => {
                        let refused = got.expect_err(&format!("{f} · {n} is refused"));
                        assert_eq!(
                            refused,
                            WorkstreamError::Illegal {
                                from: f,
                                transition: n
                            },
                            "{f} · {n}"
                        );
                        assert_eq!(
                            refused.to_string(),
                            format!("cannot {n} a workstream that is {f}")
                        );
                    }
                }
            }
        }
        assert_eq!(cells, 49, "seven states by seven transitions");
    }

    #[test]
    fn a_pull_request_keeps_its_number_through_every_move_that_does_not_end_it() {
        let open = WorkstreamState::PrOpen {
            number: 7,
            url: "u".into(),
        };
        for t in [
            WorkstreamTransition::Committed,
            WorkstreamTransition::CommitRefused,
            WorkstreamTransition::Pushed,
        ] {
            assert_eq!(open.clone().apply(&t), Ok(open.clone()), "{}", t.name());
        }
        assert_eq!(
            open.clone().apply(&WorkstreamTransition::Merged),
            Ok(WorkstreamState::Merged {
                number: 7,
                url: "u".into()
            }),
            "the merged record names the pull request it went in through"
        );
        // A second pull request is not opened over the first, nor one adopted.
        for t in [
            WorkstreamTransition::PrOpened {
                number: 9,
                url: "v".into(),
            },
            WorkstreamTransition::PrAdopted {
                number: 9,
                url: "v".into(),
            },
        ] {
            assert!(open.clone().apply(&t).is_err(), "{}", t.name());
        }
    }

    #[test]
    fn only_a_worktree_has_a_branch_of_its_own_and_a_copy_has_no_repository() {
        let project = ProjectId::from_ulid(ulid::Ulid::from_parts(1, 1));
        let mut w = Workstream {
            id: WorkstreamId::primary_of(project),
            project,
            name: None,
            note: None,
            pinned: false,
            kind: WorkstreamKind::Primary,
            goal: None,
            work_item: None,
            agent: None,
            state: WorkstreamState::Open,
            created_at: 0,
            board: Default::default(),
        };
        assert!(w.require_repository().is_ok());
        assert!(matches!(
            w.require_own_branch(),
            Err(WorkstreamError::NoOwnBranch { .. })
        ));
        w.kind = WorkstreamKind::Worktree {
            branch: "work/x".into(),
            base: "main".into(),
        };
        assert_eq!(w.require_own_branch(), Ok(("work/x", "main")));
        w.kind = WorkstreamKind::Copy;
        assert!(matches!(
            w.require_repository(),
            Err(WorkstreamError::NoRepository { .. })
        ));
        assert!(matches!(
            w.require_own_branch(),
            Err(WorkstreamError::NoRepository { .. })
        ));
    }

    #[test]
    fn readable_names_survive_intact() {
        assert_eq!(
            branch_name_for("feature", "add-analytics"),
            "feature/add-analytics"
        );
        assert_eq!(
            branch_name_for("Fix", "Cart Total (v2)"),
            "fix/cart-total-v2"
        );
    }

    #[test]
    fn everything_git_forbids_is_folded_away() {
        for (kind, slug, want) in [
            ("feature", "a..b", "feature/a.b"),
            ("feature", "x.lock.lock", "feature/x"),
            ("feature", "~tilde", "feature/tilde"),
            ("feature", "-leading-dash", "feature/leading-dash"),
            ("feature", "at@{ref}", "feature/at-ref"),
            ("feature", "café", "feature/caf"),
        ] {
            assert_eq!(branch_name_for(kind, slug), want);
        }
    }

    #[test]
    fn output_is_always_a_legal_ref() {
        let long = "a".repeat(300);
        let nasty = [
            "",
            ".",
            "..",
            "-",
            "@",
            "@{",
            ".lock",
            "/",
            "//",
            "\u{0}",
            "\t",
            " ",
            "~^:?*[\\",
            "日本語",
            "\u{202e}",
            long.as_str(),
        ];
        for kind in nasty {
            for slug in nasty {
                let b = branch_name_for(kind, slug);
                assert!(is_valid_branch_name(&b), "{kind:?} {slug:?} -> {b:?}");
            }
        }
        assert_eq!(branch_name_for("", ""), "work/item");
    }

    #[test]
    fn the_lifecycle_walks_and_publish_is_sticky() {
        use WorkstreamState::*;
        use WorkstreamTransition as T;
        let s = Open.apply(&T::CommitRefused).unwrap();
        assert_eq!(s, Dirty);
        let s = s.apply(&T::Committed).unwrap();
        let s = s.apply(&T::Pushed).unwrap();
        assert!(s.is_published());
        let s = s
            .apply(&T::PrOpened {
                number: 12,
                url: "https://example.invalid/pr/12".into(),
            })
            .unwrap();
        // More commits pushed to the PR branch keep the PR open.
        let s = s.apply(&T::Pushed).unwrap();
        assert!(matches!(s, PrOpen { number: 12, .. }));
        // A merge keeps the pull request it went in through: how the work
        // landed is a fact the record does not throw away.
        let s = s.apply(&T::Merged).unwrap();
        assert_eq!(
            s,
            Merged {
                number: 12,
                url: "https://example.invalid/pr/12".into()
            }
        );
        let s = s.apply(&T::Close).unwrap();
        assert_eq!(s.apply(&T::Committed), Err(WorkstreamError::Terminal));
    }

    #[test]
    fn illegal_moves_are_refused_by_name() {
        use WorkstreamState::*;
        use WorkstreamTransition as T;
        assert_eq!(
            Open.apply(&T::Pushed),
            Err(WorkstreamError::Illegal {
                from: "open",
                transition: "pushed"
            })
        );
        assert!(Open.apply(&T::Merged).is_err());
        assert!(Committed
            .apply(&T::PrOpened {
                number: 1,
                url: "u".into()
            })
            .is_err());
    }

    #[test]
    fn a_workstream_opened_from_a_pull_request_is_born_linked_and_merges_through_it() {
        use WorkstreamState::*;
        use WorkstreamTransition as T;
        let adopted = T::PrAdopted {
            number: 7,
            url: "https://example.invalid/pr/7".into(),
        };
        // The one edge into PrOpen that skips the push: the branch was
        // already on the code host before the record existed.
        let s = Open.apply(&adopted).unwrap();
        assert_eq!(
            s,
            PrOpen {
                number: 7,
                url: "https://example.invalid/pr/7".into()
            }
        );
        // From there the lifecycle is the ordinary one.
        let s = s.apply(&T::Pushed).unwrap();
        assert!(matches!(s, PrOpen { number: 7, .. }));
        assert_eq!(
            s.apply(&T::Merged).unwrap(),
            Merged {
                number: 7,
                url: "https://example.invalid/pr/7".into()
            }
        );
        // Adoption is a birth fact: a workstream that has committed or pushed
        // on its own links a pull request through PrOpened, never this way.
        for from in [Dirty, Committed, Pushed] {
            assert_eq!(
                from.clone().apply(&adopted),
                Err(WorkstreamError::Illegal {
                    from: from.as_str(),
                    transition: "pr_adopted"
                })
            );
        }
        assert_eq!(adopted.name(), "pr_adopted");
    }

    #[test]
    fn a_source_round_trips_with_its_word_and_defaults_to_a_derived_branch() {
        use WorkstreamSource as S;
        let cases: Vec<(S, &str)> = vec![
            (
                S::default(),
                r#"{"source":"new_branch","name":null,"start":null}"#,
            ),
            (
                S::NewBranch {
                    name: Some("feature/x".into()),
                    start: Some("v1".into()),
                },
                r#"{"source":"new_branch","name":"feature/x","start":"v1"}"#,
            ),
            (
                S::LocalBranch {
                    name: "topic".into(),
                },
                r#"{"source":"local_branch","name":"topic"}"#,
            ),
            (
                S::RemoteBranch {
                    remote: "origin".into(),
                    name: "topic".into(),
                },
                r#"{"source":"remote_branch","remote":"origin","name":"topic"}"#,
            ),
            (
                S::Tag {
                    name: "v1".into(),
                    branch: None,
                    create_at: Some("main".into()),
                },
                r#"{"source":"tag","name":"v1","branch":null,"create_at":"main"}"#,
            ),
            (
                S::PullRequest { number: 12 },
                r#"{"source":"pull_request","number":12}"#,
            ),
        ];
        for (source, json) in cases {
            assert_eq!(serde_json::to_string(&source).unwrap(), json);
            assert_eq!(serde_json::from_str::<S>(json).unwrap(), source);
        }
        // The optional fields may be left out on the wire.
        assert_eq!(
            serde_json::from_str::<S>(r#"{"source":"tag","name":"v1"}"#).unwrap(),
            S::Tag {
                name: "v1".into(),
                branch: None,
                create_at: None
            }
        );
        assert_eq!(
            serde_json::from_str::<S>(r#"{"source":"new_branch"}"#).unwrap(),
            S::default()
        );
        assert_eq!(S::default().as_str(), "new_branch");
    }

    #[test]
    fn a_source_names_the_branch_it_knows_and_describes_itself_in_a_line() {
        use WorkstreamSource as S;
        assert_eq!(S::default().branch_hint(), None);
        assert_eq!(
            S::NewBranch {
                name: Some("feature/x".into()),
                start: None
            }
            .branch_hint(),
            Some("feature/x")
        );
        assert_eq!(
            S::LocalBranch {
                name: "topic".into()
            }
            .branch_hint(),
            Some("topic")
        );
        assert_eq!(
            S::RemoteBranch {
                remote: "origin".into(),
                name: "topic".into()
            }
            .branch_hint(),
            Some("topic")
        );
        assert_eq!(
            S::Tag {
                name: "v1".into(),
                branch: Some("release/1".into()),
                create_at: None
            }
            .branch_hint(),
            Some("release/1")
        );
        assert_eq!(S::PullRequest { number: 3 }.branch_hint(), None);

        assert_eq!(S::default().describe(), "a new branch");
        assert_eq!(
            S::NewBranch {
                name: Some("x".into()),
                start: Some("v1".into())
            }
            .describe(),
            "a new branch x at v1"
        );
        assert_eq!(
            S::LocalBranch {
                name: "topic".into()
            }
            .describe(),
            "the branch topic"
        );
        assert_eq!(
            S::RemoteBranch {
                remote: "origin".into(),
                name: "topic".into()
            }
            .describe(),
            "the branch origin/topic"
        );
        assert_eq!(
            S::Tag {
                name: "v1".into(),
                branch: None,
                create_at: None
            }
            .describe(),
            "the tag v1"
        );
        assert_eq!(
            S::Tag {
                name: "v1".into(),
                branch: None,
                create_at: Some("main".into())
            }
            .describe(),
            "a new tag v1 at main"
        );
        assert_eq!(S::PullRequest { number: 12 }.describe(), "pull request #12");
    }

    #[test]
    fn a_source_spec_reads_every_spelling_and_refuses_the_rest_by_name() {
        use WorkstreamSource as S;
        let parse = |s: &str| s.parse::<S>();
        assert_eq!(
            parse("feature/x").unwrap(),
            S::NewBranch {
                name: Some("feature/x".into()),
                start: None
            }
        );
        assert_eq!(
            parse("new:feature/x@v1").unwrap(),
            S::NewBranch {
                name: Some("feature/x".into()),
                start: Some("v1".into())
            }
        );
        assert_eq!(
            parse("branch:topic").unwrap(),
            S::LocalBranch {
                name: "topic".into()
            }
        );
        assert_eq!(
            parse("remote:origin/feature/x").unwrap(),
            S::RemoteBranch {
                remote: "origin".into(),
                name: "feature/x".into()
            }
        );
        assert_eq!(
            parse("tag:v1").unwrap(),
            S::Tag {
                name: "v1".into(),
                branch: None,
                create_at: None
            }
        );
        assert_eq!(
            parse("new-tag:v1@main").unwrap(),
            S::Tag {
                name: "v1".into(),
                branch: None,
                create_at: Some("main".into())
            }
        );
        assert_eq!(parse("pr:12").unwrap(), S::PullRequest { number: 12 });
        assert_eq!(parse("pr:#12").unwrap(), S::PullRequest { number: 12 });

        for (bad, says) in [
            ("", "a source is needed"),
            ("branch:", "a name is needed"),
            ("remote:origin", "remote:<remote>/<branch>"),
            ("new-tag:v1", "new-tag:<name>@<ref>"),
            ("pr:twelve", "pr:<number>"),
            ("wat:x", "wat is not a source"),
        ] {
            let err = parse(bad).unwrap_err().to_string();
            assert!(err.contains(says), "{bad:?} -> {err}");
        }
    }

    fn worktree(project: ProjectId) -> Workstream {
        Workstream {
            id: WorkstreamId::from_ulid(ulid::Ulid::from_parts(4, 1)),
            project,
            name: None,
            note: None,
            pinned: false,
            kind: WorkstreamKind::Worktree {
                branch: "feature/checkout".into(),
                base: "main".into(),
            },
            goal: None,
            work_item: None,
            agent: None,
            state: WorkstreamState::Open,
            created_at: 9,
            board: Default::default(),
        }
    }

    fn primary(project: ProjectId) -> Workstream {
        Workstream {
            id: WorkstreamId::primary_of(project),
            kind: WorkstreamKind::Primary,
            ..worktree(project)
        }
    }

    #[test]
    fn workstream_json_roundtrip_with_and_without_a_goal() {
        let w = worktree(ProjectId::from_ulid(ulid::Ulid::from_parts(3, 1)));
        let json = serde_json::to_value(&w).unwrap();
        assert!(json.get("goal").is_none(), "an IDE branch names no goal");
        assert!(json.get("path").is_none(), "no path is ever stored");
        assert!(json.get("name").is_none() && json.get("pinned").is_none());
        assert_eq!(serde_json::from_value::<Workstream>(json).unwrap(), w);
        assert_eq!(w.branch(), Some("feature/checkout"));
        assert_eq!(w.base(), Some("main"));

        let with_goal = Workstream {
            goal: Some(GoalId::from_ulid(ulid::Ulid::from_parts(1, 1))),
            work_item: Some(WorkItemId::from_ulid(ulid::Ulid::from_parts(2, 1))),
            ..w
        };
        let json = serde_json::to_string(&with_goal).unwrap();
        assert_eq!(
            serde_json::from_str::<Workstream>(&json).unwrap(),
            with_goal
        );
    }

    #[test]
    fn the_primary_id_is_its_projects_ulid() {
        let project = ProjectId::from_ulid(ulid::Ulid::from_parts(3, 1));
        let p = primary(project);
        assert_eq!(p.id.to_string(), project.to_string());
        assert!(p.is_primary());
        assert_eq!(p.branch(), None, "the primary's branch is read live");
        assert_eq!(p.base(), None);
    }

    #[test]
    fn a_primary_never_closes_or_finishes_a_branch() {
        use WorkstreamTransition as T;
        let project = ProjectId::from_ulid(ulid::Ulid::from_parts(3, 1));
        let p = primary(project);
        for t in [
            T::Close,
            T::PrOpened {
                number: 1,
                url: "u".into(),
            },
            T::PrAdopted {
                number: 1,
                url: "u".into(),
            },
            T::Merged,
        ] {
            assert_eq!(
                p.apply(&t),
                Err(WorkstreamError::Primary {
                    transition: t.name()
                }),
                "{}",
                t.name()
            );
        }
        // A commit on the root checkout is ordinary work.
        assert_eq!(p.apply(&T::CommitRefused), Ok(WorkstreamState::Dirty));
        let committed = Workstream {
            state: p.apply(&T::Committed).unwrap(),
            ..p.clone()
        };
        assert_eq!(committed.apply(&T::Pushed), Ok(WorkstreamState::Pushed));
        // A worktree still closes as before.
        assert_eq!(
            worktree(project).apply(&T::Close),
            Ok(WorkstreamState::Closed)
        );
    }

    #[test]
    fn kind_tags_are_stable() {
        let project = ProjectId::from_ulid(ulid::Ulid::from_parts(3, 1));
        let tag = |w: &Workstream| {
            serde_json::to_value(w).unwrap()["kind"]["kind"]
                .as_str()
                .unwrap()
                .to_string()
        };
        assert_eq!(tag(&primary(project)), "primary");
        assert_eq!(tag(&worktree(project)), "worktree");
        let copy = Workstream {
            kind: WorkstreamKind::Copy,
            ..worktree(project)
        };
        assert_eq!(tag(&copy), "copy");
        assert_eq!(WorkstreamKind::Copy.as_str(), "copy");
    }

    #[test]
    fn label_prefers_name_then_branch_then_primary() {
        let project = ProjectId::from_ulid(ulid::Ulid::from_parts(3, 1));
        let w = worktree(project);
        assert_eq!(w.label(None), "feature/checkout");
        let named = Workstream {
            name: Some("  Checkout rewrite ".into()),
            ..w.clone()
        };
        assert_eq!(named.label(None), "Checkout rewrite");
        let blank = Workstream {
            name: Some("   ".into()),
            ..w
        };
        assert_eq!(
            blank.label(None),
            "feature/checkout",
            "a blank name is no name"
        );
        let p = primary(project);
        assert_eq!(p.label(Some("main")), "main");
        assert_eq!(p.label(None), "primary");
        let copy = Workstream {
            kind: WorkstreamKind::Copy,
            ..worktree(project)
        };
        assert_eq!(copy.label(None), copy.id.to_string());
    }

    // added by the coverage pass: workstream.rs

    #[test]
    fn every_source_has_its_wire_word_and_a_new_branch_at_a_start_describes_itself() {
        let cases = [
            (
                WorkstreamSource::NewBranch {
                    name: None,
                    start: None,
                },
                "new_branch",
            ),
            (
                WorkstreamSource::LocalBranch {
                    name: "main".into(),
                },
                "local_branch",
            ),
            (
                WorkstreamSource::RemoteBranch {
                    remote: "origin".into(),
                    name: "main".into(),
                },
                "remote_branch",
            ),
            (
                WorkstreamSource::Tag {
                    name: "v1".into(),
                    branch: None,
                    create_at: None,
                },
                "tag",
            ),
            (WorkstreamSource::PullRequest { number: 7 }, "pull_request"),
        ];
        for (source, word) in &cases {
            assert_eq!(source.as_str(), *word);
        }
        assert_eq!(
            WorkstreamSource::NewBranch {
                name: None,
                start: Some("v1".into()),
            }
            .describe(),
            "a new branch at v1"
        );
    }

    #[test]
    fn a_branch_name_is_refused_for_each_of_gits_reasons() {
        for bad in [
            "", "/x", "x/", "a//b", "a..b", "a@{b", "@", "x.", "x.lock", "-x", ".x", "a/.b",
            "a/b.lock", "a b", "a~b",
        ] {
            assert!(!is_valid_branch_name(bad), "{bad:?}");
        }
        assert!(is_valid_branch_name("feature/login-2"));
    }

    #[test]
    fn a_workstream_is_published_once_its_work_has_left_this_machine() {
        assert!(WorkstreamState::Pushed.is_published());
        assert!(WorkstreamState::PrOpen {
            number: 1,
            url: "u".into()
        }
        .is_published());
        assert!(WorkstreamState::Merged {
            number: 1,
            url: "u".into()
        }
        .is_published());
        assert!(!WorkstreamState::Open.is_published());
        assert!(!WorkstreamState::Committed.is_published());
        assert!(!WorkstreamState::Closed.is_published());
    }

    // added by the coverage pass: b5-workstream.rs
    #[test]
    fn a_new_branch_spec_without_a_start_describes_itself_and_the_primary_says_its_word() {
        let source: WorkstreamSource = "new:feature".parse().unwrap();
        assert_eq!(
            source,
            WorkstreamSource::NewBranch {
                name: Some("feature".into()),
                start: None,
            }
        );
        assert_eq!(source.describe(), "a new branch feature");
        assert_eq!(WorkstreamKind::Primary.as_str(), "primary");
    }

    // added by the coverage pass: b7-workstream.rs
    #[test]
    fn a_worktree_says_its_word() {
        assert_eq!(
            WorkstreamKind::Worktree {
                branch: "feature".into(),
                base: "main".into(),
            }
            .as_str(),
            "worktree"
        );
    }
}
