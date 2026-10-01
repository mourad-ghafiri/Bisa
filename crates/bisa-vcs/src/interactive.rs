//! The consented tier (ide/04): every verb that can move the
//! working tree, and the one type that lets it run.
//!
//! Three rules hold this module together, and each is a build-failing test in
//! `tests/it/interactive.rs` and the core layering tests:
//!
//! 1. **Every function takes `&HumanConsent`.** The type has no public
//!    constructor: it is minted in exactly one place — the node, from an
//!    authenticated request — and the engine takes it by reference. The MCP
//!    intake has no field for one, so no agent can reach anything here.
//! 2. **Every function writes a recovery ref first.** `git stash create` saves
//!    the dirty index and tree as a commit object without touching either;
//!    `update-ref refs/bisa/safety/<unix>-<op>[.wip]` pins it (or HEAD,
//!    for a clean tree; or, as `.stash`, a stash entry about to be dropped).
//!    Only then does the operation run. The crate's
//!    invariant becomes *nothing in this tree can make a change unrecoverable*.
//! 3. **Never a plain force push.** `--force-with-lease` only, only where the engine
//!    has checked the branch is a workstream's, and behind the Publish gate.
//!
//! The safe tier's self-grep test is scoped to `git.rs`; this file is where
//! the verbs it bans are allowed to live, and this is the only such file.

use crate::exec::{describe, s};
use crate::git::{classify, shared, CommitId, Git, RecoveryKind, StashEntry, RECOVERY_PREFIX};
use crate::{VcsError, VcsResult};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Proof that a person, at the keyboard, asked for a tree-moving operation.
///
/// No `Default`, no `Clone`, no `Deserialize`, no public field. The one
/// constructor is [`HumanConsent::mint`], which only
/// `bisa_node::ide::consent` may call — a workspace-wide test asserts
/// the call site — after checking the request's bearer token.
pub struct HumanConsent(());

impl HumanConsent {
    /// The single constructor. **Do not call this** outside the node's
    /// consent module; the layering test fails the build if you do.
    #[doc(hidden)]
    pub fn mint() -> Self {
        Self(())
    }
}

impl std::fmt::Debug for HumanConsent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HumanConsent")
    }
}

/// What an operation saved before it ran (ide/04).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Recovery {
    pub ref_name: String,
    pub commit: CommitId,
    /// The tree was clean: `commit` is HEAD, and restoring is a checkout.
    pub was_clean: bool,
    /// The branch HEAD was on, when it was on one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn op_name(op: &str) -> VcsResult<()> {
    if op.is_empty()
        || !op
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(VcsError::InvalidArg {
            what: "recovery op".into(),
            value: op.into(),
        });
    }
    Ok(())
}

/// What a verb asks `capture` to save besides the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pin<'a> {
    /// Nothing but the tree — HEAD when it is clean.
    None,
    /// A commit about to become unreachable — a branch or tag tip being
    /// deleted or renamed. Saved as the recovery itself when the tree is
    /// clean, and beside the tree (`_pin`) when it is dirty.
    Commit(&'a str),
    /// A stash entry about to be dropped or popped: saved as a `.stash` ref,
    /// which `restore` puts back on the stash list rather than checking out.
    Stash(&'a str),
    /// HEAD as it stands, and nothing of the tree: an amend folds the index
    /// into the commit it rewrites and leaves the working tree alone, so
    /// nothing there is lost — the old commit is the whole recovery.
    Head,
}

impl<'a> Pin<'a> {
    fn commit(self) -> Option<&'a str> {
        match self {
            Pin::None => None,
            Pin::Head => Some("HEAD"),
            Pin::Commit(c) | Pin::Stash(c) => Some(c),
        }
    }
}

/// The first free name for `<at>-<op>[_pin]<suffix>`: a second capture in the
/// same second for the same op must not overwrite the first.
fn free_ref_name(git: &Git, path: &Path, at: u64, op: &str, pin: bool, suffix: &str) -> String {
    let pin = if pin { "_pin" } else { "" };
    let mut n = 0;
    let mut name = format!("{RECOVERY_PREFIX}{at}-{op}{pin}{suffix}");
    while git
        .capture(
            Some(path),
            &[s("show-ref"), s("--verify"), s("-q"), s(&name)],
            false,
        )
        .is_ok_and(|o| o.success)
    {
        n += 1;
        name = format!("{RECOVERY_PREFIX}{at}-{op}_{n}{pin}{suffix}");
    }
    name
}

/// Write the recovery point. `pin` names a commit to save besides the tree —
/// a branch or tag tip about to be deleted, a stash about to be dropped — and
/// the tree is still saved when it is dirty.
///
/// Called first by every function below; `tests/it/interactive.rs` checks the
/// order in the source.
pub fn capture(git: &Git, path: &Path, op: &str, pin: Pin<'_>) -> VcsResult<Recovery> {
    op_name(op)?;
    let branch = git
        .capture(
            Some(path),
            &[s("symbolic-ref"), s("--short"), s("-q"), s("HEAD")],
            false,
        )
        .ok()
        .filter(|o| o.success)
        .map(|o| o.stdout_text())
        .filter(|b| !b.is_empty());
    // `stash create` writes a commit object holding the index and the tree
    // and returns its id; it does not touch either and does not push onto
    // the stash list. A clean tree prints nothing — and, depending on the git
    // version, exits 1 while saying nothing. An unmerged index cannot be
    // saved at all: then what was there is HEAD plus an in-progress operation
    // the caller is about to abort, and the ref pins HEAD as dirty.
    // `Pin::Head` saves no tree: the verb leaves the working tree as it is
    // and rewrites the commit alone, so the commit is the recovery and the
    // stash is not made.
    let stash = if pin == Pin::Head {
        None
    } else {
        Some(git.write_capture(Some(path), &[s("stash"), s("create")], false)?)
    };
    let wip = match &stash {
        Some(out) if out.success => out.stdout_text(),
        _ => String::new(),
    };
    let unmerged = stash
        .as_ref()
        .is_some_and(|out| !out.success && !out.stderr_text().trim().is_empty());
    let (target, was_clean) = if !wip.is_empty() {
        (wip, false)
    } else if unmerged {
        (
            git.run_text(
                Some(path),
                &[s("rev-parse"), s("--verify"), s("HEAD")],
                false,
            )?,
            false,
        )
    } else if let Pin::Commit(pin) = pin {
        (
            git.run_text(
                Some(path),
                &[
                    s("rev-parse"),
                    s("--verify"),
                    s(format!("{pin}^{{commit}}")),
                ],
                false,
            )?,
            true,
        )
    } else {
        (
            git.run_text(
                Some(path),
                &[s("rev-parse"), s("--verify"), s("HEAD")],
                false,
            )?,
            true,
        )
    };
    let at = now_secs();
    let suffix = if was_clean {
        ""
    } else {
        RecoveryKind::Tree.suffix()
    };
    let ref_name = free_ref_name(git, path, at, op, false, suffix);
    let message = format!(
        "op={op} branch={} pin={}",
        branch.as_deref().unwrap_or("-"),
        pin.commit().unwrap_or("-")
    );
    git.write(
        Some(path),
        &[
            s("update-ref"),
            s("--create-reflog"),
            s("-m"),
            s(&message),
            s(&ref_name),
            s(&target),
        ],
        false,
    )?;
    // What the pin names is saved in its own ref when the main ref could not
    // hold it: a tip beside a dirty tree (`_pin`), or a stash entry always
    // (`.stash`) — a stash is put back on the list, never checked out, so it
    // must never be the commit `restore` reads as the tree.
    let extra = match pin {
        Pin::Commit(pin) if !was_clean => Some((free_ref_name(git, path, at, op, true, ""), pin)),
        Pin::Stash(pin) => Some((
            free_ref_name(git, path, at, op, false, RecoveryKind::Stash.suffix()),
            pin,
        )),
        _ => None,
    };
    if let Some((name, pin)) = extra {
        let tip = git.run_text(
            Some(path),
            &[
                s("rev-parse"),
                s("--verify"),
                s(format!("{pin}^{{commit}}")),
            ],
            false,
        )?;
        git.write(
            Some(path),
            &[
                s("update-ref"),
                s("--create-reflog"),
                s("-m"),
                s(&message),
                s(name),
                s(tip),
            ],
            false,
        )?;
    }
    Ok(Recovery {
        ref_name,
        commit: CommitId::new(target),
        was_clean,
        branch,
    })
}

