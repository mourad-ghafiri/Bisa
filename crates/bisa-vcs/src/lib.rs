//! `git` as typed, time-boxed subprocess calls.
//!
//! This is the VCS PAL: the one place in the tree that shells out to a version
//! control binary. It knows nothing about goals, work-items or the engine —
//! it takes a repository path and returns typed data or a typed error. It
//! follows the same discipline as `bisa-iso`:
//!
//! - **Two-phase availability.** [`git::probe`] answers a *host*-level
//!   question ("is the binary here"). A
//!   *per-repository* prerequisite failure — this path is not a repository,
//!   this repository has no `origin` — surfaces at call time as its own
//!   [`VcsError`] variant, never at probe time. A probe that passed is not a
//!   promise that any particular call will succeed.
//! - **Errors are typed, not prose.** Callers switch on [`VcsError`]; they
//!   never match on strings. Prose is parsed exactly once, here at the
//!   subprocess boundary, into a variant — the same rule the harness adapters
//!   follow.
//! - **Synchronous.** Everything blocks; the engine drives it from
//!   `spawn_blocking`. No tokio in this crate.
//!
//! Five properties are load-bearing for an unattended orchestrator and are
//! enforced in one shared runner rather than left to call sites:
//!
//! 1. **argv only, never a shell.** Commands are argv arrays. A branch name of
//!    `; rm -rf /` is a (perfectly legal, if silly) ref name, not a command.
//!    Arguments that could be read as options are rejected up front — see
//!    [`git::validate_ref`].
//! 2. **No prompt can ever hang us.** `GIT_TERMINAL_PROMPT=0`, an empty
//!    `GIT_ASKPASS`/`SSH_ASKPASS` and a null stdin turn a credential prompt
//!    into an immediate failure we classify as
//!    [`VcsError::NotAuthenticated`].
//! 3. **Every invocation is time-boxed** and the child is killed on expiry, so
//!    a wedged `git push` costs one work-item a timeout instead of stalling
//!    the scheduler forever. See [`Timeouts`].
//! 4. **A read never takes git's index lock.** `GIT_OPTIONAL_LOCKS=0` on every
//!    child: a `status` or a `diff` that refreshed the index used to take
//!    `index.lock` to write it back, and a commit asking for the lock in that
//!    instant failed with *another git process seems to be running*.
//! 5. **Writes to one checkout queue.** Every invocation that writes the
//!    index, the refs, the config or the tree holds its checkout's slot
//!    (`repo_lock`) while it runs, so two writers of this process never meet
//!    the lock either. A lock some *other* process holds is
//!    [`VcsError::RepositoryBusy`], typed, for the caller to say and try again.
//!
//! Three things this crate deliberately does *not* do. It never writes global
//! git config — the one config write it makes is `user.name`/`user.email` in a
//! **repository's local** config, at a person's request
//! ([`git::set_local_identity`]); an unset identity comes back as
//! [`VcsError::IdentityUnset`], for the caller to report precisely. It never
//! passes `--no-verify` — the user's hooks run, because they are the user's
//! hooks. And:
//!
//! > **Nothing here can discard a change to a file.** This crate can create,
//! > stage, commit and push. It cannot revert a file, and it must stay that
//! > way.
//!
//! One carve-out, named rather than hidden: [`git::worktree_remove`] deletes a
//! whole checkout, uncommitted work included. That is not a file operation and
//! no per-file call reaches it — its two callers close a workstream the caller
//! explicitly asked to remove, and `remove_tree` is opt-in precisely because
//! an uncommitted branch may be the only copy of somebody's work.
//!
//! Concretely: no `git checkout -- <path>`, no `git restore` *without*
//! `--staged`, no `git clean`, no `git reset` in any mode, no `git stash`
//! command (the stash *list* is read as the reflog of `refs/stash` it is —
//! `git log -g refs/stash` — and a stash's patch as a diff between two
//! commits; pushing, applying, popping and dropping live in the consented
//! tier), no `git rm`. Every one of those would be the first call in the tree able to
//! destroy something a person typed and never committed, and an unattended
//! orchestrator holding that call is a different and much worse thing than
//! one that cannot. [`git::unstage`] is the near miss the rule exists for: it
//! runs `git restore --staged`, which rewrites the *index* and leaves the
//! working tree byte-for-byte alone; the identical command without the flag
//! overwrites the file. The flag is not a parameter, so no call site can get
//! it wrong. `tests/it/git.rs` greps this crate's own source to keep it true.
//!
//! Undo belongs to the user's own `git`, where their reflog, their editor's
//! history and their muscle memory are.

