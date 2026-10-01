//! GitLab through `glab`, the machine's own CLI: porcelain where it has a verb
//! (`mr create/view/list/approve/note/merge`), `glab api` for the rest —
//! pipelines and jobs, approvals, notes and discussions, resolving one,
//! deleting a branch — every call authenticated by the CLI itself. A host
//! other than gitlab.com is named through `GITLAB_HOST`. `glab` holds one
//! account per host, so a host bound to another login falls through to the
//! API with the stored token.

use super::{
    classify_failure, parse_glab_auth_status, parse_version, run, CliAccount, CliOutcome, CliProbe,
    CliProgram, CliRunner, CliStep, CodeHostCli, Fallback, Output, Timeouts,
};
use crate::creds::{CliToken, Secret};
use crate::gitlab::{
    encode, inline_discussion, parse_thread_id, project, reviews_of, state_query, GlApprovals,
    GlDiscussion, GlJob, GlMergeRequest, GlNote, GlPipeline, GlUser, HOST,
};
use crate::{
    Account, CheckRun, CodeHostError, MergeOutcome, MergeStrategy, PrCreate, PrFilter, PrReviews,
    PullRequest, RepoAccess, RepoRef, Review, ReviewEvent,
};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct GlabCli {
    runner: Arc<dyn CliRunner>,
    host: String,
    account: Option<String>,
    timeouts: Timeouts,
}

impl GlabCli {
    pub fn new(runner: Arc<dyn CliRunner>, host: &str) -> Self {
        Self {
            runner,
            host: host.trim().to_ascii_lowercase(),
            account: None,
            timeouts: Timeouts::default(),
        }
    }

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    fn repo_flag(repo: &RepoRef) -> [String; 2] {
        ["--repo".to_string(), repo.slug()]
    }

    fn env(&self) -> Vec<(String, String)> {
        if self.host != HOST {
            vec![("GITLAB_HOST".to_string(), self.host.clone())]
        } else {
            Vec::new()
        }
    }

    async fn status(&self) -> Vec<CliAccount> {
        let args = Self::args(&["auth", "status", "--hostname", &self.host]);
        match run(
            &self.runner,
            CliProgram::Glab,
            args,
            self.env(),
            None,
            self.timeouts.network,
        )
        .await
        {
            Ok(out) => parse_glab_auth_status(&format!("{}\n{}", out.stdout, out.stderr)),
            Err(_) => Vec::new(),
        }
    }

    /// Whether the CLI speaks as the account this host is bound to: `glab`
    /// holds one account per host, so another login is a fallback.
    async fn bound_ok(&self) -> CliOutcome<()> {
        let Some(login) = &self.account else {
            return Ok(());
        };
        match self.status().await.into_iter().find(|a| a.active) {
            Some(a) if a.login.eq_ignore_ascii_case(login) => Ok(()),
            _ => Err(CliStep::Fallback(Fallback::NoSuchAccount(login.clone()))),
        }
    }

    async fn glab(
        &self,
        args: Vec<String>,
        stdin: Option<Vec<u8>>,
        network: bool,
        what: &str,
    ) -> CliOutcome<Output> {
        if self.runner.installed(CliProgram::Glab).is_none() {
            return Err(CliStep::Fallback(Fallback::NotInstalled));
        }
        self.bound_ok().await?;
        let timeout = if network {
            self.timeouts.network
        } else {
            self.timeouts.local
        };
        let out = run(
            &self.runner,
            CliProgram::Glab,
            args,
            self.env(),
            stdin,
            timeout,
        )
        .await?;
        if !out.success() {
            return Err(classify_failure(what, &out));
        }
        Ok(out)
    }

    fn json<T: serde::de::DeserializeOwned>(out: &Output, what: &str) -> CliOutcome<T> {
        serde_json::from_str(&out.stdout)
            .map_err(|e| CliStep::Fallback(Fallback::Unparsable(format!("{what}: {e}"))))
    }

    /// `glab api <path>` with a method and an optional JSON body on stdin.
    async fn api<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
        what: &str,
    ) -> CliOutcome<T> {
        let mut args = Self::args(&["api", "--method", method, path]);
        let stdin = body.map(|b| {
            args.push("--input".into());
            args.push("-".into());
            serde_json::to_vec(&b).unwrap_or_default()
        });
        let out = self.glab(args, stdin, true, what).await?;
        if out.stdout.trim().is_empty() {
            return Self::json(
                &Output {
                    code: 0,
                    stdout: "null".into(),
                    stderr: String::new(),
                },
                what,
            );
        }
        Self::json(&out, what)
    }

    async fn view(&self, repo: &RepoRef, iid: u64) -> CliOutcome<GlMergeRequest> {
        let [r, slug] = Self::repo_flag(repo);
        let args = Self::args(&[
            "mr",
            "view",
            &iid.to_string(),
            &r,
            &slug,
            "--output",
            "json",
        ]);
        let out = self.glab(args, None, true, "read merge request").await?;
        Self::json(&out, "read merge request")
    }
}

