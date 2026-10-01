//! A code host that lives in memory, for tests — and the one whose capabilities
//! can all be off, which is what proves the desktop's form is rendered from
//! `capabilities()` and not from a GitHub-shaped assumption.

use crate::{
    Account, CheckRun, CodeHost, CodeHostCapabilities, CodeHostError, CodeHostId, CodeHostKind,
    CodeHostResult, MergeOutcome, MergeStrategy, PrCreate, PrFilter, PrReviews, PrState,
    PullRequest, RemoteProtocol, RemoteUrl, RepoAccess, RepoRef, Review, ReviewEvent,
    ReviewSummary, ReviewThread, ReviewThreadComment,
};
use std::sync::{Arc, Mutex};

/// The login the fake's credential belongs to unless a test says otherwise —
/// the author of every pull request it creates, and the reviewer of every
/// review it takes, exactly as one token on a real code host is both.
pub const DEFAULT_LOGIN: &str = "you";

/// The fake's memory — behind an `Arc`, so a fake rebound to another account
/// through [`CodeHost::for_account`] shares what the test inspects.
#[derive(Default)]
pub struct FakeState {
    prs: Mutex<Vec<PullRequest>>,
    pub reviews: Mutex<Vec<(u64, Review)>>,
    pub checks: Mutex<Vec<CheckRun>>,
    /// Review threads a test seeds; `resolve_review_thread` flips their state
    /// and `reply_review_thread` appends a comment as the fake's login.
    pub threads: Mutex<Vec<ReviewThread>>,
    /// Branches a caller asked this fake to delete on the remote, in order.
    pub deleted_branches: Mutex<Vec<String>>,
    /// The logins that answered a request, in order — which account the
    /// engine bound.
    pub asked_as: Mutex<Vec<String>>,
    /// The organizations each login can see, seeded by a test.
    pub organizations: Mutex<Vec<(String, Vec<String>)>>,
    /// What `repo_access` answers, seeded by a test; `None` is *found, push*.
    pub access: Mutex<Option<RepoAccess>>,
    /// Why a merge is refused while a test says so — checks failing, a
    /// conflict with the base, a review still owed; `None` merges.
    pub merge_refusal: Mutex<Option<String>>,
}

pub struct FakeCodeHost {
    pub caps: CodeHostCapabilities,
    pub host: String,
    /// Whose credential this is: the author of a pull request the fake
    /// creates and the reviewer of a review it takes.
    pub login: String,
    /// The kind this fake stands in for — its `id()`; `None` answers `fake`.
    pub kind: Option<CodeHostKind>,
    /// Who the account is, as the host would say — for the committer
    /// suggestion a test reads back.
    pub name: Option<String>,
    pub email: Option<String>,
    pub user_id: Option<u64>,
    pub state: Arc<FakeState>,
}

impl Default for FakeCodeHost {
    fn default() -> Self {
        Self {
            caps: CodeHostCapabilities::default(),
            host: String::new(),
            login: DEFAULT_LOGIN.to_string(),
            kind: None,
            name: None,
            email: None,
            user_id: None,
            state: Arc::new(FakeState::default()),
        }
    }
}

impl FakeCodeHost {
    /// Every capability off: the narrowest code host there is.
    pub fn minimal(host: &str) -> Self {
        Self {
            host: host.to_string(),
            ..Self::default()
        }
    }

    /// A fake that answers as one of the real kinds — so an engine test can
    /// see `github` on a record without reaching GitHub.
    pub fn of_kind(kind: CodeHostKind, host: &str) -> Self {
        Self {
            host: host.to_string(),
            kind: Some(kind),
            ..Self::default()
        }
    }

    pub fn with_capabilities(host: &str, caps: CodeHostCapabilities) -> Self {
        Self {
            caps,
            host: host.to_string(),
            ..Self::default()
        }
    }

    /// The same fake, signed in as `login` — so a test can open a pull request
    /// as one account and review it as another.
    pub fn signed_in_as(mut self, login: &str) -> Self {
        self.login = login.to_string();
        self
    }

