//! Code hosts — pull requests, checks, reviews and merges — behind one trait
//! (ide/08).
//!
//! A leaf crate: it depends on no crate of ours, so the engine can drive a
//! code host and the node can describe one without either learning a
//! provider's shape. The **Strategy pattern**, earned by
//! [`CodeHost::capabilities`]: the desktop renders its pull-request form from
//! that answer and never from a GitHub-shaped assumption, and a
//! [`fake::FakeCodeHost`] with every capability off is the test that keeps it so.
//!
//! Three code hosts ship — GitHub, GitLab, Bitbucket ([`CodeHostKind`]) — and
//! each is **the machine's own CLI first, the host's API second**
//! ([`layered::Layered`]): `gh` and `glab`, when they are installed and signed
//! in, do the work with the credential they already hold; what they cannot
//! do, or when they are absent, the API does with a token from the credential
//! chain ([`creds`]) of one **account** — a person may keep several, one per
//! login, and a code host is bound to one before a request with
//! [`CodeHost::for_account`]. [`CodeHost::account`] is how a caller proves the
//! credential works. Bitbucket Cloud has no CLI of its own, so it is the API
//! alone. The CLIs are run through one port ([`cli::CliRunner`]) with a fake
//! beside it, so nothing here is ever tested against a program on the machine.

pub mod bitbucket;
pub mod cli;
pub mod creds;
pub mod fake;
pub mod github;
pub mod gitlab;
pub mod hosts;
pub mod layered;

use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Which code host answered — a [`CodeHostKind`]'s id, or a test's fake.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct CodeHostId(pub &'static str);

/// The kinds of code host this build knows. A kind is an API shape and a CLI,
/// not an address: `github.com` and a GitHub Enterprise host are one kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CodeHostKind {
    GitHub,
    GitLab,
    Bitbucket,
}

impl CodeHostKind {
    pub const ALL: [CodeHostKind; 3] = [
        CodeHostKind::GitHub,
        CodeHostKind::GitLab,
        CodeHostKind::Bitbucket,
    ];

    pub fn id(self) -> CodeHostId {
        CodeHostId(self.as_str())
    }

    /// The wire word: `github` · `gitlab` · `bitbucket`.
    pub fn as_str(self) -> &'static str {
        match self {
            CodeHostKind::GitHub => "github",
            CodeHostKind::GitLab => "gitlab",
            CodeHostKind::Bitbucket => "bitbucket",
        }
    }

    /// The name a person reads.
    pub fn label(self) -> &'static str {
        match self {
            CodeHostKind::GitHub => "GitHub",
            CodeHostKind::GitLab => "GitLab",
            CodeHostKind::Bitbucket => "Bitbucket",
        }
    }

    /// The public instance every remote on it is detected by.
    pub fn public_host(self) -> &'static str {
        match self {
            CodeHostKind::GitHub => "github.com",
            CodeHostKind::GitLab => "gitlab.com",
            CodeHostKind::Bitbucket => "bitbucket.org",
        }
    }

    /// The environment variable that overrides every stored token of this kind.
    pub fn env_var(self) -> &'static str {
        match self {
            CodeHostKind::GitHub => "BISA_GITHUB_TOKEN",
            CodeHostKind::GitLab => "BISA_GITLAB_TOKEN",
            CodeHostKind::Bitbucket => "BISA_BITBUCKET_TOKEN",
        }
    }

    /// The CLI that speaks for this kind, when there is one.
    pub fn cli(self) -> Option<cli::CliProgram> {
        match self {
            CodeHostKind::GitHub => Some(cli::CliProgram::Gh),
            CodeHostKind::GitLab => Some(cli::CliProgram::Glab),
            CodeHostKind::Bitbucket => None,
        }
    }

    /// The page where a person makes a token for the API layer.
    pub fn token_page(self, host: &str) -> String {
        match self {
            CodeHostKind::GitHub => format!("https://{host}/settings/tokens"),
            CodeHostKind::GitLab => {
                format!("https://{host}/-/user_settings/personal_access_tokens")
            }
            CodeHostKind::Bitbucket => {
                "https://id.atlassian.com/manage-profile/security/api-tokens".to_string()
            }
        }
    }

    /// The kind whose public host this is, when it is one.
    pub fn of_host(host: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|k| k.public_host().eq_ignore_ascii_case(host))
    }
}

