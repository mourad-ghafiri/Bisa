//! The code host CLIs — `gh` for GitHub, `glab` for GitLab — behind one port,
//! so a signed-in machine does the work with the credential it already holds
//! (ide/08). The CLI is asked **first**; what it cannot answer falls through
//! to the API ([`crate::layered::Layered`]).
//!
//! Two kinds of answer come back from a CLI call:
//!
//! - [`CliStep::Fallback`] — the CLI had no say: it is not installed, not
//!   signed in, does not hold the account the host is bound to, has no verb
//!   for the operation, or answered something this crate cannot read. The API
//!   is asked next, with a token from the credential chain.
//! - [`CliStep::Host`] — the code host itself refused, through the CLI. That
//!   is the answer; asking the API again would only repeat it.
//!
//! A process is spawned in this module alone ([`Cli`]), argv only, never a
//! shell, stdin closed unless a body is given, every prompt turned off, every
//! child time-boxed. [`fake::FakeCli`] scripts the same port for tests, so no
//! test ever runs a program on the machine. A `*auth token*` answer is a
//! [`crate::creds::Secret`] the moment it is read, and never a log line.

pub mod fake;
pub mod gh;
pub mod glab;

use crate::{
    Account, CheckRun, CodeHostError, CodeHostKind, MergeOutcome, MergeStrategy, PrCreate,
    PrFilter, PrReviews, PullRequest, RepoAccess, RepoRef, Review,
};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The CLIs this crate knows how to ask. A closed set: no other program
/// name ever reaches `Command::new`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CliProgram {
    Gh,
    Glab,
}

impl CliProgram {
    pub fn binary(self) -> &'static str {
        match self {
            CliProgram::Gh => "gh",
            CliProgram::Glab => "glab",
        }
    }

    /// The name a person reads.
    pub fn label(self) -> &'static str {
        match self {
            CliProgram::Gh => "GitHub CLI",
            CliProgram::Glab => "GitLab CLI",
        }
    }

    pub fn kind(self) -> CodeHostKind {
        match self {
            CliProgram::Gh => CodeHostKind::GitHub,
            CliProgram::Glab => CodeHostKind::GitLab,
        }
    }

    /// How to install it, per package manager, and where it is documented.
    pub fn install_hints(self) -> InstallHints {
        match self {
            CliProgram::Gh => InstallHints {
                brew: "brew install gh".into(),
                apt: "sudo apt install gh".into(),
                winget: "winget install --id GitHub.cli".into(),
                url: "https://cli.github.com".into(),
            },
            CliProgram::Glab => InstallHints {
                brew: "brew install glab".into(),
                apt: "sudo apt install glab".into(),
                winget: "winget install glab.glab".into(),
                url: "https://gitlab.com/gitlab-org/cli".into(),
            },
        }
    }

    /// The argv of the CLI's own browser sign-in for `host` — run by the
    /// desktop shell in a terminal, since the CLI wants a TTY for its one-time
    /// code. HTTPS is asked for so git's helper is set up in the same act.
    pub fn login_args(self, host: &str) -> Vec<String> {
        match self {
            CliProgram::Gh => [
                "auth",
                "login",
                "--hostname",
                host,
                "--web",
                "--git-protocol",
                "https",
            ],
            CliProgram::Glab => [
                "auth",
                "login",
                "--hostname",
                host,
                "--use-keyring",
                "--git-protocol",
                "https",
            ],
        }
        .into_iter()
        .map(str::to_string)
        .collect()
    }
}

/// Where to get a CLI that is not installed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct InstallHints {
    pub brew: String,
    pub apt: String,
    pub winget: String,
    pub url: String,
}

/// Wall-clock budgets: a local question (`--version`, `auth token`) and one
/// that reaches the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    pub local: Duration,
    pub network: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            local: Duration::from_secs(15),
            network: Duration::from_secs(60),
        }
    }
}

/// A finished child. A non-zero exit is an answer here — `gh auth status`
/// exits 1 for *not signed in* — so the caller classifies.
#[derive(Clone, PartialEq, Eq)]
pub struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    pub fn success(&self) -> bool {
        self.code == 0
    }
}

