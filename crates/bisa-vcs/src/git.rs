//! `git`, as typed calls.
//!
//! Every function takes the repository (or worktree) path explicitly. There is
//! no "current repository", no cwd inherited from the process and no global
//! state, because the engine runs several workstreams at once and a process-wide
//! notion of "where we are" is a race waiting to happen.
//!
//! The free functions are the API; [`Git`] is the same API with the binary
//! path and [`Timeouts`] made explicit, which is how a test points at a stub
//! and how a caller lengthens the budget for a slow link.

use crate::operation::OperationFacts;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use crate::config_schema::ConfigScope;
use crate::exec::{self, describe, s, Output, Timeouts};
use crate::parse;
use crate::{ProbeResult, VcsError, VcsResult};

/// `%H %h %an %ae %at %s`, fields separated by US and records by RS. See
/// `parse::log` for why those two separators.
pub(crate) const LOG_FORMAT: &str = "--format=%H%x1f%h%x1f%an%x1f%ae%x1f%at%x1f%s%x1e";
/// The graph log: parents (`%P`) and decorations (`%D`) added.
pub(crate) const GRAPH_FORMAT: &str =
    "--format=%H%x1f%h%x1f%P%x1f%D%x1f%an%x1f%ae%x1f%at%x1f%s%x1e";
/// Where recovery points live (ide/04). Under `refs/` but outside `heads`,
/// `tags` and `remotes`, so no push, fetch or branch listing ever sees them.
pub const RECOVERY_PREFIX: &str = "refs/bisa/safety/";
/// Every ref the platform writes for itself, as `git log --exclude` takes it:
/// left out of the history's walk and its decorations.
pub const BISA_REFS_GLOB: &str = "refs/bisa/*";
/// The prefix of every ref the platform writes for itself.
pub const BISA_REFS_PREFIX: &str = "refs/bisa/";

/// The stash list as the reflog walk it is: `%H %gd %P %at %gs` per entry —
/// the commit, git's `stash@{n}` selector, the parents (a third one holds the
/// untracked files), when, and the reflog subject (`WIP on main: …` or
/// `On main: <message>`). Never with `--date`, which changes `%gd`'s shape.
pub(crate) const STASH_FORMAT: &str = "--format=%H%x1f%gd%x1f%P%x1f%at%x1f%gs%x1e";

/// What a recovery ref points at, read from its suffix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryKind {
    /// HEAD, or a deleted branch's or tag's tip: restoring is a checkout.
    Commit,
    /// A stash-shaped commit holding the index and the working tree
    /// (`.wip`): restoring checks out its base and applies it.
    Tree,
    /// A stash entry that was dropped or popped (`.stash`): restoring puts it
    /// back on the stash list and touches nothing in the tree.
    Stash,
}

impl RecoveryKind {
    /// The ref-name suffix that spells this kind.
    pub const fn suffix(self) -> &'static str {
        match self {
            RecoveryKind::Commit => "",
            RecoveryKind::Tree => ".wip",
            RecoveryKind::Stash => ".stash",
        }
    }

    /// The wire word — the same one serde writes.
    pub const fn as_str(self) -> &'static str {
        const STASH: &str = "stash";
        match self {
            RecoveryKind::Commit => "commit",
            RecoveryKind::Tree => "tree",
            RecoveryKind::Stash => STASH,
        }
    }
}

/// `refs/bisa/safety/<unix>-<op>[_pin][_<n>][.wip|.stash]` → `(at, op, kind)`.
pub(crate) fn parse_recovery_name(ref_name: &str) -> Option<(u64, String, RecoveryKind)> {
    let tail = ref_name.strip_prefix(RECOVERY_PREFIX)?;
    let (tail, kind) = if let Some(t) = tail.strip_suffix(".wip") {
        (t, RecoveryKind::Tree)
    } else if let Some(t) = tail.strip_suffix(".stash") {
        (t, RecoveryKind::Stash)
    } else {
        (tail, RecoveryKind::Commit)
    };
    let (at, op) = tail.split_once('-')?;
    // A tip saved beside a dirty tree carries `_pin`, and a second capture in
    // the same second carries a `_<n>` suffix before it so the names stay
    // unique; the op is the part before both.
    let op = op.strip_suffix("_pin").unwrap_or(op);
    let op = match op.rsplit_once('_') {
        Some((head, n)) if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => head,
        _ => op,
    };
    Some((at.parse().ok()?, op.to_string(), kind))
}

/// One commit in full: the graph fields, then the body (`%b`) last because
/// it may hold anything, including the separators.
pub(crate) const DETAIL_FORMAT: &str =
    "--format=%H%x1f%h%x1f%P%x1f%D%x1f%an%x1f%ae%x1f%at%x1f%s%x1f%b";

/// The empty tree. Diffing against it is how you ask "what would the first
/// commit contain?" in a repository whose HEAD is still unborn.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A commit object id. A newtype rather than a `String` so a caller cannot
/// pass a branch name where a resolved commit is meant.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct CommitId(String);

impl CommitId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// First 7 characters, for display only.
    pub fn short(&self) -> &str {
        let end = self
            .0
            .char_indices()
            .nth(7)
            .map_or(self.0.len(), |(i, _)| i);
        &self.0[..end]
    }
}

impl std::fmt::Display for CommitId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One line of `git log`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CommitSummary {
    pub id: CommitId,
    pub short: String,
    pub author: String,
    pub email: String,
    /// Author time, seconds since the epoch.
    pub timestamp: u64,
    pub subject: String,
}

/// A decoration on a commit, from `%D`: a branch, a remote branch, a tag, or
/// `HEAD` itself.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GraphRef {
    pub name: String,
    /// `head` · `branch` · `remote` · `tag`
    pub kind: String,
}

/// Which refs a graph log walks (ide/05): every local branch and tag, or
/// only what HEAD reaches — the one filter the History view offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefScope {
    All,
    Head,
}

impl RefScope {
    pub const ALL: [RefScope; 2] = [RefScope::All, RefScope::Head];

    pub fn as_str(self) -> &'static str {
        match self {
            RefScope::All => "all",
            RefScope::Head => "head",
        }
    }
}

impl std::str::FromStr for RefScope {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        RefScope::ALL
            .into_iter()
            .find(|r| r.as_str() == s)
            .ok_or_else(|| format!("unknown ref scope {s:?}: expected all or head"))
    }
}

/// One commit of the graph log (ide/05): its parents, first parent first,
/// and its decorations, with the summary fields the row shows.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GraphCommit {
    pub id: CommitId,
    pub short: String,
    pub parents: Vec<CommitId>,
    pub refs: Vec<GraphRef>,
    pub author: String,
    pub email: String,
    pub timestamp: u64,
    pub subject: String,
}

/// One commit in full, for the inspector: the graph fields plus the body and
/// the files it changed against its first parent.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CommitDetail {
    pub id: CommitId,
    pub short: String,
    pub parents: Vec<CommitId>,
    pub refs: Vec<GraphRef>,
    pub author: String,
    pub email: String,
    pub timestamp: u64,
    pub subject: String,
    pub body: String,
    pub files: Vec<FileChange>,
}

/// One local branch, from `for-each-ref refs/heads`, with where it stands:
/// its commits ahead of and behind its upstream (both 0 without one, or
/// when the upstream is gone), and whether it is merged into the branch the
/// listing was read against.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BranchInfo {
    pub name: String,
    pub head: CommitId,
    pub current: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream: Option<String>,
    pub subject: String,
    pub timestamp: u64,
    pub ahead: u32,
    pub behind: u32,
    pub merged: bool,
}

/// The side of a conflicted path a person takes whole: git's `ours` — the
/// branch the operation runs on, which during a rebase is the branch rebased
/// onto — or `theirs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictSide {
    Ours,
    Theirs,
}

/// One remote-tracking branch, from `for-each-ref refs/remotes`: what the
/// repository has heard of a remote's branch as of its last fetch — never a
/// network read. `remote/HEAD` is not a branch and is left out.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RemoteBranchInfo {
    pub remote: String,
    /// The branch's name on the remote, without the remote's prefix.
    pub name: String,
    pub head: CommitId,
    pub subject: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TagInfo {
    pub name: String,
    /// The commit the tag points at (peeled for annotated tags).
    pub target: CommitId,
    pub subject: String,
    pub timestamp: u64,
}

/// A name and an email, as git spells them in `user.name`/`user.email`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Ident {
    pub name: String,
    pub email: String,
}

impl std::fmt::Display for Ident {
    /// `Name <email>` — the shape git prints, `%an <%ae>`, and the one
    /// [`parse_ident`] reads back.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} <{}>", self.name, self.email)
    }
}

/// Where a repository's commit identity comes from. `Local` only when both
/// keys are set in the repository itself; `Global` when both resolve from
/// anywhere else (global, system, environment); `None` when a commit would
/// fail with [`crate::VcsError::IdentityUnset`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentitySource {
    Local,
    Global,
    None,
}

/// Who will author the next commit in a repository, and where that answer
/// comes from — plus the global pair, so a caller can offer to pin it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GitIdentity {
    pub name: Option<String>,
    pub email: Option<String>,
    pub source: IdentitySource,
    pub global: Option<Ident>,
}

/// Which layer a `git config --get` reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReadScope {
    Effective,
    Local,
    Global,
}

/// One schema key as the two writable layers hold it. `effective()` is what
/// git resolves for a repository: its own value, else the global one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ConfigEntry {
    pub key: String,
    pub local: Option<String>,
    pub global: Option<String>,
}

impl ConfigEntry {
    pub fn effective(&self) -> Option<&str> {
        self.local.as_deref().or(self.global.as_deref())
    }
}

/// Every schema key, in schema order. Read from no repository
/// (`local` is `None` throughout) or from one checkout.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GitConfigView {
    pub entries: Vec<ConfigEntry>,
}

impl GitConfigView {
    pub fn get(&self, key: &str) -> Option<&ConfigEntry> {
        self.entries.iter().find(|e| e.key == key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RemoteInfo {
    pub name: String,
    pub url: String,
}

/// One branch a remote holds, from `git ls-remote --heads` — the one probe
/// that proves a person can *reach* a remote, over whichever transport the
/// remote URL names, without moving a byte of anyone's content.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RemoteRef {
    pub sha: String,
    /// `refs/heads/main`, as the remote spells it.
    pub name: String,
}

/// `git --version`, comparable — the platform needs 2.36 for
/// `includeIf "hasconfig:…"` and says so instead of writing an include an
/// older git would ignore.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct GitVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl GitVersion {
    /// The first git that evaluates `hasconfig:remote.*.url:` includes.
    pub const HASCONFIG: GitVersion = GitVersion {
        major: 2,
        minor: 36,
        patch: 0,
    };

    /// `git version 2.50.1 (Apple Git-155)` → 2.50.1. Pure.
    pub fn parse(text: &str) -> Option<Self> {
        let word = text
            .split_whitespace()
            .find(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit()))?;
        let mut parts = word.split('.').map(|p| {
            p.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<u32>()
                .ok()
        });
        Some(Self {
            major: parts.next().flatten()?,
            minor: parts.next().flatten().unwrap_or(0),
            patch: parts.next().flatten().unwrap_or(0),
        })
    }

    pub fn supports_hasconfig(&self) -> bool {
        *self >= Self::HASCONFIG
    }
}

impl std::fmt::Display for GitVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// An `includeIf` entry of the person's global git config: the condition
/// (`hasconfig:remote.*.url:https://github.com/acme/**`) and the file it
/// pulls in.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Include {
    pub condition: String,
    pub path: PathBuf,
}

/// What a profile's git config file holds (ide/04 §Profiles by organization):
/// the author, and — each only when set — the SSH key `core.sshCommand`
/// names, the username git's credential helper is asked for over HTTPS, and
/// the code host account. A typed struct, not free keys, so nothing but these
/// five keys can ever be written to a file the platform includes.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProfileFile {
    /// The person's word for the profile (`bisa.label`) — one line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub name: String,
    pub email: String,
    /// The private key's path; the platform never opens it, git's ssh does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_key: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
}

impl ProfileFile {
    /// The `core.sshCommand` a key path becomes: this key and no other, so a
    /// profile bound to a key never lets ssh-agent offer a different one.
    pub fn ssh_command(key: &Path) -> String {
        format!("ssh -i {} -o IdentitiesOnly=yes", key.display())
    }

    /// The key a `core.sshCommand` written by [`Self::ssh_command`] names — or
    /// any `-i <path>` a person wrote by hand. `None` for a command without one.
    pub fn key_of_ssh_command(command: &str) -> Option<PathBuf> {
        let mut words = command.split_whitespace();
        while let Some(w) = words.next() {
            if w == "-i" {
                return words.next().map(PathBuf::from);
            }
            if let Some(rest) = w.strip_prefix("-i") {
                if !rest.is_empty() {
                    return Some(PathBuf::from(rest));
                }
            }
        }
        None
    }
}

/// A config value with the file it came from — `git config --show-origin`:
/// `origin` is git's own word, `file:/Users/ada/.gitconfig`, `command line`,
/// `standard input`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ConfigOrigin {
    pub value: String,
    pub origin: String,
}

impl ConfigOrigin {
    /// The file the value came from, when it came from one.
    pub fn file(&self) -> Option<PathBuf> {
        self.origin.strip_prefix("file:").map(PathBuf::from)
    }
}

/// A recovery point under `refs/bisa/safety/` (ide/04): what an
/// interactive operation saved before it ran.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RecoveryRef {
    /// The full ref name, as `restore` wants it.
    pub ref_name: String,
    pub commit: CommitId,
    /// The operation that wrote it: `checkout`, `rebase`, …
    pub op: String,
    /// The branch HEAD was on, when it was on one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// What `commit` is — a tip, the saved tree, or a dropped stash entry.
    pub kind: RecoveryKind,
    /// Seconds since the epoch, from the ref name.
    pub at: u64,
}

/// One entry of the stash list (ide/04 §Stash), as `git stash list` shows it
/// and as the reflog of `refs/stash` holds it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StashEntry {
    /// The entry's position, newest first — what `stash@{n}` means right now.
    /// It shifts under every push and drop, so a verb names `commit` too.
    pub index: u32,
    pub commit: CommitId,
    /// The branch HEAD was on when it was stashed; `None` when detached.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// The message the person gave (`On <branch>: <message>`); `None` for
    /// git's own `WIP on <branch>: …`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// The reflog subject verbatim — what `stash store -m` needs to put the
    /// entry back exactly as it was.
    pub subject: String,
    /// Seconds since the epoch.
    pub at: u64,
    /// The stash carries untracked files (a third parent).
    pub untracked: bool,
}

/// Which version of a file [`Git::blob`] reads: the commit at `HEAD`, the
/// index — the two sides of a staged change, and the left side of a
/// working-tree one — or any one revision by name, `Rev`: a commit and its
/// first parent are the two sides of a commit's change (ide/05). A
/// revision is validated as a ref is, so `sha^` is refused: a parent is a
/// commit id the caller already holds, never a suffix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlobRev {
    Head,
    Index,
    Rev(String),
}

/// The three sides of a conflicted path, from the index's stages (ide/04
/// §Conflicts, continued): stage 1 is the merge base, 2 ours, 3 theirs. A
/// side that has no stage — a file added on one side only, deleted on one —
/// is `None`. Bytes, never a lossy text, so a binary side is read honestly.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConflictBlobs {
    pub base: Option<Vec<u8>>,
    pub ours: Option<Vec<u8>>,
    pub theirs: Option<Vec<u8>>,
}

/// What kind of conflict an unmerged path is, from the two letters of git's
/// `u` status record — which side changed, added or deleted it. The words
/// are git's (`ours` is the current branch; under a rebase, the branch
/// rebased onto).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    /// `UU`: changed on both sides.
    BothModified,
    /// `AA`: added on both sides, differently.
    BothAdded,
    /// `DD`: deleted on both sides (a rename on one side, usually).
    BothDeleted,
    /// `DU`: deleted by ours, changed by theirs.
    DeletedByUs,
    /// `UD`: changed by ours, deleted by theirs.
    DeletedByThem,
    /// `AU`: added by ours, unmerged on theirs.
    AddedByUs,
    /// `UA`: added by theirs, unmerged on ours.
    AddedByThem,
}

impl ConflictKind {
    /// The kind for git's `XY` letters of an unmerged record; `None` for a
    /// pair git does not write.
    pub fn from_letters(x: char, y: char) -> Option<Self> {
        Some(match (x, y) {
            ('U', 'U') => Self::BothModified,
            ('A', 'A') => Self::BothAdded,
            ('D', 'D') => Self::BothDeleted,
            ('D', 'U') => Self::DeletedByUs,
            ('U', 'D') => Self::DeletedByThem,
            ('A', 'U') => Self::AddedByUs,
            ('U', 'A') => Self::AddedByThem,
            _ => return None,
        })
    }

    /// Whether a side still has the file: `ours` or `theirs` is absent when
    /// that side deleted it.
    pub fn deleted_by(self) -> Option<ConflictSide> {
        match self {
            Self::DeletedByUs => Some(ConflictSide::Ours),
            Self::DeletedByThem => Some(ConflictSide::Theirs),
            _ => None,
        }
    }
}

/// What `git merge-tree --write-tree` foresees for a merge of two commits,
/// before anything moves (ide/04 §Conflicts, continued): clean, or the
/// paths that would conflict.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MergePreview {
    pub clean: bool,
    pub paths: Vec<PathBuf>,
}

/// One line of `git blame --porcelain`: which commit last touched it, by whom
/// and when. `line` is the line's number in the file as it is now.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BlameLine {
    pub line: u32,
    pub commit: CommitId,
    pub short: String,
    pub author: String,
    /// Author time, seconds since the epoch. Zero for the uncommitted
    /// (`0000000…`) pseudo-commit git reports for lines not yet committed.
    pub timestamp: u64,
    pub summary: String,
    /// True for a line the working tree has that HEAD does not.
    pub uncommitted: bool,
}