impl std::str::FromStr for CodeHostKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|k| k.as_str().eq_ignore_ascii_case(s.trim()))
            .ok_or_else(|| format!("{s:?} is not a code host kind: github, gitlab or bitbucket"))
    }
}

impl std::fmt::Display for CodeHostKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How a remote URL reaches its host — the fact the Repository view shows
/// first, because it decides which credential a push will use: a token or
/// git's helper over HTTPS, a key over SSH.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RemoteProtocol {
    /// `https://` or `http://`.
    Https,
    /// `ssh://` or `git+ssh://`.
    Ssh,
    /// The scp-like `git@host:owner/name` — SSH too, but spelled without a
    /// scheme, which is how an SSH `Host` alias appears in a remote.
    Scp,
    /// A path on this machine, or `file://`.
    Local,
    /// A scheme this crate does not read (`git://`, `ftp://`).
    Other,
}

/// A remote URL taken apart, once, for everything that reads one: the code
/// host's `detect`, the Repository view, the profile match. `parse` is
/// **total** over a non-empty string — what is not a URL is a `Local` path —
/// so a caller never meets `None` for a remote git itself accepted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RemoteUrl {
    /// The remote as written, **without what it may hide**: this value is an
    /// API answer and rides in refusals, so an `https` URL's userinfo — a
    /// token as the user, a `user:password` — reads `***`, and an `ssh` URL
    /// keeps its user and loses a password. Nothing runs git with it.
    pub raw: String,
    pub protocol: RemoteProtocol,
    /// The user before `@`, when the URL names one (`git`) — never a
    /// password, and never an `https` URL's, which may be a token.
    pub user: Option<String>,
    /// The host as written — an SSH alias (`github-work`) is a host here; the
    /// engine asks `ssh -G` what it stands for.
    pub host: Option<String>,
    pub port: Option<u16>,
    /// The path after the host, without a leading `/` or a `.git` suffix; the
    /// whole string for a local path.
    pub path: String,
    /// The namespace the repository lives under — every path segment but the
    /// last, joined by `/`: an owner on GitHub, a group and its subgroups on
    /// GitLab, a workspace on Bitbucket. `None` with fewer than two segments.
    pub owner: Option<String>,
    /// The last path segment — the repository's name.
    pub name: Option<String>,
}

impl RemoteUrl {
    pub fn parse(remote: &str) -> Self {
        let raw = remote.trim().to_string();
        let mut url = Self {
            raw: raw.clone(),
            protocol: RemoteProtocol::Local,
            user: None,
            host: None,
            port: None,
            path: raw.clone(),
            owner: None,
            name: None,
        };
        if let Some((scheme, rest)) = raw.split_once("://") {
            let protocol = match scheme.to_ascii_lowercase().as_str() {
                "https" | "http" => RemoteProtocol::Https,
                "ssh" | "git+ssh" | "ssh+git" => RemoteProtocol::Ssh,
                "file" => RemoteProtocol::Local,
                _ => RemoteProtocol::Other,
            };
            url.protocol = protocol;
            if protocol == RemoteProtocol::Local {
                url.path = rest.to_string();
                return url;
            }
            let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
            let (user, hostport) = match authority.rsplit_once('@') {
                Some((u, h)) => (Self::shown_user(protocol, u), h),
                None => (None, authority),
            };
            if authority.contains('@') {
                let shown = user.as_deref().unwrap_or("***");
                let tail = &rest[authority.len()..];
                url.raw = format!("{scheme}://{shown}@{hostport}{tail}");
            }
            let (host, port) = match hostport.rsplit_once(':') {
                Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty() => {
                    (h, p.parse().ok())
                }
                _ => (hostport, None),
            };
            url.user = user;
            url.host = (!host.is_empty()).then(|| host.to_ascii_lowercase());
            url.port = port;
            url.set_path(path);
            return url;
        }
        // scp-like: `[user@]host:path`, where the host has no `/` and the
        // colon comes before any slash — a Windows drive letter is not this.
        if let Some((before, path)) = raw.split_once(':') {
            let is_drive = before.len() == 1 && before.chars().all(|c| c.is_ascii_alphabetic());
            if !before.is_empty() && !before.contains('/') && !is_drive {
                let (user, host) = match before.rsplit_once('@') {
                    Some((u, h)) => (Some(u.to_string()), h),
                    None => (None, before),
                };
                url.protocol = RemoteProtocol::Scp;
                url.user = user;
                url.host = Some(host.to_ascii_lowercase());
                url.set_path(path);
                return url;
            }
        }
        url
    }

