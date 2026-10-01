//! The IDE's git reads and index-only writes on a workstream's checkout
//! (ide/04 §6): hunk staging, blame and file history. Every
//! operation is keyed by a workstream — the primary for the project's own
//! tree — so a worktree and the root are one code path.
//!
//! Everything here is the *safe tier* of `bisa_vcs::git` — nothing can
//! change a byte of the working tree. Staging a hunk is `git apply --cached`,
//! the same class of operation as `stage`/`unstage`; the reverse form takes
//! the hunk back out of the index. Blame and history are reads.

use crate::projects::{blocking, checkout_of, checkout_tree};
use crate::{EngineError, Inner};
use bisa_core::{ProjectId, PublishPolicy, Vcs, WorkstreamId, WorkstreamKind};
use bisa_store::resolve_within;
use bisa_vcs::git::{
    self, BlameLine, BlobRev, BranchInfo, CommitDetail, CommitId, CommitSummary, FileStatus,
    RecoveryRef, RemoteBranchInfo, RemoteInfo, StashEntry, TagInfo,
};
pub use bisa_vcs::{ConfigScope, GitConfigView, GitIdentity, Ident, IdentitySource, InProgress};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// A patch larger than this is not a hunk somebody selected; refuse it
/// rather than feed megabytes to `git apply`.
pub const MAX_PATCH_BYTES: usize = 4 * 1024 * 1024;
/// The most commits one history call returns.
pub const HISTORY_CAP: usize = 500;
/// A commit's patch is cut here for the inspector; the flag says so.
pub const COMMIT_DIFF_CAP: usize = 2 * 1024 * 1024;

/// Apply `patch` to the index — or, with `reverse`, take it out again —
/// and answer with the file rows as `git status` now sees them.
pub async fn stage_hunk(
    inner: &Inner,
    workstream: WorkstreamId,
    patch: String,
    reverse: bool,
) -> Result<Vec<FileStatus>, EngineError> {
    if patch.trim().is_empty() {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-patch-empty"
        )));
    }
    if patch.len() > MAX_PATCH_BYTES {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-patch-bytes-cap",
            a0 = (patch.len()).to_string(),
            max_patch_bytes = (MAX_PATCH_BYTES).to_string()
        )));
    }
    let (_, path) = checkout_tree(inner, workstream)?;
    let rows = blocking(move || {
        git::apply_cached(&path, &patch, reverse)?;
        git::status_files(&path)
    })
    .await;
    // A hunk in or out of the index moves the staged and unstaged counts.
    inner.ide_status.invalidate(workstream);
    if let Err(e) = &rows {
        crate::projects::log_git_failure(
            workstream,
            if reverse {
                "unstage_hunk"
            } else {
                "stage_hunk"
            },
            e,
        );
    }
    rows
}

/// `git blame --porcelain` for one file, optionally one 1-based inclusive
/// line range.
pub async fn blame(
    inner: &Inner,
    workstream: WorkstreamId,
    pathspec: String,
    range: Option<(u32, u32)>,
) -> Result<Vec<BlameLine>, EngineError> {
    let (_, path) = checkout_tree(inner, workstream)?;
    blocking(move || git::blame(&path, &pathspec, range)).await
}

/// The commits that touched one path, newest first, following renames.
/// `limit` is clamped to `1..=HISTORY_CAP`.
pub async fn history(
    inner: &Inner,
    workstream: WorkstreamId,
    pathspec: String,
    limit: usize,
) -> Result<Vec<CommitSummary>, EngineError> {
    let limit = limit.clamp(1, HISTORY_CAP);
    let (_, path) = checkout_tree(inner, workstream)?;
    blocking(move || git::file_history(&path, &pathspec, limit)).await
}

/// One commit for the inspector: the detail (message, refs, files against the
/// first parent) and its whole patch, cut at [`COMMIT_DIFF_CAP`] on a line.
pub async fn commit(
    inner: &Inner,
    workstream: WorkstreamId,
    sha: String,
) -> Result<(CommitDetail, String, bool), EngineError> {
    let (_, path) = checkout_tree(inner, workstream)?;
    blocking(move || {
        let detail = git::commit_detail(&path, &sha)?;
        let (diff, truncated) = cap_diff(git::commit_diff(&path, &sha, &[])?);
        Ok((detail, diff, truncated))
    })
    .await
}

/// How one file of a commit is read (ide/05): the revision its left side is
/// at — the first parent, none for a root commit — the path it had there
/// (`old_path` for a rename or a copy), and the pathspecs its patch is asked
/// with (both paths of a rename, so the detection has the whole of it). One
/// plan for the hunks and for the comparison, so the two never disagree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitFilePlan {
    pub parent: Option<CommitId>,
    pub original_path: PathBuf,
    pub paths: Vec<String>,
}

