//! Pull requests, checks, reviews and merges through the code host trait
//! (ide/08). The engine's one door to `bisa-codehost`; the node
//! and the CLI reach the code host through these functions and never directly.
//!
//! Two outward actions live here — opening and **merging** a pull request —
//! and both pass the project's Publish gate exactly as a push does
//!: `auto` proceeds, `gated` waits on a human, `manual` refuses so
//! a person does it. A merge that goes through is the first writer of
//! `WorkstreamTransition::Merged`.

use crate::events::{EnginePayload, GitSetup};
use crate::projects::{
    blocking, journal_progress, pass_publish_gate, push_branch, reconcile_before_publish,
    require_worktree, transition,
};
use crate::{EngineError, Inner};
use bisa_core::{ProjectId, WorkstreamId, WorkstreamState, WorkstreamTransition};
use bisa_vcs::{git, ConfigScope};
use std::sync::Arc;

pub use bisa_codehost::cli::{CliAccount, CliProbe, CliProgram, InstallHints};
pub use bisa_codehost::creds::{AccountsStatus, StoreKind, StoredAccount, TokenSource};
pub use bisa_codehost::{
    Account, CheckRun, CodeHost, CodeHostCapabilities, CodeHostError, CodeHostId, CodeHostKind,
    Connection, MergeOutcome, MergeStrategy, PrCreate, PrFilter, PrReviews, PrState, PullRequest,
    RemoteProtocol, RemoteUrl, RepoAccess, RepoRef, Review, ReviewComment, ReviewEvent,
    ReviewSummary, ReviewThread, ReviewThreadComment,
};

/// The scopes a kind's token needs, and the ones it is better with — the
/// host's own words, for `Connection::of`.
pub fn scopes_for(kind: CodeHostKind) -> (&'static [&'static str], &'static [&'static str]) {
    match kind {
        CodeHostKind::GitHub => (
            bisa_codehost::github::REQUIRED_SCOPES,
            bisa_codehost::github::RECOMMENDED_SCOPES,
        ),
        CodeHostKind::GitLab => (
            bisa_codehost::gitlab::REQUIRED_SCOPES,
            bisa_codehost::gitlab::RECOMMENDED_SCOPES,
        ),
        CodeHostKind::Bitbucket => (
            bisa_codehost::bitbucket::REQUIRED_SCOPES,
            bisa_codehost::bitbucket::RECOMMENDED_SCOPES,
        ),
    }
}

/// The kind behind a code host id — a real kind's, or `None` for a test's fake.
pub fn kind_of(id: CodeHostId) -> Option<CodeHostKind> {
    id.0.parse().ok()
}

/// A remote URL with an SSH alias resolved to the host it stands for — asked
/// of `ssh -G`, offline, when this engine has SSH and the host reads as an
/// alias (no dot). Without SSH, or when ssh has no other name for it, the URL
/// is returned as written.
pub(crate) async fn resolve_alias(inner: &Inner, url: RemoteUrl) -> RemoteUrl {
    let is_alias = matches!(url.protocol, RemoteProtocol::Ssh | RemoteProtocol::Scp)
        && url.host.as_deref().is_some_and(|h| !h.contains('.'));
    let (Some(ssh), Some(host), true) = (inner.ssh.clone(), url.host.clone(), is_alias) else {
        return url;
    };
    let resolved = tokio::task::spawn_blocking(move || ssh.resolve(&host, None)).await;
    match resolved {
        Ok(Ok(r))
            if !r.hostname.is_empty()
                && !r
                    .hostname
                    .eq_ignore_ascii_case(url.host.as_deref().unwrap_or_default()) =>
        {
            url.with_host(&r.hostname)
        }
        _ => url,
    }
}

/// What a checkout's git config says about its code host: the `origin` URL
/// taken apart, the account it names (`codehost.account` — a pin's or a
/// profile's, through the engine's own `git` so a profile's include is seen)
/// and the kind it says its host is (`codehost.kind`, for a host of the
/// person's own).
pub(crate) struct Origin {
    pub remote: Option<RemoteUrl>,
    pub account: Option<String>,
    pub kind: Option<CodeHostKind>,
}

pub(crate) async fn origin_of(
    inner: &Inner,
    path: &std::path::Path,
) -> Result<Origin, EngineError> {
    let git = inner.git();
    let path = path.to_path_buf();
    let (remote, account, kind) = blocking(move || {
        let remote = git.remote_get(&path, "origin")?;
        let account = git.account_get(&path)?;
        let kind = git.kind_get(&path)?;
        Ok((remote, account, kind))
    })
    .await?;
    Ok(Origin {
        remote: remote.map(|r| RemoteUrl::parse(&r)),
        account,
        kind: kind.and_then(|k| k.parse().ok()),
    })
}

/// The kind's default account — the global `codehost.<kind>.account`.
pub(crate) async fn default_account(
    inner: &Inner,
    kind: CodeHostKind,
) -> Result<Option<String>, EngineError> {
    let git = inner.git();
    blocking(move || git.default_account(kind.as_str())).await
}

