//! The engine's one door to the consented tier (ide/04).
//!
//! **This is the only module in the workspace, outside `bisa-vcs`
//! itself, allowed to name `bisa_vcs::interactive`** — the core layering
//! test fails the build otherwise. Every function takes a [`HumanConsent`] by
//! value: the node minted it from an authenticated request, it is used once,
//! and the MCP intake has no way to obtain one, so no agent path leads here.
//!
//! Each call returns what the operation saved first — the [`Recovery`] — with
//! the file rows and branch as they are now, so the panel that asked can
//! redraw from the answer and offer *Restore what was here*.

use crate::identity::CommitterReason;
use crate::projects::{
    blocking, checkout_of, checkout_tree, ensure_identity, journal_fact, journal_progress,
    pass_publish_gate, reconcile_before_publish, require_worktree, transition,
};
use crate::{EngineError, Inner};
use bisa_core::{WorkstreamId, WorkstreamKind, WorkstreamTransition};
use bisa_vcs::git::{self, CommitId, FileStatus, StashEntry};
use bisa_vcs::interactive::{ops, HumanConsent, Pin, PullMode, PullOutcome, Recovery};
use bisa_vcs::VcsError;
use bisa_vcs::{MergeMode, PickRequest, RebasePlan, RebaseRequest, Resolution, RevertRequest};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use bisa_vcs::interactive::{InProgress, StashPush, StashTarget};

/// What a consented operation answers with.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Done {
    pub recovery: Recovery,
    /// The tree as `git status` sees it now.
    pub files: Vec<FileStatus>,
    /// The branch HEAD is on now, `None` when detached.
    pub branch: Option<String>,
}

fn finish(path: &Path, recovery: Recovery) -> Result<Done, VcsError> {
    let files = git::status_files(path)?;
    let branch = git::status(path).ok().and_then(|s| s.branch);
    Ok(Done {
        recovery,
        files,
        branch,
    })
}

async fn consented<F>(inner: &Inner, workstream: WorkstreamId, f: F) -> Result<Done, EngineError>
where
    F: FnOnce(&PathBuf) -> Result<Recovery, VcsError> + Send + 'static,
{
    let (_, path) = checkout_tree(inner, workstream)?;
    let done = blocking(move || {
        let recovery = f(&path)?;
        finish(&path, recovery)
    })
    .await;
    // One place for every consented operation: what failed is in git's own
    // words, which name the invocation.
    if let Err(e) = &done {
        crate::projects::log_git_failure(workstream, "consented", e);
    }
    done
}

pub async fn checkout(
    inner: &Inner,
    workstream: WorkstreamId,
    target: String,
    consent: HumanConsent,
) -> Result<Done, EngineError> {
    let d = consented(inner, workstream, move |p| {
        ops::checkout(p, &target, &consent)
    })
    .await;
    inner.ide_status.invalidate(workstream);
    d
}

pub async fn branch_delete(
    inner: &Inner,
    workstream: WorkstreamId,
    name: String,
    consent: HumanConsent,
) -> Result<Done, EngineError> {
    let d = consented(inner, workstream, move |p| {
        ops::branch_delete(p, &name, &consent)
    })
    .await;
    inner.ide_status.invalidate(workstream);
    d
}

/// Rename a local branch. Never the project's default branch — that name is
/// what every workstream branches from and what a push protects. When the
/// renamed branch is the one this workstream checks out, the workstream's
/// record follows, so the rail and the strip say the new name.
pub async fn rename_branch(
    inner: &Inner,
    workstream: WorkstreamId,
    from: String,
    to: String,
    consent: HumanConsent,
) -> Result<Done, EngineError> {
    let w = inner.ws.get_workstream(workstream)?;
    let project = inner.ws.get_project(w.project)?;
    if let bisa_core::Vcs::Git { default_branch, .. } = &project.vcs {
        if default_branch == &from {
            return Err(EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-project-s-default-branch-not-renamed-from",
                from = from.to_string()
            )));
        }
    }
    let renamed_from = from.clone();
    let renamed_to = to.clone();
    let done = consented(inner, workstream, move |p| {
        ops::branch_rename(p, &from, &to, &consent)
    })
    .await?;
    if let WorkstreamKind::Worktree { branch, base } = &w.kind {
        if branch == &renamed_from {
            let mut edited = w.clone();
            edited.kind = WorkstreamKind::Worktree {
                branch: renamed_to,
                base: base.clone(),
            };
            inner.ws.update_workstream(&edited)?;
        }
    }
    inner.ide_status.invalidate(workstream);
    Ok(done)
}

