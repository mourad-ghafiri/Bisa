//! GitHub over its API — the second layer of the GitHub code host, behind
//! `gh` ([`crate::cli::gh::GhCli`]): REST for pull requests, checks, reviews,
//! the merge and the branch, and GraphQL for review threads, which REST cannot
//! resolve. The token may be the one the CLI or git's own credential helper
//! already holds ([`crate::creds`]), asked, never read.
//!
//! Every request carries the token from the chain, resolved per call so a
//! token stored from Settings takes effect without a restart; the token never
//! appears in an error, a log or a returned value. The base URL is a field
//! ([`GitHubApi::with_base_url`]) so the whole implementation is tested against
//! a stub on the loopback interface and never against GitHub; a GitHub
//! Enterprise instance is the same implementation at `https://<host>/api/v3`
//! ([`GitHubApi::at_host`]).

use crate::{
    creds, Account, CheckRun, CodeHost, CodeHostCapabilities, CodeHostError, CodeHostId,
    CodeHostKind, CodeHostResult, MergeOutcome, MergeStrategy, PrCreate, PrFilter, PrReviews,
    PrState, PullRequest, RemoteUrl, RepoAccess, RepoRef, Review, ReviewEvent, ReviewSummary,
    ReviewThread, ReviewThreadComment, Side,
};
use bisa_http::Clients;
use reqwest::header::HeaderMap;
use reqwest::StatusCode;
use serde::Deserialize;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const HOST: &str = "github.com";
pub const API: &str = "https://api.github.com";

/// The scopes a token needs for everything this crate does: `repo` for pull
/// requests, reviews and the merge; `workflow` to read check runs from Actions.
pub const REQUIRED_SCOPES: &[&str] = &["repo", "workflow"];

/// The scopes a token is better with: `read:org`, so the account can say
/// which organizations it belongs to and the Repository view can warn when a
/// remote's owner is not among them.
pub const RECOMMENDED_SCOPES: &[&str] = &["read:org"];

#[derive(Clone)]
pub struct GitHubApi {
    /// The clients the engine built from the `network.*` settings; the
    /// process's shared set until `with_http` hands one in.
    http: Arc<Clients>,
    tokens: creds::TokenStore,
    host: String,
    api: String,
    /// The login whose token every request carries — `None` for the chain's
    /// own choice (the environment, the one stored account, the CLI's, git's).
    account: Option<String>,
}

/// What came back, before any interpretation: the status, the headers that
/// carry GitHub's rate-limit and scope facts, and the body as text.
struct Answer {
    status: StatusCode,
    headers: HeaderMap,
    text: String,
}

/// How long one request may take, connect and body included.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

impl GitHubApi {
    /// github.com. `tokens` says where a stored token lives and carries the
    /// doors to the CLI and to git's credential helper when the engine
    /// attached them.
    pub fn new(tokens: creds::TokenStore) -> Self {
        Self::with_base_url(API, tokens)
    }

    /// A GitHub Enterprise instance: the same API under `https://<host>/api/v3`.
    pub fn at_host(host: &str, tokens: creds::TokenStore) -> Self {
        let host = host.trim().to_ascii_lowercase();
        if host == HOST {
            return Self::new(tokens);
        }
        Self {
            http: Clients::shared(),
            tokens,
            api: format!("https://{host}/api/v3"),
            host,
            account: None,
        }
    }

    /// The same implementation against another base URL — a test's stub server.
    pub fn with_base_url(api: impl Into<String>, tokens: creds::TokenStore) -> Self {
        Self {
            http: Clients::shared(),
            host: tokens.host().to_string(),
            tokens,
            api: api.into().trim_end_matches('/').to_string(),
            account: None,
        }
    }

    /// The account this host is bound to, when one is.
    pub fn bound_account(&self) -> Option<&str> {
        self.account.as_deref()
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    async fn token(&self) -> CodeHostResult<creds::Secret> {
        creds::token(&self.tokens, self.account.as_deref())
            .await
            .map(|(t, _)| t)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.api)
    }

