//! What a checkout will use when it talks to its remote — the facts the Git ›
//! Repository view's **Connection** card shows (ide/04 §The Repository view):
//! the remote and its protocol, the profile it falls under, who commits and
//! where that comes from, the transport (the SSH key ssh would offer and
//! whether ssh-agent holds it; git's helpers over HTTPS), the code host
//! account the platform will ask as — and the **cautions**, each a stable id
//! with a sentence, computed by one pure function so the CLI and the desktop
//! say the same thing.
//!
//! Reading the facts is local: git config reads in the checkout, one offline
//! `ssh -G`, the public keys and ssh-agent's list. Nothing here reaches a
//! remote; that is [`check`]'s job, on an explicit button — three read-only
//! probes that change nothing on either side.

use crate::codehost::{self, CodeHostKind, Connection, RemoteProtocol, RemoteUrl, RepoAccess};
use crate::gitprofiles::{self, GitProfileView};
use crate::projects::{blocking, checkout_tree};
use crate::{EngineError, Inner};
use bisa_core::WorkstreamId;
use bisa_vcs::{ConfigScope, IdentitySource, ProfileFile, ACCOUNT_KEY};
use std::path::PathBuf;

#[derive(
    Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct RepoConnection {
    pub workstream: String,
    /// `None` for a checkout with no `origin`.
    pub remote: Option<RemoteFacts>,
    /// The code host behind the remote, by id, when this build knows one.
    pub code_host: Option<String>,
    /// The profile the remote falls under, when one does.
    pub profile: Option<ProfileFacts>,
    pub identity: IdentityFacts,
    pub transport: Transport,
    pub account: AccountFacts,
    pub cautions: Vec<Caution>,
}

#[derive(
    Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct RemoteFacts {
    pub url: String,
    pub protocol: RemoteProtocol,
    /// The host the remote reaches — an alias resolved through `ssh -G`.
    pub host: Option<String>,
    /// The alias as written, when the URL named one that resolved elsewhere.
    pub alias: Option<String>,
    pub owner: Option<String>,
    pub name: Option<String>,
    /// `host · owner/name`, for a person.
    pub summary: String,
}

#[derive(
    Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct ProfileFacts {
    pub slug: String,
    pub label: String,
    pub name: String,
    pub email: String,
    pub ssh_key: Option<String>,
    pub account: Option<String>,
}

#[derive(
    Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct IdentityFacts {
    pub name: Option<String>,
    pub email: Option<String>,
    /// The vcs crate's word: `local` · `global` · `none`. A profile's identity
    /// reads `global` — it resolves outside the repository — and `profile`
    /// says which.
    pub source: IdentitySourceView,
    /// The profile the effective identity came from, when it came from one.
    pub profile: Option<String>,
    /// The file git read the effective `user.email` from.
    pub origin: Option<String>,
}

/// `bisa_vcs::IdentitySource` on the wire — the same three words.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum IdentitySourceView {
    Local,
    Global,
    None,
}

impl From<IdentitySource> for IdentitySourceView {
    fn from(s: IdentitySource) -> Self {
        match s {
            IdentitySource::Local => Self::Local,
            IdentitySource::Global => Self::Global,
            IdentitySource::None => Self::None,
        }
    }
}

/// How a push reaches the remote, and what it will hand over.
#[derive(
    Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Transport {
    Ssh {
        /// The private key ssh would offer — the profile's `-i` first, else the
        /// first of `ssh -G`'s identity files that has a `.pub` beside it.
        key: Option<PathBuf>,
        key_name: Option<String>,
        /// Whether ssh-agent holds that key; `None` when SSH is not configured
        /// for this engine or the agent did not answer.
        loaded: Option<bool>,
        identities_only: bool,
        agent_available: Option<bool>,
        /// `GIT_SSH_COMMAND` is set in the node's environment and overrides
        /// every `core.sshCommand`.
        env_override: bool,
        /// This engine has SSH at all.
        ssh_configured: bool,
    },
    Https {
        /// Git's credential helpers, one word each.
        helpers: Vec<String>,
        /// The username git's helper holds for the host, when it holds one.
        helper_username: Option<String>,
        /// The `credential.username` the checkout resolves — a profile's.
        credential_username: Option<String>,
    },
    Local,
    None,
}

#[derive(
    Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct AccountFacts {
    /// The login the code host will be asked as, when one resolves.
    pub login: Option<String>,
    pub source: AccountSource,
    /// The stored accounts, so a pin can be chosen among them.
    pub stored: Vec<String>,
    pub env_override: bool,
}