pub async fn rebase(
    inner: &Inner,
    workstream: WorkstreamId,
    req: RebaseRequest,
    consent: HumanConsent,
) -> Result<Done, EngineError> {
    let d = consented(inner, workstream, move |p| ops::rebase(p, &req, &consent)).await;
    inner.ide_status.invalidate(workstream);
    d
}

/// An interactive rebase planned in full — every commit since the upstream
/// with its action — run with no terminal anywhere (ide/04).
pub async fn rebase_plan(
    inner: &Inner,
    workstream: WorkstreamId,
    plan: RebasePlan,
    consent: HumanConsent,
) -> Result<Done, EngineError> {
    let d = consented(inner, workstream, move |p| {
        ops::rebase_plan(p, &plan, &consent)
    })
    .await;
    inner.ide_status.invalidate(workstream);
    d
}

pub async fn merge(
    inner: &Inner,
    workstream: WorkstreamId,
    source: String,
    mode: MergeMode,
    message: Option<String>,
    consent: HumanConsent,
) -> Result<Done, EngineError> {
    let d = consented(inner, workstream, move |p| {
        ops::merge(p, &source, mode, message.as_deref(), &consent)
    })
    .await;
    inner.ide_status.invalidate(workstream);
    d
}

pub async fn cherry_pick(
    inner: &Inner,
    workstream: WorkstreamId,
    req: PickRequest,
    consent: HumanConsent,
) -> Result<Done, EngineError> {
    let d = consented(inner, workstream, move |p| {
        ops::cherry_pick(p, &req, &consent)
    })
    .await;
    inner.ide_status.invalidate(workstream);
    d
}

/// Undo commits with new ones — the graph's *Revert*.
pub async fn revert(
    inner: &Inner,
    workstream: WorkstreamId,
    req: RevertRequest,
    consent: HumanConsent,
) -> Result<Done, EngineError> {
    let d = consented(inner, workstream, move |p| ops::revert(p, &req, &consent)).await?;
    inner.ide_status.invalidate(workstream);
    Ok(d)
}

/// Go on with the operation git has half-done, once every conflicted path
/// is settled; refused with the paths while any remains (ide/04).
pub async fn continue_op(
    inner: &Inner,
    workstream: WorkstreamId,
    what: InProgress,
    consent: HumanConsent,
) -> Result<Done, EngineError> {
    let d = consented(inner, workstream, move |p| {
        ops::continue_op(p, what, &consent)
    })
    .await;
    inner.ide_status.invalidate(workstream);
    d
}

/// Leave out the commit a rebase, cherry-pick or revert stopped on.
pub async fn skip_op(
    inner: &Inner,
    workstream: WorkstreamId,
    what: InProgress,
    consent: HumanConsent,
) -> Result<Done, EngineError> {
    let d = consented(inner, workstream, move |p| ops::skip_op(p, what, &consent)).await;
    inner.ide_status.invalidate(workstream);
    d
}

/// Settle one conflicted path whole — a side taken and staged, or the path
/// removed, for a side that deleted it.
pub async fn resolve(
    inner: &Inner,
    workstream: WorkstreamId,
    path: String,
    how: Resolution,
    consent: HumanConsent,
) -> Result<Done, EngineError> {
    let d = consented(inner, workstream, move |p| {
        ops::resolve(p, &path, how, &consent)
    })
    .await;
    inner.ide_status.invalidate(workstream);
    d
}