/// The plan for `path` in `detail`, or `None` when the commit did not touch it.
pub fn commit_file_plan(detail: &CommitDetail, path: &str) -> Option<CommitFilePlan> {
    let file = detail.files.iter().find(|f| f.path == Path::new(path))?;
    let original_path = file.old_path.clone().unwrap_or_else(|| file.path.clone());
    let mut paths = vec![original_path.to_string_lossy().into_owned()];
    let new = file.path.to_string_lossy().into_owned();
    if !paths.contains(&new) {
        paths.push(new);
    }
    Some(CommitFilePlan {
        parent: detail.parents.first().cloned(),
        original_path,
        paths,
    })
}

/// The plan for one file of a commit, or the refusal a path the commit did
/// not touch earns — an invalid argument, as a bad pathspec is.
fn planned(root: &Path, sha: &str, pathspec: &str) -> bisa_vcs::VcsResult<CommitFilePlan> {
    let detail = git::commit_detail(root, sha)?;
    commit_file_plan(&detail, pathspec).ok_or_else(|| bisa_vcs::VcsError::InvalidArg {
        what: format!("a path of commit {sha}"),
        value: pathspec.to_string(),
    })
}

/// One file's patch in one commit, against the first parent, cut at
/// [`COMMIT_DIFF_CAP`] on a line — the *Hunks* view of a commit's file.
pub async fn commit_file(
    inner: &Inner,
    workstream: WorkstreamId,
    sha: String,
    pathspec: String,
) -> Result<(String, bool), EngineError> {
    let (_, root) = checkout_tree(inner, workstream)?;
    blocking(move || {
        let plan = planned(&root, &sha, &pathspec)?;
        let paths: Vec<&str> = plan.paths.iter().map(String::as_str).collect();
        Ok(cap_diff(git::commit_diff(&root, &sha, &paths)?))
    })
    .await
}

/// The two whole texts of one file's change in one commit, for a comparison:
/// the file as the first parent held it — at its old path for a rename,
/// nothing for a root commit or a new file — against the file as the commit
/// holds it, nothing for a deleted one. A read of objects; the tree is not
/// touched.
pub async fn commit_file_sides(
    inner: &Inner,
    workstream: WorkstreamId,
    sha: String,
    pathspec: String,
) -> Result<FileSides, EngineError> {
    let (_, root) = checkout_tree(inner, workstream)?;
    blocking(move || {
        let plan = planned(&root, &sha, &pathspec)?;
        let original = match plan.parent {
            Some(parent) => {
                let at = plan.original_path.to_string_lossy();
                side_text(git::blob(&root, BlobRev::Rev(parent.to_string()), &at)?)
            }
            None => SideText::default(),
        };
        let modified = side_text(git::blob(&root, BlobRev::Rev(sha), &pathspec)?);
        Ok(FileSides { original, modified })
    })
    .await
}

/// One side of a file's change as a comparison reads it: its text when it
/// is text and within [`COMMIT_DIFF_CAP`], `binary` when a NUL byte or a
/// sequence that is not UTF-8 says it is not text (no text then),
/// `truncated` when the cap cut it. `text: None` with neither flag is a
/// side that is not there — a new file's left, a deleted file's right.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SideText {
    pub text: Option<String>,
    pub binary: bool,
    pub truncated: bool,
}

/// The two whole texts of one file's change (ide/04 §The Changes view):
/// for the working-tree side the index against the tree, for the staged
/// side HEAD against the index — what a side-by-side or inline comparison
/// draws, where a patch draws hunks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileSides {
    pub original: SideText,
    pub modified: SideText,
}

/// A side's bytes read as text: a NUL byte or invalid UTF-8 is binary and
/// carries no text; over [`COMMIT_DIFF_CAP`] the text is cut on a char
/// boundary and then on a line, and says so.
pub fn side_text(bytes: Option<Vec<u8>>) -> SideText {
    let Some(bytes) = bytes else {
        return SideText::default();
    };
    if bytes.contains(&0) {
        return SideText {
            text: None,
            binary: true,
            truncated: false,
        };
    }
    let truncated = bytes.len() > COMMIT_DIFF_CAP;
    let kept = if truncated {
        &bytes[..COMMIT_DIFF_CAP]
    } else {
        &bytes[..]
    };
    let text = match std::str::from_utf8(kept) {
        Ok(t) => t.to_string(),
        // A cut can land inside a character: the head that is valid is the text.
        Err(e) if truncated && e.error_len().is_none() => {
            String::from_utf8_lossy(&kept[..e.valid_up_to()]).into_owned()
        }
        Err(_) => {
            return SideText {
                text: None,
                binary: true,
                truncated: false,
            }
        }
    };
    let text = if truncated {
        let cut = text.rfind('\n').map(|i| i + 1).unwrap_or(text.len());
        text[..cut].to_string()
    } else {
        text
    };
    SideText {
        text: Some(text),
        binary: false,
        truncated,
    }
}

