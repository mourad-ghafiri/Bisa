//! The credential chain (ide/08), one per code host kind: first match wins,
//! and a token is never logged, journaled, snapshotted, sent to the webview,
//! or returned by any route — only whether one is configured and where it
//! came from.
//!
//! 1. the kind's environment variable (`BISA_GITHUB_TOKEN`,
//!    `BISA_GITLAB_TOKEN`, `BISA_BITBUCKET_TOKEN`) — the operator's
//!    override, whatever account a checkout names;
//! 2. the token store — one **account** per login: a `0600` file
//!    `<login>.token` in the workspace's `identity/codehost/<kind>/` directory
//!    by default, or the OS keyring (service `bisa`, entry
//!    `codehost:<kind>:<login>`) when `BISA_KEYSTORE=keyring` — the same
//!    choice the workspace's own keys follow, so a person who never opted
//!    into the Keychain is never asked for their login password by this crate;
//!    beside the tokens a non-secret `accounts` list names the logins, so the
//!    keyring mode can enumerate them too. A checkout names its account
//!    through git config (`codehost.account`, a profile's or a pin's; the
//!    kind's global default otherwise); asked for no account in particular,
//!    the store answers the one login it holds, and refuses to guess between
//!    two;
//! 3. **the code host's own CLI** — `gh` or `glab`, signed in on this machine:
//!    asked for the token of the named account, or of its active one, through
//!    the [`CliToken`] port (`gh auth token`). Asked, never read: this crate
//!    opens no file of the CLI's; the token lives in memory for one request;
//! 4. **git's own credential helper** — the credential `git push` over HTTPS
//!    already uses, asked of git (`git credential fill` for the host) through
//!    the [`GitCredentials`] port the engine implements over its `git` handle.
//!    Asked, never read: git runs whatever helper its config names
//!    (osxkeychain, `gh auth git-credential`, a credential manager) and
//!    answers, or does not.
//!
//! Explicit before ambient: a token a person set or stored is the one they
//! meant; the CLI's and git's are the ones they already have. Whichever source
//! answered, the API is reached with that token and [`crate::CodeHost::account`]
//! is how Settings proves it works.
//!
//! This crate depends on no crate of ours, so the git side is a trait here and
//! an implementation in the engine — the Dependency Inversion that keeps the
//! layering honest.

use crate::{CodeHostError, CodeHostKind, CodeHostResult};
use std::path::PathBuf;
use std::sync::Arc;

pub const KEYSTORE_ENV: &str = "BISA_KEYSTORE";
pub const KEYRING_SERVICE: &str = "bisa";
/// The non-secret list of stored logins, one per line, beside the tokens.
pub const ACCOUNTS_FILE: &str = "accounts";

/// Where a person adds an account of this kind — named in every refusal.
pub fn settings_place(kind: CodeHostKind) -> String {
    format!("Settings → Git & code hosts → {}", kind.label())
}

/// A credential in memory. Its `Debug` and `Display` never show it, so a
/// token cannot reach a log or an error by accident; `expose` is the one
/// door, at the request that carries it.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The value, for the one request that needs it.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Trimmed and non-empty, else nothing.
    pub fn some(value: impl Into<String>) -> Option<Self> {
        let value: String = value.into();
        let value = value.trim();
        (!value.is_empty()).then(|| Self(value.to_string()))
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(…)")
    }
}

impl std::fmt::Display for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("…")
    }
}

/// What git's helper holds for a host: who, and the credential.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Credential {
    pub username: String,
    pub password: Secret,
}

/// Where a token came from — the one fact about it a route may report.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum TokenSource {
    Env,
    File,
    Keyring,
    /// The code host's CLI, signed in on this machine, answered.
    Cli,
    /// Git's own credential helper answered for the host.
    Git,
}

/// Where a stored token goes — for a settings panel to say so.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum StoreKind {
    /// A `0600` file under the workspace's `identity/` directory (the default).
    File,
    /// The OS keyring, chosen with `BISA_KEYSTORE=keyring`.
    Keyring,
}

