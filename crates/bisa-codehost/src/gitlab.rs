//! GitLab over its REST API (v4) — the second layer of the GitLab code host,
//! behind `glab` ([`crate::cli::glab::GlabCli`]). GitLab's words map onto the
//! crate's: a *merge request* is a [`PullRequest`], its *iid* the number, a
//! *pipeline*'s jobs are the check runs, *approvals* and top-level *notes* are
//! the reviews, resolvable *discussions* with a position are the review
//! threads. A project is addressed by its URL-encoded full path, so a group
//! with subgroups is one `RepoRef` whose `owner` carries the slashes.
//!
//! A discussion is resolved by project and merge request, so a thread's id
//! here is `owner/name#iid#discussion` — what [`thread_id`] makes and
//! [`parse_thread_id`] reads back. The base URL is a field
//! ([`GitLabApi::with_base_url`]) so the whole implementation is tested
//! against a loopback stub; a self-hosted instance is the same implementation
//! at `https://<host>/api/v4` ([`GitLabApi::at_host`]).

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

pub const HOST: &str = "gitlab.com";
pub const API: &str = "https://gitlab.com/api/v4";

/// The scopes a personal access token needs: `api` covers everything here.
pub const REQUIRED_SCOPES: &[&str] = &["api"];
pub const RECOMMENDED_SCOPES: &[&str] = &[];

/// Developer access — the least that may push and merge.
const DEVELOPER: u64 = 30;

#[derive(Clone)]
pub struct GitLabApi {
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
    retry_after: Option<String>,
    text: String,
}

/// How long one request may take, connect and body included.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Percent-encode one path component the way GitLab wants a project path:
/// `acme/platform/web` → `acme%2Fplatform%2Fweb`.
pub(crate) fn encode(component: &str) -> String {
    let mut out = String::with_capacity(component.len());
    for b in component.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The project as the API addresses it.
pub(crate) fn project(repo: &RepoRef) -> String {
    encode(&repo.slug())
}

pub(crate) fn thread_id(repo: &RepoRef, iid: u64, discussion: &str) -> String {
    format!("{}#{iid}#{discussion}", repo.slug())
}

/// `owner/name#iid#discussion` → the parts; `None` for any other shape.
pub(crate) fn parse_thread_id(id: &str) -> Option<(String, u64, String)> {
    let mut parts = id.rsplitn(3, '#');
    let discussion = parts.next()?.to_string();
    let iid = parts.next()?.parse().ok()?;
    let slug = parts.next()?.to_string();
    (!slug.is_empty() && !discussion.is_empty()).then_some((slug, iid, discussion))
}

/// The `state` query for a filter: GitLab says `opened`, not `open`.
pub(crate) fn state_query(state: Option<PrState>) -> &'static str {
    match state {
        Some(PrState::Open) | None => "opened",
        Some(PrState::Closed) => "closed",
        Some(PrState::Merged) => "merged",
        Some(PrState::Unknown) => "all",
    }
}

pub(crate) fn capabilities() -> CodeHostCapabilities {
    CodeHostCapabilities {
        draft_prs: true,
        reviewers: true,
        labels: true,
        merge_strategies: vec![MergeStrategy::Merge, MergeStrategy::Squash],
        check_runs: true,
        review_comments: true,
        review_threads: true,
        review_thread_replies: true,
        delete_branch: true,
        // GitLab has approvals and comments; a *request changes* review is not one of its verbs.
        review_events: vec![ReviewEvent::Approve, ReviewEvent::Comment],
    }
}

#[derive(Deserialize)]
pub(crate) struct GlUser {
    pub username: String,
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub public_email: Option<String>,
}
#[derive(Deserialize)]
pub(crate) struct GlDiffRefs {
    pub base_sha: Option<String>,
    pub head_sha: Option<String>,
    pub start_sha: Option<String>,
}
/// A merge request as the API and `glab mr view -F json` both spell it.
#[derive(Deserialize)]
pub(crate) struct GlMergeRequest {
    pub iid: u64,
    pub web_url: String,
    pub title: String,
    pub state: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub detailed_merge_status: Option<String>,
    #[serde(default)]
    pub merge_status: Option<String>,
    pub source_branch: String,
    #[serde(default)]
    pub sha: Option<String>,
    pub target_branch: String,
    pub author: Option<GlUser>,
    #[serde(default)]
    pub merge_commit_sha: Option<String>,
    #[serde(default)]
    pub diff_refs: Option<GlDiffRefs>,
}