/// A registered worktree, from `git worktree list --porcelain`.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorktreeEntry {
    pub path: PathBuf,
    /// Short branch name (`refs/heads/` stripped). `None` when detached.
    pub branch: Option<String>,
    pub head: Option<String>,
    pub detached: bool,
    pub bare: bool,
    pub locked: bool,
    pub lock_reason: Option<String>,
    /// git considers the registration stale; `worktree prune` would drop it.
    pub prunable: bool,
    pub prune_reason: Option<String>,
}

/// A repository's working state, from `git status --porcelain=v2 --branch`.
///
/// Counts, not paths: this is what a status badge and a "can I push?" check
/// need. Ask [`Git::status_files`] when you want the paths, or
/// [`Git::diff_stat`] when you want the line counts with them.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Status {
    /// Short branch name; `None` when HEAD is detached.
    pub branch: Option<String>,
    pub detached: bool,
    /// HEAD's commit; `None` in a repository with no commits yet.
    pub oid: Option<String>,
    pub upstream: Option<String>,
    /// Commits on HEAD that the upstream lacks. Zero without an upstream.
    pub ahead: u32,
    /// Commits on the upstream that HEAD lacks. Zero without an upstream.
    pub behind: u32,
    pub staged: u32,
    pub unstaged: u32,
    pub untracked: u32,
    pub conflicted: u32,
    pub is_clean: bool,
}

/// One path's line in `git status --porcelain=v2`, kept instead of counted.
///
/// The two letters are git's own and they mean different things: `index` is
/// what the **index** holds against HEAD, `worktree` what the **worktree**
/// holds against the index, `.` in either position meaning unmodified.
/// Keeping both rather than folding them into one verdict is the whole point
/// of a per-file surface — a file can be staged *and* modified again since,
/// and a user staging by hand needs to see that it is.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileStatus {
    /// Relative to the **repository root**, which is also what a pathspec
    /// wants — so a row from here goes straight back into [`Git::stage`].
    pub path: PathBuf,
    /// Where a rename or copy came from.
    pub old_path: Option<PathBuf>,
    /// Index state against HEAD; `.` when unmodified, `?` when untracked.
    pub index: char,
    /// Worktree state against the index; `.` when unmodified.
    pub worktree: char,
    /// git has never heard of this path. Both letters read `?`.
    pub untracked: bool,
    /// An unmerged entry, and what kind: the letters are the two sides of
    /// the conflict, not an index/worktree pair.
    pub conflict: Option<ConflictKind>,
}

impl FileStatus {
    /// An unmerged entry.
    pub fn is_conflicted(&self) -> bool {
        self.conflict.is_some()
    }

    /// The index holds something for this path that HEAD does not.
    pub fn is_staged(&self) -> bool {
        !self.untracked && !self.is_conflicted() && self.index != '.'
    }

    /// The worktree differs from the index — a file git has never seen counts,
    /// because it is unstaged by definition.
    pub fn is_unstaged(&self) -> bool {
        self.untracked || (!self.is_conflicted() && self.worktree != '.')
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    /// The entry changed type, e.g. a file became a symlink.
    TypeChanged,
    Unmerged,
    Unknown,
}

impl ChangeKind {
    pub(crate) fn from_status_letter(letter: Option<u8>) -> Self {
        match letter {
            Some(b'A') => Self::Added,
            Some(b'M') => Self::Modified,
            Some(b'D') => Self::Deleted,
            Some(b'R') => Self::Renamed,
            Some(b'C') => Self::Copied,
            Some(b'T') => Self::TypeChanged,
            Some(b'U') => Self::Unmerged,
            _ => Self::Unknown,
        }
    }
}

/// One changed file, merged from `--numstat` and `--name-status`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileChange {
    pub path: PathBuf,
    /// Present for renames and copies.
    pub old_path: Option<PathBuf>,
    pub kind: ChangeKind,
    pub insertions: u64,
    pub deletions: u64,
    /// git reported `-`/`-` line counts, so the counts above are meaningless.
    pub binary: bool,
}

// ---------------------------------------------------------------------------
// The handle
// ---------------------------------------------------------------------------

/// A credential git's helpers hold for a host, as `git credential fill`
/// answers it. `Debug` redacts the password: a credential that lands in a log
/// or a panic message is a leak, whatever the reason.
#[derive(Clone, PartialEq, Eq)]
pub struct Credential {
    pub username: String,
    pub password: String,
}

impl std::fmt::Debug for Credential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credential")
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

/// One word for a `credential.helper` value: `osxkeychain` stays, `!gh auth
/// git-credential` is `gh`, `/usr/local/bin/git-credential-manager` is
/// `manager`, `store --file …` is `store`. The last path segment of the first
/// word, less git's own prefix.
pub fn summarise_helper(raw: &str) -> String {
    let first = raw
        .trim()
        .trim_start_matches('!')
        .split_whitespace()
        .next()
        .unwrap_or("");
    let name = first.rsplit('/').next().unwrap_or(first);
    let name = name.strip_prefix("git-credential-").unwrap_or(name);
    if name.starts_with("manager") {
        return "manager".to_string();
    }
    if name.is_empty() {
        raw.trim().to_string()
    } else {
        name.to_string()
    }
}

/// The environment [`Git::command`] pins; [`Git::with_env`] refuses to override it.
const HARDENED_ENV: [&str; 6] = [
    "GIT_TERMINAL_PROMPT",
    "GIT_ASKPASS",
    "SSH_ASKPASS",
    "GIT_PAGER",
    "LC_ALL",
    "GIT_OPTIONAL_LOCKS",
];

fn is_hardened(key: &OsStr) -> bool {
    HARDENED_ENV.iter().any(|h| OsStr::new(h) == key)
}

/// A configured `git`. Cheap to construct; [`Default`] is what the free
/// functions use.
#[derive(Debug, Clone)]
pub struct Git {
    bin: OsString,
    timeouts: Timeouts,
    /// Extra environment for every child, on top of the hardening in
    /// [`Self::command`]. A test seam — `GIT_CONFIG_GLOBAL`, `GIT_CONFIG_NOSYSTEM`
    /// — so a test can hide the developer's own global config on one handle
    /// without touching the process environment other tests share; and the
    /// engine's proxy, handed to git the way curl reads it.
    env: Vec<(OsString, OsString)>,
    /// Names removed from every child's environment — a proxy the node
    /// inherited that the person said not to use.
    env_removed: Vec<OsString>,
    /// `-c key=value` pairs on every invocation, after `--no-pager` —
    /// `http.version` when the platform speaks HTTP/1.1 only.
    config: Vec<(String, String)>,
}

impl Default for Git {
    fn default() -> Self {
        Self {
            bin: OsString::from("git"),
            timeouts: Timeouts::default(),
            env: Vec::new(),
            env_removed: Vec::new(),
            config: Vec::new(),
        }
    }
}

pub(crate) fn shared() -> &'static Git {
    static SHARED: OnceLock<Git> = OnceLock::new();
    SHARED.get_or_init(Git::default)
}

impl Git {
    pub fn new() -> Self {
        Self::default()
    }

    /// The `git` this handle runs — `git` on `PATH` by default. A test names
    /// a program that does not exist to stand where a machine with no git
    /// stands; `Command::spawn` resolves a bare name against the process's
    /// own `PATH`, so a child environment could never do this.
    pub fn with_program(mut self, program: impl Into<OsString>) -> Self {
        self.bin = program.into();
        self
    }