fn run(git: &Git, path: &Path, args: &[OsString]) -> VcsResult<()> {
    git.write(Some(path), args, false)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// The verbs. Each: validate → capture → run.
// ---------------------------------------------------------------------------

/// Move HEAD and the tree to `target` — a branch (attached) or a commit
/// (detached). Refuses to overwrite local changes: git's own check stands,
/// and a dirty tree that would be clobbered is [`VcsError::Dirty`].
pub fn checkout(
    git: &Git,
    path: &Path,
    target: &str,
    _consent: &HumanConsent,
) -> VcsResult<Recovery> {
    crate::git::validate_ref("target", target)?;
    let recovery = capture(git, path, "checkout", Pin::None)?;
    run(git, path, &[s("checkout"), s(target), s("--")])?;
    Ok(recovery)
}

/// Delete a local branch. The tip is saved in the recovery ref, so a branch
/// deleted here can always be recreated from *Safety*.
pub fn branch_delete(
    git: &Git,
    path: &Path,
    name: &str,
    _consent: &HumanConsent,
) -> VcsResult<Recovery> {
    crate::git::validate_ref("branch", name)?;
    let recovery = capture(git, path, "branch_delete", Pin::Commit(name))?;
    run(git, path, &[s("branch"), s("-D"), s("--"), s(name)])?;
    Ok(recovery)
}

/// Rename a local branch. The old tip is pinned in the recovery ref, so the
/// old name can always be recreated from *Safety*. `git branch -m` refuses an
/// existing target on its own; that refusal comes back as git's sentence.
pub fn branch_rename(
    git: &Git,
    path: &Path,
    from: &str,
    to: &str,
    _consent: &HumanConsent,
) -> VcsResult<Recovery> {
    crate::git::validate_ref("branch", from)?;
    crate::git::validate_ref("branch", to)?;
    let recovery = capture(git, path, "branch_rename", Pin::Commit(from))?;
    run(git, path, &[s("branch"), s("-m"), s("--"), s(from), s(to)])?;
    Ok(recovery)
}

/// A conflict as the repository tells it: git's sentence from `classify`,
/// plus the paths left unmerged and the operation left in progress, read
/// from the checkout after the failure. Every other error passes through.
fn with_conflict_facts(git: &Git, path: &Path, err: VcsError) -> VcsError {
    match err {
        VcsError::Conflict { message, .. } => VcsError::Conflict {
            message,
            paths: git.conflicted_paths(path).unwrap_or_default(),
            in_progress: git.in_progress(path).ok().flatten(),
        },
        other => other,
    }
}

/// How a rebase is asked for: the current branch replayed onto `upstream` —
/// or, with `onto`, only the commits since `upstream` replayed onto `onto`
/// (git's three-point form) — with the dirty tree carried across when
/// `autostash` is asked.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RebaseRequest {
    pub upstream: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub onto: Option<String>,
    #[serde(default)]
    pub autostash: bool,
}

/// Rebase the current branch as `req` asks. A conflict leaves the rebase in
/// progress and is [`VcsError::Conflict`] naming the paths; [`continue_op`]
/// once they are settled, [`skip_op`] to leave the commit out, or [`abort`].
pub fn rebase(
    git: &Git,
    path: &Path,
    req: &RebaseRequest,
    _consent: &HumanConsent,
) -> VcsResult<Recovery> {
    crate::git::validate_ref("upstream", &req.upstream)?;
    if let Some(onto) = &req.onto {
        crate::git::validate_ref("onto", onto)?;
    }
    let recovery = capture(git, path, "rebase", Pin::None)?;
    let mut args = vec![s("rebase")];
    if req.autostash {
        args.push(s("--autostash"));
    }
    if let Some(onto) = &req.onto {
        args.push(s("--onto"));
        args.push(s(onto));
    }
    args.push(s(&req.upstream));
    run(git, path, &args).map_err(|e| with_conflict_facts(git, path, e))?;
    Ok(recovery)
}

/// How a merge lands: git's default (a fast-forward when it can, a merge
/// commit otherwise), always a merge commit, a fast-forward or a refusal,
/// or every change squashed into the index — staged and uncommitted, for
/// the person to commit as one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeMode {
    Ff,
    NoFf,
    FfOnly,
    Squash,
}

impl MergeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            MergeMode::Ff => "ff",
            MergeMode::NoFf => "no_ff",
            MergeMode::FfOnly => "ff_only",
            MergeMode::Squash => "squash",
        }
    }
}

/// Merge `source` into the current branch by `mode`, with `message` as the
/// merge commit's when there is one to make (`--no-edit` otherwise: git's
/// own words stand, no editor opens). A squash leaves the result staged.
pub fn merge(
    git: &Git,
    path: &Path,
    source: &str,
    mode: MergeMode,
    message: Option<&str>,
    _consent: &HumanConsent,
) -> VcsResult<Recovery> {
    crate::git::validate_ref("source", source)?;
    if message.is_some_and(|m| m.trim().is_empty()) {
        return Err(VcsError::InvalidArg {
            what: "merge message".into(),
            value: "(empty)".into(),
        });
    }
    let recovery = capture(git, path, "merge", Pin::None)?;
    let mut args = vec![s("merge"), s("--no-edit")];
    match mode {
        MergeMode::Ff => {}
        MergeMode::NoFf => args.push(s("--no-ff")),
        MergeMode::FfOnly => args.push(s("--ff-only")),
        MergeMode::Squash => args.push(s("--squash")),
    }
    if let Some(m) = message {
        args.push(s("-m"));
        args.push(s(m));
    }
    args.push(s(source));
    run(git, path, &args).map_err(|e| with_conflict_facts(git, path, e))?;
    Ok(recovery)
}

/// The commits to pick, oldest first, and how: `record_origin` adds the
/// *(cherry picked from …)* line (`-x`), `no_commit` leaves the change
/// staged for one commit of the person's, `mainline` names the parent a
/// merge commit is picked against (`-m`, 1-based).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PickRequest {
    pub commits: Vec<String>,
    #[serde(default)]
    pub record_origin: bool,
    #[serde(default)]
    pub no_commit: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mainline: Option<u32>,
}

/// The commits to undo, oldest first, and how — the same options a pick
/// takes but the origin line, which a revert writes on its own.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RevertRequest {
    pub commits: Vec<String>,
    #[serde(default)]
    pub no_commit: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mainline: Option<u32>,
}

/// Every commit named, validated, or the one refusal for none at all.
fn commit_list(what: &str, commits: &[String]) -> VcsResult<()> {
    if commits.is_empty() {
        return Err(VcsError::InvalidArg {
            what: what.into(),
            value: "(no commits)".into(),
        });
    }
    for c in commits {
        crate::git::validate_ref("commit", c)?;
    }
    Ok(())
}

/// Apply `req.commits` on top of the current branch as new commits, in the
/// order given. A conflict is typed with its paths and the cherry-pick left
/// in progress; [`continue_op`], [`skip_op`] and [`abort`] take it on.
pub fn cherry_pick(
    git: &Git,
    path: &Path,
    req: &PickRequest,
    _consent: &HumanConsent,
) -> VcsResult<Recovery> {
    commit_list("cherry-pick", &req.commits)?;
    let recovery = capture(git, path, "cherry_pick", Pin::None)?;
    let mut args = vec![s("cherry-pick")];
    if req.record_origin {
        args.push(s("-x"));
    }
    if req.no_commit {
        args.push(s("--no-commit"));
    }
    if let Some(m) = req.mainline {
        args.push(s("-m"));
        args.push(s(m.to_string()));
    }
    args.extend(req.commits.iter().map(s));
    run(git, path, &args).map_err(|e| with_conflict_facts(git, path, e))?;
    Ok(recovery)
}