impl std::fmt::Debug for Output {
    /// stdout may be a token (`auth token`), so it is measured, never shown.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Output")
            .field("code", &self.code)
            .field("stdout_bytes", &self.stdout.len())
            .field("stderr", &self.stderr)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CliError {
    #[error("{} is not installed", .0.binary())]
    NotInstalled(CliProgram),
    #[error("`{what}` did not finish within {secs}s")]
    Timeout { what: String, secs: u64 },
    #[error("`{what}` failed: {detail}")]
    Failed { what: String, detail: String },
}

/// The port: run one program with one argv, an environment on top of the
/// hardened one, and an optional body on stdin, under one budget.
pub trait CliRunner: Send + Sync + std::fmt::Debug {
    /// Where the program is on `PATH`, when it is — a lookup, no spawn.
    fn installed(&self, program: CliProgram) -> Option<PathBuf>;
    fn run(
        &self,
        program: CliProgram,
        args: &[String],
        env: &[(String, String)],
        stdin: Option<&[u8]>,
        timeout: Duration,
    ) -> Result<Output, CliError>;
}

/// The real programs, on `PATH`. argv only, never a shell; every prompt off
/// (`GH_PROMPT_DISABLED`, `GIT_TERMINAL_PROMPT=0`), no pager, no colour, no
/// update nag, `LC_ALL=C` so the words read the same on every machine; the
/// child terminated when its budget passes.
#[derive(Debug, Clone, Default)]
pub struct Cli {
    pub timeouts: Timeouts,
    /// The engine's clients, for the environment the programs are handed:
    /// the same proxy the platform's own HTTP uses, so `gh` and `glab` leave
    /// this machine the way the API path does. `None` inherits the process's.
    pub http: Option<Arc<bisa_http::Clients>>,
}

impl Cli {
    pub fn with_http(mut self, http: Arc<bisa_http::Clients>) -> Self {
        self.http = Some(http);
        self
    }
}

/// The environment every child gets, whatever the caller adds.
const HARDENED_ENV: [(&str, &str); 9] = [
    ("GH_PROMPT_DISABLED", "1"),
    ("GH_NO_UPDATE_NOTIFIER", "1"),
    ("GH_PAGER", "cat"),
    ("GLAB_CHECK_UPDATE", "false"),
    ("NO_COLOR", "1"),
    ("PAGER", "cat"),
    ("GIT_TERMINAL_PROMPT", "0"),
    ("TERM", "dumb"),
    ("LC_ALL", "C"),
];

impl CliRunner for Cli {
    fn installed(&self, program: CliProgram) -> Option<PathBuf> {
        which::which(program.binary()).ok()
    }

    fn run(
        &self,
        program: CliProgram,
        args: &[String],
        env: &[(String, String)],
        stdin: Option<&[u8]>,
        timeout: Duration,
    ) -> Result<Output, CliError> {
        let what = format!("{} {}", program.binary(), args.join(" "));
        let mut cmd = Command::new(program.binary());
        cmd.args(args)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in HARDENED_ENV {
            cmd.env(k, v);
        }
        if let Some(http) = &self.http {
            let child = http.child_env();
            for (k, v) in &child.set {
                cmd.env(k, v);
            }
            for k in &child.remove {
                cmd.env_remove(k);
            }
        }
        for (k, v) in env {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                CliError::NotInstalled(program)
            } else {
                CliError::Failed {
                    what: what.clone(),
                    detail: format!("spawn: {e}"),
                }
            }
        })?;
        // The body is a pull request's words — small — written before the
        // pipes are drained. Its result is judged after the CLI's own exit:
        // a child that stopped reading is the CLI's failure to report, in its
        // words; a short write only matters when the CLI succeeded.
        let fed = match (stdin, child.stdin.take()) {
            (Some(body), Some(mut pipe)) => {
                let fed = pipe.write_all(body);
                drop(pipe);
                Some(fed)
            }
            _ => None,
        };
        let out_reader = drain(child.stdout.take().expect("stdout was piped"));
        let err_reader = drain(child.stderr.take().expect("stderr was piped"));
        let deadline = Instant::now() + timeout;
        let mut nap = Duration::from_millis(1);
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        if let Err(e) = child.kill() {
                            tracing::debug!("terminating a timed-out {program:?}: {e}");
                        }
                        if let Err(e) = child.wait() {
                            tracing::debug!("reaping a timed-out {program:?}: {e}");
                        }
                        return Err(CliError::Timeout {
                            what,
                            secs: timeout.as_secs(),
                        });
                    }
                    std::thread::sleep(nap.min(remaining));
                    nap = (nap * 2).min(Duration::from_millis(25));
                }
                Err(e) => {
                    return Err(CliError::Failed {
                        what,
                        detail: format!("waiting: {e}"),
                    })
                }
            }
        };
        // A pipe that could not be read to its end is a failure whatever the
        // exit status: a truncated answer would be read as the whole one.
        let stdout = joined(out_reader).map_err(|detail| CliError::Failed {
            what: what.clone(),
            detail: format!("reading stdout: {detail}"),
        })?;
        let stderr = joined(err_reader).map_err(|detail| CliError::Failed {
            what: what.clone(),
            detail: format!("reading stderr: {detail}"),
        })?;
        if let Some(Err(e)) = fed {
            if status.success() {
                return Err(CliError::Failed {
                    what,
                    detail: format!("feeding stdin: {e}"),
                });
            }
        }
        Ok(Output {
            code: status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
        })
    }
}

