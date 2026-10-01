//! Bitbucket Cloud over its REST API (2.0) — the one layer of the Bitbucket
//! code host, since Bitbucket Cloud has no CLI of its own. Bitbucket's words
//! map onto the crate's: a pull request's *id* is the number, the *commit
//! statuses* on its head are the check runs, its *participants* who approved
//! or requested changes are the reviews, its inline *comments* are the review
//! threads (Bitbucket resolves a comment, so a thread is one comment and its
//! replies; the id here is `workspace/slug#pr#comment`).
//!
//! Bitbucket's API tokens and app passwords are **Basic** credentials — a
//! login and a secret — so a token here is checked and used beside the login
//! it was stored under, and `verify_token` needs the login the person gives.
//! The base URL is a field ([`BitbucketApi::with_base_url`]) so the whole
//! implementation is tested against a loopback stub.

use crate::{
    creds, Account, CheckRun, CodeHost, CodeHostCapabilities, CodeHostError, CodeHostId,
    CodeHostKind, CodeHostResult, MergeOutcome, MergeStrategy, PrCreate, PrFilter, PrReviews,
    PrState, PullRequest, RemoteUrl, RepoAccess, RepoRef, Review, ReviewEvent, ReviewSummary,
    ReviewThread, ReviewThreadComment,
};
use bisa_http::Clients;
use reqwest::StatusCode;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

pub const HOST: &str = "bitbucket.org";
pub const API: &str = "https://api.bitbucket.org/2.0";

/// Bitbucket lists no scopes on a `GET /user`; the token page says what a
/// token may do, so nothing is required or recommended here.
pub const REQUIRED_SCOPES: &[&str] = &[];
pub const RECOMMENDED_SCOPES: &[&str] = &[];

#[derive(Clone)]
pub struct BitbucketApi {
    /// The clients the engine built from the `network.*` settings; the
    /// process's shared set until `with_http` hands one in.
    http: Arc<Clients>,
    tokens: creds::TokenStore,
    host: String,
    api: String,
    account: Option<String>,
}

struct Answer {
    status: StatusCode,
    text: String,
}

/// How long one request may take, connect and body included.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) fn capabilities() -> CodeHostCapabilities {
    CodeHostCapabilities {
        draft_prs: true,
        reviewers: false,
        labels: false,
        merge_strategies: vec![MergeStrategy::Merge, MergeStrategy::Squash],
        check_runs: true,
        review_comments: true,
        review_threads: true,
        review_thread_replies: true,
        delete_branch: true,
        review_events: vec![
            ReviewEvent::Approve,
            ReviewEvent::RequestChanges,
            ReviewEvent::Comment,
        ],
    }
}

pub(crate) fn thread_id(repo: &RepoRef, pr: u64, comment: u64) -> String {
    format!("{}#{pr}#{comment}", repo.slug())
}

pub(crate) fn parse_thread_id(id: &str) -> Option<(String, u64, u64)> {
    let mut parts = id.rsplitn(3, '#');
    let comment = parts.next()?.parse().ok()?;
    let pr = parts.next()?.parse().ok()?;
    let slug = parts.next()?.to_string();
    (!slug.is_empty()).then_some((slug, pr, comment))
}

/// A Basic credential out of a token and the login it belongs to: the bound
/// account, else the login given, else a `login:secret` token.
pub(crate) fn basic_parts(token: &str, login: Option<&str>) -> Option<(String, String)> {
    let token = token.trim();
    if let Some(login) = login.map(str::trim).filter(|l| !l.is_empty()) {
        return Some((login.to_string(), token.to_string()));
    }
    let (user, secret) = token.split_once(':')?;
    (!user.is_empty() && !secret.is_empty()).then(|| (user.to_string(), secret.to_string()))
}

#[derive(Deserialize)]
struct BbUser {
    #[serde(default)]
    nickname: Option<String>,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
}

impl BbUser {
    fn name(&self) -> Option<String> {
        self.nickname
            .clone()
            .or_else(|| self.username.clone())
            .or_else(|| self.display_name.clone())
    }
}

