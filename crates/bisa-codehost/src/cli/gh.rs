//! GitHub through `gh`, the machine's own CLI: porcelain where it has a verb
//! (`pr create/view/list/checks/review/merge`), `gh api` for the rest — the
//! reviews-and-threads GraphQL, resolving a thread, deleting a branch, an
//! inline review — every call authenticated by the CLI itself. A host other
//! than github.com is named through `GH_HOST`; an account other than the
//! CLI's active one through `GH_TOKEN`, taken from `gh auth token --user` for
//! that one command and never written anywhere.

use super::{
    classify_failure, parse_gh_auth_status, parse_version, run, CliAccount, CliOutcome, CliProbe,
    CliProgram, CliRunner, CliStep, CodeHostCli, Fallback, Output, Timeouts,
};
use crate::creds::{CliToken, Secret};
use crate::github::{
    graphql_data, resolve_thread_mutation, review_body, reviews_of, HOST, REPLY_THREAD_MUTATION,
    REVIEWS_QUERY,
};
use crate::{
    Account, CheckRun, CodeHostError, MergeOutcome, MergeStrategy, PrCreate, PrFilter, PrReviews,
    PrState, PullRequest, RepoAccess, RepoRef, Review,
};
use serde::Deserialize;
use std::sync::Arc;

const PR_FIELDS: &str = "number,url,title,state,isDraft,mergeable,headRefName,headRefOid,baseRefName,author,mergeCommit";

#[derive(Clone, Debug)]
pub struct GhCli {
    runner: Arc<dyn CliRunner>,
    host: String,
    account: Option<String>,
    timeouts: Timeouts,
}

impl GhCli {
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

    fn repo_flag(&self, repo: &RepoRef) -> [String; 2] {
        [
            "-R".to_string(),
            format!("{}/{}/{}", self.host, repo.owner, repo.name),
        ]
    }

    fn installed(&self) -> CliOutcome<()> {
        match self.runner.installed(CliProgram::Gh) {
            Some(_) => Ok(()),
            None => Err(CliStep::Fallback(Fallback::NotInstalled)),
        }
    }

    /// The environment a call runs with: `GH_HOST` off github.com, and
    /// `GH_TOKEN` when the host is bound to an account — the CLI's token for
    /// that login, asked for and used once.
    async fn env(&self) -> CliOutcome<Vec<(String, String)>> {
        let mut env = Vec::new();
        if self.host != HOST {
            env.push(("GH_HOST".to_string(), self.host.clone()));
        }
        if let Some(login) = &self.account {
            match self.token_for(Some(login)).await {
                Some((_, secret)) => {
                    env.push(("GH_TOKEN".to_string(), secret.expose().to_string()))
                }
                None => return Err(CliStep::Fallback(Fallback::NoSuchAccount(login.clone()))),
            }
        }
        Ok(env)
    }

    /// `gh auth token --hostname H [--user L]`, as a secret.
    async fn token_for(&self, login: Option<&str>) -> Option<(String, Secret)> {
        self.runner.installed(CliProgram::Gh)?;
        let mut args = Self::args(&["auth", "token", "--hostname", &self.host]);
        let who = match login {
            Some(l) => {
                args.push("--user".into());
                args.push(l.to_string());
                l.to_string()
            }
            None => self
                .status()
                .await
                .into_iter()
                .find(|a| a.active)
                .map(|a| a.login)?,
        };
        let out = run(
            &self.runner,
            CliProgram::Gh,
            args,
            Vec::new(),
            None,
            self.timeouts.local,
        )
        .await
        .ok()?;
        if !out.success() {
            return None;
        }
        Some((who, Secret::some(out.stdout)?))
    }

    async fn status(&self) -> Vec<CliAccount> {
        let args = Self::args(&["auth", "status", "--hostname", &self.host]);
        match run(
            &self.runner,
            CliProgram::Gh,
            args,
            Vec::new(),
            None,
            self.timeouts.network,
        )
        .await
        {
            // `gh` prints the status to stderr in older versions and stdout in newer ones.
            Ok(out) => parse_gh_auth_status(&format!("{}\n{}", out.stdout, out.stderr)),
            Err(_) => Vec::new(),
        }
    }

    /// One call: not installed is a fallback; a non-zero exit is classified.
    async fn gh(
        &self,
        args: Vec<String>,
        stdin: Option<Vec<u8>>,
        network: bool,
        what: &str,
    ) -> CliOutcome<Output> {
        self.installed()?;
        let env = self.env().await?;
        let timeout = if network {
            self.timeouts.network
        } else {
            self.timeouts.local
        };
        let out = run(&self.runner, CliProgram::Gh, args, env, stdin, timeout).await?;
        if !out.success() {
            return Err(classify_failure(what, &out));
        }
        Ok(out)
    }