    /// Set one environment variable on every `git` this handle runs. Applied
    /// after the hardening, so a caller cannot un-harden a child by accident
    /// — the hardened names are refused.
    pub fn with_env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        let key = key.into();
        if !is_hardened(&key) {
            self.env.push((key, value.into()));
        }
        self
    }

    /// Remove one environment variable from every `git` this handle runs —
    /// applied after the hardening, which it cannot touch either.
    pub fn without_env(mut self, key: impl Into<OsString>) -> Self {
        let key = key.into();
        if !is_hardened(&key) {
            self.env_removed.push(key);
        }
        self
    }

    /// One `-c key=value` on every `git` this handle runs.
    pub fn with_config(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.config.push((key.into(), value.into()));
        self
    }

    /// Build a hardened `git` invocation.
    ///
    /// The environment settings are not conveniences, they are the difference
    /// between a failed call and a wedged scheduler: with prompts disabled and
    /// stdin closed (in `exec::run`) a missing credential is an immediate
    /// non-zero exit we classify as [`VcsError::NotAuthenticated`]. `LC_ALL=C`
    /// pins the language of that stderr so the classification below is stable
    /// on a French laptop.
    ///
    /// `GIT_OPTIONAL_LOCKS=0` is the one that keeps a *read* honest: a
    /// `status` or a `diff` refreshes the index's stat cache and, left to
    /// itself, takes `index.lock` to write it back — and a commit that asks
    /// for the lock in that instant fails. With it off, a read never locks;
    /// the writes, which queue per checkout (`repo_lock`), refresh instead.
    ///
    /// Note what is *not* here: no `--no-verify`, and no `core.hooksPath`
    /// override. The user's hooks run on commits and pushes made through this
    /// crate, exactly as they would by hand.
    fn command(&self, cwd: Option<&Path>) -> Command {
        let mut cmd = Command::new(&self.bin);
        cmd.arg("--no-pager");
        for (key, value) in &self.config {
            cmd.arg("-c").arg(format!("{key}={value}"));
        }
        if let Some(dir) = cwd {
            cmd.arg("-C").arg(dir);
        }
        cmd.env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_ASKPASS", "")
            .env("SSH_ASKPASS", "")
            .env("GIT_PAGER", "cat")
            .env("LC_ALL", "C")
            .env("GIT_OPTIONAL_LOCKS", "0");
        for (k, v) in &self.env {
            cmd.env(k, v);
        }
        for k in &self.env_removed {
            cmd.env_remove(k);
        }
        cmd
    }

    /// Run and hand back the raw outcome, non-zero exits included. For the
    /// handful of callers where "git said no" is a legitimate answer.
    pub(crate) fn capture(
        &self,
        cwd: Option<&Path>,
        args: &[OsString],
        network: bool,
    ) -> VcsResult<Output> {
        let mut cmd = self.command(cwd);
        cmd.args(args);
        let what = describe(&self.bin, args);
        exec::run(cmd, &what, self.timeouts.pick(network))
    }

    /// Run and require success, classifying failure into a [`VcsError`].
    pub(crate) fn run(
        &self,
        cwd: Option<&Path>,
        args: &[OsString],
        network: bool,
    ) -> VcsResult<Vec<u8>> {
        let out = self.capture(cwd, args, network)?;
        if out.success {
            Ok(out.stdout)
        } else {
            Err(classify(&describe(&self.bin, args), cwd, &out))
        }
    }

    /// [`Self::run`] with variables of its own in the child's environment —
    /// a snapshot's private `GIT_INDEX_FILE` (`snapshot.rs`). Local budget.
    pub(crate) fn run_env(
        &self,
        cwd: &Path,
        args: &[OsString],
        env: &[(&OsStr, &OsStr)],
    ) -> VcsResult<Vec<u8>> {
        let mut cmd = self.command(Some(cwd));
        cmd.args(args);
        for (key, value) in env {
            cmd.env(key, value);
        }
        let what = describe(&self.bin, args);
        let out = exec::run(cmd, &what, self.timeouts.pick(false))?;
        if out.success {
            Ok(out.stdout)
        } else {
            Err(classify(&what, Some(cwd), &out))
        }
    }

    /// [`Self::capture`] with bytes on the child's stdin — the raw outcome,
    /// non-zero exits included. Local budget: nothing that reads from us talks
    /// to a remote.
    pub(crate) fn capture_input(
        &self,
        cwd: Option<&Path>,
        args: &[OsString],
        input: Vec<u8>,
    ) -> VcsResult<Output> {
        let mut cmd = self.command(cwd);
        cmd.args(args);
        let what = describe(&self.bin, args);
        exec::run_with_input(cmd, &what, self.timeouts.pick(false), Some(input))
    }

    /// [`Self::run`] with bytes on the child's stdin.
    pub(crate) fn run_input(
        &self,
        cwd: Option<&Path>,
        args: &[OsString],
        input: Vec<u8>,
    ) -> VcsResult<Vec<u8>> {
        let out = self.capture_input(cwd, args, input)?;
        if out.success {
            Ok(out.stdout)
        } else {
            Err(classify(&describe(&self.bin, args), cwd, &out))
        }
    }

    // -- writes -------------------------------------------------------------
    //
    // A write holds its checkout (`repo_lock::hold`) for the life of the
    // child: two writers of this process on one repository queue instead of
    // meeting git's `index.lock` and failing. A write with no checkout — a
    // global config edit, a clone into a path that does not exist yet —
    // holds the path it is about.

    /// [`Self::run`] for an invocation that writes the index, the refs, the
    /// config or the tree: the checkout is held while it runs.
    pub(crate) fn write(
        &self,
        cwd: Option<&Path>,
        args: &[OsString],
        network: bool,
    ) -> VcsResult<Vec<u8>> {
        let out = self.write_capture(cwd, args, network)?;
        if out.success {
            Ok(out.stdout)
        } else {
            Err(classify(&describe(&self.bin, args), cwd, &out))
        }
    }

    /// [`Self::capture`] for a write — the raw outcome, the checkout held.
    pub(crate) fn write_capture(
        &self,
        cwd: Option<&Path>,
        args: &[OsString],
        network: bool,
    ) -> VcsResult<Output> {
        let _held = cwd.map(crate::repo_lock::hold);
        self.capture(cwd, args, network)
    }

    /// [`Self::run_input`] for a write — `git apply` and its kin.
    pub(crate) fn write_input(
        &self,
        cwd: Option<&Path>,
        args: &[OsString],
        input: Vec<u8>,
    ) -> VcsResult<Vec<u8>> {
        let _held = cwd.map(crate::repo_lock::hold);
        self.run_input(cwd, args, input)
    }

    /// A write whose checkout is not its `cwd` — a clone or a worktree into
    /// `at`, run from nowhere or from another repository.
    pub(crate) fn write_at(
        &self,
        held: &Path,
        cwd: Option<&Path>,
        args: &[OsString],
        network: bool,
    ) -> VcsResult<Vec<u8>> {
        let _held = crate::repo_lock::hold(held);
        self.run(cwd, args, network)
    }

    pub(crate) fn run_text(
        &self,
        cwd: Option<&Path>,
        args: &[OsString],
        network: bool,
    ) -> VcsResult<String> {
        Ok(String::from_utf8_lossy(&self.run(cwd, args, network)?)
            .trim()
            .to_string())
    }

    // -- availability -------------------------------------------------------

    /// Host-level: is there a usable `git`? Says nothing about any particular
    /// path — [`Self::is_repo`] answers that, and every repository-scoped call
    /// can still fail [`VcsError::NotARepository`].
    pub fn probe(&self) -> ProbeResult {
        match self.capture(None, &[s("--version")], false) {
            Ok(out) if out.success => ProbeResult::available(),
            Ok(out) => ProbeResult::unavailable(format!("`git --version` failed: {}", out.code)),
            Err(e) => ProbeResult::unavailable(format!("git not usable: {e}")),
        }
    }

    /// Is `path` inside a git repository? A plain bool: callers use this to
    /// choose a code path, not to report an error.
    pub fn is_repo(&self, path: &Path) -> bool {
        self.capture(Some(path), &[s("rev-parse"), s("--git-dir")], false)
            .map(|o| o.success)
            .unwrap_or(false)
    }

    /// Absolute path of the working tree's root.
    pub fn repo_root(&self, path: &Path) -> VcsResult<PathBuf> {
        self.run_text(
            Some(path),
            &[
                s("rev-parse"),
                s("--path-format=absolute"),
                s("--show-toplevel"),
            ],
            false,
        )
        .map(PathBuf::from)
    }

    /// Absolute path of the *common* git directory — the one shared by every
    /// worktree of a repository. This is how a worktree finds its main
    /// repository without being told where it is.
    pub fn common_dir(&self, path: &Path) -> VcsResult<PathBuf> {
        self.run_text(
            Some(path),
            &[
                s("rev-parse"),
                s("--path-format=absolute"),
                s("--git-common-dir"),
            ],
            false,
        )
        .map(PathBuf::from)
    }

    /// The branch new work should branch from.
    ///
    /// `origin/HEAD` first, because when the clone recorded it that *is* the
    /// code host's default branch and no local checkout can contradict it. When it
    /// is absent (a `git init`, or a clone made with `--no-checkout`), fall
    /// back to the branch HEAD points at — which is also the right answer in a
    /// repository with no commits, where HEAD is unborn but named.
    /// Whether a local branch of that name exists.
    pub fn branch_exists(&self, path: &Path, name: &str) -> VcsResult<bool> {
        validate_ref("branch", name)?;
        let out = self.capture(
            Some(path),
            &[
                s("rev-parse"),
                s("-q"),
                s("--verify"),
                s(format!("refs/heads/{name}")),
            ],
            false,
        )?;
        Ok(out.success)
    }

    /// The branch `origin/HEAD` points at, when the remote has said — `None`
    /// when no origin, or an origin that never told (a bare repository a
    /// `push -u` created has no HEAD of its own).
    pub fn remote_default_branch(&self, path: &Path) -> VcsResult<Option<String>> {
        let origin_head = self.capture(
            Some(path),
            &[
                s("symbolic-ref"),
                s("--quiet"),
                s("refs/remotes/origin/HEAD"),
            ],
            false,
        )?;
        Ok(origin_head
            .success
            .then(|| origin_head.stdout_text())
            .and_then(|head| {
                head.strip_prefix("refs/remotes/origin/")
                    .map(str::to_string)
            }))
    }

    /// The branch the checkout treats as its trunk: `origin/HEAD` when the
    /// remote has said, else the branch checked out now.
    pub fn default_branch(&self, path: &Path) -> VcsResult<String> {
        if let Some(name) = self.remote_default_branch(path)? {
            return Ok(name);
        }

        let head = self.capture(
            Some(path),
            &[s("symbolic-ref"), s("--short"), s("HEAD")],
            false,
        )?;
        if head.success {
            return Ok(head.stdout_text());
        }
        if !self.is_repo(path) {
            return Err(VcsError::NotARepository(path.to_path_buf()));
        }
        Err(VcsError::other(format!(
            "{}: HEAD is detached and no origin/HEAD is recorded",
            path.display()
        )))
    }

    // -- creating repositories ---------------------------------------------

    /// `git init`. Creates `path` if it does not exist.
    ///
    /// This crate never runs this unprompted; it is here so the platform can
    /// do it on a user's explicit action.
    pub fn init(&self, path: &Path) -> VcsResult<()> {
        std::fs::create_dir_all(path)
            .map_err(|e| VcsError::other(format!("create {}: {e}", path.display())))?;
        self.write(Some(path), &[s("init"), s("--quiet")], false)?;
        Ok(())
    }

    /// `git clone`, optionally shallow. Network op, so it gets the network
    /// budget.
    pub fn clone(&self, url: &str, dest: &Path, depth: Option<u32>) -> VcsResult<()> {
        validate_value("clone url", url)?;
        let dest = absolutize(dest)?;
        let mut args = vec![s("clone"), s("--quiet")];
        if let Some(depth) = depth {
            args.push(s("--depth"));
            args.push(s(depth.to_string()));
        }
        // `--` so a URL or destination can never be read as an option, even
        // though validate_value already refused a leading dash.
        args.push(s("--"));
        args.push(s(url));
        args.push(s(&dest));
        self.write_at(&dest, None, &args, true)?;
        Ok(())
    }

    // -- worktrees ----------------------------------------------------------

    /// Create a worktree at `path` on a **new** branch `branch` starting at
    /// `base`. Fails if the branch already exists — use
    /// [`Self::worktree_add_existing`] when you mean to check one out.
    pub fn worktree_add(
        &self,
        repo: &Path,
        path: &Path,
        branch: &str,
        base: &str,
    ) -> VcsResult<()> {
        validate_ref("branch", branch)?;
        validate_value("base", base)?;
        let path = absolutize(path)?;
        self.write(
            Some(repo),
            &[
                s("worktree"),
                s("add"),
                s("--quiet"),
                s("-b"),
                s(branch),
                s(&path),
                s(base),
            ],
            false,
        )?;
        Ok(())
    }

    /// Create a worktree at `path` checking out an existing branch.
    pub fn worktree_add_existing(&self, repo: &Path, path: &Path, branch: &str) -> VcsResult<()> {
        validate_ref("branch", branch)?;
        let path = absolutize(path)?;
        self.write(
            Some(repo),
            &[s("worktree"), s("add"), s("--quiet"), s(&path), s(branch)],
            false,
        )?;
        Ok(())
    }

    /// Create a worktree at `path` on a **new** local branch `branch` that
    /// starts at, and tracks, `<remote>/<remote_branch>` — the way a branch
    /// someone pushed from another machine, or a pull request's head, is
    /// taken up here. The remote-tracking ref must already be known: fetch
    /// first ([`Self::fetch_branch`]). Fails if `branch` already exists.
    pub fn worktree_add_tracking(
        &self,
        repo: &Path,
        path: &Path,
        branch: &str,
        remote: &str,
        remote_branch: &str,
    ) -> VcsResult<()> {
        validate_ref("branch", branch)?;
        validate_ref("remote", remote)?;
        validate_ref("remote branch", remote_branch)?;
        let path = absolutize(path)?;
        self.write(
            Some(repo),
            &[
                s("worktree"),
                s("add"),
                s("--quiet"),
                s("--track"),
                s("-b"),
                s(branch),
                s(&path),
                s(format!("{remote}/{remote_branch}")),
            ],
            false,
        )?;
        Ok(())
    }

    /// Create a detached worktree — no branch, so nothing to clean up in the
    /// ref namespace. This is what ephemeral isolation wants.
    pub fn worktree_add_detached(
        &self,
        repo: &Path,
        path: &Path,
        commitish: Option<&str>,
    ) -> VcsResult<()> {
        let path = absolutize(path)?;
        let mut args = vec![
            s("worktree"),
            s("add"),
            s("--quiet"),
            s("--detach"),
            s(&path),
        ];
        if let Some(c) = commitish {
            validate_value("commit-ish", c)?;
            args.push(s(c));
        }
        self.write(Some(repo), &args, false)?;
        Ok(())
    }

    pub fn worktree_list(&self, repo: &Path) -> VcsResult<Vec<WorktreeEntry>> {
        let text = self.run_text(
            Some(repo),
            &[s("worktree"), s("list"), s("--porcelain")],
            false,
        )?;
        Ok(parse::worktree_list(&text))
    }

    /// Remove a worktree and its registration.
    ///
    /// Runs against the repository's *common* git dir rather than from inside
    /// the tree being deleted, so the command does not saw off the branch it
    /// is sitting on. Without `force`, a worktree with uncommitted changes is
    /// refused — reported as [`VcsError::Dirty`], not as an opaque exit code.
    pub fn worktree_remove(&self, path: &Path, force: bool) -> VcsResult<()> {
        let common = self.common_dir(path)?;
        let path = std::fs::canonicalize(path)
            .map_err(|e| VcsError::other(format!("canonicalize {}: {e}", path.display())))?;
        let mut args = vec![s("--git-dir"), s(&common), s("worktree"), s("remove")];
        if force {
            args.push(s("--force"));
        }
        args.push(s("--"));
        args.push(s(&path));
        self.write_at(&path, None, &args, false)?;
        Ok(())
    }

    /// Drop registrations whose directories are gone. `repo` may be any path
    /// git can discover a repository from, including a bare git directory —
    /// which is what a caller has left after deleting a worktree by hand.
    pub fn worktree_prune(&self, repo: &Path) -> VcsResult<()> {
        self.write(Some(repo), &[s("worktree"), s("prune")], false)?;
        Ok(())
    }

    // -- inspection ---------------------------------------------------------

    pub fn status(&self, path: &Path) -> VcsResult<Status> {
        Ok(self.status_raw(path, "normal")?.0)
    }

    /// The same status, kept per file instead of counted.
    ///
    /// A second invocation rather than a second return value from
    /// [`Self::status`], because the two ask git a genuinely different
    /// question: this one passes `--untracked-files=all` so a surface can
    /// offer to stage *one* new file, while [`Self::status`] keeps `normal`
    /// so a directory of a thousand new files stays a single line in a badge.
    /// Same parser, so the rows and the counts can never disagree about what
    /// a record means.
    ///
    /// Paths come back relative to the repository root, which is what
    /// [`Self::stage`] wants — call both with the same `path`.
    pub fn status_files(&self, path: &Path) -> VcsResult<Vec<FileStatus>> {
        Ok(self.status_raw(path, "all")?.1)
    }

    fn status_raw(&self, path: &Path, untracked: &str) -> VcsResult<(Status, Vec<FileStatus>)> {
        let raw = self.run(
            Some(path),
            &[
                s("status"),
                s("--porcelain=v2"),
                s("--branch"),
                s("-z"),
                s(format!("--untracked-files={untracked}")),
            ],
            false,
        )?;
        Ok(parse::status(&raw))
    }

    /// Unified diff text. `staged` selects the index against HEAD; otherwise
    /// the working tree against the index. For "everything not yet committed"
    /// use [`Self::diff_head`].
    pub fn diff(&self, path: &Path, staged: bool) -> VcsResult<String> {
        let mut args = vec![s("diff"), s("--no-color")];
        if staged {
            args.push(s("--cached"));
            args.push(s(self.baseline(path)?));
        }
        Ok(String::from_utf8_lossy(&self.run(Some(path), &args, false)?).into_owned())
    }

    /// Apply `patch` to the index and nothing else: `git apply --cached`.
    ///
    /// This is how one hunk — or a few lines of one — is staged without the
    /// rest of the file. `reverse` takes the same hunk back out of the index,
    /// which is `unstage` at hunk granularity. Both leave the working tree
    /// exactly as it was, so this is in the safe tier with `stage` and
    /// `unstage`. `--recount` lets a patch a client assembled line by line
    /// carry stale hunk counts; the hunk *content* still has to match, and a
    /// mismatch is [`VcsError`], not a partial application.
    pub fn apply_cached(&self, path: &Path, patch: &str, reverse: bool) -> VcsResult<()> {
        if patch.trim().is_empty() {
            return Err(VcsError::InvalidArg {
                what: "patch".into(),
                value: "(empty)".into(),
            });
        }
        let mut args = vec![s("apply"), s("--cached"), s("--recount")];
        if reverse {
            args.push(s("--reverse"));
        }
        args.push(s("-"));
        self.write_input(Some(path), &args, patch.as_bytes().to_vec())?;
        Ok(())
    }

    /// `git blame --porcelain` of one file, optionally one line range
    /// (1-based, inclusive). Read-only.
    pub fn blame(
        &self,
        path: &Path,
        pathspec: &str,
        range: Option<(u32, u32)>,
    ) -> VcsResult<Vec<BlameLine>> {
        // Validated like every pathspec, but passed bare: `blame` takes one
        // file name, not a pathspec, and reads `:(...)` magic as a file name.
        literal_pathspec("pathspec", pathspec)?;
        // No commit, no line to attribute: a repository with no commits yet
        // answers no lines, and the gutter has nothing to decorate.
        if self.head(path).is_err() {
            if !self.is_repo(path) {
                return Err(VcsError::NotARepository(path.to_path_buf()));
            }
            return Ok(Vec::new());
        }
        let mut args = vec![s("blame"), s("--porcelain")];
        if let Some((start, end)) = range {
            if start == 0 || end < start {
                return Err(VcsError::InvalidArg {
                    what: "line range".into(),
                    value: format!("{start}..{end}"),
                });
            }
            args.push(s(format!("-L{start},{end}")));
        }
        args.push(s("--"));
        args.push(s(pathspec));
        let text = String::from_utf8_lossy(&self.run(Some(path), &args, false)?).into_owned();
        Ok(parse::blame(&text))
    }

    /// The commits that touched one path, newest first, following renames.
    /// A repository with no commits yet answers an empty list: nothing has
    /// touched anything, which is a fact about the file, not a fault.
    pub fn file_history(
        &self,
        path: &Path,
        pathspec: &str,
        limit: usize,
    ) -> VcsResult<Vec<CommitSummary>> {
        let specs = literal_pathspecs("pathspec", &[pathspec])?;
        if self.head(path).is_err() {
            if !self.is_repo(path) {
                return Err(VcsError::NotARepository(path.to_path_buf()));
            }
            return Ok(Vec::new());
        }
        let mut args = vec![
            s("log"),
            s("--follow"),
            s(LOG_FORMAT),
            s(format!("--max-count={limit}")),
            s("--"),
        ];
        args.extend(specs.into_iter().map(s));
        let text = self.run_text(Some(path), &args, false)?;
        Ok(parse::log(&text))
    }
    /// Unified diff of the working tree *and* index against HEAD — the patch
    /// that a single commit would record.
    pub fn diff_head(&self, path: &Path) -> VcsResult<String> {
        let base = self.baseline(path)?;
        let args = [s("diff"), s("--no-color"), s(base)];
        Ok(String::from_utf8_lossy(&self.run(Some(path), &args, false)?).into_owned())
    }

    /// What the branch at HEAD changed since it left `base` — `base...HEAD`,
    /// three dots: the diff from their merge base, so what landed on `base`
    /// since does not read as the branch undoing it. The change a pull
    /// request proposes. Committed work only; the working tree is not read.
    pub fn branch_diff(&self, path: &Path, base: &str) -> VcsResult<String> {
        validate_ref("base", base)?;
        let args = [s("diff"), s("--no-color"), s(format!("{base}...HEAD"))];
        Ok(String::from_utf8_lossy(&self.run(Some(path), &args, false)?).into_owned())
    }

    /// One file's patch. `staged` selects the index against HEAD; otherwise
    /// the working tree against the index — the same two questions
    /// [`Self::diff`] asks, narrowed to a path.
    ///
    /// An **untracked** file has no patch here, and deliberately so: `git
    /// diff` cannot show a file git has never seen without staging it first,
    /// and reading a diff must not stage anything. Such a path comes back as
    /// an empty string; [`Self::status_files`] is what says it is new.
    pub fn diff_file(&self, path: &Path, pathspec: &str, staged: bool) -> VcsResult<String> {
        let spec = literal_pathspec("pathspec", pathspec)?;
        let mut args = vec![s("diff"), s("--no-color")];
        if staged {
            args.push(s("--cached"));
            args.push(s(self.baseline(path)?));
        }
        args.push(s("--"));
        args.push(s(spec));
        Ok(String::from_utf8_lossy(&self.run(Some(path), &args, false)?).into_owned())
    }

    /// Per-file change summary against HEAD.
    ///
    /// Two invocations because git will not give both in one: `--numstat` has
    /// the line counts and `--name-status` has the operation. They are merged
    /// on the new path, and a file the second call did not classify is
    /// reported as [`ChangeKind::Modified`], which is what a numstat row
    /// without a rename marker means.
    pub fn diff_stat(&self, path: &Path) -> VcsResult<Vec<FileChange>> {
        let base = self.baseline(path)?;
        let nums = parse::numstat(&self.run(
            Some(path),
            &[s("diff"), s("-z"), s("--numstat"), s(&base)],
            false,
        )?);
        let kinds = parse::name_status(&self.run(
            Some(path),
            &[s("diff"), s("-z"), s("--name-status"), s(&base)],
            false,
        )?);

        Ok(merge_stats(nums, kinds))
    }

    /// Local branches with their tips, newest first, each with its standing:
    /// ahead/behind its upstream from `%(upstream:track)`, and `merged` when
    /// `merged_into` names a branch it is reachable from (`--merged`; a name
    /// the repository lacks marks none). Read-only.
    pub fn branch_list(
        &self,
        path: &Path,
        merged_into: Option<&str>,
    ) -> VcsResult<Vec<BranchInfo>> {
        let text = self.run_text(
            Some(path),
            &[
                s("for-each-ref"),
                s("--sort=-committerdate"),
                s("--format=%(refname:short)%1f%(objectname)%1f%(HEAD)%1f%(upstream:short)%1f%(committerdate:unix)%1f%(upstream:track,nobracket)%1f%(subject)"),
                s("refs/heads/"),
            ],
            false,
        )?;
        let merged: Vec<String> = match merged_into {
            Some(base) => {
                validate_ref("merged into", base)?;
                self.capture(
                    Some(path),
                    &[
                        s("for-each-ref"),
                        s("--format=%(refname:short)"),
                        s(format!("--merged={base}")),
                        s("refs/heads/"),
                    ],
                    false,
                )
                .ok()
                .filter(|o| o.success)
                .map(|o| {
                    o.stdout_text()
                        .lines()
                        .map(|l| l.trim().to_string())
                        .filter(|l| !l.is_empty())
                        .collect()
                })
                .unwrap_or_default()
            }
            None => Vec::new(),
        };
        Ok(text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| {
                let mut f = l.split('\u{1f}');
                let name = f.next()?.to_string();
                let head = CommitId::new(f.next()?);
                let current = f.next()? == "*";
                let upstream = f.next().filter(|u| !u.is_empty()).map(str::to_string);
                let timestamp = f.next()?.trim().parse().unwrap_or(0);
                let (ahead, behind) = parse::upstream_track(f.next().unwrap_or(""));
                let subject = f.next().unwrap_or("").to_string();
                Some(BranchInfo {
                    merged: merged.iter().any(|m| m == &name),
                    name,
                    head,
                    current,
                    upstream,
                    subject,
                    timestamp,
                    ahead,
                    behind,
                })
            })
            .collect())
    }

    /// The paths that differ between two commits — `git diff --name-only
    /// <from> <to>` — at most `limit`, in git's order: what a branch that
    /// moved changed. Read-only.
    pub fn changed_paths(
        &self,
        path: &Path,
        from: &str,
        to: &str,
        limit: usize,
    ) -> VcsResult<Vec<String>> {
        validate_ref("from", from)?;
        validate_ref("to", to)?;
        let out = self.run(
            Some(path),
            &[s("diff"), s("-z"), s("--name-only"), s(from), s(to)],
            false,
        )?;
        Ok(String::from_utf8_lossy(&out)
            .split('\0')
            .filter(|p| !p.is_empty())
            .take(limit)
            .map(str::to_string)
            .collect())
    }

    /// The commits `from` has that `to` lacks — `to..from` — newest first,
    /// at most `limit`: what a cherry-pick from a branch offers, what an
    /// interactive rebase replays. Read-only.
    pub fn commits_between(
        &self,
        path: &Path,
        from: &str,
        to: &str,
        limit: usize,
    ) -> VcsResult<Vec<CommitSummary>> {
        validate_ref("from", from)?;
        validate_ref("to", to)?;
        let text = self.run_text(
            Some(path),
            &[
                s("log"),
                s(LOG_FORMAT),
                s(format!("--max-count={limit}")),
                s(format!("{to}..{from}")),
            ],
            false,
        )?;
        Ok(parse::log(&text))
    }

    /// The checkout's own git directory — a worktree's private one under
    /// `.git/worktrees/` — as an absolute path.
    pub fn git_dir(&self, path: &Path) -> VcsResult<PathBuf> {
        let dir = self.run_text(Some(path), &[s("rev-parse"), s("--git-dir")], false)?;
        let git_dir = Path::new(dir.trim());
        Ok(if git_dir.is_absolute() {
            git_dir.to_path_buf()
        } else {
            path.join(git_dir)
        })
    }

    /// Remote-tracking branches with their tips, newest first, as of the
    /// last fetch. Read-only, and local: it never asks the remote.
    pub fn remote_branch_list(&self, path: &Path) -> VcsResult<Vec<RemoteBranchInfo>> {
        let text = self.run_text(
            Some(path),
            &[
                s("for-each-ref"),
                s("--sort=-committerdate"),
                s("--format=%(refname:short)%1f%(objectname)%1f%(committerdate:unix)%1f%(subject)"),
                s("refs/remotes/"),
            ],
            false,
        )?;
        Ok(text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| {
                let mut f = l.split('\u{1f}');
                let (remote, name) = f.next()?.split_once('/')?;
                if name == "HEAD" {
                    return None;
                }
                Some(RemoteBranchInfo {
                    remote: remote.to_string(),
                    name: name.to_string(),
                    head: CommitId::new(f.next()?),
                    timestamp: f.next()?.trim().parse().unwrap_or(0),
                    subject: f.next().unwrap_or("").to_string(),
                })
            })
            .collect())
    }

    /// Tags, newest first, peeled to the commit they point at. Read-only.
    pub fn tag_list(&self, path: &Path) -> VcsResult<Vec<TagInfo>> {
        let text = self.run_text(
            Some(path),
            &[
                s("for-each-ref"),
                s("--sort=-creatordate"),
                s("--format=%(refname:short)%1f%(*objectname)%1f%(objectname)%1f%(creatordate:unix)%1f%(subject)"),
                s("refs/tags/"),
            ],
            false,
        )?;
        Ok(text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| {
                let mut f = l.split('\u{1f}');
                let name = f.next()?.to_string();
                let peeled = f.next()?;
                let object = f.next()?;
                Some(TagInfo {
                    name,
                    target: CommitId::new(if peeled.is_empty() { object } else { peeled }),
                    timestamp: f.next()?.trim().parse().unwrap_or(0),
                    subject: f.next().unwrap_or("").to_string(),
                })
            })
            .collect())
    }

    /// Remotes with their fetch URLs. Read-only.
    pub fn remote_list(&self, path: &Path) -> VcsResult<Vec<RemoteInfo>> {
        let text = self.run_text(Some(path), &[s("remote"), s("-v")], false)?;
        let mut out: Vec<RemoteInfo> = Vec::new();
        for line in text.lines() {
            let mut f = line.split_whitespace();
            let (Some(name), Some(url)) = (f.next(), f.next()) else {
                continue;
            };
            if out.iter().any(|r| r.name == name) {
                continue;
            }
            out.push(RemoteInfo {
                name: name.to_string(),
                url: url.to_string(),
            });
        }
        Ok(out)
    }

    /// Create a branch at `start` (HEAD when absent) **without switching to
    /// it** — a new ref, nothing in the tree moves, so it is in the safe tier.
    /// With `track`, the branch follows `start` — a remote branch, as a
    /// rule — as its upstream (`--track`: a ref and a line of config).
    /// Switching is `interactive::checkout`.
    pub fn branch_create(
        &self,
        path: &Path,
        name: &str,
        start: Option<&str>,
        track: bool,
    ) -> VcsResult<()> {
        validate_ref("branch", name)?;
        let mut args = vec![s("branch")];
        if track {
            if start.is_none() {
                return Err(VcsError::InvalidArg {
                    what: "start point".into(),
                    value: "(none — a tracking branch needs the branch it follows)".into(),
                });
            }
            args.push(s("--track"));
        }
        args.push(s("--"));
        args.push(s(name));
        if let Some(start) = start {
            validate_ref("start point", start)?;
            args.push(s(start));
        }
        self.write(Some(path), &args, false)?;
        Ok(())
    }

    /// Delete a local branch no worktree holds. `-D`, not `-d`: the caller
    /// has already established the branch carries nothing of its own (the
    /// engine asks [`Git::ahead_behind`] first and deletes only at zero
    /// ahead), and git's own "merged into HEAD" test would refuse a branch
    /// whose base is not the primary's HEAD — which is not the question.
    /// A branch a worktree still holds is refused by git itself. Safe by
    /// that guard alone: nothing this deletes was reachable from nowhere
    /// else. Deleting a branch that carries commits of its own is
    /// `interactive::branch_delete`, behind a person's consent and a
    /// recovery ref.
    pub fn branch_delete(&self, path: &Path, name: &str) -> VcsResult<()> {
        validate_ref("branch", name)?;
        self.write(Some(path), &[s("branch"), s("-D"), s("--"), s(name)], false)?;
        Ok(())
    }

    /// Point a branch at the upstream it follows — `--set-upstream-to` — or
    /// at none. Configuration, not history: safe.
    pub fn branch_set_upstream(
        &self,
        path: &Path,
        branch: &str,
        upstream: Option<&str>,
    ) -> VcsResult<()> {
        validate_ref("branch", branch)?;
        let args = match upstream {
            Some(up) => {
                validate_ref("upstream", up)?;
                vec![
                    s("branch"),
                    s(format!("--set-upstream-to={up}")),
                    s("--"),
                    s(branch),
                ]
            }
            None => vec![s("branch"), s("--unset-upstream"), s("--"), s(branch)],
        };
        self.write(Some(path), &args, false)?;
        Ok(())
    }

    /// Create a lightweight tag at `target` (HEAD when absent) — a new ref,
    /// nothing in the tree moves, so it is in the safe tier like
    /// [`Self::branch_create`]. The panel's annotated tag, with its recovery
    /// point beside the consented delete, is `interactive::tag_create`.
    pub fn tag_create(&self, path: &Path, name: &str, target: Option<&str>) -> VcsResult<()> {
        validate_ref("tag", name)?;
        let mut args = vec![s("tag"), s("--"), s(name)];
        if let Some(target) = target {
            validate_ref("target", target)?;
            args.push(s(target));
        }
        self.write(Some(path), &args, false)?;
        Ok(())
    }

    /// Recovery points, newest first (ide/04). Read-only: the refs are written
    /// by the interactive tier and removed only by a person.
    pub fn recovery_list(&self, path: &Path) -> VcsResult<Vec<RecoveryRef>> {
        let text = self.run_text(
            Some(path),
            &[
                s("for-each-ref"),
                s("--sort=-refname"),
                s("--format=%(refname)%1f%(objectname)"),
                s(RECOVERY_PREFIX),
            ],
            false,
        )?;
        let mut out = Vec::new();
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let mut f = line.split('\u{1f}');
            let (Some(ref_name), Some(sha)) = (f.next(), f.next()) else {
                continue;
            };
            let Some(parsed) = parse_recovery_name(ref_name) else {
                continue;
            };
            // The branch travels in the ref's own reflog message, written with
            // `--create-reflog -m` when the ref was made.
            let branch = self
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
            out.push(RecoveryRef {
                ref_name: ref_name.to_string(),
                commit: CommitId::new(sha),
                op: parsed.1,
                branch,
                kind: parsed.2,
                at: parsed.0,
            });
        }
        Ok(out)
    }

    /// The stash list, newest first (ide/04 §Stash). Read-only, and spelled
    /// as the reflog walk `git stash list` itself runs: the safe tier never
    /// names the `stash` command, it reads `refs/stash` as the ref it is. No
    /// `refs/stash` is an empty list, not an error.
    pub fn stash_list(&self, path: &Path) -> VcsResult<Vec<StashEntry>> {
        let has = self.capture(
            Some(path),
            &[s("rev-parse"), s("-q"), s("--verify"), s("refs/stash")],
            false,
        )?;
        if !has.success {
            return Ok(Vec::new());
        }
        let out = self.run(
            Some(path),
            &[s("log"), s("-g"), s(STASH_FORMAT), s("refs/stash"), s("--")],
            false,
        )?;
        Ok(parse::stash_list(&String::from_utf8_lossy(&out)))
    }

    /// One stash entry's patch: what it holds against the commit it was made
    /// on, and — when it carries untracked files — those files as additions.
    pub fn stash_diff(&self, path: &Path, commit: &str) -> VcsResult<String> {
        validate_ref("commit", commit)?;
        let tracked = self.run(
            Some(path),
            &[
                s("diff"),
                s("--no-color"),
                s(format!("{commit}^1")),
                s(commit),
                s("--"),
            ],
            false,
        )?;
        let mut patch = String::from_utf8_lossy(&tracked).into_owned();
        let untracked = self.capture(
            Some(path),
            &[
                s("rev-parse"),
                s("-q"),
                s("--verify"),
                s(format!("{commit}^3")),
            ],
            false,
        )?;
        if untracked.success {
            // The untracked commit has no parent, so its whole tree is the
            // patch — `--root` is what makes `diff-tree` say so.
            let out = self.run(
                Some(path),
                &[
                    s("diff-tree"),
                    s("--no-commit-id"),
                    s("-r"),
                    s("--root"),
                    s("-p"),
                    s(untracked.stdout_text()),
                    s("--"),
                ],
                false,
            )?;
            patch.push_str(&String::from_utf8_lossy(&out));
        }
        Ok(patch)
    }

    /// One file's bytes as HEAD holds them, as the index does, or as one
    /// revision does — `git show HEAD:p` / `:0:p` / `<rev>:p`. `None` when
    /// the path is not there at that revision: a file git has never seen
    /// has no index entry, a new file nothing at HEAD or at its parent, a
    /// deleted file nothing in the index or at the commit. Bytes, never a
    /// lossy text, so a binary verdict is the reader's to make honestly.
    /// Read-only, and `HEAD` in a repository with no commits is `None` too.
    pub fn blob(&self, path: &Path, rev: BlobRev, pathspec: &str) -> VcsResult<Option<Vec<u8>>> {
        // Validated like every pathspec, but `rev:path` is a revision spec:
        // it takes the bare path relative to the top and no pathspec magic.
        literal_pathspecs("pathspec", &[pathspec])?;
        let spec = match rev {
            BlobRev::Head => format!("HEAD:{pathspec}"),
            BlobRev::Index => format!(":0:{pathspec}"),
            BlobRev::Rev(rev) => {
                validate_ref("rev", &rev)?;
                format!("{rev}:{pathspec}")
            }
        };
        Ok(self
            .capture(Some(path), &[s("show"), s(spec), s("--")], false)
            .ok()
            .filter(|o| o.success)
            .map(|o| o.stdout))
    }

    /// The three sides of a conflicted path — `git show :1:p`, `:2:p`,
    /// `:3:p` — as bytes. Read-only; the working tree's own (marker-laden)
    /// text is the file itself.
    pub fn conflict_blobs(&self, path: &Path, pathspec: &str) -> VcsResult<ConflictBlobs> {
        // Validated like every pathspec (no option, no `..`, no absolute path),
        // but `:N:path` is a stage spec, not a pathspec: it takes the bare
        // path relative to the top and no `:(top,literal)` magic.
        literal_pathspecs("pathspec", &[pathspec])?;
        let side = |stage: u8| -> Option<Vec<u8>> {
            self.capture(
                Some(path),
                &[s("show"), s(format!(":{stage}:{pathspec}"))],
                false,
            )
            .ok()
            .filter(|o| o.success)
            .map(|o| o.stdout)
        };
        Ok(ConflictBlobs {
            base: side(1),
            ours: side(2),
            theirs: side(3),
        })
    }

    /// What a merge of `theirs` into `ours` would do, before anything moves
    /// — `git merge-tree --write-tree`, which writes objects and touches no
    /// tree and no index: clean, or the paths that would conflict. `None`
    /// on a git without it (before 2.38). A rebase is many merges, so for
    /// one the answer over the branch tips is a likelihood, not a promise.
    pub fn merge_preview(
        &self,
        path: &Path,
        ours: &str,
        theirs: &str,
    ) -> VcsResult<Option<MergePreview>> {
        for r in [ours, theirs] {
            if r.starts_with('-') || r.is_empty() {
                return Err(VcsError::InvalidArg {
                    what: "revision".into(),
                    value: r.into(),
                });
            }
        }
        let out = self.capture(
            Some(path),
            &[
                s("merge-tree"),
                s("--write-tree"),
                s("--name-only"),
                s("--no-messages"),
                s(ours),
                s(theirs),
            ],
            false,
        )?;
        let text = String::from_utf8_lossy(&out.stdout);
        Ok(match out.code.as_str() {
            "0" => Some(MergePreview {
                clean: true,
                paths: Vec::new(),
            }),
            "1" => Some(MergePreview {
                clean: false,
                // The tree's oid first, then one conflicted path per line.
                paths: text
                    .lines()
                    .skip(1)
                    .filter(|l| !l.is_empty())
                    .map(PathBuf::from)
                    .collect(),
            }),
            _ => None,
        })
    }

    /// The facts of the operation half-done in this checkout (ide/04
    /// §Conflicts, continued): what is being merged, rebased, picked or
    /// reverted and which side is which — from git's directory, with the
    /// commits' subjects and the branch a sha is the tip of filled in. `None`
    /// when nothing is half-done.
    pub fn operation_facts(&self, path: &Path) -> VcsResult<Option<OperationFacts>> {
        let Some(kind) = self.in_progress(path)? else {
            return Ok(None);
        };
        let git_dir = self.git_dir(path)?;
        let markers = crate::operation::Markers::read(&git_dir, kind);
        let branch = self.status(path).ok().and_then(|st| st.branch);
        let mut facts = crate::operation::facts_of(kind, &markers, branch);
        for side in [&mut facts.ours, &mut facts.theirs] {
            let Some(sha) = side.commit.clone() else {
                continue;
            };
            if side.subject.is_none() {
                side.subject = self.subject_of(path, &sha);
            }
            if side.name.is_none() && side.role == crate::operation::SideRole::Branch {
                side.name = self.branch_at(path, &sha);
            }
        }
        Ok(Some(facts))
    }

    /// A commit's subject line, or none when git cannot show it.
    fn subject_of(&self, path: &Path, sha: &str) -> Option<String> {
        self.capture(
            Some(path),
            &[s("log"), s("-1"), s("--format=%s"), s(sha), s("--")],
            false,
        )
        .ok()
        .filter(|o| o.success)
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|t| !t.is_empty())
    }

    /// The branch or remote-tracking branch whose tip is exactly `sha`, if
    /// one is — `git name-rev` names an ancestor too (`main~2`), which is
    /// not a name a person should read as the side.
    fn branch_at(&self, path: &Path, sha: &str) -> Option<String> {
        self.capture(
            Some(path),
            &[
                s("name-rev"),
                s("--name-only"),
                s("--no-undefined"),
                s("--refs=refs/heads/*"),
                s("--refs=refs/remotes/*"),
                s(sha),
            ],
            false,
        )
        .ok()
        .filter(|o| o.success)
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|n| !n.is_empty() && !n.contains('~') && !n.contains('^'))
        .map(|n| n.strip_prefix("remotes/").unwrap_or(&n).to_string())
    }

    /// The graph log (ide/05): every commit reachable from any ref, in
    /// topological order, newest first, with parents and decorations.
    /// `limit` bounds the count for a first screen; `None` is the whole log.
    /// A repository with no commits yet answers an empty list.
    pub fn graph_log(
        &self,
        path: &Path,
        limit: Option<usize>,
        refs: RefScope,
    ) -> VcsResult<Vec<GraphCommit>> {
        if self.head(path).is_err() {
            if !self.is_repo(path) {
                return Err(VcsError::NotARepository(path.to_path_buf()));
            }
            return Ok(Vec::new());
        }
        let mut args = vec![s("log"), s("--topo-order"), s("--date-order")];
        match refs {
            // Every ref of the person's: the recovery points the interactive
            // tier writes under `refs/bisa/` are Safety's to show (ide/04),
            // never the history's — `--exclude` before `--all` leaves them
            // out of the walk, and `parse::decorations` out of the chips.
            RefScope::All => {
                args.push(s(format!("--exclude={BISA_REFS_GLOB}")));
                args.push(s("--all"));
            }
            RefScope::Head => args.push(s("HEAD")),
        }
        args.push(s("--decorate=full"));
        args.push(s(GRAPH_FORMAT));
        if let Some(n) = limit {
            args.push(s(format!("--max-count={n}")));
        }
        let text = String::from_utf8_lossy(&self.run(Some(path), &args, false)?).into_owned();
        Ok(parse::graph_log(&text))
    }

    /// A fingerprint of every ref and HEAD, so a cached layout knows when the
    /// repository moved on. Two calls agree iff nothing a graph shows changed.
    pub fn refs_fingerprint(&self, path: &Path) -> VcsResult<String> {
        let refs = self.run(
            Some(path),
            &[s("for-each-ref"), s("--format=%(objectname) %(refname)")],
            false,
        )?;
        let head = self
            .capture(Some(path), &[s("rev-parse"), s("HEAD")], false)?
            .stdout;
        // FNV-1a: the crate has no hashing dependency, and this only has to
        // tell "same" from "different", never resist an adversary.
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in refs.iter().chain(b"\n").chain(head.iter()) {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        Ok(format!("{h:016x}"))
    }

    /// One commit in full: message, decorations, and the files it changed
    /// against its first parent (a root commit is diffed against nothing).
    /// Renames are detected (`-M`), in lockstep with [`Self::commit_diff`],
    /// so a moved file is one change with its `old_path` — `diff-tree` is
    /// plumbing and would otherwise report a delete and an add.
    pub fn commit_detail(&self, path: &Path, commit: &str) -> VcsResult<CommitDetail> {
        validate_ref("commit", commit)?;
        let head = String::from_utf8_lossy(&self.run(
            Some(path),
            &[
                s("show"),
                s("-s"),
                s("--decorate=full"),
                s(DETAIL_FORMAT),
                s(commit),
                s("--"),
            ],
            false,
        )?)
        .into_owned();
        let Some(mut detail) = parse::commit_detail(&head) else {
            return Err(VcsError::other(format!("cannot read commit {commit}")));
        };
        let against = against_first_parent(detail.parents.first().map(CommitId::as_str), commit);
        let tree = |mode: &str| -> VcsResult<Vec<u8>> {
            let mut args = vec![
                s("diff-tree"),
                s("--no-commit-id"),
                s("-r"),
                s("-M"),
                s("-z"),
                s(mode),
            ];
            args.extend(against.iter().cloned());
            args.push(s("--"));
            self.run(Some(path), &args, false)
        };
        let nums = parse::numstat(&tree("--numstat")?);
        let kinds = parse::name_status(&tree("--name-status")?);
        detail.files = merge_stats(nums, kinds);
        Ok(detail)
    }

    /// The first parent of `commit`, `None` for a root commit.
    fn first_parent(&self, path: &Path, commit: &str) -> VcsResult<Option<String>> {
        let out = self.capture(
            Some(path),
            &[
                s("rev-parse"),
                s("--verify"),
                s("--quiet"),
                s(format!("{commit}^1")),
            ],
            false,
        )?;
        Ok(out.success.then(|| out.stdout_text()))
    }

    /// One commit's patch against its first parent — the whole commit when
    /// `paths` is empty, else the files named (ide/05: one file of a commit
    /// read on its own). Renames are detected (`-M`), as in
    /// [`Self::commit_detail`]; a renamed file is asked with both its paths,
    /// or the detection has only half of it and reports a delete and an add.
    pub fn commit_diff(&self, path: &Path, commit: &str, paths: &[&str]) -> VcsResult<String> {
        validate_ref("commit", commit)?;
        let specs = literal_pathspecs("path", paths)?;
        let parent = self.first_parent(path, commit)?;
        let mut args = vec![
            s("diff-tree"),
            s("--no-commit-id"),
            s("-r"),
            s("-M"),
            s("-p"),
        ];
        args.extend(against_first_parent(parent.as_deref(), commit));
        args.push(s("--"));
        args.extend(specs.into_iter().map(s));
        let out = self.run(Some(path), &args, false)?;
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    /// Paths git knows nothing about yet, honouring `.gitignore`.
    pub fn list_untracked(&self, path: &Path) -> VcsResult<Vec<PathBuf>> {
        let raw = self.run(
            Some(path),
            &[
                s("ls-files"),
                s("--others"),
                s("--exclude-standard"),
                s("-z"),
            ],
            false,
        )?;
        Ok(raw
            .split(|b| *b == 0)
            .filter(|t| !t.is_empty())
            .map(|t| PathBuf::from(String::from_utf8_lossy(t).into_owned()))
            .collect())
    }

    /// `HEAD`, or the empty tree when the repository has no commits yet.
    /// Diffing against `HEAD` in a fresh repository is a hard error; against
    /// the empty tree it is the obvious answer.
    fn baseline(&self, path: &Path) -> VcsResult<String> {
        match self.head(path) {
            Ok(_) => Ok("HEAD".to_string()),
            Err(e) if e.is_unavailable() => Err(e),
            Err(_) => Ok(EMPTY_TREE.to_string()),
        }
    }

    pub fn head(&self, path: &Path) -> VcsResult<CommitId> {
        let out = self.capture(Some(path), &[s("rev-parse"), s("HEAD")], false)?;
        if out.success {
            return Ok(CommitId::new(out.stdout_text()));
        }
        if !self.is_repo(path) {
            return Err(VcsError::NotARepository(path.to_path_buf()));
        }
        Err(VcsError::other(format!(
            "{} has no commits yet",
            path.display()
        )))
    }

    /// Commits reachable from HEAD, newest first. `base` limits the walk to
    /// `base..HEAD`, which is the "what is on this branch" question a workstream
    /// asks.
    pub fn log(
        &self,
        path: &Path,
        base: Option<&str>,
        limit: usize,
    ) -> VcsResult<Vec<CommitSummary>> {
        let mut args = vec![s("log"), s(LOG_FORMAT), s(format!("--max-count={limit}"))];
        match base {
            Some(base) => {
                validate_value("base", base)?;
                args.push(s(format!("{base}..HEAD")));
            }
            None => args.push(s("HEAD")),
        }
        let text = self.run_text(Some(path), &args, false)?;
        Ok(parse::log(&text))
    }

    /// `(ahead, behind)` of HEAD relative to `base`.
    ///
    /// `rev-list --left-right --count` rather than a merge-base dance: one
    /// invocation, no temporary refs, and it is what git's own status uses.
    pub fn ahead_behind(&self, path: &Path, base: &str) -> VcsResult<(u32, u32)> {
        validate_value("base", base)?;
        let text = self.run_text(
            Some(path),
            &[
                s("rev-list"),
                s("--left-right"),
                s("--count"),
                s(format!("{base}...HEAD")),
            ],
            false,
        )?;
        let (behind, ahead) = parse::left_right(&text);
        Ok((ahead, behind))
    }

    // -- writing ------------------------------------------------------------

    /// Stage everything, including deletions, from the repository root.
    pub fn add_all(&self, path: &Path) -> VcsResult<()> {
        self.write(Some(path), &[s("add"), s("--all"), s("--"), s(".")], false)?;
        Ok(())
    }

    /// The selected paths git still finds as `reach` reads them, in the
    /// selection's order; the rest are left out.
    ///
    /// A selection is the Changes list as it was last read. With an agent at
    /// work in the checkout, a file it listed can be gone by the click — and
    /// git refuses a whole `add`, `restore`, `checkout` or `stash push` for
    /// one pathspec that matches nothing, so one vanished file would stop
    /// every other from staging. Each path is validated first, as every
    /// pathspec is ([`literal_pathspec`]): an unsafe one is refused, never
    /// quietly dropped. Read-only — one `ls-files` or `diff`.
    pub(crate) fn still_known<S: AsRef<str>>(
        &self,
        path: &Path,
        selection: &[S],
        reach: Reach,
    ) -> VcsResult<Vec<String>> {
        let specs = literal_pathspecs("pathspec", selection)?;
        if specs.is_empty() {
            return Ok(Vec::new());
        }
        let mut args = match reach {
            Reach::Worktree => vec![
                s("ls-files"),
                s("-z"),
                s("--full-name"),
                s("--cached"),
                s("--others"),
                s("--exclude-standard"),
            ],
            Reach::Index => vec![s("ls-files"), s("-z"), s("--full-name"), s("--cached")],
            Reach::Staged => vec![
                s("diff"),
                s("--cached"),
                s("--name-only"),
                s("-z"),
                s("--no-renames"),
                s(self.baseline(path)?),
            ],
        };
        args.push(s("--"));
        args.extend(specs.into_iter().map(s));
        let listed = self.run(Some(path), &args, false)?;
        let listed = String::from_utf8_lossy(&listed);
        let entries: Vec<&str> = listed.split('\0').filter(|e| !e.is_empty()).collect();
        Ok(selection
            .iter()
            .map(AsRef::as_ref)
            .filter(|p| entries.iter().any(|e| covers(p, e)))
            .map(str::to_string)
            .collect())
    }

    /// Stage exactly the paths given, and nothing else.
    ///
    /// Index-only: `git add` copies worktree content *into* the index and
    /// never writes into the working tree, so nothing a user typed can be
    /// lost here. An empty list is a no-op rather than "stage everything" —
    /// the difference between those two is somebody's afternoon. A path that
    /// has gone since it was listed — never tracked, and no longer on disk —
    /// has nothing to stage and is left out ([`Self::still_known`]); a
    /// tracked file deleted from disk is still in the index, and its
    /// deletion stages.
    pub fn stage<S: AsRef<str>>(&self, path: &Path, pathspecs: &[S]) -> VcsResult<()> {
        let kept = self.still_known(path, pathspecs, Reach::Worktree)?;
        let specs = literal_pathspecs("pathspec", &kept)?;
        if specs.is_empty() {
            return Ok(());
        }
        let mut args = vec![s("add"), s("--")];
        args.extend(specs.into_iter().map(s));
        self.write(Some(path), &args, false)?;
        Ok(())
    }

    /// Take paths back out of the index, leaving the working tree alone.
    ///
    /// `restore --staged` **with** the flag, always. The same command without
    /// it overwrites the worktree from the index, which would make this the
    /// one call in the crate that can destroy what somebody typed — see the
    /// invariant in the crate docs. The flag is not a parameter here, so
    /// there is no call site that could pass the wrong one. Only the paths
    /// with a staged change are handed on ([`Self::still_known`]): any other
    /// is a no-op to unstage, and one git no longer knows at all — staged a
    /// moment ago, gone since — would refuse the whole batch.
    pub fn unstage<S: AsRef<str>>(&self, path: &Path, pathspecs: &[S]) -> VcsResult<()> {
        let kept = self.still_known(path, pathspecs, Reach::Staged)?;
        let specs = literal_pathspecs("pathspec", &kept)?;
        if specs.is_empty() {
            return Ok(());
        }
        // The source is spelled out rather than left to default to HEAD, so
        // that a repository with no commits yet — where HEAD cannot be
        // resolved and the default would simply fail — unstages against the
        // empty tree instead. Restoring an index entry *from the empty tree*
        // is how you drop it without a `git rm` in the crate.
        let mut args = vec![
            s("restore"),
            s("--staged"),
            s(format!("--source={}", self.baseline(path)?)),
            s("--"),
        ];
        args.extend(specs.into_iter().map(s));
        self.write(Some(path), &args, false)?;
        Ok(())
    }

    /// Commit the index and return the new commit.
    ///
    /// `--no-verify` is deliberately never passed: if the user has a pre-commit
    /// hook, it runs, and its refusal is theirs to see. An unset identity comes
    /// back as [`VcsError::IdentityUnset`] rather than being papered over — the
    /// fix is [`Self::set_local_identity`], never a global write.
    /// Commit the index. Refuses when nobody is set to commit here
    /// ([`VcsError::IdentityUnset`]) **before** asking git, because git on a
    /// developer's machine would otherwise invent an author from the login
    /// name and the host — a commit nobody chose to sign that way.
    pub fn commit(&self, path: &Path, message: &str, allow_empty: bool) -> VcsResult<CommitId> {
        if self.identity(path)?.source == IdentitySource::None {
            return Err(VcsError::IdentityUnset(format!(
                "nobody is set to commit in {}: user.name and user.email resolve to nothing",
                path.display()
            )));
        }
        // A message of nothing is refused before git is asked: git says so
        // itself, in a sentence about an editor nobody opened.
        if message.trim().is_empty() {
            return Err(VcsError::InvalidArg {
                what: "commit message".into(),
                value: message.to_string(),
            });
        }
        let mut args = vec![s("commit"), s("--quiet"), s("-m"), s(message)];
        if allow_empty {
            args.push(s("--allow-empty"));
        }
        self.write(Some(path), &args, false)?;
        self.head(path)
    }

    // -- identity -----------------------------------------------------------

    /// Who git will name as the author of a commit in `path`, and where that
    /// answer comes from. Three reads per key — the effective value, the
    /// repository's own, the global one — so a caller can tell *set here* from
    /// *inherited*, and offer to pin the global pair into the repository.
    pub fn identity(&self, path: &Path) -> VcsResult<GitIdentity> {
        let cwd = Some(path);
        let name = self.config_get(cwd, ReadScope::Effective, "user.name")?;
        let email = self.config_get(cwd, ReadScope::Effective, "user.email")?;
        let local_name = self.config_get(cwd, ReadScope::Local, "user.name")?;
        let local_email = self.config_get(cwd, ReadScope::Local, "user.email")?;
        let global = self.global_pair(cwd)?;
        let source = if local_name.is_some() && local_email.is_some() {
            IdentitySource::Local
        } else if name.is_some() && email.is_some() {
            IdentitySource::Global
        } else {
            IdentitySource::None
        };
        Ok(GitIdentity {
            name,
            email,
            source,
            global,
        })
    }

    /// Set who commits in this repository: `user.name` and `user.email` in the
    /// repository's **local** config, which the primary checkout and every
    /// worktree of it share. Never `--global`; validated before anything is
    /// spawned. Answers with the identity as git now resolves it.
    pub fn set_local_identity(
        &self,
        path: &Path,
        name: &str,
        email: &str,
    ) -> VcsResult<GitIdentity> {
        validate_identity(name, email)?;
        self.config_set(ConfigScope::Local, Some(path), "user.name", name)?;
        self.config_set(ConfigScope::Local, Some(path), "user.email", email)?;
        self.identity(path)
    }

    /// The schema's keys as the global layer and — with a checkout — the local
    /// layer hold them. One `--list` per layer, filtered to the
    /// schema, so a form can show every key with what it inherits.
    pub fn config_view(&self, path: Option<&Path>) -> VcsResult<GitConfigView> {
        let global = self.config_list(None, ReadScope::Global)?;
        let local = match path {
            Some(p) => self.config_list(Some(p), ReadScope::Local)?,
            None => Vec::new(),
        };
        // git lists keys in lower case (`user.useconfigonly`); the schema
        // spells them as git documents them. A key's section and name are
        // case-insensitive to git, so they are here.
        let find = |list: &[(String, String)], key: &str| {
            list.iter()
                .rev()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v.clone())
        };
        Ok(GitConfigView {
            entries: crate::config_schema::GIT_CONFIG_KEYS
                .iter()
                .map(|d| ConfigEntry {
                    key: d.key.to_string(),
                    local: find(&local, d.key),
                    global: find(&global, d.key),
                })
                .collect(),
        })
    }

    /// Write one schema key at one layer — **the** config write of the crate.
    /// Refused before anything is spawned when the key is unknown, does not
    /// live at that layer, or the value is not of its kind. `Global` takes no
    /// path and is reached only from Settings → Git (I45); `Local` needs the
    /// checkout.
    pub fn config_set(
        &self,
        scope: ConfigScope,
        path: Option<&Path>,
        key: &str,
        value: &str,
    ) -> VcsResult<()> {
        let def = crate::config_schema::writable_key(key, scope)?;
        let value = crate::config_schema::validate_config_value(def, value)?;
        let cwd = self.config_cwd(scope, path)?;
        let flag = match scope {
            ConfigScope::Global => s("--global"),
            ConfigScope::Local => s("--local"),
        };
        self.write(cwd, &[s("config"), flag, s(def.key), s(&value)], false)?;
        Ok(())
    }

    /// Remove one schema key from one layer, so the value falls through to
    /// the next. A key that was not set is not an error.
    pub fn config_unset(
        &self,
        scope: ConfigScope,
        path: Option<&Path>,
        key: &str,
    ) -> VcsResult<()> {
        let def = crate::config_schema::writable_key(key, scope)?;
        let cwd = self.config_cwd(scope, path)?;
        let flag = match scope {
            ConfigScope::Global => s("--global"),
            ConfigScope::Local => s("--local"),
        };
        let args = [s("config"), flag, s("--unset-all"), s(def.key)];
        let out = self.write_capture(cwd, &args, false)?;
        // Exit 5 is git's "the section or key was not set".
        if out.success || out.code == "5" {
            return Ok(());
        }
        Err(classify(&describe(&self.bin, &args), cwd, &out))
    }

    fn config_cwd<'a>(
        &self,
        scope: ConfigScope,
        path: Option<&'a Path>,
    ) -> VcsResult<Option<&'a Path>> {
        match (scope, path) {
            (ConfigScope::Global, None) => Ok(None),
            (ConfigScope::Global, Some(p)) => Err(VcsError::InvalidArg {
                what: "a global config write takes no repository".to_string(),
                value: p.display().to_string(),
            }),
            (ConfigScope::Local, Some(p)) => Ok(Some(p)),
            (ConfigScope::Local, None) => Err(VcsError::InvalidArg {
                what: "a local config write needs a repository".to_string(),
                value: String::new(),
            }),
        }
    }

    /// `git config --<layer> --list -z`: every `key\nvalue` pair the layer
    /// holds, keys lower-cased the way git reports them. Exit 128 with no
    /// global file is "nothing set", not a failure.
    fn config_list(
        &self,
        cwd: Option<&Path>,
        scope: ReadScope,
    ) -> VcsResult<Vec<(String, String)>> {
        let flag = match scope {
            ReadScope::Global => s("--global"),
            ReadScope::Local => s("--local"),
            ReadScope::Effective => s("--show-origin"),
        };
        let args = [s("config"), flag, s("--list"), s("-z")];
        let out = self.capture(cwd, &args, false)?;
        if !out.success {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("unable to read config file") || stderr.contains("No such file") {
                return Ok(Vec::new());
            }
            return Err(classify(&describe(&self.bin, &args), cwd, &out));
        }
        Ok(String::from_utf8_lossy(&out.stdout)
            .split('\0')
            .filter(|rec| !rec.is_empty())
            .map(|rec| match rec.split_once('\n') {
                Some((k, v)) => (k.to_string(), v.to_string()),
                None => (rec.to_string(), String::new()),
            })
            .collect())
    }

    /// The person's global `user.name`/`user.email`, read from no repository
    /// at all — what a caller offers to pin into a repository that has none,
    /// and what Settings shows as "your global identity". `Some` only when both
    /// keys resolve. A read; this crate never writes global config (I45).
    pub fn global_identity(&self) -> VcsResult<Option<Ident>> {
        self.global_pair(None)
    }

    fn global_pair(&self, cwd: Option<&Path>) -> VcsResult<Option<Ident>> {
        let name = self.config_get(cwd, ReadScope::Global, "user.name")?;
        let email = self.config_get(cwd, ReadScope::Global, "user.email")?;
        Ok(match (name, email) {
            (Some(name), Some(email)) => Some(Ident { name, email }),
            _ => None,
        })
    }

    /// One `git config --get`, at one scope. Exit 1 is git's "unset", not a
    /// failure; everything else is classified like any other refusal. `cwd`
    /// is `None` for a read that needs no repository (the global layer).
    fn config_get(
        &self,
        cwd: Option<&Path>,
        scope: ReadScope,
        key: &str,
    ) -> VcsResult<Option<String>> {
        let mut args = vec![s("config")];
        match scope {
            ReadScope::Effective => {}
            ReadScope::Local => args.push(s("--local")),
            ReadScope::Global => args.push(s("--global")),
        }
        args.push(s("--get"));
        args.push(s(key));
        let out = self.capture(cwd, &args, false)?;
        if out.success {
            let value = out.stdout_text();
            return Ok((!value.is_empty()).then_some(value));
        }
        if out.code == "1" {
            return Ok(None);
        }
        Err(classify(&describe(&self.bin, &args), cwd, &out))
    }

    // -- credentials --------------------------------------------------------

    /// Ask git's own credential helpers for the credential they hold for
    /// `protocol://host` — the one `git push` over HTTPS uses — through
    /// `git credential fill`. Never a file read of ours, never a key: git runs
    /// whatever helper its config names (osxkeychain, `gh`, a credential
    /// manager), and under the hardened environment a helper that would prompt
    /// fails at once instead of hanging. `Ok(None)` when no helper answered;
    /// only a spawn failure or the timeout is an error.
    pub fn credential_fill(&self, protocol: &str, host: &str) -> VcsResult<Option<Credential>> {
        let args = [s("credential"), s("fill")];
        let question = format!("protocol={protocol}\nhost={host}\n\n").into_bytes();
        let out = self.capture_input(None, &args, question)?;
        if !out.success {
            return Ok(None);
        }
        let mut username = None;
        let mut password = None;
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            if let Some(v) = line.strip_prefix("username=") {
                username = Some(v.to_string());
            } else if let Some(v) = line.strip_prefix("password=") {
                password = Some(v.to_string());
            }
        }
        Ok(password
            .filter(|p| !p.is_empty())
            .map(|password| Credential {
                username: username.unwrap_or_default(),
                password,
            }))
    }

    /// The credential helpers the person's global git config names —
    /// `credential.helper`, every value — summarised to a word each
    /// ([`summarise_helper`]). A read of non-secret config, so Settings can
    /// say *pushes over HTTPS use osxkeychain* without asking for a secret.
    pub fn credential_helpers(&self) -> VcsResult<Vec<String>> {
        let args = [
            s("config"),
            s("--global"), // ReadScope::Global
            s("--get-all"),
            s("credential.helper"),
        ];
        let out = self.capture(None, &args, false)?;
        if out.success {
            return Ok(out
                .stdout_text()
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(summarise_helper)
                .collect());
        }
        let stderr = String::from_utf8_lossy(&out.stderr);
        if out.code == "1"
            || stderr.contains("unable to read config file")
            || stderr.contains("No such file")
        {
            return Ok(Vec::new());
        }
        Err(classify(&describe(&self.bin, &args), None, &out))
    }

    // -- profiles: includes, the profile file, origins -----------------------

    /// `git --version`, parsed. A git that answers nothing parseable is
    /// `NotAvailable` — the probe would have said so first.
    pub fn version(&self) -> VcsResult<GitVersion> {
        let text = self.run_text(None, &[s("--version")], false)?;
        GitVersion::parse(&text)
            .ok_or_else(|| VcsError::NotAvailable(format!("`git --version` said {text:?}")))
    }

    /// Set one `includeIf.<condition>.path` in the person's **global** config
    /// to `file` — idempotently, and **last**: git reads a config file top to
    /// bottom and the last value wins, so an include that is meant to
    /// override `user.name` has to sit below it. The entry is removed
    /// (`--unset-all --fixed-value`, so a path with dots is never a regex) and
    /// appended again, which moves it to the end whatever was written since.
    /// The global layer alone: an include is the person's, never a
    /// repository's, so `Local` is refused before anything is spawned. With
    /// [`Self::config_set`] at `Global` and [`Self::include_remove`], one of
    /// the three sanctioned global writes (I45).
    pub fn include_set(&self, scope: ConfigScope, condition: &str, file: &Path) -> VcsResult<()> {
        let flag = match scope {
            ConfigScope::Global => s("--global"),
            ConfigScope::Local => {
                return Err(VcsError::InvalidArg {
                    what: "an include is written at the global layer only".to_string(),
                    value: condition.to_string(),
                })
            }
        };
        validate_include_condition(condition)?;
        let file = validate_include_path(file)?;
        let key = format!("includeIf.{condition}.path");
        let unset = [
            s("config"),
            flag.clone(),
            s("--unset-all"),
            s("--fixed-value"),
            s(&key),
            s(&file),
        ];
        let out = self.write_capture(None, &unset, false)?;
        // Exit 5 is git's "the section or key was not set".
        if !out.success && out.code != "5" {
            return Err(classify(&describe(&self.bin, &unset), None, &out));
        }
        self.write(
            None,
            &[s("config"), flag, s("--add"), s(&key), s(&file)],
            false,
        )?;
        Ok(())
    }

    /// Remove every global `includeIf.*.path` that names `file`. A file no
    /// include names is not an error.
    pub fn include_remove(&self, scope: ConfigScope, file: &Path) -> VcsResult<()> {
        let flag = match scope {
            ConfigScope::Global => s("--global"),
            ConfigScope::Local => {
                return Err(VcsError::InvalidArg {
                    what: "an include is removed at the global layer only".to_string(),
                    value: file.display().to_string(),
                })
            }
        };
        let file = validate_include_path(file)?;
        for include in self.includes()? {
            if include.path.as_os_str() != file.as_os_str() {
                continue;
            }
            let key = format!("includeIf.{}.path", include.condition);
            let args = [
                s("config"),
                flag.clone(),
                s("--unset-all"),
                s("--fixed-value"),
                s(&key),
                s(&file),
            ];
            let out = self.write_capture(None, &args, false)?;
            // Exit 5 is git's "the section or key was not set".
            if !out.success && out.code != "5" {
                return Err(classify(&describe(&self.bin, &args), None, &out));
            }
        }
        Ok(())
    }

    /// Every `includeIf` of the person's global config, condition and file —
    /// a read (`--get-regexp`), so Settings can show what the global file
    /// pulls in and the engine can tell its own profiles from a person's.
    pub fn includes(&self) -> VcsResult<Vec<Include>> {
        let flag = s("--global"); // ReadScope::Global
        let args = [
            s("config"),
            flag,
            s("--get-regexp"),
            s("-z"),
            s("^includeif\\..*\\.path$"),
        ];
        let out = self.capture(None, &args, false)?;
        if !out.success {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if out.code == "1"
                || stderr.contains("unable to read config file")
                || stderr.contains("No such file")
            {
                return Ok(Vec::new());
            }
            return Err(classify(&describe(&self.bin, &args), None, &out));
        }
        Ok(String::from_utf8_lossy(&out.stdout)
            .split('\0')
            .filter(|rec| !rec.is_empty())
            .filter_map(|rec| {
                let (key, value) = rec.split_once('\n')?;
                let condition = key.strip_prefix("includeif.")?.strip_suffix(".path")?;
                Some(Include {
                    condition: condition.to_string(),
                    path: PathBuf::from(value),
                })
            })
            .collect())
    }

    /// Write a profile's file: the five keys of [`ProfileFile`] through
    /// `git config --file`, nothing else, each validated before anything is
    /// spawned; an absent optional is unset so a re-save removes what a person
    /// cleared. The file is created owner-only. Never the global layer.
    pub fn config_file_write(&self, file: &Path, profile: &ProfileFile) -> VcsResult<()> {
        validate_identity(&profile.name, &profile.email)?;
        let file = validate_include_path(file)?;
        let bad = |what: &str, value: &str| VcsError::InvalidArg {
            what: what.to_string(),
            value: value.to_string(),
        };
        let ssh_command = match &profile.ssh_key {
            Some(key) => {
                let text = key
                    .to_str()
                    .ok_or_else(|| bad("ssh key path", &key.display().to_string()))?;
                if !is_key_path(text) {
                    return Err(bad("ssh key path", text));
                }
                Some(ProfileFile::ssh_command(key))
            }
            None => None,
        };
        for (what, login) in [
            ("credential.username", &profile.credential_username),
            (crate::config_schema::ACCOUNT_KEY, &profile.account),
        ] {
            if let Some(login) = login {
                if !crate::config_schema::validate_login(login) {
                    return Err(bad(what, login));
                }
            }
        }
        if let Some(label) = &profile.label {
            let l = label.trim();
            if l.is_empty() || l.starts_with('-') || l.contains(['\r', '\n']) {
                return Err(bad("bisa.label", label));
            }
        }
        create_owner_only(&file)?;
        let set = |key: &str, value: Option<&str>| -> VcsResult<()> {
            match value {
                Some(v) => {
                    self.write(
                        None,
                        &[s("config"), s("--file"), s(&file), s(key), s(v)],
                        false,
                    )?;
                }
                None => {
                    let args = [s("config"), s("--file"), s(&file), s("--unset-all"), s(key)];
                    let out = self.write_capture(None, &args, false)?;
                    if !out.success && out.code != "5" {
                        return Err(classify(&describe(&self.bin, &args), None, &out));
                    }
                }
            }
            Ok(())
        };
        set("bisa.label", profile.label.as_deref().map(str::trim))?;
        set("user.name", Some(profile.name.trim()))?;
        set("user.email", Some(profile.email.trim()))?;
        set("core.sshCommand", ssh_command.as_deref())?;
        set(
            "credential.username",
            profile.credential_username.as_deref().map(str::trim),
        )?;
        set(
            crate::config_schema::ACCOUNT_KEY,
            profile
                .account
                .as_deref()
                .map(str::trim)
                .map(str::to_ascii_lowercase)
                .as_deref(),
        )?;
        Ok(())
    }

    /// Read a profile's file back — `None` when there is no such file. Only
    /// the five keys are read; anything else a person added by hand is left
    /// where it is and never reported.
    pub fn config_file_read(&self, file: &Path) -> VcsResult<Option<ProfileFile>> {
        if !file.is_file() {
            return Ok(None);
        }
        let args = [s("config"), s("--file"), s(file), s("--list"), s("-z")];
        let out = self.capture(None, &args, false)?;
        if !out.success {
            return Err(classify(&describe(&self.bin, &args), None, &out));
        }
        let mut profile = ProfileFile::default();
        for rec in String::from_utf8_lossy(&out.stdout)
            .split('\0')
            .filter(|r| !r.is_empty())
        {
            let (key, value) = match rec.split_once('\n') {
                Some(kv) => kv,
                None => (rec, ""),
            };
            match key {
                "bisa.label" => profile.label = Some(value.to_string()),
                "user.name" => profile.name = value.to_string(),
                "user.email" => profile.email = value.to_string(),
                "core.sshcommand" => profile.ssh_key = ProfileFile::key_of_ssh_command(value),
                "credential.username" => profile.credential_username = Some(value.to_string()),
                k if k == crate::config_schema::ACCOUNT_KEY => {
                    profile.account = Some(value.to_string())
                }
                _ => {}
            }
        }
        Ok(Some(profile))
    }

    /// One key's effective value **and the file it came from**
    /// (`--show-origin`), read in `path` — or from no repository — so a caller
    /// can say *inherited from your Acme profile* rather than *global*.
    /// `None` when the key is unset.
    pub fn config_origin(&self, path: Option<&Path>, key: &str) -> VcsResult<Option<ConfigOrigin>> {
        let args = [s("config"), s("--show-origin"), s("-z"), s("--get"), s(key)];
        let out = self.capture(path, &args, false)?;
        if !out.success {
            if out.code == "1" {
                return Ok(None);
            }
            return Err(classify(&describe(&self.bin, &args), path, &out));
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let mut parts = text.split('\0');
        let origin = parts.next().unwrap_or_default();
        let value = parts.next().unwrap_or_default();
        // A git without `-z` on this path separates the two with a tab.
        let (origin, value) = match origin.split_once('\t') {
            Some((o, v)) if value.is_empty() => (o, v.trim_end_matches('\n')),
            _ => (origin, value),
        };
        // A repository's own file is reported relative to it (`file:.git/config`);
        // the caller compares paths, so it is made absolute here.
        let origin = match (origin.strip_prefix("file:"), path) {
            (Some(rel), Some(root)) if !Path::new(rel).is_absolute() => {
                format!("file:{}", root.join(rel).display())
            }
            _ => origin.to_string(),
        };
        Ok(Some(ConfigOrigin {
            value: value.to_string(),
            origin,
        }))
    }

    /// The code host account a checkout names — a local pin or a profile's
    /// (`codehost.account`, effective). The kind's default is
    /// [`Git::default_account`]'s; the engine asks for it when this is `None`.
    pub fn account_get(&self, path: &Path) -> VcsResult<Option<String>> {
        self.config_get(
            Some(path),
            ReadScope::Effective,
            crate::config_schema::ACCOUNT_KEY,
        )
    }

    /// The kind of code host a checkout says its remote is (`codehost.kind`,
    /// effective) — for a host of the person's own that no public name detects.
    pub fn kind_get(&self, path: &Path) -> VcsResult<Option<String>> {
        self.config_get(
            Some(path),
            ReadScope::Effective,
            crate::config_schema::KIND_KEY,
        )
    }

    /// The default account of one kind — the global `codehost.<kind>.account`,
    /// read from no repository. A kind this schema does not know has none.
    pub fn default_account(&self, kind: &str) -> VcsResult<Option<String>> {
        match crate::config_schema::default_account_key(kind) {
            Some(key) => self.config_get(None, ReadScope::Global, key),
            None => Ok(None),
        }
    }

    /// `git ls-remote --heads <remote>` in `path`: the branches the remote
    /// holds. The one call that proves a person can reach a remote — over SSH
    /// with the key the checkout resolves, over HTTPS with git's helper — and
    /// changes nothing on either side. Network budget; a refused credential is
    /// `NotAuthenticated` like any push's.
    pub fn ls_remote(&self, path: &Path, remote: &str) -> VcsResult<Vec<RemoteRef>> {
        validate_value("remote", remote)?;
        let text = self.run_text(Some(path), &[s("ls-remote"), s("--heads"), s(remote)], true)?;
        Ok(text
            .lines()
            .filter_map(|line| {
                let (sha, name) = line.split_once(['\t', ' '])?;
                Some(RemoteRef {
                    sha: sha.trim().to_string(),
                    name: name.trim().to_string(),
                })
            })
            .collect())
    }

    // -- remotes ------------------------------------------------------------

    /// URL of a remote, or `None` when there is no such remote. Absence is an
    /// answer, not an error — "does this project have an origin yet?" is a
    /// question the platform asks constantly.
    pub fn remote_get(&self, path: &Path, name: &str) -> VcsResult<Option<String>> {
        validate_ref("remote", name)?;
        let out = self.capture(
            Some(path),
            &[s("remote"), s("get-url"), s("--"), s(name)],
            false,
        )?;
        if out.success {
            return Ok(Some(out.stdout_text()));
        }
        match classify("git remote get-url", Some(path), &out) {
            VcsError::NoRemote(_) => Ok(None),
            other => Err(other),
        }
    }

    pub fn remote_add(&self, path: &Path, name: &str, url: &str) -> VcsResult<()> {
        validate_ref("remote", name)?;
        validate_value("remote url", url)?;
        self.write(
            Some(path),
            &[s("remote"), s("add"), s("--"), s(name), s(url)],
            false,
        )?;
        Ok(())
    }

    /// Push one branch. `set_upstream` records the tracking relationship, so a
    /// later [`Self::status`] can report ahead/behind.
    pub fn push(
        &self,
        path: &Path,
        remote: &str,
        branch: &str,
        set_upstream: bool,
    ) -> VcsResult<()> {
        validate_ref("remote", remote)?;
        validate_ref("branch", branch)?;
        let mut args = vec![s("push"), s("--quiet")];
        if set_upstream {
            args.push(s("--set-upstream"));
        }
        args.push(s(remote));
        args.push(s(format!("refs/heads/{branch}:refs/heads/{branch}")));
        self.write(Some(path), &args, true)?;
        Ok(())
    }

    pub fn fetch(&self, path: &Path, remote: &str) -> VcsResult<()> {
        validate_ref("remote", remote)?;
        self.write(
            Some(path),
            &[s("fetch"), s("--quiet"), s("--"), s(remote)],
            true,
        )?;
        Ok(())
    }

    /// Fetch one branch of a remote — `refs/remotes/<remote>/<branch>` is
    /// what it updates — rather than everything the remote has. The read a
    /// tracking worktree ([`Self::worktree_add_tracking`]) needs first; a
    /// branch the remote does not have is git's error, with its words.
    pub fn fetch_branch(&self, path: &Path, remote: &str, branch: &str) -> VcsResult<()> {
        validate_ref("remote", remote)?;
        validate_ref("branch", branch)?;
        self.write(
            Some(path),
            &[
                s("fetch"),
                s("--quiet"),
                s("--"),
                s(remote),
                s(format!(
                    "+refs/heads/{branch}:refs/remotes/{remote}/{branch}"
                )),
            ],
            true,
        )?;
        Ok(())
    }

    /// Point an existing remote at another URL. Safe: it rewrites a line of
    /// config and moves nothing in the tree; the old URL is still in the
    /// reflog of nothing, so the caller shows it before asking.
    pub fn remote_set_url(&self, path: &Path, name: &str, url: &str) -> VcsResult<()> {
        validate_ref("remote", name)?;
        validate_value("remote url", url)?;
        self.write(
            Some(path),
            &[s("remote"), s("set-url"), s("--"), s(name), s(url)],
            false,
        )?;
        Ok(())
    }

    /// Make `name` point at `url`, adding the remote when it does not exist
    /// and re-pointing it when it does — the one call a "Set origin" control
    /// needs, whichever state it finds.
    pub fn ensure_remote(&self, path: &Path, name: &str, url: &str) -> VcsResult<()> {
        match self.remote_get(path, name)? {
            Some(current) if current == url => Ok(()),
            Some(_) => self.remote_set_url(path, name, url),
            None => self.remote_add(path, name, url),
        }
    }

    /// The branch HEAD tracks — `origin/main` — or `None` when it has no
    /// upstream (a branch never pushed, or a detached HEAD).
    pub fn upstream_of(&self, path: &Path) -> VcsResult<Option<String>> {
        let out = self.capture(
            Some(path),
            &[
                s("rev-parse"),
                s("--abbrev-ref"),
                s("--symbolic-full-name"),
                s("@{upstream}"),
            ],
            false,
        )?;
        if out.success {
            let name = out.stdout_text();
            return Ok((!name.is_empty()).then_some(name));
        }
        let stderr = out.stderr_text().to_ascii_lowercase();
        if stderr.contains("no upstream") || stderr.contains("does not point to a branch") {
            return Ok(None);
        }
        Err(classify("git rev-parse @{upstream}", Some(path), &out))
    }

    /// Whether `target` can be reached from HEAD by moving forward alone —
    /// `git merge-base --is-ancestor HEAD <target>` — which is what a
    /// fast-forward pull needs to be true.
    pub fn can_fast_forward(&self, path: &Path, target: &str) -> VcsResult<bool> {
        validate_ref("target", target)?;
        let out = self.capture(
            Some(path),
            &[s("merge-base"), s("--is-ancestor"), s("HEAD"), s(target)],
            false,
        )?;
        match out.code.as_str() {
            _ if out.success => Ok(true),
            "1" => Ok(false),
            _ => Err(classify("git merge-base --is-ancestor", Some(path), &out)),
        }
    }

    /// The operation git has left half-done in this checkout, if any: a
    /// rebase, a merge, a cherry-pick or a revert — read from the files git
    /// keeps under its directory, never from prose.
    pub fn in_progress(&self, path: &Path) -> VcsResult<Option<crate::interactive::InProgress>> {
        use crate::interactive::InProgress;
        let markers: [(&str, InProgress); 5] = [
            ("rebase-merge", InProgress::Rebase),
            ("rebase-apply", InProgress::Rebase),
            ("MERGE_HEAD", InProgress::Merge),
            ("CHERRY_PICK_HEAD", InProgress::CherryPick),
            ("REVERT_HEAD", InProgress::Revert),
        ];
        // The checkout's own directory — a worktree's private one under
        // `.git/worktrees/` — is where these markers live.
        let git_dir = self.git_dir(path)?;
        Ok(markers
            .iter()
            .find(|(marker, _)| git_dir.join(marker).exists())
            .map(|(_, op)| *op))
    }

    /// The paths with unmerged entries in the index — what a conflict left
    /// for a person to settle — relative to the repository root.
    pub fn conflicted_paths(&self, path: &Path) -> VcsResult<Vec<PathBuf>> {
        Ok(self
            .status_files(path)?
            .into_iter()
            .filter(|f| f.is_conflicted())
            .map(|f| f.path)
            .collect())
    }
}

