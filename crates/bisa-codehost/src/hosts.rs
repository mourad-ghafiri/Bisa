//! The three code hosts assembled: for each kind, its API over a token store
//! rooted at `identity/codehost/<kind>/`, layered under its CLI when the kind
//! has one and a runner was attached. The engine builds one [`Hosts`] with
//! its git handle and its CLI runner, takes the public registry, and keeps
//! the same [`Hosts`] as the registry's factory for self-hosted instances.

use crate::cli::gh::GhCli;
use crate::cli::glab::GlabCli;
use crate::cli::{CliProgram, CliRunner, CodeHostCli};
use crate::creds::{GitCredentials, TokenStore};
use crate::layered::Layered;
use crate::{
    bitbucket::BitbucketApi, github::GitHubApi, gitlab::GitLabApi, CodeHost, CodeHostFactory,
    CodeHostKind, CodeHostRegistry,
};
use bisa_http::Clients;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct Hosts {
    tokens_root: PathBuf,
    /// The keyring when `BISA_KEYSTORE=keyring` (`false` pins the file store, for tests).
    keystore_from_env: bool,
    git: Option<Arc<dyn GitCredentials>>,
    cli: Option<Arc<dyn CliRunner>>,
    /// The clients every host's API goes through.
    http: Arc<Clients>,
}

impl std::fmt::Debug for Hosts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hosts")
            .field("tokens_root", &self.tokens_root)
            .field("git", &self.git.is_some())
            .field("cli", &self.cli.is_some())
            .field("http", &self.http)
            .finish()
    }
}

impl Hosts {
    /// Token stores under `tokens_root/<kind>/`, the keyring by the
    /// environment's choice.
    pub fn new(tokens_root: impl Into<PathBuf>) -> Self {
        Self {
            tokens_root: tokens_root.into(),
            keystore_from_env: true,
            git: None,
            cli: None,
            http: Clients::shared(),
        }
    }

    /// File stores only, whatever the environment says — for tests.
    pub fn file_only(tokens_root: impl Into<PathBuf>) -> Self {
        Self {
            tokens_root: tokens_root.into(),
            keystore_from_env: false,
            git: None,
            cli: None,
            http: Clients::shared(),
        }
    }

    /// The clients every host's API goes through — the engine's, built from
    /// the `network.*` settings and swapped when they change.
    pub fn with_http(mut self, http: Arc<Clients>) -> Self {
        self.http = http;
        self
    }

    pub fn with_git(mut self, git: Arc<dyn GitCredentials>) -> Self {
        self.git = Some(git);
        self
    }

    pub fn with_cli(mut self, cli: Arc<dyn CliRunner>) -> Self {
        self.cli = Some(cli);
        self
    }

    /// The CLI for a kind at a host, when the kind has one and a runner is attached.
    pub fn cli_for(&self, kind: CodeHostKind, host: &str) -> Option<Arc<dyn CodeHostCli>> {
        let runner = self.cli.clone()?;
        Some(match kind.cli()? {
            CliProgram::Gh => Arc::new(GhCli::new(runner, host)),
            CliProgram::Glab => Arc::new(GlabCli::new(runner, host)),
        })
    }

    /// The token store for a kind at a host: the kind's directory, git's
    /// helpers, and the CLI as a token source.
    pub fn store_for(&self, kind: CodeHostKind, host: &str) -> TokenStore {
        let dir = self.tokens_root.join(kind.as_str());
        let mut store = if self.keystore_from_env {
            TokenStore::new(kind, host, dir)
        } else {
            TokenStore::file_only(kind, host, dir)
        };
        if let Some(git) = &self.git {
            store = store.with_git(Arc::clone(git));
        }
        if let Some(runner) = &self.cli {
            match kind.cli() {
                Some(CliProgram::Gh) => {
                    store = store.with_cli(Arc::new(GhCli::new(Arc::clone(runner), host)))
                }
                Some(CliProgram::Glab) => {
                    store = store.with_cli(Arc::new(GlabCli::new(Arc::clone(runner), host)))
                }
                None => {}
            }
        }
        store
    }