/// The code host and repository behind a checkout's `origin`, **bound to the
/// account the checkout resolves** — its pin or profile's, else the kind's
/// default — or a typed refusal naming what is missing. A host no public name
/// detects is made for the kind the checkout names (`codehost.kind`).
pub(crate) async fn code_host_for_path(
    inner: &Inner,
    path: std::path::PathBuf,
) -> Result<(Arc<dyn CodeHost>, RepoRef), EngineError> {
    let origin = origin_of(inner, &path).await?;
    let remote = origin.remote.ok_or_else(|| {
        EngineError::CodeHost(CodeHostError::NotFound(format!(
            "{} has no `origin` remote, so there is no code host to talk to",
            path.display()
        )))
    })?;
    let resolved = resolve_alias(inner, remote).await;
    let (host, repo) = inner.code_hosts.detect_as(&resolved, origin.kind).ok_or_else(|| {
        EngineError::CodeHost(CodeHostError::Unsupported(format!(
            "no code host this build knows hosts {} — for a host of your own, say which kind it is with `codehost.kind`",
            resolved.raw
        )))
    })?;
    let account = match origin.account {
        Some(a) => Some(a),
        None => match kind_of(host.id()) {
            Some(kind) => default_account(inner, kind).await?,
            None => None,
        },
    };
    Ok((host.for_account(account.as_deref()), repo))
}

/// What the code host behind a project's `origin` can do, or `None` for a
/// project with no remote or one on a code host this build does not know.
pub async fn capabilities(
    inner: &Inner,
    project: ProjectId,
) -> Result<Option<(CodeHostId, RepoRef, CodeHostCapabilities)>, EngineError> {
    let (_, root) = crate::projects::project_tree(inner, project)?;
    match code_host_for_path(inner, root).await {
        Ok((f, repo)) => Ok(Some((f.id(), repo, f.capabilities()))),
        Err(EngineError::CodeHost(CodeHostError::NotFound(_)))
        | Err(EngineError::CodeHost(CodeHostError::Unsupported(_))) => Ok(None),
        Err(e) => Err(e),
    }
}

/// What to open. `head`/`base` come from the workstream; the rest is the form.
#[derive(Clone, Debug, Default)]
pub struct PrRequest {
    pub title: String,
    pub body: String,
    pub draft: bool,
    pub reviewers: Vec<String>,
    pub labels: Vec<String>,
}

/// Open a pull request for the workstream's branch — through the Publish gate.
///
/// A pull request is opened on pushed work. A branch that is not on the remote
/// yet is pushed first, **under the same gate**: one question — "push X and
/// open a pull request for it" — one decision, and nothing leaves the machine
/// before it. The record is reconciled with the checkout first
/// ([`reconcile_before_publish`]), so a commit made in a terminal counts; a
/// branch with nothing beyond its base is refused before anything is asked.
pub async fn open_pr(
    inner: &Inner,
    id: WorkstreamId,
    req: PrRequest,
) -> Result<PullRequest, EngineError> {
    let w = inner.ws.get_workstream(id)?;
    let (branch, base) = require_worktree(&w)?;
    let path = inner.ws.workstream_checkout(&w)?;
    let status = blocking({
        let path = path.clone();
        move || git::status(&path)
    })
    .await?;
    let w = reconcile_before_publish(inner, &w, &status, &path).await?;
    let needs_push = match &w.state {
        WorkstreamState::Committed => true,
        WorkstreamState::Pushed => false,
        WorkstreamState::Open | WorkstreamState::Dirty => {
            return Err(EngineError::NothingToPublish {
                workstream: w.id.to_string(),
                branch: branch.clone(),
                base: base.clone(),
            })
        }
        // A terminal state, or a pull request already open: the table's own
        // words, asked before the code host is touched — a refusal here costs
        // nothing, where one after `create_pr` would leave a pull request the
        // record does not know about.
        WorkstreamState::PrOpen { .. }
        | WorkstreamState::Merged { .. }
        | WorkstreamState::Closed => {
            w.apply(&WorkstreamTransition::PrOpened {
                number: 0,
                url: String::new(),
            })
            .map_err(bisa_store::StoreError::from)?;
            false
        }
    };
    let project = inner.ws.get_project(w.project)?;
    let what = if needs_push {
        format!("push {branch} and open a pull request for it")
    } else {
        format!("open a pull request for {branch}")
    };
    let asked = pass_publish_gate(inner, &w, &project, &what).await?;
    let opened = opened_after_the_gate(inner, &w, path, branch, base, needs_push, req).await;
    asked.heard(inner, &w, &what, opened)
}

/// What opening a pull request is once the gate is behind it: the push when
/// the branch is not on the remote yet, then the code host.
async fn opened_after_the_gate(
    inner: &Inner,
    w: &bisa_core::Workstream,
    path: std::path::PathBuf,
    branch: String,
    base: String,
    needs_push: bool,
    req: PrRequest,
) -> Result<PullRequest, EngineError> {
    let id = w.id;
    let w = if needs_push {
        push_branch(inner, w, &path, &branch).await?;
        inner.ws.get_workstream(id)?
    } else {
        w.clone()
    };
    let (host, repo) = code_host_for_path(inner, path).await?;
    let caps = host.capabilities();
    if req.draft && !caps.draft_prs {
        return Err(CodeHostError::Unsupported("draft pull requests".into()).into());
    }
    // The title and the body leave the machine: whoever wrote them, a secret
    // in them travels as a placeholder or not at all.
    let title = inner.security.redact_inbound(&req.title, "pull_request");
    let body = inner.security.redact_inbound(&req.body, "pull_request");
    let pr = host
        .create_pr(
            &repo,
            PrCreate {
                title,
                body,
                head: branch.clone(),
                base,
                draft: req.draft,
                reviewers: if caps.reviewers {
                    req.reviewers
                } else {
                    vec![]
                },
                labels: if caps.labels { req.labels } else { vec![] },
            },
        )
        .await?;
    transition(
        inner,
        &w,
        &WorkstreamTransition::PrOpened {
            number: pr.number,
            url: pr.url.clone(),
        },
    )?;
    journal_progress(inner, &w, "opened pr", &branch, Some(&pr.url));
    inner.ide_status.invalidate(id);
    inner.cache.invalidate_codehost(id);
    Ok(pr)
}