/// Delete a branch on a remote — an outward act, so it passes the project's
/// Publish gate like a push (`Auto` proceeds, `Gated` opens the gate, `Manual`
/// refuses), never the project's default branch, and consented. The
/// remote-tracking tip is pinned in Safety first by the tier itself.
pub async fn push_delete(
    inner: &Inner,
    workstream: WorkstreamId,
    remote: String,
    branch: String,
    consent: HumanConsent,
) -> Result<Done, EngineError> {
    let (w, project, path) = checkout_of(inner, workstream)?;
    if let bisa_core::Vcs::Git { default_branch, .. } = &project.vcs {
        if default_branch == &branch {
            return Err(EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-project-s-default-branch-not-deleted-from",
                branch = branch.to_string()
            )));
        }
    }
    let what = format!("delete {remote}/{branch}");
    let asked = pass_publish_gate(inner, &w, &project, &what).await?;
    let d = blocking(move || {
        let recovery = ops::push_delete(&path, &remote, &branch, &consent)?;
        finish(&path, recovery)
    })
    .await;
    inner.ide_status.invalidate(workstream);
    asked.heard(inner, &w, &what, d)
}

/// What a pull answers with: the tree as every consented verb reports it,
/// and where the branch went.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Pulled {
    #[serde(flatten)]
    pub done: Done,
    pub pull: PullOutcome,
}

/// Fetch `remote` and bring the checkout's branch up to its upstream by
/// `mode` (ide/04: a pull is a safe fetch and a consented merge or rebase).
/// The record is not walked here — a pull brings other people's commits in,
/// it publishes nothing — but the status cache is dropped so ahead/behind
/// and the branch tip read fresh.
pub async fn pull(
    inner: &Inner,
    workstream: WorkstreamId,
    remote: String,
    mode: PullMode,
    consent: HumanConsent,
) -> Result<Pulled, EngineError> {
    let (_, path) = checkout_tree(inner, workstream)?;
    let pulled = blocking(move || {
        let (recovery, outcome) = ops::pull(&path, &remote, mode, &consent)?;
        Ok(Pulled {
            done: finish(&path, recovery)?,
            pull: outcome,
        })
    })
    .await?;
    inner.ide_status.invalidate(workstream);
    Ok(pulled)
}

pub async fn abort(
    inner: &Inner,
    workstream: WorkstreamId,
    what: InProgress,
    consent: HumanConsent,
) -> Result<Done, EngineError> {
    let d = consented(inner, workstream, move |p| ops::abort(p, what, &consent)).await;
    inner.ide_status.invalidate(workstream);
    d
}

/// Throw away a working-tree hunk (the unstaged patch, reversed) — bounded
/// like a staged one.
pub async fn discard_hunk(
    inner: &Inner,
    workstream: WorkstreamId,
    patch: String,
    consent: HumanConsent,
) -> Result<Done, EngineError> {
    if patch.len() > super::git::MAX_PATCH_BYTES {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-patch-bytes-cap-2",
            a0 = (patch.len()).to_string(),
            a1 = (super::git::MAX_PATCH_BYTES).to_string()
        )));
    }
    let d = consented(inner, workstream, move |p| {
        ops::discard_hunk(p, &patch, &consent)
    })
    .await;
    inner.ide_status.invalidate(workstream);
    d
}

pub async fn discard_paths(
    inner: &Inner,
    workstream: WorkstreamId,
    paths: Vec<String>,
    consent: HumanConsent,
) -> Result<Done, EngineError> {
    let d = consented(inner, workstream, move |p| {
        ops::discard_paths(p, &paths, &consent)
    })
    .await;
    inner.ide_status.invalidate(workstream);
    d
}