/// Undo `req.commits` with new commits — the one way to take a change back
/// that leaves history whole, which is why `reset` is not in this crate and
/// this is. A conflict is typed like a cherry-pick's.
pub fn revert(
    git: &Git,
    path: &Path,
    req: &RevertRequest,
    _consent: &HumanConsent,
) -> VcsResult<Recovery> {
    commit_list("revert", &req.commits)?;
    let recovery = capture(git, path, "revert", Pin::None)?;
    let mut args = vec![s("revert"), s("--no-edit")];
    if req.no_commit {
        args.push(s("--no-commit"));
    }
    if let Some(m) = req.mainline {
        args.push(s("-m"));
        args.push(s(m.to_string()));
    }
    args.extend(req.commits.iter().map(s));
    run(git, path, &args).map_err(|e| with_conflict_facts(git, path, e))?;
    Ok(recovery)
}

// ---------------------------------------------------------------------------
// An interactive rebase without a terminal: the todo git would open in an
// editor is written by us.
// ---------------------------------------------------------------------------

/// What happens to one commit of an interactive rebase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RebaseAction {
    Pick,
    Reword,
    Squash,
    Fixup,
    Drop,
}

/// One line of the plan: the commit, what to do with it, and — for a reword,
/// or a squash whose message the person wrote — the message it ends up with.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RebaseStep {
    pub action: RebaseAction,
    pub commit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// An interactive rebase, planned in full before it runs: every commit the
/// current branch has since `upstream`, in the order they will be replayed
/// (oldest first), each with its action.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RebasePlan {
    pub upstream: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub onto: Option<String>,
    pub steps: Vec<RebaseStep>,
}

/// How many commits a plan may cover — a rebase of more is a job for a
/// terminal, not a dialog.
pub const REBASE_PLAN_CAP: usize = 200;

/// The command git runs as its sequence editor: the todo arrives in
/// `BISA_REBASE_TODO` and is written whole into the file git names —
/// the content never touches the shell string, so nothing in it can break
/// the quoting.
const SEQUENCE_EDITOR: &str = r#"sh -c 'printf "%s" "$BISA_REBASE_TODO" > "$1"' bisa-rebase"#;

/// The plan against the commits it may name, before anything is written:
/// every commit between `upstream` and HEAD exactly once and no other, at
/// least one kept, the first kept one a pick or a reword (a squash or a
/// fixup folds into the commit before it), a reword with its words.
fn validate_plan(plan: &RebasePlan, range: &[CommitId]) -> VcsResult<()> {
    let bad = |why: String| VcsError::InvalidArg {
        what: "rebase plan".into(),
        value: why,
    };
    if plan.steps.is_empty() {
        return Err(bad("no steps".into()));
    }
    if range.len() > REBASE_PLAN_CAP {
        return Err(bad(format!(
            "{} commits — more than a plan takes ({REBASE_PLAN_CAP})",
            range.len()
        )));
    }
    let mut seen: Vec<&str> = Vec::new();
    for step in &plan.steps {
        if !range.iter().any(|c| c.as_str() == step.commit) {
            return Err(bad(format!(
                "{} is not a commit between {} and HEAD",
                step.commit, plan.upstream
            )));
        }
        if seen.contains(&step.commit.as_str()) {
            return Err(bad(format!("{} is named twice", step.commit)));
        }
        seen.push(&step.commit);
        if step.action == RebaseAction::Reword
            && step.message.as_deref().is_none_or(|m| m.trim().is_empty())
        {
            return Err(bad(format!(
                "{} is reworded without a message",
                step.commit
            )));
        }
    }
    if let Some(missing) = range.iter().find(|c| !seen.contains(&c.as_str())) {
        return Err(bad(format!(
            "{} is between {} and HEAD but not in the plan",
            missing.as_str(),
            plan.upstream
        )));
    }
    let kept: Vec<&RebaseStep> = plan
        .steps
        .iter()
        .filter(|s| s.action != RebaseAction::Drop)
        .collect();
    let Some(first) = kept.first() else {
        return Err(bad(
            "every commit is dropped — nothing is left to rebase".into()
        ));
    };
    if matches!(first.action, RebaseAction::Squash | RebaseAction::Fixup) {
        return Err(bad(format!(
            "{} cannot be folded into a commit before it: it is the first kept",
            first.commit
        )));
    }
    Ok(())
}

/// `'…'` for a POSIX shell, with a `'` inside written as `'\''`.
fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// The todo git reads, one line per step in git's own grammar. A reword is a
/// pick followed by an `exec` that amends the message from its file; a
/// squash with words is a fixup followed by the same, so the person's
/// message is the one that lands; a squash without words is git's own,
/// whose combined message stands as it is.
fn todo_text(steps: &[RebaseStep], message_files: &[(usize, PathBuf)]) -> String {
    let mut out = String::new();
    for (i, step) in steps.iter().enumerate() {
        let file = message_files.iter().find(|(n, _)| *n == i).map(|(_, p)| p);
        let amend = |out: &mut String| {
            if let Some(p) = file {
                out.push_str(&format!(
                    "exec git commit --amend -F {}\n",
                    shell_quote(&p.display().to_string())
                ));
            }
        };
        match step.action {
            RebaseAction::Pick => out.push_str(&format!("pick {}\n", step.commit)),
            RebaseAction::Reword => {
                out.push_str(&format!("pick {}\n", step.commit));
                amend(&mut out);
            }
            RebaseAction::Squash => {
                if file.is_some() {
                    out.push_str(&format!("fixup {}\n", step.commit));
                    amend(&mut out);
                } else {
                    out.push_str(&format!("squash {}\n", step.commit));
                }
            }
            RebaseAction::Fixup => out.push_str(&format!("fixup {}\n", step.commit)),
            RebaseAction::Drop => out.push_str(&format!("drop {}\n", step.commit)),
        }
    }
    out
}

/// Run an interactive rebase as `plan` says, with no terminal anywhere:
/// the plan is checked against the commits between `plan.upstream` and HEAD,
/// the messages a reword or a squash carries are written under the git
/// directory (`bisa/rebase/<n>.msg`, where a `--continue` after a
/// conflict finds them without any environment), the todo is handed to git
/// through [`SEQUENCE_EDITOR`], and `GIT_EDITOR=true` keeps every editor
/// shut. A conflict on the way is a rebase in progress like any other.
pub fn rebase_plan(
    git: &Git,
    path: &Path,
    plan: &RebasePlan,
    _consent: &HumanConsent,
) -> VcsResult<Recovery> {
    crate::git::validate_ref("upstream", &plan.upstream)?;
    if let Some(onto) = &plan.onto {
        crate::git::validate_ref("onto", onto)?;
    }
    let range: Vec<CommitId> = git
        .commits_between(path, "HEAD", &plan.upstream, REBASE_PLAN_CAP + 1)?
        .into_iter()
        .map(|c| c.id)
        .collect();
    validate_plan(plan, &range)?;
    let git_dir = git.git_dir(path)?;
    let msg_dir = git_dir.join("bisa").join("rebase");
    std::fs::create_dir_all(&msg_dir)
        .map_err(|e| VcsError::other(format!("{}: {e}", msg_dir.display())))?;
    let mut files = Vec::new();
    for (i, step) in plan.steps.iter().enumerate() {
        let wanted = matches!(step.action, RebaseAction::Reword | RebaseAction::Squash);
        if let Some(m) = step
            .message
            .as_deref()
            .filter(|m| wanted && !m.trim().is_empty())
        {
            let file = msg_dir.join(format!("{i}.msg"));
            std::fs::write(&file, m)
                .map_err(|e| VcsError::other(format!("{}: {e}", file.display())))?;
            files.push((i, file));
        }
    }
    let todo = todo_text(&plan.steps, &files);
    let recovery = capture(git, path, "rebase_plan", Pin::None)?;
    // The same handle with the editors shut (`Clone::clone`: `git clone` is
    // a verb of the handle too).
    let git = &Clone::clone(git)
        .with_env("GIT_EDITOR", "true")
        .with_env("GIT_SEQUENCE_EDITOR", SEQUENCE_EDITOR)
        .with_env("BISA_REBASE_TODO", todo);
    let mut args = vec![s("rebase"), s("-i")];
    if let Some(onto) = &plan.onto {
        args.push(s("--onto"));
        args.push(s(onto));
    }
    args.push(s(&plan.upstream));
    run(git, path, &args).map_err(|e| with_conflict_facts(git, path, e))?;
    Ok(recovery)
}