/// The open pull requests on the code host behind a project's `origin` —
/// what a workstream can be opened *from* (`WorkstreamSource::PullRequest`).
/// Read fresh each time: the picker asks on opening and on *Refresh*, and a
/// list is not the cache's shape (it holds one pull request per workstream).
pub async fn list_open_prs(
    inner: &Inner,
    project: ProjectId,
) -> Result<Vec<PullRequest>, EngineError> {
    let (_, root) = crate::projects::project_tree(inner, project)?;
    let (host, repo) = code_host_for_path(inner, root).await?;
    Ok(host
        .list_prs(
            &repo,
            PrFilter {
                state: Some(PrState::Open),
                head: None,
            },
        )
        .await?)
}

/// The pull request a workstream opened — or `None` when the workstream has
/// none. Served from the code host cache while it is fresh.
pub async fn linked_pr(
    inner: &Inner,
    id: WorkstreamId,
) -> Result<Option<PullRequest>, EngineError> {
    let w = inner.ws.get_workstream(id)?;
    // A merged branch still has a pull request, and it is the record of how
    // the work landed: the cockpit keeps showing it rather than blanking the
    // moment the merge succeeds.
    let Some(number) = pr_number(&w) else {
        return Ok(None);
    };
    // Served from the code host cache (`cache.codehost.ttl_ms`); a review,
    // merge or new PR invalidates it.
    let ttl = inner.cache.settings().codehost_ttl();
    let cache = inner.cache.codehost_pr();
    if let Some(pr) = cache.get(ttl, &id) {
        return Ok(pr);
    }
    let (host, repo) = code_host_for_path(inner, inner.ws.workstream_checkout(&w)?).await?;
    let pr = Some(host.get_pr(&repo, number).await?);
    if !ttl.is_zero() {
        cache.insert(id, pr.clone());
    }
    Ok(pr)
}

/// [`linked_pr`] asked of the code host whatever the cache holds — for a
/// reader whose whole point is to notice a change: the project events' poll
/// of pull request states. What it answers is what the cache serves next.
pub(crate) async fn linked_pr_fresh(
    inner: &Inner,
    id: WorkstreamId,
) -> Result<Option<PullRequest>, EngineError> {
    inner.cache.codehost_pr().invalidate(&id);
    linked_pr(inner, id).await
}

/// The pull request a workstream is linked to — open, or the one it merged
/// through — and nothing before one was opened.
fn pr_number(w: &bisa_core::Workstream) -> Option<u64> {
    match &w.state {
        WorkstreamState::PrOpen { number, .. } | WorkstreamState::Merged { number, .. } => {
            Some(*number)
        }
        _ => None,
    }
}

fn linked_number(w: &bisa_core::Workstream) -> Result<u64, EngineError> {
    pr_number(w).ok_or_else(|| {
        EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-workstream-has-no-pull-request",
            a0 = (w.id).to_string()
        ))
    })
}

/// The pull request a write acts on — merging one that already merged is not
/// a code host call worth making, and the state machine would refuse it after.
fn open_number(w: &bisa_core::Workstream) -> Result<u64, EngineError> {
    match &w.state {
        WorkstreamState::PrOpen { number, .. } => Ok(*number),
        _ => Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-workstream-has-no-open-pull-request",
            a0 = (w.id).to_string()
        ))),
    }
}

/// The check runs on the workstream's pull request.
pub async fn checks(inner: &Inner, id: WorkstreamId) -> Result<Vec<CheckRun>, EngineError> {
    let ttl = inner.cache.settings().codehost_ttl();
    let cache = inner.cache.codehost_checks();
    if let Some(runs) = cache.get(ttl, &id) {
        return Ok(runs);
    }
    let w = inner.ws.get_workstream(id)?;
    let number = linked_number(&w)?;
    let (host, repo) = code_host_for_path(inner, inner.ws.workstream_checkout(&w)?).await?;
    if !host.capabilities().check_runs {
        return Ok(vec![]);
    }
    let runs = host.checks(&repo, number).await?;
    if !ttl.is_zero() {
        cache.insert(id, runs.clone());
    }
    Ok(runs)
}

/// The first line of every review an agent submits: this text, then the
/// agent's id. One credential posts every review the platform posts, so the
/// code host cannot tell an agent's review from the person's — the body can.
/// The IDE's Review step reads it back (`reviewStepModel.reviewerOf`), and a
/// collaborator on the code host reads it as the plain sentence it is.
pub const AGENT_REVIEW_MARK: &str = "Reviewed by Bisa agent ";

/// The first line of every reply an agent leaves on a review thread: this
/// text, then the agent's id — the same argument as [`AGENT_REVIEW_MARK`],
/// with its own words, since *reviewed by* on a reply would be a lie. The
/// IDE's Comments rows read it back (`reviewStepModel.replierOf`).
pub const AGENT_REPLY_MARK: &str = "Reply by Bisa agent ";

/// Who is submitting a review through the platform's credential: the person
/// at the IDE (`POST …/pr/review`) or an agent in the workstream's chat
/// (the `pr_review_submit` MCP tool, which carries the agent's id).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reviewer {
    Person,
    Agent(String),
}

impl Reviewer {
    /// The review as the code host should take it: a person's untouched, an
    /// agent's opened with [`AGENT_REVIEW_MARK`] and its id — even when the
    /// agent said nothing beside an approval, so the attribution never drops.
    pub fn sign(&self, review: Review) -> Review {
        match self {
            Reviewer::Person => review,
            Reviewer::Agent(agent) => {
                let mark = format!("{AGENT_REVIEW_MARK}{agent}");
                let body = match review.words() {
                    Some(words) => format!("{mark}\n\n{words}"),
                    None => mark,
                };
                Review {
                    body: Some(body),
                    ..review
                }
            }
        }
    }