/// The two whole texts of one file's change, for a comparison: the
/// staged side is HEAD's blob against the index's; the working-tree side
/// is the index's blob against the file on disk. A side git does not hold
/// — nothing at HEAD for a new file, no index entry for one it has never
/// seen — or a file gone from the tree is a side with no text. A read,
/// never a stage.
pub async fn file_sides(
    inner: &Inner,
    workstream: WorkstreamId,
    pathspec: String,
    staged: bool,
) -> Result<FileSides, EngineError> {
    let (_, root) = checkout_tree(inner, workstream)?;
    tokio::task::spawn_blocking(move || -> Result<FileSides, EngineError> {
        if staged {
            return Ok(FileSides {
                original: side_text(git::blob(&root, BlobRev::Head, &pathspec)?),
                modified: side_text(git::blob(&root, BlobRev::Index, &pathspec)?),
            });
        }
        let original = side_text(git::blob(&root, BlobRev::Index, &pathspec)?);
        let on_disk = resolve_within(&root, &pathspec)?;
        let bytes = match std::fs::read(&on_disk) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        Ok(FileSides {
            original,
            modified: side_text(bytes),
        })
    })
    .await
    .map_err(|e| {
        EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-vcs-task-did-not-finish",
            e = e.to_string()
        ))
    })?
}

/// A conflicted path, whole (ide/04 §Conflicts, continued): what kind of
/// conflict it is, its three sides from the index's stages, and the file as
/// git wrote it on disk — the marker-laden text a person resolves block by
/// block — with the file's hash, for the compare-and-swap save that follows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Conflict {
    pub kind: Option<bisa_vcs::ConflictKind>,
    pub base: SideText,
    pub ours: SideText,
    pub theirs: SideText,
    /// The working tree's text; none when the file is not there (deleted by
    /// a side and not yet chosen) or is binary.
    pub text: Option<String>,
    pub hash: Option<String>,
    /// Any side, or the file itself, is not text.
    pub binary: bool,
}

/// Read a conflicted path whole. A read, never a stage.
pub async fn conflict(
    inner: &Inner,
    workstream: WorkstreamId,
    pathspec: String,
) -> Result<Conflict, EngineError> {
    let (_, root) = checkout_tree(inner, workstream)?;
    tokio::task::spawn_blocking(move || -> Result<Conflict, EngineError> {
        let blobs = git::conflict_blobs(&root, &pathspec)?;
        let kind = git::status_files(&root)?
            .into_iter()
            .find(|f| f.path == std::path::Path::new(&pathspec))
            .and_then(|f| f.conflict);
        let on_disk = resolve_within(&root, &pathspec)?;
        let bytes = match std::fs::read(&on_disk) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        let hash = bytes.as_deref().map(bisa_store::content_hash);
        let file = side_text(bytes);
        let base = side_text(blobs.base);
        let ours = side_text(blobs.ours);
        let theirs = side_text(blobs.theirs);
        let binary = file.binary || base.binary || ours.binary || theirs.binary;
        Ok(Conflict {
            kind,
            base,
            ours,
            theirs,
            text: if binary { None } else { file.text },
            hash,
            binary,
        })
    })
    .await
    .map_err(|e| {
        EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-vcs-task-did-not-finish",
            e = e.to_string()
        ))
    })?
}

/// The facts of the operation half-done in a checkout, or none.
pub async fn operation(
    inner: &Inner,
    workstream: WorkstreamId,
) -> Result<Option<bisa_vcs::OperationFacts>, EngineError> {
    let (_, path) = checkout_tree(inner, workstream)?;
    blocking(move || git::operation_facts(&path)).await
}

/// What merging `source` into the checkout's HEAD would do, before it runs.
/// `None` on a git without `merge-tree --write-tree`.
pub async fn merge_preview(
    inner: &Inner,
    workstream: WorkstreamId,
    source: String,
) -> Result<Option<bisa_vcs::MergePreview>, EngineError> {
    let (_, path) = checkout_tree(inner, workstream)?;
    blocking(move || git::merge_preview(&path, "HEAD", &source)).await
}

/// Cut a patch at [`COMMIT_DIFF_CAP`] on a line boundary; says whether it did.
pub fn cap_diff(mut diff: String) -> (String, bool) {
    let truncated = diff.len() > COMMIT_DIFF_CAP;
    if truncated {
        let mut end = COMMIT_DIFF_CAP;
        while !diff.is_char_boundary(end) {
            end -= 1;
        }
        let cut = diff[..end].rfind('\n').map(|i| i + 1).unwrap_or(end);
        diff.truncate(cut);
    }
    (diff, truncated)
}