    /// The part of a URL's userinfo that is a name and not a secret: an
    /// `ssh` user without its password. Over `https` the user may itself be
    /// a token (`https://<token>@host/…`), so none of it is kept.
    fn shown_user(protocol: RemoteProtocol, userinfo: &str) -> Option<String> {
        if protocol != RemoteProtocol::Ssh {
            return None;
        }
        let name = userinfo.split(':').next().unwrap_or_default();
        (!name.is_empty()).then(|| name.to_string())
    }

    fn set_path(&mut self, path: &str) {
        let trimmed = path.trim_matches('/');
        let trimmed = trimmed.strip_suffix(".git").unwrap_or(trimmed);
        self.path = trimmed.to_string();
        let segments: Vec<&str> = trimmed.split('/').filter(|s| !s.is_empty()).collect();
        if let [namespace @ .., name] = segments.as_slice() {
            if !namespace.is_empty() {
                self.owner = Some(namespace.join("/"));
                self.name = Some(name.to_string());
            }
        }
    }

    /// The repository this URL names on `host`, when it names one.
    pub fn repo_ref(&self, host: &str) -> Option<RepoRef> {
        Some(RepoRef {
            host: host.to_string(),
            owner: self.owner.clone()?,
            name: self.name.clone()?,
        })
    }

    /// Whether the host is `host`, case-insensitively.
    pub fn is_on(&self, host: &str) -> bool {
        self.host
            .as_deref()
            .is_some_and(|h| h.eq_ignore_ascii_case(host))
    }

    /// The same URL with its host replaced — what an SSH alias resolves to.
    pub fn with_host(&self, host: &str) -> Self {
        Self {
            host: Some(host.to_ascii_lowercase()),
            ..self.clone()
        }
    }

    /// `host · owner/name` for a person to read, else the path.
    pub fn summary(&self) -> String {
        match (&self.host, &self.owner, &self.name) {
            (Some(h), Some(o), Some(n)) => format!("{h} · {o}/{n}"),
            (Some(h), _, _) => format!("{h} · {}", self.path),
            _ => self.path.clone(),
        }
    }
}

/// What a code host can do. Every field is a control the desktop shows only when
/// it is true — absent, never greyed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CodeHostCapabilities {
    pub draft_prs: bool,
    pub reviewers: bool,
    pub labels: bool,
    pub merge_strategies: Vec<MergeStrategy>,
    pub check_runs: bool,
    pub review_comments: bool,
    /// The code host can read submitted reviews and resolve review threads — what
    /// the desktop's review viewer and *Resolve* buttons need.
    pub review_threads: bool,
    /// The code host can take a reply on a review thread — what the desktop's
    /// *Reply* box and an agent's `pr_thread_reply` need.
    pub review_thread_replies: bool,
    /// The code host can delete a branch on the remote — what the merge dialog's
    /// *delete the branch* toggle needs to exist.
    pub delete_branch: bool,
    /// The verdicts a review may carry here: GitLab has approvals and comments
    /// but no *request changes* review — the desktop offers only these.
    #[serde(default)]
    pub review_events: Vec<ReviewEvent>,
}

/// A repository on a code host, from its remote URL. `owner` is the whole
/// namespace — `acme`, or GitLab's `acme/platform` — and `slug()` the path
/// the host's API addresses the repository by.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoRef {
    pub host: String,
    pub owner: String,
    pub name: String,
}