/// How a pull moves the branch once the fetch has landed (ide/04: a pull is
/// a safe fetch followed by a consented merge or rebase).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PullMode {
    /// Move forward only; local commits make it [`VcsError::NotFastForward`].
    FfOnly,
    /// Replay local commits on top of the upstream.
    Rebase,
    /// A merge commit when both sides moved.
    Merge,
}

impl PullMode {
    pub const ALL: [PullMode; 3] = [PullMode::FfOnly, PullMode::Rebase, PullMode::Merge];

    pub fn as_str(self) -> &'static str {
        match self {
            PullMode::FfOnly => "ff_only",
            PullMode::Rebase => "rebase",
            PullMode::Merge => "merge",
        }
    }
}

/// What a pull did: where HEAD was, where it is, and whether it moved.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PullOutcome {
    pub mode: PullMode,
    /// The upstream the branch was pulled from, `origin/main`.
    pub upstream: String,
    pub from: CommitId,
    pub to: CommitId,
    pub moved: bool,
}

/// Fetch `remote`, then bring the current branch up to its upstream by `mode`.
///
/// Refuses before anything moves when an operation is already in progress
/// ([`VcsError::InProgress`]), when the branch has no upstream
/// ([`VcsError::NoRemote`]), or — in [`PullMode::FfOnly`] — when the branch
/// has commits of its own ([`VcsError::NotFastForward`] with the counts, so
/// the caller can offer a rebase or a merge instead). A conflict is typed
/// with its paths and the operation left in progress.
pub fn pull(
    git: &Git,
    path: &Path,
    remote: &str,
    mode: PullMode,
    _consent: &HumanConsent,
) -> VcsResult<(Recovery, PullOutcome)> {
    crate::git::validate_ref("remote", remote)?;
    if let Some(op) = git.in_progress(path)? {
        return Err(VcsError::InProgress(op));
    }
    let recovery = capture(git, path, "pull", Pin::None)?;
    git.fetch(path, remote)?;
    let upstream = git.upstream_of(path)?.ok_or_else(|| {
        VcsError::NoRemote(format!(
            "the branch in {} tracks no upstream; push it first",
            path.display()
        ))
    })?;
    let from = git.head(path)?;
    match mode {
        PullMode::FfOnly => {
            if !git.can_fast_forward(path, &upstream)? {
                let (ahead, behind) = git.ahead_behind(path, &upstream)?;
                return Err(VcsError::NotFastForward { ahead, behind });
            }
            run(git, path, &[s("merge"), s("--ff-only"), s(&upstream)])
                .map_err(|e| with_conflict_facts(git, path, e))?;
        }
        PullMode::Rebase => {
            run(git, path, &[s("rebase"), s(&upstream)])
                .map_err(|e| with_conflict_facts(git, path, e))?;
        }
        PullMode::Merge => {
            run(git, path, &[s("merge"), s("--no-edit"), s(&upstream)])
                .map_err(|e| with_conflict_facts(git, path, e))?;
        }
    }
    let to = git.head(path)?;
    let moved = to != from;
    Ok((
        recovery,
        PullOutcome {
            mode,
            upstream,
            from,
            to,
            moved,
        },
    ))
}

/// Which in-progress operation to abandon — or, from [`crate::git::Git::in_progress`],
/// which one git has left half-done.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InProgress {
    Rebase,
    Merge,
    CherryPick,
    Revert,
}

impl InProgress {
    pub fn as_str(self) -> &'static str {
        match self {
            InProgress::Rebase => "rebase",
            InProgress::Merge => "merge",
            InProgress::CherryPick => "cherry_pick",
            InProgress::Revert => "revert",
        }
    }
}

impl InProgress {
    /// git's own verb for the operation.
    fn verb(self) -> &'static str {
        match self {
            InProgress::Rebase => "rebase",
            InProgress::Merge => "merge",
            InProgress::CherryPick => "cherry-pick",
            InProgress::Revert => "revert",
        }
    }
}

/// Abandon a conflicted rebase, merge, cherry-pick or revert, returning the
/// tree to where it was when that operation started. It moves the tree, so
/// it is consented and recorded like the rest.
pub fn abort(
    git: &Git,
    path: &Path,
    what: InProgress,
    _consent: &HumanConsent,
) -> VcsResult<Recovery> {
    let recovery = capture(git, path, "abort", Pin::None)?;
    run(git, path, &[s(what.verb()), s("--abort")])?;
    Ok(recovery)
}

/// `what` must be the operation git has half-done here, by name.
fn in_progress_is(git: &Git, path: &Path, what: InProgress) -> VcsResult<()> {
    match git.in_progress(path)? {
        Some(op) if op == what => Ok(()),
        other => Err(VcsError::InvalidArg {
            what: "operation".into(),
            value: match other {
                Some(op) => format!("a {} is in progress, not a {}", op.as_str(), what.as_str()),
                None => format!("no {} is in progress", what.as_str()),
            },
        }),
    }
}

/// Go on with a rebase, merge, cherry-pick or revert once every conflicted
/// path is settled — `<verb> --continue` with `GIT_EDITOR=true`, so the
/// message git prepared stands and no editor opens (`merge --continue` takes
/// no other option; `rebase --continue` would open one even for an
/// unchanged message). Refused, before any ref is written, while unmerged
/// paths remain — [`VcsError::Conflict`] naming them — or when nothing of
/// that kind is half-done. The next commit that conflicts leaves the
/// operation in progress like the first did.
pub fn continue_op(
    git: &Git,
    path: &Path,
    what: InProgress,
    _consent: &HumanConsent,
) -> VcsResult<Recovery> {
    in_progress_is(git, path, what)?;
    let unmerged = git.conflicted_paths(path)?;
    if !unmerged.is_empty() {
        return Err(VcsError::Conflict {
            message: format!(
                "{} unmerged path{} remain — settle them first",
                unmerged.len(),
                if unmerged.len() == 1 { "" } else { "s" }
            ),
            paths: unmerged,
            in_progress: Some(what),
        });
    }
    let recovery = capture(git, path, "continue", Pin::None)?;
    let git = &Clone::clone(git).with_env("GIT_EDITOR", "true");
    run(git, path, &[s(what.verb()), s("--continue")])
        .map_err(|e| with_conflict_facts(git, path, e))?;
    Ok(recovery)
}

/// Leave out the commit a rebase, cherry-pick or revert stopped on and go
/// on with the rest — `<verb> --skip`. A merge has nothing to skip and is
/// refused by name.
pub fn skip_op(
    git: &Git,
    path: &Path,
    what: InProgress,
    _consent: &HumanConsent,
) -> VcsResult<Recovery> {
    if what == InProgress::Merge {
        return Err(VcsError::InvalidArg {
            what: "operation".into(),
            value: "a merge has nothing to skip — settle its files or abort it".into(),
        });
    }
    in_progress_is(git, path, what)?;
    let recovery = capture(git, path, "skip", Pin::None)?;
    let git = &Clone::clone(git).with_env("GIT_EDITOR", "true");
    run(git, path, &[s(what.verb()), s("--skip")])
        .map_err(|e| with_conflict_facts(git, path, e))?;
    Ok(recovery)
}

/// How a conflicted path is settled whole, by git: a side taken — git's
/// `ours` (the branch the operation runs on; during a rebase, the branch
/// rebased onto) or `theirs` — or the file removed, the answer to a side
/// that deleted it (`ConflictKind::DeletedByThem`, `DeletedByUs`), where
/// no side can be checked out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    Ours,
    Theirs,
    Delete,
}

impl From<crate::git::ConflictSide> for Resolution {
    fn from(side: crate::git::ConflictSide) -> Self {
        match side {
            crate::git::ConflictSide::Ours => Self::Ours,
            crate::git::ConflictSide::Theirs => Self::Theirs,
        }
    }
}