/// The stash list, newest first (ide/04 §Stash). The list is the
/// repository's — every worktree of a project shares `refs/stash` — so an
/// entry made in another workstream is here too; `branch` says where.
pub async fn stashes(
    inner: &Inner,
    workstream: WorkstreamId,
) -> Result<Vec<StashEntry>, EngineError> {
    let (_, path) = checkout_tree(inner, workstream)?;
    blocking(move || git::stash_list(&path)).await
}

/// One stash entry's patch — the tracked change against the commit it was
/// made on, and the untracked files it carries — cut like a commit's.
pub async fn stash_diff(
    inner: &Inner,
    workstream: WorkstreamId,
    commit: String,
) -> Result<(String, bool), EngineError> {
    let (_, path) = checkout_tree(inner, workstream)?;
    blocking(move || Ok(cap_diff(git::stash_diff(&path, &commit)?))).await
}

/// Who will author commits in this checkout's repository, and where that
/// answer comes from. A worktree shares its repository's config, so the
/// primary and every workstream of a project answer alike.
pub async fn identity(inner: &Inner, workstream: WorkstreamId) -> Result<GitIdentity, EngineError> {
    let (_, path) = checkout_tree(inner, workstream)?;
    let git = inner.git();
    blocking(move || git.identity(&path)).await
}

/// What a repository nobody commits in could commit as: the identity the
/// checkout's connected code host account suggests (`identity::committer_suggestion`),
/// with the login it comes from. `None` without an account, without a host
/// this build knows, or when the account's record gives nothing to build
/// an email from. Offered, never written.
pub struct CommitterSuggestion {
    pub ident: Ident,
    pub login: String,
}

pub async fn committer_suggestion(
    inner: &Inner,
    workstream: WorkstreamId,
) -> Result<Option<CommitterSuggestion>, EngineError> {
    let Some((kind, login)) = crate::ide::connection::resolved_account(inner, workstream).await?
    else {
        return Ok(None);
    };
    let account = crate::codehost::account_of(inner, kind, &login).await?;
    Ok(crate::identity::committer_suggestion(kind, &account)
        .map(|ident| CommitterSuggestion { ident, login }))
}

/// The project's git config as the two layers hold it — every schema key,
/// what the repository sets and what it inherits from the global layer.
/// Keyed by a workstream: a worktree shares its repository's config.
pub async fn local_config(
    inner: &Inner,
    workstream: WorkstreamId,
) -> Result<GitConfigView, EngineError> {
    let (_, path) = checkout_tree(inner, workstream)?;
    let git = inner.git();
    blocking(move || git.config_view(Some(&path))).await
}

/// Write the repository's **local** git config: `set` key by key, then
/// `unset` (a key unset falls back to the global layer). Each key is validated
/// by the schema; the first refusal stops the batch and is the caller's 400.
/// The write itself is a person's own configuration and is not journaled;
/// what it *answers* is announced — when the repository now resolves an
/// identity, `committer_set` goes out, the project leaves the committer desk,
/// and a settlement the missing identity had refused is committed after all.
/// Answers the fresh view.
pub async fn set_local_config(
    inner: &Inner,
    workstream: WorkstreamId,
    set: Vec<(String, String)>,
    unset: Vec<String>,
) -> Result<GitConfigView, EngineError> {
    let (project, path) = checkout_tree(inner, workstream)?;
    let git = inner.git();
    let (view, identity) = blocking(move || {
        for (key, value) in &set {
            git.config_set(ConfigScope::Local, Some(&path), key, value)?;
        }
        for key in &unset {
            git.config_unset(ConfigScope::Local, Some(&path), key)?;
        }
        Ok((git.config_view(Some(&path))?, git.identity(&path)?))
    })
    .await?;
    // What matters is that the repository *resolves* an identity now — its
    // own pair, or one key of its own over the global layer — the same
    // reading `Committer::resolve` makes; only a repository nobody answers
    // for stays on the desk.
    if identity.source != IdentitySource::None {
        if let (Some(name), Some(email)) = (identity.name, identity.email) {
            crate::identity::committer_set(inner, &project, workstream, Ident { name, email })
                .await;
        }
    }
    Ok(view)
}

/// `project identity --name --email`: the two identity keys, locally.
pub async fn set_identity(
    inner: &Inner,
    workstream: WorkstreamId,
    name: &str,
    email: &str,
) -> Result<GitIdentity, EngineError> {
    set_local_config(
        inner,
        workstream,
        vec![
            ("user.name".to_string(), name.to_string()),
            ("user.email".to_string(), email.to_string()),
        ],
        Vec::new(),
    )
    .await?;
    identity(inner, workstream).await
}