/// Where the account came from — the chain, in its order.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AccountSource {
    /// A pin in the repository's local config.
    Local,
    /// The matched profile's.
    Profile,
    /// The kind's global default (`codehost.<kind>.account`).
    Global,
    /// The kind's environment variable answers whatever is named.
    Env,
    /// Nothing named it; the one stored account answers.
    OnlyStored,
    /// Nothing named it; the code host's CLI is signed in on this machine.
    Cli,
    /// Nothing named it; git's credential helper holds a login for the host.
    GitHelper,
    None,
}

/// Something a person would want to know before pushing — never a block.
#[derive(
    Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct Caution {
    pub id: CautionId,
    pub sentence: String,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CautionId {
    /// A local identity is set and it is not the matched profile's.
    IdentityDiffersFromProfile,
    /// An SSH remote whose key ssh-agent does not hold.
    KeyNotLoaded,
    /// A code host remote and no account resolves.
    NoAccount,
    /// The owner is an organization the account does not see itself in.
    AccountOutsideOwner,
    /// The profile's `credential.username` is not its account.
    CredentialUsernameDiffers,
    /// One key is bound in two profiles with different accounts.
    KeySharedAcrossAccounts,
}

/// What the probes answered (`POST …/git/connection/check`).
#[derive(
    Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct ConnectionCheck {
    /// The code host's word on the bound account, when there is a code host.
    pub code_host: Option<Connection>,
    /// Whether that account sees the repository and may push.
    pub access: Option<RepoAccess>,
    /// The SSH handshake's greeting, for an SSH remote with SSH configured.
    pub ssh: Option<crate::ssh::HostGreeting>,
    /// `git ls-remote --heads origin`: the heads it listed, or git's refusal.
    pub ls_remote: Option<LsRemoteOutcome>,
}

#[derive(
    Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct LsRemoteOutcome {
    pub ok: bool,
    pub heads: usize,
    pub detail: Option<String>,
}

/// The facts the cautions are computed from — what `connection` gathered,
/// handed to one pure function.
struct Facts<'a> {
    remote: Option<&'a RemoteFacts>,
    code_host: Option<&'a str>,
    /// The kind behind `code_host`, for the words — `None` for a test's fake.
    kind: Option<CodeHostKind>,
    profile: Option<&'a GitProfileView>,
    identity: &'a IdentityFacts,
    transport: &'a Transport,
    account: &'a AccountFacts,
    organizations: Option<&'a [String]>,
    profiles: &'a [GitProfileView],
}

/// The Settings panel that signs a kind in, and the CLI that does it, for a sentence.
fn sign_in_words(kind: Option<CodeHostKind>) -> String {
    match kind {
        Some(kind) => match kind.cli() {
            Some(cli) => format!(
                "sign in under Settings › Git & code hosts › {label} — with the {cli} (`{bin} auth login`) or a token — or name an account in a profile",
                label = kind.label(),
                cli = cli.label(),
                bin = cli.binary()
            ),
            None => format!("add a token under Settings › Git & code hosts › {}, or name an account in a profile", kind.label()),
        },
        None => "sign in on your code host's panel under Settings › Git & code hosts, or name an account in a profile".to_string(),
    }
}