/// Settle one conflicted path whole — a side checked out and staged, or
/// the path removed from the tree and the index (`git rm`, git's own
/// answer to a *deleted by them*). For a file the block-by-block view
/// cannot edit, and for the kinds that are a choice, not a merge.
pub fn resolve(
    git: &Git,
    path: &Path,
    pathspec: &str,
    how: Resolution,
    _consent: &HumanConsent,
) -> VcsResult<Recovery> {
    let specs = crate::git::literal_pathspecs("pathspec", &[pathspec])?;
    let spec = &specs[0];
    let recovery = capture(git, path, "resolve", Pin::None)?;
    match how {
        Resolution::Ours | Resolution::Theirs => {
            let flag = if how == Resolution::Ours {
                "--ours"
            } else {
                "--theirs"
            };
            run(git, path, &[s("checkout"), s(flag), s("--"), s(spec)])?;
            run(git, path, &[s("add"), s("--"), s(spec)])?;
        }
        Resolution::Delete => {
            run(git, path, &[s("rm"), s("--quiet"), s("--"), s(spec)])?;
        }
    }
    Ok(recovery)
}

/// Throw away the working-tree change in one hunk (or the lines of one):
/// the unstaged patch, applied in reverse to the tree. The index is untouched.
pub fn discard_hunk(
    git: &Git,
    path: &Path,
    patch: &str,
    _consent: &HumanConsent,
) -> VcsResult<Recovery> {
    if patch.trim().is_empty() {
        return Err(VcsError::InvalidArg {
            what: "patch".into(),
            value: "(empty)".into(),
        });
    }
    let recovery = capture(git, path, "discard_hunk", Pin::None)?;
    let args = vec![s("apply"), s("--reverse"), s("--recount"), s("-")];
    git.write_input(Some(path), &args, patch.as_bytes().to_vec())?;
    Ok(recovery)
}

/// Throw away every working-tree change in `pathspecs`, restoring them from
/// the index. Untracked files are not touched — git has nothing to restore
/// them *to*, and deleting is a different verb this crate does not have.
///
/// A path is read the way staging reads one (`literal_pathspecs`): the same
/// rules, the same `:(top,literal)` form, so a name with a space that stages
/// discards too, and a `*` in a name is that name and not a glob.
pub fn discard_paths<S: AsRef<str>>(
    git: &Git,
    path: &Path,
    pathspecs: &[S],
    _consent: &HumanConsent,
) -> VcsResult<Recovery> {
    let raw: Vec<&str> = pathspecs.iter().map(AsRef::as_ref).collect();
    let specs = crate::git::literal_pathspecs("pathspec", &raw)?;
    if specs.is_empty() {
        return Err(VcsError::InvalidArg {
            what: "pathspec".into(),
            value: "(none)".into(),
        });
    }
    let recovery = capture(git, path, "discard_paths", Pin::None)?;
    // A conflicted path has no one index version to go back to; the side git
    // has whole is HEAD, and that is what discarding it means — the way out
    // of a stash conflict, which leaves nothing to abort. The partition reads
    // the path as given; the spec git runs is the wrapped one.
    let conflicted = git.conflicted_paths(path)?;
    let (mut unmerged, mut plain): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    for (p, spec) in raw.iter().zip(specs) {
        if conflicted.iter().any(|c| c == Path::new(p)) {
            unmerged.push(spec);
        } else {
            plain.push(spec);
        }
    }
    if !plain.is_empty() {
        let mut args = vec![s("checkout"), s("--")];
        args.extend(plain.into_iter().map(s));
        run(git, path, &args)?;
    }
    if !unmerged.is_empty() {
        let mut args = vec![s("checkout"), s("HEAD"), s("--")];
        args.extend(unmerged.into_iter().map(s));
        run(git, path, &args)?;
    }
    Ok(recovery)
}

/// Create a tag at `target` (HEAD when absent); annotated when `message` is
/// given. Creating a tag moves nothing, but deleting one does, and the two
/// belong together in the panel that shows them — so both are consented.
pub fn tag_create(
    git: &Git,
    path: &Path,
    name: &str,
    target: Option<&str>,
    message: Option<&str>,
    _consent: &HumanConsent,
) -> VcsResult<Recovery> {
    crate::git::validate_ref("tag", name)?;
    if let Some(t) = target {
        crate::git::validate_ref("target", t)?;
    }
    let recovery = capture(git, path, "tag_create", Pin::None)?;
    let mut args = vec![s("tag")];
    if let Some(m) = message {
        args.push(s("-a"));
        args.push(s("-m"));
        args.push(s(m));
    }
    args.push(s("--"));
    args.push(s(name));
    if let Some(t) = target {
        args.push(s(t));
    }
    run(git, path, &args)?;
    Ok(recovery)
}

/// Delete a tag; its target is saved in the recovery ref first.
pub fn tag_delete(
    git: &Git,
    path: &Path,
    name: &str,
    _consent: &HumanConsent,
) -> VcsResult<Recovery> {
    crate::git::validate_ref("tag", name)?;
    let recovery = capture(git, path, "tag_delete", Pin::Commit(name))?;
    run(git, path, &[s("tag"), s("-d"), s("--"), s(name)])?;
    Ok(recovery)
}

/// Remove a remote. Nothing in the tree moves, but the configuration a push
/// depends on does, and a person is the one to decide that.
pub fn remote_remove(
    git: &Git,
    path: &Path,
    name: &str,
    _consent: &HumanConsent,
) -> VcsResult<Recovery> {
    crate::git::validate_value("remote", name)?;
    let recovery = capture(git, path, "remote_remove", Pin::None)?;
    run(git, path, &[s("remote"), s("remove"), s("--"), s(name)])?;
    Ok(recovery)
}

/// Rewrite HEAD with what is staged and a new message — `commit --amend`,
/// consented: the old commit is pinned first (`Pin::Commit("HEAD")`), so
/// Safety's *Restore* brings it back. Refused before any ref is written when
/// the message is blank, an operation is half-done, there is no commit yet,
/// or nobody is set to commit here — the reads that decide are not runners.
/// `--no-verify` is never passed. The caller decides whether HEAD may be
/// rewritten at all (a detached HEAD, a branch already published); this
/// function only makes the rewrite recoverable.
pub fn amend(
    git: &Git,
    path: &Path,
    message: &str,
    _consent: &HumanConsent,
) -> VcsResult<(Recovery, CommitId)> {
    let message = message.trim();
    if message.is_empty() {
        return Err(VcsError::InvalidArg {
            what: "message".into(),
            value: String::new(),
        });
    }
    if let Some(op) = git.in_progress(path)? {
        return Err(VcsError::InProgress(op));
    }
    // No commit yet: `head` says so, before anything is saved.
    git.head(path)?;
    if git.identity(path)?.source == crate::git::IdentitySource::None {
        return Err(VcsError::IdentityUnset(format!(
            "nobody is set to commit in {}: user.name and user.email resolve to nothing",
            path.display()
        )));
    }
    let recovery = capture(git, path, "amend", Pin::Head)?;
    git.write(
        Some(path),
        &[s("commit"), s("--amend"), s("--quiet"), s("-m"), s(message)],
        false,
    )?;
    Ok((recovery, git.head(path)?))
}

/// Push `branch` to `remote` with `--force-with-lease` — the only forced
/// push this crate has. **The caller has checked** that `branch` is a
/// workstream's own branch and never a project's default branch, and has passed
/// the Publish gate; this function checks neither, because it cannot know.
pub fn push_with_lease(
    git: &Git,
    path: &Path,
    remote: &str,
    branch: &str,
    _consent: &HumanConsent,
) -> VcsResult<Recovery> {
    crate::git::validate_value("remote", remote)?;
    crate::git::validate_ref("branch", branch)?;
    let recovery = capture(git, path, "push_with_lease", Pin::None)?;
    let what = describe(
        std::ffi::OsStr::new("git"),
        &[s("push"), s("--force-with-lease"), s(remote), s(branch)],
    );
    let out = git.write_capture(
        Some(path),
        &[s("push"), s("--force-with-lease"), s(remote), s(branch)],
        true,
    )?;
    if !out.success {
        let stderr = out.stderr_text();
        if stderr.to_ascii_lowercase().contains("stale info") || stderr.contains("[rejected]") {
            return Err(VcsError::Conflict {
                message: format!(
                    "{remote}/{branch} moved since it was last fetched; fetch, look, and push again"
                ),
                paths: Vec::new(),
                in_progress: None,
            });
        }
        return Err(VcsError::other(format!("{what}: {stderr}")));
    }
    Ok(recovery)
}