/// Local branches, newest first, each with its standing: ahead/behind its
/// upstream, and whether it is merged into the project's default branch.
pub async fn branches(
    inner: &Inner,
    workstream: WorkstreamId,
) -> Result<Vec<BranchInfo>, EngineError> {
    let (_, project, path) = checkout_of(inner, workstream)?;
    let recorded = match &project.vcs {
        Vcs::Git { default_branch, .. } => Some(default_branch.clone()),
        _ => None,
    };
    blocking(move || {
        // The project records its default branch when it is made; a repository
        // initialised afterwards, or renamed, may call it something else. The
        // branch that exists is the one *merged* is judged against — the
        // remote's word, else a conventional trunk that exists, and never the
        // branch checked out now: a branch is trivially merged into itself.
        let default = match recorded {
            Some(b) if git::branch_exists(&path, &b).unwrap_or(false) => Some(b),
            _ => git::remote_default_branch(&path)
                .ok()
                .flatten()
                .or_else(|| {
                    ["main", "master"]
                        .into_iter()
                        .find(|b| git::branch_exists(&path, b).unwrap_or(false))
                        .map(str::to_string)
                }),
        };
        git::branch_list(&path, default.as_deref())
    })
    .await
}

/// Remote-tracking branches, newest first, as of the last fetch — a local
/// read; *Fetch* is what brings the remote's news in.
pub async fn remote_branches(
    inner: &Inner,
    workstream: WorkstreamId,
) -> Result<Vec<RemoteBranchInfo>, EngineError> {
    let (_, path) = checkout_tree(inner, workstream)?;
    blocking(move || git::remote_branch_list(&path)).await
}

/// Create a branch without switching to it — a new ref, nothing moves; with
/// `track`, one that follows `start` (a remote branch) as its upstream.
pub async fn branch_create(
    inner: &Inner,
    workstream: WorkstreamId,
    name: String,
    start: Option<String>,
    track: bool,
) -> Result<Vec<BranchInfo>, EngineError> {
    let (_, path) = checkout_tree(inner, workstream)?;
    blocking(move || {
        git::branch_create(&path, &name, start.as_deref(), track)?;
        git::branch_list(&path, None)
    })
    .await
}

/// Point a branch at the upstream it follows, or at none. Configuration,
/// not history.
pub async fn set_upstream(
    inner: &Inner,
    workstream: WorkstreamId,
    branch: String,
    upstream: Option<String>,
) -> Result<(), EngineError> {
    let (_, path) = checkout_tree(inner, workstream)?;
    blocking(move || git::branch_set_upstream(&path, &branch, upstream.as_deref())).await?;
    inner.ide_status.invalidate(workstream);
    Ok(())
}

/// The commits `from` has that `to` lacks, newest first — what a cherry-pick
/// from a branch offers and what an interactive rebase replays.
pub async fn commits_between(
    inner: &Inner,
    workstream: WorkstreamId,
    from: String,
    to: String,
    limit: usize,
) -> Result<Vec<CommitSummary>, EngineError> {
    let (_, path) = checkout_tree(inner, workstream)?;
    blocking(move || git::commits_between(&path, &from, &to, limit)).await
}

pub async fn tags(inner: &Inner, workstream: WorkstreamId) -> Result<Vec<TagInfo>, EngineError> {
    let (_, path) = checkout_tree(inner, workstream)?;
    blocking(move || git::tag_list(&path)).await
}

pub async fn remotes(
    inner: &Inner,
    workstream: WorkstreamId,
) -> Result<Vec<RemoteInfo>, EngineError> {
    let (_, path) = checkout_tree(inner, workstream)?;
    blocking(move || git::remote_list(&path)).await
}

/// Add a remote. Configuration, not history: nothing in the tree moves.
pub async fn remote_add(
    inner: &Inner,
    workstream: WorkstreamId,
    name: String,
    url: String,
) -> Result<Vec<RemoteInfo>, EngineError> {
    let (_, path) = checkout_tree(inner, workstream)?;
    let remotes = blocking({
        let (name, url) = (name.clone(), url.clone());
        move || {
            git::remote_add(&path, &name, &url)?;
            git::remote_list(&path)
        }
    })
    .await?;
    record_origin(inner, workstream, &name, &url)?;
    Ok(remotes)
}

/// Point `name` at `url` — adding it when it is not there, re-pointing it
/// when it is. Configuration, not history, so safe; the project record
/// follows for `origin`, since that is the remote the record names.
pub async fn set_remote(
    inner: &Inner,
    workstream: WorkstreamId,
    name: String,
    url: String,
) -> Result<Vec<RemoteInfo>, EngineError> {
    let (_, path) = checkout_tree(inner, workstream)?;
    let remotes = blocking({
        let (name, url) = (name.clone(), url.clone());
        move || {
            git::ensure_remote(&path, &name, &url)?;
            git::remote_list(&path)
        }
    })
    .await?;
    record_origin(inner, workstream, &name, &url)?;
    inner.ide_status.invalidate(workstream);
    Ok(remotes)
}