pub async fn tag_create(
    inner: &Inner,
    workstream: WorkstreamId,
    name: String,
    target: Option<String>,
    message: Option<String>,
    consent: HumanConsent,
) -> Result<Done, EngineError> {
    let d = consented(inner, workstream, move |p| {
        ops::tag_create(p, &name, target.as_deref(), message.as_deref(), &consent)
    })
    .await;
    inner.ide_status.invalidate(workstream);
    d
}

pub async fn tag_delete(
    inner: &Inner,
    workstream: WorkstreamId,
    name: String,
    consent: HumanConsent,
) -> Result<Done, EngineError> {
    let d = consented(inner, workstream, move |p| {
        ops::tag_delete(p, &name, &consent)
    })
    .await;
    inner.ide_status.invalidate(workstream);
    d
}

pub async fn remote_remove(
    inner: &Inner,
    workstream: WorkstreamId,
    name: String,
    consent: HumanConsent,
) -> Result<Done, EngineError> {
    let d = consented(inner, workstream, move |p| {
        ops::remote_remove(p, &name, &consent)
    })
    .await;
    inner.ide_status.invalidate(workstream);
    d
}

/// *Restore what was here*: put back what a recovery ref saved.
pub async fn restore(
    inner: &Inner,
    workstream: WorkstreamId,
    ref_name: String,
    consent: HumanConsent,
) -> Result<Done, EngineError> {
    let d = consented(inner, workstream, move |p| {
        ops::restore(p, &ref_name, &consent)
    })
    .await;
    inner.ide_status.invalidate(workstream);
    d
}

// ---------------------------------------------------------------------------
// Stash (ide/04 §Stash). The list and a stash's patch are `super::git`'s
// reads; the four verbs here move the tree or the list and are consented.
// Each drops the status cache: the tree changed, or a stash conflict left
// paths unmerged.
// ---------------------------------------------------------------------------

/// What a stash push answers with: the tree as every consented verb reports
/// it, and the entry the list now starts with.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Stashed {
    #[serde(flatten)]
    pub done: Done,
    pub stash: StashEntry,
}

/// Park the working tree's changes as a stash entry.
pub async fn stash_push(
    inner: &Inner,
    workstream: WorkstreamId,
    what: StashPush,
    consent: HumanConsent,
) -> Result<Stashed, EngineError> {
    let (_, path) = checkout_tree(inner, workstream)?;
    let stashed = blocking(move || {
        let (recovery, stash) = ops::stash_push(&path, &what, &consent)?;
        Ok(Stashed {
            done: finish(&path, recovery)?,
            stash,
        })
    })
    .await?;
    inner.ide_status.invalidate(workstream);
    Ok(stashed)
}

/// Apply a stash entry and keep it on the list.
pub async fn stash_apply(
    inner: &Inner,
    workstream: WorkstreamId,
    target: StashTarget,
    consent: HumanConsent,
) -> Result<Done, EngineError> {
    let d = consented(inner, workstream, move |p| {
        ops::stash_apply(p, &target, &consent)
    })
    .await;
    inner.ide_status.invalidate(workstream);
    d
}

/// Apply a stash entry and drop it — only when the apply went cleanly.
pub async fn stash_pop(
    inner: &Inner,
    workstream: WorkstreamId,
    target: StashTarget,
    consent: HumanConsent,
) -> Result<Done, EngineError> {
    let d = consented(inner, workstream, move |p| {
        ops::stash_pop(p, &target, &consent)
    })
    .await;
    inner.ide_status.invalidate(workstream);
    d
}

/// Drop a stash entry; its commit is pinned as a `.stash` recovery first.
pub async fn stash_drop(
    inner: &Inner,
    workstream: WorkstreamId,
    target: StashTarget,
    consent: HumanConsent,
) -> Result<Done, EngineError> {
    let d = consented(inner, workstream, move |p| {
        ops::stash_drop(p, &target, &consent)
    })
    .await;
    inner.ide_status.invalidate(workstream);
    d
}

/// What an amend answers with: the recovery and the tree, and the commit
/// HEAD is now.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Amended {
    pub done: Done,
    pub commit: CommitId,
}