impl RepoRef {
    pub fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PrState {
    Open,
    Closed,
    Merged,
    Unknown,
}

impl PrState {
    /// The wire word: `open` · `closed` · `merged` · `unknown`.
    pub fn as_str(self) -> &'static str {
        match self {
            PrState::Open => "open",
            PrState::Closed => "closed",
            PrState::Merged => "merged",
            PrState::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PullRequest {
    pub number: u64,
    pub url: String,
    pub title: String,
    pub state: PrState,
    pub is_draft: bool,
    /// `None` while the code host is still computing it.
    pub mergeable: Option<bool>,
    pub head: String,
    pub head_sha: String,
    pub base: String,
    pub author: Option<String>,
}

/// Everything a pull request needs at creation. Reviewers and labels are
/// asked for only when [`CodeHostCapabilities`] says the code host has them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PrCreate {
    pub title: String,
    pub body: String,
    pub head: String,
    pub base: String,
    pub draft: bool,
    #[serde(default)]
    pub reviewers: Vec<String>,
    #[serde(default)]
    pub labels: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PrFilter {
    pub state: Option<PrState>,
    /// A head branch name.
    pub head: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CheckRun {
    pub name: String,
    /// `queued` · `in_progress` · `completed`
    pub status: String,
    /// `success` · `failure` · `neutral` · `cancelled` · `skipped` · `timed_out` · `action_required`, once completed.
    pub conclusion: Option<String>,
    pub url: Option<String>,
    pub summary: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReviewEvent {
    Approve,
    RequestChanges,
    Comment,
}

/// Which side of the diff an inline comment sits on: the base (a deleted or
/// unchanged-on-the-left line) or the head (an added or unchanged line).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Left,
    #[default]
    Right,
}

/// One inline comment of a review. `line` is a line of the diff — a comment
/// on a line the pull request did not touch is refused by the code host —
/// and `start_line`/`start_side` make it a span ending at `line`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReviewComment {
    pub path: String,
    pub line: u32,
    #[serde(default)]
    pub side: Side,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_side: Option<Side>,
    pub body: String,
}

/// A review to submit. `body` is the summary — required, in words, for a
/// comment or a change request; welcome but optional beside an approval.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Review {
    pub event: ReviewEvent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default)]
    pub comments: Vec<ReviewComment>,
}

impl Review {
    /// The summary, trimmed, when it says something.
    pub fn words(&self) -> Option<&str> {
        self.body
            .as_deref()
            .map(str::trim)
            .filter(|b| !b.is_empty())
    }

    /// A comment or a change request without a summary is one every code
    /// host refuses — said here, before any request.
    pub fn needs_words(&self) -> bool {
        self.event != ReviewEvent::Approve && self.words().is_none()
    }

    /// Whether this verdict is one a code host refuses from the pull
    /// request's own author: an approval or a change request, never a comment.
    pub fn is_verdict(&self) -> bool {
        self.event != ReviewEvent::Comment
    }
}

/// A review submitted on a pull request, read back from the code host.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ReviewSummary {
    pub author: Option<String>,
    /// The code host's own word: `approved` · `changes_requested` · `commented` · `dismissed` · `pending`.
    pub state: String,
    pub body: String,
    pub submitted_at: Option<String>,
}

/// One comment inside a review thread, read from the code host.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ReviewThreadComment {
    pub author: Option<String>,
    pub body: String,
    pub created_at: Option<String>,
}

/// A resolvable inline conversation on a pull request. `id` is the code host's
/// opaque thread id, the handle [`CodeHost::resolve_review_thread`] acts on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ReviewThread {
    pub id: String,
    pub path: Option<String>,
    pub line: Option<u32>,
    pub is_resolved: bool,
    pub is_outdated: bool,
    pub comments: Vec<ReviewThreadComment>,
}

/// A pull request's submitted reviews and its resolvable threads, read together
/// (one round-trip). Empty when the code host has no review-reading capability.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PrReviews {
    pub reviews: Vec<ReviewSummary>,
    pub threads: Vec<ReviewThread>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MergeStrategy {
    Merge,
    Squash,
    Rebase,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MergeOutcome {
    pub merged: bool,
    pub sha: Option<String>,
    pub message: String,
    /// Whether the head branch was deleted on the remote afterwards: `None`
    /// when nobody asked, `Some(false)` when the code host could not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_branch_deleted: Option<bool>,
}

/// Whose credential this is, as the code host sees it — what Settings shows
/// as *Connected as @login*. `scopes` is what the token may do, in the host's
/// own words (`repo`, `workflow` …); empty when the host does not say — a
/// GitHub fine-grained token carries no scope header. `organizations` are the
/// ones the token can see itself belonging to — empty when it lacks the scope
/// to say, which is not the same as belonging to none.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Account {
    pub login: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub organizations: Vec<String>,
    /// The person's display name, when the host has one.
    #[serde(default)]
    pub name: Option<String>,
    /// The person's public email, when the host shows one.
    #[serde(default)]
    pub email: Option<String>,
    /// The host's numeric id for the account — what its no-reply address is
    /// made of on GitHub and GitLab.
    #[serde(default)]
    pub id: Option<u64>,
}

/// Whether an account can see a repository, and push to it — read, never
/// tried: `GET /repos/{o}/{r}` and its `permissions`.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
pub struct RepoAccess {
    pub found: bool,
    pub push: bool,
}