    /// One round trip with the API's headers on it. The token is `bearer_auth`'s
    /// and goes nowhere else.
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
        token: &creds::Secret,
        what: &str,
    ) -> CodeHostResult<Answer> {
        let resp = req
            .timeout(REQUEST_TIMEOUT)
            .bearer_auth(token.expose())
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await
            .map_err(|e| CodeHostError::Transport(format!("{what}: {e}")))?;
        let status = resp.status();
        let headers = resp.headers().clone();
        let text = resp
            .text()
            .await
            .map_err(|e| CodeHostError::Transport(format!("{what}: {e}")))?;
        Ok(Answer {
            status,
            headers,
            text,
        })
    }

    /// A JSON-answering REST call with the configured token.
    async fn send<T: serde::de::DeserializeOwned>(
        &self,
        req: reqwest::RequestBuilder,
        what: &str,
    ) -> CodeHostResult<T> {
        let token = self.token().await?;
        let answer = self.request(req, &token, what).await?;
        if !answer.status.is_success() {
            return Err(status_error(&answer, what));
        }
        serde_json::from_str(&answer.text).map_err(|e| {
            CodeHostError::Transport(format!("{what}: GitHub answered something unexpected: {e}"))
        })
    }

    /// GitHub's GraphQL endpoint — the only way to read review threads and to
    /// resolve one. Returns the `data` payload; a GraphQL `errors`
    /// array is a refusal, not a 200-shaped success.
    async fn graphql(
        &self,
        query: &str,
        variables: serde_json::Value,
        what: &str,
    ) -> CodeHostResult<serde_json::Value> {
        let token = self.token().await?;
        let req = self
            .http()
            .post(self.url("/graphql"))
            .json(&serde_json::json!({ "query": query, "variables": variables }));
        let answer = self.request(req, &token, what).await?;
        if !answer.status.is_success() {
            return Err(status_error(&answer, what));
        }
        let value: serde_json::Value = serde_json::from_str(&answer.text).map_err(|e| {
            CodeHostError::Transport(format!("{what}: GitHub answered something unexpected: {e}"))
        })?;
        graphql_data(value, what)
    }

    /// `GET /user` with `token`: the login, and the classic token's scopes from
    /// the `X-OAuth-Scopes` header (a fine-grained token carries none); then,
    /// best effort, `GET /user/orgs` — the organizations the token can see.
    /// A token without `read:org` sees none, and the answer says so through
    /// `Connection::recommended_missing`, never as an error.
    async fn whoami(&self, token: &creds::Secret) -> CodeHostResult<Account> {
        let what = "check the token";
        let answer = self
            .request(self.http().get(self.url("/user")), token, what)
            .await?;
        if !answer.status.is_success() {
            return Err(status_error(&answer, what));
        }
        let user: GhUser = serde_json::from_str(&answer.text).map_err(|e| {
            CodeHostError::Transport(format!("{what}: GitHub answered something unexpected: {e}"))
        })?;
        let organizations = match self
            .request(
                self.http()
                    .get(self.url("/user/orgs"))
                    .query(&[("per_page", "100")]),
                token,
                "read organizations",
            )
            .await
        {
            Ok(orgs) if orgs.status.is_success() => serde_json::from_str::<Vec<GhOrg>>(&orgs.text)
                .map(|list| list.into_iter().map(|o| o.login).collect())
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        Ok(Account {
            login: user.login,
            scopes: scopes_of(&answer.headers),
            organizations,
            name: user.name.filter(|n| !n.trim().is_empty()),
            email: user.email.filter(|e| !e.trim().is_empty()),
            id: user.id,
        })
    }
}

/// The `data` of a GraphQL answer, or its first error as a refusal — shared
/// with `gh api graphql`, whose answer has the same shape.
pub(crate) fn graphql_data(
    value: serde_json::Value,
    what: &str,
) -> CodeHostResult<serde_json::Value> {
    if let Some(errors) = value.get("errors") {
        if !errors.is_null() {
            let message = errors[0]["message"]
                .as_str()
                .unwrap_or("GraphQL error")
                .to_string();
            return Err(CodeHostError::Refused(format!("{what}: {message}")));
        }
    }
    Ok(value
        .get("data")
        .cloned()
        .unwrap_or(serde_json::Value::Null))
}