    fn json<T: serde::de::DeserializeOwned>(out: &Output, what: &str) -> CliOutcome<T> {
        serde_json::from_str(&out.stdout)
            .map_err(|e| CliStep::Fallback(Fallback::Unparsable(format!("{what}: {e}"))))
    }

    async fn view(&self, repo: &RepoRef, number: u64) -> CliOutcome<PullRequest> {
        let [r, slug] = self.repo_flag(repo);
        let args = Self::args(&[
            "pr",
            "view",
            &number.to_string(),
            &r,
            &slug,
            "--json",
            PR_FIELDS,
        ]);
        let out = self.gh(args, None, true, "read pull request").await?;
        let pr: GhJsonPull = Self::json(&out, "read pull request")?;
        Ok(pr.into())
    }
}

/// A REST user, as `gh api user` and `gh api user/orgs` answer: the numeric
/// id is what a no-reply address is made of, so it is read here and nowhere
/// else.
#[derive(Deserialize)]
struct GhJsonUser {
    login: String,
    #[serde(default)]
    id: Option<u64>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    email: Option<String>,
}
/// A pull request's author as `gh pr view --json author` shapes it — the
/// porcelain's own object, whose `id` is a GraphQL node id, not a number.
/// The login is the one word a pull request keeps, so it is the one read.
#[derive(Deserialize)]
struct GhJsonAuthor {
    login: String,
}
#[derive(Deserialize)]
struct GhJsonCommit {
    oid: String,
}
/// What `gh pr view --json` says — its own field names, not the REST API's.
#[derive(Deserialize)]
struct GhJsonPull {
    number: u64,
    url: String,
    title: String,
    state: String,
    #[serde(rename = "isDraft", default)]
    is_draft: bool,
    #[serde(default)]
    mergeable: String,
    #[serde(rename = "headRefName", default)]
    head: String,
    #[serde(rename = "headRefOid", default)]
    head_sha: String,
    #[serde(rename = "baseRefName", default)]
    base: String,
    author: Option<GhJsonAuthor>,
    #[serde(rename = "mergeCommit", default)]
    merge_commit: Option<GhJsonCommit>,
}

impl From<GhJsonPull> for PullRequest {
    fn from(p: GhJsonPull) -> Self {
        PullRequest {
            number: p.number,
            url: p.url,
            title: p.title,
            state: match p.state.as_str() {
                "OPEN" => PrState::Open,
                "CLOSED" => PrState::Closed,
                "MERGED" => PrState::Merged,
                _ => PrState::Unknown,
            },
            is_draft: p.is_draft,
            mergeable: match p.mergeable.as_str() {
                "MERGEABLE" => Some(true),
                "CONFLICTING" => Some(false),
                _ => None,
            },
            head: p.head,
            head_sha: p.head_sha,
            base: p.base,
            author: p.author.map(|a| a.login),
        }
    }
}

/// What `gh pr checks --json` says per run.
#[derive(Deserialize)]
struct GhJsonCheck {
    name: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    bucket: String,
    link: Option<String>,
    description: Option<String>,
}

impl From<GhJsonCheck> for CheckRun {
    fn from(c: GhJsonCheck) -> Self {
        let pending = c.bucket == "pending"
            || matches!(
                c.state.as_str(),
                "IN_PROGRESS" | "QUEUED" | "PENDING" | "WAITING" | "REQUESTED"
            );
        let (status, conclusion) = if pending {
            (
                if c.state == "IN_PROGRESS" {
                    "in_progress"
                } else {
                    "queued"
                }
                .to_string(),
                None,
            )
        } else {
            ("completed".to_string(), Some(c.state.to_ascii_lowercase()))
        };
        CheckRun {
            name: c.name,
            status,
            conclusion,
            url: c.link.filter(|l| !l.is_empty()),
            summary: c.description.filter(|d| !d.is_empty()),
        }
    }
}

/// The number at the end of a pull request URL `…/pull/12`.
fn number_of_url(text: &str) -> Option<u64> {
    text.lines()
        .rev()
        .map(str::trim)
        .find(|l| l.contains("/pull/"))
        .and_then(|l| l.rsplit('/').next())
        .and_then(|n| {
            n.trim_end_matches(|c: char| !c.is_ascii_digit())
                .parse()
                .ok()
        })
}

#[async_trait::async_trait]
impl CliToken for GhCli {
    async fn token(&self, login: Option<&str>) -> Option<(String, Secret)> {
        self.token_for(login).await
    }