/// What a credential is worth, asked of the code host itself — the one line
/// Settings shows per account. `missing` names the required scopes a classic
/// token lacks and `recommended_missing` the ones it would be better with;
/// both empty when the host does not list scopes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Connection {
    /// Nothing to check: no token in the environment or the store.
    NoToken,
    Connected {
        login: String,
        scopes: Vec<String>,
        missing: Vec<String>,
        recommended_missing: Vec<String>,
        organizations: Vec<String>,
    },
    /// The host rejected the token — replace it.
    Refused { reason: String },
    /// The host did not answer — the network, or a rate limit.
    Unreachable { reason: String },
}

impl Connection {
    /// Read an [`CodeHost::account`] outcome as a connection, checking the
    /// scopes against `required` and `recommended` when the host listed any.
    pub fn of(outcome: CodeHostResult<Account>, required: &[&str], recommended: &[&str]) -> Self {
        match outcome {
            Ok(account) => {
                let lacking = |wanted: &[&str]| -> Vec<String> {
                    if account.scopes.is_empty() {
                        return Vec::new();
                    }
                    wanted
                        .iter()
                        .filter(|r| !account.scopes.iter().any(|s| s == *r))
                        .map(|r| r.to_string())
                        .collect()
                };
                Connection::Connected {
                    missing: lacking(required),
                    recommended_missing: lacking(recommended),
                    login: account.login,
                    scopes: account.scopes,
                    organizations: account.organizations,
                }
            }
            Err(CodeHostError::NotAuthenticated(reason)) => Connection::Refused { reason },
            Err(e) => Connection::Unreachable {
                reason: e.to_string(),
            },
        }
    }

    /// The login, when connected.
    pub fn login(&self) -> Option<&str> {
        match self {
            Connection::Connected { login, .. } => Some(login),
            _ => None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CodeHostError {
    /// No credential, or one the code host rejected. Rendered as an instruction.
    #[error("not authenticated with the code host: {0}")]
    NotAuthenticated(String),
    #[error("not found on the code host: {0}")]
    NotFound(String),
    /// The code host said no for a reason the person has to act on: not
    /// mergeable, a review of your own PR, a branch that moved.
    #[error("the code host refused: {0}")]
    Refused(String),
    /// This code host cannot do that; the desktop never asks for it, the CLI can.
    #[error("unsupported by this code host: {0}")]
    Unsupported(String),
    #[error("code host unreachable: {0}")]
    Transport(String),
}

pub type CodeHostResult<T> = Result<T, CodeHostError>;

#[async_trait::async_trait]
pub trait CodeHost: Send + Sync {
    fn id(&self) -> CodeHostId;
    fn capabilities(&self) -> CodeHostCapabilities;
    /// Whether this code host hosts the repository `remote` names.
    fn detect(&self, remote: &RemoteUrl) -> Option<RepoRef>;
    /// The same code host bound to one account's credential — the login whose
    /// token every request carries from here on. `None` is the unbound host
    /// (the chain's own choice); a host already bound to `login` answers
    /// itself. The engine binds before any request, from the checkout's
    /// `codehost.account`.
    fn for_account(self: Arc<Self>, login: Option<&str>) -> Arc<dyn CodeHost>;
    /// Whose credential the bound one is — the connection check. A host
    /// with no credential answers `NotAuthenticated`; one that rejects it, the
    /// same; one that cannot be reached, `Transport`.
    async fn account(&self) -> CodeHostResult<Account>;
    /// Whose credential `token` would be, **before** it is stored: Settings asks
    /// this so a token the host refuses is never kept. `login` is the account
    /// the person says the token belongs to — Bitbucket's API tokens are Basic
    /// credentials and cannot be checked without it; GitHub and GitLab answer
    /// the login themselves and ignore it.
    async fn verify_token(&self, token: &str, login: Option<&str>) -> CodeHostResult<Account>;
    /// Whether the bound account can see `repo` and push to it — read, never
    /// tried. A repository the account cannot see is `found: false`, not an
    /// error.
    async fn repo_access(&self, repo: &RepoRef) -> CodeHostResult<RepoAccess>;
    async fn create_pr(&self, repo: &RepoRef, req: PrCreate) -> CodeHostResult<PullRequest>;
    async fn get_pr(&self, repo: &RepoRef, number: u64) -> CodeHostResult<PullRequest>;
    async fn list_prs(&self, repo: &RepoRef, filter: PrFilter) -> CodeHostResult<Vec<PullRequest>>;
    async fn checks(&self, repo: &RepoRef, number: u64) -> CodeHostResult<Vec<CheckRun>>;
    async fn submit_review(
        &self,
        repo: &RepoRef,
        number: u64,
        review: Review,
    ) -> CodeHostResult<()>;
    /// The pull request's submitted reviews and resolvable threads.
    /// A code host without the capability answers an empty [`PrReviews`].
    async fn pr_reviews(&self, repo: &RepoRef, number: u64) -> CodeHostResult<PrReviews>;
    /// Resolve (or unresolve) a review thread by its code host id. A code host without
    /// the capability answers `Unsupported`.
    async fn resolve_review_thread(&self, thread_id: &str, resolved: bool) -> CodeHostResult<()>;
    /// Reply on a review thread by its code host id, as the bound account. A
    /// code host without the capability answers `Unsupported`. Resolving is
    /// [`Self::resolve_review_thread`]'s: one verb per method, the engine
    /// composes the two.
    async fn reply_review_thread(&self, thread_id: &str, body: &str) -> CodeHostResult<()>;
    async fn merge(
        &self,
        repo: &RepoRef,
        number: u64,
        strategy: MergeStrategy,
    ) -> CodeHostResult<MergeOutcome>;
    /// Delete `branch` on the remote — after a merge, when the person asked.
    /// A code host without the capability answers `Unsupported`.
    async fn delete_branch(&self, repo: &RepoRef, branch: &str) -> CodeHostResult<()>;
}

/// Makes a code host of a kind for an instance that is not its public host —
/// a GitHub Enterprise or a self-hosted GitLab, named by the checkout's
/// `codehost.kind`. [`hosts::Hosts`] is the one implementation; a test may
/// hand in its own.
pub trait CodeHostFactory: Send + Sync {
    fn at_host(&self, kind: CodeHostKind, host: &str) -> Option<Arc<dyn CodeHost>>;
}

/// The code hosts this build knows, asked in order which one hosts a remote,
/// and — through its factory — able to make one for a self-hosted instance.
#[derive(Clone, Default)]
pub struct CodeHostRegistry {
    code_hosts: Vec<Arc<dyn CodeHost>>,
    factory: Option<Arc<dyn CodeHostFactory>>,
}

impl std::fmt::Debug for CodeHostRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.ids()).finish()
    }
}