/// GitHub's word for a failed status, with its rate limits read as what they
/// are — a *wait*, not a bad token. Never the token.
fn status_error(answer: &Answer, what: &str) -> CodeHostError {
    let status = answer.status;
    if let Some(wait) = rate_limited(status, &answer.headers) {
        return CodeHostError::Transport(format!(
            "{what}: GitHub's rate limit is reached — try again {wait}"
        ));
    }
    let message = serde_json::from_str::<serde_json::Value>(&answer.text)
        .ok()
        .map(|v| explanation(&v))
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| status.to_string());
    match status.as_u16() {
        401 | 403 => CodeHostError::NotAuthenticated(format!("{what}: {message}")),
        404 => CodeHostError::NotFound(format!("{what}: {message}")),
        405 | 409 | 422 => CodeHostError::Refused(format!("{what}: {message}")),
        _ => CodeHostError::Transport(format!("{what}: {status} {message}")),
    }
}

/// What GitHub said, in its own words. A refusal carries a `message` — often
/// only the status's name, *Unprocessable Entity* — and an `errors` list that
/// names the actual violation: bare sentences on some endpoints (*Can not
/// approve your own pull request*), `{resource, field, code, message}` objects
/// on others. The sentence a person reads is the errors' when there are any,
/// so a refusal explains itself instead of naming a status.
fn explanation(v: &serde_json::Value) -> String {
    let errors: Vec<String> = v["errors"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|e| match e {
                    serde_json::Value::String(s) => Some(s.clone()),
                    serde_json::Value::Object(o) => o
                        .get("message")
                        .and_then(|m| m.as_str())
                        .map(str::to_string)
                        .or_else(|| {
                            let field = o.get("field").and_then(|f| f.as_str())?;
                            let code = o.get("code").and_then(|c| c.as_str()).unwrap_or("invalid");
                            Some(format!("{field} is {code}"))
                        }),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    if !errors.is_empty() {
        return errors.join("; ");
    }
    v["message"].as_str().unwrap_or_default().to_string()
}

/// When a 403 or 429 is GitHub's rate limit rather than a refusal: the primary
/// limit says `x-ratelimit-remaining: 0` and when it resets; the secondary
/// says `retry-after`. `Some(wait)` when it is a limit — the seconds to wait,
/// `None` inside when GitHub named no reset still to come — and `None` when
/// it is not. Public: the engine's unsigned read of the latest release
/// (`bisa_engine::updates`) meets the same limit and reads the same headers.
pub fn rate_limit_secs(status: StatusCode, headers: &HeaderMap) -> Option<Option<u64>> {
    if status.as_u16() != 403 && status.as_u16() != 429 {
        return None;
    }
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok())
    };
    if let Some(secs) = header("retry-after") {
        return Some(Some(secs));
    }
    if header("x-ratelimit-remaining") == Some(0) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let secs = header("x-ratelimit-reset")
            .map(|reset| reset.saturating_sub(now))
            .filter(|s| *s > 0);
        return Some(secs);
    }
    None
}

/// The wait, in words — `in 42s`, or `in a minute` when GitHub named no
/// reset still to come.
fn rate_limited(status: StatusCode, headers: &HeaderMap) -> Option<String> {
    rate_limit_secs(status, headers).map(|secs| match secs {
        Some(s) => format!("in {s}s"),
        None => "in a minute".to_string(),
    })
}