/// The record's `Vcs::Git { remote }` is `origin`'s URL: keep it true when
/// `origin` changes. Any other remote is git's business alone.
fn record_origin(
    inner: &Inner,
    workstream: WorkstreamId,
    name: &str,
    url: &str,
) -> Result<(), EngineError> {
    if name != "origin" {
        return Ok(());
    }
    let (_, mut project, _) = checkout_of(inner, workstream)?;
    if let Vcs::Git { remote, .. } = &mut project.vcs {
        if remote.as_deref() != Some(url) {
            *remote = Some(url.to_string());
            inner.ws.update_project(project)?;
        }
    }
    Ok(())
}

/// Bring `remote`'s refs up to date. Network, but nothing in the tree moves,
/// so safe; the status cache is dropped so ahead/behind reads fresh.
pub async fn fetch(
    inner: &Inner,
    workstream: WorkstreamId,
    remote: String,
) -> Result<(), EngineError> {
    let (_, path) = checkout_tree(inner, workstream)?;
    blocking(move || git::fetch(&path, &remote)).await?;
    inner.ide_status.invalidate(workstream);
    Ok(())
}

/// The operation git has left half-done in a checkout, if any.
pub async fn in_progress(
    inner: &Inner,
    workstream: WorkstreamId,
) -> Result<Option<InProgress>, EngineError> {
    let (_, path) = checkout_tree(inner, workstream)?;
    blocking(move || git::in_progress(&path)).await
}

/// The recovery points the interactive tier wrote, newest first (ide/04).
pub async fn recovery(
    inner: &Inner,
    workstream: WorkstreamId,
) -> Result<Vec<RecoveryRef>, EngineError> {
    let (_, path) = checkout_tree(inner, workstream)?;
    blocking(move || git::recovery_list(&path)).await
}

// ---------------------------------------------------------------------------
// Per-workstream status (ide/07)
// ---------------------------------------------------------------------------
//
// How long a status answer is reused is `cache.git_status.ttl_ms` (// default 2 s): longer than any burst of keystrokes in the switcher and shorter
// than anybody notices — the point is that ten rows do not run ten `git status`
// processes per keystroke, not that the number is stale-proof.

/// Branch, distance from its base and from its upstream, dirty counts, and
/// the agents running in it — everything a switcher row shows.
#[derive(Clone, Debug, serde::Serialize)]
pub struct WorkstreamStatus {
    pub workstream: WorkstreamId,
    pub project: ProjectId,
    pub kind: WorkstreamKind,
    /// The person's label, when they gave one.
    pub name: Option<String>,
    /// The project's publish policy — what a push here has to pass.
    pub publish: PublishPolicy,
    /// The project's default branch, when it is a repository.
    pub default_branch: Option<String>,
    /// The checkout is on disk. False after `?tree=true` closed it.
    pub exists: bool,
    pub git: bool,
    pub branch: Option<String>,
    pub base: Option<String>,
    pub ahead_of_base: Option<u32>,
    pub behind_base: Option<u32>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub staged: u32,
    pub unstaged: u32,
    pub untracked: u32,
    pub conflicted: u32,
    pub clean: bool,
    /// A merge, rebase, cherry-pick or revert git has left half-done here.
    pub in_progress: Option<InProgress>,
    /// The pull request the record knows of, when one is open — the card's
    /// chip, without a call to the code host.
    pub pr: Option<PrRef>,
    /// Sessions standing in this workstream: work-item runs and
    /// chat sessions alike, counted by where they run.
    pub running_agents: u32,
    pub state: bisa_core::WorkstreamState,
}

/// A pull request as the workstream record names it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct PrRef {
    pub number: u64,
    pub url: String,
}

impl PrRef {
    fn of(state: &bisa_core::WorkstreamState) -> Option<Self> {
        match state {
            bisa_core::WorkstreamState::PrOpen { number, url } => Some(Self {
                number: *number,
                url: url.clone(),
            }),
            _ => None,
        }
    }
}

/// The cache: one answer per workstream, over the shared TTL toolkit.
/// The TTL is `cache.git_status.ttl_ms`, passed by the caller.
pub struct StatusCache {
    cache: bisa_cache::TtlCache<WorkstreamId, WorkstreamStatus>,
}

impl Default for StatusCache {
    fn default() -> Self {
        Self {
            cache: bisa_cache::TtlCache::new("ide.git_status"),
        }
    }
}