fn cautions(f: &Facts<'_>) -> Vec<Caution> {
    let mut out = Vec::new();
    let mut push = |id: CautionId, sentence: String| out.push(Caution { id, sentence });
    if let Some(p) = f.profile {
        if f.identity.source == IdentitySourceView::Local
            && (f.identity.name.as_deref() != Some(p.profile.name.as_str())
                || f.identity.email.as_deref() != Some(p.profile.email.as_str()))
        {
            push(
                CautionId::IdentityDiffersFromProfile,
                format!(
                    "this repository sets its own author ({} <{}>), not the {} profile's ({} <{}>) — commits here will not carry the profile's identity",
                    f.identity.name.as_deref().unwrap_or("?"),
                    f.identity.email.as_deref().unwrap_or("?"),
                    p.profile.label,
                    p.profile.name,
                    p.profile.email
                ),
            );
        }
        if let Some(key) = &p.profile.ssh_key {
            let others: Vec<&GitProfileView> = f
                .profiles
                .iter()
                .filter(|o| {
                    o.profile.slug != p.profile.slug
                        && o.profile.ssh_key.as_deref() == Some(key.as_str())
                        && o.profile.account != p.profile.account
                })
                .collect();
            if let Some(other) = others.first() {
                push(
                    CautionId::KeySharedAcrossAccounts,
                    format!(
                        "the key {key} is bound to the {} profile and to the {} profile with a different account — a push from either reaches the host as the same key",
                        p.profile.label, other.profile.label
                    ),
                );
            }
        }
    }
    if let Transport::Ssh {
        key: Some(key),
        loaded: Some(false),
        key_name,
        ..
    } = f.transport
    {
        push(
            CautionId::KeyNotLoaded,
            format!(
                "ssh-agent does not hold {} ({}) — a push over SSH will ask for it or fail; load it under Settings › Git & code hosts › SSH keys",
                key_name.as_deref().unwrap_or("the key"),
                key.display()
            ),
        );
    }
    if let Transport::Https {
        credential_username: Some(cred),
        ..
    } = f.transport
    {
        if let Some(login) = &f.account.login {
            if !cred.eq_ignore_ascii_case(login) {
                push(
                    CautionId::CredentialUsernameDiffers,
                    format!("HTTPS pushes go as {cred} while pull requests are opened as @{login} — two accounts on one repository"),
                );
            }
        }
    }
    // Nothing answers: no pin, no profile, no default, no environment, no
    // CLI signed in, no helper — only then is there a caution. A machine
    // whose `gh` or git already knows the host is not one.
    if f.code_host.is_some() && f.account.login.is_none() && !f.account.env_override {
        let where_ = match f.remote.and_then(|r| r.host.as_deref()) {
            Some(h) => format!(" on {h}"),
            None => String::new(),
        };
        let label = f.kind.map(|k| k.label()).unwrap_or("code host");
        push(
            CautionId::NoAccount,
            format!(
                "no {label} account resolves for this repository{where_}: pull requests cannot be opened until you {}",
                sign_in_words(f.kind)
            ),
        );
    }
    if let (Some(orgs), Some(login), Some(owner)) = (
        f.organizations,
        &f.account.login,
        f.remote.and_then(|r| r.owner.as_deref()),
    ) {
        if !orgs.is_empty()
            && !owner.eq_ignore_ascii_case(login)
            && !orgs.iter().any(|o| o.eq_ignore_ascii_case(owner))
        {
            push(
                CautionId::AccountOutsideOwner,
                format!("@{login} is not a member of {owner} as far as its token can see — a pull request here may be refused"),
            );
        }
    }
    out
}

/// The remote's facts: the URL as written, the host it reaches once an SSH
/// alias is resolved, and the alias when there was one. Shared with the
/// inspection of a remote before a project exists.
pub(crate) fn remote_facts(parsed: &RemoteUrl, resolved: &RemoteUrl) -> RemoteFacts {
    let alias = (parsed.host != resolved.host)
        .then(|| parsed.host.clone())
        .flatten();
    RemoteFacts {
        url: resolved.raw.clone(),
        protocol: resolved.protocol,
        host: resolved.host.clone(),
        alias,
        owner: resolved.owner.clone(),
        name: resolved.name.clone(),
        summary: resolved.summary(),
    }
}

/// The connection facts for one checkout (`GET /workstreams/{wid}/git/connection`).
/// The account chain walked once: a pin or a profile, the kind's default,
/// the environment, the one stored account, the CLI signed in here, git's
/// helper — the same order for the connection card and for the committer
/// suggestion, so the two can never name different accounts.
struct AccountChain {
    facts: AccountFacts,
    status: Option<bisa_codehost::creds::AccountsStatus>,
}