impl CodeHostRegistry {
    pub fn new(code_hosts: Vec<Arc<dyn CodeHost>>) -> Self {
        Self {
            code_hosts,
            factory: None,
        }
    }

    /// The same registry, able to make a host for an instance nobody
    /// registered — what `codehost.kind` on a checkout asks for.
    pub fn with_factory(mut self, factory: Arc<dyn CodeHostFactory>) -> Self {
        self.factory = Some(factory);
        self
    }

    pub fn detect(&self, remote: &RemoteUrl) -> Option<(Arc<dyn CodeHost>, RepoRef)> {
        self.code_hosts
            .iter()
            .find_map(|f| f.detect(remote).map(|r| (Arc::clone(f), r)))
    }

    /// `detect`, then — when nothing registered claims the remote and the
    /// checkout says which kind its host is — a host made for that instance.
    pub fn detect_as(
        &self,
        remote: &RemoteUrl,
        kind: Option<CodeHostKind>,
    ) -> Option<(Arc<dyn CodeHost>, RepoRef)> {
        if let Some(found) = self.detect(remote) {
            return Some(found);
        }
        let kind = kind?;
        let host = remote.host.as_deref()?;
        let made = self.factory.as_ref()?.at_host(kind, host)?;
        let repo = remote.repo_ref(host)?;
        Some((made, repo))
    }

    /// The code host with this id, when the build has one — how a settings
    /// surface reaches a kind without a remote to detect it from.
    pub fn by_id(&self, id: CodeHostId) -> Option<Arc<dyn CodeHost>> {
        self.code_hosts.iter().find(|f| f.id() == id).cloned()
    }

    /// The registered host of a kind — its public instance.
    pub fn by_kind(&self, kind: CodeHostKind) -> Option<Arc<dyn CodeHost>> {
        self.by_id(kind.id())
    }

    pub fn ids(&self) -> Vec<CodeHostId> {
        self.code_hosts.iter().map(|f| f.id()).collect()
    }