    async fn active_login(&self) -> Option<String> {
        self.runner.installed(CliProgram::Gh)?;
        self.status()
            .await
            .into_iter()
            .find(|a| a.active)
            .map(|a| a.login)
    }
}

#[async_trait::async_trait]
impl CodeHostCli for GhCli {
    fn program(&self) -> CliProgram {
        CliProgram::Gh
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
        let Some(path) = self.runner.installed(CliProgram::Gh) else {
            return CliProbe::not_installed(CliProgram::Gh);
        };
        let version = match run(
            &self.runner,
            CliProgram::Gh,
            Self::args(&["--version"]),
            Vec::new(),
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
            CliProgram::Gh,
            args,
            Vec::new(),
            None,
            self.timeouts.network,
        )
        .await
        {
            Ok(out) => {
                let text = format!("{}\n{}", out.stdout, out.stderr);
                let accounts = parse_gh_auth_status(&text);
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
            program: CliProgram::Gh,
            installed: true,
            path: Some(path.display().to_string()),
            version,
            accounts,
            detail,
        }
    }

    async fn account(&self) -> CliOutcome<Account> {
        let out = self
            .gh(
                Self::args(&["api", "user"]),
                None,
                true,
                "check the account",
            )
            .await?;
        let user: GhJsonUser = Self::json(&out, "check the account")?;
        let organizations = match self
            .gh(
                Self::args(&["api", "user/orgs", "--paginate"]),
                None,
                true,
                "read organizations",
            )
            .await
        {
            Ok(out) => serde_json::from_str::<Vec<GhJsonUser>>(&out.stdout)
                .map(|list| list.into_iter().map(|o| o.login).collect())
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        Ok(Account {
            login: user.login,
            scopes: Vec::new(),
            organizations,
            name: user.name.filter(|n| !n.trim().is_empty()),
            email: user.email.filter(|e| !e.trim().is_empty()),
            id: user.id,
        })
    }

    async fn repo_access(&self, repo: &RepoRef) -> CliOutcome<RepoAccess> {
        let path = format!("repos/{}/{}", repo.owner, repo.name);
        match self
            .gh(
                Self::args(&["api", &path]),
                None,
                true,
                "read repository access",
            )
            .await
        {
            Ok(out) => {
                let v: serde_json::Value = Self::json(&out, "read repository access")?;
                Ok(RepoAccess {
                    found: true,
                    push: v["permissions"]["push"].as_bool().unwrap_or(false),
                })
            }
            Err(CliStep::Host(CodeHostError::NotFound(_))) => Ok(RepoAccess {
                found: false,
                push: false,
            }),
            Err(e) => Err(e),
        }
    }

    /// `gh pr create` prints the new pull request's URL; the pull request is
    /// then read back through `pr view`. Once the create has succeeded, a
    /// failure to read it back is the host's, never a fallback — the API
    /// would create a second one.
    async fn create_pr(&self, repo: &RepoRef, req: PrCreate) -> CliOutcome<PullRequest> {
        let [r, slug] = self.repo_flag(repo);
        let mut args = Self::args(&[
            "pr", "create", &r, &slug, "--title", &req.title, "--body", &req.body, "--head",
            &req.head, "--base", &req.base,
        ]);
        if req.draft {
            args.push("--draft".into());
        }
        for reviewer in &req.reviewers {
            args.push("--reviewer".into());
            args.push(reviewer.clone());
        }
        for label in &req.labels {
            args.push("--label".into());
            args.push(label.clone());
        }
        let out = self.gh(args, None, true, "create pull request").await?;
        let number = number_of_url(&out.stdout).ok_or_else(|| {
            CliStep::Host(CodeHostError::Transport(
                "create pull request: gh printed no pull request URL".into(),
            ))
        })?;
        self.view(repo, number).await.map_err(|e| match e {
            CliStep::Fallback(why) => CliStep::Host(CodeHostError::Transport(format!(
                "pull request #{number} was created, but reading it back failed: {why}"
            ))),
            host => host,
        })
    }

    async fn get_pr(&self, repo: &RepoRef, number: u64) -> CliOutcome<PullRequest> {
        self.view(repo, number).await
    }

    async fn list_prs(&self, repo: &RepoRef, filter: PrFilter) -> CliOutcome<Vec<PullRequest>> {
        let [r, slug] = self.repo_flag(repo);
        let state = match filter.state {
            Some(PrState::Open) | None => "open",
            Some(PrState::Closed) => "closed",
            Some(PrState::Merged) => "merged",
            Some(PrState::Unknown) => "all",
        };
        let mut args = Self::args(&[
            "pr", "list", &r, &slug, "--state", state, "--limit", "50", "--json", PR_FIELDS,
        ]);
        if let Some(head) = &filter.head {
            args.push("--head".into());
            args.push(head.clone());
        }
        let out = self.gh(args, None, true, "list pull requests").await?;
        let prs: Vec<GhJsonPull> = Self::json(&out, "list pull requests")?;
        Ok(prs.into_iter().map(PullRequest::from).collect())
    }

    /// `gh pr checks` exits 1 when a check failed and 8 while some are still
    /// running — with the JSON on stdout all the same, so those exits are
    /// answers, not failures.
    async fn checks(&self, repo: &RepoRef, number: u64) -> CliOutcome<Vec<CheckRun>> {
        self.installed()?;
        let env = self.env().await?;
        let [r, slug] = self.repo_flag(repo);
        let args = Self::args(&[
            "pr",
            "checks",
            &number.to_string(),
            &r,
            &slug,
            "--json",
            "name,state,bucket,link,description",
        ]);
        let out = run(
            &self.runner,
            CliProgram::Gh,
            args,
            env,
            None,
            self.timeouts.network,
        )
        .await?;
        let has_json = out.stdout.trim_start().starts_with('[');
        if !has_json {
            if !out.success() {
                return Err(classify_failure("read checks", &out));
            }
            return Ok(Vec::new());
        }
        let checks: Vec<GhJsonCheck> = Self::json(&out, "read checks")?;
        Ok(checks.into_iter().map(CheckRun::from).collect())
    }

    /// Porcelain for a review without inline comments; `gh api` posting the
    /// REST body for one with them, since `pr review` has no `--comment-on`.
    async fn submit_review(&self, repo: &RepoRef, number: u64, review: Review) -> CliOutcome<()> {
        if review.needs_words() {
            return Err(CliStep::Host(CodeHostError::Refused(
                "submit review: a comment or a change request needs words — say what you noticed"
                    .into(),
            )));
        }
        if review.comments.is_empty() {
            let [r, slug] = self.repo_flag(repo);
            let verdict = match review.event {
                crate::ReviewEvent::Approve => "--approve",
                crate::ReviewEvent::RequestChanges => "--request-changes",
                crate::ReviewEvent::Comment => "--comment",
            };
            let mut args = Self::args(&["pr", "review", &number.to_string(), &r, &slug, verdict]);
            if let Some(words) = review.words() {
                args.push("--body".into());
                args.push(words.to_string());
            }
            self.gh(args, None, true, "submit review").await?;
            return Ok(());
        }
        let path = format!("repos/{}/{}/pulls/{number}/reviews", repo.owner, repo.name);
        let body = serde_json::to_vec(&review_body(&review)).unwrap_or_default();
        let args = Self::args(&["api", "--method", "POST", &path, "--input", "-"]);
        self.gh(args, Some(body), true, "submit review").await?;
        Ok(())
    }

    async fn pr_reviews(&self, repo: &RepoRef, number: u64) -> CliOutcome<PrReviews> {
        let what = "read reviews";
        let query = format!("query={REVIEWS_QUERY}");
        let owner = format!("owner={}", repo.owner);
        let name = format!("name={}", repo.name);
        let number = format!("number={number}");
        let args = Self::args(&[
            "api", "graphql", "-f", &query, "-f", &owner, "-f", &name, "-F", &number,
        ]);
        let out = self.gh(args, None, true, what).await?;
        let value: serde_json::Value = Self::json(&out, what)?;
        let data = graphql_data(value, what).map_err(CliStep::Host)?;
        reviews_of(data, what).map_err(|e| CliStep::Fallback(Fallback::Unparsable(e.to_string())))
    }

    async fn resolve_review_thread(&self, thread_id: &str, resolved: bool) -> CliOutcome<()> {
        let what = "resolve review thread";
        let query = format!("query={}", resolve_thread_mutation(resolved));
        let id = format!("id={thread_id}");
        let args = Self::args(&["api", "graphql", "-f", &query, "-f", &id]);
        let out = self.gh(args, None, true, what).await?;
        let value: serde_json::Value = Self::json(&out, what)?;
        graphql_data(value, what).map_err(CliStep::Host)?;
        Ok(())
    }

    async fn reply_review_thread(&self, thread_id: &str, body: &str) -> CliOutcome<()> {
        let what = "reply on review thread";
        let query = format!("query={REPLY_THREAD_MUTATION}");
        let id = format!("id={thread_id}");
        let body = format!("body={body}");
        let args = Self::args(&["api", "graphql", "-f", &query, "-f", &id, "-f", &body]);
        let out = self.gh(args, None, true, what).await?;
        let value: serde_json::Value = Self::json(&out, what)?;
        graphql_data(value, what).map_err(CliStep::Host)?;
        Ok(())
    }

    async fn merge(
        &self,
        repo: &RepoRef,
        number: u64,
        strategy: MergeStrategy,
    ) -> CliOutcome<MergeOutcome> {
        let [r, slug] = self.repo_flag(repo);
        let how = match strategy {
            MergeStrategy::Merge => "--merge",
            MergeStrategy::Squash => "--squash",
            MergeStrategy::Rebase => "--rebase",
        };
        let args = Self::args(&["pr", "merge", &number.to_string(), &r, &slug, how]);
        self.gh(args, None, true, "merge pull request").await?;
        // What landed: the pull request read back, its merge commit when gh says.
        let [r, slug] = self.repo_flag(repo);
        let args = Self::args(&[
            "pr",
            "view",
            &number.to_string(),
            &r,
            &slug,
            "--json",
            PR_FIELDS,
        ]);
        let out = self
            .gh(args, None, true, "read pull request")
            .await
            .map_err(|e| match e {
                CliStep::Fallback(why) => CliStep::Host(CodeHostError::Transport(format!(
                    "pull request #{number} was merged, but reading it back failed: {why}"
                ))),
                host => host,
            })?;
        let pr: GhJsonPull = Self::json(&out, "read pull request")?;
        let sha = pr.merge_commit.as_ref().map(|c| c.oid.clone());
        let merged = pr.state == "MERGED";
        Ok(MergeOutcome {
            merged,
            sha,
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

    async fn delete_branch(&self, repo: &RepoRef, branch: &str) -> CliOutcome<()> {
        let path = format!("repos/{}/{}/git/refs/heads/{branch}", repo.owner, repo.name);
        let args = Self::args(&["api", "--method", "DELETE", &path]);
        self.gh(args, None, true, "delete branch").await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rest_user_keeps_its_numeric_id_and_the_porcelain_author_is_a_login_alone() {
        let user: GhJsonUser = serde_json::from_str(
            r#"{"login":"ada","id":42439055,"node_id":"MDQ6VXNlcjQyNDM5MDU1","name":"Ada","email":null}"#,
        )
        .unwrap();
        assert_eq!(
            (
                user.login.as_str(),
                user.id,
                user.name.as_deref(),
                user.email
            ),
            ("ada", Some(42439055), Some("Ada"), None)
        );
        let author: GhJsonAuthor = serde_json::from_str(
            r#"{"id":"MDQ6VXNlcjQyNDM5MDU1","is_bot":false,"login":"ada","name":"Ada"}"#,
        )
        .unwrap();
        assert_eq!(author.login, "ada");
    }

    #[test]
    fn a_pull_request_url_yields_its_number_and_gh_json_reads_into_a_pull_request() {
        assert_eq!(number_of_url("Creating pull request for work/x into main in acme/web\n\nhttps://github.com/acme/web/pull/12\n"), Some(12));
        assert_eq!(number_of_url("nothing"), None);
        let pr: PullRequest = serde_json::from_str::<GhJsonPull>(
            r#"{"number":12,"url":"u","title":"t","state":"OPEN","isDraft":true,"mergeable":"CONFLICTING","headRefName":"work/x","headRefOid":"abc","baseRefName":"main","author":{"id":"MDQ6VXNlcjQyNDM5MDU1","is_bot":false,"login":"ada","name":"Ada"},"mergeCommit":null}"#,
        )
        .unwrap()
        .into();
        assert_eq!(
            (pr.state, pr.is_draft, pr.mergeable, pr.author.as_deref()),
            (PrState::Open, true, Some(false), Some("ada")),
            "the porcelain's author carries a node id, not a number: the login is what is read"
        );
        let check: CheckRun = serde_json::from_str::<GhJsonCheck>(
            r#"{"name":"ci","state":"IN_PROGRESS","bucket":"pending","link":"","description":""}"#,
        )
        .unwrap()
        .into();
        assert_eq!(
            (check.status.as_str(), check.conclusion, check.url),
            ("in_progress", None, None)
        );
        let done: CheckRun = serde_json::from_str::<GhJsonCheck>(r#"{"name":"ci","state":"SUCCESS","bucket":"pass","link":"https://x","description":"ok"}"#).unwrap().into();
        assert_eq!(
            (done.status.as_str(), done.conclusion.as_deref()),
            ("completed", Some("success"))
        );
    }
}