/// The merge request number at the end of the URL `glab mr create` prints (`…/-/merge_requests/7`).
fn iid_of_url(text: &str) -> Option<u64> {
    text.lines()
        .rev()
        .map(str::trim)
        .find(|l| l.contains("/merge_requests/"))
        .and_then(|l| l.rsplit('/').next())
        .and_then(|n| {
            n.trim_end_matches(|c: char| !c.is_ascii_digit())
                .parse()
                .ok()
        })
}

#[async_trait::async_trait]
impl CliToken for GlabCli {
    /// `glab auth status --show-token` prints the token on its own line; read
    /// into a secret at once. `glab` has one account per host: a login it is
    /// not signed in as has no token here.
    async fn token(&self, login: Option<&str>) -> Option<(String, Secret)> {
        self.runner.installed(CliProgram::Glab)?;
        let active = self.status().await.into_iter().find(|a| a.active)?;
        if login.is_some_and(|l| !l.eq_ignore_ascii_case(&active.login)) {
            return None;
        }
        let args = Self::args(&["auth", "status", "--hostname", &self.host, "--show-token"]);
        let out = run(
            &self.runner,
            CliProgram::Glab,
            args,
            self.env(),
            None,
            self.timeouts.network,
        )
        .await
        .ok()?;
        let text = format!("{}\n{}", out.stdout, out.stderr);
        let token = text
            .lines()
            .map(|l| l.trim().trim_start_matches(['✓', '✗', '-', '!']).trim())
            .find_map(|l| l.strip_prefix("Token:"))
            .and_then(|t| Secret::some(t.trim().trim_matches('*')))?;
        Some((active.login, token))
    }

    async fn active_login(&self) -> Option<String> {
        self.runner.installed(CliProgram::Glab)?;
        self.status()
            .await
            .into_iter()
            .find(|a| a.active)
            .map(|a| a.login)
    }
}

#[async_trait::async_trait]
impl CodeHostCli for GlabCli {
    fn program(&self) -> CliProgram {
        CliProgram::Glab
    }

    fn host(&self) -> &str {
        &self.host
    }