// ---------------------------------------------------------------------------
// Argument safety
// ---------------------------------------------------------------------------

/// Reject a value that could be read as an option, or that cannot survive an
/// argv round trip.
///
/// Nothing here is defending against a shell — there is no shell, argv arrays
/// go straight to `execve`, and a branch named `; rm -rf /` is just an
/// unusual ref name. What it defends against is *option injection*: git has no
/// `--` separator for `worktree add`, `push` or `remote`, so a value beginning
/// with `-` would be parsed as a flag. `--upload-pack=…` on a fetch is the
/// classic way that turns into command execution.
pub fn validate_value(what: &str, value: &str) -> VcsResult<()> {
    let bad = |value: &str| VcsError::InvalidArg {
        what: what.to_string(),
        value: value.to_string(),
    };
    if value.is_empty() || value.starts_with('-') {
        return Err(bad(value));
    }
    if value
        .chars()
        .any(|c| c.is_control() || c.is_whitespace() || c == '\0')
    {
        return Err(bad(value));
    }
    Ok(())
}

/// [`validate_value`] plus the rules `git check-ref-format` enforces, applied
/// up front so a bad branch name is a typed error rather than an exit code.
/// Deliberately not exhaustive — git remains the authority, and anything this
/// lets through it will refuse itself.
/// A commit identity that git will accept and that cannot smuggle an option
/// or a second config line: trimmed, non-empty, single-line, not starting with
/// `-`; the email has one `@` and no whitespace. Pure — nothing is spawned.
pub fn validate_identity(name: &str, email: &str) -> VcsResult<()> {
    let bad = |what: &str, value: &str| VcsError::InvalidArg {
        what: what.to_string(),
        value: value.to_string(),
    };
    let name = name.trim();
    let email = email.trim();
    if name.is_empty() || name.starts_with('-') || name.contains(['\r', '\n']) {
        return Err(bad("user.name", name));
    }
    if email.is_empty()
        || email.starts_with('-')
        || email.contains(char::is_whitespace)
        || email.matches('@').count() != 1
        || email.starts_with('@')
        || email.ends_with('@')
    {
        return Err(bad("user.email", email));
    }
    Ok(())
}