    /// A reply on a review thread as the code host should take it: a
    /// person's words untouched, an agent's opened with [`AGENT_REPLY_MARK`]
    /// and its id, a blank line, then the words.
    pub fn sign_reply(&self, words: &str) -> String {
        match self {
            Reviewer::Person => words.to_string(),
            Reviewer::Agent(agent) => format!("{AGENT_REPLY_MARK}{agent}\n\n{words}"),
        }
    }

    /// The journal's word for who reviewed.
    pub fn words(&self) -> String {
        match self {
            Reviewer::Person => "by you".to_string(),
            Reviewer::Agent(agent) => format!("by {agent}"),
        }
    }
}

/// Submit a review on the workstream's pull request. Not an outward *publish*
/// — a review changes nothing about what code exists where — so no gate.
///
/// One credential opens the pull request and reviews it, so the reviewer is
/// the author whenever the platform opened it — and a code host takes a
/// comment from the author but never an approval or a change request. Said
/// here, before the round trip, as [`EngineError::OwnPullRequest`], so a
/// screen can offer the right verdicts instead of relaying a refusal. An
/// agent's review is signed ([`Reviewer::sign`]) before it goes, so the IDE
/// can hold the order it asks for — the agent's review, then the person's.
pub async fn review(
    inner: &Inner,
    id: WorkstreamId,
    review: Review,
    reviewer: Reviewer,
) -> Result<(), EngineError> {
    let w = inner.ws.get_workstream(id)?;
    let number = linked_number(&w)?;
    let (host, repo) = code_host_for_path(inner, inner.ws.workstream_checkout(&w)?).await?;
    if review.is_verdict() {
        let pr = host.get_pr(&repo, number).await?;
        let viewer = host.account().await?.login;
        if pr.author.as_deref() == Some(viewer.as_str()) {
            return Err(EngineError::OwnPullRequest { author: viewer });
        }
    }
    host.submit_review(&repo, number, reviewer.sign(review))
        .await?;
    inner.cache.invalidate_codehost(id);
    journal_progress(
        inner,
        &w,
        "reviewed pr",
        &number.to_string(),
        Some(&reviewer.words()),
    );
    Ok(())
}

/// The submitted reviews and resolvable threads on the workstream's PR,
/// served from the code host cache; a code host without the capability answers empty.
pub async fn pr_reviews(inner: &Inner, id: WorkstreamId) -> Result<PrReviews, EngineError> {
    let ttl = inner.cache.settings().codehost_ttl();
    let cache = inner.cache.codehost_reviews();
    if let Some(hit) = cache.get(ttl, &id) {
        return Ok(hit);
    }
    let w = inner.ws.get_workstream(id)?;
    let number = linked_number(&w)?;
    let (host, repo) = code_host_for_path(inner, inner.ws.workstream_checkout(&w)?).await?;
    if !host.capabilities().review_threads {
        return Ok(PrReviews::default());
    }
    let reviews = host.pr_reviews(&repo, number).await?;
    if !ttl.is_zero() {
        cache.insert(id, reviews.clone());
    }
    Ok(reviews)
}

/// Where the workstream's pull request lives, as the content screen names a
/// source: the code host, the repository and the number.
pub async fn review_source(
    inner: &Inner,
    id: WorkstreamId,
) -> Result<crate::content::ContentSource, EngineError> {
    let w = inner.ws.get_workstream(id)?;
    let number = linked_number(&w)?;
    let (_host, repo) = code_host_for_path(inner, inner.ws.workstream_checkout(&w)?).await?;
    Ok(crate::content::ContentSource::CodeHost {
        host: repo.host.clone(),
        repo: repo.slug(),
        number,
    })
}

/// Every word the code host said on the pull request, as one text the
/// content screen reads: the reviews' bodies, then each thread's comments.
pub fn review_words(reviews: &PrReviews) -> String {
    let mut out = String::new();
    for r in &reviews.reviews {
        if !r.body.trim().is_empty() {
            out.push_str(&format!(
                "review by {} ({}):\n{}\n\n",
                r.author.as_deref().unwrap_or("someone"),
                r.state,
                r.body.trim()
            ));
        }
    }
    for t in &reviews.threads {
        for c in &t.comments {
            if !c.body.trim().is_empty() {
                out.push_str(&format!(
                    "comment by {} on {}:\n{}\n\n",
                    c.author.as_deref().unwrap_or("someone"),
                    t.path.as_deref().unwrap_or("the pull request"),
                    c.body.trim()
                ));
            }
        }
    }
    out
}

/// Resolve or unresolve a review thread on the workstream's PR. Not
/// an outward publish, so no gate — like [`review`]; the reviews cache drops.
pub async fn resolve_review_thread(
    inner: &Inner,
    id: WorkstreamId,
    thread_id: &str,
    resolved: bool,
) -> Result<(), EngineError> {
    let w = inner.ws.get_workstream(id)?;
    let (host, _repo) = code_host_for_path(inner, inner.ws.workstream_checkout(&w)?).await?;
    host.resolve_review_thread(thread_id, resolved).await?;
    inner.cache.invalidate_codehost(id);
    Ok(())
}