    fn for_account(self: Arc<Self>, login: Option<&str>) -> Arc<dyn CodeHostCli> {
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

    async fn probe(&self) -> CliProbe {
        let Some(path) = self.runner.installed(CliProgram::Glab) else {
            return CliProbe::not_installed(CliProgram::Glab);
        };
        let version = match run(
            &self.runner,
            CliProgram::Glab,
            Self::args(&["--version"]),
            self.env(),
            None,
            self.timeouts.local,
        )
        .await
        {
            Ok(out) => parse_version(&out.stdout),
            Err(_) => None,
        };
        let args = Self::args(&["auth", "status", "--hostname", &self.host]);
        let (accounts, detail) = match run(
            &self.runner,
            CliProgram::Glab,
            args,
            self.env(),
            None,
            self.timeouts.network,
        )
        .await
        {
            Ok(out) => {
                let text = format!("{}\n{}", out.stdout, out.stderr);
                let accounts = parse_glab_auth_status(&text);
                let detail = accounts
                    .is_empty()
                    .then(|| {
                        text.lines()
                            .map(str::trim)
                            .find(|l| !l.is_empty())
                            .map(str::to_string)
                    })
                    .flatten();
                (accounts, detail)
            }
            Err(e) => (Vec::new(), Some(e.to_string())),
        };
        CliProbe {
            program: CliProgram::Glab,
            installed: true,
            path: Some(path.display().to_string()),
            version,
            accounts,
            detail,
        }
    }

    async fn account(&self) -> CliOutcome<Account> {
        let user: GlUser = self.api("GET", "user", None, "check the account").await?;
        let organizations: Vec<String> = match self
            .api::<Vec<serde_json::Value>>(
                "GET",
                "groups?min_access_level=10&per_page=100&top_level_only=true",
                None,
                "read groups",
            )
            .await
        {
            Ok(list) => list
                .into_iter()
                .filter_map(|g| g["full_path"].as_str().map(str::to_string))
                .collect(),
            Err(_) => Vec::new(),
        };
        Ok(Account {
            login: user.username,
            scopes: Vec::new(),
            organizations,
            name: user.name.filter(|n| !n.trim().is_empty()),
            email: user.public_email.filter(|e| !e.trim().is_empty()),
            id: user.id,
        })
    }

    async fn repo_access(&self, repo: &RepoRef) -> CliOutcome<RepoAccess> {
        match self
            .api::<serde_json::Value>(
                "GET",
                &format!("projects/{}", project(repo)),
                None,
                "read repository access",
            )
            .await
        {
            Ok(v) => {
                let level =
                    |path: &str| v["permissions"][path]["access_level"].as_u64().unwrap_or(0);
                Ok(RepoAccess {
                    found: true,
                    push: level("project_access").max(level("group_access")) >= 30,
                })
            }
            Err(CliStep::Host(CodeHostError::NotFound(_))) => Ok(RepoAccess {
                found: false,
                push: false,
            }),
            Err(e) => Err(e),
        }
    }

    /// `glab mr create` prints the new merge request's URL; the merge request
    /// is then read back with `mr view`. Once the create has succeeded, a
    /// failure to read it back is the host's, never a fallback.
    async fn create_pr(&self, repo: &RepoRef, req: PrCreate) -> CliOutcome<PullRequest> {
        let [r, slug] = Self::repo_flag(repo);
        let mut args = Self::args(&[
            "mr",
            "create",
            &r,
            &slug,
            "--title",
            &req.title,
            "--description",
            &req.body,
            "--source-branch",
            &req.head,
            "--target-branch",
            &req.base,
            "--yes",
        ]);
        if req.draft {
            args.push("--draft".into());
        }
        if !req.reviewers.is_empty() {
            args.push("--reviewer".into());
            args.push(req.reviewers.join(","));
        }
        if !req.labels.is_empty() {
            args.push("--label".into());
            args.push(req.labels.join(","));
        }
        let out = self.glab(args, None, true, "create merge request").await?;
        let iid = iid_of_url(&format!("{}\n{}", out.stdout, out.stderr)).ok_or_else(|| {
            CliStep::Host(CodeHostError::Transport(
                "create merge request: glab printed no merge request URL".into(),
            ))
        })?;
        self.view(repo, iid)
            .await
            .map(PullRequest::from)
            .map_err(|e| match e {
                CliStep::Fallback(why) => CliStep::Host(CodeHostError::Transport(format!(
                    "merge request !{iid} was created, but reading it back failed: {why}"
                ))),
                host => host,
            })
    }

    async fn get_pr(&self, repo: &RepoRef, number: u64) -> CliOutcome<PullRequest> {
        self.view(repo, number).await.map(PullRequest::from)
    }

    async fn list_prs(&self, repo: &RepoRef, filter: PrFilter) -> CliOutcome<Vec<PullRequest>> {
        let mut path = format!(
            "projects/{}/merge_requests?per_page=50&state={}",
            project(repo),
            state_query(filter.state)
        );
        if let Some(head) = &filter.head {
            path.push_str(&format!("&source_branch={}", encode(head)));
        }
        let mrs: Vec<GlMergeRequest> = self.api("GET", &path, None, "list merge requests").await?;
        Ok(mrs.into_iter().map(PullRequest::from).collect())
    }

    async fn checks(&self, repo: &RepoRef, number: u64) -> CliOutcome<Vec<CheckRun>> {
        let pipelines: Vec<GlPipeline> = self
            .api(
                "GET",
                &format!(
                    "projects/{}/merge_requests/{number}/pipelines",
                    project(repo)
                ),
                None,
                "read pipelines",
            )
            .await?;
        let Some(latest) = pipelines.first() else {
            return Ok(Vec::new());
        };
        let jobs: Vec<GlJob> = self
            .api(
                "GET",
                &format!(
                    "projects/{}/pipelines/{}/jobs?per_page=100",
                    project(repo),
                    latest.id
                ),
                None,
                "read jobs",
            )
            .await?;
        Ok(jobs.into_iter().map(CheckRun::from).collect())
    }

    async fn submit_review(&self, repo: &RepoRef, number: u64, review: Review) -> CliOutcome<()> {
        if review.needs_words() {
            return Err(CliStep::Host(CodeHostError::Refused(
                "submit review: a comment or a change request needs words — say what you noticed"
                    .into(),
            )));
        }
        let [r, slug] = Self::repo_flag(repo);
        match review.event {
            ReviewEvent::RequestChanges => return Err(CliStep::Host(CodeHostError::Unsupported(
                "GitLab has no request-changes review — leave a comment and say what should change"
                    .into(),
            ))),
            ReviewEvent::Approve => {
                self.glab(
                    Self::args(&["mr", "approve", &number.to_string(), &r, &slug]),
                    None,
                    true,
                    "approve merge request",
                )
                .await?;
            }
            ReviewEvent::Comment => {}
        }
        if let Some(words) = review.words() {
            self.glab(
                Self::args(&[
                    "mr",
                    "note",
                    &number.to_string(),
                    &r,
                    &slug,
                    "--message",
                    words,
                ]),
                None,
                true,
                "comment on merge request",
            )
            .await?;
        }
        if !review.comments.is_empty() {
            let mr = self.view(repo, number).await?;
            let refs = mr.diff_refs.ok_or_else(|| {
                CliStep::Host(CodeHostError::Refused(
                    "submit review: the merge request has no diff to comment on yet".into(),
                ))
            })?;
            for comment in &review.comments {
                let _: serde_json::Value = self
                    .api(
                        "POST",
                        &format!(
                            "projects/{}/merge_requests/{number}/discussions",
                            project(repo)
                        ),
                        Some(inline_discussion(comment, &refs)),
                        "comment on the diff",
                    )
                    .await?;
            }
        }
        Ok(())
    }

    async fn pr_reviews(&self, repo: &RepoRef, number: u64) -> CliOutcome<PrReviews> {
        let base = format!("projects/{}/merge_requests/{number}", project(repo));
        let approvals: GlApprovals = self
            .api("GET", &format!("{base}/approvals"), None, "read approvals")
            .await?;
        let notes: Vec<GlNote> = self
            .api(
                "GET",
                &format!("{base}/notes?per_page=100"),
                None,
                "read notes",
            )
            .await?;
        let discussions: Vec<GlDiscussion> = self
            .api(
                "GET",
                &format!("{base}/discussions?per_page=100"),
                None,
                "read discussions",
            )
            .await?;
        Ok(reviews_of(repo, number, approvals, notes, discussions))
    }

    async fn resolve_review_thread(&self, thread_id: &str, resolved: bool) -> CliOutcome<()> {
        let (slug, iid, discussion) = parse_thread_id(thread_id).ok_or_else(|| {
            CliStep::Host(CodeHostError::NotFound(format!(
                "review thread {thread_id}"
            )))
        })?;
        let path = format!(
            "projects/{}/merge_requests/{iid}/discussions/{discussion}?resolved={resolved}",
            encode(&slug)
        );
        let _: serde_json::Value = self
            .api("PUT", &path, None, "resolve review thread")
            .await?;
        Ok(())
    }

    async fn reply_review_thread(&self, thread_id: &str, body: &str) -> CliOutcome<()> {
        let (slug, iid, discussion) = parse_thread_id(thread_id).ok_or_else(|| {
            CliStep::Host(CodeHostError::NotFound(format!(
                "review thread {thread_id}"
            )))
        })?;
        let path = format!(
            "projects/{}/merge_requests/{iid}/discussions/{discussion}/notes",
            encode(&slug)
        );
        let _: serde_json::Value = self
            .api(
                "POST",
                &path,
                Some(serde_json::json!({"body": body})),
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
    ) -> CliOutcome<MergeOutcome> {
        let [r, slug] = Self::repo_flag(repo);
        let mut args = Self::args(&["mr", "merge", &number.to_string(), &r, &slug, "--yes"]);
        match strategy {
            MergeStrategy::Merge => {}
            MergeStrategy::Squash => args.push("--squash".into()),
            MergeStrategy::Rebase => {
                return Err(CliStep::Host(CodeHostError::Unsupported(
                    "a rebase merge on GitLab".into(),
                )))
            }
        }
        self.glab(args, None, true, "merge merge request").await?;
        let mr = self.view(repo, number).await.map_err(|e| match e {
            CliStep::Fallback(why) => CliStep::Host(CodeHostError::Transport(format!(
                "merge request !{number} was merged, but reading it back failed: {why}"
            ))),
            host => host,
        })?;
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

    async fn delete_branch(&self, repo: &RepoRef, branch: &str) -> CliOutcome<()> {
        let _: serde_json::Value = self
            .api(
                "DELETE",
                &format!(
                    "projects/{}/repository/branches/{}",
                    project(repo),
                    encode(branch)
                ),
                None,
                "delete branch",
            )
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_merge_request_url_yields_its_iid() {
        assert_eq!(iid_of_url("Creating merge request for work/x into main in acme/web\n\nhttps://gitlab.com/acme/web/-/merge_requests/7\n"), Some(7));
        assert_eq!(iid_of_url("!7 opened"), None);
    }
}