/// Delete `branch` on `remote` — the one outward act here besides the
/// leased push, and gated by the caller the same way: **the caller has
/// checked** the branch is not the project's default and has passed the
/// Publish gate. The remote-tracking ref's tip is pinned first, so Safety
/// can recreate the branch; a branch never fetched has no tip here and is
/// refused before anything is asked of the remote.
pub fn push_delete(
    git: &Git,
    path: &Path,
    remote: &str,
    branch: &str,
    _consent: &HumanConsent,
) -> VcsResult<Recovery> {
    crate::git::validate_value("remote", remote)?;
    crate::git::validate_ref("branch", branch)?;
    let tracking = format!("{remote}/{branch}");
    let recovery = capture(git, path, "push_delete", Pin::Commit(&tracking))?;
    let args = [s("push"), s("--delete"), s(remote), s(branch)];
    let what = describe(std::ffi::OsStr::new("git"), &args);
    let out = git.write_capture(Some(path), &args, true)?;
    if !out.success {
        return Err(VcsError::other(format!("{what}: {}", out.stderr_text())));
    }
    Ok(recovery)
}

/// Apply a stash-shaped commit onto the tree: `--index` first, so the staged
/// half comes back staged. git refuses `--index` for two different reasons
/// and only one of them is retried: *conflicts in index. Try without --index*
/// is decided before anything is written, so the plain apply is the same act
/// with less asked of it; every other failure — a merge conflict among them —
/// has already changed the tree, and a second apply on top of it would only
/// say *cannot apply a stash in the middle of a merge*. That failure is
/// classified as it is and enriched with the conflicted paths.
fn apply_stash(git: &Git, path: &Path, rev: &str) -> VcsResult<()> {
    let args = [s("stash"), s("apply"), s("--index"), s(rev)];
    let out = git.write_capture(Some(path), &args, false)?;
    if out.success {
        return Ok(());
    }
    let what = describe(std::ffi::OsStr::new("git"), &args);
    if out
        .stderr_text()
        .to_ascii_lowercase()
        .contains("try without --index")
    {
        return run(git, path, &[s("stash"), s("apply"), s(rev)])
            .map_err(|e| with_conflict_facts(git, path, e));
    }
    Err(with_conflict_facts(
        git,
        path,
        classify(&what, Some(path), &out),
    ))
}

/// A stash carrying untracked files cannot land where one of them already
/// exists: git stops at the first collision before touching anything else,
/// and its sentence names no path. Read the paths first and refuse by name.
fn refuse_untracked_collisions(git: &Git, path: &Path, rev: &str) -> VcsResult<()> {
    let third = git.capture(
        Some(path),
        &[
            s("rev-parse"),
            s("-q"),
            s("--verify"),
            s(format!("{rev}^3")),
        ],
        false,
    )?;
    if !third.success {
        return Ok(());
    }
    let listed = git.run_text(
        Some(path),
        &[
            s("ls-tree"),
            s("-r"),
            s("--name-only"),
            s(third.stdout_text()),
            s("--"),
        ],
        false,
    )?;
    let taken: Vec<String> = listed
        .lines()
        .map(str::trim)
        .filter(|p| !p.is_empty() && path.join(p).exists())
        .map(str::to_string)
        .collect();
    if taken.is_empty() {
        return Ok(());
    }
    Err(VcsError::Dirty {
        path: path.to_path_buf(),
        details: format!(
            "the stash carries untracked files that already exist here: {}",
            taken.join(", ")
        ),
    })
}

/// Put back what a recovery ref saved (ide/04 *Restore what was here*).
///
/// A commit recovery: HEAD moves to its branch when that branch still points
/// at it, else detaches onto it. A tree recovery is a stash-shaped commit:
/// HEAD moves to its base the same way, then the saved index and tree are
/// applied on top. A stash recovery is an entry that was dropped or popped:
/// it goes back on the stash list with its own subject, and the tree is not
/// touched. Restoring itself is recorded first — you can undo an undo.
pub fn restore(
    git: &Git,
    path: &Path,
    ref_name: &str,
    _consent: &HumanConsent,
) -> VcsResult<Recovery> {
    if !ref_name.starts_with(RECOVERY_PREFIX) || ref_name.contains("..") {
        return Err(VcsError::InvalidArg {
            what: "recovery ref".into(),
            value: ref_name.into(),
        });
    }
    let kind = if ref_name.ends_with(RecoveryKind::Tree.suffix()) {
        RecoveryKind::Tree
    } else if ref_name.ends_with(RecoveryKind::Stash.suffix()) {
        RecoveryKind::Stash
    } else {
        RecoveryKind::Commit
    };
    let saved = git.run_text(
        Some(path),
        &[
            s("rev-parse"),
            s("--verify"),
            s(format!("{ref_name}^{{commit}}")),
        ],
        false,
    )?;
    if kind == RecoveryKind::Tree {
        refuse_untracked_collisions(git, path, &saved)?;
    }
    let recovery = capture(git, path, "restore", Pin::None)?;
    if kind == RecoveryKind::Stash {
        let subject = git.run_text(
            Some(path),
            &[s("log"), s("-1"), s("--format=%s"), s(&saved), s("--")],
            false,
        )?;
        run(
            git,
            path,
            &[s("stash"), s("store"), s("-m"), s(subject), s(&saved)],
        )?;
        return Ok(recovery);
    }
    // Unmerged paths — a stash apply that stopped on a conflict leaves them
    // with no operation to abort — would refuse the checkout below; the side
    // git has whole is HEAD, and the recovery is about to replace them anyway.
    let conflicted = git.conflicted_paths(path)?;
    if !conflicted.is_empty() {
        let mut args = vec![s("checkout"), s("HEAD"), s("--")];
        args.extend(conflicted.into_iter().map(|p| s(p.as_os_str())));
        run(git, path, &args)?;
    }
    let dirty = kind == RecoveryKind::Tree;
    let base = if dirty {
        git.run_text(
            Some(path),
            &[s("rev-parse"), s("--verify"), s(format!("{saved}^1"))],
            false,
        )?
    } else {
        saved.clone()
    };
    // The branch the ref was written on, from its reflog message.
    let branch = git
        .capture(
            Some(path),
            &[
                s("reflog"),
                s("show"),
                s("--format=%gs"),
                s("-1"),
                s(ref_name),
            ],
            false,
        )
        .ok()
        .filter(|o| o.success)
        .map(|o| o.stdout_text())
        .and_then(|m| {
            m.split_whitespace()
                .find_map(|kv| kv.strip_prefix("branch=").map(str::to_string))
        })
        .filter(|b| !b.is_empty() && b != "-");
    let branch_tip = branch.as_deref().and_then(|b| {
        git.run_text(
            Some(path),
            &[s("rev-parse"), s("--verify"), s(format!("refs/heads/{b}"))],
            false,
        )
        .ok()
    });
    match (branch, branch_tip) {
        (Some(b), Some(tip)) if tip == base => run(git, path, &[s("checkout"), s(b), s("--")])?,
        _ => run(git, path, &[s("checkout"), s("--detach"), s(base), s("--")])?,
    }
    if dirty {
        apply_stash(git, path, &saved)?;
    }
    Ok(recovery)
}

// ---------------------------------------------------------------------------
// Stash (ide/04 §Stash): the four verbs. The list and a stash's patch are the
// safe tier's reads (`Git::stash_list`, `Git::stash_diff`).
// ---------------------------------------------------------------------------

/// What to stash, and how.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StashPush {
    /// `On <branch>: <message>`; none is git's own `WIP on <branch>: …`.
    #[serde(default)]
    pub message: Option<String>,
    /// `-u`: untracked files go too — never ignored ones (that is `--all`,
    /// which this crate does not spell).
    #[serde(default)]
    pub include_untracked: bool,
    /// `--keep-index`: the staged half stays staged in the tree as well.
    #[serde(default)]
    pub keep_index: bool,
    /// Only these paths; empty means the whole tree.
    #[serde(default)]
    pub paths: Vec<String>,
}

/// One stash entry, named twice so nothing acts on the wrong one: the index
/// `git stash pop/drop` want, and the commit the caller saw at it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StashTarget {
    pub index: u32,
    pub commit: String,
}

impl StashTarget {
    fn selector(&self) -> String {
        format!("stash@{{{}}}", self.index)
    }
}