/// The classic token's scopes, as GitHub lists them: `repo, workflow, read:org`.
fn scopes_of(headers: &HeaderMap) -> Vec<String> {
    headers
        .get("x-oauth-scopes")
        .and_then(|v| v.to_str().ok())
        .map(|v| {
            v.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Deserialize)]
struct GhUser {
    login: String,
    #[serde(default)]
    id: Option<u64>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    email: Option<String>,
}
#[derive(Deserialize)]
struct GhOrg {
    login: String,
}
#[derive(Deserialize, Default)]
struct GhPermissions {
    #[serde(default)]
    push: bool,
}
#[derive(Deserialize)]
struct GhRepo {
    #[serde(default)]
    permissions: GhPermissions,
}
#[derive(Deserialize)]
struct GhRef {
    #[serde(rename = "ref")]
    name: String,
    #[serde(default)]
    sha: String,
}
#[derive(Deserialize)]
struct GhPull {
    number: u64,
    html_url: String,
    title: String,
    state: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    merged: bool,
    #[serde(default)]
    merged_at: Option<String>,
    mergeable: Option<bool>,
    head: GhRef,
    base: GhRef,
    user: Option<GhUser>,
}

impl From<GhPull> for PullRequest {
    fn from(p: GhPull) -> Self {
        let state = if p.merged || p.merged_at.is_some() {
            PrState::Merged
        } else {
            match p.state.as_str() {
                "open" => PrState::Open,
                "closed" => PrState::Closed,
                _ => PrState::Unknown,
            }
        };
        PullRequest {
            number: p.number,
            url: p.html_url,
            title: p.title,
            state,
            is_draft: p.draft,
            mergeable: p.mergeable,
            head: p.head.name,
            head_sha: p.head.sha,
            base: p.base.name,
            author: p.user.map(|u| u.login),
        }
    }
}

#[derive(Deserialize)]
struct GhCheckOutput {
    title: Option<String>,
    summary: Option<String>,
}
#[derive(Deserialize)]
struct GhCheckRun {
    name: String,
    status: String,
    conclusion: Option<String>,
    html_url: Option<String>,
    output: Option<GhCheckOutput>,
}
#[derive(Deserialize)]
struct GhCheckRuns {
    #[serde(default)]
    check_runs: Vec<GhCheckRun>,
}
#[derive(Deserialize)]
struct GhMerge {
    #[serde(default)]
    merged: bool,
    sha: Option<String>,
    #[serde(default)]
    message: String,
}

// --- GraphQL review shapes, shared with `gh api graphql` ---------------
#[derive(Deserialize)]
struct GqlNodes<T> {
    nodes: Vec<T>,
}
impl<T> Default for GqlNodes<T> {
    fn default() -> Self {
        Self { nodes: Vec::new() }
    }
}

#[derive(Deserialize)]
struct GqlReviewsData {
    repository: Option<GqlRepository>,
}
#[derive(Deserialize)]
struct GqlRepository {
    #[serde(rename = "pullRequest")]
    pull_request: Option<GqlPullRequest>,
}
#[derive(Deserialize, Default)]
struct GqlPullRequest {
    reviews: GqlNodes<GqlReview>,
    #[serde(rename = "reviewThreads")]
    review_threads: GqlNodes<GqlThread>,
}
#[derive(Deserialize)]
struct GqlReview {
    author: Option<GhUser>,
    #[serde(default)]
    state: String,
    #[serde(default)]
    body: String,
    #[serde(rename = "submittedAt")]
    submitted_at: Option<String>,
}
#[derive(Deserialize)]
struct GqlThread {
    id: String,
    #[serde(rename = "isResolved", default)]
    is_resolved: bool,
    #[serde(rename = "isOutdated", default)]
    is_outdated: bool,
    path: Option<String>,
    line: Option<u32>,
    #[serde(default)]
    comments: GqlNodes<GqlThreadComment>,
}
#[derive(Deserialize)]
struct GqlThreadComment {
    author: Option<GhUser>,
    #[serde(default)]
    body: String,
    #[serde(rename = "createdAt")]
    created_at: Option<String>,
}

/// The one GraphQL query that reads a pull request's reviews and threads.
pub(crate) const REVIEWS_QUERY: &str = r#"
  query($owner:String!,$name:String!,$number:Int!){
    repository(owner:$owner,name:$name){
      pullRequest(number:$number){
        reviews(first:100){ nodes{ author{login} state body submittedAt } }
        reviewThreads(first:100){ nodes{
          id isResolved isOutdated path line
          comments(first:100){ nodes{ author{login} body createdAt } }
        } }
      }
    }
  }"#;

/// `resolveReviewThread` / `unresolveReviewThread` — GraphQL-only.
pub(crate) fn resolve_thread_mutation(resolved: bool) -> &'static str {
    if resolved {
        "mutation($id:ID!){ resolveReviewThread(input:{threadId:$id}){ thread{ id } } }"
    } else {
        "mutation($id:ID!){ unresolveReviewThread(input:{threadId:$id}){ thread{ id } } }"
    }
}

/// `addPullRequestReviewThreadReply` — GraphQL-only, on the same thread node
/// id [`REVIEWS_QUERY`] reads and the resolve mutations act on.
pub(crate) const REPLY_THREAD_MUTATION: &str =
    "mutation($id:ID!,$body:String!){ addPullRequestReviewThreadReply(input:{pullRequestReviewThreadId:$id,body:$body}){ comment{ id } } }";