async fn account_chain(
    inner: &Inner,
    kind: Option<CodeHostKind>,
    host_name: Option<&str>,
    account: &Option<String>,
    account_file: Option<&std::path::Path>,
    profiles_dir: &std::path::Path,
) -> Result<AccountChain, EngineError> {
    let status = match (kind, host_name) {
        (Some(kind), Some(host)) => {
            Some(bisa_codehost::creds::accounts_status(&inner.hosts.store_for(kind, host)).await)
        }
        _ => None,
    };
    let default = match kind {
        Some(kind) => codehost::default_account(inner, kind).await?,
        None => None,
    };
    let stored: Vec<String> = status
        .as_ref()
        .map(|s| s.accounts.iter().map(|a| a.login.clone()).collect())
        .unwrap_or_default();
    let env_override = status.as_ref().is_some_and(|s| s.env_override);
    let cli_login = status.as_ref().and_then(|s| s.cli_login.clone());
    let helper_username = status.as_ref().and_then(|s| s.helper_username.clone());
    let named = match (account, account_file) {
        (Some(_), Some(file))
            if gitprofiles::profile_slug_of_file(profiles_dir, file).is_some() =>
        {
            Some(AccountSource::Profile)
        }
        (Some(_), _) => Some(AccountSource::Local),
        (None, _) => None,
    };
    let (login, account_source) = match named {
        Some(source) => (account.clone(), source),
        None if default.is_some() => (default.clone(), AccountSource::Global),
        None if env_override => (None, AccountSource::Env),
        None if stored.len() == 1 => (stored.first().cloned(), AccountSource::OnlyStored),
        None if cli_login.is_some() => (cli_login.clone(), AccountSource::Cli),
        None if helper_username.is_some() => (helper_username.clone(), AccountSource::GitHelper),
        None => (None, AccountSource::None),
    };
    Ok(AccountChain {
        facts: AccountFacts {
            login,
            source: account_source,
            stored,
            env_override,
        },
        status,
    })
}

/// The code host and the account a checkout's remote resolves to — the
/// same chain the connection card walks, without the transport probes: no
/// `ssh -G`, no key check. `None` when the remote is not a code host this
/// build knows or no account resolves.
pub async fn resolved_account(
    inner: &Inner,
    wid: WorkstreamId,
) -> Result<Option<(CodeHostKind, String)>, EngineError> {
    let (_, path) = checkout_tree(inner, wid)?;
    let git = inner.git();
    let profiles_dir = inner.ws.paths().git_profiles_dir();
    let (remote_raw, account, account_origin, kind_hint) = blocking({
        let path = path.clone();
        move || {
            let remote = git.remote_get(&path, "origin")?;
            let account = git.account_get(&path)?;
            let account_origin = git.config_origin(Some(&path), ACCOUNT_KEY)?;
            let kind_hint = git
                .kind_get(&path)?
                .and_then(|k| k.parse::<CodeHostKind>().ok());
            Ok((remote, account, account_origin, kind_hint))
        }
    })
    .await?;
    let Some(raw) = remote_raw else {
        return Ok(None);
    };
    let resolved = codehost::resolve_alias(inner, RemoteUrl::parse(&raw)).await;
    let Some(kind) = inner
        .code_hosts
        .detect_as(&resolved, kind_hint)
        .map(|(h, _)| h.id())
        .and_then(codehost::kind_of)
    else {
        return Ok(None);
    };
    let chain = account_chain(
        inner,
        Some(kind),
        resolved.host.as_deref(),
        &account,
        account_origin.as_ref().and_then(|o| o.file()).as_deref(),
        &profiles_dir,
    )
    .await?;
    Ok(chain.facts.login.map(|login| (kind, login)))
}

