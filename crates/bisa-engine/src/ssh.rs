//! SSH for git hosts, through the engine (Settings › Git & code hosts › SSH keys,
//! ide/04): the keys with whether ssh-agent holds them, the `Host` blocks of
//! `ssh_config` for git hosts, a new key pair, a key loaded into ssh-agent,
//! what `ssh -G` would offer a host, and the one handshake a git host greets.
//! Every call is `bisa-ssh`'s over the engine's configured `Ssh` — `None`
//! for an engine nobody configured, which answers `SshUnavailable` and spawns
//! nothing. Public material only; a private key is never opened anywhere.

use crate::events::{EnginePayload, GitSetup};
use crate::{EngineError, Inner};
use std::path::{Path, PathBuf};

pub use bisa_ssh::{
    host_block_text, AgentKey, AgentState, Cli, HostBlock, HostGreeting, KeyRow, NewKey, PublicKey,
    Resolved, Ssh, SshError, SshOverview, KNOWN_GIT_HOSTS,
};

/// The SSH a node that serves a person runs with: the real programs over
/// `~/.ssh`, `~` being this process's home. `None` on a machine with no home
/// directory — the routes then answer `SshUnavailable`. Tests never call this;
/// they hand in a `FakeSsh` on a temp dir.
pub fn default_ssh() -> Option<Ssh> {
    let home = dirs::home_dir()?;
    Some(Ssh::new(
        std::sync::Arc::new(Cli::default()),
        home.join(".ssh"),
        home,
    ))
}

fn ssh(inner: &Inner) -> Result<Ssh, EngineError> {
    inner
        .ssh
        .clone()
        .ok_or_else(|| EngineError::SshUnavailable("this engine was started without SSH".into()))
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, SshError> + Send + 'static,
) -> Result<T, EngineError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| {
            EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-ssh-task",
                e = e.to_string()
            ))
        })?
        .map_err(EngineError::from)
}

/// Keys, ssh-agent and the git hosts' `Host` blocks in one read.
pub async fn overview(inner: &Inner) -> Result<SshOverview, EngineError> {
    let ssh = ssh(inner)?;
    blocking(move || ssh.overview()).await
}

/// Generate an ed25519 key pair in the SSH directory — no passphrase from
/// here (one on argv would be visible to every process); the panel says so.
pub async fn generate(inner: &Inner, key: NewKey) -> Result<PublicKey, EngineError> {
    let ssh = ssh(inner)?;
    let public = blocking(move || ssh.generate(&key)).await?;
    inner.emit(crate::events::EngineEvent::global(
        EnginePayload::GitSetupChanged {
            what: GitSetup::Keys,
        },
    ));
    Ok(public)
}

/// Load one key into ssh-agent.
pub async fn load(inner: &Inner, name: String) -> Result<(), EngineError> {
    let ssh = ssh(inner)?;
    blocking(move || ssh.agent_add(&name)).await?;
    inner.emit(crate::events::EngineEvent::global(
        EnginePayload::GitSetupChanged {
            what: GitSetup::Keys,
        },
    ));
    Ok(())
}

/// What ssh would do for `host` — offline. With `identity`, the key a
/// profile's `core.sshCommand` names.
pub async fn resolve(
    inner: &Inner,
    host: String,
    identity: Option<PathBuf>,
) -> Result<Resolved, EngineError> {
    let ssh = ssh(inner)?;
    blocking(move || ssh.resolve(&host, identity.as_deref())).await
}

/// The one handshake — `ssh -T user@host` in batch mode — read into a
/// greeting. Nothing is written on either side.
pub async fn test(
    inner: &Inner,
    host: String,
    user: String,
    identity: Option<PathBuf>,
) -> Result<HostGreeting, EngineError> {
    let ssh = ssh(inner)?;
    blocking(move || ssh.test(&host, &user, identity.as_deref())).await
}

/// The SSH directory this engine reads, when it has one.
pub fn dir(inner: &Inner) -> Option<&Path> {
    inner.ssh.as_ref().map(|s| s.dir())
}
