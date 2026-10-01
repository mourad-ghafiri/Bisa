//! One code host from two layers: the machine's CLI first, the host's API
//! second (ide/08). Every operation asks the CLI; a [`CliStep::Fallback`]
//! falls through to the API, a [`CliStep::Host`] is the host's own refusal
//! and comes back as is — the API would only repeat it, and for a creation it
//! could repeat the *act*. `id`, `capabilities` and `detect` are the API
//! layer's: the shape of a host does not change with the program that
//! reaches it. Binding an account binds both layers.

use crate::cli::{CliProbe, CliStep, CodeHostCli};
use crate::{
    Account, CheckRun, CodeHost, CodeHostCapabilities, CodeHostId, CodeHostResult, MergeOutcome,
    MergeStrategy, PrCreate, PrFilter, PrReviews, PullRequest, RemoteUrl, RepoAccess, RepoRef,
    Review,
};
use std::sync::Arc;

pub struct Layered {
    cli: Option<Arc<dyn CodeHostCli>>,
    api: Arc<dyn CodeHost>,
}

impl Layered {
    pub fn new(api: Arc<dyn CodeHost>, cli: Option<Arc<dyn CodeHostCli>>) -> Self {
        Self { cli, api }
    }

    /// A kind with no CLI, or a build that leaves the CLI out.
    pub fn api_only(api: Arc<dyn CodeHost>) -> Self {
        Self { cli: None, api }
    }

    /// The CLI layer, when there is one — for a probe.
    pub fn cli(&self) -> Option<&Arc<dyn CodeHostCli>> {
        self.cli.as_ref()
    }

    /// The API layer alone.
    pub fn api(&self) -> &Arc<dyn CodeHost> {
        &self.api
    }

    /// Installed, version, accounts — `None` for a kind with no CLI.
    pub async fn probe(&self) -> Option<CliProbe> {
        match &self.cli {
            Some(cli) => Some(cli.probe().await),
            None => None,
        }
    }
}

/// Ask the CLI, and fall through to the API only when the CLI had no say.
macro_rules! first_cli {
    ($self:ident, $what:literal, $cli:ident => $call:expr, $api:expr) => {{
        if let Some($cli) = &$self.cli {
            match $call.await {
                Ok(v) => return Ok(v),
                Err(CliStep::Host(e)) => return Err(e),
                Err(CliStep::Fallback(why)) => {
                    tracing::debug!(what = $what, %why, "the CLI had no say; asking the API");
                }
            }
        }
        $api.await
    }};
}

#[async_trait::async_trait]
impl CodeHost for Layered {
    fn id(&self) -> CodeHostId {
        self.api.id()
    }

    fn capabilities(&self) -> CodeHostCapabilities {
        self.api.capabilities()
    }

    fn detect(&self, remote: &RemoteUrl) -> Option<RepoRef> {
        self.api.detect(remote)
    }

    fn for_account(self: Arc<Self>, login: Option<&str>) -> Arc<dyn CodeHost> {
        Arc::new(Layered {
            cli: self.cli.clone().map(|c| c.for_account(login)),
            api: Arc::clone(&self.api).for_account(login),
        })
    }

    async fn account(&self) -> CodeHostResult<Account> {
        first_cli!(self, "account", cli => cli.account(), self.api.account())
    }

    /// A token is the API's to check: the CLI keeps its own.
    async fn verify_token(&self, token: &str, login: Option<&str>) -> CodeHostResult<Account> {
        self.api.verify_token(token, login).await
    }

    async fn repo_access(&self, repo: &RepoRef) -> CodeHostResult<RepoAccess> {
        first_cli!(self, "repo_access", cli => cli.repo_access(repo), self.api.repo_access(repo))
    }

    async fn create_pr(&self, repo: &RepoRef, req: PrCreate) -> CodeHostResult<PullRequest> {
        first_cli!(self, "create_pr", cli => cli.create_pr(repo, req.clone()), self.api.create_pr(repo, req))
    }

    async fn get_pr(&self, repo: &RepoRef, number: u64) -> CodeHostResult<PullRequest> {
        first_cli!(self, "get_pr", cli => cli.get_pr(repo, number), self.api.get_pr(repo, number))
    }

    async fn list_prs(&self, repo: &RepoRef, filter: PrFilter) -> CodeHostResult<Vec<PullRequest>> {
        first_cli!(self, "list_prs", cli => cli.list_prs(repo, filter.clone()), self.api.list_prs(repo, filter))
    }

    async fn checks(&self, repo: &RepoRef, number: u64) -> CodeHostResult<Vec<CheckRun>> {
        first_cli!(self, "checks", cli => cli.checks(repo, number), self.api.checks(repo, number))
    }

    async fn submit_review(
        &self,
        repo: &RepoRef,
        number: u64,
        review: Review,
    ) -> CodeHostResult<()> {
        first_cli!(self, "submit_review", cli => cli.submit_review(repo, number, review.clone()), self.api.submit_review(repo, number, review))
    }

    async fn pr_reviews(&self, repo: &RepoRef, number: u64) -> CodeHostResult<PrReviews> {
        first_cli!(self, "pr_reviews", cli => cli.pr_reviews(repo, number), self.api.pr_reviews(repo, number))
    }

    async fn resolve_review_thread(&self, thread_id: &str, resolved: bool) -> CodeHostResult<()> {
        first_cli!(self, "resolve_review_thread", cli => cli.resolve_review_thread(thread_id, resolved), self.api.resolve_review_thread(thread_id, resolved))
    }

    async fn reply_review_thread(&self, thread_id: &str, body: &str) -> CodeHostResult<()> {
        first_cli!(self, "reply_review_thread", cli => cli.reply_review_thread(thread_id, body), self.api.reply_review_thread(thread_id, body))
    }

    async fn merge(
        &self,
        repo: &RepoRef,
        number: u64,
        strategy: MergeStrategy,
    ) -> CodeHostResult<MergeOutcome> {
        first_cli!(self, "merge", cli => cli.merge(repo, number, strategy), self.api.merge(repo, number, strategy))
    }

    async fn delete_branch(&self, repo: &RepoRef, branch: &str) -> CodeHostResult<()> {
        first_cli!(self, "delete_branch", cli => cli.delete_branch(repo, branch), self.api.delete_branch(repo, branch))
    }
}