pub async fn connection(inner: &Inner, wid: WorkstreamId) -> Result<RepoConnection, EngineError> {
    let (_, path) = checkout_tree(inner, wid)?;
    let git = inner.git();
    let profiles_dir = inner.ws.paths().git_profiles_dir();
    // Everything git knows, in one blocking hop.
    let (
        remote_raw,
        identity,
        email_origin,
        account,
        account_origin,
        kind_hint,
        ssh_command,
        credential_username,
    ) = blocking({
        let path = path.clone();
        move || {
            let remote = git.remote_get(&path, "origin")?;
            let identity = git.identity(&path)?;
            let email_origin = git.config_origin(Some(&path), "user.email")?;
            let account = git.account_get(&path)?;
            let account_origin = git.config_origin(Some(&path), ACCOUNT_KEY)?;
            let kind_hint = git
                .kind_get(&path)?
                .and_then(|k| k.parse::<CodeHostKind>().ok());
            let ssh_command = git
                .config_origin(Some(&path), "core.sshCommand")?
                .map(|o| o.value);
            let credential_username = git
                .config_origin(Some(&path), "credential.username")?
                .map(|o| o.value);
            Ok((
                remote,
                identity,
                email_origin,
                account,
                account_origin,
                kind_hint,
                ssh_command,
                credential_username,
            ))
        }
    })
    .await?;

    let profiles = gitprofiles::list(inner)
        .await
        .map(|v| v.profiles)
        .unwrap_or_default();
    let parsed = remote_raw.as_deref().map(RemoteUrl::parse);
    let resolved = match parsed.clone() {
        Some(url) => Some(codehost::resolve_alias(inner, url).await),
        None => None,
    };
    let remote = match (&parsed, &resolved) {
        (Some(p), Some(r)) => Some(remote_facts(p, r)),
        _ => None,
    };
    let detected = resolved.as_ref().and_then(|r| {
        inner
            .code_hosts
            .detect_as(r, kind_hint)
            .map(|(h, _)| h.id())
    });
    let code_host = detected.map(|id| id.0.to_string());
    let kind = detected.and_then(codehost::kind_of);
    let profile = remote_raw
        .as_deref()
        .and_then(|raw| profiles.iter().find(|p| p.profile.matches(raw)));
    let identity_profile = email_origin
        .as_ref()
        .and_then(|o| o.file())
        .and_then(|f| gitprofiles::profile_slug_of_file(&profiles_dir, &f))
        .map(|s| s.to_string());
    let identity_facts = IdentityFacts {
        name: identity.name.clone(),
        email: identity.email.clone(),
        source: identity.source.into(),
        profile: identity_profile,
        origin: email_origin.as_ref().map(|o| o.origin.clone()),
    };

    // The accounts of the remote's kind at the remote's host — the chain the
    // code host will walk: a pin or a profile, the kind's default, the
    // environment, the one stored account, the CLI signed in here, git's
    // helper. Without a code host there is no chain and nothing to say.
    let host_name = resolved.as_ref().and_then(|r| r.host.clone());
    let chain = account_chain(
        inner,
        kind,
        host_name.as_deref(),
        &account,
        account_origin.as_ref().and_then(|o| o.file()).as_deref(),
        &profiles_dir,
    )
    .await?;
    let status = chain.status;
    let helper_username = status.as_ref().and_then(|s| s.helper_username.clone());
    let account_facts = chain.facts;

    let transport = match resolved.as_ref().map(|r| r.protocol) {
        None => Transport::None,
        Some(RemoteProtocol::Local) | Some(RemoteProtocol::Other) => Transport::Local,
        Some(RemoteProtocol::Https) => Transport::Https {
            helpers: status
                .as_ref()
                .map(|s| s.helpers.clone())
                .unwrap_or_default(),
            helper_username,
            credential_username,
        },
        Some(RemoteProtocol::Ssh) | Some(RemoteProtocol::Scp) => {
            ssh_transport(inner, parsed.as_ref(), ssh_command.as_deref()).await
        }
    };

    let cautions = cautions(&Facts {
        remote: remote.as_ref(),
        code_host: code_host.as_deref(),
        kind,
        profile,
        identity: &identity_facts,
        transport: &transport,
        account: &account_facts,
        organizations: None,
        profiles: &profiles,
    });
    Ok(RepoConnection {
        workstream: wid.to_string(),
        remote,
        code_host,
        profile: profile.map(|p| ProfileFacts {
            slug: p.profile.slug.to_string(),
            label: p.profile.label.clone(),
            name: p.profile.name.clone(),
            email: p.profile.email.clone(),
            ssh_key: p.profile.ssh_key.clone(),
            account: p.profile.account.clone(),
        }),
        identity: identity_facts,
        transport,
        account: account_facts,
        cautions,
    })
}