    /// The person behind the account, as the host's user record would say.
    pub fn known_as(mut self, name: &str, email: Option<&str>, user_id: Option<u64>) -> Self {
        self.name = Some(name.to_string());
        self.email = email.map(str::to_string);
        self.user_id = user_id;
        self
    }

    /// Seed the organizations `login` can see.
    pub fn with_organizations(self, login: &str, organizations: &[&str]) -> Self {
        self.state
            .organizations
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((
                login.to_string(),
                organizations.iter().map(|o| o.to_string()).collect(),
            ));
        self
    }

    /// Seed a pull request authored by somebody else, for a review that is
    /// not the author's own.
    pub fn seed_pr(&self, repo: &RepoRef, author: &str, head: &str, base: &str) -> PullRequest {
        let mut prs = self.state.prs.lock().unwrap_or_else(|e| e.into_inner());
        let number = prs.len() as u64 + 1;
        let pr = PullRequest {
            number,
            url: format!("https://{}/{}/pull/{number}", self.host, repo.slug()),
            title: format!("{head} into {base}"),
            state: PrState::Open,
            is_draft: false,
            mergeable: Some(true),
            head: head.to_string(),
            head_sha: "0".repeat(40),
            base: base.to_string(),
            author: Some(author.to_string()),
        };
        prs.push(pr.clone());
        pr
    }

    /// Move a seeded pull request to `state` — closed on the code host, say —
    /// so a test can watch the engine refuse what is no longer open.
    pub fn set_pr_state(&self, number: u64, state: PrState) {
        let mut prs = self.state.prs.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(pr) = prs.iter_mut().find(|p| p.number == number) {
            pr.state = state;
        }
    }

    /// Refuse every merge with `why`, as a code host does while a pull
    /// request cannot be merged yet; `None` lets merges through again.
    pub fn refuse_merges(&self, why: Option<&str>) {
        if let Ok(mut refusal) = self.state.merge_refusal.lock() {
            *refusal = why.map(str::to_string);
        }
    }

    /// The logins that answered requests so far — what a test asserts the
    /// engine bound.
    pub fn asked_as(&self) -> Vec<String> {
        self.state
            .asked_as
            .lock()
            .map(|a| a.clone())
            .unwrap_or_default()
    }

    fn note_asked(&self) {
        if let Ok(mut a) = self.state.asked_as.lock() {
            a.push(self.login.clone());
        }
    }
}