/// An `includeIf` condition the platform will write: a `hasconfig:remote.*.url:`
/// or `gitdir:` condition, one line, without the quote that would end the
/// section header early. Pure.
fn validate_include_condition(condition: &str) -> VcsResult<()> {
    let bad = || VcsError::InvalidArg {
        what: "includeIf condition".to_string(),
        value: condition.to_string(),
    };
    let ok_prefix = condition.starts_with("hasconfig:remote.*.url:")
        || condition.starts_with("gitdir:")
        || condition.starts_with("gitdir/i:");
    if !ok_prefix
        || condition.contains(['"', '\r', '\n', '\0'])
        || condition.contains(char::is_whitespace)
    {
        return Err(bad());
    }
    Ok(())
}

/// A file the global config may include: absolute, one line, not
/// option-shaped. Pure.
fn validate_include_path(file: &Path) -> VcsResult<PathBuf> {
    let text = file.to_string_lossy();
    if !file.is_absolute() || text.starts_with('-') || text.contains(['\r', '\n', '\0']) {
        return Err(VcsError::InvalidArg {
            what: "include path".to_string(),
            value: text.into_owned(),
        });
    }
    Ok(file.to_path_buf())
}

/// A key path `core.sshCommand` can carry unquoted: absolute, no `..`, and
/// only the characters a key file is ever named with. Pure.
pub fn is_key_path(text: &str) -> bool {
    text.starts_with('/')
        && !text.contains("..")
        && text.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'/' | b'@' | b'+' | b'-')
        })
}