/// The reviews and threads out of [`REVIEWS_QUERY`]'s `data`.
pub(crate) fn reviews_of(data: serde_json::Value, what: &str) -> CodeHostResult<PrReviews> {
    let data: GqlReviewsData = serde_json::from_value(data)
        .map_err(|e| CodeHostError::Transport(format!("{what}: {e}")))?;
    let pr = data
        .repository
        .and_then(|r| r.pull_request)
        .unwrap_or_default();
    Ok(PrReviews {
        reviews: pr
            .reviews
            .nodes
            .into_iter()
            .map(|r| ReviewSummary {
                author: r.author.map(|a| a.login),
                state: r.state.to_lowercase(),
                body: r.body,
                submitted_at: r.submitted_at,
            })
            .collect(),
        threads: pr
            .review_threads
            .nodes
            .into_iter()
            .map(|t| ReviewThread {
                id: t.id,
                path: t.path,
                line: t.line,
                is_resolved: t.is_resolved,
                is_outdated: t.is_outdated,
                comments: t
                    .comments
                    .nodes
                    .into_iter()
                    .map(|c| ReviewThreadComment {
                        author: c.author.map(|a| a.login),
                        body: c.body,
                        created_at: c.created_at,
                    })
                    .collect(),
            })
            .collect(),
    })
}

/// The REST body of a review — shared with `gh api`, which posts the same.
pub(crate) fn review_body(review: &Review) -> serde_json::Value {
    let event = match review.event {
        ReviewEvent::Approve => "APPROVE",
        ReviewEvent::RequestChanges => "REQUEST_CHANGES",
        ReviewEvent::Comment => "COMMENT",
    };
    let side = |s: Side| match s {
        Side::Left => "LEFT",
        Side::Right => "RIGHT",
    };
    let comments: Vec<serde_json::Value> = review
        .comments
        .iter()
        .map(|c| {
            let mut comment = serde_json::json!({"path": c.path, "line": c.line, "side": side(c.side), "body": c.body});
            if let Some(start) = c.start_line {
                comment["start_line"] = serde_json::json!(start);
                comment["start_side"] = serde_json::json!(side(c.start_side.unwrap_or(c.side)));
            }
            comment
        })
        .collect();
    let mut body = serde_json::json!({"event": event, "comments": comments});
    // An approval may carry no summary; the key is absent then, never `""`.
    if let Some(words) = review.words() {
        body["body"] = serde_json::json!(words);
    }
    body
}