/// The list still holds `target.commit` at `target.index`; else the list
/// moved — here or in another worktree of the same repository — and the verb
/// is refused with what sits there now. A read, so it precedes `capture`.
fn verify_target(git: &Git, path: &Path, target: &StashTarget) -> VcsResult<()> {
    crate::git::validate_ref("commit", &target.commit)?;
    let entries = git.stash_list(path)?;
    let now = entries.get(target.index as usize).map(|e| e.commit.clone());
    if now.as_ref().is_some_and(|c| c.as_str() == target.commit) {
        return Ok(());
    }
    Err(VcsError::StashMoved {
        index: target.index,
        commit: target.commit.clone(),
        now,
    })
}

/// The tree can take a stash: no operation half done, no unmerged path.
fn refuse_busy_tree(git: &Git, path: &Path) -> VcsResult<()> {
    if let Some(op) = git.in_progress(path)? {
        return Err(VcsError::InProgress(op));
    }
    let conflicted = git.conflicted_paths(path)?;
    if !conflicted.is_empty() {
        return Err(VcsError::Dirty {
            path: path.to_path_buf(),
            details: format!(
                "unmerged paths to settle first: {}",
                conflicted
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
    }
    Ok(())
}

/// Park the working tree's changes as a stash entry (`git stash push`).
///
/// Refused before any recovery ref is written when there is nothing it would
/// save — a clean tree, only untracked files without `include_untracked`,
/// paths with no change, no commit yet to stash against
/// ([`VcsError::NothingToStash`]) — and when an operation is half done or a
/// path is unmerged. The tree is captured first like every verb: the recovery
/// ref and the stash hold the same content, and the invariant is not bent for
/// one verb. Answers the entry the list now starts with.
pub fn stash_push(
    git: &Git,
    path: &Path,
    what: &StashPush,
    _consent: &HumanConsent,
) -> VcsResult<(Recovery, StashEntry)> {
    let specs = crate::git::literal_pathspecs("pathspec", &what.paths)?;
    let message = what
        .message
        .as_deref()
        .map(str::trim)
        .filter(|m| !m.is_empty());
    if git.head(path).is_err() {
        return Err(VcsError::NothingToStash);
    }
    refuse_busy_tree(git, path)?;
    let files = git.status_files(path)?;
    let scoped: Vec<_> = files
        .iter()
        .filter(|f| {
            what.paths.is_empty()
                || what
                    .paths
                    .iter()
                    .any(|p| f.path == Path::new(p) || f.path.starts_with(p))
        })
        .collect();
    let tracked = scoped
        .iter()
        .any(|f| !f.untracked && (f.is_staged() || f.is_unstaged()));
    let untracked = what.include_untracked && scoped.iter().any(|f| f.untracked);
    if !tracked && !untracked {
        return Err(VcsError::NothingToStash);
    }
    let recovery = capture(git, path, "stash_push", Pin::None)?;
    let before = git.stash_list(path)?.first().map(|e| e.commit.clone());
    let mut args = vec![s("stash"), s("push")];
    if what.include_untracked {
        args.push(s("-u"));
    }
    if what.keep_index {
        args.push(s("--keep-index"));
    }
    if let Some(m) = message {
        args.push(s("-m"));
        args.push(s(m));
    }
    if !specs.is_empty() {
        args.push(s("--"));
        args.extend(specs.into_iter().map(s));
    }
    run(git, path, &args)?;
    let entries = git.stash_list(path)?;
    match entries.into_iter().next() {
        Some(entry) if Some(&entry.commit) != before.as_ref() => Ok((recovery, entry)),
        _ => Err(VcsError::NothingToStash),
    }
}

/// Apply a stash entry onto the tree and keep it on the list.
pub fn stash_apply(
    git: &Git,
    path: &Path,
    target: &StashTarget,
    _consent: &HumanConsent,
) -> VcsResult<Recovery> {
    verify_target(git, path, target)?;
    refuse_busy_tree(git, path)?;
    refuse_untracked_collisions(git, path, &target.commit)?;
    let recovery = capture(git, path, "stash_apply", Pin::None)?;
    apply_stash(git, path, &target.selector())?;
    Ok(recovery)
}

/// Apply a stash entry and drop it — **only when the apply succeeded**: a
/// conflict leaves the entry on the list, as git does. The entry is pinned as
/// a `.stash` recovery before anything moves, so a popped stash is in
/// *Safety* even when the apply went cleanly.
pub fn stash_pop(
    git: &Git,
    path: &Path,
    target: &StashTarget,
    _consent: &HumanConsent,
) -> VcsResult<Recovery> {
    verify_target(git, path, target)?;
    refuse_busy_tree(git, path)?;
    refuse_untracked_collisions(git, path, &target.commit)?;
    let recovery = capture(git, path, "stash_pop", Pin::Stash(&target.commit))?;
    apply_stash(git, path, &target.selector())?;
    // The apply took a moment; another worktree may have pushed meanwhile.
    verify_target(git, path, target)?;
    run(git, path, &[s("stash"), s("drop"), s(target.selector())])?;
    Ok(recovery)
}

/// Drop a stash entry. Its commit is pinned as a `.stash` recovery first, so
/// *Restore* puts it back on the list.
pub fn stash_drop(
    git: &Git,
    path: &Path,
    target: &StashTarget,
    _consent: &HumanConsent,
) -> VcsResult<Recovery> {
    verify_target(git, path, target)?;
    let recovery = capture(git, path, "stash_drop", Pin::Stash(&target.commit))?;
    run(git, path, &[s("stash"), s("drop"), s(target.selector())])?;
    Ok(recovery)
}

// ---------------------------------------------------------------------------
// Free functions over the shared client, mirroring `git.rs`.
// ---------------------------------------------------------------------------

/// Free-function form of every verb, over the shared client. Same names, in
/// a nested module so the two forms never collide.
pub mod ops {
    use super::*;

    /// See [`super::checkout`].
    pub fn checkout(path: &Path, target: &str, consent: &HumanConsent) -> VcsResult<Recovery> {
        super::checkout(shared(), path, target, consent)
    }
    /// See [`super::branch_delete`].
    pub fn branch_delete(path: &Path, name: &str, consent: &HumanConsent) -> VcsResult<Recovery> {
        super::branch_delete(shared(), path, name, consent)
    }
    /// See [`super::branch_rename`].
    pub fn branch_rename(
        path: &Path,
        from: &str,
        to: &str,
        consent: &HumanConsent,
    ) -> VcsResult<Recovery> {
        super::branch_rename(shared(), path, from, to, consent)
    }
    /// See [`super::rebase`].
    pub fn rebase(path: &Path, req: &RebaseRequest, consent: &HumanConsent) -> VcsResult<Recovery> {
        super::rebase(shared(), path, req, consent)
    }
    /// See [`super::rebase_plan`].
    pub fn rebase_plan(
        path: &Path,
        plan: &RebasePlan,
        consent: &HumanConsent,
    ) -> VcsResult<Recovery> {
        super::rebase_plan(shared(), path, plan, consent)
    }
    /// See [`super::merge`].
    pub fn merge(
        path: &Path,
        source: &str,
        mode: MergeMode,
        message: Option<&str>,
        consent: &HumanConsent,
    ) -> VcsResult<Recovery> {
        super::merge(shared(), path, source, mode, message, consent)
    }
    /// See [`super::cherry_pick`].
    pub fn cherry_pick(
        path: &Path,
        req: &PickRequest,
        consent: &HumanConsent,
    ) -> VcsResult<Recovery> {
        super::cherry_pick(shared(), path, req, consent)
    }
    /// See [`super::revert`].
    pub fn revert(path: &Path, req: &RevertRequest, consent: &HumanConsent) -> VcsResult<Recovery> {
        super::revert(shared(), path, req, consent)
    }
    /// See [`super::continue_op`].
    pub fn continue_op(
        path: &Path,
        what: InProgress,
        consent: &HumanConsent,
    ) -> VcsResult<Recovery> {
        super::continue_op(shared(), path, what, consent)
    }
    /// See [`super::skip_op`].
    pub fn skip_op(path: &Path, what: InProgress, consent: &HumanConsent) -> VcsResult<Recovery> {
        super::skip_op(shared(), path, what, consent)
    }
    /// See [`super::resolve`].
    pub fn resolve(
        path: &Path,
        pathspec: &str,
        how: super::Resolution,
        consent: &HumanConsent,
    ) -> VcsResult<Recovery> {
        super::resolve(shared(), path, pathspec, how, consent)
    }
    /// See [`super::push_delete`].
    pub fn push_delete(
        path: &Path,
        remote: &str,
        branch: &str,
        consent: &HumanConsent,
    ) -> VcsResult<Recovery> {
        super::push_delete(shared(), path, remote, branch, consent)
    }
    /// See [`super::pull`].
    pub fn pull(
        path: &Path,
        remote: &str,
        mode: PullMode,
        consent: &HumanConsent,
    ) -> VcsResult<(Recovery, PullOutcome)> {
        super::pull(shared(), path, remote, mode, consent)
    }
    /// See [`super::abort`].
    pub fn abort(path: &Path, what: InProgress, consent: &HumanConsent) -> VcsResult<Recovery> {
        super::abort(shared(), path, what, consent)
    }
    /// See [`super::discard_hunk`].
    pub fn discard_hunk(path: &Path, patch: &str, consent: &HumanConsent) -> VcsResult<Recovery> {
        super::discard_hunk(shared(), path, patch, consent)
    }
    /// See [`super::discard_paths`].
    pub fn discard_paths<S: AsRef<str>>(
        path: &Path,
        pathspecs: &[S],
        consent: &HumanConsent,
    ) -> VcsResult<Recovery> {
        super::discard_paths(shared(), path, pathspecs, consent)
    }
    /// See [`super::tag_create`].
    pub fn tag_create(
        path: &Path,
        name: &str,
        target: Option<&str>,
        message: Option<&str>,
        consent: &HumanConsent,
    ) -> VcsResult<Recovery> {
        super::tag_create(shared(), path, name, target, message, consent)
    }
    /// See [`super::tag_delete`].
    pub fn tag_delete(path: &Path, name: &str, consent: &HumanConsent) -> VcsResult<Recovery> {
        super::tag_delete(shared(), path, name, consent)
    }
    /// See [`super::remote_remove`].
    pub fn remote_remove(path: &Path, name: &str, consent: &HumanConsent) -> VcsResult<Recovery> {
        super::remote_remove(shared(), path, name, consent)
    }
    /// See [`super::amend`].
    pub fn amend(
        path: &Path,
        message: &str,
        consent: &HumanConsent,
    ) -> VcsResult<(Recovery, CommitId)> {
        super::amend(shared(), path, message, consent)
    }
    /// See [`super::push_with_lease`].
    pub fn push_with_lease(
        path: &Path,
        remote: &str,
        branch: &str,
        consent: &HumanConsent,
    ) -> VcsResult<Recovery> {
        super::push_with_lease(shared(), path, remote, branch, consent)
    }
    /// See [`super::restore`].
    pub fn restore(path: &Path, ref_name: &str, consent: &HumanConsent) -> VcsResult<Recovery> {
        super::restore(shared(), path, ref_name, consent)
    }
    /// See [`super::stash_push`].
    pub fn stash_push(
        path: &Path,
        what: &StashPush,
        consent: &HumanConsent,
    ) -> VcsResult<(Recovery, StashEntry)> {
        super::stash_push(shared(), path, what, consent)
    }
    /// See [`super::stash_apply`].
    pub fn stash_apply(
        path: &Path,
        target: &StashTarget,
        consent: &HumanConsent,
    ) -> VcsResult<Recovery> {
        super::stash_apply(shared(), path, target, consent)
    }
    /// See [`super::stash_pop`].
    pub fn stash_pop(
        path: &Path,
        target: &StashTarget,
        consent: &HumanConsent,
    ) -> VcsResult<Recovery> {
        super::stash_pop(shared(), path, target, consent)
    }
    /// See [`super::stash_drop`].
    pub fn stash_drop(
        path: &Path,
        target: &StashTarget,
        consent: &HumanConsent,
    ) -> VcsResult<Recovery> {
        super::stash_drop(shared(), path, target, consent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha(n: u8) -> CommitId {
        CommitId::new(format!("{:0>40}", n))
    }

    fn step(action: RebaseAction, n: u8, message: Option<&str>) -> RebaseStep {
        RebaseStep {
            action,
            commit: sha(n).as_str().to_string(),
            message: message.map(str::to_string),
        }
    }

    fn plan(steps: Vec<RebaseStep>) -> RebasePlan {
        RebasePlan {
            upstream: "main".into(),
            onto: None,
            steps,
        }
    }

    #[test]
    fn a_plan_names_every_commit_of_the_range_once_keeps_one_and_starts_with_a_commit_that_stands()
    {
        let range = [sha(3), sha(2), sha(1)];
        assert!(validate_plan(
            &plan(vec![
                step(RebaseAction::Pick, 1, None),
                step(RebaseAction::Squash, 2, None),
                step(RebaseAction::Drop, 3, None)
            ]),
            &range
        )
        .is_ok());
        let refused = |steps: Vec<RebaseStep>| {
            matches!(
                validate_plan(&plan(steps), &range),
                Err(VcsError::InvalidArg { .. })
            )
        };
        assert!(refused(vec![]), "no steps");
        assert!(
            refused(vec![
                step(RebaseAction::Pick, 1, None),
                step(RebaseAction::Pick, 2, None)
            ]),
            "a commit of the range missing"
        );
        assert!(
            refused(vec![
                step(RebaseAction::Pick, 1, None),
                step(RebaseAction::Pick, 2, None),
                step(RebaseAction::Pick, 3, None),
                step(RebaseAction::Pick, 9, None)
            ]),
            "a commit outside the range"
        );
        assert!(
            refused(vec![
                step(RebaseAction::Pick, 1, None),
                step(RebaseAction::Pick, 1, None),
                step(RebaseAction::Pick, 2, None)
            ]),
            "a commit twice"
        );
        assert!(
            refused(vec![
                step(RebaseAction::Drop, 1, None),
                step(RebaseAction::Drop, 2, None),
                step(RebaseAction::Drop, 3, None)
            ]),
            "nothing kept"
        );
        assert!(
            refused(vec![
                step(RebaseAction::Drop, 1, None),
                step(RebaseAction::Fixup, 2, None),
                step(RebaseAction::Pick, 3, None)
            ]),
            "the first kept cannot fold into a commit before it"
        );
        assert!(
            refused(vec![
                step(RebaseAction::Reword, 1, None),
                step(RebaseAction::Pick, 2, None),
                step(RebaseAction::Pick, 3, None)
            ]),
            "a reword needs its words"
        );
        assert!(refused(vec![
            step(RebaseAction::Reword, 1, Some("  ")),
            step(RebaseAction::Pick, 2, None),
            step(RebaseAction::Pick, 3, None)
        ]));
    }

    #[test]
    fn the_todo_is_gits_grammar_with_the_messages_amended_from_their_files() {
        let steps = vec![
            step(RebaseAction::Pick, 1, None),
            step(RebaseAction::Reword, 2, Some("two, reworded")),
            step(RebaseAction::Squash, 3, Some("two and three")),
            step(RebaseAction::Squash, 4, None),
            step(RebaseAction::Fixup, 5, None),
            step(RebaseAction::Drop, 6, None),
        ];
        let files = vec![
            (1, PathBuf::from("/g/it's/1.msg")),
            (2, PathBuf::from("/g/it's/2.msg")),
        ];
        let todo = todo_text(&steps, &files);
        assert_eq!(
            todo,
            format!(
                "pick {a}\npick {b}\nexec git commit --amend -F '/g/it'\\''s/1.msg'\nfixup {c}\nexec git commit --amend -F '/g/it'\\''s/2.msg'\nsquash {d}\nfixup {e}\ndrop {f}\n",
                a = sha(1).as_str(),
                b = sha(2).as_str(),
                c = sha(3).as_str(),
                d = sha(4).as_str(),
                e = sha(5).as_str(),
                f = sha(6).as_str()
            )
        );
        assert_eq!(shell_quote("plain"), "'plain'");
        assert!(
            SEQUENCE_EDITOR.contains("BISA_REBASE_TODO"),
            "the todo travels in the environment, never in the command"
        );
    }
}