/// Create `file` owner-only when it does not exist yet (and its directory),
/// so a profile's author and key path are never world-readable. An existing
/// file keeps its mode.
fn create_owner_only(file: &Path) -> VcsResult<()> {
    let io = |e: std::io::Error| VcsError::other(format!("{}: {e}", file.display()));
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(io)?;
    }
    if file.exists() {
        return Ok(());
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    match options.open(file) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(io(e)),
    }
}

/// Read `Name <email>` — the one line the CLI's `--committer` takes and
/// [`Ident`] prints — into a validated [`Ident`]. Anything without exactly
/// one `<…>` tail, or that [`validate_identity`] refuses, is `InvalidArg`
/// naming `committer`. Pure — nothing is spawned.
pub fn parse_ident(text: &str) -> VcsResult<Ident> {
    let bad = || VcsError::InvalidArg {
        what: "committer".to_string(),
        value: text.to_string(),
    };
    let text = text.trim();
    let (name, rest) = text.split_once('<').ok_or_else(bad)?;
    let email = rest.strip_suffix('>').ok_or_else(bad)?;
    if email.contains(['<', '>']) {
        return Err(bad());
    }
    validate_identity(name, email).map_err(|_| bad())?;
    Ok(Ident {
        name: name.trim().to_string(),
        email: email.trim().to_string(),
    })
}