/// Read a child's pipe to its end on a thread of its own, keeping the error.
fn drain<R: std::io::Read + Send + 'static>(
    mut pipe: R,
) -> std::thread::JoinHandle<std::io::Result<Vec<u8>>> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        pipe.read_to_end(&mut buf)?;
        Ok(buf)
    })
}

/// What a reader thread read, or why it could not — a panic in it counted
/// as one more way not to have read.
fn joined(reader: std::thread::JoinHandle<std::io::Result<Vec<u8>>>) -> Result<Vec<u8>, String> {
    reader
        .join()
        .map_err(|_| "the reader thread panicked".to_string())?
        .map_err(|e| e.to_string())
}

/// Run on a blocking thread, so an async caller never holds the runtime
/// while a child runs.
pub(crate) async fn run(
    runner: &Arc<dyn CliRunner>,
    program: CliProgram,
    args: Vec<String>,
    env: Vec<(String, String)>,
    stdin: Option<Vec<u8>>,
    timeout: Duration,
) -> Result<Output, CliError> {
    let runner = Arc::clone(runner);
    tokio::task::spawn_blocking(move || runner.run(program, &args, &env, stdin.as_deref(), timeout))
        .await
        .unwrap_or_else(|e| {
            Err(CliError::Failed {
                what: format!("{} {}", program.binary(), "…"),
                detail: format!("the blocking task ended: {e}"),
            })
        })
}

/// One account a CLI is signed in as, from its `auth status`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CliAccount {
    pub host: String,
    pub login: String,
    /// The account the CLI uses for this host unless told otherwise.
    pub active: bool,
    /// The protocol the CLI configured git with (`https` · `ssh`), when it said.
    pub protocol: Option<String>,
}

/// What the machine has: whether the CLI is installed and where, which
/// version, and the accounts it is signed in as for the host asked about.
/// Never a token. `detail` carries the CLI's own words when it is installed
/// and signed in to nothing, or did not answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CliProbe {
    pub program: CliProgram,
    pub installed: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    pub accounts: Vec<CliAccount>,
    pub detail: Option<String>,
}

impl CliProbe {
    pub fn not_installed(program: CliProgram) -> Self {
        Self {
            program,
            installed: false,
            path: None,
            version: None,
            accounts: Vec::new(),
            detail: None,
        }
    }

    /// The account the CLI would use for `host`.
    pub fn active(&self) -> Option<&CliAccount> {
        self.accounts
            .iter()
            .find(|a| a.active)
            .or_else(|| self.accounts.first())
    }

    /// Whether the CLI is signed in to the host at all.
    pub fn signed_in(&self) -> bool {
        !self.accounts.is_empty()
    }
}

