//! The **folder repositories** (ide/04 §The notes repository; 19 —
//! Drawings): `<data_dir>/notes/` and `<data_dir>/drawings/`, each a git
//! repository the person commits and pushes from its overlay.
//!
//! The store writes the files; this module owns the repository around them,
//! once for both folders. It is made lazily — the first record written, or
//! the first time the overlay asks — and only the safe tier touches it:
//! `add --all` and `commit`, `push` with the upstream set, `fetch`, the
//! remote's URL, the local identity. The one consented act is a fast-forward
//! `pull`, which captures a safety ref first like every consented act — and
//! which only the notes offer: a drawing's record arrives by sync, and a pull
//! that changed its file would be overwritten by the next ingest.
//!
//! **No Publish gate.** The gate is a project's policy with a goal to ask;
//! these folders belong to no project and no goal, and the only hand that
//! pushes them is the person's own, on a button. A push here leaves the
//! machine the moment it is pressed, and the overlay says so in words.
//!
//! The status is cached on `cache.git_status.ttl_ms` and dropped by every
//! write, so an overlay's strip reads *3 changes* the moment a record saves.

use crate::projects::blocking;
use crate::{EngineError, Inner};
use bisa_core::AgentId;
use bisa_store::Paths;
use bisa_vcs::git::CommitSummary;
use bisa_vcs::interactive::{ops, HumanConsent, PullMode, PullOutcome, Recovery};
use bisa_vcs::{GitIdentity, VcsError};
use std::path::PathBuf;
use std::time::Duration;

/// The remote every act names.
pub const ORIGIN: &str = "origin";

/// The commit-message brief for notes — the change is prose, not code.
const NOTES_BRIEF: &str = "\
Write the commit message for the change below, which is to a person's notes \
— Markdown files, one note each, with a small front matter block. Reply with \
the message and nothing else — no preamble, no code fences. A short \
imperative subject line under 72 characters naming what the notes now say \
(a note added, one changed, one removed — by title where the diff shows it); \
then, only if it helps, a blank line and a brief body. Describe what the diff \
shows, and do not invent a motive it does not support.";

/// The commit-message brief for drawings — the change is a picture, kept as
/// Excalidraw's JSON, and the file's `bisa.title` names it.
const DRAWINGS_BRIEF: &str = "\
Write the commit message for the change below, which is to a person's drawings \
— `.excalidraw` JSON files, one drawing each, whose `bisa.title` field names \
the drawing. Reply with the message and nothing else — no preamble, no code \
fences. A short imperative subject line under 72 characters naming what the \
drawings now show (a drawing added, one changed, one removed — by title where \
the diff shows it); then, only if it helps, a blank line and a brief body. \
Describe what the diff shows, and do not invent a motive it does not support.";

/// Which folder a repository is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Folder {
    Notes,
    Drawings,
}

impl Folder {
    /// The repository's root under the workspace.
    pub fn root(self, paths: &Paths) -> PathBuf {
        match self {
            Folder::Notes => paths.notes_dir(),
            Folder::Drawings => paths.drawings_dir(),
        }
    }

    /// The word a refusal or a log line calls the records.
    pub fn word(self) -> &'static str {
        match self {
            Folder::Notes => "notes",
            Folder::Drawings => "drawings",
        }
    }

    fn brief(self) -> &'static str {
        match self {
            Folder::Notes => NOTES_BRIEF,
            Folder::Drawings => DRAWINGS_BRIEF,
        }
    }

    /// What the repository excludes — written to its own `.git/info/exclude`,
    /// never a tracked `.gitignore`, so it is neither a change to commit nor
    /// a file a clone carries: the drawings' `state/` holds the signed
    /// snapshots, which are the records themselves and never the export.
    fn ignore(self) -> &'static [&'static str] {
        match self {
            Folder::Notes => &[],
            Folder::Drawings => &["state/"],
        }
    }

    /// Whether the folder offers a pull: notes do, drawings do not (their
    /// records arrive by sync; the files are an export).
    pub fn pulls(self) -> bool {
        matches!(self, Folder::Notes)
    }

    fn cache_name(self) -> &'static str {
        match self {
            Folder::Notes => "notes.git_status",
            Folder::Drawings => "drawings.git_status",
        }
    }
}

/// The last commit, as the strip reads it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RepoCommit {
    pub short: String,
    pub subject: String,
    /// Author time, unix seconds.
    pub at: u64,
}

impl From<CommitSummary> for RepoCommit {
    fn from(c: CommitSummary) -> Self {
        Self {
            short: c.short,
            subject: c.subject,
            at: c.timestamp,
        }
    }
}