impl StatusCache {
    fn fresh(&self, id: WorkstreamId, ttl: Duration) -> Option<WorkstreamStatus> {
        self.cache.get(ttl, &id)
    }
    fn put(&self, s: WorkstreamStatus) {
        self.cache.insert(s.workstream, s);
    }
    /// Forget one workstream — after a close, a commit, a push.
    pub fn invalidate(&self, id: WorkstreamId) {
        self.cache.invalidate(&id);
    }
}

/// The work sessions standing in a checkout: a step's worker, never a
/// conversation's turn — that one is its conversation's, shown, stopped and
/// followed there, and it is not what "running agents" on a workstream means.
fn running_in(inner: &Inner, w: &bisa_core::Workstream) -> u32 {
    inner
        .registry
        .list()
        .into_iter()
        .filter(|a| a.workstream == Some(w.id) && a.kind.is_work())
        .count() as u32
}

/// One workstream's status, from the cache when it is fresh.
pub async fn workstream_status(
    inner: &Inner,
    id: WorkstreamId,
) -> Result<WorkstreamStatus, EngineError> {
    let ttl = inner.cache.settings().git_status_ttl();
    if let Some(s) = inner.ide_status.fresh(id, ttl) {
        return Ok(s);
    }
    let (w, project, path) = checkout_of(inner, id)?;
    let running = running_in(inner, &w);
    let (branch, base) = (w.branch().map(str::to_string), w.base().map(str::to_string));
    let exists = path.is_dir();
    // A worktree is git by construction; the primary is git when its project
    // is a repository; a copy never is.
    let is_git = exists
        && match w.kind {
            WorkstreamKind::Worktree { .. } => true,
            WorkstreamKind::Primary => project.vcs.is_git(),
            WorkstreamKind::Copy => false,
        };
    let (status, ahead_behind, in_progress) = if is_git {
        let base_ref = base.clone();
        let (status, ab, op) = blocking(move || {
            let st = git::status(&path)?;
            let ab = base_ref
                .as_deref()
                .and_then(|b| git::ahead_behind(&path, b).ok());
            let op = git::in_progress(&path).ok().flatten();
            Ok((st, ab, op))
        })
        .await?;
        (Some(status), ab, op)
    } else {
        (None, None, None)
    };
    let default_branch = match &project.vcs {
        Vcs::Git { default_branch, .. } => Some(default_branch.clone()),
        Vcs::None => None,
    };
    let s = WorkstreamStatus {
        workstream: id,
        project: w.project,
        kind: w.kind.clone(),
        name: w.name.clone(),
        publish: project.publish,
        default_branch,
        exists,
        git: is_git,
        branch: status.as_ref().and_then(|s| s.branch.clone()).or(branch),
        base,
        ahead_of_base: ahead_behind.map(|(a, _)| a),
        behind_base: ahead_behind.map(|(_, b)| b),
        upstream: status.as_ref().and_then(|s| s.upstream.clone()),
        ahead: status.as_ref().map(|s| s.ahead).unwrap_or(0),
        behind: status.as_ref().map(|s| s.behind).unwrap_or(0),
        staged: status.as_ref().map(|s| s.staged).unwrap_or(0),
        unstaged: status.as_ref().map(|s| s.unstaged).unwrap_or(0),
        untracked: status.as_ref().map(|s| s.untracked).unwrap_or(0),
        conflicted: status.as_ref().map(|s| s.conflicted).unwrap_or(0),
        clean: status.as_ref().map(|s| s.is_clean).unwrap_or(true),
        in_progress,
        pr: PrRef::of(&w.state),
        running_agents: running,
        state: w.state.clone(),
    };
    if !ttl.is_zero() {
        inner.ide_status.put(s.clone());
    }
    Ok(s)
}