    /// One layered code host of `kind` at `host`.
    pub fn host(&self, kind: CodeHostKind, host: &str) -> Arc<Layered> {
        let tokens = self.store_for(kind, host);
        let api: Arc<dyn CodeHost> = match kind {
            CodeHostKind::GitHub => {
                Arc::new(GitHubApi::at_host(host, tokens).with_http(Arc::clone(&self.http)))
            }
            CodeHostKind::GitLab => {
                Arc::new(GitLabApi::at_host(host, tokens).with_http(Arc::clone(&self.http)))
            }
            CodeHostKind::Bitbucket => {
                Arc::new(BitbucketApi::new(tokens).with_http(Arc::clone(&self.http)))
            }
        };
        Arc::new(Layered::new(api, self.cli_for(kind, host)))
    }

    /// The three public hosts.
    pub fn public(&self) -> Vec<Arc<dyn CodeHost>> {
        CodeHostKind::ALL
            .into_iter()
            .map(|kind| self.host(kind, kind.public_host()) as Arc<dyn CodeHost>)
            .collect()
    }

    /// The registry: the public hosts, and this as the factory for the rest.
    pub fn registry(self: Arc<Self>) -> CodeHostRegistry {
        CodeHostRegistry::new(self.public()).with_factory(self)
    }
}

impl CodeHostFactory for Hosts {
    fn at_host(&self, kind: CodeHostKind, host: &str) -> Option<Arc<dyn CodeHost>> {
        // Bitbucket Server is another API, not this one at another address.
        if kind == CodeHostKind::Bitbucket && !host.eq_ignore_ascii_case(kind.public_host()) {
            return None;
        }
        Some(self.host(kind, host))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::fake::FakeCli;
    use crate::RemoteUrl;

    #[test]
    fn the_registry_holds_the_three_public_hosts_and_makes_a_self_hosted_one() {
        let dir = tempfile::tempdir().unwrap();
        let hosts = Arc::new(Hosts::file_only(dir.path()).with_cli(Arc::new(FakeCli::new())));
        let registry = Arc::clone(&hosts).registry();
        assert_eq!(
            registry.ids().iter().map(|i| i.0).collect::<Vec<_>>(),
            ["github", "gitlab", "bitbucket"]
        );
        let (host, repo) = registry
            .detect(&RemoteUrl::parse("git@gitlab.com:acme/platform/web.git"))
            .unwrap();
        assert_eq!(
            (host.id().0, repo.slug().as_str()),
            ("gitlab", "acme/platform/web")
        );
        assert!(registry
            .detect(&RemoteUrl::parse("git@git.acme.internal:acme/web.git"))
            .is_none());
        let (own, repo) = registry
            .detect_as(
                &RemoteUrl::parse("git@git.acme.internal:acme/web.git"),
                Some(CodeHostKind::GitLab),
            )
            .expect("codehost.kind names the instance");
        assert_eq!(
            (own.id().0, repo.host.as_str()),
            ("gitlab", "git.acme.internal")
        );
        assert!(
            registry
                .detect_as(
                    &RemoteUrl::parse("git@bb.acme.internal:acme/web.git"),
                    Some(CodeHostKind::Bitbucket)
                )
                .is_none(),
            "Bitbucket Server is not Bitbucket Cloud"
        );
        assert!(registry
            .detect_as(
                &RemoteUrl::parse("git@git.acme.internal:acme/web.git"),
                None
            )
            .is_none());
        assert!(hosts.cli_for(CodeHostKind::GitHub, "github.com").is_some());
        assert!(hosts
            .cli_for(CodeHostKind::Bitbucket, "bitbucket.org")
            .is_none());
        assert!(
            Hosts::file_only(dir.path())
                .cli_for(CodeHostKind::GitHub, "github.com")
                .is_none(),
            "no runner, no CLI"
        );
        assert_eq!(
            hosts.store_for(CodeHostKind::GitLab, "gitlab.com").kind(),
            CodeHostKind::GitLab
        );
        assert!(hosts
            .host(CodeHostKind::GitHub, "github.com")
            .cli()
            .is_some());
        assert!(hosts
            .host(CodeHostKind::Bitbucket, "bitbucket.org")
            .cli()
            .is_none());
    }
}