/// Rewrite the checkout's last commit with what is staged and `message` —
/// `paths` staged first, as a commit does. Consented; the tier pins the old
/// commit before anything moves, so Safety's *Restore* is the undo. Refused
/// on a detached HEAD (nothing of the workstream's to rewrite), a repository
/// nobody is set to commit in (the committer desk is asked, as for a
/// commit), a half-done operation, or no commit yet. Whether HEAD is already
/// published is the person's to weigh — the panel says so — since the
/// branch then needs the lease push.
pub async fn amend(
    inner: &Inner,
    workstream: WorkstreamId,
    message: String,
    paths: Vec<String>,
    consent: HumanConsent,
) -> Result<Amended, EngineError> {
    let (project, path) = checkout_tree(inner, workstream)?;
    let status = blocking({
        let path = path.clone();
        move || git::status(&path)
    })
    .await?;
    if status.branch.is_none() {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-workstream-detached-head-nothing-amend",
            workstream = workstream.to_string()
        )));
    }
    ensure_identity(
        inner,
        path.clone(),
        &project,
        CommitterReason::CommitRefused,
        None,
    )
    .await?;
    if !paths.is_empty() {
        blocking({
            let path = path.clone();
            move || git::stage(&path, &paths)
        })
        .await?;
    }
    let amended = blocking({
        let path = path.clone();
        move || {
            let (recovery, commit) = ops::amend(&path, &message, &consent)?;
            Ok(Amended {
                done: finish(&path, recovery)?,
                commit,
            })
        }
    })
    .await?;
    inner.ide_status.invalidate(workstream);
    // Every goal attached to the project learns the commit changed under
    // it, as it learns of a commit.
    for goal in inner.ws.goals_of_project(project.id)? {
        journal_fact(
            inner,
            bisa_core::Home::Goal { goal },
            None,
            None,
            "amended",
            &format!(
                "{} {} → {}",
                project.slug,
                amended.done.recovery.commit.short(),
                amended.commit.short()
            ),
            Some(amended.commit.short()),
        );
    }
    Ok(amended)
}

/// Push a workstream's branch with `--force-with-lease` — the one forced push
/// there is. Three checks stand in front of it, in this order: the branch is
/// the workstream's own and **not the project's default branch**; the project's
/// Publish gate passes (same gate as an ordinary push); and the person
/// consented. The recovery ref is written by the tier itself.
pub async fn push_workstream_with_lease(
    inner: &Inner,
    id: WorkstreamId,
    consent: HumanConsent,
) -> Result<Recovery, EngineError> {
    let (w, project, path) = checkout_of(inner, id)?;
    let (branch, _) = require_worktree(&w)?;
    let root = inner.ws.project_root_path(&project);
    // No default branch readable is no answer, not a refusal: the check
    // below simply cannot say the branch is the default one.
    let default = blocking({
        let root = root.clone();
        move || git::default_branch(&root)
    })
    .await
    .inspect_err(|e| tracing::debug!(target: "bisa_engine::ide", "default branch not read: {e}"))
    .ok();
    if default.as_deref() == Some(branch.as_str()) {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-project-s-default-branch-forced-push-only",
            branch = branch.to_string()
        )));
    }
    let status = blocking({
        let path = path.clone();
        move || git::status(&path)
    })
    .await?;
    let w = reconcile_before_publish(inner, &w, &status, &path).await?;
    let what = format!("push {branch} with lease");
    let asked = pass_publish_gate(inner, &w, &project, &what).await?;
    let pushed = async {
        let recovery = blocking({
            let (path, branch) = (path.clone(), branch.clone());
            move || ops::push_with_lease(&path, "origin", &branch, &consent)
        })
        .await?;
        transition(inner, &w, &WorkstreamTransition::Pushed)?;
        journal_progress(inner, &w, "pushed with lease", &branch, Some("origin"));
        inner.ide_status.invalidate(id);
        Ok(recovery)
    }
    .await;
    asked.heard(inner, &w, &what, pushed)
}