/// The version out of `gh version 2.63.2 (2024-12-05)` or
/// `glab version 1.50.0 (2025-01-01)`: the first token that looks like one.
pub fn parse_version(text: &str) -> Option<String> {
    text.split_whitespace()
        .map(|w| w.trim_start_matches('v'))
        .find(|w| {
            let mut parts = w.split('.');
            matches!((parts.next(), parts.next()), (Some(a), Some(b)) if a.chars().all(|c| c.is_ascii_digit()) && b.chars().all(|c| c.is_ascii_digit()))
        })
        .map(|w| w.trim_end_matches(|c: char| !c.is_ascii_alphanumeric()).to_string())
}

/// `gh auth status` for one host, read into accounts:
///
/// ```text
/// github.com
///   ✓ Logged in to github.com account octocat (keyring)
///   - Active account: true
///   - Git operations protocol: https
///   - Token: gho_************************************
///   - Token scopes: 'gist', 'read:org', 'repo'
///   ✓ Logged in to github.com account ada-acme (keyring)
///   - Active account: false
/// ```
///
/// A line that failed (`X Failed to log in to github.com account …`) is not an
/// account. The token line is never read.
pub fn parse_gh_auth_status(text: &str) -> Vec<CliAccount> {
    let mut accounts: Vec<CliAccount> = Vec::new();
    for raw in text.lines() {
        let line = raw
            .trim()
            .trim_start_matches(['✓', '✗', 'X', '-', '!'])
            .trim();
        if let Some(rest) = line.strip_prefix("Logged in to ") {
            // "<host> account <login> (<store>)" or older "<host> as <login> (…)"
            let mut words = rest.split_whitespace();
            let host = words.next().unwrap_or_default().to_string();
            let mut login = None;
            let mut previous = "";
            for w in words {
                if previous == "account" || previous == "as" {
                    login = Some(
                        w.trim_matches(|c: char| {
                            !c.is_ascii_alphanumeric() && c != '-' && c != '_' && c != '.'
                        })
                        .to_string(),
                    );
                    break;
                }
                previous = w;
            }
            if let Some(login) = login.filter(|l| !l.is_empty()) {
                accounts.push(CliAccount {
                    host,
                    login,
                    active: false,
                    protocol: None,
                });
            }
        } else if let Some(last) = accounts.last_mut() {
            if let Some(v) = line.strip_prefix("Active account:") {
                last.active = v.trim().eq_ignore_ascii_case("true");
            } else if let Some(v) = line.strip_prefix("Git operations protocol:") {
                last.protocol = Some(v.trim().to_string());
            }
        }
    }
    // One account and no *Active account* line (an older CLI): it is the one.
    if accounts.len() == 1 && !accounts[0].active {
        accounts[0].active = true;
    }
    accounts
}

/// `glab auth status` for one host, read into accounts:
///
/// ```text
/// gitlab.com
///   ✓ Logged in to gitlab.com as octocat (/Users/me/.config/glab-cli/config.yml)
///   ✓ Git operations for gitlab.com configured to use https protocol.
///   ✓ API calls for gitlab.com are made over https protocol
///   ✓ REST API Endpoint: https://gitlab.com/api/v4/
///   ✓ Token: **************************
/// ```
///
/// `glab` holds one account per host, so it is the active one.
pub fn parse_glab_auth_status(text: &str) -> Vec<CliAccount> {
    let mut accounts: Vec<CliAccount> = Vec::new();
    for raw in text.lines() {
        let line = raw
            .trim()
            .trim_start_matches(['✓', '✗', 'X', '-', '!'])
            .trim();
        if let Some(rest) = line.strip_prefix("Logged in to ") {
            let mut words = rest.split_whitespace();
            let host = words.next().unwrap_or_default().to_string();
            let mut login = None;
            let mut previous = "";
            for w in words {
                if previous == "as" {
                    login = Some(
                        w.trim_matches(|c: char| {
                            !c.is_ascii_alphanumeric() && c != '-' && c != '_' && c != '.'
                        })
                        .to_string(),
                    );
                    break;
                }
                previous = w;
            }
            if let Some(login) = login.filter(|l| !l.is_empty()) {
                accounts.push(CliAccount {
                    host,
                    login,
                    active: true,
                    protocol: None,
                });
            }
        } else if let (Some(last), Some(idx)) =
            (accounts.last_mut(), line.find("configured to use "))
        {
            let protocol = line[idx + "configured to use ".len()..]
                .split_whitespace()
                .next()
                .unwrap_or_default();
            if !protocol.is_empty() {
                last.protocol = Some(protocol.to_string());
            }
        }
    }
    accounts
}