/// The port to git's credential helpers. Implemented by the engine over its
/// own `git` handle (so a test's isolated git is what is asked); this crate
/// knows only the answers.
#[async_trait::async_trait]
pub trait GitCredentials: Send + Sync + std::fmt::Debug {
    /// What git's helpers hold for `https://<host>`, or `None` when no helper
    /// answered.
    async fn fill(&self, host: &str) -> Option<Credential>;
    /// The helpers git is configured with, one word each — non-secret config,
    /// for Settings to say *pushes over HTTPS use osxkeychain*.
    async fn helpers(&self) -> Vec<String>;
}

/// The port to a code host CLI's credential (`gh auth token`): the login it
/// belongs to and the token, for the named account or the CLI's active one.
/// Implemented by [`crate::cli`]'s CLIs; a kind without a CLI has none.
#[async_trait::async_trait]
pub trait CliToken: Send + Sync + std::fmt::Debug {
    async fn token(&self, login: Option<&str>) -> Option<(String, Secret)>;
    /// The login the CLI is signed in as for this host, without its token.
    async fn active_login(&self) -> Option<String>;
}

/// Where stored tokens go: one per login under the kind's token directory
/// (default) or the OS keyring (opt-in, `BISA_KEYSTORE=keyring`) — and,
/// when the engine attached them, the doors to the CLI and to git's helpers.
#[derive(Clone, Debug)]
pub struct TokenStore {
    kind: CodeHostKind,
    host: String,
    dir: PathBuf,
    keyring: bool,
    git: Option<Arc<dyn GitCredentials>>,
    cli: Option<Arc<dyn CliToken>>,
}

/// One stored account: the login and where its token lives.
#[derive(
    Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct StoredAccount {
    pub login: String,
    pub source: TokenSource,
}

/// A login as this crate keeps it, lowercased, since logins compare
/// case-insensitively on every code host. One grammar for the three: letters,
/// digits, `.`, `_` and `-`, starting with a letter or digit, at most 255.
pub fn normalize_login(login: &str) -> CodeHostResult<String> {
    let login = login.trim();
    let ok = !login.is_empty()
        && login.len() <= 255
        && login
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphanumeric())
        && login
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.');
    if !ok {
        return Err(CodeHostError::Refused(format!(
            "{login:?} is not a code host login"
        )));
    }
    Ok(login.to_ascii_lowercase())
}

impl TokenStore {
    /// From the environment's choice, like the workspace's own key store.
    /// `dir` is this kind's directory (`identity/codehost/<kind>/`); `host`
    /// the instance the tokens are for.
    pub fn new(kind: CodeHostKind, host: impl Into<String>, dir: impl Into<PathBuf>) -> Self {
        let keyring = std::env::var(KEYSTORE_ENV)
            .map(|v| v.trim().eq_ignore_ascii_case("keyring"))
            .unwrap_or(false);
        Self {
            kind,
            host: host.into(),
            dir: dir.into(),
            keyring,
            git: None,
            cli: None,
        }
    }

    pub fn file_only(kind: CodeHostKind, host: impl Into<String>, dir: impl Into<PathBuf>) -> Self {
        Self {
            kind,
            host: host.into(),
            dir: dir.into(),
            keyring: false,
            git: None,
            cli: None,
        }
    }

    /// Attach git's credential helpers as the last source.
    pub fn with_git(mut self, git: Arc<dyn GitCredentials>) -> Self {
        self.git = Some(git);
        self
    }

    /// Attach the code host's CLI as the source before git's.
    pub fn with_cli(mut self, cli: Arc<dyn CliToken>) -> Self {
        self.cli = Some(cli);
        self
    }