/// Every open workstream of a project — or of the workspace, with `None` —
/// the primary first, each from the cache when fresh. One that cannot be read is skipped rather
/// than failing the row beside it.
pub async fn workstream_statuses(
    inner: &Inner,
    project: Option<ProjectId>,
) -> Result<Vec<WorkstreamStatus>, EngineError> {
    use bisa_store::WorkstreamFilter;
    let filter = match project {
        Some(p) => WorkstreamFilter::Project(p),
        None => WorkstreamFilter::All,
    };
    // Read the workstreams concurrently, not one git subprocess at a time.
    // Bounded so a project with many checkouts doesn't spawn a
    // subprocess storm; the input order (primary first) is restored after.
    use futures::stream::StreamExt;
    const CONCURRENCY: usize = 8;
    // Collect the ids up front (primary first) rather than borrowing each
    // `Workstream` across the stream: a per-item `&Workstream` gives the async
    // block a higher-ranked lifetime the axum handler bound cannot satisfy, so
    // the futures capture only the `Copy` id and the shared `&Inner`.
    let ids: Vec<WorkstreamId> = inner
        .ws
        .list_workstreams(filter)?
        .into_iter()
        .filter(|w| !w.state.is_terminal())
        .map(|w| w.id)
        .collect();
    let mut indexed: Vec<(usize, WorkstreamStatus)> =
        futures::stream::iter(ids.into_iter().enumerate())
            .map(|(i, id)| async move {
                match workstream_status(inner, id).await {
                    Ok(s) => Some((i, s)),
                    Err(e) => {
                        tracing::warn!(workstream = %id, "status could not be read, the row is skipped: {e}");
                        None
                    }
                }
            })
            .buffer_unordered(CONCURRENCY)
            .filter_map(|x| async move { x })
            .collect()
            .await;
    indexed.sort_by_key(|(i, _)| *i);
    Ok(indexed.into_iter().map(|(_, s)| s).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_vcs::git::{ChangeKind, FileChange};

    fn change(path: &str, old: Option<&str>, kind: ChangeKind) -> FileChange {
        FileChange {
            path: PathBuf::from(path),
            old_path: old.map(PathBuf::from),
            kind,
            insertions: 1,
            deletions: 0,
            binary: false,
        }
    }

    fn detail(parents: &[&str], files: Vec<FileChange>) -> CommitDetail {
        CommitDetail {
            id: CommitId::new("c0ffee"),
            short: "c0ffee".into(),
            parents: parents.iter().map(|p| CommitId::new(*p)).collect(),
            refs: Vec::new(),
            author: "Ada".into(),
            email: "ada@example.com".into(),
            timestamp: 0,
            subject: "a change".into(),
            body: String::new(),
            files,
        }
    }

    #[test]
    fn a_commits_file_is_planned_against_the_first_parent_at_its_old_path_or_not_at_all() {
        let merge = detail(
            &["p1", "p2"],
            vec![
                change("a.txt", None, ChangeKind::Modified),
                change("docs/README.md", Some("README.md"), ChangeKind::Renamed),
            ],
        );
        let plain = commit_file_plan(&merge, "a.txt").unwrap();
        assert_eq!(
            plain.parent,
            Some(CommitId::new("p1")),
            "the first parent, never the second"
        );
        assert_eq!(plain.original_path, PathBuf::from("a.txt"));
        assert_eq!(plain.paths, vec!["a.txt"], "one path, asked once");

        let moved = commit_file_plan(&merge, "docs/README.md").unwrap();
        assert_eq!(
            moved.original_path,
            PathBuf::from("README.md"),
            "the left side is read where the file was"
        );
        assert_eq!(
            moved.paths,
            vec!["README.md", "docs/README.md"],
            "both paths, so the detection has the whole rename"
        );
        assert_eq!(
            commit_file_plan(&merge, "README.md"),
            None,
            "the old path is not a file of the commit"
        );
        assert_eq!(
            commit_file_plan(&merge, "missing.txt"),
            None,
            "a path the commit did not touch"
        );

        let root = detail(&[], vec![change("a.txt", None, ChangeKind::Added)]);
        let first = commit_file_plan(&root, "a.txt").unwrap();
        assert_eq!(first.parent, None, "a root commit has no left side");
    }

    #[test]
    fn a_side_is_text_binary_absent_or_cut_at_the_cap() {
        assert_eq!(
            side_text(None),
            SideText::default(),
            "a side git does not hold"
        );
        assert_eq!(
            side_text(Some(b"fn main() {}\n".to_vec())),
            SideText {
                text: Some("fn main() {}\n".into()),
                binary: false,
                truncated: false
            }
        );
        let nul = side_text(Some(b"PNG\x00\x01".to_vec()));
        assert!(
            nul.binary && nul.text.is_none(),
            "a NUL byte is binary, and no text is served"
        );
        let bad = side_text(Some(vec![0xff, 0xfe, b'a']));
        assert!(
            bad.binary && bad.text.is_none(),
            "bytes that are not UTF-8 are binary"
        );

        let mut big = "line\n".repeat(COMMIT_DIFF_CAP / 5 + 10);
        big.push_str("tail without newline");
        let cut = side_text(Some(big.into_bytes()));
        assert!(cut.truncated && !cut.binary);
        let text = cut.text.unwrap();
        assert!(text.len() <= COMMIT_DIFF_CAP);
        assert!(text.ends_with('\n'), "cut on a line");

        // A cap landing inside a character keeps the head that is whole.
        let mut wide = "é".repeat(COMMIT_DIFF_CAP / 2 + 4).into_bytes();
        wide.push(b'\n');
        let cut = side_text(Some(wide));
        assert!(
            cut.truncated && !cut.binary,
            "a cut mid-character is not binary"
        );
        assert!(std::str::from_utf8(cut.text.unwrap().as_bytes()).is_ok());
    }
}