pub(crate) fn capabilities() -> CodeHostCapabilities {
    CodeHostCapabilities {
        draft_prs: true,
        reviewers: true,
        labels: true,
        merge_strategies: vec![
            MergeStrategy::Merge,
            MergeStrategy::Squash,
            MergeStrategy::Rebase,
        ],
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

#[async_trait::async_trait]
impl CodeHost for GitHubApi {
    fn id(&self) -> CodeHostId {
        CodeHostKind::GitHub.id()
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
        let token = self.token().await?;
        self.whoami(&token).await
    }

    async fn verify_token(&self, token: &str, _login: Option<&str>) -> CodeHostResult<Account> {
        let token = creds::Secret::some(token)
            .ok_or_else(|| CodeHostError::NotAuthenticated("an empty token".into()))?;
        self.whoami(&token).await
    }

    /// `GET /repos/{owner}/{name}`: found, and whether `permissions.push` is
    /// granted to the bound account. A 404 is *not found* — GitHub answers
    /// that for a private repository the account cannot see — never an error.
    async fn repo_access(&self, repo: &RepoRef) -> CodeHostResult<RepoAccess> {
        let what = "read repository access";
        let token = self.token().await?;
        let url = self.url(&format!("/repos/{}/{}", repo.owner, repo.name));
        let answer = self.request(self.http().get(&url), &token, what).await?;
        if answer.status.as_u16() == 404 {
            return Ok(RepoAccess {
                found: false,
                push: false,
            });
        }
        if !answer.status.is_success() {
            return Err(status_error(&answer, what));
        }
        let r: GhRepo = serde_json::from_str(&answer.text).map_err(|e| {
            CodeHostError::Transport(format!("{what}: GitHub answered something unexpected: {e}"))
        })?;
        Ok(RepoAccess {
            found: true,
            push: r.permissions.push,
        })
    }

    async fn create_pr(&self, repo: &RepoRef, req: PrCreate) -> CodeHostResult<PullRequest> {
        let url = self.url(&format!("/repos/{}/{}/pulls", repo.owner, repo.name));
        let body = serde_json::json!({
            "title": req.title,
            "body": req.body,
            "head": req.head,
            "base": req.base,
            "draft": req.draft,
        });
        let pr: GhPull = self
            .send(self.http().post(&url).json(&body), "create pull request")
            .await?;
        let pr: PullRequest = pr.into();
        // Reviewers and labels are best effort: the pull request exists, and
        // a reviewer who cannot be requested is a note, not a failure.
        if !req.reviewers.is_empty() {
            let url = self.url(&format!(
                "/repos/{}/{}/pulls/{}/requested_reviewers",
                repo.owner, repo.name, pr.number
            ));
            if let Err(e) = self
                .send::<serde_json::Value>(
                    self.http()
                        .post(&url)
                        .json(&serde_json::json!({"reviewers": req.reviewers})),
                    "request reviewers",
                )
                .await
            {
                tracing::warn!(target: "bisa_codehost::github", repo = %format!("{}/{}", repo.owner, repo.name), pr = pr.number, "the pull request is open, but its reviewers were not requested: {e}");
            }
        }
        if !req.labels.is_empty() {
            let url = self.url(&format!(
                "/repos/{}/{}/issues/{}/labels",
                repo.owner, repo.name, pr.number
            ));
            if let Err(e) = self
                .send::<serde_json::Value>(
                    self.http()
                        .post(&url)
                        .json(&serde_json::json!({"labels": req.labels})),
                    "add labels",
                )
                .await
            {
                tracing::warn!(target: "bisa_codehost::github", repo = %format!("{}/{}", repo.owner, repo.name), pr = pr.number, "the pull request is open, but its labels were not added: {e}");
            }
        }
        Ok(pr)
    }

    async fn get_pr(&self, repo: &RepoRef, number: u64) -> CodeHostResult<PullRequest> {
        let url = self.url(&format!(
            "/repos/{}/{}/pulls/{number}",
            repo.owner, repo.name
        ));
        let pr: GhPull = self
            .send(self.http().get(&url), "read pull request")
            .await?;
        Ok(pr.into())
    }

    /// REST knows `open`, `closed` and `all`; *merged* is a closed pull request
    /// with a `merged_at`, so `Merged` and `Closed` are the same request
    /// filtered here — a caller asking for merged ones never sees the others.
    async fn list_prs(&self, repo: &RepoRef, filter: PrFilter) -> CodeHostResult<Vec<PullRequest>> {
        let url = self.url(&format!("/repos/{}/{}/pulls", repo.owner, repo.name));
        let mut req = self.http().get(&url).query(&[("per_page", "50")]);
        req = match filter.state {
            Some(PrState::Open) | None => req.query(&[("state", "open")]),
            Some(PrState::Closed) | Some(PrState::Merged) => req.query(&[("state", "closed")]),
            Some(PrState::Unknown) => req.query(&[("state", "all")]),
        };
        if let Some(head) = &filter.head {
            req = req.query(&[("head", format!("{}:{head}", repo.owner))]);
        }
        let prs: Vec<GhPull> = self.send(req, "list pull requests").await?;
        Ok(prs
            .into_iter()
            .map(PullRequest::from)
            .filter(|p| match filter.state {
                Some(PrState::Merged) => p.state == PrState::Merged,
                Some(PrState::Closed) => p.state == PrState::Closed,
                _ => true,
            })
            .collect())
    }

    /// Two round trips: the pull request for its head SHA, then the check runs
    /// on that commit — GitHub keys checks by commit, not by pull request.
    async fn checks(&self, repo: &RepoRef, number: u64) -> CodeHostResult<Vec<CheckRun>> {
        let pr = self.get_pr(repo, number).await?;
        let url = self.url(&format!(
            "/repos/{}/{}/commits/{}/check-runs",
            repo.owner, repo.name, pr.head_sha
        ));
        let runs: GhCheckRuns = self
            .send(
                self.http().get(&url).query(&[("per_page", "100")]),
                "read check runs",
            )
            .await?;
        Ok(runs
            .check_runs
            .into_iter()
            .map(|r| CheckRun {
                name: r.name,
                status: r.status,
                conclusion: r.conclusion,
                url: r.html_url,
                summary: r.output.and_then(|o| o.summary.or(o.title)),
            })
            .collect())
    }

    async fn submit_review(
        &self,
        repo: &RepoRef,
        number: u64,
        review: Review,
    ) -> CodeHostResult<()> {
        let url = self.url(&format!(
            "/repos/{}/{}/pulls/{number}/reviews",
            repo.owner, repo.name
        ));
        // GitHub refuses a comment or a change request with no words; said
        // here, in the person's language, before the round trip.
        if review.needs_words() {
            return Err(CodeHostError::Refused(
                "submit review: a comment or a change request needs words — say what you noticed"
                    .into(),
            ));
        }
        let _: serde_json::Value = self
            .send(
                self.http().post(&url).json(&review_body(&review)),
                "submit review",
            )
            .await?;
        Ok(())
    }

    async fn pr_reviews(&self, repo: &RepoRef, number: u64) -> CodeHostResult<PrReviews> {
        let what = "read reviews";
        let vars = serde_json::json!({ "owner": repo.owner, "name": repo.name, "number": number });
        let data = self.graphql(REVIEWS_QUERY, vars, what).await?;
        reviews_of(data, what)
    }

    async fn resolve_review_thread(&self, thread_id: &str, resolved: bool) -> CodeHostResult<()> {
        self.graphql(
            resolve_thread_mutation(resolved),
            serde_json::json!({ "id": thread_id }),
            "resolve review thread",
        )
        .await?;
        Ok(())
    }

    async fn reply_review_thread(&self, thread_id: &str, body: &str) -> CodeHostResult<()> {
        self.graphql(
            REPLY_THREAD_MUTATION,
            serde_json::json!({ "id": thread_id, "body": body }),
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
        let url = self.url(&format!(
            "/repos/{}/{}/pulls/{number}/merge",
            repo.owner, repo.name
        ));
        let method = match strategy {
            MergeStrategy::Merge => "merge",
            MergeStrategy::Squash => "squash",
            MergeStrategy::Rebase => "rebase",
        };
        let m: GhMerge = self
            .send(
                self.http()
                    .put(&url)
                    .json(&serde_json::json!({"merge_method": method})),
                "merge pull request",
            )
            .await?;
        Ok(MergeOutcome {
            merged: m.merged,
            sha: m.sha,
            message: m.message,
            remote_branch_deleted: None,
        })
    }

    async fn delete_branch(&self, repo: &RepoRef, branch: &str) -> CodeHostResult<()> {
        // `DELETE /repos/{owner}/{repo}/git/refs/heads/{branch}` answers 204
        // with no body; a 422 is GitHub's word for "already gone".
        let url = self.url(&format!(
            "/repos/{}/{}/git/refs/heads/{branch}",
            repo.owner, repo.name
        ));
        let what = "delete branch";
        let token = self.token().await?;
        let answer = self.request(self.http().delete(&url), &token, what).await?;
        if answer.status.is_success() {
            return Ok(());
        }
        Err(status_error(&answer, what))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(*k, HeaderValue::from_str(v).unwrap());
        }
        h
    }

    fn store(dir: &tempfile::TempDir) -> creds::TokenStore {
        creds::TokenStore::file_only(CodeHostKind::GitHub, HOST, dir.path())
    }

    #[test]
    fn a_rate_limit_is_a_wait_and_a_plain_403_is_not() {
        assert_eq!(
            rate_limited(StatusCode::FORBIDDEN, &headers(&[("retry-after", "42")])),
            Some("in 42s".to_string())
        );
        let far = (SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 90)
            .to_string();
        let wait = rate_limited(
            StatusCode::FORBIDDEN,
            &headers(&[("x-ratelimit-remaining", "0"), ("x-ratelimit-reset", &far)]),
        )
        .unwrap();
        assert!(wait.starts_with("in ") && wait.ends_with('s'), "{wait}");
        assert_eq!(
            rate_limited(
                StatusCode::TOO_MANY_REQUESTS,
                &headers(&[("x-ratelimit-remaining", "0")])
            ),
            Some("in a minute".to_string())
        );
        assert_eq!(
            rate_limited(
                StatusCode::FORBIDDEN,
                &headers(&[("x-ratelimit-remaining", "12")])
            ),
            None
        );
        assert_eq!(
            rate_limited(StatusCode::UNAUTHORIZED, &headers(&[("retry-after", "1")])),
            None
        );
    }

    #[test]
    fn the_seconds_behind_the_wait_are_read_by_the_engine_too() {
        assert_eq!(
            rate_limit_secs(StatusCode::FORBIDDEN, &headers(&[("retry-after", "42")])),
            Some(Some(42))
        );
        assert_eq!(
            rate_limit_secs(
                StatusCode::TOO_MANY_REQUESTS,
                &headers(&[("x-ratelimit-remaining", "0")])
            ),
            Some(None),
            "a limit with no reset still to come is a limit without a wait"
        );
        assert_eq!(
            rate_limit_secs(
                StatusCode::FORBIDDEN,
                &headers(&[("x-ratelimit-remaining", "12")])
            ),
            None
        );
        assert_eq!(
            rate_limit_secs(StatusCode::NOT_FOUND, &headers(&[("retry-after", "1")])),
            None
        );
    }

    #[test]
    fn scopes_are_split_and_trimmed_and_absent_for_a_fine_grained_token() {
        assert_eq!(
            scopes_of(&headers(&[("x-oauth-scopes", "repo, workflow ,read:org,")])),
            vec!["repo", "workflow", "read:org"]
        );
        assert!(scopes_of(&HeaderMap::new()).is_empty());
    }

    #[test]
    fn the_base_url_loses_its_trailing_slash_and_an_enterprise_host_has_its_own() {
        let dir = tempfile::tempdir().unwrap();
        let host = GitHubApi::with_base_url("http://127.0.0.1:1/", store(&dir));
        assert_eq!(host.url("/user"), "http://127.0.0.1:1/user");
        assert_eq!(host.host(), "github.com");
        let ghe = GitHubApi::at_host(
            "GitHub.Acme.internal",
            creds::TokenStore::file_only(CodeHostKind::GitHub, "github.acme.internal", dir.path()),
        );
        assert_eq!(ghe.url("/user"), "https://github.acme.internal/api/v3/user");
        assert!(ghe
            .detect(&RemoteUrl::parse("git@github.acme.internal:acme/web.git"))
            .is_some());
        assert!(ghe
            .detect(&RemoteUrl::parse("git@github.com:acme/web.git"))
            .is_none());
        assert_eq!(GitHubApi::at_host("github.com", store(&dir)).url(""), API);
    }

    #[test]
    fn binding_an_account_is_a_clone_and_the_same_login_is_the_same_host() {
        let dir = tempfile::tempdir().unwrap();
        let host = GitHubApi::with_base_url("http://127.0.0.1:1", store(&dir));
        assert_eq!(host.bound_account(), None);
        let bound = GitHubApi {
            account: Some("ada-acme".into()),
            ..host.clone()
        };
        assert_eq!(bound.bound_account(), Some("ada-acme"));
        let shared: Arc<GitHubApi> = Arc::new(host.clone());
        assert_eq!(
            Arc::clone(&shared).for_account(Some(" Ada-Acme ")).id().0,
            "github"
        );
        assert_eq!(Arc::clone(&shared).for_account(None).id().0, "github");
        assert_eq!(host.bound_account(), None, "the original is untouched");
        assert_eq!(capabilities().review_events.len(), 3);
    }

    #[test]
    fn a_graphql_answer_is_its_data_or_its_first_error() {
        let ok = graphql_data(serde_json::json!({"data": {"x": 1}}), "q").unwrap();
        assert_eq!(ok["x"], 1);
        let err = graphql_data(
            serde_json::json!({"errors": [{"message": "Could not resolve"}]}),
            "q",
        )
        .unwrap_err();
        assert!(matches!(err, CodeHostError::Refused(m) if m.contains("Could not resolve")));
        let reviews = reviews_of(
            serde_json::json!({"repository": {"pullRequest": {"reviews": {"nodes": [{"author": {"login": "a"}, "state": "APPROVED", "body": "", "submittedAt": null}]}, "reviewThreads": {"nodes": []}}}}),
            "q",
        )
        .unwrap();
        assert_eq!(reviews.reviews[0].state, "approved");
        let body = review_body(&Review {
            event: ReviewEvent::Approve,
            body: None,
            comments: vec![],
        });
        assert_eq!(body["event"], "APPROVE");
        assert!(body.get("body").is_none(), "no words, no key");
    }
}