    pub fn kind(&self) -> CodeHostKind {
        self.kind
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn store_kind(&self) -> StoreKind {
        if self.keyring {
            StoreKind::Keyring
        } else {
            StoreKind::File
        }
    }

    fn token_path(&self, login: &str) -> PathBuf {
        self.dir.join(format!("{login}.token"))
    }

    fn accounts_path(&self) -> PathBuf {
        self.dir.join(ACCOUNTS_FILE)
    }

    fn entry(&self, login: &str) -> String {
        format!("codehost:{}:{login}", self.kind.as_str())
    }

    /// The stored logins, sorted — the non-secret list, never a token.
    pub fn logins(&self) -> Vec<String> {
        let mut logins: Vec<String> = std::fs::read_to_string(self.accounts_path())
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect();
        logins.sort();
        logins.dedup();
        logins
    }

    fn write_logins(&self, logins: &[String]) -> CodeHostResult<()> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|e| CodeHostError::Transport(format!("{}: {e}", self.dir.display())))?;
        let path = self.accounts_path();
        let text = logins.iter().map(|l| format!("{l}\n")).collect::<String>();
        std::fs::write(&path, text)
            .map_err(|e| CodeHostError::Transport(format!("{}: {e}", path.display())))
    }

    /// Every stored account with where its token lives.
    pub fn accounts(&self) -> Vec<StoredAccount> {
        self.logins()
            .into_iter()
            .map(|login| StoredAccount {
                login,
                source: if self.keyring {
                    TokenSource::Keyring
                } else {
                    TokenSource::File
                },
            })
            .collect()
    }

    /// One login's token, when stored.
    pub(crate) fn read(&self, login: &str) -> Option<(Secret, TokenSource)> {
        let login = normalize_login(login).ok()?;
        if self.keyring {
            let t = keyring::Entry::new(KEYRING_SERVICE, &self.entry(&login))
                .ok()?
                .get_password()
                .ok()
                .and_then(Secret::some)?;
            return Some((t, TokenSource::Keyring));
        }
        let t = std::fs::read_to_string(self.token_path(&login))
            .ok()
            .and_then(Secret::some)?;
        Some((t, TokenSource::File))
    }

    async fn token_from_cli(&self, login: Option<&str>) -> Option<(String, Secret)> {
        match &self.cli {
            Some(cli) => cli.token(login).await,
            None => None,
        }
    }

    /// The login the CLI is signed in as, when a CLI is attached and signed in.
    pub async fn cli_login(&self) -> Option<String> {
        match &self.cli {
            Some(cli) => cli.active_login().await.filter(|l| !l.trim().is_empty()),
            None => None,
        }
    }

    async fn credential_from_git(&self) -> Option<Credential> {
        match &self.git {
            Some(git) => git.fill(&self.host).await,
            None => None,
        }
    }

    async fn helpers(&self) -> Vec<String> {
        match &self.git {
            Some(git) => git.helpers().await,
            None => Vec::new(),
        }
    }

    /// Keep one account's token. Written once; nothing here reads it back to
    /// a caller. The login is the one the code host answered for the token,
    /// never one a person typed.
    pub fn store(&self, login: &str, token: &str) -> CodeHostResult<()> {
        let login = normalize_login(login)?;
        let token = token.trim();
        if token.is_empty() {
            return Err(CodeHostError::Refused("an empty token".into()));
        }
        if self.keyring {
            keyring::Entry::new(KEYRING_SERVICE, &self.entry(&login))
                .and_then(|e| e.set_password(token))
                .map_err(|e| CodeHostError::Transport(format!("keyring: {e}")))?;
        } else {
            std::fs::create_dir_all(&self.dir)
                .map_err(|e| CodeHostError::Transport(format!("{}: {e}", self.dir.display())))?;
            let path = self.token_path(&login);
            #[cfg(unix)]
            {
                use std::io::Write as _;
                use std::os::unix::fs::OpenOptionsExt as _;
                let mut f = std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .mode(0o600)
                    .open(&path)
                    .map_err(|e| CodeHostError::Transport(format!("{}: {e}", path.display())))?;
                f.write_all(token.as_bytes())
                    .map_err(|e| CodeHostError::Transport(format!("{}: {e}", path.display())))?;
            }
            #[cfg(not(unix))]
            {
                std::fs::write(&path, token)
                    .map_err(|e| CodeHostError::Transport(format!("{}: {e}", path.display())))?;
            }
        }
        let mut logins = self.logins();
        if !logins.contains(&login) {
            logins.push(login);
            logins.sort();
        }
        self.write_logins(&logins)
    }

    /// Forget one account. A login never stored is not an error.
    pub fn forget(&self, login: &str) -> CodeHostResult<()> {
        let login = normalize_login(login)?;
        if self.keyring {
            match keyring::Entry::new(KEYRING_SERVICE, &self.entry(&login))
                .and_then(|e| e.delete_credential())
            {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(e) => return Err(CodeHostError::Transport(format!("keyring: {e}"))),
            }
        } else {
            match std::fs::remove_file(self.token_path(&login)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(CodeHostError::Transport(format!(
                        "{}: {e}",
                        self.token_path(&login).display()
                    )))
                }
            }
        }
        let logins: Vec<String> = self.logins().into_iter().filter(|l| *l != login).collect();
        self.write_logins(&logins)
    }

    fn token_from_env(&self) -> Option<Secret> {
        std::env::var(self.kind.env_var())
            .ok()
            .and_then(Secret::some)
    }

    /// Whether the kind's environment variable is set — the operator's override.
    pub fn env_override(&self) -> bool {
        self.token_from_env().is_some()
    }
}