/// Why the CLI had no say, so the API is asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fallback {
    NotInstalled,
    NotSignedIn,
    /// The host is bound to a login the CLI is not signed in as.
    NoSuchAccount(String),
    /// The CLI has no verb for this operation (or a flag this version lacks).
    NoVerb(&'static str),
    /// The CLI answered, but not in a shape this crate reads.
    Unparsable(String),
    /// The CLI failed for a reason that is not the host's refusal.
    Failed(String),
}

impl std::fmt::Display for Fallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Fallback::NotInstalled => f.write_str("the CLI is not installed"),
            Fallback::NotSignedIn => f.write_str("the CLI is not signed in"),
            Fallback::NoSuchAccount(l) => write!(f, "the CLI is not signed in as @{l}"),
            Fallback::NoVerb(what) => write!(f, "the CLI has no verb for {what}"),
            Fallback::Unparsable(why) => write!(f, "the CLI's answer could not be read: {why}"),
            Fallback::Failed(why) => write!(f, "the CLI failed: {why}"),
        }
    }
}

/// What one CLI call came to: an answer, a reason to ask the API instead,
/// or the host's own refusal relayed by the CLI.
#[derive(Debug)]
pub enum CliStep {
    Fallback(Fallback),
    Host(CodeHostError),
}

pub type CliOutcome<T> = Result<T, CliStep>;

impl From<CliError> for CliStep {
    fn from(e: CliError) -> Self {
        match e {
            CliError::NotInstalled(_) => CliStep::Fallback(Fallback::NotInstalled),
            CliError::Timeout { what, secs } => CliStep::Fallback(Fallback::Failed(format!(
                "`{what}` did not finish within {secs}s"
            ))),
            CliError::Failed { what, detail } => {
                CliStep::Fallback(Fallback::Failed(format!("`{what}`: {detail}")))
            }
        }
    }
}

/// Read a failed CLI exit as what it is: the host's refusal, relayed, or the
/// CLI's own trouble. The words are the CLIs' (`gh` prefixes an API status
/// with `HTTP 404:`; both say *not logged in* / *authentication required*).
pub(crate) fn classify_failure(what: &str, output: &Output) -> CliStep {
    let text = format!("{}\n{}", output.stderr, output.stdout);
    let lower = text.to_ascii_lowercase();
    let detail = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("no output")
        .to_string();
    if lower.contains("not logged in")
        || lower.contains("not logged into")
        || lower.contains("authentication required")
        || lower.contains("auth login")
        || lower.contains("no token")
    {
        return CliStep::Fallback(Fallback::NotSignedIn);
    }
    if lower.contains("unknown command")
        || lower.contains("unknown flag")
        || lower.contains("unknown shorthand flag")
    {
        return CliStep::Fallback(Fallback::NoVerb("this version's verbs"));
    }
    if lower.contains("http 401") || lower.contains("bad credentials") || lower.contains("http 403")
    {
        return CliStep::Host(CodeHostError::NotAuthenticated(format!("{what}: {detail}")));
    }
    if lower.contains("http 404")
        || lower.contains("could not resolve to")
        || lower.contains("404 not found")
    {
        return CliStep::Host(CodeHostError::NotFound(format!("{what}: {detail}")));
    }
    if lower.contains("http 422")
        || lower.contains("http 405")
        || lower.contains("http 409")
        || lower.contains("validation failed")
        || lower.contains("not mergeable")
        || lower.contains("is not mergeable")
        || lower.contains("already exists")
        || lower.contains("can not approve")
        || lower.contains("cannot approve")
        || lower.contains("review body is required")
        || lower.contains("merge request is not mergeable")
        || lower.contains("405 method not allowed")
        || lower.contains("409 conflict")
    {
        return CliStep::Host(CodeHostError::Refused(format!("{what}: {detail}")));
    }
    CliStep::Fallback(Fallback::Failed(format!("{what}: {detail}")))
}