impl From<GlMergeRequest> for PullRequest {
    fn from(mr: GlMergeRequest) -> Self {
        let mergeable = match mr
            .detailed_merge_status
            .as_deref()
            .or(mr.merge_status.as_deref())
        {
            Some("mergeable") | Some("can_be_merged") => Some(true),
            Some("checking") | Some("unchecked") | Some("preparing") | None => None,
            Some(_) => Some(false),
        };
        PullRequest {
            number: mr.iid,
            url: mr.web_url,
            title: mr.title,
            state: match mr.state.as_str() {
                "opened" | "locked" => PrState::Open,
                "closed" => PrState::Closed,
                "merged" => PrState::Merged,
                _ => PrState::Unknown,
            },
            is_draft: mr.draft,
            mergeable,
            head: mr.source_branch,
            head_sha: mr.sha.unwrap_or_default(),
            base: mr.target_branch,
            author: mr.author.map(|a| a.username),
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct GlPipeline {
    pub id: u64,
}
#[derive(Deserialize)]
pub(crate) struct GlJob {
    pub name: String,
    pub status: String,
    pub web_url: Option<String>,
    #[serde(default)]
    pub stage: Option<String>,
}

impl From<GlJob> for CheckRun {
    fn from(j: GlJob) -> Self {
        let (status, conclusion) = match j.status.as_str() {
            "created" | "pending" | "waiting_for_resource" | "preparing" | "scheduled" => {
                ("queued", None)
            }
            "running" => ("in_progress", None),
            "success" => ("completed", Some("success")),
            "failed" => ("completed", Some("failure")),
            "canceled" | "canceling" => ("completed", Some("cancelled")),
            "skipped" => ("completed", Some("skipped")),
            "manual" => ("completed", Some("action_required")),
            _ => ("completed", Some("neutral")),
        };
        CheckRun {
            name: j.name,
            status: status.to_string(),
            conclusion: conclusion.map(str::to_string),
            url: j.web_url,
            summary: j.stage,
        }
    }
}

#[derive(Deserialize, Default)]
pub(crate) struct GlApprovals {
    #[serde(default)]
    pub approved_by: Vec<GlApprovedBy>,
}
#[derive(Deserialize)]
pub(crate) struct GlApprovedBy {
    pub user: GlUser,
}
#[derive(Deserialize)]
pub(crate) struct GlPosition {
    #[serde(default)]
    pub new_path: Option<String>,
    #[serde(default)]
    pub old_path: Option<String>,
    #[serde(default)]
    pub new_line: Option<u32>,
    #[serde(default)]
    pub old_line: Option<u32>,
}
#[derive(Deserialize)]
pub(crate) struct GlNote {
    pub author: Option<GlUser>,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub system: bool,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub resolvable: bool,
    #[serde(default)]
    pub resolved: bool,
    #[serde(default)]
    pub position: Option<GlPosition>,
}
#[derive(Deserialize)]
pub(crate) struct GlDiscussion {
    pub id: String,
    #[serde(default)]
    pub notes: Vec<GlNote>,
}

/// Reviews and threads out of the three reads: the approvals (each an
/// *approved* review with no words), the notes a person wrote at the top
/// level (a *commented* review each), and the resolvable discussions with a
/// diff position (the threads).
pub(crate) fn reviews_of(
    repo: &RepoRef,
    iid: u64,
    approvals: GlApprovals,
    notes: Vec<GlNote>,
    discussions: Vec<GlDiscussion>,
) -> PrReviews {
    let mut reviews: Vec<ReviewSummary> = approvals
        .approved_by
        .into_iter()
        .map(|a| ReviewSummary {
            author: Some(a.user.username),
            state: "approved".into(),
            body: String::new(),
            submitted_at: None,
        })
        .collect();
    reviews.extend(
        notes
            .into_iter()
            .filter(|n| !n.system && n.position.is_none())
            .map(|n| ReviewSummary {
                author: n.author.map(|a| a.username),
                state: "commented".into(),
                body: n.body,
                submitted_at: n.created_at,
            }),
    );
    let threads = discussions
        .into_iter()
        .filter(|d| {
            d.notes
                .first()
                .is_some_and(|n| n.resolvable && n.position.is_some())
        })
        .map(|d| {
            let first = &d.notes[0];
            let position = first.position.as_ref();
            ReviewThread {
                id: thread_id(repo, iid, &d.id),
                path: position.and_then(|p| p.new_path.clone().or(p.old_path.clone())),
                line: position.and_then(|p| p.new_line.or(p.old_line)),
                is_resolved: d.notes.iter().all(|n| !n.resolvable || n.resolved),
                is_outdated: false,
                comments: d
                    .notes
                    .iter()
                    .filter(|n| !n.system)
                    .map(|n| ReviewThreadComment {
                        author: n.author.as_ref().map(|a| a.username.clone()),
                        body: n.body.clone(),
                        created_at: n.created_at.clone(),
                    })
                    .collect(),
            }
        })
        .collect();
    PrReviews { reviews, threads }
}

/// The body of a discussion that comments on one line of the diff.
pub(crate) fn inline_discussion(
    comment: &crate::ReviewComment,
    refs: &GlDiffRefs,
) -> serde_json::Value {
    let mut position = serde_json::json!({
        "position_type": "text",
        "base_sha": refs.base_sha,
        "head_sha": refs.head_sha,
        "start_sha": refs.start_sha,
        "new_path": comment.path,
        "old_path": comment.path,
    });
    match comment.side {
        crate::Side::Right => position["new_line"] = serde_json::json!(comment.line),
        crate::Side::Left => position["old_line"] = serde_json::json!(comment.line),
    }
    serde_json::json!({ "body": comment.body, "position": position })
}

fn status_error(answer: &Answer, what: &str) -> CodeHostError {
    let status = answer.status;
    if status.as_u16() == 429 {
        let wait = answer
            .retry_after
            .as_deref()
            .map(|s| format!("in {s}s"))
            .unwrap_or_else(|| "in a minute".into());
        return CodeHostError::Transport(format!(
            "{what}: GitLab's rate limit is reached — try again {wait}"
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
        400 | 405 | 406 | 409 | 422 => CodeHostError::Refused(format!("{what}: {message}")),
        _ => CodeHostError::Transport(format!("{what}: {status} {message}")),
    }
}

/// GitLab's `message` is a string, or an object of field → reasons.
pub(crate) fn explanation(v: &serde_json::Value) -> String {
    match v.get("message").or_else(|| v.get("error")) {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(list)) => list
            .iter()
            .filter_map(|x| x.as_str())
            .collect::<Vec<_>>()
            .join("; "),
        Some(serde_json::Value::Object(o)) => o
            .iter()
            .map(|(field, reasons)| match reasons {
                serde_json::Value::Array(list) => format!(
                    "{field} {}",
                    list.iter()
                        .filter_map(|x| x.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                other => format!("{field} {other}"),
            })
            .collect::<Vec<_>>()
            .join("; "),
        _ => String::new(),
    }
}

impl GitLabApi {
    pub fn new(tokens: creds::TokenStore) -> Self {
        Self::with_base_url(API, tokens)
    }

    /// A self-hosted instance: the same API under `https://<host>/api/v4`.
    pub fn at_host(host: &str, tokens: creds::TokenStore) -> Self {
        let host = host.trim().to_ascii_lowercase();
        if host == HOST {
            return Self::new(tokens);
        }
        Self {
            http: Clients::shared(),
            tokens,
            api: format!("https://{host}/api/v4"),
            host,
            account: None,
        }
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

    async fn token(&self) -> CodeHostResult<creds::Secret> {
        creds::token(&self.tokens, self.account.as_deref())
            .await
            .map(|(t, _)| t)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.api)
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
        token: &creds::Secret,
        what: &str,
    ) -> CodeHostResult<Answer> {
        let resp = req
            .timeout(REQUEST_TIMEOUT)
            .bearer_auth(token.expose())
            .send()
            .await
            .map_err(|e| CodeHostError::Transport(format!("{what}: {e}")))?;
        let status = resp.status();
        let retry_after = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let text = resp
            .text()
            .await
            .map_err(|e| CodeHostError::Transport(format!("{what}: {e}")))?;
        Ok(Answer {
            status,
            retry_after,
            text,
        })
    }

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
        if answer.text.trim().is_empty() {
            return serde_json::from_str("null")
                .map_err(|e| CodeHostError::Transport(format!("{what}: {e}")));
        }
        serde_json::from_str(&answer.text).map_err(|e| {
            CodeHostError::Transport(format!("{what}: GitLab answered something unexpected: {e}"))
        })
    }

    async fn whoami(&self, token: &creds::Secret) -> CodeHostResult<Account> {
        let what = "check the token";
        let answer = self
            .request(self.http().get(self.url("/user")), token, what)
            .await?;
        if !answer.status.is_success() {
            return Err(status_error(&answer, what));
        }
        let user: GlUser = serde_json::from_str(&answer.text).map_err(|e| {
            CodeHostError::Transport(format!("{what}: GitLab answered something unexpected: {e}"))
        })?;
        // Best effort: the token's own scopes (a personal access token says),
        // and the groups the account belongs to.
        let scopes = match self
            .request(
                self.http().get(self.url("/personal_access_tokens/self")),
                token,
                "read token scopes",
            )
            .await
        {
            Ok(a) if a.status.is_success() => serde_json::from_str::<serde_json::Value>(&a.text)
                .ok()
                .and_then(|v| {
                    v["scopes"].as_array().map(|s| {
                        s.iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect()
                    })
                })
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let organizations = match self
            .request(
                self.http().get(self.url("/groups")).query(&[
                    ("min_access_level", "10"),
                    ("per_page", "100"),
                    ("top_level_only", "true"),
                ]),
                token,
                "read groups",
            )
            .await
        {
            Ok(a) if a.status.is_success() => {
                serde_json::from_str::<Vec<serde_json::Value>>(&a.text)
                    .map(|list| {
                        list.into_iter()
                            .filter_map(|g| g["full_path"].as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default()
            }
            _ => Vec::new(),
        };
        Ok(Account {
            login: user.username,
            scopes,
            organizations,
            name: user.name.filter(|n| !n.trim().is_empty()),
            email: user.public_email.filter(|e| !e.trim().is_empty()),
            id: user.id,
        })
    }

    async fn merge_request(&self, repo: &RepoRef, iid: u64) -> CodeHostResult<GlMergeRequest> {
        let url = self.url(&format!("/projects/{}/merge_requests/{iid}", project(repo)));
        self.send(self.http().get(&url), "read merge request").await
    }
}

#[async_trait::async_trait]
impl CodeHost for GitLabApi {
    fn id(&self) -> CodeHostId {
        CodeHostKind::GitLab.id()
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

    /// `GET /projects/:id`: found, and whether the account's access level on
    /// the project or its group is Developer or more.
    async fn repo_access(&self, repo: &RepoRef) -> CodeHostResult<RepoAccess> {
        let what = "read repository access";
        let token = self.token().await?;
        let url = self.url(&format!("/projects/{}", project(repo)));
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
        let v: serde_json::Value = serde_json::from_str(&answer.text).map_err(|e| {
            CodeHostError::Transport(format!("{what}: GitLab answered something unexpected: {e}"))
        })?;
        let level = |path: &str| v["permissions"][path]["access_level"].as_u64().unwrap_or(0);
        Ok(RepoAccess {
            found: true,
            push: level("project_access").max(level("group_access")) >= DEVELOPER,
        })
    }

    async fn create_pr(&self, repo: &RepoRef, req: PrCreate) -> CodeHostResult<PullRequest> {
        let url = self.url(&format!("/projects/{}/merge_requests", project(repo)));
        let title = if req.draft && !req.title.to_ascii_lowercase().starts_with("draft:") {
            format!("Draft: {}", req.title)
        } else {
            req.title.clone()
        };
        let mut body = serde_json::json!({
            "source_branch": req.head,
            "target_branch": req.base,
            "title": title,
            "description": req.body,
        });
        if !req.labels.is_empty() {
            body["labels"] = serde_json::json!(req.labels.join(","));
        }
        // Reviewers are ids on GitLab: each username is looked up, best effort.
        let mut reviewer_ids = Vec::new();
        for username in &req.reviewers {
            let users: Vec<serde_json::Value> = match self
                .send(
                    self.http()
                        .get(self.url("/users"))
                        .query(&[("username", username.as_str())]),
                    "look up reviewer",
                )
                .await
            {
                Ok(list) => list,
                Err(e) => {
                    tracing::warn!(target: "bisa_codehost::gitlab", repo = %format!("{}/{}", repo.owner, repo.name), "reviewer @{username} was not looked up, so is not asked: {e}");
                    continue;
                }
            };
            if let Some(id) = users.first().and_then(|u| u["id"].as_u64()) {
                reviewer_ids.push(id);
            }
        }
        if !reviewer_ids.is_empty() {
            body["reviewer_ids"] = serde_json::json!(reviewer_ids);
        }
        let mr: GlMergeRequest = self
            .send(self.http().post(&url).json(&body), "create merge request")
            .await?;
        Ok(mr.into())
    }

    async fn get_pr(&self, repo: &RepoRef, number: u64) -> CodeHostResult<PullRequest> {
        Ok(self.merge_request(repo, number).await?.into())
    }

    async fn list_prs(&self, repo: &RepoRef, filter: PrFilter) -> CodeHostResult<Vec<PullRequest>> {
        let url = self.url(&format!("/projects/{}/merge_requests", project(repo)));
        let mut req = self
            .http()
            .get(&url)
            .query(&[("per_page", "50"), ("state", state_query(filter.state))]);
        if let Some(head) = &filter.head {
            req = req.query(&[("source_branch", head.as_str())]);
        }
        let mrs: Vec<GlMergeRequest> = self.send(req, "list merge requests").await?;
        Ok(mrs.into_iter().map(PullRequest::from).collect())
    }

    /// The merge request's latest pipeline, then its jobs — one check run each.
    async fn checks(&self, repo: &RepoRef, number: u64) -> CodeHostResult<Vec<CheckRun>> {
        let url = self.url(&format!(
            "/projects/{}/merge_requests/{number}/pipelines",
            project(repo)
        ));
        let pipelines: Vec<GlPipeline> = self.send(self.http().get(&url), "read pipelines").await?;
        let Some(latest) = pipelines.first() else {
            return Ok(Vec::new());
        };
        let url = self.url(&format!(
            "/projects/{}/pipelines/{}/jobs",
            project(repo),
            latest.id
        ));
        let jobs: Vec<GlJob> = self
            .send(
                self.http().get(&url).query(&[("per_page", "100")]),
                "read jobs",
            )
            .await?;
        Ok(jobs.into_iter().map(CheckRun::from).collect())
    }

    /// An approval is `POST …/approve` (plus a note when there are words); a
    /// comment is a note, and each inline comment a discussion on the diff.
    /// GitLab has no *request changes* review.
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
        let base = format!("/projects/{}/merge_requests/{number}", project(repo));
        match review.event {
            ReviewEvent::RequestChanges => return Err(CodeHostError::Unsupported(
                "GitLab has no request-changes review — leave a comment and say what should change"
                    .into(),
            )),
            ReviewEvent::Approve => {
                let _: serde_json::Value = self
                    .send(
                        self.http().post(self.url(&format!("{base}/approve"))),
                        "approve merge request",
                    )
                    .await?;
            }
            ReviewEvent::Comment => {}
        }
        if let Some(words) = review.words() {
            let _: serde_json::Value = self
                .send(
                    self.http()
                        .post(self.url(&format!("{base}/notes")))
                        .json(&serde_json::json!({"body": words})),
                    "comment on merge request",
                )
                .await?;
        }
        if !review.comments.is_empty() {
            let mr = self.merge_request(repo, number).await?;
            let refs = mr.diff_refs.ok_or_else(|| {
                CodeHostError::Refused(
                    "submit review: the merge request has no diff to comment on yet".into(),
                )
            })?;
            for comment in &review.comments {
                let _: serde_json::Value = self
                    .send(
                        self.http()
                            .post(self.url(&format!("{base}/discussions")))
                            .json(&inline_discussion(comment, &refs)),
                        "comment on the diff",
                    )
                    .await?;
            }
        }
        Ok(())
    }

    async fn pr_reviews(&self, repo: &RepoRef, number: u64) -> CodeHostResult<PrReviews> {
        let base = format!("/projects/{}/merge_requests/{number}", project(repo));
        let approvals: GlApprovals = self
            .send(
                self.http().get(self.url(&format!("{base}/approvals"))),
                "read approvals",
            )
            .await?;
        let notes: Vec<GlNote> = self
            .send(
                self.http()
                    .get(self.url(&format!("{base}/notes")))
                    .query(&[("per_page", "100")]),
                "read notes",
            )
            .await?;
        let discussions: Vec<GlDiscussion> = self
            .send(
                self.http()
                    .get(self.url(&format!("{base}/discussions")))
                    .query(&[("per_page", "100")]),
                "read discussions",
            )
            .await?;
        Ok(reviews_of(repo, number, approvals, notes, discussions))
    }

    async fn resolve_review_thread(&self, thread_id: &str, resolved: bool) -> CodeHostResult<()> {
        let (slug, iid, discussion) = parse_thread_id(thread_id)
            .ok_or_else(|| CodeHostError::NotFound(format!("review thread {thread_id}")))?;
        let url = self.url(&format!(
            "/projects/{}/merge_requests/{iid}/discussions/{discussion}",
            encode(&slug)
        ));
        let _: serde_json::Value = self
            .send(
                self.http()
                    .put(&url)
                    .query(&[("resolved", if resolved { "true" } else { "false" })]),
                "resolve review thread",
            )
            .await?;
        Ok(())
    }

    /// A note on the discussion: it joins the thread and inherits its resolvability.
    async fn reply_review_thread(&self, thread_id: &str, body: &str) -> CodeHostResult<()> {
        let (slug, iid, discussion) = parse_thread_id(thread_id)
            .ok_or_else(|| CodeHostError::NotFound(format!("review thread {thread_id}")))?;
        let url = self.url(&format!(
            "/projects/{}/merge_requests/{iid}/discussions/{discussion}/notes",
            encode(&slug)
        ));
        let _: serde_json::Value = self
            .send(
                self.http()
                    .post(&url)
                    .json(&serde_json::json!({"body": body})),
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
        let squash = match strategy {
            MergeStrategy::Merge => false,
            MergeStrategy::Squash => true,
            MergeStrategy::Rebase => {
                return Err(CodeHostError::Unsupported(
                    "a rebase merge on GitLab".into(),
                ))
            }
        };
        let url = self.url(&format!(
            "/projects/{}/merge_requests/{number}/merge",
            project(repo)
        ));
        let mr: GlMergeRequest = self
            .send(
                self.http()
                    .put(&url)
                    .json(&serde_json::json!({"squash": squash})),
                "merge merge request",
            )
            .await?;
        let merged = mr.state == "merged";
        Ok(MergeOutcome {
            merged,
            sha: mr.merge_commit_sha,
            message: if merged {
                "merged".into()
            } else {
                format!("merge request !{number} is {}", mr.state)
            },
            remote_branch_deleted: None,
        })
    }

    async fn delete_branch(&self, repo: &RepoRef, branch: &str) -> CodeHostResult<()> {
        let what = "delete branch";
        let token = self.token().await?;
        let url = self.url(&format!(
            "/projects/{}/repository/branches/{}",
            project(repo),
            encode(branch)
        ));
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

    fn repo() -> RepoRef {
        RepoRef {
            host: HOST.into(),
            owner: "acme/platform".into(),
            name: "web".into(),
        }
    }

    #[test]
    fn a_project_path_is_encoded_and_a_thread_id_round_trips() {
        assert_eq!(project(&repo()), "acme%2Fplatform%2Fweb");
        assert_eq!(encode("feature/x y"), "feature%2Fx%20y");
        // A discussion id is a hex string, never a `#`, so the composite splits from the right.
        let id = thread_id(&repo(), 7, "6a9c1e");
        assert_eq!(id, "acme/platform/web#7#6a9c1e");
        assert_eq!(
            parse_thread_id(&id),
            Some(("acme/platform/web".to_string(), 7, "6a9c1e".to_string()))
        );
        assert_eq!(parse_thread_id("nonsense"), None);
        assert_eq!(state_query(Some(PrState::Open)), "opened");
    }

    #[test]
    fn a_merge_request_and_a_job_read_into_the_crates_words() {
        let mr: GlMergeRequest = serde_json::from_str(
            r#"{"iid":3,"web_url":"u","title":"t","state":"opened","draft":true,"detailed_merge_status":"checking","source_branch":"work/x","sha":"abc","target_branch":"main","author":{"username":"ada"}}"#,
        )
        .unwrap();
        let pr: PullRequest = mr.into();
        assert_eq!(
            (
                pr.number,
                pr.state,
                pr.is_draft,
                pr.mergeable,
                pr.author.as_deref()
            ),
            (3, PrState::Open, true, None, Some("ada"))
        );
        let conflicting: PullRequest = serde_json::from_str::<GlMergeRequest>(
            r#"{"iid":3,"web_url":"u","title":"t","state":"merged","detailed_merge_status":"conflict","source_branch":"a","target_branch":"b"}"#,
        )
        .unwrap()
        .into();
        assert_eq!(
            (conflicting.state, conflicting.mergeable),
            (PrState::Merged, Some(false))
        );
        let job: CheckRun = serde_json::from_str::<GlJob>(
            r#"{"name":"test","status":"running","web_url":"w","stage":"test"}"#,
        )
        .unwrap()
        .into();
        assert_eq!((job.status.as_str(), job.conclusion), ("in_progress", None));
        let failed: CheckRun =
            serde_json::from_str::<GlJob>(r#"{"name":"lint","status":"failed","web_url":null}"#)
                .unwrap()
                .into();
        assert_eq!(failed.conclusion.as_deref(), Some("failure"));
    }

    #[test]
    fn approvals_notes_and_discussions_become_reviews_and_threads() {
        let approvals: GlApprovals =
            serde_json::from_str(r#"{"approved_by":[{"user":{"username":"alice"}}]}"#).unwrap();
        let notes: Vec<GlNote> = serde_json::from_str(
            r#"[{"author":{"username":"bob"},"body":"looks fine","system":false,"created_at":"2026-01-01T00:00:00Z"},{"author":{"username":"gitlab"},"body":"approved this merge request","system":true},{"author":{"username":"carol"},"body":"inline","position":{"new_path":"a.rs","new_line":3}}]"#,
        )
        .unwrap();
        let discussions: Vec<GlDiscussion> = serde_json::from_str(
            r#"[{"id":"d1","notes":[{"author":{"username":"carol"},"body":"rename","resolvable":true,"resolved":false,"position":{"new_path":"a.rs","new_line":3}},{"author":{"username":"ada"},"body":"done","resolvable":true,"resolved":false}]},{"id":"d2","notes":[{"author":{"username":"bob"},"body":"general","resolvable":false}]}]"#,
        )
        .unwrap();
        let out = reviews_of(&repo(), 7, approvals, notes, discussions);
        assert_eq!(out.reviews.len(), 2, "an approval and one top-level note; the system note and the inline note are not reviews");
        assert_eq!(
            (
                out.reviews[0].author.as_deref(),
                out.reviews[0].state.as_str()
            ),
            (Some("alice"), "approved")
        );
        assert_eq!(
            (
                out.reviews[1].author.as_deref(),
                out.reviews[1].state.as_str()
            ),
            (Some("bob"), "commented")
        );
        assert_eq!(
            out.threads.len(),
            1,
            "only a resolvable discussion with a position is a thread"
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
            ("acme/platform/web#7#d1", Some("a.rs"), Some(3), false, 2)
        );
        assert_eq!(
            explanation(&serde_json::json!({"message": {"title": ["can't be blank"]}})),
            "title can't be blank"
        );
        assert_eq!(
            explanation(&serde_json::json!({"message": "405 Method Not Allowed"})),
            "405 Method Not Allowed"
        );
    }

    #[test]
    fn the_base_url_and_a_self_hosted_instance() {
        let dir = tempfile::tempdir().unwrap();
        let store = creds::TokenStore::file_only(CodeHostKind::GitLab, HOST, dir.path());
        let api = GitLabApi::with_base_url("http://127.0.0.1:1/", store);
        assert_eq!(api.url("/user"), "http://127.0.0.1:1/user");
        let own = GitLabApi::at_host(
            "git.acme.internal",
            creds::TokenStore::file_only(CodeHostKind::GitLab, "git.acme.internal", dir.path()),
        );
        assert_eq!(own.url("/user"), "https://git.acme.internal/api/v4/user");
        assert_eq!(
            own.detect(&RemoteUrl::parse(
                "git@git.acme.internal:acme/platform/web.git"
            ))
            .unwrap()
            .slug(),
            "acme/platform/web"
        );
        assert_eq!(
            capabilities().review_events,
            vec![ReviewEvent::Approve, ReviewEvent::Comment]
        );
    }
}