/// The token for `account` — or, with none named, for the one account the
/// store holds, else the CLI's, else git's — and where it came from.
pub async fn token(
    store: &TokenStore,
    account: Option<&str>,
) -> CodeHostResult<(Secret, TokenSource)> {
    let account = match account {
        Some(a) => Some(normalize_login(a)?),
        None => None,
    };
    let logins = store.logins();
    // The CLI is asked only when the store cannot answer on its own, and for
    // the login the chain is about — never a second time.
    let stored_answers = match account.as_deref() {
        Some(login) => store.read(login).is_some(),
        None => matches!(logins.as_slice(), [one] if store.read(one).is_some()),
    };
    let cli = if store.token_from_env().is_some() || stored_answers {
        None
    } else {
        store.token_from_cli(account.as_deref()).await
    };
    let git = if cli.is_none() && account.is_none() {
        store.credential_from_git().await
    } else {
        None
    };
    chain(
        store.kind,
        store.token_from_env(),
        account.as_deref(),
        &logins,
        |login| store.read(login),
        cli,
        git.map(|c| c.password),
    )
}

/// The chain itself, with every source handed in: the environment, then the
/// store — the named account, else its one account — then the CLI's token,
/// then what git's helper answered. Pure over its inputs, so the order is
/// testable on a machine with no token anywhere.
fn chain(
    kind: CodeHostKind,
    env: Option<Secret>,
    account: Option<&str>,
    logins: &[String],
    stored: impl Fn(&str) -> Option<(Secret, TokenSource)>,
    cli: Option<(String, Secret)>,
    git: Option<Secret>,
) -> CodeHostResult<(Secret, TokenSource)> {
    let place = settings_place(kind);
    let label = kind.label();
    if let Some(t) = env {
        return Ok((t, TokenSource::Env));
    }
    match account {
        Some(login) => {
            if let Some(found) = stored(login) {
                return Ok(found);
            }
            if let Some((who, t)) = cli {
                if who.eq_ignore_ascii_case(login) {
                    return Ok((t, TokenSource::Cli));
                }
            }
            let cli_words = match kind.cli() {
                Some(program) => format!(
                    ", sign in to it with the {} CLI (`{} auth login`)",
                    label,
                    program.binary()
                ),
                None => String::new(),
            };
            return Err(CodeHostError::NotAuthenticated(format!(
                "no token is stored for @{login}: add that account in {place}{cli_words}, or point this repository at another with `codehost.account`"
            )));
        }
        None => match logins {
            [one] => {
                if let Some(found) = stored(one) {
                    return Ok(found);
                }
            }
            [] => {}
            many => {
                let names = many
                    .iter()
                    .map(|l| format!("@{l}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(CodeHostError::NotAuthenticated(format!(
                    "several {label} accounts are stored ({names}) and this repository names none: pick a default account in {place}, or set `codehost.account` in a profile or in the repository"
                )));
            }
        },
    }
    if let Some((_, t)) = cli {
        return Ok((t, TokenSource::Cli));
    }
    if let Some(t) = git {
        return Ok((t, TokenSource::Git));
    }
    let doors = match kind.cli() {
        Some(program) => format!(
            "set {env}, sign in with the {label} CLI (`{bin} auth login`), add an account in {place}, or sign in to {label} over HTTPS in your terminal so git's credential helper holds one",
            env = kind.env_var(),
            bin = program.binary(),
        ),
        None => format!(
            "set {env}, add an account in {place}, or sign in to {label} over HTTPS in your terminal so git's credential helper holds one",
            env = kind.env_var(),
        ),
    };
    Err(CodeHostError::NotAuthenticated(format!(
        "no {label} token: {doors}"
    )))
}

/// Which source would answer for `account`, without handing the token to the caller.
pub async fn token_source(store: &TokenStore, account: Option<&str>) -> Option<TokenSource> {
    token(store, account).await.ok().map(|(_, s)| s)
}