#[derive(Deserialize)]
struct BbBranch {
    name: String,
}
#[derive(Deserialize)]
struct BbCommit {
    hash: String,
}
#[derive(Deserialize)]
struct BbEnd {
    branch: BbBranch,
    #[serde(default)]
    commit: Option<BbCommit>,
}
#[derive(Deserialize)]
struct BbLink {
    href: String,
}
#[derive(Deserialize, Default)]
struct BbLinks {
    #[serde(default)]
    html: Option<BbLink>,
}
#[derive(Deserialize)]
struct BbParticipant {
    user: BbUser,
    #[serde(default)]
    approved: bool,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    participated_on: Option<String>,
}
#[derive(Deserialize)]
struct BbPull {
    id: u64,
    title: String,
    state: String,
    #[serde(default)]
    draft: bool,
    source: BbEnd,
    destination: BbEnd,
    #[serde(default)]
    author: Option<BbUser>,
    #[serde(default)]
    links: BbLinks,
    #[serde(default)]
    participants: Vec<BbParticipant>,
    #[serde(default)]
    merge_commit: Option<BbCommit>,
}

impl From<BbPull> for PullRequest {
    fn from(p: BbPull) -> Self {
        PullRequest {
            number: p.id,
            url: p.links.html.map(|l| l.href).unwrap_or_default(),
            title: p.title,
            state: match p.state.as_str() {
                "OPEN" => PrState::Open,
                "MERGED" => PrState::Merged,
                "DECLINED" | "SUPERSEDED" => PrState::Closed,
                _ => PrState::Unknown,
            },
            is_draft: p.draft,
            // Bitbucket says nothing about mergeability until the merge is
            // asked for; it refuses then, and the refusal is relayed.
            mergeable: Some(true),
            head: p.source.branch.name,
            head_sha: p.source.commit.map(|c| c.hash).unwrap_or_default(),
            base: p.destination.branch.name,
            author: p.author.and_then(|a| a.name()),
        }
    }
}

/// One page of a paged answer — the first 50 or 100 are read, never more.
#[derive(Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
struct BbPage<T> {
    #[serde(default = "Vec::new")]
    values: Vec<T>,
}

#[derive(Deserialize)]
struct BbStatus {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    key: Option<String>,
    state: String,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

impl From<BbStatus> for CheckRun {
    fn from(s: BbStatus) -> Self {
        let (status, conclusion) = match s.state.as_str() {
            "INPROGRESS" => ("in_progress", None),
            "SUCCESSFUL" => ("completed", Some("success")),
            "FAILED" => ("completed", Some("failure")),
            "STOPPED" => ("completed", Some("cancelled")),
            _ => ("queued", None),
        };
        CheckRun {
            name: s.name.or(s.key).unwrap_or_else(|| "status".into()),
            status: status.into(),
            conclusion: conclusion.map(str::to_string),
            url: s.url,
            summary: s.description.filter(|d| !d.is_empty()),
        }
    }
}

#[derive(Deserialize)]
struct BbContent {
    #[serde(default)]
    raw: String,
}
#[derive(Deserialize)]
struct BbInline {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    to: Option<u32>,
    #[serde(default)]
    from: Option<u32>,
}
#[derive(Deserialize)]
struct BbParent {
    id: u64,
}
#[derive(Deserialize)]
struct BbComment {
    id: u64,
    #[serde(default)]
    content: Option<BbContent>,
    #[serde(default)]
    user: Option<BbUser>,
    #[serde(default)]
    created_on: Option<String>,
    #[serde(default)]
    inline: Option<BbInline>,
    #[serde(default)]
    resolution: Option<serde_json::Value>,
    #[serde(default)]
    deleted: bool,
    #[serde(default)]
    parent: Option<BbParent>,
}

/// Reviews from the participants — an approval, a request for changes — and
/// threads from the inline comments: each top-level inline comment is a
/// thread, its replies the comments, resolved when Bitbucket says so.
fn reviews_of(repo: &RepoRef, pr: &BbPull, comments: Vec<BbComment>) -> PrReviews {
    let reviews = pr
        .participants
        .iter()
        .filter_map(|p| {
            let state = match (p.approved, p.state.as_deref()) {
                (true, _) | (_, Some("approved")) => "approved",
                (_, Some("changes_requested")) => "changes_requested",
                _ => return None,
            };
            Some(ReviewSummary {
                author: p.user.name(),
                state: state.into(),
                body: String::new(),
                submitted_at: p.participated_on.clone(),
            })
        })
        .collect();
    let live: Vec<&BbComment> = comments.iter().filter(|c| !c.deleted).collect();
    let threads = live
        .iter()
        .filter(|c| c.parent.is_none() && c.inline.is_some())
        .map(|top| {
            let inline = top.inline.as_ref();
            let mut all: Vec<&BbComment> = vec![top];
            all.extend(
                live.iter()
                    .filter(|c| c.parent.as_ref().is_some_and(|p| p.id == top.id)),
            );
            ReviewThread {
                id: thread_id(repo, pr.id, top.id),
                path: inline.and_then(|i| i.path.clone()),
                line: inline.and_then(|i| i.to.or(i.from)),
                is_resolved: top.resolution.is_some(),
                is_outdated: false,
                comments: all
                    .into_iter()
                    .map(|c| ReviewThreadComment {
                        author: c.user.as_ref().and_then(|u| u.name()),
                        body: c
                            .content
                            .as_ref()
                            .map(|b| b.raw.clone())
                            .unwrap_or_default(),
                        created_at: c.created_on.clone(),
                    })
                    .collect(),
            }
        })
        .collect();
    PrReviews { reviews, threads }
}

fn status_error(answer: &Answer, what: &str) -> CodeHostError {
    let status = answer.status;
    let message = serde_json::from_str::<serde_json::Value>(&answer.text)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(str::to_string))
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| status.to_string());
    match status.as_u16() {
        401 | 403 => CodeHostError::NotAuthenticated(format!("{what}: {message}")),
        404 => CodeHostError::NotFound(format!("{what}: {message}")),
        400 | 409 | 422 | 555 => CodeHostError::Refused(format!("{what}: {message}")),
        429 => CodeHostError::Transport(format!(
            "{what}: Bitbucket's rate limit is reached — try again in a minute"
        )),
        _ => CodeHostError::Transport(format!("{what}: {status} {message}")),
    }
}