/// Reply on a review thread of the workstream's PR — and resolve it in the
/// same act when asked, which is what *Reply and resolve* on the desktop and
/// an agent's `resolve: true` mean. A reply with no words is refused before
/// any request. An agent's reply is signed ([`Reviewer::sign_reply`]) so the
/// IDE and a collaborator can tell it from the person's — one credential
/// posts both. Not an outward publish, so no gate — like [`review`]; the
/// reviews cache drops once, after both calls.
pub async fn reply_review_thread(
    inner: &Inner,
    id: WorkstreamId,
    thread_id: &str,
    body: &str,
    resolve: bool,
    reviewer: Reviewer,
) -> Result<(), EngineError> {
    let words = body.trim();
    if words.is_empty() {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-reply-needs-words-say-what-you-changed"
        )));
    }
    let w = inner.ws.get_workstream(id)?;
    // No pull request, no thread to reply on: said before a host is asked.
    linked_number(&w)?;
    let (host, _repo) = code_host_for_path(inner, inner.ws.workstream_checkout(&w)?).await?;
    host.reply_review_thread(thread_id, &reviewer.sign_reply(words))
        .await?;
    if resolve {
        host.resolve_review_thread(thread_id, true).await?;
    }
    inner.cache.invalidate_codehost(id);
    journal_progress(
        inner,
        &w,
        if resolve {
            "replied on pr thread and resolved it"
        } else {
            "replied on pr thread"
        },
        thread_id,
        Some(&reviewer.words()),
    );
    Ok(())
}

/// Merge the workstream's pull request — **through the Publish gate**: a merge
/// leaves the machine and cannot be recalled, which is the gate's own test.
/// The first writer of `WorkstreamTransition::Merged`.
///
/// `delete_branch` asks the code host to delete the head branch on the remote
/// once the merge landed (the merge dialog's toggle, defaulting from
/// `git.delete_branch_after_merge`); the outcome says whether it did. The
/// local branch and checkout are the cleanup step's, not this call's.
pub async fn merge(
    inner: &Inner,
    id: WorkstreamId,
    strategy: MergeStrategy,
    delete_branch: bool,
) -> Result<MergeOutcome, EngineError> {
    let w = inner.ws.get_workstream(id)?;
    let number = open_number(&w)?;
    let project = inner.ws.get_project(w.project)?;
    let what = format!("merge pull request #{number}");
    let asked = pass_publish_gate(inner, &w, &project, &what).await?;
    let merged = merged_after_the_gate(inner, &w, number, strategy, delete_branch).await;
    asked.heard(inner, &w, &what, merged)
}

/// What a merge is once the gate is behind it: the code host's merge, the
/// record moved, the branch on the code host deleted when asked.
async fn merged_after_the_gate(
    inner: &Inner,
    w: &bisa_core::Workstream,
    number: u64,
    strategy: MergeStrategy,
    delete_branch: bool,
) -> Result<MergeOutcome, EngineError> {
    let id = w.id;
    let (host, repo) = code_host_for_path(inner, inner.ws.workstream_checkout(w)?).await?;
    let caps = host.capabilities();
    if !caps.merge_strategies.contains(&strategy) {
        return Err(CodeHostError::Unsupported(format!("merge strategy {strategy:?}")).into());
    }
    let merged = host.merge(&repo, number, strategy).await;
    // Whatever the code host said, what was cached of this pull request is
    // older than its answer: a refusal often means it moved — closed, merged
    // by somebody else, a check that failed since.
    inner.cache.invalidate_codehost(id);
    let mut out = merged.inspect_err(|e| {
        tracing::info!(target: "bisa_engine::codehost", workstream = %w.id, pr = number, "the code host did not merge: {e}");
    })?;
    if out.merged {
        transition(inner, w, &WorkstreamTransition::Merged)?;
        journal_progress(
            inner,
            w,
            "merged pr",
            &number.to_string(),
            out.sha.as_deref(),
        );
        if delete_branch {
            // A failure here is reported, never fatal: the merge already
            // happened, and the branch can be deleted on the code host by hand.
            let deleted = match (caps.delete_branch, w.branch()) {
                (true, Some(branch)) => match host.delete_branch(&repo, branch).await {
                    Ok(()) => {
                        journal_progress(inner, w, "deleted remote branch", branch, None);
                        true
                    }
                    Err(e) => {
                        tracing::warn!(workstream = %w.id, "remote branch not deleted: {e}");
                        false
                    }
                },
                _ => false,
            };
            out.remote_branch_deleted = Some(deleted);
        }
        inner.ide_status.invalidate(id);
    }
    Ok(out)
}

// --- the accounts, per kind ---------------------------------------------------
//
// The credential chain (ide/08 §Credentials), one per kind: the kind's
// environment variable, the accounts stored from Settings — a login each —
// the kind's CLI signed in on this machine, and git's own credential helper
// for the host — the last two asked, never read from a file or a key of
// anyone's. Which account a checkout uses is git config's to say
// (`codehost.account`: a profile's or a local pin's; else the kind's
// `codehost.<kind>.account`); whether a credential *works* is the host's, so
// the check is a request — `CodeHost::account` — and a token is verified
// before it is kept.

/// The code host's port to git's credential helpers, over the engine's own
/// `git` handle — so a test's isolated git is what gets asked, and the
/// developer's keychain never is.
#[derive(Debug)]
struct GitHelperCredentials(bisa_vcs::Git);

#[async_trait::async_trait]
impl bisa_codehost::creds::GitCredentials for GitHelperCredentials {
    async fn fill(&self, host: &str) -> Option<bisa_codehost::creds::Credential> {
        let git = Clone::clone(&self.0);
        let host = host.to_string();
        tokio::task::spawn_blocking(move || git.credential_fill("https", &host).ok().flatten())
            .await
            .ok()
            .flatten()
            .map(|c| bisa_codehost::creds::Credential {
                username: c.username,
                password: bisa_codehost::creds::Secret::new(c.password),
            })
    }

