//! `bisa git …`: the person's git setup — profiles by organization, SSH
//! for git hosts, the code host accounts, and what one checkout will use when
//! it talks to its remote (ide/04, ide/08). The same engine functions the
//! desktop's Settings › Git & code hosts and Git › Repository call; the CLI adds
//! nothing of its own and never prints a secret — a token comes in on stdin,
//! once, and never comes back out.

use anyhow::{anyhow, Context as _, Result};
use bisa_core::{ProfileSpec, Slug, WorkstreamId};
use bisa_engine::ide::connection::{self, Transport};
use bisa_engine::{codehost, gitprofiles, ssh};
use clap::Subcommand;
use std::path::PathBuf;

use crate::ctx::Ctx;
use crate::output::Out;

#[derive(Subcommand, Debug)]
pub enum GitCmd {
    /// Profiles by organization: who you are for one owner on one code host
    Profile {
        #[command(subcommand)]
        command: ProfileCmd,
    },
    /// SSH for git hosts: your public keys, ssh-agent, a new key, one handshake
    Ssh {
        #[command(subcommand)]
        command: SshCmd,
    },
    /// The code host accounts of one kind: their logins, the default, a check
    Account {
        /// Which code host: `github` (default), `gitlab` or `bitbucket`
        #[arg(long, default_value = "github", global = true)]
        host: String,
        #[command(subcommand)]
        command: AccountCmd,
    },
    /// One code host's health: its CLI (installed, signed in as whom), the
    /// stored accounts, git's helper, the default, who would answer
    Health {
        /// Which code host: `github` (default), `gitlab` or `bitbucket`
        #[arg(long, default_value = "github")]
        host: String,
    },
    /// How to sign in to one code host: the CLI's browser login to run, the
    /// way to install the CLI, or the page a token comes from
    Login {
        /// Which code host: `github` (default), `gitlab` or `bitbucket`
        #[arg(long, default_value = "github")]
        host: String,
    },
    /// What a checkout will use when it talks to its remote — the remote, the
    /// protocol, the profile, who commits, the key, the account, the cautions
    Connection {
        /// A workstream id; a project's own tree is its primary workstream
        workstream: String,
        /// Also run the read-only probes: the code host, the SSH handshake, `git ls-remote`
        #[arg(long)]
        check: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum ProfileCmd {
    /// Every profile, and the includes of your global git config that are not the platform's
    List,
    /// Save a profile (a new one, or the one with this slug)
    Set {
        /// The profile's slug — its file name under the workspace's `identity/git/profiles/`
        slug: String,
        #[arg(long)]
        label: String,
        /// The code host, `github.com` by default
        #[arg(long, default_value = "github.com")]
        host: String,
        /// The owner — an organization or a user on the host
        #[arg(long)]
        owner: String,
        /// SSH `Host` aliases that stand for the host in a remote URL (`github-work`)
        #[arg(long = "alias")]
        aliases: Vec<String>,
        #[arg(long)]
        name: String,
        #[arg(long)]
        email: String,
        /// The private key `core.sshCommand` will name; never opened by the platform
        #[arg(long = "ssh-key")]
        ssh_key: Option<String>,
        /// The code host login whose stored token answers for these repositories
        #[arg(long)]
        account: Option<String>,
    },
    /// Remove a profile: its includes, then its file
    Rm { slug: String },
}

#[derive(Subcommand, Debug)]
pub enum SshCmd {
    /// Your public keys with whether ssh-agent holds them, ssh-agent, and the `Host` blocks for git hosts
    Keys,
    /// Generate an ed25519 key pair in your SSH directory (no passphrase from here — add one with `ssh-keygen -p`)
    Keygen {
        /// The file name, `id_ed25519_acme`
        name: String,
        /// The comment the public key carries — an email, a machine
        #[arg(long, default_value = "")]
        comment: String,
    },
    /// Load a key pair into ssh-agent
    Load { name: String },
    /// What ssh would do for a host — offline: the hostname an alias stands for, the identities it would offer
    Resolve {
        host: String,
        /// Force this key, as a profile's `core.sshCommand` would
        #[arg(long)]
        key: Option<PathBuf>,
    },
    /// One authentication handshake with a git host (`ssh -T user@host`); nothing is written on either side
    Test {
        host: String,
        #[arg(long, default_value = "git")]
        user: String,
        #[arg(long)]
        key: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
pub enum AccountCmd {
    /// The stored accounts, where each token lives, git's helpers, the default
    List,
    /// Add an account: the token is read from **stdin**, verified with the host, and stored under the login the host answers
    Add {
        /// The login the token belongs to — Bitbucket's API token is a Basic credential and needs it
        #[arg(long)]
        login: Option<String>,
    },
    /// Forget one account's token
    Rm { login: String },
    /// Ask the host whose the login's credential is, and which organizations it sees
    Check { login: String },
    /// Set the kind's default account (the global `codehost.<kind>.account`), or clear it with `--clear`
    Default {
        login: Option<String>,
        #[arg(long)]
        clear: bool,
    },
}

pub async fn git(ctx: &Ctx, out: &Out, cmd: GitCmd) -> Result<()> {
    match cmd {
        GitCmd::Profile { command } => profile(ctx, out, command).await,
        GitCmd::Ssh { command } => ssh_cmd(ctx, out, command).await,
        GitCmd::Account { host, command } => account(ctx, out, &host, command).await,
        GitCmd::Health { host } => health_cmd(ctx, out, &host).await,
        GitCmd::Login { host } => login_cmd(ctx, out, &host).await,
        GitCmd::Connection { workstream, check } => {
            connection_cmd(ctx, out, &workstream, check).await
        }
    }
}

/// The kind a `--host` names, or the refusal that lists the three.
fn kind_of(host: &str) -> Result<codehost::CodeHostKind> {
    host.parse::<codehost::CodeHostKind>()
        .map_err(|e| anyhow!(e))
}

/// Where a git setup verb is carried out. One engine holds a workspace: while
/// a node runs — whenever the desktop is open — the verb is the node's to
/// carry out, through the routes Settings › Git & code hosts uses; with no
/// node, an engine of this process's own does it. Either way the answer is
/// the engine's own type, so what a verb prints is one piece of code.
enum Door {
    Node(crate::client::NodeClient),
    Own(bisa_engine::Engine),
}

/// What the node answered, read as the type the engine answers with.
fn answered<T: serde::de::DeserializeOwned>(answer: serde_json::Value) -> Result<T> {
    serde_json::from_value(answer).context(bisa_core::text!("cli-gitsetup-node-answer-not-read"))
}

impl Door {
    async fn open(ctx: &Ctx) -> Result<Self> {
        Ok(match ctx.node_client().await {
            Some(node) => Door::Node(node),
            None => Door::Own(ctx.engine().await?.0),
        })
    }

    /// An engine of this process's own is stopped; the node's goes on.
    async fn close(self) {
        if let Door::Own(engine) = self {
            engine.shutdown().await;
        }
    }

    async fn health(&self, kind: codehost::CodeHostKind) -> Result<codehost::CodeHostHealth> {
        match self {
            Door::Node(node) => answered(node.get(&format!("/codehost/{kind}/health")).await?),
            Door::Own(engine) => Ok(codehost::health(engine.inner(), kind).await?),
        }
    }

    async fn login_plan(&self, kind: codehost::CodeHostKind) -> Result<codehost::LoginPlan> {
        match self {
            Door::Node(node) => answered(node.get(&format!("/codehost/{kind}/login")).await?),
            Door::Own(engine) => Ok(codehost::login_plan(engine.inner(), kind).await),
        }
    }

    async fn accounts(&self, kind: codehost::CodeHostKind) -> Result<codehost::AccountsView> {
        match self {
            Door::Node(node) => answered(node.get(&format!("/codehost/{kind}/accounts")).await?),
            Door::Own(engine) => Ok(codehost::accounts(engine.inner(), kind).await?),
        }
    }

    async fn add_account(
        &self,
        kind: codehost::CodeHostKind,
        token: &str,
        login: Option<&str>,
    ) -> Result<(String, codehost::Connection)> {
        match self {
            Door::Node(node) => {
                let v = node
                    .put(
                        &format!("/codehost/{kind}/accounts"),
                        serde_json::json!({"token": token, "login": login}),
                    )
                    .await?;
                Ok((
                    answered(v["login"].clone())?,
                    answered(v["connection"].clone())?,
                ))
            }
            Door::Own(engine) => {
                Ok(codehost::add_account(engine.inner(), kind, token, login).await?)
            }
        }
    }

    async fn forget_account(&self, kind: codehost::CodeHostKind, login: &str) -> Result<()> {
        match self {
            Door::Node(node) => {
                node.delete(&format!("/codehost/{kind}/accounts/{}", segment(login)))
                    .await?;
                Ok(())
            }
            Door::Own(engine) => Ok(codehost::forget_account(engine.inner(), kind, login)?),
        }
    }

    async fn check_account(
        &self,
        kind: codehost::CodeHostKind,
        login: &str,
    ) -> Result<codehost::Connection> {
        match self {
            Door::Node(node) => answered(
                node.post(
                    &format!("/codehost/{kind}/accounts/{}/check", segment(login)),
                    serde_json::json!({}),
                )
                .await?,
            ),
            Door::Own(engine) => Ok(codehost::check_account(engine.inner(), kind, login).await?),
        }
    }

    async fn set_default_account(
        &self,
        kind: codehost::CodeHostKind,
        login: Option<&str>,
    ) -> Result<Option<String>> {
        match self {
            Door::Node(node) => {
                let v = node
                    .put(
                        &format!("/codehost/{kind}/default"),
                        serde_json::json!({"login": login}),
                    )
                    .await?;
                answered(v["default"].clone())
            }
            Door::Own(engine) => {
                Ok(codehost::set_default_account(engine.inner(), kind, login).await?)
            }
        }
    }

    async fn profiles(&self) -> Result<gitprofiles::GitProfilesView> {
        match self {
            Door::Node(node) => answered(node.get("/git/profiles").await?),
            Door::Own(engine) => Ok(gitprofiles::list(engine.inner()).await?),
        }
    }

    async fn put_profile(
        &self,
        slug: Slug,
        spec: ProfileSpec,
    ) -> Result<gitprofiles::GitProfileView> {
        match self {
            Door::Node(node) => answered(
                node.put(
                    &format!("/git/profiles/{slug}"),
                    serde_json::to_value(&spec)?,
                )
                .await?,
            ),
            Door::Own(engine) => Ok(gitprofiles::put(engine.inner(), slug, spec).await?),
        }
    }

    async fn remove_profile(&self, slug: &Slug) -> Result<()> {
        match self {
            Door::Node(node) => {
                node.delete(&format!("/git/profiles/{slug}")).await?;
                Ok(())
            }
            Door::Own(engine) => Ok(gitprofiles::remove(engine.inner(), slug.clone()).await?),
        }
    }

    async fn ssh_overview(&self) -> Result<ssh::SshOverview> {
        match self {
            Door::Node(node) => answered(node.get("/git/ssh").await?),
            Door::Own(engine) => Ok(ssh::overview(engine.inner()).await?),
        }
    }

    async fn ssh_generate(&self, key: ssh::NewKey) -> Result<ssh::PublicKey> {
        match self {
            Door::Node(node) => answered(
                node.post(
                    "/git/ssh/keys",
                    serde_json::json!({"name": key.name, "comment": key.comment}),
                )
                .await?,
            ),
            Door::Own(engine) => Ok(ssh::generate(engine.inner(), key).await?),
        }
    }

    async fn ssh_load(&self, name: &str) -> Result<()> {
        match self {
            Door::Node(node) => {
                node.post(&ssh_load_path(name), serde_json::json!({}))
                    .await?;
                Ok(())
            }
            Door::Own(engine) => Ok(ssh::load(engine.inner(), name.to_string()).await?),
        }
    }

    async fn ssh_resolve(&self, host: &str, key: Option<PathBuf>) -> Result<ssh::Resolved> {
        match self {
            Door::Node(node) => answered(node.get(&ssh_resolve_path(host, key.as_deref())).await?),
            Door::Own(engine) => Ok(ssh::resolve(engine.inner(), host.to_string(), key).await?),
        }
    }

    async fn ssh_test(
        &self,
        host: &str,
        user: String,
        key: Option<PathBuf>,
    ) -> Result<ssh::HostGreeting> {
        match self {
            Door::Node(node) => answered(
                node.post(
                    "/git/ssh/test",
                    serde_json::json!({
                        "host": host,
                        "user": user,
                        "key": key.as_ref().map(|k| k.to_string_lossy().into_owned()),
                    }),
                )
                .await?,
            ),
            Door::Own(engine) => Ok(ssh::test(engine.inner(), host.to_string(), user, key).await?),
        }
    }

    async fn connection(&self, wid: WorkstreamId) -> Result<connection::RepoConnection> {
        match self {
            Door::Node(node) => answered(
                node.get(&format!("/workstreams/{wid}/git/connection"))
                    .await?,
            ),
            Door::Own(engine) => Ok(connection::connection(engine.inner(), wid).await?),
        }
    }

    async fn connection_check(&self, wid: WorkstreamId) -> Result<connection::ConnectionCheck> {
        match self {
            Door::Node(node) => answered(
                node.post(
                    &format!("/workstreams/{wid}/git/connection/check"),
                    serde_json::json!({}),
                )
                .await?,
            ),
            Door::Own(engine) => Ok(connection::check(engine.inner(), wid).await?),
        }
    }
}

/// The node's route that loads one key pair into ssh-agent.
fn ssh_load_path(name: &str) -> String {
    format!("/git/ssh/keys/{}/load", segment(name))
}

/// The node's route that says what ssh would do for a host, a profile's key
/// forced when one is given.
fn ssh_resolve_path(host: &str, key: Option<&std::path::Path>) -> String {
    let mut path = format!("/git/ssh/resolve?host={}", segment(host));
    if let Some(key) = key {
        path.push_str(&format!("&key={}", segment(&key.to_string_lossy())));
    }
    path
}

/// One segment of a path, or one value of a query, as a URL carries it: the
/// unreserved characters as they are, every other byte percent-encoded — a
/// name with a slash or a space never becomes another route.
fn segment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

async fn health_cmd(ctx: &Ctx, out: &Out, host: &str) -> Result<()> {
    let kind = kind_of(host)?;
    let door = Door::open(ctx).await?;
    let result: Result<serde_json::Value> = async {
        let health = door.health(kind).await?;
        match &health.cli {
            Some(cli) if cli.installed => {
                let version = cli.version.clone().unwrap_or_else(|| {
                    bisa_i18n::say(&bisa_core::text!("cli-gitsetup-unknown-version"))
                });
                match cli.active() {
                    Some(a) => out.say(&bisa_core::text!(
                        "cli-gitsetup-cli-installed-signed-in",
                        program = cli.program.label().to_string(),
                        version = version.clone(),
                        host = a.host.clone(),
                        login = a.login.clone()
                    )),
                    None => out.say(&bisa_core::text!(
                        "cli-gitsetup-cli-installed-not-signed-in",
                        program = cli.program.label().to_string(),
                        version = version.clone(),
                        host = health.host.clone()
                    )),
                }
            }
            Some(cli) => out.say(&bisa_core::text!(
                "cli-gitsetup-cli-not-installed",
                program = cli.program.label().to_string(),
                hint = cli.program.install_hints().brew.to_string()
            )),
            None => out.say(&bisa_core::text!(
                "cli-gitsetup-has-no-cli-own-api-with",
                a0 = (health.label).to_string()
            )),
        }
        for a in &health.accounts.accounts {
            let default = if health.default.as_deref() == Some(a.login.as_str()) {
                "  (default)"
            } else {
                ""
            };
            out.say(&bisa_core::text!(
                "cli-gitsetup-stored",
                a0 = (a.login).to_string(),
                a1 = format!("{:?}", a.source),
                default = default.to_string()
            ));
        }
        if !health.accounts.helpers.is_empty() {
            out.say(&bisa_core::text!(
                "cli-gitsetup-git-s-helper",
                a0 = (if health.accounts.helpers.len() == 1 {
                    ""
                } else {
                    "s"
                })
                .to_string(),
                a1 = (health.accounts.helpers.join(", ")).to_string(),
                a2 = (health
                    .accounts
                    .helper_username
                    .as_deref()
                    .map(|u| format!(" as {u}"))
                    .unwrap_or_default())
                .to_string()
            ));
        }
        match &health.resolves {
            Some(r) => out.say(&bisa_core::text!(
                "cli-gitsetup-resolves",
                a0 =
                    (r.login.as_deref().map(|l| format!("@{l}")).unwrap_or_else(
                        || bisa_i18n::say(&bisa_core::text!("cli-gitsetup-environment-s-token"))
                    ))
                    .to_string(),
                a1 = format!("{:?}", r.source)
            )),
            None => out.say(&bisa_core::text!(
                "cli-gitsetup-nothing-answers-bisa-git-login-host",
                a0 = (health.label).to_string(),
                a1 = (kind).to_string()
            )),
        }
        Ok(serde_json::to_value(&health)?)
    }
    .await;
    door.close().await;
    out.json_value(result?);
    Ok(())
}

async fn login_cmd(ctx: &Ctx, out: &Out, host: &str) -> Result<()> {
    let kind = kind_of(host)?;
    let door = Door::open(ctx).await?;
    let plan = door.login_plan(kind).await;
    door.close().await;
    let plan = plan?;
    match &plan {
        codehost::LoginPlan::Cli {
            program,
            host,
            words,
            ..
        } => {
            out.human(words);
            out.human(&format!(
                "run: {} {}",
                program.binary(),
                program.login_args(host).join(" ")
            ));
        }
        codehost::LoginPlan::Install {
            program,
            hints,
            words,
            token_page,
        } => {
            out.human(words);
            out.say(&bisa_core::text!(
                "cli-gitsetup-install",
                a0 = (hints.brew).to_string(),
                a1 = (hints.apt).to_string(),
                a2 = (hints.winget).to_string(),
                a3 = (hints.url).to_string()
            ));
            out.say(&bisa_core::text!(
                "cli-gitsetup-token-then-printf-s-token-bisa",
                token_page = token_page.to_string(),
                a0 = (program.kind()).to_string()
            ));
        }
        codehost::LoginPlan::Token { token_page, words } => {
            out.human(words);
            out.say(&bisa_core::text!(
                "cli-gitsetup-token-then-printf-s-token-bisa-2",
                token_page = token_page.to_string(),
                kind = kind.to_string()
            ));
        }
    }
    out.json_value(serde_json::to_value(&plan)?);
    Ok(())
}

async fn profile(ctx: &Ctx, out: &Out, cmd: ProfileCmd) -> Result<()> {
    let door = Door::open(ctx).await?;
    let result: Result<serde_json::Value> = async {
        match cmd {
            ProfileCmd::List => {
                let view = door.profiles().await?;
                if !view.hasconfig_supported {
                    out.say(&bisa_core::text!(
                        "cli-gitsetup-git-cannot-evaluate-includeif-hasconfig-profiles",
                        a0 = (view.git_version).to_string()
                    ));
                }
                if view.profiles.is_empty() {
                    out.say(&bisa_core::text!(
                        "cli-gitsetup-no-profiles-yet-bisa-git-profile"
                    ));
                }
                for p in &view.profiles {
                    out.human(&format!(
                        "{}  {}  {}/{}{}  {} <{}>{}{}",
                        p.profile.slug,
                        p.profile.label,
                        p.profile.host,
                        p.profile.owner,
                        if p.profile.aliases.is_empty() {
                            String::new()
                        } else {
                            bisa_i18n::say(&bisa_core::text!(
                                "cli-gitsetup-aliases",
                                a0 = (p.profile.aliases.join(", ")).to_string()
                            ))
                        },
                        p.profile.name,
                        p.profile.email,
                        p.profile
                            .ssh_key
                            .as_deref()
                            .map(|k| format!("  key {k}"))
                            .unwrap_or_default(),
                        p.profile
                            .account
                            .as_deref()
                            .map(|a| bisa_i18n::say(&bisa_core::text!(
                                "cli-gitsetup-account-2",
                                a = a.to_string()
                            )))
                            .unwrap_or_default(),
                    ));
                }
                for f in &view.foreign_includes {
                    out.say(&bisa_core::text!(
                        "cli-gitsetup-not-platform-s-includeif",
                        a0 = (f.condition).to_string(),
                        a1 = (f.path.display()).to_string()
                    ));
                }
                Ok(serde_json::to_value(&view)?)
            }
            ProfileCmd::Set {
                slug,
                label,
                host,
                owner,
                aliases,
                name,
                email,
                ssh_key,
                account,
            } => {
                let slug = Slug::new(&slug).map_err(|e| anyhow!("{e}"))?;
                let spec = ProfileSpec {
                    label,
                    host,
                    owner,
                    aliases,
                    name,
                    email,
                    ssh_key,
                    account,
                };
                let view = door.put_profile(slug, spec).await?;
                out.say(&bisa_core::text!(
                    "cli-gitsetup-profile-saved-commits-as",
                    a0 = (view.profile.slug).to_string(),
                    a1 = (view.profile.owner).to_string(),
                    a2 = (view.profile.name).to_string(),
                    a3 = (view.profile.email).to_string(),
                    a4 = (view.profile.host).to_string()
                ));
                Ok(serde_json::to_value(&view)?)
            }
            ProfileCmd::Rm { slug } => {
                let slug = Slug::new(&slug).map_err(|e| anyhow!("{e}"))?;
                door.remove_profile(&slug).await?;
                out.say(&bisa_core::text!(
                    "cli-gitsetup-profile-removed",
                    slug = slug.to_string()
                ));
                Ok(serde_json::json!({"removed": slug.to_string()}))
            }
        }
    }
    .await;
    door.close().await;
    out.json_value(result?);
    Ok(())
}

async fn ssh_cmd(ctx: &Ctx, out: &Out, cmd: SshCmd) -> Result<()> {
    let door = Door::open(ctx).await?;
    let result: Result<serde_json::Value> = async {
        match cmd {
            SshCmd::Keys => {
                let view = door.ssh_overview().await?;
                out.human(&format!("{}", view.dir.display()));
                if view.keys.is_empty() {
                    out.say(&bisa_core::text!("cli-gitsetup-no-key-pairs-with-pub-file"));
                }
                for k in &view.keys {
                    out.human(&format!(
                        "{}  {}  {}  {}{}",
                        k.key.name,
                        k.key.algorithm,
                        k.key.fingerprint,
                        if k.loaded {
                            bisa_i18n::say(&bisa_core::text!("cli-gitsetup-loaded"))
                        } else {
                            bisa_i18n::say(&bisa_core::text!("cli-gitsetup-not-loaded"))
                        },
                        if k.key.comment.is_empty() {
                            String::new()
                        } else {
                            format!("  ({})", k.key.comment)
                        }
                    ));
                }
                out.human(&match (&view.agent.available, &view.agent.reason) {
                    (true, _) => bisa_i18n::say(&bisa_core::text!(
                        "cli-gitsetup-ssh-agent-holds-key-s",
                        a0 = (view.agent.keys.len()).to_string()
                    )),
                    (false, Some(r)) => bisa_i18n::say(&bisa_core::text!(
                        "cli-gitsetup-ssh-agent-reason",
                        r = r.to_string()
                    )),
                    (false, None) => {
                        bisa_i18n::say(&bisa_core::text!("cli-gitsetup-ssh-agent-not-available"))
                    }
                });
                for h in &view.hosts {
                    out.say(&bisa_core::text!(
                        "cli-gitsetup-host-block",
                        patterns = h.patterns.join(" "),
                        hostname = h
                            .option("hostname")
                            .map(|n| format!(" → {n}"))
                            .unwrap_or_default()
                    ));
                }
                Ok(serde_json::to_value(&view)?)
            }
            SshCmd::Keygen { name, comment } => {
                let key = door.ssh_generate(ssh::NewKey { name, comment }).await?;
                out.human(&format!(
                    "{}  {}  {}",
                    key.name,
                    key.fingerprint,
                    key.path.display()
                ));
                out.say(&bisa_core::text!(
                    "cli-gitsetup-no-passphrase-was-set-add-one"
                ));
                Ok(serde_json::to_value(&key)?)
            }
            SshCmd::Load { name } => {
                door.ssh_load(&name).await?;
                out.say(&bisa_core::text!(
                    "cli-gitsetup-loaded-into-ssh-agent",
                    name = name.to_string()
                ));
                Ok(serde_json::json!({"loaded": name}))
            }
            SshCmd::Resolve { host, key } => {
                let r = door.ssh_resolve(&host, key).await?;
                out.human(&format!("{host} → {}@{}:{}", r.user, r.hostname, r.port));
                for f in &r.identity_files {
                    out.human(&format!("  identity {}", f.display()));
                }
                Ok(serde_json::to_value(&r)?)
            }
            SshCmd::Test { host, user, key } => {
                let g = door.ssh_test(&host, user, key).await?;
                out.human(&match &g {
                    ssh::HostGreeting::Authenticated { login: Some(l) } => {
                        bisa_i18n::say(&bisa_core::text!(
                            "cli-gitsetup-host-authenticated-as",
                            host = host.to_string(),
                            login = l.to_string()
                        ))
                    }
                    ssh::HostGreeting::Authenticated { login: None } => {
                        bisa_i18n::say(&bisa_core::text!(
                            "cli-gitsetup-host-authenticated",
                            host = host.to_string()
                        ))
                    }
                    ssh::HostGreeting::Refused { reason } => bisa_i18n::say(&bisa_core::text!(
                        "cli-gitsetup-host-refused",
                        host = host.to_string(),
                        reason = reason.to_string()
                    )),
                    ssh::HostGreeting::HostKeyUnknown { reason } => {
                        bisa_i18n::say(&bisa_core::text!(
                            "cli-gitsetup-host-key-unknown",
                            host = host.to_string(),
                            reason = reason.to_string()
                        ))
                    }
                    ssh::HostGreeting::Unreachable { reason } => bisa_i18n::say(&bisa_core::text!(
                        "cli-gitsetup-host-unreachable",
                        host = host.to_string(),
                        reason = reason.to_string()
                    )),
                });
                Ok(serde_json::to_value(&g)?)
            }
        }
    }
    .await;
    door.close().await;
    out.json_value(result?);
    Ok(())
}

async fn account(ctx: &Ctx, out: &Out, host: &str, cmd: AccountCmd) -> Result<()> {
    let kind = kind_of(host)?;
    // The token is read before the engine starts, so a refused stdin costs
    // nothing — and it is read from stdin, never taken as an argument a shell
    // history would keep.
    let token = match &cmd {
        AccountCmd::Add { .. } => {
            let mut line = String::new();
            std::io::stdin()
                .read_line(&mut line)
                .context(bisa_core::text!("cli-gitsetup-reading-token-from-stdin"))?;
            let token = line.trim().to_string();
            if token.is_empty() {
                return Err(anyhow!(bisa_core::text!(
                    "cli-gitsetup-no-token-stdin-printf-s-token"
                )));
            }
            Some(token)
        }
        _ => None,
    };
    let door = Door::open(ctx).await?;
    let result: Result<serde_json::Value> = async {
        match cmd {
            AccountCmd::List => {
                let view = door.accounts(kind).await?;
                if view.status.env_override {
                    out.say(&bisa_core::text!(
                        "cli-gitsetup-set-every-request-uses-whatever-checkout",
                        a0 = (kind.env_var()).to_string()
                    ));
                }
                if let Some(login) = &view.status.cli_login {
                    out.say(&bisa_core::text!(
                        "cli-gitsetup-signed-as",
                        a0 = (kind.cli().map(|c| c.label()).unwrap_or("CLI")).to_string(),
                        login = login.to_string()
                    ));
                }
                if view.status.accounts.is_empty() {
                    out.say(&bisa_core::text!(
                        "cli-gitsetup-no-accounts-stored-printf-s-token",
                        kind = kind.to_string()
                    ));
                }
                for a in &view.status.accounts {
                    let default = if view.default.as_deref() == Some(a.login.as_str()) {
                        "  (default)"
                    } else {
                        ""
                    };
                    out.human(&format!("@{}  {:?}{default}", a.login, a.source));
                }
                if !view.status.helpers.is_empty() {
                    out.say(&bisa_core::text!(
                        "cli-gitsetup-pushes-over-https-use-git-s",
                        a0 = (view.status.helpers.join(", ")).to_string(),
                        a1 = (view
                            .status
                            .helper_username
                            .as_deref()
                            .map(|u| format!(" as {u}"))
                            .unwrap_or_default())
                        .to_string()
                    ));
                }
                Ok(serde_json::to_value(&view)?)
            }
            AccountCmd::Add { login } => {
                let (login, connection) = door
                    .add_account(kind, token.as_deref().unwrap_or_default(), login.as_deref())
                    .await?;
                out.human(&format!(
                    "@{login} added — {}",
                    connection_words(&connection)
                ));
                Ok(serde_json::json!({"login": login, "connection": connection}))
            }
            AccountCmd::Rm { login } => {
                door.forget_account(kind, &login).await?;
                out.human(&format!("@{login} forgotten"));
                Ok(serde_json::json!({"forgotten": login}))
            }
            AccountCmd::Check { login } => {
                let connection = door.check_account(kind, &login).await?;
                out.human(&format!("@{login}: {}", connection_words(&connection)));
                Ok(serde_json::to_value(&connection)?)
            }
            AccountCmd::Default { login, clear } => {
                if login.is_none() && !clear {
                    return Err(anyhow!(bisa_core::text!(
                        "cli-gitsetup-name-login-pass-clear"
                    )));
                }
                let default = door
                    .set_default_account(kind, if clear { None } else { login.as_deref() })
                    .await?;
                out.human(&match &default {
                    Some(l) => bisa_i18n::say(&bisa_core::text!(
                        "cli-gitsetup-default-account",
                        l = l.to_string()
                    )),
                    None => bisa_i18n::say(&bisa_core::text!("cli-gitsetup-no-default-account")),
                });
                Ok(serde_json::json!({"default": default}))
            }
        }
    }
    .await;
    door.close().await;
    out.json_value(result?);
    Ok(())
}

fn connection_words(c: &codehost::Connection) -> String {
    match c {
        codehost::Connection::NoToken => bisa_i18n::say(&bisa_core::text!("cli-gitsetup-no-token")),
        codehost::Connection::Connected {
            login,
            scopes,
            missing,
            recommended_missing,
            organizations,
        } => {
            let mut s = bisa_i18n::say(&bisa_core::text!(
                "cli-gitsetup-connected-as",
                login = login.to_string()
            ));
            if !scopes.is_empty() {
                s.push_str(&format!(" · {}", scopes.join(", ")));
            }
            if !missing.is_empty() {
                s.push_str(&format!(" · missing {}", missing.join(", ")));
            }
            if !recommended_missing.is_empty() {
                s.push_str(&format!(
                    " · better with {}",
                    recommended_missing.join(", ")
                ));
            }
            if !organizations.is_empty() {
                s.push_str(&format!(" · organizations {}", organizations.join(", ")));
            }
            s
        }
        codehost::Connection::Refused { reason } => bisa_i18n::say(&bisa_core::text!(
            "cli-gitsetup-refused",
            reason = reason.to_string()
        )),
        codehost::Connection::Unreachable { reason } => bisa_i18n::say(&bisa_core::text!(
            "cli-gitsetup-unreachable",
            reason = reason.to_string()
        )),
    }
}

async fn connection_cmd(ctx: &Ctx, out: &Out, workstream: &str, check: bool) -> Result<()> {
    let wid = workstream.parse::<WorkstreamId>().map_err(|e| {
        anyhow!(bisa_core::text!(
            "cli-gitsetup-not-workstream-id",
            workstream = format!("{workstream:?}"),
            e = e.to_string()
        ))
    })?;
    let door = Door::open(ctx).await?;
    let result: Result<serde_json::Value> = async {
        let c = door.connection(wid).await?;
        match &c.remote {
            Some(r) => out.say(&bisa_core::text!(
                "cli-gitsetup-remote",
                a0 = (r.summary).to_string(),
                a1 = format!("{:?}", r.protocol)
            )),
            None => out.say(&bisa_core::text!("cli-gitsetup-remote-none")),
        }
        out.say(&bisa_core::text!(
            "cli-gitsetup-code-host",
            a0 = (c.code_host.as_deref().unwrap_or("none")).to_string()
        ));
        out.human(&format!(
            "profile  {}",
            c.profile
                .as_ref()
                .map(|p| format!("{} ({})", p.label, p.slug))
                .unwrap_or_else(|| bisa_i18n::say(&bisa_core::text!(
                    "cli-gitsetup-none-global-config-applies"
                )))
        ));
        out.say(&bisa_core::text!(
            "cli-gitsetup-author",
            a0 = (c.identity.name.as_deref().unwrap_or("?")).to_string(),
            a1 = (c.identity.email.as_deref().unwrap_or("?")).to_string(),
            a2 = format!("{:?}", c.identity.source),
            a3 = (c
                .identity
                .profile
                .as_deref()
                .map(|p| format!(", profile {p}"))
                .unwrap_or_default())
            .to_string()
        ));
        out.human(&match &c.transport {
            Transport::Ssh {
                key,
                loaded,
                ssh_configured,
                ..
            } => bisa_i18n::say(&bisa_core::text!(
                "cli-gitsetup-transport-ssh-key",
                a0 = (key
                    .as_ref()
                    .map(|k| k.display().to_string())
                    .unwrap_or_else(|| bisa_i18n::say(&bisa_core::text!(
                        "cli-gitsetup-none-ours"
                    ))))
                .to_string(),
                a1 = (match (ssh_configured, loaded) {
                    (false, _) =>
                        bisa_i18n::say(&bisa_core::text!("cli-gitsetup-ssh-not-configured-node")),
                    (true, Some(true)) =>
                        bisa_i18n::say(&bisa_core::text!("cli-gitsetup-loaded-ssh-agent")),
                    (true, Some(false)) =>
                        bisa_i18n::say(&bisa_core::text!("cli-gitsetup-not-loaded-ssh-agent")),
                    (true, None) =>
                        bisa_i18n::say(&bisa_core::text!("cli-gitsetup-ssh-agent-unknown")),
                })
                .to_string()
            )),
            Transport::Https {
                helpers,
                helper_username,
                ..
            } => bisa_i18n::say(&bisa_core::text!(
                "cli-gitsetup-transport-https-helpers",
                a0 = (if helpers.is_empty() {
                    "none".to_string()
                } else {
                    helpers.join(", ")
                })
                .to_string(),
                a1 = (helper_username
                    .as_deref()
                    .map(|u| format!(" as {u}"))
                    .unwrap_or_default())
                .to_string()
            )),
            Transport::Local => bisa_i18n::say(&bisa_core::text!("cli-gitsetup-transport-local")),
            Transport::None => bisa_i18n::say(&bisa_core::text!("cli-gitsetup-transport-none")),
        });
        out.say(&bisa_core::text!(
            "cli-gitsetup-account",
            a0 = (c
                .account
                .login
                .as_deref()
                .map(|l| format!("@{l}"))
                .unwrap_or_else(|| "none".into()))
            .to_string(),
            a1 = format!("{:?}", c.account.source)
        ));
        for caution in &c.cautions {
            out.human(&format!("caution  {}", caution.sentence));
        }
        let mut value = serde_json::to_value(&c)?;
        if check {
            let probes = door.connection_check(wid).await?;
            if let Some(ch) = &probes.code_host {
                out.say(&bisa_core::text!(
                    "cli-gitsetup-check-code-host",
                    a0 = (connection_words(ch)).to_string()
                ));
            }
            if let Some(a) = &probes.access {
                out.say(&bisa_core::text!(
                    "cli-gitsetup-check-repository",
                    a0 = (if a.found {
                        bisa_i18n::say(&bisa_core::text!("cli-gitsetup-found"))
                    } else {
                        bisa_i18n::say(&bisa_core::text!("cli-gitsetup-not-found"))
                    })
                    .to_string(),
                    a1 = (if a.push { ", can push" } else { "" }).to_string()
                ));
            }
            if let Some(g) = &probes.ssh {
                out.say(&bisa_core::text!(
                    "cli-gitsetup-check-ssh",
                    g = format!("{g:?}")
                ));
            }
            if let Some(ls) = &probes.ls_remote {
                out.say(&bisa_core::text!(
                    "cli-gitsetup-check-ls-remote",
                    a0 = (if ls.ok {
                        format!("{} head(s)", ls.heads)
                    } else {
                        ls.detail.clone().unwrap_or_default()
                    })
                    .to_string()
                ));
            }
            value["check"] = serde_json::to_value(&probes)?;
        }
        Ok(value)
    }
    .await;
    door.close().await;
    out.json_value(result?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a person types is one segment of the route, whatever it holds:
    /// a slash, a space, a question mark or an ampersand never makes
    /// another route or another key of the query.
    #[test]
    fn a_name_is_one_segment_of_the_route_whatever_it_holds() {
        assert_eq!(segment("id_ed25519-acme.v2~"), "id_ed25519-acme.v2~");
        assert_eq!(segment("a b/c?d&e=f"), "a%20b%2Fc%3Fd%26e%3Df");
        assert_eq!(segment("clé"), "cl%C3%A9");
        assert_eq!(
            ssh_load_path("../config"),
            "/git/ssh/keys/..%2Fconfig/load",
            "a name that climbs is a name, and the node refuses it as one"
        );
    }

    #[test]
    fn a_host_is_resolved_with_the_key_forced_only_when_one_is_given() {
        assert_eq!(
            ssh_resolve_path("github-acme", None),
            "/git/ssh/resolve?host=github-acme"
        );
        assert_eq!(
            ssh_resolve_path(
                "git@host&key=/other",
                Some(std::path::Path::new("/keys/id acme"))
            ),
            "/git/ssh/resolve?host=git%40host%26key%3D%2Fother&key=%2Fkeys%2Fid%20acme",
            "what the host says is never read as the key"
        );
    }
}