use std::path::PathBuf;

pub mod config_schema;
mod exec;
pub mod git;
pub mod interactive;
pub mod operation;
mod parse;
mod repo_lock;
pub mod snapshot;

pub use config_schema::{
    default_account_key, key_def, validate_config_value, validate_login, writable_key,
    ConfigKeyDef, ConfigKind, ConfigScope, ACCOUNT_KEY, GIT_CONFIG_KEYS, KIND_KEY,
};
pub use exec::{scrub_userinfo, Timeouts};
pub use git::{
    is_key_path, BlameLine, BlobRev, BranchInfo, ChangeKind, CommitDetail, CommitId, CommitSummary,
    ConfigEntry, ConfigOrigin, ConflictBlobs, ConflictKind, ConflictSide, Credential, FileChange,
    FileStatus, Git, GitConfigView, GitIdentity, GitVersion, GraphCommit, GraphRef, Ident,
    IdentitySource, Include, MergePreview, ProfileFile, RecoveryKind, RecoveryRef,
    RemoteBranchInfo, RemoteInfo, RemoteRef, StashEntry, Status, TagInfo, WorktreeEntry,
};
pub use interactive::{
    HumanConsent, InProgress, MergeMode, PickRequest, Pin, PullMode, PullOutcome, RebaseAction,
    RebasePlan, RebaseRequest, RebaseStep, Recovery, Resolution, RevertRequest, StashPush,
    StashTarget, REBASE_PLAN_CAP,
};
pub use operation::{OperationFacts, SideRef, SideRole, Step};
pub use snapshot::TreeId;

/// Result of a host-level probe. Mirrors `bisa_iso::ProbeResult` by
/// design: same two-phase contract, so callers read one shape everywhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeResult {
    pub available: bool,
    pub reason: Option<String>,
}

impl ProbeResult {
    pub const fn available() -> Self {
        Self {
            available: true,
            reason: None,
        }
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            available: false,
            reason: Some(reason.into()),
        }
    }
}

/// Every way a VCS call can fail, as a type the caller can act on.
///
/// The distinctions here are the ones a caller *behaves* differently about:
/// fall back to another backend ([`Self::NotAvailable`],
/// [`Self::NotARepository`]), ask the user to fix something concrete
/// ([`Self::IdentityUnset`], [`Self::NotAuthenticated`], [`Self::NoRemote`]),
/// resolve something first ([`Self::Dirty`], [`Self::Conflict`]), or retry
/// ([`Self::Timeout`]). [`Self::Command`] is the honest "git said no and we
/// will not pretend to understand why" case, and it carries the raw stderr so
/// a human can read it.
#[derive(Debug, Clone, thiserror::Error)]
pub enum VcsError {
    /// The binary is missing or unusable on this host.
    #[error("vcs unavailable: {0}")]
    NotAvailable(String),

    /// The path exists but is not inside a git repository. Per-path, so it is
    /// a call-time error and not a probe failure.
    #[error("{} is not a git repository", .0.display())]
    NotARepository(PathBuf),

    /// The working tree has changes the operation refuses to discard.
    #[error("{} has uncommitted changes: {details}", .path.display())]
    Dirty { path: PathBuf, details: String },