/// What a CLI can do for a code host — the same operations as
/// [`crate::CodeHost`], each answering [`CliOutcome`] so the layered host
/// knows when to ask the API instead.
#[async_trait::async_trait]
pub trait CodeHostCli: Send + Sync + std::fmt::Debug {
    fn program(&self) -> CliProgram;
    fn host(&self) -> &str;
    /// The same CLI speaking as `login` — `gh` through the token it holds for
    /// that account; a login it does not hold makes every call a
    /// [`Fallback::NoSuchAccount`].
    fn for_account(self: Arc<Self>, login: Option<&str>) -> Arc<dyn CodeHostCli>;
    /// Installed, version, accounts — never a token, never a network call the
    /// CLI does not make on its own.
    async fn probe(&self) -> CliProbe;
    async fn account(&self) -> CliOutcome<Account>;
    async fn repo_access(&self, repo: &RepoRef) -> CliOutcome<RepoAccess>;
    async fn create_pr(&self, repo: &RepoRef, req: PrCreate) -> CliOutcome<PullRequest>;
    async fn get_pr(&self, repo: &RepoRef, number: u64) -> CliOutcome<PullRequest>;
    async fn list_prs(&self, repo: &RepoRef, filter: PrFilter) -> CliOutcome<Vec<PullRequest>>;
    async fn checks(&self, repo: &RepoRef, number: u64) -> CliOutcome<Vec<CheckRun>>;
    async fn submit_review(&self, repo: &RepoRef, number: u64, review: Review) -> CliOutcome<()>;
    async fn pr_reviews(&self, repo: &RepoRef, number: u64) -> CliOutcome<PrReviews>;
    async fn resolve_review_thread(&self, thread_id: &str, resolved: bool) -> CliOutcome<()>;
    async fn reply_review_thread(&self, thread_id: &str, body: &str) -> CliOutcome<()>;
    async fn merge(
        &self,
        repo: &RepoRef,
        number: u64,
        strategy: MergeStrategy,
    ) -> CliOutcome<MergeOutcome>;
    async fn delete_branch(&self, repo: &RepoRef, branch: &str) -> CliOutcome<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_read_from_either_clis_banner() {
        assert_eq!(
            parse_version(
                "gh version 2.63.2 (2024-12-05)\nhttps://github.com/cli/cli/releases/tag/v2.63.2\n"
            )
            .as_deref(),
            Some("2.63.2")
        );
        assert_eq!(
            parse_version("glab version 1.50.0 (2025-01-01)").as_deref(),
            Some("1.50.0")
        );
        assert_eq!(
            parse_version("glab 1.51.0-dev").as_deref(),
            Some("1.51.0-dev")
        );
        assert_eq!(parse_version("nothing here"), None);
    }