/// What removing a checkout did: the close, and the recovery ref written
/// first — `None` for a copy workstream or a checkout already gone.
#[derive(Clone, Debug)]
pub struct TreeRemoved {
    pub closed: crate::projects::Closed,
    pub recovery: Option<Recovery>,
}

/// What a workspace does when the tree of a dirty checkout is asked to go.
pub const DIRTY_CLOSE_KEY: &str = "workstreams.dirty_close";

/// Whether `workstreams.dirty_close` is `refuse`. Unreadable, it is the
/// registry's default — the recovery ref — and never a refusal nobody chose.
fn refuses_dirty_close(inner: &Inner) -> bool {
    inner
        .ws
        .setting(DIRTY_CLOSE_KEY, None)
        .ok()
        .and_then(|r| r.value.as_str().map(|v| v == "refuse"))
        .unwrap_or(false)
}

/// What is uncommitted in a checkout, in words, or `None` when it is clean:
/// what a refusal names, so the person knows what they would have lost.
pub(crate) fn dirty_words(status: &git::Status) -> Option<String> {
    let parts: Vec<String> = [
        (status.conflicted, "conflicted"),
        (status.staged, "staged"),
        (status.unstaged, "changed"),
        (status.untracked, "untracked"),
    ]
    .into_iter()
    .filter(|(n, _)| *n > 0)
    .map(|(n, what)| format!("{n} {what} file{}", if n == 1 { "" } else { "s" }))
    .collect();
    (!parts.is_empty()).then(|| parts.join(", "))
}

/// Close a workstream **and remove its checkout**, saving what the checkout
/// held first (ide/07): every session standing in it is stopped
/// (`sessions::stop_workstream`) — an agent must not be writing into a tree
/// whose recovery ref is being cut — then a recovery ref in the project's
/// repository — which every worktree shares, so the ref outlives the
/// directory — then the existing `worktree_remove` carve-out. A clean tree
/// still pins its HEAD, so the branch tip is one `git branch` away even if
/// the branch itself is deleted later. The primary is refused before
/// anything is stopped.
pub async fn close_workstream_removing_tree(
    inner: &Arc<Inner>,
    id: WorkstreamId,
    consent: HumanConsent,
) -> Result<TreeRemoved, EngineError> {
    let (w, _, path) = crate::projects::closable(inner, id)?;
    // `workstreams.dirty_close = refuse`: a checkout that holds work nobody
    // committed keeps its tree, said before anything is stopped. The other
    // value is what follows — a recovery ref, then the tree.
    if path.is_dir() && refuses_dirty_close(inner) {
        let at = path.clone();
        let status = blocking(move || git::status(&at)).await?;
        if let Some(why) = dirty_words(&status) {
            return Err(EngineError::Conflict(bisa_core::text!(
                "error-engine-conflict-checkout-holds-refuse-so-tree-stays-commit",
                why = why.to_string(),
                dirty_close_key = (DIRTY_CLOSE_KEY).to_string()
            )));
        }
    }
    let stopped_sessions = crate::sessions::stop_workstream(inner, id);
    let recovery = match (&w.kind, path.is_dir()) {
        (WorkstreamKind::Worktree { .. }, true) => {
            Some(
                blocking(move || {
                    // Consent is what let this function be called; the tier's
                    // `capture` is the recovery half and takes none itself.
                    let _held = &consent;
                    bisa_vcs::interactive::capture(
                        &git::Git::default(),
                        &path,
                        "workstream_close",
                        Pin::None,
                    )
                })
                .await?,
            )
        }
        _ => None,
    };
    crate::projects::close_workstream_with(inner, id, true, crate::scripts::ScriptPolicy::Refuse)
        .await?;
    inner.ide_status.invalidate(id);
    Ok(TreeRemoved {
        closed: crate::projects::Closed {
            workstream: inner.ws.get_workstream(id)?,
            stopped_sessions,
        },
        recovery,
    })
}