impl BitbucketApi {
    pub fn new(tokens: creds::TokenStore) -> Self {
        Self::with_base_url(API, tokens)
    }

    pub fn with_base_url(api: impl Into<String>, tokens: creds::TokenStore) -> Self {
        Self {
            http: Clients::shared(),
            host: tokens.host().to_string(),
            tokens,
            api: api.into().trim_end_matches('/').to_string(),
            account: None,
        }
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn bound_account(&self) -> Option<&str> {
        self.account.as_deref()
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.api)
    }

    /// The Basic credential the chain yields: the token, beside the login it
    /// was stored under — the bound account, the one stored login, or a
    /// `login:secret` token from the environment.
    async fn credential(&self) -> CodeHostResult<(String, creds::Secret)> {
        let (token, _) = creds::token(&self.tokens, self.account.as_deref()).await?;
        let login = match &self.account {
            Some(l) => Some(l.clone()),
            None => match self.tokens.logins().as_slice() {
                [one] => Some(one.clone()),
                _ => None,
            },
        };
        match basic_parts(token.expose(), login.as_deref()) {
            Some((user, secret)) => Ok((user, creds::Secret::new(secret))),
            None => Err(CodeHostError::NotAuthenticated(format!(
                "a Bitbucket token needs the login it belongs to: add the account in {} with its login, or set {} to `login:token`",
                creds::settings_place(CodeHostKind::Bitbucket),
                CodeHostKind::Bitbucket.env_var()
            ))),
        }
    }

    /// The client for the internet, as the engine's policy has it now.
    fn http(&self) -> Arc<reqwest::Client> {
        self.http.outbound()
    }

    /// The same host over the engine's clients — the proxy and the HTTP
    /// version the person chose, swapped under every request they make.
    pub fn with_http(mut self, http: Arc<Clients>) -> Self {
        self.http = http;
        self
    }