/// Where a folder repository stands. The node's DTO mirrors it with the
/// identity in its own view.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RepoStatus {
    /// Files changed since the last commit — staged, unstaged and new alike;
    /// a commit here takes them all.
    pub changed: u32,
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// `origin`'s URL, when set.
    pub remote: Option<String>,
    pub identity: GitIdentity,
    pub last_commit: Option<RepoCommit>,
    /// A merge or rebase half-done — a pull that could not fast-forward, say.
    pub in_progress: Option<String>,
}

/// One folder's repository: which folder, and its one cached status over
/// the shared TTL toolkit.
pub struct FolderGit {
    folder: Folder,
    status: bisa_cache::TtlCell<RepoStatus>,
}

impl FolderGit {
    pub fn new(folder: Folder) -> Self {
        Self {
            folder,
            status: bisa_cache::TtlCell::new(folder.cache_name()),
        }
    }

    pub fn folder(&self) -> Folder {
        self.folder
    }

    fn fresh(&self, ttl: Duration) -> Option<RepoStatus> {
        self.status.get(ttl)
    }

    /// Forget the status — after any write to the records or the repository.
    pub fn invalidate(&self) {
        self.status.clear();
    }

    fn root(&self, inner: &Inner) -> PathBuf {
        self.folder.root(inner.ws.paths())
    }

    /// The repository, made if it is not one yet — with its excludes where
    /// the folder has any. Idempotent and cheap once made.
    pub fn ensure(&self, inner: &Inner) -> Result<PathBuf, EngineError> {
        let path = self.root(inner);
        let git = inner.git();
        if !git.is_repo(&path) {
            git.init(&path)?;
        }
        // `git init` writes a commented exclude file of its own: the folder's
        // lines are appended once, and left alone when they are there.
        let ignore = self.folder.ignore();
        if !ignore.is_empty() {
            let file = Self::exclude_file(&path);
            let existing = std::fs::read_to_string(&file).unwrap_or_default();
            let missing: Vec<&str> = ignore
                .iter()
                .copied()
                .filter(|line| !existing.lines().any(|have| have.trim() == *line))
                .collect();
            if !missing.is_empty() {
                let mut body = existing;
                if !body.is_empty() && !body.ends_with('\n') {
                    body.push('\n');
                }
                for line in missing {
                    body.push_str(line);
                    body.push('\n');
                }
                bisa_store::write_atomic(&file, body.as_bytes())?;
            }
        }
        Ok(path)
    }

    /// The repository's own exclude file: `.git/info/exclude`.
    pub fn exclude_file(root: &std::path::Path) -> PathBuf {
        root.join(".git").join("info").join("exclude")
    }

    /// A record was written or removed: the repository exists and its status is stale.
    pub fn touched(&self, inner: &Inner) {
        if let Err(e) = self.ensure(inner) {
            tracing::warn!(
                "the {} repository could not be made: {e}",
                self.folder.word()
            );
        }
        self.invalidate();
    }

    /// Where the repository stands, from the cache when it is fresh.
    pub async fn status(&self, inner: &Inner) -> Result<RepoStatus, EngineError> {
        let ttl = inner.cache.settings().git_status_ttl();
        if let Some(s) = self.fresh(ttl) {
            return Ok(s);
        }
        let path = self.ensure(inner)?;
        let git = inner.git();
        let status = blocking(move || {
            let s = git.status(&path)?;
            let remote = git.remote_get(&path, ORIGIN)?;
            let identity = git.identity(&path)?;
            let last_commit = if s.oid.is_some() {
                git.log(&path, None, 1)?
                    .into_iter()
                    .next()
                    .map(RepoCommit::from)
            } else {
                None
            };
            let in_progress = git
                .in_progress(&path)?
                .map(|p| format!("{p:?}").to_lowercase());
            Ok(RepoStatus {
                changed: s.staged + s.unstaged + s.untracked + s.conflicted,
                branch: s.branch,
                upstream: s.upstream,
                ahead: s.ahead,
                behind: s.behind,
                remote,
                identity,
                last_commit,
                in_progress,
            })
        })
        .await?;
        self.status.set(status.clone());
        Ok(status)
    }