pub fn validate_ref(what: &str, name: &str) -> VcsResult<()> {
    validate_value(what, name)?;
    let bad = || {
        Err(VcsError::InvalidArg {
            what: what.to_string(),
            value: name.to_string(),
        })
    };
    if name.contains("..")
        || name.contains("//")
        || name.contains("@{")
        || name.ends_with(".lock")
        || name.ends_with('/')
        || name.ends_with('.')
        || name.starts_with('/')
        || name == "@"
        || name
            .chars()
            .any(|c| matches!(c, '~' | '^' | ':' | '?' | '*' | '[' | '\\'))
    {
        return bad();
    }
    Ok(())
}

/// Check one path a caller selected, and turn it into a pathspec that can
/// only ever mean that one path.
///
/// **Deliberately not [`validate_value`].** That one refuses whitespace, and
/// `parse.rs` says why it must not be used here: a filename with a space, a
/// quote or a `é` in it is not an edge case, it is Tuesday. Refusing those
/// would refuse to stage half the files on a real machine.
///
/// What is refused instead is everything that could mean *something other
/// than the file the user pointed at*:
///
/// | Refused | Because |
/// |---|---|
/// | empty | selects nothing, and `git add --` with nothing is not a no-op everywhere |
/// | a leading `-` | read as an option by any call site that forgets its `--` |
/// | a leading `:` | git's pathspec **magic** — `:/`, `:(exclude)…`, `:(glob)…` reach far beyond one file |
/// | an absolute path | a selection is repository-relative; an absolute path is a different repository's business |
/// | any `..` component | `a/../../etc` leaves the repository, which no selection ever means to do |
/// | a control character or NUL | git's own porcelain cannot round-trip it, so we would be staging something we cannot name back |
///
/// What survives is wrapped as `:(top,literal)<path>`, which pins both of the
/// remaining ambiguities: **literal** stops a `*` or `[` inside a legitimate
/// filename from being read as a glob that matches half the tree, and **top**
/// anchors the path at the repository root so it means the same thing no
/// matter which directory the invocation runs from.
fn literal_pathspec(what: &str, spec: &str) -> VcsResult<String> {
    let bad = || {
        Err(VcsError::InvalidArg {
            what: what.to_string(),
            value: spec.to_string(),
        })
    };
    if spec.is_empty()
        || spec.starts_with('-')
        || spec.starts_with(':')
        || spec.chars().any(char::is_control)
    {
        return bad();
    }
    let path = Path::new(spec);
    if path.is_absolute() || spec.starts_with('/') || spec.starts_with('\\') {
        return bad();
    }
    if path.components().any(|c| {
        matches!(
            c,
            std::path::Component::ParentDir | std::path::Component::Prefix(_)
        )
    }) {
        return bad();
    }
    Ok(format!(":(top,literal){spec}"))
}

/// Join `--numstat` counts with `--name-status` kinds on the path. A path with
/// counts and no kind is a modification; git never omits the other way round.
fn merge_stats(
    nums: Vec<parse::NumStat>,
    kinds: Vec<(PathBuf, ChangeKind, Option<PathBuf>)>,
) -> Vec<FileChange> {
    nums.into_iter()
        .map(|n| {
            let kind = kinds
                .iter()
                .find(|(p, _, _)| *p == n.path)
                .map(|(_, k, _)| *k)
                .unwrap_or(ChangeKind::Modified);
            FileChange {
                path: n.path,
                old_path: n.old_path,
                kind,
                insertions: n.insertions,
                deletions: n.deletions,
                binary: n.binary,
            }
        })
        .collect()
}

/// The revisions `diff-tree` compares to read one commit against its first
/// parent: the parent and the commit when there is one, the commit alone
/// with `--root` for a root commit. Named explicitly because `-m
/// --first-parent` on a single merge commit still diffs against every
/// parent — the flag narrows a walk, not a single commit's parents.
fn against_first_parent(parent: Option<&str>, commit: &str) -> Vec<OsString> {
    match parent {
        Some(parent) => vec![s(parent), s(commit)],
        None => vec![s("--root"), s(commit)],
    }
}

pub(crate) fn literal_pathspecs<S: AsRef<str>>(what: &str, specs: &[S]) -> VcsResult<Vec<String>> {
    specs
        .iter()
        .map(|spec| literal_pathspec(what, spec.as_ref()))
        .collect()
}

/// What a verb about to write reads of a selection, so that a path the
/// Changes list showed a moment ago — and an agent has deleted since — is
/// left out instead of failing the whole batch ([`Git::still_known`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reach {
    /// The tree as `git add` reads it: every index entry — a tracked file
    /// deleted from disk is still one, and its deletion stages — and every
    /// file on disk git does not ignore.
    Worktree,
    /// The index alone, as `git checkout --` and `git stash push` without
    /// untracked files read it.
    Index,
    /// The paths whose index differs from HEAD — the empty tree before a
    /// first commit — as `git restore --staged` reads them: any other path
    /// is a no-op to unstage, or git's refusal.
    Staged,
}

/// Whether `entry`, a path git listed relative to the top, is `selected`:
/// the path itself or, for a folder, a path under it.
fn covers(selected: &str, entry: &str) -> bool {
    let selected = selected.trim_end_matches('/');
    entry == selected
        || entry
            .strip_prefix(selected)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// Make a path absolute without requiring it to exist.
///
/// Two reasons. `git worktree add` takes its path positionally with no `--`
/// escape, and a leading `/` is the one thing that cannot be mistaken for an
/// option. And these commands run with an explicit `-C`, so a relative path
/// would resolve against the *repository*, not the caller's cwd — quietly the
/// wrong directory.
fn absolutize(path: &Path) -> VcsResult<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    let cwd = std::env::current_dir()
        .map_err(|e| VcsError::other(format!("resolving {}: {e}", path.display())))?;
    Ok(cwd.join(path))
}

// ---------------------------------------------------------------------------
// Failure classification
// ---------------------------------------------------------------------------

/// Turn a failed git invocation into a typed error.
///
/// This is the *only* place in the tree that reads git's prose, and it is the
/// boundary the "never parse prose" rule allows: parse once, here, into a
/// type; everything upstream switches on the type. `LC_ALL=C` in
/// `Git::command` is what makes these needles stable. Anything unrecognised
/// stays [`VcsError::Command`] with stderr intact rather than being forced
/// into a variant that would lie about it.
pub(crate) fn classify(what: &str, cwd: Option<&Path>, out: &Output) -> VcsError {
    // Every word below ends in an error somebody reads: git's own output is
    // scrubbed of a remote's credentials before any of it is kept.
    let stderr = crate::exec::scrub_userinfo(&out.stderr_text());
    // `merge` and `cherry-pick` report a conflict on stdout and say nothing on
    // stderr, so both streams are read; the words are the same either way.
    let stdout = crate::exec::scrub_userinfo(&out.stdout_text());
    let hay = format!("{stderr}\n{stdout}").to_ascii_lowercase();
    let has = |needle: &str| hay.contains(needle);
    let here = || cwd.map(Path::to_path_buf).unwrap_or_default();

    if has("not a git repository")
        || has("this operation must be run in a work tree")
        || has("not a working tree")
    {
        return VcsError::NotARepository(here());
    }
    if has("index.lock") || has("another git process seems to be running") {
        return VcsError::RepositoryBusy { path: here() };
    }
    if has("no such remote") || has("does not appear to be a git repository") {
        return VcsError::NoRemote(stderr);
    }
    if has("could not read username")
        || has("could not read password")
        || has("authentication failed")
        || has("terminal prompts disabled")
        || has("permission denied (publickey")
        || has("could not read from remote repository")
    {
        return VcsError::NotAuthenticated(stderr);
    }
    if has("please tell me who you are")
        || has("unable to auto-detect email address")
        || has("empty ident name")
    {
        return VcsError::IdentityUnset(stderr);
    }
    if has("contains modified or untracked files")
        || has("your local changes to the following files would be overwritten")
        || has("cannot rebase: you have unstaged changes")
    {
        return VcsError::Dirty {
            path: here(),
            details: stderr,
        };
    }
    if has("conflict") || has("needs merge") || has("unmerged files") {
        // The paths and the operation left in progress are the repository's
        // to tell; the consented tier asks it and fills them in.
        return VcsError::Conflict {
            message: if stderr.trim().is_empty() {
                stdout
            } else {
                stderr
            },
            paths: Vec::new(),
            in_progress: None,
        };
    }
    VcsError::Command {
        what: what.to_string(),
        code: out.code.clone(),
        stderr,
    }
}

// ---------------------------------------------------------------------------
// Free functions — the default `git`
// ---------------------------------------------------------------------------