/// Everything a settings panel may know about the accounts: whether the
/// environment overrides them all, where a stored one goes, the logins with
/// their source, git's helpers and the username the helper holds, the login
/// the CLI is signed in as. Never a token.
#[derive(
    Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct AccountsStatus {
    /// The kind's environment variable is set: every request uses it,
    /// whatever a checkout names.
    pub env_override: bool,
    pub store: StoreKind,
    pub accounts: Vec<StoredAccount>,
    /// Every credential helper git is configured with — what pushes and pulls
    /// over HTTPS use.
    pub helpers: Vec<String>,
    /// The username git's helper holds for `https://<host>`, when it holds one.
    pub helper_username: Option<String>,
    /// The login the code host's CLI is signed in as on this machine, when
    /// the kind has a CLI and it is.
    pub cli_login: Option<String>,
}

pub async fn accounts_status(store: &TokenStore) -> AccountsStatus {
    AccountsStatus {
        env_override: store.env_override(),
        store: store.store_kind(),
        accounts: store.accounts(),
        helpers: store.helpers().await,
        helper_username: store
            .credential_from_git()
            .await
            .map(|c| c.username)
            .filter(|u| !u.trim().is_empty()),
        cli_login: store.cli_login().await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct FakeGit(Option<&'static str>, Vec<&'static str>);

    #[async_trait::async_trait]
    impl GitCredentials for FakeGit {
        async fn fill(&self, host: &str) -> Option<Credential> {
            assert_eq!(host, "github.com", "asked for the store's host");
            self.0.map(|t| Credential {
                username: "ada".into(),
                password: Secret::new(t),
            })
        }
        async fn helpers(&self) -> Vec<String> {
            self.1.iter().map(|s| s.to_string()).collect()
        }
    }

    #[derive(Debug)]
    struct FakeCliToken(&'static str, &'static str);

    #[async_trait::async_trait]
    impl CliToken for FakeCliToken {
        async fn token(&self, login: Option<&str>) -> Option<(String, Secret)> {
            match login {
                Some(l) if !l.eq_ignore_ascii_case(self.0) => None,
                _ => Some((self.0.to_string(), Secret::new(self.1))),
            }
        }
        async fn active_login(&self) -> Option<String> {
            Some(self.0.to_string())
        }
    }

    fn secret(s: &str) -> Secret {
        Secret::new(s)
    }

    #[test]
    fn a_secret_never_prints() {
        let s = Secret::new("ghp_very_secret");
        assert_eq!(format!("{s:?}"), "Secret(…)");
        assert_eq!(s.to_string(), "…");
        assert_eq!(s.expose(), "ghp_very_secret");
        assert_eq!(Secret::some("  "), None);
        assert_eq!(Secret::some(" x ").unwrap().expose(), "x");
        let c = Credential {
            username: "ada".into(),
            password: s,
        };
        assert!(!format!("{c:?}").contains("ghp_"));
    }

    #[test]
    fn logins_take_one_grammar_for_the_three_hosts() {
        assert_eq!(normalize_login(" Ada-Acme ").unwrap(), "ada-acme");
        assert_eq!(
            normalize_login("ada.lovelace_1").unwrap(),
            "ada.lovelace_1",
            "GitLab's dots and underscores"
        );
        assert!(matches!(
            normalize_login("bad login"),
            Err(CodeHostError::Refused(_))
        ));
        assert!(matches!(
            normalize_login("-leading"),
            Err(CodeHostError::Refused(_))
        ));
        assert!(
            matches!(normalize_login("acme/web"), Err(CodeHostError::Refused(_))),
            "a namespace is not a login"
        );
        assert!(matches!(
            normalize_login(""),
            Err(CodeHostError::Refused(_))
        ));
    }

    #[test]
    fn the_file_store_keeps_one_token_per_login_with_owner_only_permissions_and_a_list() {
        let dir = tempfile::tempdir().unwrap();
        let store = TokenStore::file_only(
            CodeHostKind::GitHub,
            "github.com",
            dir.path().join("github"),
        );
        assert_eq!(store.store_kind(), StoreKind::File);
        assert_eq!(store.kind(), CodeHostKind::GitHub);
        assert!(store.read("ada").is_none());
        assert!(store.logins().is_empty());
        assert!(matches!(
            store.store("ada", "   "),
            Err(CodeHostError::Refused(_))
        ));
        assert!(matches!(
            store.store("bad login", "ghp_x"),
            Err(CodeHostError::Refused(_))
        ));
        store.store("Ada-Acme", "ghp_example\n").unwrap();
        store.store("octocat", "ghp_other").unwrap();
        assert_eq!(
            store.read("ada-acme"),
            Some((secret("ghp_example"), TokenSource::File))
        );
        assert_eq!(
            store.read("ADA-ACME"),
            Some((secret("ghp_example"), TokenSource::File)),
            "logins compare case-insensitively"
        );
        assert_eq!(
            store.logins(),
            ["ada-acme", "octocat"],
            "sorted, lowercased"
        );
        assert_eq!(
            store.accounts()[0],
            StoredAccount {
                login: "ada-acme".into(),
                source: TokenSource::File
            }
        );
        let list = std::fs::read_to_string(dir.path().join("github").join(ACCOUNTS_FILE)).unwrap();
        assert!(!list.contains("ghp_"), "the list is not a secret: {list}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(dir.path().join("github").join("ada-acme.token"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
        store.forget("ada-acme").unwrap();
        store.forget("ada-acme").unwrap();
        assert!(store.read("ada-acme").is_none());
        assert_eq!(store.logins(), ["octocat"]);
        assert_eq!(store.entry("octocat"), "codehost:github:octocat");
        let gl = TokenStore::file_only(
            CodeHostKind::GitLab,
            "gitlab.com",
            dir.path().join("gitlab"),
        );
        assert_eq!(
            gl.entry("octocat"),
            "codehost:gitlab:octocat",
            "one keyring namespace per kind"
        );
    }

    #[test]
    fn the_chain_is_the_environment_then_the_named_or_only_account_then_the_cli_then_gits_helper() {
        let kind = CodeHostKind::GitHub;
        let stored = |login: &str| match login {
            "ada" => Some((secret("ada-token"), TokenSource::File)),
            "bob" => Some((secret("bob-token"), TokenSource::File)),
            _ => None,
        };
        let one = vec!["ada".to_string()];
        let two = vec!["ada".to_string(), "bob".to_string()];
        let none: Vec<String> = vec![];
        let cli = || Some(("carol".to_string(), secret("from-cli")));
        let git = || Some(secret("from-git"));
        assert_eq!(
            chain(
                kind,
                Some(secret("env")),
                Some("bob"),
                &two,
                stored,
                cli(),
                git()
            )
            .unwrap(),
            (secret("env"), TokenSource::Env),
            "the environment is the operator's override, whatever the account"
        );
        assert_eq!(
            chain(kind, None, Some("bob"), &two, stored, cli(), git()).unwrap(),
            (secret("bob-token"), TokenSource::File),
            "the named account's token"
        );
        assert_eq!(
            chain(kind, None, Some("Carol"), &two, stored, cli(), git()).unwrap(),
            (secret("from-cli"), TokenSource::Cli),
            "a named account the CLI is signed in as"
        );
        let unknown = chain(kind, None, Some("dave"), &two, stored, cli(), git()).unwrap_err();
        assert!(matches!(unknown, CodeHostError::NotAuthenticated(_)));
        let words = unknown.to_string();
        assert!(
            words.contains("@dave")
                && words.contains(&settings_place(kind))
                && words.contains("gh auth login"),
            "a named account with no token is a refusal that names every door: {words}"
        );
        assert_eq!(
            chain(kind, None, None, &one, stored, cli(), git()).unwrap(),
            (secret("ada-token"), TokenSource::File),
            "unnamed, the one stored account is the one they meant"
        );
        let ambiguous = chain(kind, None, None, &two, stored, cli(), git()).unwrap_err();
        let words = ambiguous.to_string();
        assert!(
            words.contains("@ada") && words.contains("@bob") && words.contains("default account"),
            "two stored and none named is a refusal that names them: {words}"
        );
        assert_eq!(
            chain(kind, None, None, &none, stored, cli(), git()).unwrap(),
            (secret("from-cli"), TokenSource::Cli),
            "nothing stored: the CLI they are signed in with"
        );
        assert_eq!(
            chain(kind, None, None, &none, stored, None, git()).unwrap(),
            (secret("from-git"), TokenSource::Git),
            "no CLI: git's helper is the one they already have"
        );
        let missing = chain(kind, None, None, &none, stored, None, None).unwrap_err();
        assert!(matches!(missing, CodeHostError::NotAuthenticated(_)));
        let words = missing.to_string();
        assert!(
            words.contains(kind.env_var())
                && words.contains(&settings_place(kind))
                && words.contains("credential helper")
                && words.contains("gh auth login"),
            "{words}"
        );
        let bitbucket = chain(
            CodeHostKind::Bitbucket,
            None,
            None,
            &none,
            stored,
            None,
            None,
        )
        .unwrap_err()
        .to_string();
        assert!(
            bitbucket.contains("BISA_BITBUCKET_TOKEN") && !bitbucket.contains("auth login"),
            "a kind with no CLI offers no CLI: {bitbucket}"
        );
    }

    #[tokio::test]
    async fn the_status_reports_the_accounts_the_cli_and_gits_helper_and_never_a_token() {
        let dir = tempfile::tempdir().unwrap();
        let store = TokenStore::file_only(CodeHostKind::GitHub, "github.com", dir.path())
            .with_git(Arc::new(FakeGit(
                Some("ghp_from_git"),
                vec!["osxkeychain", "gh"],
            )))
            .with_cli(Arc::new(FakeCliToken("octocat", "gho_from_cli")));
        store.store("octocat", "ghp_stored").unwrap();
        let status = accounts_status(&store).await;
        assert_eq!(status.helpers, ["osxkeychain", "gh"]);
        assert_eq!(status.helper_username.as_deref(), Some("ada"));
        assert_eq!(status.cli_login.as_deref(), Some("octocat"));
        assert_eq!(status.accounts.len(), 1);
        assert_eq!(status.accounts[0].login, "octocat");
        assert_eq!(status.store, StoreKind::File);
        let text = serde_json::to_string(&status).unwrap();
        assert!(!text.contains("ghp_") && !text.contains("gho_"), "{text}");
        // The environment may hold a real token on a developer's machine, so the
        // chain's answer is asserted only when it does not.
        if !store.env_override() {
            assert_eq!(
                token(&store, None).await.unwrap(),
                (secret("ghp_stored"), TokenSource::File)
            );
            assert_eq!(
                token(&store, Some("OCTOCAT")).await.unwrap().1,
                TokenSource::File
            );
            let cli_only =
                TokenStore::file_only(CodeHostKind::GitHub, "github.com", dir.path().join("empty"))
                    .with_git(Arc::new(FakeGit(Some("ghp_from_git"), vec![])))
                    .with_cli(Arc::new(FakeCliToken("octocat", "gho_from_cli")));
            assert_eq!(
                token(&cli_only, None).await.unwrap(),
                (secret("gho_from_cli"), TokenSource::Cli),
                "the CLI before git"
            );
            assert_eq!(
                token(&cli_only, Some("octocat")).await.unwrap().1,
                TokenSource::Cli
            );
            assert!(
                token(&cli_only, Some("someone-else")).await.is_err(),
                "a named account the CLI does not hold is a refusal"
            );
            let git_only =
                TokenStore::file_only(CodeHostKind::GitHub, "github.com", dir.path().join("empty"))
                    .with_git(Arc::new(FakeGit(Some("ghp_from_git"), vec![])));
            assert_eq!(
                token(&git_only, None).await.unwrap(),
                (secret("ghp_from_git"), TokenSource::Git)
            );
            assert_eq!(token_source(&git_only, None).await, Some(TokenSource::Git));
        }
        assert!(matches!(
            token(&store, Some("not a login")).await,
            Err(CodeHostError::Refused(_))
        ));
    }

    #[test]
    fn the_keyring_is_opt_in_only() {
        let dir = tempfile::tempdir().unwrap();
        // Whatever the environment says elsewhere, `file_only` never asks the
        // keyring — this is the property the person relies on.
        assert_eq!(
            TokenStore::file_only(CodeHostKind::GitLab, "gitlab.com", dir.path()).store_kind(),
            StoreKind::File
        );
        assert_eq!(
            settings_place(CodeHostKind::Bitbucket),
            "Settings → Git & code hosts → Bitbucket"
        );
    }
}