    async fn request(
        &self,
        req: reqwest::RequestBuilder,
        user: &str,
        secret: &creds::Secret,
        what: &str,
    ) -> CodeHostResult<Answer> {
        let resp = req
            .timeout(REQUEST_TIMEOUT)
            .basic_auth(user, Some(secret.expose()))
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| CodeHostError::Transport(format!("{what}: {e}")))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| CodeHostError::Transport(format!("{what}: {e}")))?;
        Ok(Answer { status, text })
    }

    async fn send<T: serde::de::DeserializeOwned>(
        &self,
        req: reqwest::RequestBuilder,
        what: &str,
    ) -> CodeHostResult<T> {
        let (user, secret) = self.credential().await?;
        let answer = self.request(req, &user, &secret, what).await?;
        if !answer.status.is_success() {
            return Err(status_error(&answer, what));
        }
        if answer.text.trim().is_empty() {
            return serde_json::from_str("null")
                .map_err(|e| CodeHostError::Transport(format!("{what}: {e}")));
        }
        serde_json::from_str(&answer.text).map_err(|e| {
            CodeHostError::Transport(format!(
                "{what}: Bitbucket answered something unexpected: {e}"
            ))
        })
    }

    async fn whoami(&self, user: &str, secret: &creds::Secret) -> CodeHostResult<Account> {
        let what = "check the token";
        let answer = self
            .request(self.http().get(self.url("/user")), user, secret, what)
            .await?;
        if !answer.status.is_success() {
            return Err(status_error(&answer, what));
        }
        let me: BbUser = serde_json::from_str(&answer.text).map_err(|e| {
            CodeHostError::Transport(format!(
                "{what}: Bitbucket answered something unexpected: {e}"
            ))
        })?;
        let workspaces = match self
            .request(
                self.http()
                    .get(self.url("/user/permissions/workspaces"))
                    .query(&[("pagelen", "100")]),
                user,
                secret,
                "read workspaces",
            )
            .await
        {
            Ok(a) if a.status.is_success() => {
                serde_json::from_str::<BbPage<serde_json::Value>>(&a.text)
                    .map(|p| {
                        p.values
                            .into_iter()
                            .filter_map(|v| v["workspace"]["slug"].as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default()
            }
            _ => Vec::new(),
        };
        Ok(Account {
            login: me.name().unwrap_or_else(|| user.to_string()),
            scopes: Vec::new(),
            organizations: workspaces,
            name: me.display_name.filter(|n| !n.trim().is_empty()),
            email: None,
            id: None,
        })
    }

    fn pr_path(repo: &RepoRef, number: u64) -> String {
        format!(
            "/repositories/{}/{}/pullrequests/{number}",
            repo.owner, repo.name
        )
    }

    async fn pull(&self, repo: &RepoRef, number: u64) -> CodeHostResult<BbPull> {
        self.send(
            self.http().get(self.url(&Self::pr_path(repo, number))),
            "read pull request",
        )
        .await
    }
}

#[async_trait::async_trait]
impl CodeHost for BitbucketApi {
    fn id(&self) -> CodeHostId {
        CodeHostKind::Bitbucket.id()
    }

    fn capabilities(&self) -> CodeHostCapabilities {
        capabilities()
    }

    fn detect(&self, remote: &RemoteUrl) -> Option<RepoRef> {
        remote
            .is_on(&self.host)
            .then(|| remote.repo_ref(&self.host))
            .flatten()
    }

    fn for_account(self: Arc<Self>, login: Option<&str>) -> Arc<dyn CodeHost> {
        let wanted = login
            .map(|l| l.trim().to_ascii_lowercase())
            .filter(|l| !l.is_empty());
        if wanted == self.account {
            return self;
        }
        Arc::new(Self {
            account: wanted,
            ..(*self).clone()
        })
    }

    async fn account(&self) -> CodeHostResult<Account> {
        let (user, secret) = self.credential().await?;
        self.whoami(&user, &secret).await
    }

    async fn verify_token(&self, token: &str, login: Option<&str>) -> CodeHostResult<Account> {
        let (user, secret) = basic_parts(token, login).ok_or_else(|| {
            CodeHostError::NotAuthenticated(
                "a Bitbucket token is checked beside the login it belongs to — say which".into(),
            )
        })?;
        if secret.is_empty() {
            return Err(CodeHostError::NotAuthenticated("an empty token".into()));
        }
        self.whoami(&user, &creds::Secret::new(secret)).await
    }

    /// `GET /repositories/{ws}/{slug}`: found; then the account's permission on
    /// it, best effort — `write` or `admin` may push.
    async fn repo_access(&self, repo: &RepoRef) -> CodeHostResult<RepoAccess> {
        let what = "read repository access";
        let (user, secret) = self.credential().await?;
        let url = self.url(&format!("/repositories/{}/{}", repo.owner, repo.name));
        let answer = self
            .request(self.http().get(&url), &user, &secret, what)
            .await?;
        if answer.status.as_u16() == 404 {
            return Ok(RepoAccess {
                found: false,
                push: false,
            });
        }
        if !answer.status.is_success() {
            return Err(status_error(&answer, what));
        }
        let q = format!("repository.full_name=\"{}\"", repo.slug());
        let push = match self
            .request(
                self.http()
                    .get(self.url("/user/permissions/repositories"))
                    .query(&[("q", q.as_str())]),
                &user,
                &secret,
                "read permission",
            )
            .await
        {
            Ok(a) if a.status.is_success() => {
                serde_json::from_str::<BbPage<serde_json::Value>>(&a.text)
                    .ok()
                    .and_then(|p| {
                        p.values.first().and_then(|v| {
                            v["permission"]
                                .as_str()
                                .map(|s| s == "write" || s == "admin")
                        })
                    })
                    .unwrap_or(false)
            }
            _ => false,
        };
        Ok(RepoAccess { found: true, push })
    }

    async fn create_pr(&self, repo: &RepoRef, req: PrCreate) -> CodeHostResult<PullRequest> {
        let url = self.url(&format!(
            "/repositories/{}/{}/pullrequests",
            repo.owner, repo.name
        ));
        let body = serde_json::json!({
            "title": req.title,
            "description": req.body,
            "draft": req.draft,
            "source": { "branch": { "name": req.head } },
            "destination": { "branch": { "name": req.base } },
        });
        let pr: BbPull = self
            .send(self.http().post(&url).json(&body), "create pull request")
            .await?;
        Ok(pr.into())
    }

    async fn get_pr(&self, repo: &RepoRef, number: u64) -> CodeHostResult<PullRequest> {
        Ok(self.pull(repo, number).await?.into())
    }

    async fn list_prs(&self, repo: &RepoRef, filter: PrFilter) -> CodeHostResult<Vec<PullRequest>> {
        let url = self.url(&format!(
            "/repositories/{}/{}/pullrequests",
            repo.owner, repo.name
        ));
        let mut req = self.http().get(&url).query(&[("pagelen", "50")]);
        req = match filter.state {
            Some(PrState::Open) | None => req.query(&[("state", "OPEN")]),
            Some(PrState::Merged) => req.query(&[("state", "MERGED")]),
            Some(PrState::Closed) => req.query(&[("state", "DECLINED")]),
            Some(PrState::Unknown) => req.query(&[
                ("state", "OPEN"),
                ("state", "MERGED"),
                ("state", "DECLINED"),
            ]),
        };
        if let Some(head) = &filter.head {
            req = req.query(&[("q", format!("source.branch.name=\"{head}\""))]);
        }
        let page: BbPage<BbPull> = self.send(req, "list pull requests").await?;
        Ok(page.values.into_iter().map(PullRequest::from).collect())
    }

    /// The commit statuses on the pull request's head — one check run each.
    async fn checks(&self, repo: &RepoRef, number: u64) -> CodeHostResult<Vec<CheckRun>> {
        let pr = self.pull(repo, number).await?;
        let Some(sha) = pr.source.commit.map(|c| c.hash) else {
            return Ok(Vec::new());
        };
        let url = self.url(&format!(
            "/repositories/{}/{}/commit/{sha}/statuses",
            repo.owner, repo.name
        ));
        let page: BbPage<BbStatus> = self
            .send(
                self.http().get(&url).query(&[("pagelen", "100")]),
                "read commit statuses",
            )
            .await?;
        Ok(page.values.into_iter().map(CheckRun::from).collect())
    }

    async fn submit_review(
        &self,
        repo: &RepoRef,
        number: u64,
        review: Review,
    ) -> CodeHostResult<()> {
        if review.needs_words() {
            return Err(CodeHostError::Refused(
                "submit review: a comment or a change request needs words — say what you noticed"
                    .into(),
            ));
        }
        let base = Self::pr_path(repo, number);
        match review.event {
            ReviewEvent::Approve => {
                let _: serde_json::Value = self
                    .send(
                        self.http().post(self.url(&format!("{base}/approve"))),
                        "approve pull request",
                    )
                    .await?;
            }
            ReviewEvent::RequestChanges => {
                let _: serde_json::Value = self
                    .send(
                        self.http()
                            .post(self.url(&format!("{base}/request-changes"))),
                        "request changes on pull request",
                    )
                    .await?;
            }
            ReviewEvent::Comment => {}
        }
        if let Some(words) = review.words() {
            let _: serde_json::Value = self
                .send(
                    self.http()
                        .post(self.url(&format!("{base}/comments")))
                        .json(&serde_json::json!({"content": {"raw": words}})),
                    "comment on pull request",
                )
                .await?;
        }
        for c in &review.comments {
            let inline = match c.side {
                crate::Side::Right => serde_json::json!({"path": c.path, "to": c.line}),
                crate::Side::Left => serde_json::json!({"path": c.path, "from": c.line}),
            };
            let _: serde_json::Value = self
                .send(
                    self.http()
                        .post(self.url(&format!("{base}/comments")))
                        .json(&serde_json::json!({"content": {"raw": c.body}, "inline": inline})),
                    "comment on the diff",
                )
                .await?;
        }
        Ok(())
    }

    async fn pr_reviews(&self, repo: &RepoRef, number: u64) -> CodeHostResult<PrReviews> {
        let pr = self.pull(repo, number).await?;
        let url = self.url(&format!("{}/comments", Self::pr_path(repo, number)));
        let page: BbPage<BbComment> = self
            .send(
                self.http().get(&url).query(&[("pagelen", "100")]),
                "read comments",
            )
            .await?;
        Ok(reviews_of(repo, &pr, page.values))
    }

    async fn resolve_review_thread(&self, thread_id: &str, resolved: bool) -> CodeHostResult<()> {
        let (slug, pr, comment) = parse_thread_id(thread_id)
            .ok_or_else(|| CodeHostError::NotFound(format!("review thread {thread_id}")))?;
        let url = self.url(&format!(
            "/repositories/{slug}/pullrequests/{pr}/comments/{comment}/resolve"
        ));
        let req = if resolved {
            self.http().post(&url)
        } else {
            self.http().delete(&url)
        };
        let _: serde_json::Value = self.send(req, "resolve review thread").await?;
        Ok(())
    }

    /// A comment whose parent is the thread's root comment — the one the id names.
    async fn reply_review_thread(&self, thread_id: &str, body: &str) -> CodeHostResult<()> {
        let (slug, pr, comment) = parse_thread_id(thread_id)
            .ok_or_else(|| CodeHostError::NotFound(format!("review thread {thread_id}")))?;
        let url = self.url(&format!("/repositories/{slug}/pullrequests/{pr}/comments"));
        let _: serde_json::Value = self
            .send(
                self.http().post(&url).json(
                    &serde_json::json!({"content": {"raw": body}, "parent": {"id": comment}}),
                ),
                "reply on review thread",
            )
            .await?;
        Ok(())
    }

    async fn merge(
        &self,
        repo: &RepoRef,
        number: u64,
        strategy: MergeStrategy,
    ) -> CodeHostResult<MergeOutcome> {
        let how = match strategy {
            MergeStrategy::Merge => "merge_commit",
            MergeStrategy::Squash => "squash",
            MergeStrategy::Rebase => {
                return Err(CodeHostError::Unsupported(
                    "a rebase merge on Bitbucket".into(),
                ))
            }
        };
        let url = self.url(&format!("{}/merge", Self::pr_path(repo, number)));
        let pr: BbPull = self
            .send(
                self.http().post(&url).json(
                    &serde_json::json!({"merge_strategy": how, "close_source_branch": false}),
                ),
                "merge pull request",
            )
            .await?;
        let merged = pr.state == "MERGED";
        Ok(MergeOutcome {
            merged,
            sha: pr.merge_commit.map(|c| c.hash),
            message: if merged {
                "merged".into()
            } else {
                format!(
                    "pull request #{number} is {}",
                    pr.state.to_ascii_lowercase()
                )
            },
            remote_branch_deleted: None,
        })
    }

    async fn delete_branch(&self, repo: &RepoRef, branch: &str) -> CodeHostResult<()> {
        let what = "delete branch";
        let (user, secret) = self.credential().await?;
        let url = self.url(&format!(
            "/repositories/{}/{}/refs/branches/{branch}",
            repo.owner, repo.name
        ));
        let answer = self
            .request(self.http().delete(&url), &user, &secret, what)
            .await?;
        if answer.status.is_success() {
            return Ok(());
        }
        Err(status_error(&answer, what))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> RepoRef {
        RepoRef {
            host: HOST.into(),
            owner: "acme".into(),
            name: "web".into(),
        }
    }

    #[test]
    fn a_basic_credential_is_the_login_beside_the_token() {
        assert_eq!(
            basic_parts("app-pass", Some("ada")),
            Some(("ada".into(), "app-pass".into()))
        );
        assert_eq!(
            basic_parts("ada:app-pass", None),
            Some(("ada".into(), "app-pass".into()))
        );
        assert_eq!(
            basic_parts("app-pass", None),
            None,
            "no login, no credential"
        );
        assert_eq!(basic_parts(":x", None), None);
        let id = thread_id(&repo(), 4, 99);
        assert_eq!(id, "acme/web#4#99");
        assert_eq!(parse_thread_id(&id), Some(("acme/web".into(), 4, 99)));
        assert_eq!(parse_thread_id("acme/web#4#not"), None);
    }

    #[test]
    fn a_pull_request_its_statuses_and_its_comments_read_into_the_crates_words() {
        let pr: BbPull = serde_json::from_str(
            r#"{"id":4,"title":"t","state":"OPEN","draft":false,"source":{"branch":{"name":"work/x"},"commit":{"hash":"abc"}},"destination":{"branch":{"name":"main"}},"author":{"nickname":"ada"},"links":{"html":{"href":"https://bitbucket.org/acme/web/pull-requests/4"}},"participants":[{"user":{"nickname":"alice"},"approved":true,"state":"approved","participated_on":"2026-01-01T00:00:00Z"},{"user":{"nickname":"bob"},"approved":false,"state":"changes_requested"},{"user":{"nickname":"carol"},"approved":false,"state":null}]}"#,
        )
        .unwrap();
        let comments: Vec<BbComment> = serde_json::from_str(
            r#"[{"id":1,"content":{"raw":"rename this"},"user":{"nickname":"alice"},"created_on":"2026-01-01T00:00:00Z","inline":{"path":"a.rs","to":12},"resolution":null},{"id":2,"content":{"raw":"done"},"user":{"nickname":"ada"},"parent":{"id":1}},{"id":3,"content":{"raw":"general"},"user":{"nickname":"bob"}},{"id":4,"content":{"raw":"gone"},"deleted":true,"inline":{"path":"b.rs","to":1}}]"#,
        )
        .unwrap();
        let out = reviews_of(&repo(), &pr, comments);
        assert_eq!(
            out.reviews.len(),
            2,
            "an approval and a request for changes; a mere participant is not a review"
        );
        assert_eq!(
            (
                out.reviews[0].author.as_deref(),
                out.reviews[0].state.as_str()
            ),
            (Some("alice"), "approved")
        );
        assert_eq!(out.reviews[1].state, "changes_requested");
        assert_eq!(
            out.threads.len(),
            1,
            "one inline top-level comment that is not deleted"
        );
        let t = &out.threads[0];
        assert_eq!(
            (
                t.id.as_str(),
                t.path.as_deref(),
                t.line,
                t.is_resolved,
                t.comments.len()
            ),
            ("acme/web#4#1", Some("a.rs"), Some(12), false, 2)
        );
        let pull: PullRequest = pr.into();
        assert_eq!(
            (
                pull.number,
                pull.state,
                pull.head_sha.as_str(),
                pull.author.as_deref(),
                pull.mergeable
            ),
            (4, PrState::Open, "abc", Some("ada"), Some(true))
        );
        assert_eq!(pull.url, "https://bitbucket.org/acme/web/pull-requests/4");
        let status: CheckRun = serde_json::from_str::<BbStatus>(
            r#"{"key":"ci","state":"FAILED","url":"u","description":"2 tests"}"#,
        )
        .unwrap()
        .into();
        assert_eq!(
            (
                status.name.as_str(),
                status.status.as_str(),
                status.conclusion.as_deref()
            ),
            ("ci", "completed", Some("failure"))
        );
        let running: CheckRun =
            serde_json::from_str::<BbStatus>(r#"{"name":"build","state":"INPROGRESS"}"#)
                .unwrap()
                .into();
        assert_eq!(
            (running.status.as_str(), running.conclusion),
            ("in_progress", None)
        );
    }

    #[test]
    fn the_base_url_and_the_capabilities() {
        let dir = tempfile::tempdir().unwrap();
        let api = BitbucketApi::with_base_url(
            "http://127.0.0.1:1/",
            creds::TokenStore::file_only(CodeHostKind::Bitbucket, HOST, dir.path()),
        );
        assert_eq!(api.url("/user"), "http://127.0.0.1:1/user");
        assert!(api
            .detect(&RemoteUrl::parse("git@bitbucket.org:acme/web.git"))
            .is_some());
        assert!(!capabilities().reviewers && capabilities().review_threads);
        assert_eq!(capabilities().review_events.len(), 3);
    }
}