    /// Every registered host — for a registry with one, the one.
    pub fn all(&self) -> &[Arc<dyn CodeHost>] {
        &self.code_hosts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_remote_never_carries_what_its_userinfo_hides() {
        // Placeholders, not credentials: what matters is that none survives.
        for (written, shown, user) in [
            (
                "https://x-access-token:not-a-real-value@git.example.com/acme/web.git",
                "https://***@git.example.com/acme/web.git",
                None,
            ),
            (
                "https://not-a-real-value@github.com/acme/web",
                "https://***@github.com/acme/web",
                None,
            ),
            (
                "ssh://git:not-a-real-value@github.com:2222/acme/web.git",
                "ssh://git@github.com:2222/acme/web.git",
                Some("git"),
            ),
            (
                "https://github.com/acme/web.git",
                "https://github.com/acme/web.git",
                None,
            ),
        ] {
            let r = RemoteUrl::parse(written);
            assert_eq!(r.raw, shown, "{written}");
            assert_eq!(r.user.as_deref(), user, "{written}");
            assert_eq!(
                (r.owner.as_deref(), r.name.as_deref()),
                (Some("acme"), Some("web")),
                "the rest of it still reads: {written}"
            );
            let wire = serde_json::to_string(&r).unwrap();
            assert!(!wire.contains("not-a-real-value"), "{wire}");
            assert!(!format!("{r:?}").contains("not-a-real-value"));
        }
        // An scp-like remote has no place for a secret, and stays as written.
        let scp = RemoteUrl::parse("git@github.com:acme/web.git");
        assert_eq!(scp.raw, "git@github.com:acme/web.git");
        assert_eq!(scp.user.as_deref(), Some("git"));
    }

    #[test]
    fn a_connection_reads_the_account_and_names_the_scopes_it_lacks() {
        let required = &["repo", "workflow"];
        let recommended = &["read:org"];
        let classic = Connection::of(
            Ok(Account {
                login: "octocat".into(),
                scopes: vec!["repo".into(), "read:org".into()],
                organizations: vec!["acme".into()],
                ..Account::default()
            }),
            required,
            recommended,
        );
        assert_eq!(
            classic,
            Connection::Connected {
                login: "octocat".into(),
                scopes: vec!["repo".into(), "read:org".into()],
                missing: vec!["workflow".into()],
                recommended_missing: vec![],
                organizations: vec!["acme".into()],
            }
        );
        let fine_grained = Connection::of(
            Ok(Account {
                login: "octocat".into(),
                ..Default::default()
            }),
            required,
            recommended,
        );
        assert!(
            matches!(&fine_grained, Connection::Connected { missing, recommended_missing, .. } if missing.is_empty() && recommended_missing.is_empty()),
            "no scope list, no verdict on it"
        );
        assert_eq!(fine_grained.login(), Some("octocat"));
        let no_org_scope = Connection::of(
            Ok(Account {
                login: "o".into(),
                scopes: vec!["repo".into(), "workflow".into()],
                organizations: vec![],
                ..Account::default()
            }),
            required,
            recommended,
        );
        assert!(
            matches!(no_org_scope, Connection::Connected { recommended_missing, .. } if recommended_missing == ["read:org"])
        );
        assert!(matches!(
            Connection::of(Err(CodeHostError::NotAuthenticated("Bad credentials".into())), required, recommended),
            Connection::Refused { reason } if reason == "Bad credentials"
        ));
        assert!(matches!(
            Connection::of(
                Err(CodeHostError::Transport("rate limit".into())),
                required,
                recommended
            ),
            Connection::Unreachable { .. }
        ));
        let wire = serde_json::to_value(Connection::NoToken).unwrap();
        assert_eq!(wire, serde_json::json!({"state": "no_token"}));
        assert_eq!(Connection::NoToken.login(), None);
    }

    #[test]
    fn a_remote_url_is_taken_apart_in_every_spelling_and_is_total() {
        for (url, protocol, user, host, port) in [
            (
                "https://github.com/bisa/bisa.git",
                RemoteProtocol::Https,
                None,
                "github.com",
                None,
            ),
            (
                "http://GitHub.com/bisa/bisa",
                RemoteProtocol::Https,
                None,
                "github.com",
                None,
            ),
            (
                "https://github.com/bisa/bisa/",
                RemoteProtocol::Https,
                None,
                "github.com",
                None,
            ),
            (
                "git@github.com:bisa/bisa.git",
                RemoteProtocol::Scp,
                Some("git"),
                "github.com",
                None,
            ),
            (
                "ssh://git@github.com/bisa/bisa.git",
                RemoteProtocol::Ssh,
                Some("git"),
                "github.com",
                None,
            ),
            (
                "ssh://git@github.com:2222/bisa/bisa",
                RemoteProtocol::Ssh,
                Some("git"),
                "github.com",
                Some(2222),
            ),
            (
                "git+ssh://github.com/bisa/bisa",
                RemoteProtocol::Ssh,
                None,
                "github.com",
                None,
            ),
        ] {
            let r = RemoteUrl::parse(url);
            assert_eq!(r.protocol, protocol, "{url}");
            assert_eq!(r.user.as_deref(), user, "{url}");
            assert_eq!(r.host.as_deref(), Some(host), "{url}");
            assert_eq!(r.port, port, "{url}");
            assert_eq!(
                (r.owner.as_deref(), r.name.as_deref()),
                (Some("bisa"), Some("bisa")),
                "{url}"
            );
            assert!(r.is_on("GitHub.com"));
            assert_eq!(r.repo_ref("github.com").unwrap().slug(), "bisa/bisa");
            assert_eq!(r.summary(), "github.com · bisa/bisa");
        }
        // An SSH alias is a host until `ssh -G` says what it stands for.
        let alias = RemoteUrl::parse("git@github-work:acme/web.git");
        assert_eq!(alias.host.as_deref(), Some("github-work"));
        assert_eq!(alias.protocol, RemoteProtocol::Scp);
        assert!(!alias.is_on("github.com"));
        let resolved = alias.with_host("GitHub.com");
        assert!(resolved.is_on("github.com"));
        assert_eq!(resolved.repo_ref("github.com").unwrap().slug(), "acme/web");
        assert_eq!(
            resolved.raw, alias.raw,
            "the raw URL is kept for the profile match"
        );

        // One segment: no owner and name, but still a URL.
        let one = RemoteUrl::parse("https://github.com/only-owner");
        assert!(one.repo_ref("github.com").is_none());
        assert_eq!((one.owner, one.name), (None, None));
        // Three or more: the namespace is everything but the last — a GitLab
        // group with a subgroup, a Bitbucket workspace.
        let three = RemoteUrl::parse("https://gitlab.example/group/sub/repo.git");
        assert_eq!(three.path, "group/sub/repo");
        assert_eq!(
            (three.owner.as_deref(), three.name.as_deref()),
            (Some("group/sub"), Some("repo"))
        );
        assert_eq!(
            three.repo_ref("gitlab.example").unwrap().slug(),
            "group/sub/repo"
        );
        assert_eq!(three.summary(), "gitlab.example · group/sub/repo");
        let nested = RemoteUrl::parse("git@gitlab.com:acme/platform/web/api.git");
        assert_eq!(nested.owner.as_deref(), Some("acme/platform/web"));
        assert_eq!(nested.name.as_deref(), Some("api"));

        // The kinds: their public hosts, ids and words.
        assert_eq!(
            CodeHostKind::of_host("GitLab.com"),
            Some(CodeHostKind::GitLab)
        );
        assert_eq!(CodeHostKind::of_host("gitlab.example"), None);
        assert_eq!(
            "bitbucket".parse::<CodeHostKind>(),
            Ok(CodeHostKind::Bitbucket)
        );
        assert!("gitea".parse::<CodeHostKind>().is_err());
        assert_eq!(CodeHostKind::GitHub.id(), CodeHostId("github"));
        assert_eq!(CodeHostKind::GitLab.env_var(), "BISA_GITLAB_TOKEN");
        assert_eq!(CodeHostKind::Bitbucket.cli(), None);
        assert_eq!(
            serde_json::to_value(CodeHostKind::GitLab).unwrap(),
            serde_json::json!("gitlab")
        );

        // Paths are local — total, never None.
        for local in [
            "/tmp/origin.git",
            "../origin",
            "C:\\repos\\origin",
            "file:///tmp/origin.git",
        ] {
            let r = RemoteUrl::parse(local);
            assert_eq!(r.protocol, RemoteProtocol::Local, "{local}");
            assert_eq!(r.host, None, "{local}");
            assert!(r.repo_ref("github.com").is_none());
        }
        assert_eq!(
            RemoteUrl::parse("/tmp/origin.git").summary(),
            "/tmp/origin.git"
        );
        assert_eq!(
            RemoteUrl::parse("git://github.com/o/r.git").protocol,
            RemoteProtocol::Other
        );
    }
}