/// The SSH side: the key `core.sshCommand` names, else what `ssh -G` would
/// offer the host as written (the alias, so the person's `Host` block counts),
/// and whether ssh-agent holds it.
async fn ssh_transport(
    inner: &Inner,
    url: Option<&RemoteUrl>,
    ssh_command: Option<&str>,
) -> Transport {
    let env_override = std::env::var_os("GIT_SSH_COMMAND").is_some();
    let Some(ssh) = inner.ssh.clone() else {
        let key = ssh_command.and_then(ProfileFile::key_of_ssh_command);
        return Transport::Ssh {
            key_name: key
                .as_ref()
                .and_then(|k| k.file_name().map(|n| n.to_string_lossy().into_owned())),
            key,
            loaded: None,
            identities_only: ssh_command.is_some_and(|c| c.contains("IdentitiesOnly=yes")),
            agent_available: None,
            env_override,
            ssh_configured: false,
        };
    };
    let host = url.and_then(|u| u.host.clone());
    let profile_key = ssh_command.and_then(ProfileFile::key_of_ssh_command);
    let facts = tokio::task::spawn_blocking(move || {
        // A probe that fails is *unknown*, never *no*: each `None` below
        // stays a `None`, and the reason is kept for troubleshooting.
        let overview = ssh
            .overview()
            .inspect_err(|e| tracing::debug!(target: "bisa_engine::ide", "ssh overview not read: {e}"))
            .ok();
        let resolved = host.as_deref().and_then(|h| {
            ssh.resolve(h, profile_key.as_deref())
                .inspect_err(|e| {
                    tracing::debug!(target: "bisa_engine::ide", "ssh config for {h} not resolved: {e}")
                })
                .ok()
        });
        let key = profile_key.or_else(|| {
            resolved.as_ref().and_then(|r| {
                r.identity_files
                    .iter()
                    .find(|f| {
                        overview
                            .as_ref()
                            .is_some_and(|o| o.keys.iter().any(|k| k.key.path == **f))
                    })
                    .cloned()
            })
        });
        let loaded = match (&overview, &key) {
            (Some(o), Some(k)) if o.agent.available => {
                Some(o.keys.iter().any(|row| row.key.path == *k && row.loaded))
            }
            _ => None,
        };
        (
            key,
            loaded,
            resolved.map(|r| r.identities_only).unwrap_or(false),
            overview.map(|o| o.agent.available),
        )
    })
    .await
    .inspect_err(|e| tracing::warn!(target: "bisa_engine::ide", "the ssh probe task ended early: {e}"))
    .ok();
    let (key, loaded, identities_only, agent_available) =
        facts.unwrap_or((None, None, false, None));
    Transport::Ssh {
        key_name: key
            .as_ref()
            .and_then(|k| k.file_name().map(|n| n.to_string_lossy().into_owned())),
        key,
        loaded,
        identities_only: identities_only
            || ssh_command.is_some_and(|c| c.contains("IdentitiesOnly=yes")),
        agent_available,
        env_override,
        ssh_configured: true,
    }
}

/// The three read-only probes for one checkout: the code host's word on the
/// bound account and its access to the repository, the SSH handshake for an
/// SSH remote, and `git ls-remote --heads origin`. Nothing on either side
/// changes; a refused probe is an answer, not an error.
pub async fn check(inner: &Inner, wid: WorkstreamId) -> Result<ConnectionCheck, EngineError> {
    let (_, path) = checkout_tree(inner, wid)?;
    let facts = connection(inner, wid).await?;
    let (code_host, access) = match codehost::code_host_for_path(inner, path.clone()).await {
        Ok((host, repo)) => {
            let (required, recommended) = codehost::kind_of(host.id())
                .map(codehost::scopes_for)
                .unwrap_or((&[], &[]));
            let connection = Connection::of(host.account().await, required, recommended);
            let access = host
                .repo_access(&repo)
                .await
                .inspect_err(|e| {
                    tracing::debug!(target: "bisa_engine::ide", "repository access not read: {e}")
                })
                .ok();
            (Some(connection), access)
        }
        Err(e) => {
            tracing::debug!(target: "bisa_engine::ide", "no code host for the checkout: {e}");
            (None, None)
        }
    };
    let ssh = match (&facts.transport, &facts.remote, inner.ssh.clone()) {
        (Transport::Ssh { key, .. }, Some(remote), Some(ssh)) => {
            let host = remote
                .alias
                .clone()
                .or_else(|| remote.host.clone())
                .unwrap_or_default();
            let user = RemoteUrl::parse(&remote.url)
                .user
                .unwrap_or_else(|| "git".to_string());
            let key = key.clone();
            tokio::task::spawn_blocking(move || ssh.test(&host, &user, key.as_deref()))
                .await
                .inspect_err(|e| {
                    tracing::warn!(target: "bisa_engine::ide", "the ssh test task ended early: {e}")
                })
                .ok()
                .and_then(|r| {
                    r.inspect_err(|e| {
                        tracing::debug!(target: "bisa_engine::ide", "ssh test did not run: {e}")
                    })
                    .ok()
                })
        }
        _ => None,
    };
    let ls_remote = if facts.remote.is_some() {
        let git = inner.git();
        let outcome = tokio::task::spawn_blocking(move || git.ls_remote(&path, "origin")).await;
        Some(match outcome {
            Ok(Ok(heads)) => LsRemoteOutcome {
                ok: true,
                heads: heads.len(),
                detail: None,
            },
            Ok(Err(e)) => LsRemoteOutcome {
                ok: false,
                heads: 0,
                detail: Some(e.to_string()),
            },
            Err(e) => LsRemoteOutcome {
                ok: false,
                heads: 0,
                detail: Some(e.to_string()),
            },
        })
    } else {
        None
    };
    Ok(ConnectionCheck {
        code_host,
        access,
        ssh,
        ls_remote,
    })
}