    /// Commit everything that changed, as one commit. Nobody set to commit is
    /// refused with the folder's own door, not a project's.
    pub async fn commit(&self, inner: &Inner, message: &str) -> Result<RepoCommit, EngineError> {
        let message = message.trim().to_string();
        if message.is_empty() {
            return Err(EngineError::NothingToCommit(
                "a commit needs a message".into(),
            ));
        }
        let path = self.ensure(inner)?;
        let changed = {
            let (git, path) = (inner.git(), path.clone());
            blocking(move || {
                git.status(&path)
                    .map(|s| s.staged + s.unstaged + s.untracked + s.conflicted)
            })
            .await?
        };
        if changed == 0 {
            return Err(EngineError::NothingToCommit(
                "nothing has changed since the last commit".into(),
            ));
        }
        let git = inner.git();
        let done = blocking(move || {
            git.add_all(&path)?;
            git.commit(&path, &message, false)?;
            Ok(git
                .log(&path, None, 1)?
                .into_iter()
                .next()
                .map(RepoCommit::from))
        })
        .await;
        self.invalidate();
        match done {
            Ok(Some(c)) => Ok(c),
            Ok(None) => Err(EngineError::NothingToCommit(
                "the commit left no record".into(),
            )),
            Err(EngineError::Vcs(VcsError::IdentityUnset(_))) => {
                Err(EngineError::RepoIdentityUnset {
                    what: self.folder.word(),
                })
            }
            Err(e) => Err(e),
        }
    }

    /// Ask the General Agent for a message describing the change, read-only —
    /// the folder's brief over the same path the IDE's *Suggest* takes.
    pub async fn suggest(&self, inner: &Inner) -> Result<String, EngineError> {
        let path = self.ensure(inner)?;
        let git = inner.git();
        // The IDE describes what is staged; a folder commits everything, so
        // stage everything first and describe that.
        blocking({
            let path = path.clone();
            move || {
                git.add_all(&path)?;
                Ok(())
            }
        })
        .await?;
        self.invalidate();
        crate::projects::suggest_for_tree(
            inner,
            path,
            self.folder.brief(),
            AgentId::GENERAL,
            crate::ask::Asking::of(bisa_core::AskPurpose::CommitMessage),
        )
        .await
    }

    /// Push the branch to `origin`, setting the upstream the first time. The
    /// person's own act — no gate stands here.
    pub async fn push(&self, inner: &Inner) -> Result<RepoStatus, EngineError> {
        let path = self.ensure(inner)?;
        let (remote, s) = {
            let (git, path) = (inner.git(), path.clone());
            blocking(move || Ok((git.remote_get(&path, ORIGIN)?, git.status(&path)?))).await?
        };
        if remote.is_none() {
            return Err(EngineError::Vcs(VcsError::NoRemote(ORIGIN.into())));
        }
        if s.oid.is_none() {
            return Err(EngineError::NothingToCommit(
                "nothing has been committed yet — commit first".into(),
            ));
        }
        let Some(branch) = s.branch else {
            return Err(EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-repository-has-no-branch-checked-out",
                what = self.folder.word().to_string()
            )));
        };
        let git = inner.git();
        blocking(move || git.push(&path, ORIGIN, &branch, true)).await?;
        self.invalidate();
        self.status(inner).await
    }

    /// Read what `origin` has, without moving anything here.
    pub async fn fetch(&self, inner: &Inner) -> Result<RepoStatus, EngineError> {
        let path = self.ensure(inner)?;
        let git = inner.git();
        blocking(move || git.fetch(&path, ORIGIN)).await?;
        self.invalidate();
        self.status(inner).await
    }

    /// Bring the branch up to `origin` by fast-forward only, a safety ref
    /// first. Anything a fast-forward cannot take is refused in words: the
    /// person resolves that in a terminal, where every git word is at hand.
    /// A folder that does not pull says so.
    pub async fn pull(
        &self,
        inner: &Inner,
        consent: HumanConsent,
    ) -> Result<(Recovery, PullOutcome), EngineError> {
        if !self.folder.pulls() {
            return Err(EngineError::PullNotOffered {
                what: self.folder.word(),
            });
        }
        let path = self.ensure(inner)?;
        let pulled = blocking(move || ops::pull(&path, ORIGIN, PullMode::FfOnly, &consent)).await;
        self.invalidate();
        pulled
    }

    /// Set `origin` — added, or its URL replaced.
    pub async fn set_remote(&self, inner: &Inner, url: &str) -> Result<RepoStatus, EngineError> {
        let url = url.trim().to_string();
        if url.is_empty() {
            return Err(EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-remote-needs-url"
            )));
        }
        let path = self.ensure(inner)?;
        let git = inner.git();
        blocking(move || git.ensure_remote(&path, ORIGIN, &url)).await?;
        self.invalidate();
        self.status(inner).await
    }

    /// Who commits here: the repository's own local pair.
    pub async fn set_identity(
        &self,
        inner: &Inner,
        name: &str,
        email: &str,
    ) -> Result<RepoStatus, EngineError> {
        let path = self.ensure(inner)?;
        let git = inner.git();
        let (name, email) = (name.trim().to_string(), email.trim().to_string());
        blocking(move || git.set_local_identity(&path, &name, &email)).await?;
        self.invalidate();
        self.status(inner).await
    }
}