    /// Merge, rebase, cherry-pick, revert or checkout stopped on conflicting
    /// content: git's sentence, the paths it left unmerged, and the operation
    /// it left in progress (so the caller knows what to abort). Both extras are
    /// empty when the failure was classified without a repository to ask.
    #[error("conflict: {message}")]
    Conflict {
        message: String,
        paths: Vec<PathBuf>,
        in_progress: Option<interactive::InProgress>,
    },

    /// A fast-forward-only pull met local commits: the branch is `ahead` of
    /// its upstream by that many and `behind` by that many, and moving it
    /// would need a merge or a rebase the person has not asked for.
    #[error("not a fast-forward: {ahead} ahead and {behind} behind the upstream")]
    NotFastForward { ahead: u32, behind: u32 },

    /// A merge, rebase, cherry-pick or revert is already in progress here;
    /// finish or abort it before starting another operation.
    #[error("a {0:?} is in progress; resolve or abort it first")]
    InProgress(interactive::InProgress),

    /// `stash push` was asked with nothing it would save: a clean tree, only
    /// untracked files without *include untracked*, paths with no change, or
    /// a repository with no commit yet to stash against. Refused before any
    /// recovery ref is written, so nothing is left behind.
    #[error("nothing to stash")]
    NothingToStash,

    /// A discard whose every selected path has no change git can put back
    /// any more: gone since the list was read — an agent's file, deleted —
    /// or never tracked, which a discard does not touch. Refused before any
    /// recovery ref is written, as [`Self::NothingToStash`] is.
    #[error("nothing to discard")]
    NothingToDiscard,

    /// `stash@{index}` no longer holds `commit`: the list moved under the
    /// caller — a push or a drop here or in another worktree of the same
    /// repository — and acting by index alone would touch the wrong entry.
    /// `now` is what sits at that index today, when anything does.
    #[error("stash@{{{index}}} no longer holds {commit}; the stash list moved — reload it")]
    StashMoved {
        index: u32,
        commit: String,
        now: Option<git::CommitId>,
    },

    /// The named remote does not exist, or does not point at a repository.
    #[error("no usable remote: {0}")]
    NoRemote(String),

    /// Credentials are missing or refused. Because prompts are disabled this
    /// arrives immediately rather than as a hang.
    #[error("not authenticated: {0}")]
    NotAuthenticated(String),

    /// `user.name`/`user.email` are unset, so git will not write a commit.
    /// Its own variant because the fix is a specific instruction to the user —
    /// set who commits in this repository ([`git::set_local_identity`]) — and
    /// because this crate will not silently write global config for them.
    #[error("git identity unset: {0}")]
    IdentityUnset(String),

    /// Another git process — not this one's, whose writers queue per
    /// checkout — holds the repository's lock: an editor waiting on a commit
    /// message, an agent's own `git` in a terminal. Nothing was written; the
    /// caller tries again once that process has finished.
    #[error("another git process is using {}; try again once it has finished", path.display())]
    RepositoryBusy { path: PathBuf },

    /// An argument would have been read as an option or is otherwise unsafe
    /// to pass. Rejected before spawning anything.
    #[error("invalid {what}: {value:?}")]
    InvalidArg { what: String, value: String },

    /// The child exceeded its budget and was killed.
    #[error("{what} timed out after {secs}s")]
    Timeout { what: String, secs: u64 },

    /// An unclassified non-zero exit. `stderr` is kept verbatim — this crate
    /// does not invent a story about output it does not recognise.
    #[error("{what} (exit {code}): {stderr}")]
    Command {
        what: String,
        code: String,
        stderr: String,
    },

    #[error("{0}")]
    Other(String),
}

impl VcsError {
    pub fn other(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }

    /// True when the failure means "this host or this path cannot do it at
    /// all", which is the cue for a caller to fall back to another backend
    /// rather than surface a hard failure. Mirrors
    /// `bisa_iso::IsoError::is_unavailable`.
    pub const fn is_unavailable(&self) -> bool {
        matches!(self, Self::NotAvailable(_) | Self::NotARepository(_))
    }
}

pub type VcsResult<T> = Result<T, VcsError>;