#[async_trait::async_trait]
impl CodeHost for FakeCodeHost {
    fn id(&self) -> CodeHostId {
        self.kind
            .map(CodeHostKind::id)
            .unwrap_or(CodeHostId("fake"))
    }
    fn capabilities(&self) -> CodeHostCapabilities {
        self.caps.clone()
    }
    fn detect(&self, remote: &RemoteUrl) -> Option<RepoRef> {
        // A URL on this fake's host, or — so an engine test can push to a real
        // bare `origin` on disk and still reach a code host — a local path whose
        // last component names the host.
        if remote.is_on(&self.host) {
            return remote.repo_ref(&self.host);
        }
        if remote.protocol != RemoteProtocol::Local {
            return None;
        }
        let name = remote
            .path
            .trim_end_matches('/')
            .rsplit(['/', '\\'])
            .next()?;
        let name = name.strip_suffix(".git").unwrap_or(name);
        (name == self.host).then(|| RepoRef {
            host: self.host.clone(),
            owner: "local".into(),
            name: name.to_string(),
        })
    }
    fn for_account(self: Arc<Self>, login: Option<&str>) -> Arc<dyn CodeHost> {
        match login {
            // Another login is another person: the host would know them
            // by their own record, which a test seeds through `known_as`
            // on the fake it binds — not this one's.
            Some(l) if !l.eq_ignore_ascii_case(&self.login) => Arc::new(Self {
                caps: self.caps.clone(),
                host: self.host.clone(),
                login: l.to_ascii_lowercase(),
                kind: self.kind,
                name: None,
                email: None,
                user_id: None,
                state: Arc::clone(&self.state),
            }),
            _ => self,
        }
    }
    async fn account(&self) -> CodeHostResult<Account> {
        self.note_asked();
        let organizations = self
            .state
            .organizations
            .lock()
            .map(|o| {
                o.iter()
                    .find(|(l, _)| l.eq_ignore_ascii_case(&self.login))
                    .map(|(_, orgs)| orgs.clone())
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        Ok(Account {
            login: self.login.clone(),
            scopes: vec![],
            organizations,
            name: self.name.clone(),
            email: self.email.clone(),
            id: self.user_id,
        })
    }
    async fn verify_token(&self, token: &str, _login: Option<&str>) -> CodeHostResult<Account> {
        if token.trim().is_empty() {
            return Err(CodeHostError::NotAuthenticated("an empty token".into()));
        }
        self.account().await
    }
    async fn repo_access(&self, _repo: &RepoRef) -> CodeHostResult<RepoAccess> {
        self.note_asked();
        Ok(self
            .state
            .access
            .lock()
            .map(|a| *a)
            .unwrap_or_default()
            .unwrap_or(RepoAccess {
                found: true,
                push: true,
            }))
    }
    async fn create_pr(&self, repo: &RepoRef, req: PrCreate) -> CodeHostResult<PullRequest> {
        self.note_asked();
        if req.draft && !self.caps.draft_prs {
            return Err(CodeHostError::Unsupported("draft pull requests".into()));
        }
        let mut prs = self
            .state
            .prs
            .lock()
            .map_err(|_| CodeHostError::Transport("lock".into()))?;
        let number = prs.len() as u64 + 1;
        let pr = PullRequest {
            number,
            url: format!("https://{}/{}/pull/{number}", self.host, repo.slug()),
            title: req.title,
            state: PrState::Open,
            is_draft: req.draft,
            mergeable: Some(true),
            head: req.head,
            head_sha: "0".repeat(40),
            base: req.base,
            author: Some(self.login.clone()),
        };
        prs.push(pr.clone());
        Ok(pr)
    }
    async fn get_pr(&self, _repo: &RepoRef, number: u64) -> CodeHostResult<PullRequest> {
        self.note_asked();
        let prs = self
            .state
            .prs
            .lock()
            .map_err(|_| CodeHostError::Transport("lock".into()))?;
        prs.iter()
            .find(|p| p.number == number)
            .cloned()
            .ok_or_else(|| CodeHostError::NotFound(format!("pull request #{number}")))
    }
    async fn list_prs(
        &self,
        _repo: &RepoRef,
        filter: PrFilter,
    ) -> CodeHostResult<Vec<PullRequest>> {
        let prs = self
            .state
            .prs
            .lock()
            .map_err(|_| CodeHostError::Transport("lock".into()))?;
        Ok(prs
            .iter()
            .filter(|p| filter.state.is_none_or(|s| s == p.state))
            .filter(|p| filter.head.as_deref().is_none_or(|h| h == p.head))
            .cloned()
            .collect())
    }
    async fn checks(&self, _repo: &RepoRef, _number: u64) -> CodeHostResult<Vec<CheckRun>> {
        if !self.caps.check_runs {
            return Err(CodeHostError::Unsupported("check runs".into()));
        }
        Ok(self
            .state
            .checks
            .lock()
            .map(|c| c.clone())
            .unwrap_or_default())
    }
    /// The rules every real code host holds, held here too so the engine and
    /// the node see them in tests: a comment or a change request needs words,
    /// and the pull request's own author may comment but not approve or
    /// request changes — GitHub's sentences, word for word.
    async fn submit_review(
        &self,
        _repo: &RepoRef,
        number: u64,
        review: Review,
    ) -> CodeHostResult<()> {
        if !review.comments.is_empty() && !self.caps.review_comments {
            return Err(CodeHostError::Unsupported("review comments".into()));
        }
        let pr = self.get_pr(_repo, number).await?;
        if review.needs_words() {
            return Err(CodeHostError::Refused(
                "submit review: Review body is required".into(),
            ));
        }
        if review.is_verdict()
            && pr
                .author
                .as_deref()
                .is_some_and(|a| a.eq_ignore_ascii_case(&self.login))
        {
            let what = match review.event {
                ReviewEvent::Approve => "approve",
                _ => "request changes on",
            };
            return Err(CodeHostError::Refused(format!(
                "submit review: Can not {what} your own pull request"
            )));
        }
        if let Ok(mut r) = self.state.reviews.lock() {
            r.push((number, review));
        }
        Ok(())
    }
    async fn pr_reviews(&self, repo: &RepoRef, number: u64) -> CodeHostResult<PrReviews> {
        if !self.caps.review_threads {
            return Ok(PrReviews::default());
        }
        self.get_pr(repo, number).await?;
        let reviews = self
            .state
            .reviews
            .lock()
            .map(|r| {
                r.iter()
                    .filter(|(n, _)| *n == number)
                    .map(|(_, rev)| ReviewSummary {
                        author: Some(self.login.clone()),
                        state: match rev.event {
                            ReviewEvent::Approve => "approved",
                            ReviewEvent::RequestChanges => "changes_requested",
                            ReviewEvent::Comment => "commented",
                        }
                        .into(),
                        body: rev.words().unwrap_or_default().to_string(),
                        submitted_at: None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let threads = self
            .state
            .threads
            .lock()
            .map(|t| t.clone())
            .unwrap_or_default();
        Ok(PrReviews { reviews, threads })
    }
    async fn resolve_review_thread(&self, thread_id: &str, resolved: bool) -> CodeHostResult<()> {
        if !self.caps.review_threads {
            return Err(CodeHostError::Unsupported(
                "resolving review threads".into(),
            ));
        }
        let mut threads = self
            .state
            .threads
            .lock()
            .map_err(|_| CodeHostError::Transport("lock".into()))?;
        let thread = threads
            .iter_mut()
            .find(|t| t.id == thread_id)
            .ok_or_else(|| CodeHostError::NotFound(format!("review thread {thread_id}")))?;
        thread.is_resolved = resolved;
        Ok(())
    }
    async fn reply_review_thread(&self, thread_id: &str, body: &str) -> CodeHostResult<()> {
        if !self.caps.review_thread_replies {
            return Err(CodeHostError::Unsupported(
                "replying on review threads".into(),
            ));
        }
        let mut threads = self
            .state
            .threads
            .lock()
            .map_err(|_| CodeHostError::Transport("lock".into()))?;
        let thread = threads
            .iter_mut()
            .find(|t| t.id == thread_id)
            .ok_or_else(|| CodeHostError::NotFound(format!("review thread {thread_id}")))?;
        // A thread the fake's own table does not hold was never asked of a host.
        self.note_asked();
        thread.comments.push(ReviewThreadComment {
            author: Some(self.login.clone()),
            body: body.to_string(),
            created_at: None,
        });
        Ok(())
    }
    async fn merge(
        &self,
        _repo: &RepoRef,
        number: u64,
        strategy: MergeStrategy,
    ) -> CodeHostResult<MergeOutcome> {
        self.note_asked();
        if !self.caps.merge_strategies.contains(&strategy) {
            return Err(CodeHostError::Unsupported(format!(
                "merge strategy {strategy:?}"
            )));
        }
        let mut prs = self
            .state
            .prs
            .lock()
            .map_err(|_| CodeHostError::Transport("lock".into()))?;
        let pr = prs
            .iter_mut()
            .find(|p| p.number == number)
            .ok_or_else(|| CodeHostError::NotFound(format!("pull request #{number}")))?;
        if pr.state != PrState::Open {
            return Err(CodeHostError::Refused(format!(
                "pull request #{number} is not open"
            )));
        }
        if let Some(why) = self.state.merge_refusal.lock().ok().and_then(|r| r.clone()) {
            return Err(CodeHostError::Refused(why));
        }
        pr.state = PrState::Merged;
        Ok(MergeOutcome {
            merged: true,
            sha: Some("1".repeat(40)),
            message: "merged".into(),
            remote_branch_deleted: None,
        })
    }
    async fn delete_branch(&self, _repo: &RepoRef, branch: &str) -> CodeHostResult<()> {
        if !self.caps.delete_branch {
            return Err(CodeHostError::Unsupported(
                "deleting a branch on the remote".into(),
            ));
        }
        self.state
            .deleted_branches
            .lock()
            .map_err(|_| CodeHostError::Transport("lock".into()))?
            .push(branch.to_string());
        Ok(())
    }
}