    async fn helpers(&self) -> Vec<String> {
        let git = Clone::clone(&self.0);
        tokio::task::spawn_blocking(move || git.credential_helpers().unwrap_or_default())
            .await
            .unwrap_or_default()
    }
}

/// Git's credential helpers as the chain's last token source, over `git`.
pub fn git_credentials(git: &bisa_vcs::Git) -> Arc<dyn bisa_codehost::creds::GitCredentials> {
    Arc::new(GitHelperCredentials(Clone::clone(git)))
}

/// The real code host CLIs on `PATH` — what the node that serves a person
/// hands `EngineConfig::cli`, and no test fixture ever does.
pub fn default_cli(http: Arc<bisa_http::Clients>) -> Arc<dyn bisa_codehost::cli::CliRunner> {
    Arc::new(bisa_codehost::cli::Cli::default().with_http(http))
}

/// A kind's token store, at its public host — the one Settings is about.
fn store_for(inner: &Inner, kind: CodeHostKind) -> bisa_codehost::creds::TokenStore {
    inner.hosts.store_for(kind, kind.public_host())
}

/// The registered host of a kind — or, in a test engine given one in-memory
/// code host and no real ones, that one, so the accounts path runs against
/// the fake — else the typed word for its absence.
fn host_of(inner: &Inner, kind: CodeHostKind) -> Result<Arc<dyn CodeHost>, EngineError> {
    inner
        .code_hosts
        .by_kind(kind)
        .or_else(|| match inner.code_hosts.all() {
            [only] if kind_of(only.id()).is_none() => Some(Arc::clone(only)),
            _ => None,
        })
        .ok_or_else(|| {
            CodeHostError::Unsupported(format!("this build has no {} code host", kind.label()))
                .into()
        })
}

/// The accounts of one kind as Settings shows them: the stored logins with
/// their source, whether the environment overrides them all, git's helpers and
/// the username they hold, the login the CLI is signed in as, and the
/// **default** account — the global `codehost.<kind>.account`. Never a token;
/// no request to the host.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct AccountsView {
    pub kind: CodeHostKind,
    /// The instance these accounts are for — the kind's public host.
    pub host: String,
    #[serde(flatten)]
    pub status: AccountsStatus,
    /// The login a checkout with no profile and no pin uses. `None` means the
    /// chain's own choice: the one stored account, the CLI's, git's helper's.
    pub default: Option<String>,
}

pub async fn accounts(inner: &Inner, kind: CodeHostKind) -> Result<AccountsView, EngineError> {
    let status = bisa_codehost::creds::accounts_status(&store_for(inner, kind)).await;
    let default = default_account(inner, kind).await?;
    Ok(AccountsView {
        kind,
        host: kind.public_host().to_string(),
        status,
        default,
    })
}

/// Ask the host whose `login`'s credential is — *Connected as @login ·
/// organizations …*, or why not. A login with no credential anywhere in the
/// chain is `NoToken` without a request.
pub async fn check_account(
    inner: &Inner,
    kind: CodeHostKind,
    login: &str,
) -> Result<Connection, EngineError> {
    let store = store_for(inner, kind);
    if bisa_codehost::creds::token_source(&store, Some(login))
        .await
        .is_none()
    {
        return Ok(Connection::NoToken);
    }
    let host = host_of(inner, kind)?.for_account(Some(login));
    let (required, recommended) = scopes_for(kind);
    Ok(Connection::of(host.account().await, required, recommended))
}

/// The host's own record of `login` — the person behind the account, as
/// the host says: name, public email, id. Read live; a host that cannot be
/// reached is the caller's `Err`.
pub(crate) async fn account_of(
    inner: &Inner,
    kind: CodeHostKind,
    login: &str,
) -> Result<bisa_codehost::Account, EngineError> {
    let host = host_of(inner, kind)?.for_account(Some(login));
    Ok(host.account().await?)
}

/// Add an account: the token is verified with the host **first** and stored
/// under the login the host answers — a refused token is never kept, and
/// nobody types who a token belongs to (Bitbucket's Basic credential is the
/// one exception: its `login` is the person's, checked beside the token).
/// Answers the login and its connection.
pub async fn add_account(
    inner: &Inner,
    kind: CodeHostKind,
    token: &str,
    login: Option<&str>,
) -> Result<(String, Connection), EngineError> {
    let account = host_of(inner, kind)?.verify_token(token, login).await?;
    let login = account.login.to_ascii_lowercase();
    store_for(inner, kind).store(&login, token)?;
    accounts_changed(inner);
    let (required, recommended) = scopes_for(kind);
    Ok((login, Connection::of(Ok(account), required, recommended)))
}

/// Forget one account's token. A login never stored is not an error.
pub fn forget_account(inner: &Inner, kind: CodeHostKind, login: &str) -> Result<(), EngineError> {
    store_for(inner, kind).forget(login)?;
    accounts_changed(inner);
    Ok(())
}

/// Set — or clear, with `None` — a kind's default account: the global
/// `codehost.<kind>.account`, a schema key written through `config_set` like
/// any other global key (I45), from Settings › Git & code hosts and nowhere
/// else. The platform's profile includes are re-appended afterwards so they
/// keep the last word. Answers the default as git now reads it.
pub async fn set_default_account(
    inner: &Inner,
    kind: CodeHostKind,
    login: Option<&str>,
) -> Result<Option<String>, EngineError> {
    let git = inner.git();
    let profiles = inner.ws.paths().git_profiles_dir();
    let login = login.map(str::to_string);
    let key = bisa_vcs::default_account_key(kind.as_str()).ok_or_else(|| {
        EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-has-no-default-account-key",
            kind = kind.to_string()
        ))
    })?;
    let default = blocking(move || {
        match &login {
            Some(l) => git.config_set(ConfigScope::Global, None, key, l)?,
            None => git.config_unset(ConfigScope::Global, None, key)?,
        }
        crate::gitprofiles::reappend_includes(&git, &profiles)?;
        git.default_account(kind.as_str())
    })
    .await?;
    accounts_changed(inner);
    Ok(default)
}