    #[test]
    fn gh_auth_status_is_read_into_accounts_and_the_token_line_is_ignored() {
        let text = "github.com\n  ✓ Logged in to github.com account octocat (keyring)\n  - Active account: true\n  - Git operations protocol: https\n  - Token: gho_************************************\n  - Token scopes: 'gist', 'read:org', 'repo'\n  ✓ Logged in to github.com account ada-acme (keyring)\n  - Active account: false\n  - Git operations protocol: ssh\n  - Token: gho_************************************\n";
        let accounts = parse_gh_auth_status(text);
        assert_eq!(accounts.len(), 2);
        assert_eq!(
            accounts[0],
            CliAccount {
                host: "github.com".into(),
                login: "octocat".into(),
                active: true,
                protocol: Some("https".into())
            }
        );
        assert_eq!(
            accounts[1],
            CliAccount {
                host: "github.com".into(),
                login: "ada-acme".into(),
                active: false,
                protocol: Some("ssh".into())
            }
        );
        assert!(
            !format!("{accounts:?}").contains("gho_"),
            "the token line was never read"
        );
        let failed = "github.com\n  X Failed to log in to github.com account octocat (keyring)\n  - Active account: true\n  - The token in keyring is invalid.\n";
        assert!(
            parse_gh_auth_status(failed).is_empty(),
            "a failed login is not an account"
        );
        assert!(parse_gh_auth_status(
            "You are not logged into any GitHub hosts. To log in, run: gh auth login\n"
        )
        .is_empty());
        let older = "github.com\n  ✓ Logged in to github.com as octocat (oauth_token)\n  ✓ Git operations for github.com configured to use https protocol.\n";
        let one = parse_gh_auth_status(older);
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].login, "octocat");
        assert!(one[0].active, "one account with no Active line is the one");
    }

    #[test]
    fn glab_auth_status_is_read_into_one_active_account() {
        let text = "gitlab.com\n  ✓ Logged in to gitlab.com as ada.lovelace (/Users/me/.config/glab-cli/config.yml)\n  ✓ Git operations for gitlab.com configured to use https protocol.\n  ✓ API calls for gitlab.com are made over https protocol\n  ✓ REST API Endpoint: https://gitlab.com/api/v4/\n  ✓ Token: **************************\n";
        let accounts = parse_glab_auth_status(text);
        assert_eq!(
            accounts,
            vec![CliAccount {
                host: "gitlab.com".into(),
                login: "ada.lovelace".into(),
                active: true,
                protocol: Some("https".into())
            }]
        );
        assert!(parse_glab_auth_status(
            "No GitLab hosts are configured. To log in, run: glab auth login\n"
        )
        .is_empty());
    }

    #[test]
    fn a_failed_exit_is_the_hosts_refusal_or_a_reason_to_ask_the_api() {
        let out = |stderr: &str| Output {
            code: 1,
            stdout: String::new(),
            stderr: stderr.into(),
        };
        assert!(matches!(
            classify_failure(
                "x",
                &out("To get started with GitHub CLI, please run:  gh auth login")
            ),
            CliStep::Fallback(Fallback::NotSignedIn)
        ));
        assert!(matches!(
            classify_failure("x", &out("unknown command \"frob\" for \"gh pr\"")),
            CliStep::Fallback(Fallback::NoVerb(_))
        ));
        assert!(matches!(classify_failure("read", &out("GraphQL: Could not resolve to a PullRequest with the number of 99. (repository.pullRequest)")), CliStep::Host(CodeHostError::NotFound(_))));
        assert!(matches!(classify_failure("merge", &out("X Pull request #12 is not mergeable: the merge commit cannot be cleanly created.")), CliStep::Host(CodeHostError::Refused(_))));
        assert!(matches!(
            classify_failure(
                "x",
                &out("HTTP 401: Bad credentials (https://api.github.com/user)")
            ),
            CliStep::Host(CodeHostError::NotAuthenticated(_))
        ));
        assert!(matches!(
            classify_failure("x", &out("dial tcp: lookup api.github.com: no such host")),
            CliStep::Fallback(Fallback::Failed(_))
        ));
        let step: CliStep = CliError::NotInstalled(CliProgram::Gh).into();
        assert!(matches!(step, CliStep::Fallback(Fallback::NotInstalled)));
        assert_eq!(
            Fallback::NoSuchAccount("ada".into()).to_string(),
            "the CLI is not signed in as @ada"
        );
    }

    #[test]
    fn the_programs_are_a_closed_set_with_their_words_and_sign_in() {
        assert_eq!(CliProgram::Gh.binary(), "gh");
        assert_eq!(CliProgram::Glab.kind(), CodeHostKind::GitLab);
        assert_eq!(
            CliProgram::Gh.login_args("github.example"),
            [
                "auth",
                "login",
                "--hostname",
                "github.example",
                "--web",
                "--git-protocol",
                "https"
            ]
        );
        assert!(CliProgram::Glab
            .login_args("gitlab.com")
            .starts_with(&["auth".to_string(), "login".to_string()]));
        assert_eq!(CliProgram::Gh.install_hints().brew, "brew install gh");
        assert_eq!(CodeHostKind::GitHub.cli(), Some(CliProgram::Gh));
        let probe = CliProbe::not_installed(CliProgram::Glab);
        assert!(!probe.installed && !probe.signed_in() && probe.active().is_none());
        let out = Output {
            code: 0,
            stdout: "gho_secret".into(),
            stderr: String::new(),
        };
        assert!(
            !format!("{out:?}").contains("gho_"),
            "an output's stdout is measured, never shown: {out:?}"
        );
    }
}