/// See [`Git::probe`].
pub fn probe() -> ProbeResult {
    shared().probe()
}
/// See [`Git::is_repo`].
pub fn is_repo(path: &Path) -> bool {
    shared().is_repo(path)
}
/// See [`Git::repo_root`].
pub fn repo_root(path: &Path) -> VcsResult<PathBuf> {
    shared().repo_root(path)
}
/// See [`Git::common_dir`].
pub fn common_dir(path: &Path) -> VcsResult<PathBuf> {
    shared().common_dir(path)
}
/// See [`Git::default_branch`].
pub fn default_branch(path: &Path) -> VcsResult<String> {
    shared().default_branch(path)
}
/// See [`Git::remote_default_branch`].
pub fn remote_default_branch(path: &Path) -> VcsResult<Option<String>> {
    shared().remote_default_branch(path)
}
/// See [`Git::branch_exists`].
pub fn branch_exists(path: &Path, name: &str) -> VcsResult<bool> {
    shared().branch_exists(path, name)
}
/// See [`Git::init`].
pub fn init(path: &Path) -> VcsResult<()> {
    shared().init(path)
}
/// See [`Git::clone`].
pub fn clone(url: &str, dest: &Path, depth: Option<u32>) -> VcsResult<()> {
    shared().clone(url, dest, depth)
}
/// See [`Git::worktree_add`].
pub fn worktree_add(repo: &Path, path: &Path, branch: &str, base: &str) -> VcsResult<()> {
    shared().worktree_add(repo, path, branch, base)
}
/// See [`Git::worktree_add_existing`].
pub fn worktree_add_tracking(
    repo: &Path,
    path: &Path,
    branch: &str,
    remote: &str,
    remote_branch: &str,
) -> VcsResult<()> {
    shared().worktree_add_tracking(repo, path, branch, remote, remote_branch)
}
pub fn worktree_add_existing(repo: &Path, path: &Path, branch: &str) -> VcsResult<()> {
    shared().worktree_add_existing(repo, path, branch)
}
/// See [`Git::worktree_add_detached`].
pub fn worktree_add_detached(repo: &Path, path: &Path, commitish: Option<&str>) -> VcsResult<()> {
    shared().worktree_add_detached(repo, path, commitish)
}
/// See [`Git::worktree_list`].
pub fn worktree_list(repo: &Path) -> VcsResult<Vec<WorktreeEntry>> {
    shared().worktree_list(repo)
}
/// See [`Git::worktree_remove`].
pub fn worktree_remove(path: &Path, force: bool) -> VcsResult<()> {
    shared().worktree_remove(path, force)
}
/// See [`Git::branch_delete`].
pub fn branch_delete(path: &Path, name: &str) -> VcsResult<()> {
    shared().branch_delete(path, name)
}
/// See [`Git::worktree_prune`].
pub fn worktree_prune(repo: &Path) -> VcsResult<()> {
    shared().worktree_prune(repo)
}
/// See [`Git::status`].
pub fn status(path: &Path) -> VcsResult<Status> {
    shared().status(path)
}
/// See [`Git::status_files`].
pub fn status_files(path: &Path) -> VcsResult<Vec<FileStatus>> {
    shared().status_files(path)
}
/// See [`Git::diff`].
pub fn diff(path: &Path, staged: bool) -> VcsResult<String> {
    shared().diff(path, staged)
}
/// See [`Git::diff_file`].
pub fn diff_file(path: &Path, pathspec: &str, staged: bool) -> VcsResult<String> {
    shared().diff_file(path, pathspec, staged)
}
/// See [`Git::stash_list`].
pub fn stash_list(path: &Path) -> VcsResult<Vec<StashEntry>> {
    shared().stash_list(path)
}
/// See [`Git::stash_diff`].
pub fn stash_diff(path: &Path, commit: &str) -> VcsResult<String> {
    shared().stash_diff(path, commit)
}
/// See [`Git::stage`].
pub fn stage<S: AsRef<str>>(path: &Path, pathspecs: &[S]) -> VcsResult<()> {
    shared().stage(path, pathspecs)
}
/// See [`Git::unstage`].
pub fn unstage<S: AsRef<str>>(path: &Path, pathspecs: &[S]) -> VcsResult<()> {
    shared().unstage(path, pathspecs)
}
/// See [`Git::apply_cached`].
pub fn apply_cached(path: &Path, patch: &str, reverse: bool) -> VcsResult<()> {
    shared().apply_cached(path, patch, reverse)
}
/// See [`Git::blame`].
pub fn blame(path: &Path, pathspec: &str, range: Option<(u32, u32)>) -> VcsResult<Vec<BlameLine>> {
    shared().blame(path, pathspec, range)
}
/// See [`Git::file_history`].
pub fn file_history(path: &Path, pathspec: &str, limit: usize) -> VcsResult<Vec<CommitSummary>> {
    shared().file_history(path, pathspec, limit)
}
/// See [`Git::branch_list`].
pub fn branch_list(path: &Path, merged_into: Option<&str>) -> VcsResult<Vec<BranchInfo>> {
    shared().branch_list(path, merged_into)
}
/// See [`Git::branch_set_upstream`].
pub fn branch_set_upstream(path: &Path, branch: &str, upstream: Option<&str>) -> VcsResult<()> {
    shared().branch_set_upstream(path, branch, upstream)
}
/// See [`Git::changed_paths`].
pub fn changed_paths(path: &Path, from: &str, to: &str, limit: usize) -> VcsResult<Vec<String>> {
    shared().changed_paths(path, from, to, limit)
}
/// See [`Git::branch_diff`].
pub fn branch_diff(path: &Path, base: &str) -> VcsResult<String> {
    shared().branch_diff(path, base)
}
/// See [`Git::commits_between`].
pub fn commits_between(
    path: &Path,
    from: &str,
    to: &str,
    limit: usize,
) -> VcsResult<Vec<CommitSummary>> {
    shared().commits_between(path, from, to, limit)
}
/// See [`Git::tag_list`].
pub fn remote_branch_list(path: &Path) -> VcsResult<Vec<RemoteBranchInfo>> {
    shared().remote_branch_list(path)
}
pub fn tag_list(path: &Path) -> VcsResult<Vec<TagInfo>> {
    shared().tag_list(path)
}
/// See [`Git::remote_list`].
pub fn remote_list(path: &Path) -> VcsResult<Vec<RemoteInfo>> {
    shared().remote_list(path)
}
/// See [`Git::branch_create`].
pub fn branch_create(path: &Path, name: &str, start: Option<&str>, track: bool) -> VcsResult<()> {
    shared().branch_create(path, name, start, track)
}
/// See [`Git::recovery_list`].
pub fn tag_create(path: &Path, name: &str, target: Option<&str>) -> VcsResult<()> {
    shared().tag_create(path, name, target)
}
pub fn recovery_list(path: &Path) -> VcsResult<Vec<RecoveryRef>> {
    shared().recovery_list(path)
}
/// See [`Git::blob`].
pub fn blob(path: &Path, rev: BlobRev, pathspec: &str) -> VcsResult<Option<Vec<u8>>> {
    shared().blob(path, rev, pathspec)
}
/// See [`Git::conflict_blobs`].
pub fn conflict_blobs(path: &Path, pathspec: &str) -> VcsResult<ConflictBlobs> {
    shared().conflict_blobs(path, pathspec)
}
/// See [`Git::merge_preview`].
pub fn merge_preview(path: &Path, ours: &str, theirs: &str) -> VcsResult<Option<MergePreview>> {
    shared().merge_preview(path, ours, theirs)
}
/// See [`Git::operation_facts`].
pub fn operation_facts(path: &Path) -> VcsResult<Option<OperationFacts>> {
    shared().operation_facts(path)
}
/// See [`Git::graph_log`].
pub fn graph_log(path: &Path, limit: Option<usize>, refs: RefScope) -> VcsResult<Vec<GraphCommit>> {
    shared().graph_log(path, limit, refs)
}
/// See [`Git::refs_fingerprint`].
pub fn refs_fingerprint(path: &Path) -> VcsResult<String> {
    shared().refs_fingerprint(path)
}
/// See [`Git::commit_detail`].
pub fn commit_detail(path: &Path, commit: &str) -> VcsResult<CommitDetail> {
    shared().commit_detail(path, commit)
}
/// See [`Git::commit_diff`].
pub fn commit_diff(path: &Path, commit: &str, paths: &[&str]) -> VcsResult<String> {
    shared().commit_diff(path, commit, paths)
}
/// See [`Git::diff_head`].
pub fn diff_head(path: &Path) -> VcsResult<String> {
    shared().diff_head(path)
}
/// See [`Git::diff_stat`].
pub fn diff_stat(path: &Path) -> VcsResult<Vec<FileChange>> {
    shared().diff_stat(path)
}
/// See [`Git::list_untracked`].
pub fn list_untracked(path: &Path) -> VcsResult<Vec<PathBuf>> {
    shared().list_untracked(path)
}
/// See [`Git::add_all`].
pub fn add_all(path: &Path) -> VcsResult<()> {
    shared().add_all(path)
}
/// See [`Git::identity`].
pub fn identity(path: &Path) -> VcsResult<GitIdentity> {
    shared().identity(path)
}
/// See [`Git::global_identity`].
pub fn global_identity() -> VcsResult<Option<Ident>> {
    shared().global_identity()
}
/// See [`Git::credential_fill`].
pub fn credential_fill(protocol: &str, host: &str) -> VcsResult<Option<Credential>> {
    shared().credential_fill(protocol, host)
}
/// See [`Git::credential_helpers`].
pub fn credential_helpers() -> VcsResult<Vec<String>> {
    shared().credential_helpers()
}
/// See [`Git::version`].
pub fn version() -> VcsResult<GitVersion> {
    shared().version()
}
/// See [`Git::includes`].
pub fn includes() -> VcsResult<Vec<Include>> {
    shared().includes()
}
/// See [`Git::config_origin`].
pub fn config_origin(path: Option<&Path>, key: &str) -> VcsResult<Option<ConfigOrigin>> {
    shared().config_origin(path, key)
}
/// See [`Git::account_get`].
pub fn account_get(path: &Path) -> VcsResult<Option<String>> {
    shared().account_get(path)
}
/// See [`Git::ls_remote`].
pub fn ls_remote(path: &Path, remote: &str) -> VcsResult<Vec<RemoteRef>> {
    shared().ls_remote(path, remote)
}
/// See [`Git::set_local_identity`].
pub fn set_local_identity(path: &Path, name: &str, email: &str) -> VcsResult<GitIdentity> {
    shared().set_local_identity(path, name, email)
}
/// See [`Git::config_view`].
pub fn config_view(path: Option<&Path>) -> VcsResult<GitConfigView> {
    shared().config_view(path)
}
/// See [`Git::config_set`].
pub fn config_set(
    scope: ConfigScope,
    path: Option<&Path>,
    key: &str,
    value: &str,
) -> VcsResult<()> {
    shared().config_set(scope, path, key, value)
}
/// See [`Git::config_unset`].
pub fn config_unset(scope: ConfigScope, path: Option<&Path>, key: &str) -> VcsResult<()> {
    shared().config_unset(scope, path, key)
}
/// See [`Git::commit`].
pub fn commit(path: &Path, message: &str, allow_empty: bool) -> VcsResult<CommitId> {
    shared().commit(path, message, allow_empty)
}
/// See [`Git::head`].
pub fn head(path: &Path) -> VcsResult<CommitId> {
    shared().head(path)
}
/// See [`Git::log`].
pub fn log(path: &Path, base: Option<&str>, limit: usize) -> VcsResult<Vec<CommitSummary>> {
    shared().log(path, base, limit)
}
/// See [`Git::ahead_behind`].
pub fn ahead_behind(path: &Path, base: &str) -> VcsResult<(u32, u32)> {
    shared().ahead_behind(path, base)
}
/// See [`Git::remote_get`].
pub fn remote_get(path: &Path, name: &str) -> VcsResult<Option<String>> {
    shared().remote_get(path, name)
}
/// See [`Git::remote_add`].
pub fn remote_add(path: &Path, name: &str, url: &str) -> VcsResult<()> {
    shared().remote_add(path, name, url)
}
/// See [`Git::push`].
pub fn push(path: &Path, remote: &str, branch: &str, set_upstream: bool) -> VcsResult<()> {
    shared().push(path, remote, branch, set_upstream)
}
/// See [`Git::fetch`].
pub fn fetch(path: &Path, remote: &str) -> VcsResult<()> {
    shared().fetch(path, remote)
}
/// See [`Git::remote_set_url`].
pub fn fetch_branch(path: &Path, remote: &str, branch: &str) -> VcsResult<()> {
    shared().fetch_branch(path, remote, branch)
}
pub fn remote_set_url(path: &Path, name: &str, url: &str) -> VcsResult<()> {
    shared().remote_set_url(path, name, url)
}
/// See [`Git::ensure_remote`].
pub fn ensure_remote(path: &Path, name: &str, url: &str) -> VcsResult<()> {
    shared().ensure_remote(path, name, url)
}
/// See [`Git::upstream_of`].
pub fn upstream_of(path: &Path) -> VcsResult<Option<String>> {
    shared().upstream_of(path)
}
/// See [`Git::can_fast_forward`].
pub fn can_fast_forward(path: &Path, target: &str) -> VcsResult<bool> {
    shared().can_fast_forward(path, target)
}
/// See [`Git::in_progress`].
pub fn in_progress(path: &Path) -> VcsResult<Option<crate::interactive::InProgress>> {
    shared().in_progress(path)
}
/// See [`Git::conflicted_paths`].
pub fn conflicted_paths(path: &Path) -> VcsResult<Vec<PathBuf>> {
    shared().conflicted_paths(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_that_is_not_there_is_not_available() {
        let err = Git::new()
            .with_program("bisa-no-such-git-binary")
            .version()
            .expect_err("no such program");
        let words = err.to_string();
        assert!(
            !words.is_empty(),
            "the sentence names what went wrong: {words}"
        );
    }

    fn out(stderr: &str) -> Output {
        Output {
            success: false,
            code: "128".into(),
            stdout: Vec::new(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    #[test]
    fn command_carries_the_hardened_environment_and_never_takes_an_optional_lock() {
        let git = Git::new()
            .with_env("GIT_OPTIONAL_LOCKS", "1")
            .with_env("GIT_CONFIG_NOSYSTEM", "1");
        let cmd = git.command(None);
        let env: Vec<(String, String)> = cmd
            .get_envs()
            .filter_map(|(k, v)| {
                v.map(|v| {
                    (
                        k.to_string_lossy().into_owned(),
                        v.to_string_lossy().into_owned(),
                    )
                })
            })
            .collect();
        let value = |name: &str| {
            env.iter()
                .rev()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(
            value("GIT_OPTIONAL_LOCKS"),
            Some("0"),
            "a read never takes index.lock; the seam cannot turn it back on"
        );
        assert_eq!(value("GIT_TERMINAL_PROMPT"), Some("0"));
        assert_eq!(value("LC_ALL"), Some("C"));
        assert_eq!(
            value("GIT_CONFIG_NOSYSTEM"),
            Some("1"),
            "a name outside the hardened set is the caller's"
        );
        assert_eq!(HARDENED_ENV.len(), 6);
    }

    #[test]
    fn a_removal_and_a_config_pair_ride_every_invocation_and_cannot_unharden() {
        let git = Git::new()
            .without_env("HTTPS_PROXY")
            .without_env("GIT_TERMINAL_PROMPT")
            .with_config("http.version", "HTTP/1.1");
        let cmd = git.command(None);
        let removed: Vec<String> = cmd
            .get_envs()
            .filter(|(_, v)| v.is_none())
            .map(|(k, _)| k.to_string_lossy().into_owned())
            .collect();
        assert_eq!(removed, vec!["HTTPS_PROXY"], "the hardened name is refused");
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, vec!["--no-pager", "-c", "http.version=HTTP/1.1"]);
    }

    #[test]
    fn classifies_captured_git_stderr() {
        // Every needle below is verbatim stderr from git 2.50 under LC_ALL=C.
        type Want = fn(&VcsError) -> bool;
        let cases: &[(&str, Want)] = &[
            (
                "fatal: not a git repository (or any of the parent directories): .git",
                |e| matches!(e, VcsError::NotARepository(_)),
            ),
            (
                "fatal: Unable to create '/x/.git/index.lock': File exists.\n\nAnother git process seems to be running in this repository, e.g.\nan editor opened by 'git commit'. Please make sure all processes\nare terminated then try again.",
                |e| matches!(e, VcsError::RepositoryBusy { .. }),
            ),
            ("error: No such remote 'nope'", |e| {
                matches!(e, VcsError::NoRemote(_))
            }),
            (
                "fatal: 'nope' does not appear to be a git repository\nfatal: Could not read from remote repository.",
                |e| matches!(e, VcsError::NoRemote(_)),
            ),
            (
                "fatal: could not read Username for 'https://github.com': terminal prompts disabled",
                |e| matches!(e, VcsError::NotAuthenticated(_)),
            ),
            (
                "*** Please tell me who you are.\nfatal: unable to auto-detect email address (got 'u@h.(none)')",
                |e| matches!(e, VcsError::IdentityUnset(_)),
            ),
            (
                "fatal: '/tmp/wt' contains modified or untracked files, use --force to delete it",
                |e| matches!(e, VcsError::Dirty { .. }),
            ),
            (
                "CONFLICT (content): Merge conflict in c.txt\nAutomatic merge failed",
                |e| matches!(e, VcsError::Conflict { paths, in_progress: None, .. } if paths.is_empty()),
            ),
            ("fatal: a branch named 'dirtybr' already exists", |e| {
                matches!(e, VcsError::Command { .. })
            }),
        ];
        for (stderr, want) in cases {
            let err = classify("git x", Some(Path::new("/tmp/r")), &out(stderr));
            assert!(want(&err), "misclassified {stderr:?} as {err}");
        }
    }

    #[test]
    fn option_injection_is_refused() {
        for evil in [
            "--upload-pack=evil",
            "-u",
            "--exec=touch /tmp/pwned",
            "",
            "with space",
            "tab\there",
        ] {
            assert!(
                validate_value("branch", evil).is_err(),
                "must reject {evil:?}"
            );
            assert!(
                validate_ref("branch", evil).is_err(),
                "must reject {evil:?}"
            );
        }
    }

    #[test]
    fn ref_rules_reject_what_git_would() {
        for bad in [
            "a..b",
            "a//b",
            "a@{0}",
            "x.lock",
            "trail/",
            "/lead",
            "@",
            "ca^ret",
            "co:lon",
            "ti~lde",
            "star*",
            "brack[et",
            "back\\slash",
            "dot.",
        ] {
            assert!(validate_ref("branch", bad).is_err(), "must reject {bad:?}");
        }
        for good in [
            "main",
            "feature/checkout",
            "fix-cart-total",
            "release/2.1",
            // Shell metacharacters are legal in a ref name and harmless in an
            // argv array; refusing them would be theatre.
            "evil;touch;pwned.txt",
        ] {
            assert!(validate_ref("branch", good).is_ok(), "must accept {good:?}");
        }
    }

    #[test]
    fn commit_id_short_is_char_safe() {
        assert_eq!(CommitId::new("828ac972b950e17").short(), "828ac97");
        assert_eq!(CommitId::new("abc").short(), "abc");
    }
}