/// An account was added, forgotten or made the default: what every checkout
/// cached of its pull request was read as the account before, so it is
/// dropped, and the surfaces that list accounts are told.
fn accounts_changed(inner: &Inner) {
    inner.cache.clear_codehost();
    inner.emit(crate::events::EngineEvent::global(
        EnginePayload::GitSetupChanged {
            what: GitSetup::Accounts,
        },
    ));
}

// --- health, sign-in, inspection -----------------------------------------------

/// Who would answer for a kind with nothing named, and from where.
#[derive(
    Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct Resolves {
    /// The login, when the source knows one (the environment does not).
    pub login: Option<String>,
    pub source: TokenSource,
}

/// One kind's health as Settings shows it (`GET /codehost/{kind}/health`):
/// the CLI — installed, version, signed in as whom — the stored accounts,
/// git's helper, the default, and who would answer with nothing named. All
/// offline: the CLI's `auth status` is the one thing asked of a program, and a
/// CLI that answers over the network is time-boxed. Never a token.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct CodeHostHealth {
    pub kind: CodeHostKind,
    pub host: String,
    pub label: String,
    /// The kind's CLI, when it has one; `None` for Bitbucket.
    pub cli: Option<CliProbe>,
    #[serde(flatten)]
    pub accounts: AccountsStatus,
    pub default: Option<String>,
    pub resolves: Option<Resolves>,
    /// Where a person makes a token for the API layer.
    pub token_page: String,
}

pub async fn health(inner: &Inner, kind: CodeHostKind) -> Result<CodeHostHealth, EngineError> {
    let host = kind.public_host();
    let store = store_for(inner, kind);
    let cli = match inner.hosts.cli_for(kind, host) {
        Some(cli) => Some(cli.probe().await),
        None => None,
    };
    let accounts = bisa_codehost::creds::accounts_status(&store).await;
    let default = default_account(inner, kind).await?;
    let resolves = match bisa_codehost::creds::token_source(&store, default.as_deref()).await {
        Some(source) => Some(Resolves {
            login: match source {
                TokenSource::Env => None,
                TokenSource::File | TokenSource::Keyring => default
                    .clone()
                    .or_else(|| accounts.accounts.first().map(|a| a.login.clone())),
                TokenSource::Cli => accounts.cli_login.clone(),
                TokenSource::Git => accounts.helper_username.clone(),
            },
            source,
        }),
        None => None,
    };
    Ok(CodeHostHealth {
        kind,
        host: host.to_string(),
        label: kind.label().to_string(),
        cli,
        accounts,
        default,
        resolves,
        token_page: kind.token_page(host),
    })
}

/// How a person signs in to a kind from Settings (`GET /codehost/{kind}/login`):
/// the CLI's own browser login, run in a terminal by the desktop shell, when
/// the CLI is installed; the way to install it when it is not; a token from
/// the host's page for a kind with no CLI — and as the second door everywhere.
#[derive(
    Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LoginPlan {
    Cli {
        program: CliProgram,
        host: String,
        words: String,
        token_page: String,
    },
    Install {
        program: CliProgram,
        hints: InstallHints,
        words: String,
        token_page: String,
    },
    Token {
        token_page: String,
        words: String,
    },
}

pub async fn login_plan(inner: &Inner, kind: CodeHostKind) -> LoginPlan {
    let host = kind.public_host();
    let token_page = kind.token_page(host);
    let Some(program) = kind.cli() else {
        return LoginPlan::Token {
            token_page,
            words: format!(
                "{} has no CLI of its own: make an API token on {} and add it here with the login it belongs to.",
                kind.label(),
                kind.label()
            ),
        };
    };
    let installed = match inner.hosts.cli_for(kind, host) {
        Some(cli) => cli.probe().await.installed,
        None => false,
    };
    if installed {
        LoginPlan::Cli {
            program,
            host: host.to_string(),
            words: format!(
                "The {} signs you in through your browser: a terminal opens with `{} auth login`, shows a one-time code, and opens {} for you. The credential stays with the CLI; this platform uses it from there.",
                program.label(),
                program.binary(),
                kind.label()
            ),
            token_page,
        }
    } else {
        LoginPlan::Install {
            program,
            hints: program.install_hints(),
            words: format!(
                "Install the {} and sign in with it — one credential for git, the CLI and this platform. Or add a token from {} below.",
                program.label(),
                kind.label()
            ),
            token_page,
        }
    }
}

/// What to inspect: a URL a person is about to clone, or a folder they picked.
#[derive(Clone, Debug)]
pub enum InspectSource {
    Url(String),
    Path(std::path::PathBuf),
}

/// Where an account choice for a new repository would come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AccountChoiceSource {
    Profile,
    Default,
    Cli,
    Stored,
    GitHelper,
}

/// One account a new repository could speak as.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, schemars::JsonSchema)]
pub struct AccountChoice {
    pub login: String,
    pub source: AccountChoiceSource,
    pub note: String,
}

/// The code host an inspected remote is on.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, schemars::JsonSchema)]
pub struct InspectedHost {
    pub kind: CodeHostKind,
    pub host: String,
    pub label: String,
}