/// Pin — or, with `None`, unpin — the code host account for one repository:
/// `codehost.account` in its local config, a schema key through
/// `config_set`. Answers the fresh connection.
pub async fn set_account(
    inner: &Inner,
    wid: WorkstreamId,
    login: Option<String>,
) -> Result<RepoConnection, EngineError> {
    let (_, path) = checkout_tree(inner, wid)?;
    let git = inner.git();
    blocking(move || match &login {
        Some(l) => git.config_set(ConfigScope::Local, Some(&path), ACCOUNT_KEY, l),
        None => git.config_unset(ConfigScope::Local, Some(&path), ACCOUNT_KEY),
    })
    .await?;
    inner.ide_status.invalidate(wid);
    connection(inner, wid).await
}

/// The profile the checkout's effective identity came from, when it came
/// from one — the `profile` fact beside `GET …/git/identity`.
pub async fn identity_profile(
    inner: &Inner,
    wid: WorkstreamId,
) -> Result<Option<String>, EngineError> {
    let (_, path) = checkout_tree(inner, wid)?;
    let git = inner.git();
    let dir = inner.ws.paths().git_profiles_dir();
    blocking(move || {
        Ok(git
            .config_origin(Some(&path), "user.email")?
            .and_then(|o| o.file())
            .and_then(|f| gitprofiles::profile_slug_of_file(&dir, &f))
            .map(|s| s.to_string()))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_core::{GitProfile, ProfileSpec, Slug};

    fn view(slug: &str, key: Option<&str>, account: Option<&str>) -> GitProfileView {
        let profile = GitProfile::from_spec(
            Slug::new(slug).unwrap(),
            ProfileSpec {
                label: slug.to_uppercase(),
                host: "github.com".into(),
                owner: slug.into(),
                aliases: vec![],
                name: "Ada".into(),
                email: format!("ada@{slug}.example"),
                ssh_key: key.map(str::to_string),
                account: account.map(str::to_string),
            },
        )
        .unwrap();
        GitProfileView {
            globs: profile.url_globs(),
            profile,
            file: PathBuf::from(format!("/ws/identity/git/profiles/{slug}.gitconfig")),
        }
    }

    fn remote(owner: &str) -> RemoteFacts {
        RemoteFacts {
            url: format!("git@github.com:{owner}/web.git"),
            protocol: RemoteProtocol::Scp,
            host: Some("github.com".into()),
            alias: None,
            owner: Some(owner.into()),
            name: Some("web".into()),
            summary: format!("github.com · {owner}/web"),
        }
    }

    fn ids(c: &[Caution]) -> Vec<CautionId> {
        c.iter().map(|c| c.id).collect()
    }

    #[test]
    fn every_caution_fires_on_its_shape_and_a_clean_setup_has_none() {
        let acme = view("acme", Some("/k/acme"), Some("ada-acme"));
        let profiles = vec![
            acme.clone(),
            view("widgets", Some("/k/acme"), Some("ada-widgets")),
            view("other", Some("/k/other"), Some("ada-acme")),
        ];
        let r = remote("acme");
        let identity = IdentityFacts {
            name: Some("Ada".into()),
            email: Some("ada@acme.example".into()),
            source: IdentitySourceView::Global,
            profile: Some("acme".into()),
            origin: None,
        };
        let loaded = Transport::Ssh {
            key: Some(PathBuf::from("/k/acme")),
            key_name: Some("acme".into()),
            loaded: Some(true),
            identities_only: true,
            agent_available: Some(true),
            env_override: false,
            ssh_configured: true,
        };
        let account = AccountFacts {
            login: Some("ada-acme".into()),
            source: AccountSource::Profile,
            stored: vec!["ada-acme".into()],
            env_override: false,
        };
        let clean = cautions(&Facts {
            remote: Some(&r),
            code_host: Some("github"),
            kind: Some(CodeHostKind::GitHub),
            profile: Some(&acme),
            identity: &identity,
            transport: &loaded,
            account: &account,
            organizations: Some(&["acme".to_string()]),
            profiles: &profiles[..1],
        });
        assert!(clean.is_empty(), "{clean:?}");

        // The key is shared with a profile bound to another account.
        let shared = cautions(&Facts {
            profiles: &profiles,
            remote: Some(&r),
            code_host: Some("github"),
            kind: Some(CodeHostKind::GitHub),
            profile: Some(&acme),
            identity: &identity,
            transport: &loaded,
            account: &account,
            organizations: None,
        });
        assert_eq!(ids(&shared), [CautionId::KeySharedAcrossAccounts]);
        assert!(
            shared[0].sentence.contains("WIDGETS"),
            "{}",
            shared[0].sentence
        );

        // A local identity that is not the profile's.
        let local = IdentityFacts {
            name: Some("Bob".into()),
            source: IdentitySourceView::Local,
            ..identity.clone()
        };
        let differs = cautions(&Facts {
            identity: &local,
            profiles: &profiles[..1],
            remote: Some(&r),
            code_host: Some("github"),
            kind: Some(CodeHostKind::GitHub),
            profile: Some(&acme),
            transport: &loaded,
            account: &account,
            organizations: None,
        });
        assert_eq!(ids(&differs), [CautionId::IdentityDiffersFromProfile]);

        // The key is not loaded.
        let unloaded = Transport::Ssh {
            key: Some(PathBuf::from("/k/acme")),
            key_name: Some("acme".into()),
            loaded: Some(false),
            identities_only: true,
            agent_available: Some(true),
            env_override: false,
            ssh_configured: true,
        };
        let not_loaded = cautions(&Facts {
            transport: &unloaded,
            profiles: &profiles[..1],
            remote: Some(&r),
            code_host: Some("github"),
            kind: Some(CodeHostKind::GitHub),
            profile: Some(&acme),
            identity: &identity,
            account: &account,
            organizations: None,
        });
        assert_eq!(ids(&not_loaded), [CautionId::KeyNotLoaded]);
        assert!(not_loaded[0].sentence.contains("SSH keys"));

        // No account for a code host remote — but not when the environment answers.
        let nobody = AccountFacts {
            login: None,
            source: AccountSource::None,
            stored: vec![],
            env_override: false,
        };
        let none = cautions(&Facts {
            account: &nobody,
            profile: None,
            profiles: &[],
            remote: Some(&r),
            code_host: Some("github"),
            kind: Some(CodeHostKind::GitHub),
            identity: &identity,
            transport: &loaded,
            organizations: None,
        });
        assert_eq!(ids(&none), [CautionId::NoAccount]);
        let env = AccountFacts {
            env_override: true,
            ..nobody.clone()
        };
        assert!(cautions(&Facts {
            account: &env,
            profile: None,
            profiles: &[],
            remote: Some(&r),
            code_host: Some("github"),
            kind: Some(CodeHostKind::GitHub),
            identity: &identity,
            transport: &loaded,
            organizations: None
        })
        .is_empty());
        assert!(
            cautions(&Facts {
                account: &nobody,
                profile: None,
                profiles: &[],
                remote: Some(&r),
                code_host: None,
                kind: None,
                identity: &identity,
                transport: &loaded,
                organizations: None
            })
            .is_empty(),
            "no code host, no account needed"
        );

        // The account is outside the owner's organizations.
        let outside = cautions(&Facts {
            organizations: Some(&["widgets".to_string()]),
            profiles: &profiles[..1],
            remote: Some(&r),
            code_host: Some("github"),
            kind: Some(CodeHostKind::GitHub),
            profile: Some(&acme),
            identity: &identity,
            transport: &loaded,
            account: &account,
        });
        assert_eq!(ids(&outside), [CautionId::AccountOutsideOwner]);
        let own_repo = remote("ada-acme");
        assert!(
            cautions(&Facts {
                remote: Some(&own_repo),
                organizations: Some(&["widgets".to_string()]),
                profiles: &profiles[..1],
                code_host: Some("github"),
                kind: Some(CodeHostKind::GitHub),
                profile: None,
                identity: &identity,
                transport: &loaded,
                account: &account
            })
            .is_empty(),
            "the account's own repositories are never outside"
        );

        // HTTPS: the helper's username is one login, the pull requests another.
        let https = Transport::Https {
            helpers: vec!["osxkeychain".into()],
            helper_username: None,
            credential_username: Some("ada-personal".into()),
        };
        let two = cautions(&Facts {
            transport: &https,
            profiles: &profiles[..1],
            remote: Some(&r),
            code_host: Some("github"),
            kind: Some(CodeHostKind::GitHub),
            profile: Some(&acme),
            identity: &identity,
            account: &account,
            organizations: None,
        });
        assert_eq!(ids(&two), [CautionId::CredentialUsernameDiffers]);
    }
}