/// What a remote is before a project exists (`POST /codehost/inspect`): the
/// URL taken apart with its SSH alias resolved, the code host, the profile it
/// falls under, every account that could speak for it and the one suggested,
/// and what a person would want to know. Offline: no request leaves.
#[derive(Clone, Debug, serde::Serialize, schemars::JsonSchema)]
pub struct RemoteInspection {
    pub remote: Option<crate::ide::connection::RemoteFacts>,
    pub code_host: Option<InspectedHost>,
    pub profile: Option<crate::ide::connection::ProfileFacts>,
    pub accounts: Vec<AccountChoice>,
    pub suggested: Option<String>,
    pub cautions: Vec<String>,
}

pub async fn inspect(
    inner: &Inner,
    source: InspectSource,
) -> Result<RemoteInspection, EngineError> {
    let raw = match source {
        InspectSource::Url(url) => {
            let url = url.trim().to_string();
            if url.is_empty() {
                return Err(EngineError::Invalid(bisa_core::text!(
                    "error-engine-invalid-url-inspect"
                )));
            }
            Some(url)
        }
        InspectSource::Path(path) => {
            let git = inner.git();
            blocking(move || {
                if !bisa_vcs::git::is_repo(&path) {
                    return Ok(None);
                }
                git.remote_get(&path, "origin")
            })
            .await?
        }
    };
    let Some(raw) = raw else {
        return Ok(RemoteInspection {
            remote: None,
            code_host: None,
            profile: None,
            accounts: vec![],
            suggested: None,
            cautions: vec![],
        });
    };
    let parsed = RemoteUrl::parse(&raw);
    let resolved = resolve_alias(inner, parsed.clone()).await;
    let remote = Some(crate::ide::connection::remote_facts(&parsed, &resolved));
    let detected = inner
        .code_hosts
        .detect(&resolved)
        .and_then(|(h, _)| kind_of(h.id()));
    let mut cautions = Vec::new();
    let Some(kind) = detected else {
        if resolved.host.as_deref().is_some_and(|h| h.contains('.'))
            && !matches!(resolved.protocol, RemoteProtocol::Local)
        {
            cautions.push(format!(
                "{} is not a code host this build knows by name — a GitHub Enterprise or a self-hosted GitLab is told apart by `codehost.kind` on the repository once it exists",
                resolved.host.as_deref().unwrap_or("this host")
            ));
        }
        return Ok(RemoteInspection {
            remote,
            code_host: None,
            profile: None,
            accounts: vec![],
            suggested: None,
            cautions,
        });
    };
    let host = resolved
        .host
        .clone()
        .unwrap_or_else(|| kind.public_host().to_string());
    let profile = crate::gitprofiles::matching(inner, &raw)
        .await
        .ok()
        .flatten();
    let store = inner.hosts.store_for(kind, &host);
    let status = bisa_codehost::creds::accounts_status(&store).await;
    let default = default_account(inner, kind).await?;

    let mut accounts: Vec<AccountChoice> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut offer = |login: &str, source: AccountChoiceSource, note: String| {
        if seen.insert(login.to_ascii_lowercase()) {
            accounts.push(AccountChoice {
                login: login.to_string(),
                source,
                note,
            });
        }
    };
    if let Some(p) = &profile {
        if let Some(login) = &p.profile.account {
            offer(
                login,
                AccountChoiceSource::Profile,
                format!("from the {} profile", p.profile.label),
            );
        }
    }
    if let Some(login) = &default {
        offer(
            login,
            AccountChoiceSource::Default,
            format!("the default {} account", kind.label()),
        );
    }
    if let Some(login) = &status.cli_login {
        offer(
            login,
            AccountChoiceSource::Cli,
            format!(
                "signed in with the {}",
                kind.cli().map(|c| c.label()).unwrap_or("CLI")
            ),
        );
    }
    for a in &status.accounts {
        offer(
            &a.login,
            AccountChoiceSource::Stored,
            "a stored token".to_string(),
        );
    }
    if let Some(login) = &status.helper_username {
        offer(
            login,
            AccountChoiceSource::GitHelper,
            "git's credential helper".to_string(),
        );
    }
    let suggested = accounts.first().map(|a| a.login.clone());
    if suggested.is_none() && !status.env_override {
        cautions.push(format!(
            "nobody is signed in to {label}: pull requests cannot be opened until you sign in under Settings › Git & code hosts › {label}",
            label = kind.label()
        ));
    }
    Ok(RemoteInspection {
        remote,
        code_host: Some(InspectedHost {
            kind,
            host,
            label: kind.label().to_string(),
        }),
        profile: profile.map(|p| crate::ide::connection::ProfileFacts {
            slug: p.profile.slug.to_string(),
            label: p.profile.label.clone(),
            name: p.profile.name.clone(),
            email: p.profile.email.clone(),
            ssh_key: p.profile.ssh_key.clone(),
            account: p.profile.account.clone(),
        }),
        accounts,
        suggested,
        cautions,
    })
}

/// The code host a repository at `root` is on, for the project record —
/// offline, from `origin` alone; `None` when there is no remote or no host
/// this build knows.
pub(crate) async fn record_for(
    inner: &Inner,
    root: &std::path::Path,
) -> Option<bisa_core::project::CodeHost> {
    let origin = origin_of(inner, root).await.ok()?;
    let remote = origin.remote?;
    let resolved = resolve_alias(inner, remote).await;
    let (host, repo) = inner.code_hosts.detect_as(&resolved, origin.kind)?;
    // A test's fake is not a kind; the record names only the three.
    let kind = kind_of(host.id())?;
    Some(bisa_core::project::CodeHost {
        kind: kind.as_str().to_string(),
        host: repo.host.clone(),
        owner_repo: repo.slug(),
    })
}
