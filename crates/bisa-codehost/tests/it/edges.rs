//! The crate at its edges: a remote URL with no user and one with no
//! repository, the registry and the hosts printed for a log, the layered host
//! answering every verb from the API when no CLI is on the machine, the fake
//! reading a verdict back and refusing what it does not hold, the real
//! runner feeding a body and timing a child out against a script of the
//! test's own, and each CLI and each API read where the program or the host
//! answers something short, strange or late. No program of the machine's
//! runs: the only `gh` here is a shell script written by the test, found on
//! a `PATH` handed to the one call that spawns it.

use crate::support::{Canned, Stub};
use bisa_codehost::bitbucket::BitbucketApi;
use bisa_codehost::cli::fake::FakeCli;
use bisa_codehost::cli::gh::GhCli;
use bisa_codehost::cli::glab::GlabCli;
use bisa_codehost::cli::{Cli, CliError, CliProgram, CliRunner, CliStep, CodeHostCli, Fallback};
use bisa_codehost::creds::{CliToken, TokenStore};
use bisa_codehost::fake::FakeCodeHost;
use bisa_codehost::github::GitHubApi;
use bisa_codehost::gitlab::GitLabApi;
use bisa_codehost::hosts::Hosts;
use bisa_codehost::layered::Layered;
use bisa_codehost::{
    CodeHost, CodeHostCapabilities, CodeHostError, CodeHostKind, CodeHostRegistry, MergeStrategy,
    PrCreate, PrFilter, PrState, RemoteProtocol, RemoteUrl, RepoRef, Review, ReviewComment,
    ReviewEvent, ReviewThread, ReviewThreadComment, Side,
};
use serde_json::json;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

fn repo() -> RepoRef {
    RepoRef {
        host: "github.com".into(),
        owner: "acme".into(),
        name: "web".into(),
    }
}

fn gitlab_repo() -> RepoRef {
    RepoRef {
        host: "gitlab.com".into(),
        owner: "acme/platform".into(),
        name: "web".into(),
    }
}

fn full() -> CodeHostCapabilities {
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
        review_events: vec![
            ReviewEvent::Approve,
            ReviewEvent::RequestChanges,
            ReviewEvent::Comment,
        ],
    }
}

fn args(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| w.to_string()).collect()
}

/// A token store under `dir` holding one login — a word, never a credential.
fn stored(kind: CodeHostKind, host: &str, dir: &Path) -> TokenStore {
    let tokens = TokenStore::file_only(kind, host, dir);
    tokens.store("stub", "a-word-the-stub-ignores").unwrap();
    tokens
}

fn no_env_token(kind: CodeHostKind) -> bool {
    std::env::var(kind.env_var()).map_or(true, |v| v.trim().is_empty())
}

// --- the crate's own types ------------------------------------------------------

#[test]
fn a_remote_without_a_user_or_a_repository_reads_as_written_and_the_registry_prints_its_ids() {
    let bare = RemoteUrl::parse("host.example:tools/one.git");
    assert_eq!(bare.protocol, RemoteProtocol::Scp);
    assert_eq!(bare.user, None, "scp with nobody before the @");
    assert_eq!(bare.host.as_deref(), Some("host.example"));
    assert_eq!(bare.summary(), "host.example · tools/one");

    let only = RemoteUrl::parse("https://github.com/only");
    assert_eq!(
        (only.owner.as_deref(), only.name.as_deref()),
        (None, None),
        "one segment names no repository"
    );
    assert_eq!(only.summary(), "github.com · only", "the host and the path");
    assert_eq!(RemoteUrl::parse("just-a-folder").summary(), "just-a-folder");
    let none = RemoteUrl::parse("https://github.com");
    assert_eq!(
        (none.path.as_str(), none.name),
        ("", None),
        "no path, no name"
    );
    assert_eq!(PrState::Unknown.as_str(), "unknown");

    let fake: Arc<dyn CodeHost> =
        Arc::new(FakeCodeHost::of_kind(CodeHostKind::GitLab, "gitlab.com"));
    let registry = CodeHostRegistry::new(vec![fake]);
    assert_eq!(format!("{registry:?}"), r#"[CodeHostId("gitlab")]"#);
    assert_eq!(registry.all().len(), 1, "a registry with one host: the one");

    let dir = tempfile::tempdir().unwrap();
    let words = format!("{:?}", Hosts::file_only(dir.path()));
    assert!(
        words.starts_with("Hosts {")
            && words.contains("git: false")
            && words.contains("cli: false"),
        "the hosts print what they are made of, never a token: {words}"
    );
}

// --- the layered host -------------------------------------------------------------

/// With `gh` not on the machine, every verb of the layered host is the API's:
/// the CLI steps aside once, before any call, and the fake behind the API
/// answers each in turn.
#[tokio::test]
async fn without_the_cli_every_verb_of_the_layered_host_is_the_apis() {
    let api = Arc::new(FakeCodeHost::with_capabilities("github.com", full()).signed_in_as("you"));
    api.seed_pr(&repo(), "alice", "work/x", "main");
    api.state.threads.lock().unwrap().push(ReviewThread {
        id: "T1".into(),
        path: Some("a.rs".into()),
        line: Some(3),
        is_resolved: false,
        is_outdated: false,
        comments: vec![ReviewThreadComment {
            author: Some("alice".into()),
            body: "rename".into(),
            created_at: None,
        }],
    });
    let gh = GhCli::new(Arc::new(FakeCli::new()), "github.com");
    let host = Layered::new(api.clone() as Arc<dyn CodeHost>, Some(Arc::new(gh)));
    assert_eq!(host.api().id().0, "fake");
    assert!(host.capabilities().delete_branch, "the API's capabilities");

    assert_eq!(host.account().await.unwrap().login, "you");
    assert!(host.repo_access(&repo()).await.unwrap().push);
    let opened = host
        .create_pr(
            &repo(),
            PrCreate {
                title: "Add".into(),
                body: String::new(),
                head: "work/y".into(),
                base: "main".into(),
                draft: false,
                reviewers: vec![],
                labels: vec![],
            },
        )
        .await
        .unwrap();
    assert_eq!(opened.number, 2);
    let open = host.list_prs(&repo(), PrFilter::default()).await.unwrap();
    assert_eq!(open.len(), 2);
    assert!(host.checks(&repo(), 1).await.unwrap().is_empty());
    host.submit_review(
        &repo(),
        1,
        Review {
            event: ReviewEvent::Approve,
            body: None,
            comments: vec![],
        },
    )
    .await
    .unwrap();
    let reviews = host.pr_reviews(&repo(), 1).await.unwrap();
    assert_eq!(reviews.reviews[0].state, "approved");
    host.resolve_review_thread("T1", true).await.unwrap();
    assert!(host.pr_reviews(&repo(), 1).await.unwrap().threads[0].is_resolved);
    let merged = host.merge(&repo(), 1, MergeStrategy::Squash).await.unwrap();
    assert!(merged.merged);
    host.delete_branch(&repo(), "work/x").await.unwrap();
    assert_eq!(
        api.state.deleted_branches.lock().unwrap().as_slice(),
        ["work/x"]
    );
    assert!(
        api.asked_as().iter().all(|l| l == "you"),
        "every answer the API's, as the one account"
    );
}

// --- the fake -----------------------------------------------------------------------

#[tokio::test]
async fn the_fake_reads_a_verdict_back_by_its_word_and_refuses_what_it_does_not_hold() {
    let host = FakeCodeHost::with_capabilities("github.com", full()).signed_in_as("you");
    host.seed_pr(&repo(), "alice", "work/x", "main");
    for (event, word) in [
        (ReviewEvent::Approve, "approved"),
        (ReviewEvent::RequestChanges, "changes_requested"),
    ] {
        host.submit_review(
            &repo(),
            1,
            Review {
                event,
                body: Some("said".into()),
                comments: vec![],
            },
        )
        .await
        .unwrap();
        let last = host
            .pr_reviews(&repo(), 1)
            .await
            .unwrap()
            .reviews
            .pop()
            .unwrap();
        assert_eq!(last.state, word);
    }
    let gone = host.resolve_review_thread("T9", true).await.unwrap_err();
    assert!(
        matches!(&gone, CodeHostError::NotFound(w) if w.contains("T9")),
        "{gone}"
    );
    let gone = host
        .merge(&repo(), 99, MergeStrategy::Merge)
        .await
        .unwrap_err();
    assert!(
        matches!(&gone, CodeHostError::NotFound(w) if w.contains("#99")),
        "{gone}"
    );
}

// --- the real runner, against a script of the test's own ----------------------------

/// A `gh` of the test's own: a shell script in a folder the call's `PATH`
/// names, so the real runner resolves and spawns it and nothing of the
/// machine's is ever asked.
fn script(dir: &Path, body: &str) -> String {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join("gh");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    dir.display().to_string()
}

/// The call's `PATH`: the script's folder first, then the system's two for
/// what the script itself runs (`cat`, `sleep`) — never the machine's own.
fn on_path(dir: &str) -> Vec<(String, String)> {
    vec![("PATH".to_string(), format!("{dir}:/usr/bin:/bin"))]
}

#[test]
fn the_real_runner_feeds_a_body_times_a_child_out_and_says_when_a_program_cannot_run() {
    let root = tempfile::tempdir().unwrap();
    let runner = Cli::default();
    let budget = Duration::from_secs(10);

    // The body goes in on stdin and the exit code comes back as an answer.
    let echo = script(
        &root.path().join("echo"),
        "cat; printf 'said on stderr' >&2; exit 3",
    );
    let out = runner
        .run(
            CliProgram::Gh,
            &args(&["pr", "view"]),
            &on_path(&echo),
            Some(b"the body"),
            budget,
        )
        .unwrap();
    assert_eq!((out.code, out.stdout.as_str()), (3, "the body"));
    assert_eq!(out.stderr, "said on stderr");
    assert!(
        format!("{out:?}").contains("stdout_bytes: 8") && !format!("{out:?}").contains("the body"),
        "stdout is measured, never shown: {out:?}"
    );
    assert!(!out.success());

    // The same call under the engine's HTTP clients: the child is handed the
    // proxy environment the platform's own requests use.
    let proxied = Cli::default().with_http(bisa_http::Clients::shared());
    let out = proxied
        .run(
            CliProgram::Gh,
            &args(&["pr", "view"]),
            &on_path(&echo),
            Some(b"again"),
            budget,
        )
        .unwrap();
    assert_eq!((out.code, out.stdout.as_str()), (3, "again"));

    // Under a manual proxy the child is handed the proxy's names, and the
    // ones the policy leaves unset are taken away from what it inherits.
    let manual = bisa_http::HttpPolicy {
        proxy: bisa_http::ProxyPolicy::Manual {
            http: Some(reqwest::Url::parse("http://proxy.test:3128").unwrap()),
            https: None,
            no_proxy: vec![],
        },
        http1_only: false,
    };
    let behind = Cli::default().with_http(Arc::new(bisa_http::Clients::new(&manual).unwrap()));
    let tells = script(
        &root.path().join("tells"),
        "printf '%s|%s' \"$HTTP_PROXY\" \"${HTTPS_PROXY-unset}\"",
    );
    let out = behind
        .run(
            CliProgram::Gh,
            &args(&["api"]),
            &on_path(&tells),
            None,
            budget,
        )
        .unwrap();
    assert_eq!(out.stdout, "http://proxy.test:3128/|unset");

    // No program on the path handed in: not installed.
    let empty = root.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let missing = runner
        .run(
            CliProgram::Gh,
            &args(&["--version"]),
            &on_path(&empty.display().to_string()),
            None,
            budget,
        )
        .unwrap_err();
    assert_eq!(missing, CliError::NotInstalled(CliProgram::Gh));

    // A file by the program's name that cannot be run: the spawn's own words.
    let plain = root.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    std::fs::write(plain.join("gh"), "not a program\n").unwrap();
    let refused = runner
        .run(
            CliProgram::Gh,
            &args(&["--version"]),
            &on_path(&plain.display().to_string()),
            None,
            budget,
        )
        .unwrap_err();
    assert!(
        matches!(&refused, CliError::Failed { what, detail } if what == "gh --version" && detail.starts_with("spawn:")),
        "{refused:?}"
    );

    // A child that outlives its budget is ended, and the budget is the answer.
    let slow = script(&root.path().join("slow"), "exec sleep 5");
    let late = runner
        .run(
            CliProgram::Gh,
            &args(&["auth", "status"]),
            &on_path(&slow),
            None,
            Duration::from_millis(200),
        )
        .unwrap_err();
    assert!(
        matches!(&late, CliError::Timeout { what, secs: 0 } if what == "gh auth status"),
        "{late:?}"
    );

    // A child that never reads its stdin and succeeds: the short write is a
    // failure, since the CLI answered without the body. One that failed is
    // judged by its own exit, the write set aside.
    let big = vec![b'x'; 1 << 20];
    let deaf = script(&root.path().join("deaf"), "exit 0");
    let unfed = runner
        .run(
            CliProgram::Gh,
            &args(&["api"]),
            &on_path(&deaf),
            Some(&big),
            budget,
        )
        .unwrap_err();
    assert!(
        matches!(&unfed, CliError::Failed { detail, .. } if detail.starts_with("feeding stdin:")),
        "{unfed:?}"
    );
    let deaf_and_failed = script(&root.path().join("deaf-failed"), "echo no >&2; exit 1");
    let answered = runner
        .run(
            CliProgram::Gh,
            &args(&["api"]),
            &on_path(&deaf_and_failed),
            Some(&big),
            budget,
        )
        .unwrap();
    assert_eq!((answered.code, answered.stderr.trim()), (1, "no"));

    // The words of each way the CLI had no say, and the runner's errors as steps.
    assert_eq!(
        Fallback::NotInstalled.to_string(),
        "the CLI is not installed"
    );
    assert_eq!(
        Fallback::NotSignedIn.to_string(),
        "the CLI is not signed in"
    );
    assert_eq!(
        Fallback::NoVerb("this version's verbs").to_string(),
        "the CLI has no verb for this version's verbs"
    );
    assert_eq!(
        Fallback::Unparsable("no json".into()).to_string(),
        "the CLI's answer could not be read: no json"
    );
    assert!(matches!(
        CliStep::from(late),
        CliStep::Fallback(Fallback::Failed(w)) if w.contains("did not finish within 0s")
    ));
    assert!(matches!(
        CliStep::from(refused),
        CliStep::Fallback(Fallback::Failed(w)) if w.starts_with("`gh --version`: spawn:")
    ));
}

// --- gh at its edges ------------------------------------------------------------------

const CLOSED_JSON: &str = r#"{"number":12,"url":"u","title":"t","state":"CLOSED","isDraft":false,"mergeable":"UNKNOWN","headRefName":"work/x","headRefOid":"abc","baseRefName":"main","author":null,"mergeCommit":null}"#;
const STRANGE_JSON: &str = r#"{"number":13,"url":"u","title":"t","state":"WEIRD","isDraft":false,"mergeable":"","headRefName":"work/x","headRefOid":"abc","baseRefName":"main","author":null,"mergeCommit":null}"#;

fn gh_installed() -> FakeCli {
    FakeCli::new().with_program(CliProgram::Gh, "/opt/homebrew/bin/gh")
}

#[tokio::test]
async fn gh_reads_a_closed_or_strange_pull_request_a_queued_check_and_the_account_as_the_host_shows_it(
) {
    let cli = Arc::new(
        gh_installed()
            .answer(CliProgram::Gh, &["pr", "view", "12"], 0, CLOSED_JSON, "")
            .answer(CliProgram::Gh, &["pr", "view", "13"], 0, STRANGE_JSON, "")
            .answer(
                CliProgram::Gh,
                &["pr", "checks", "12"],
                8,
                r#"[{"name":"a","state":"QUEUED","bucket":"","link":"","description":""},{"name":"b","state":"IN_PROGRESS","bucket":"pending","link":null,"description":null},{"name":"c","state":"SUCCESS","bucket":"pass","link":"https://ci","description":"ok"}]"#,
                "",
            )
            .answer(
                CliProgram::Gh,
                &["api", "user"],
                0,
                r#"{"login":"ada","id":7,"name":"   ","email":""}"#,
                "",
            )
            .answer(
                CliProgram::Gh,
                &["api", "user/orgs"],
                0,
                r#"[{"login":"acme"},{"login":"widgets"}]"#,
                "",
            ),
    );
    let gh = Arc::new(GhCli::new(cli.clone(), "github.com"));
    assert_eq!(gh.program(), CliProgram::Gh);
    assert_eq!(gh.host(), "github.com");
    let same = Arc::clone(&gh).for_account(None);
    assert_eq!(
        same.host(),
        "github.com",
        "unbound asked to stay unbound: itself"
    );

    let closed = gh.get_pr(&repo(), 12).await.unwrap();
    assert_eq!((closed.state, closed.mergeable), (PrState::Closed, None));
    assert_eq!(closed.author, None);
    assert_eq!(
        gh.get_pr(&repo(), 13).await.unwrap().state,
        PrState::Unknown
    );

    let checks = gh.checks(&repo(), 12).await.unwrap();
    assert_eq!(
        checks
            .iter()
            .map(|c| (c.status.as_str(), c.conclusion.as_deref()))
            .collect::<Vec<_>>(),
        [
            ("queued", None),
            ("in_progress", None),
            ("completed", Some("success"))
        ],
        "an exit of 8 is an answer: some checks still run"
    );
    assert_eq!(
        (checks[0].url.as_deref(), checks[0].summary.as_deref()),
        (None, None)
    );

    let account = gh.account().await.unwrap();
    assert_eq!(account.login, "ada");
    assert_eq!(account.organizations, ["acme", "widgets"]);
    assert_eq!(
        (account.name, account.email, account.id),
        (None, None, Some(7)),
        "a blank name or email is none"
    );

    // Signed in to nothing the fake was told of: no token, no active login.
    assert!(CliToken::token(&*gh, None).await.is_none());
    assert!(gh.active_login().await.is_none());
}

#[tokio::test]
async fn gh_says_when_a_pull_request_was_made_or_merged_but_could_not_be_read_back() {
    // Created, but gh printed no URL: the host's words, never a second create.
    let silent = GhCli::new(
        Arc::new(gh_installed().answer(CliProgram::Gh, &["pr", "create"], 0, "", "")),
        "github.com",
    );
    let req = PrCreate {
        title: "Add".into(),
        body: String::new(),
        head: "work/x".into(),
        base: "main".into(),
        draft: false,
        reviewers: vec![],
        labels: vec![],
    };
    let err = silent.create_pr(&repo(), req.clone()).await.unwrap_err();
    assert!(
        matches!(&err, CliStep::Host(CodeHostError::Transport(w)) if w.contains("printed no pull request URL")),
        "{err:?}"
    );

    // Created, then the host itself refused the read: the host's refusal as it is.
    let gone = GhCli::new(
        Arc::new(
            gh_installed()
                .answer(
                    CliProgram::Gh,
                    &["pr", "create"],
                    0,
                    "https://github.com/acme/web/pull/12\n",
                    "",
                )
                .answer(
                    CliProgram::Gh,
                    &["pr", "view", "12"],
                    1,
                    "",
                    "HTTP 404: Not Found (https://api.github.com/repos/acme/web/pulls/12)",
                ),
        ),
        "github.com",
    );
    let err = gone.create_pr(&repo(), req).await.unwrap_err();
    assert!(
        matches!(&err, CliStep::Host(CodeHostError::NotFound(_))),
        "{err:?}"
    );

    // Merged, then the read back had no answer: the merge stands, said so.
    let unread = GhCli::new(
        Arc::new(gh_installed().answer(CliProgram::Gh, &["pr", "merge", "12"], 0, "", "")),
        "github.com",
    );
    let err = unread
        .merge(&repo(), 12, MergeStrategy::Merge)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, CliStep::Host(CodeHostError::Transport(w)) if w.contains("was merged, but reading it back failed")),
        "{err:?}"
    );

    let merged_gone = GhCli::new(
        Arc::new(
            gh_installed()
                .answer(CliProgram::Gh, &["pr", "merge", "12"], 0, "", "")
                .answer(
                    CliProgram::Gh,
                    &["pr", "view", "12"],
                    1,
                    "",
                    "HTTP 404: Not Found",
                ),
        ),
        "github.com",
    );
    let err = merged_gone
        .merge(&repo(), 12, MergeStrategy::Merge)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, CliStep::Host(CodeHostError::NotFound(_))),
        "the host's own refusal of the read: {err:?}"
    );

    // Reviews that are not shaped as reviews: a fallback, so the API is asked.
    let odd = GhCli::new(
        Arc::new(gh_installed().answer(
            CliProgram::Gh,
            &["api", "graphql"],
            0,
            r#"{"data":{"repository":{"pullRequest":{"reviews":"nope"}}}}"#,
            "",
        )),
        "github.com",
    );
    let err = odd.pr_reviews(&repo(), 12).await.unwrap_err();
    assert!(
        matches!(&err, CliStep::Fallback(Fallback::Unparsable(_))),
        "{err:?}"
    );
}

// --- glab at its edges ----------------------------------------------------------------

const MR_NO_DIFF_JSON: &str = r#"{"iid":7,"web_url":"u","title":"t","state":"opened","draft":false,"source_branch":"work/x","sha":"abc","target_branch":"main","author":{"username":"ada.lovelace"},"diff_refs":null}"#;

fn glab_installed() -> FakeCli {
    FakeCli::new().with_program(CliProgram::Glab, "/opt/homebrew/bin/glab")
}

#[tokio::test]
async fn glab_probes_an_unsigned_machine_reads_the_account_the_access_and_the_checks() {
    let cli = Arc::new(
        glab_installed()
            .always(CliProgram::Glab, &["--version"], 0, "glab version 1.50.0 (2025-01-01)\n", "")
            .answer(
                CliProgram::Glab,
                &["auth", "status"],
                1,
                "",
                "No GitLab hosts are configured.\nTo log in, run: glab auth login\n",
            )
            .answer(CliProgram::Glab, &["api", "user"], 0, r#"{"username":"ada","id":3,"name":"","public_email":"  "}"#, "")
            .answer(CliProgram::Glab, &["api", "groups?"], 0, r#"[{"full_path":"acme"},{"id":2}]"#, "")
            .answer(CliProgram::Glab, &["api", "projects/acme%2Fplatform%2Fweb"], 0, r#"{"permissions":{"project_access":{"access_level":30},"group_access":null}}"#, "")
            .answer(CliProgram::Glab, &["api", "projects/acme%2Fplatform%2Fweb"], 1, "", "HTTP 404 Not Found")
            .answer(CliProgram::Glab, &["api", "projects/acme%2Fplatform%2Fweb"], 1, "", "HTTP 401 Unauthorized")
            .answer(CliProgram::Glab, &["api", "merge_requests/7/pipelines"], 0, r#"[{"id":5}]"#, "")
            .answer(CliProgram::Glab, &["api", "pipelines/5/jobs"], 0, r#"[{"name":"build","status":"success","web_url":null,"stage":"test"},{"name":"wait","status":"manual"}]"#, "")
            .answer(CliProgram::Glab, &["api", "merge_requests/8/pipelines"], 0, "[]", ""),
    );
    let glab = Arc::new(GlabCli::new(cli.clone(), "gitlab.com"));
    assert_eq!(glab.program(), CliProgram::Glab);
    assert_eq!(glab.host(), "gitlab.com");
    assert_eq!(Arc::clone(&glab).for_account(None).host(), "gitlab.com");

    let probe = glab.probe().await;
    assert!(probe.installed && probe.accounts.is_empty());
    assert_eq!(
        probe.detail.as_deref(),
        Some("No GitLab hosts are configured."),
        "signed in to nothing: the CLI's first line"
    );
    assert!(glab.active_login().await.is_none(), "no status, no login");

    let account = glab.account().await.unwrap();
    assert_eq!((account.login.as_str(), account.id), ("ada", Some(3)));
    assert_eq!(
        account.organizations,
        ["acme"],
        "a group without a path is skipped"
    );
    assert_eq!((account.name, account.email), (None, None));

    let access = glab.repo_access(&gitlab_repo()).await.unwrap();
    assert!(access.found && access.push);
    let access = glab.repo_access(&gitlab_repo()).await.unwrap();
    assert!(
        !access.found && !access.push,
        "a 404 is not found, not an error"
    );
    let err = glab.repo_access(&gitlab_repo()).await.unwrap_err();
    assert!(
        matches!(&err, CliStep::Host(CodeHostError::NotAuthenticated(_))),
        "{err:?}"
    );

    let checks = glab.checks(&gitlab_repo(), 7).await.unwrap();
    assert_eq!(
        checks
            .iter()
            .map(|c| (c.name.as_str(), c.conclusion.as_deref()))
            .collect::<Vec<_>>(),
        [
            ("build", Some("success")),
            ("wait", Some("action_required"))
        ]
    );
    assert!(glab.checks(&gitlab_repo(), 8).await.unwrap().is_empty());
}

#[tokio::test]
async fn glab_says_when_a_merge_request_was_made_or_merged_but_could_not_be_read_and_refuses_a_bad_thread(
) {
    let req = PrCreate {
        title: "Add".into(),
        body: String::new(),
        head: "work/x".into(),
        base: "main".into(),
        draft: false,
        reviewers: vec![],
        labels: vec![],
    };
    let silent = GlabCli::new(
        Arc::new(glab_installed().answer(CliProgram::Glab, &["mr", "create"], 0, "", "")),
        "gitlab.com",
    );
    let err = silent
        .create_pr(&gitlab_repo(), req.clone())
        .await
        .unwrap_err();
    assert!(
        matches!(&err, CliStep::Host(CodeHostError::Transport(w)) if w.contains("printed no merge request URL")),
        "{err:?}"
    );
    let unread = GlabCli::new(
        Arc::new(glab_installed().answer(
            CliProgram::Glab,
            &["mr", "create"],
            0,
            "https://gitlab.com/acme/platform/web/-/merge_requests/7\n",
            "",
        )),
        "gitlab.com",
    );
    let err = unread
        .create_pr(&gitlab_repo(), req.clone())
        .await
        .unwrap_err();
    assert!(
        matches!(&err, CliStep::Host(CodeHostError::Transport(w)) if w.contains("was created, but reading it back failed")),
        "{err:?}"
    );
    let created_gone = GlabCli::new(
        Arc::new(
            glab_installed()
                .answer(
                    CliProgram::Glab,
                    &["mr", "create"],
                    0,
                    "https://gitlab.com/acme/platform/web/-/merge_requests/7\n",
                    "",
                )
                .answer(
                    CliProgram::Glab,
                    &["mr", "view", "7"],
                    1,
                    "",
                    "404 Not Found",
                ),
        ),
        "gitlab.com",
    );
    let err = created_gone
        .create_pr(&gitlab_repo(), req)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, CliStep::Host(CodeHostError::NotFound(_))),
        "{err:?}"
    );
    let merged_gone = GlabCli::new(
        Arc::new(
            glab_installed()
                .answer(CliProgram::Glab, &["mr", "merge", "7"], 0, "", "")
                .answer(
                    CliProgram::Glab,
                    &["mr", "view", "7"],
                    1,
                    "",
                    "404 Not Found",
                ),
        ),
        "gitlab.com",
    );
    let err = merged_gone
        .merge(&gitlab_repo(), 7, MergeStrategy::Merge)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, CliStep::Host(CodeHostError::NotFound(_))),
        "{err:?}"
    );
    let merged_unread = GlabCli::new(
        Arc::new(glab_installed().answer(CliProgram::Glab, &["mr", "merge", "7"], 0, "", "")),
        "gitlab.com",
    );
    let err = merged_unread
        .merge(&gitlab_repo(), 7, MergeStrategy::Merge)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, CliStep::Host(CodeHostError::Transport(w)) if w.contains("was merged, but reading it back failed")),
        "{err:?}"
    );

    // An inline comment on a merge request that has no diff yet.
    let no_diff = GlabCli::new(
        Arc::new(
            glab_installed()
                .answer(CliProgram::Glab, &["mr", "note", "7"], 0, "", "")
                .answer(
                    CliProgram::Glab,
                    &["mr", "view", "7"],
                    0,
                    MR_NO_DIFF_JSON,
                    "",
                ),
        ),
        "gitlab.com",
    );
    let err = no_diff
        .submit_review(
            &gitlab_repo(),
            7,
            Review {
                event: ReviewEvent::Comment,
                body: Some("look".into()),
                comments: vec![ReviewComment {
                    path: "a.rs".into(),
                    line: 3,
                    side: Side::Right,
                    start_line: None,
                    start_side: None,
                    body: "rename".into(),
                }],
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&err, CliStep::Host(CodeHostError::Refused(w)) if w.contains("no diff to comment on yet")),
        "{err:?}"
    );

    // A thread id that is not one: not found, before any call.
    let glab = GlabCli::new(Arc::new(glab_installed()), "gitlab.com");
    let err = glab
        .resolve_review_thread("nonsense", true)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, CliStep::Host(CodeHostError::NotFound(_))),
        "{err:?}"
    );
    let err = glab
        .reply_review_thread("nonsense", "hi")
        .await
        .unwrap_err();
    assert!(
        matches!(&err, CliStep::Host(CodeHostError::NotFound(_))),
        "{err:?}"
    );
}

// --- the GitHub API at its edges -------------------------------------------------------

#[tokio::test]
async fn github_says_when_the_host_answered_nothing_readable_and_lists_by_the_filter_it_was_given()
{
    let data_with_null_errors =
        json!({"errors": null, "data": {"repository": {"pullRequest": null}}});
    let stub = Stub::start(vec![
        Canned::empty("GET", "/repos/acme/web/pulls/1", 200),
        Canned::empty("GET", "/repos/acme/web", 200),
        Canned::empty("GET", "/user", 200),
        Canned::json("POST", "/graphql", 500, json!({"message": "down"})),
        Canned::empty("POST", "/graphql", 200),
        Canned::json("POST", "/graphql", 200, data_with_null_errors),
        Canned::json(
            "POST",
            "/repos/acme/web/pulls",
            422,
            json!({"message": "Validation Failed", "errors": [42, {"resource": "PullRequest", "field": "head", "code": "invalid"}]}),
        ),
        Canned::json(
            "GET",
            "/repos/acme/web/pulls",
            200,
            json!([{
                "number": 5, "html_url": "u", "title": "t", "state": "weird", "draft": false,
                "merged": false, "merged_at": null, "mergeable": null,
                "head": {"ref": "work/x", "sha": "a"}, "base": {"ref": "main", "sha": "b"}, "user": null
            }]),
        ),
    ])
    .await;
    let dir = tempfile::tempdir().unwrap();
    let api = GitHubApi::with_base_url(
        stub.base_url(),
        stored(CodeHostKind::GitHub, "github.com", dir.path()),
    );
    assert!(
        CodeHost::capabilities(&api).draft_prs,
        "GitHub's own capabilities"
    );

    let err = api.get_pr(&repo(), 1).await.unwrap_err();
    assert!(
        matches!(&err, CodeHostError::Transport(w) if w.contains("answered something unexpected")),
        "{err}"
    );
    let err = api.repo_access(&repo()).await.unwrap_err();
    assert!(
        matches!(&err, CodeHostError::Transport(w) if w.contains("answered something unexpected")),
        "{err}"
    );
    let err = api.account().await.unwrap_err();
    assert!(
        matches!(&err, CodeHostError::Transport(w) if w.contains("answered something unexpected")),
        "{err}"
    );
    let err = api.pr_reviews(&repo(), 1).await.unwrap_err();
    assert!(
        matches!(&err, CodeHostError::Transport(w) if w.contains("down")),
        "a GraphQL door that is down: {err}"
    );
    let err = api.pr_reviews(&repo(), 1).await.unwrap_err();
    assert!(
        matches!(&err, CodeHostError::Transport(w) if w.contains("answered something unexpected")),
        "{err}"
    );
    let empty = api.pr_reviews(&repo(), 1).await.unwrap();
    assert!(
        empty.reviews.is_empty() && empty.threads.is_empty(),
        "errors: null is no error, and a pull request the host does not have has no reviews"
    );
    let err = api
        .create_pr(
            &repo(),
            PrCreate {
                title: "Add".into(),
                body: String::new(),
                head: "work/x".into(),
                base: "main".into(),
                draft: false,
                reviewers: vec![],
                labels: vec![],
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&err, CodeHostError::Refused(w) if w.contains("head is invalid") && !w.contains("42")),
        "an error that is neither words nor an object is passed over: {err}"
    );
    let listed = api
        .list_prs(
            &repo(),
            PrFilter {
                state: Some(PrState::Open),
                head: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        listed[0].state,
        PrState::Unknown,
        "a state GitHub never says"
    );
    let calls = stub.calls();
    assert!(
        calls
            .last()
            .unwrap()
            .query
            .as_deref()
            .unwrap()
            .contains("state=open"),
        "{:?}",
        calls.last()
    );
}

// --- the GitLab API at its edges -------------------------------------------------------

const PROJECT: &str = "/projects/acme%2Fplatform%2Fweb";

fn mr_with_diff(iid: u64) -> serde_json::Value {
    let mut value = mr(iid, "opened", "mergeable");
    value["diff_refs"] = json!({"base_sha": "b", "head_sha": "h", "start_sha": "s"});
    value
}

fn mr(iid: u64, state: &str, status: &str) -> serde_json::Value {
    json!({
        "iid": iid, "web_url": "u", "title": "t", "state": state, "draft": false,
        "detailed_merge_status": status, "source_branch": "work/x", "sha": "abc",
        "target_branch": "main", "author": {"username": "ada"}, "merge_commit_sha": null,
        "diff_refs": null
    })
}

#[tokio::test]
async fn gitlab_lists_by_each_state_word_reads_every_job_status_and_speaks_every_shape_of_refusal()
{
    let stub = Stub::start(vec![
        Canned::json(
            "GET",
            &format!("{PROJECT}/merge_requests"),
            200,
            json!([mr(1, "closed", "checking")]),
        ),
        Canned::json(
            "GET",
            &format!("{PROJECT}/merge_requests"),
            200,
            json!([mr(2, "strange", "conflict")]),
        ),
        Canned::json(
            "GET",
            &format!("{PROJECT}/merge_requests/7/pipelines"),
            200,
            json!([{"id": 9}]),
        ),
        Canned::json(
            "GET",
            &format!("{PROJECT}/pipelines/9/jobs"),
            200,
            json!([
                {"name": "a", "status": "pending"}, {"name": "b", "status": "canceled"},
                {"name": "c", "status": "skipped"}, {"name": "d", "status": "manual"},
                {"name": "e", "status": "something-new"}
            ]),
        ),
        Canned::json(
            "POST",
            &format!("{PROJECT}/merge_requests/7/notes"),
            422,
            json!({"message": ["too short", "no"]}),
        ),
        Canned::json(
            "POST",
            &format!("{PROJECT}/merge_requests/7/notes"),
            422,
            json!({"message": {"body": "is blank", "title": ["x"]}}),
        ),
        Canned::json(
            "POST",
            &format!("{PROJECT}/merge_requests/7/notes"),
            422,
            json!({"other": 1}),
        ),
        Canned::empty("POST", &format!("{PROJECT}/merge_requests/7/approve"), 200),
        Canned::json(
            "GET",
            &format!("{PROJECT}/merge_requests/7"),
            200,
            mr(7, "opened", "mergeable"),
        ),
        Canned::empty("POST", &format!("{PROJECT}/merge_requests/7/approve"), 200),
        Canned::json(
            "GET",
            &format!("{PROJECT}/merge_requests/7"),
            200,
            mr_with_diff(7),
        ),
        Canned::json(
            "POST",
            &format!("{PROJECT}/merge_requests/7/discussions"),
            200,
            json!({"id": "d9"}),
        ),
        Canned::empty("GET", "/user", 200),
        Canned::empty("GET", PROJECT, 200),
    ])
    .await;
    let dir = tempfile::tempdir().unwrap();
    let api = Arc::new(GitLabApi::with_base_url(
        stub.base_url(),
        stored(CodeHostKind::GitLab, "gitlab.com", dir.path()),
    ));
    assert_eq!(api.host(), "gitlab.com");
    assert_eq!(api.bound_account(), None);
    assert!(CodeHost::capabilities(&*api)
        .review_events
        .contains(&ReviewEvent::Approve));
    let bound = Arc::clone(&api).for_account(Some(" Ada "));
    let again = Arc::clone(&api).for_account(None);
    assert!(
        Arc::ptr_eq(&again, &(api.clone() as Arc<dyn CodeHost>)),
        "unbound stays itself"
    );
    drop(bound);

    let closed = api
        .list_prs(
            &gitlab_repo(),
            PrFilter {
                state: Some(PrState::Closed),
                head: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        (closed[0].state, closed[0].mergeable),
        (PrState::Closed, None)
    );
    let all = api
        .list_prs(
            &gitlab_repo(),
            PrFilter {
                state: Some(PrState::Unknown),
                head: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        (all[0].state, all[0].mergeable),
        (PrState::Unknown, Some(false))
    );
    let queries: Vec<String> = stub
        .calls()
        .iter()
        .filter_map(|c| c.query.clone())
        .collect();
    assert!(
        queries[0].contains("state=closed") && queries[1].contains("state=all"),
        "{queries:?}"
    );

    let checks = api.checks(&gitlab_repo(), 7).await.unwrap();
    assert_eq!(
        checks
            .iter()
            .map(|c| (c.status.as_str(), c.conclusion.as_deref()))
            .collect::<Vec<_>>(),
        [
            ("queued", None),
            ("completed", Some("cancelled")),
            ("completed", Some("skipped")),
            ("completed", Some("action_required")),
            ("completed", Some("neutral")),
        ]
    );

    let comment = |body: &str| Review {
        event: ReviewEvent::Comment,
        body: Some(body.into()),
        comments: vec![],
    };
    let listed = api
        .submit_review(&gitlab_repo(), 7, comment("a"))
        .await
        .unwrap_err();
    assert!(listed.to_string().contains("too short; no"), "{listed}");
    let fielded = api
        .submit_review(&gitlab_repo(), 7, comment("b"))
        .await
        .unwrap_err();
    assert!(
        fielded.to_string().contains("body \"is blank\"")
            && fielded.to_string().contains("title x"),
        "{fielded}"
    );
    let wordless = api
        .submit_review(&gitlab_repo(), 7, comment("c"))
        .await
        .unwrap_err();
    assert!(
        matches!(&wordless, CodeHostError::Refused(w) if w.ends_with("422 Unprocessable Entity")),
        "a refusal with no message is the status's own words: {wordless}"
    );

    // An approval the host acknowledges with an empty body, then an inline
    // comment on the left side: refused while the merge request has no diff,
    // posted against the old line once it has one.
    let left = || Review {
        event: ReviewEvent::Approve,
        body: None,
        comments: vec![ReviewComment {
            path: "a.rs".into(),
            line: 3,
            side: Side::Left,
            start_line: None,
            start_side: None,
            body: "old".into(),
        }],
    };
    let err = api
        .submit_review(&gitlab_repo(), 7, left())
        .await
        .unwrap_err();
    assert!(
        matches!(&err, CodeHostError::Refused(w) if w.contains("no diff to comment on yet")),
        "{err}"
    );
    api.submit_review(&gitlab_repo(), 7, left()).await.unwrap();
    let discussion = stub
        .calls()
        .into_iter()
        .rev()
        .find(|c| c.path.ends_with("/discussions"))
        .unwrap();
    assert_eq!(discussion.body["position"]["old_line"], json!(3));
    assert!(discussion.body["position"].get("new_line").is_none());
    let err = api.account().await.unwrap_err();
    assert!(
        matches!(&err, CodeHostError::Transport(w) if w.contains("answered something unexpected")),
        "{err}"
    );
    let err = api.repo_access(&gitlab_repo()).await.unwrap_err();
    assert!(
        matches!(&err, CodeHostError::Transport(w) if w.contains("answered something unexpected")),
        "{err}"
    );
}

// --- the Bitbucket API at its edges ----------------------------------------------------

/// Git's credential helper, answering one password for every host.
#[derive(Debug)]
struct HelperSaying(&'static str);

#[async_trait::async_trait]
impl bisa_codehost::creds::GitCredentials for HelperSaying {
    async fn fill(&self, _host: &str) -> Option<bisa_codehost::creds::Credential> {
        Some(bisa_codehost::creds::Credential {
            username: "someone".into(),
            password: bisa_codehost::creds::Secret::new(self.0),
        })
    }
    async fn helpers(&self) -> Vec<String> {
        vec!["sh".into()]
    }
}

fn bb_pr(id: u64, state: &str) -> serde_json::Value {
    json!({
        "id": id, "title": "t", "state": state, "draft": false,
        "source": {"branch": {"name": "work/x"}, "commit": {"hash": "abc"}},
        "destination": {"branch": {"name": "main"}},
        "author": null, "links": {"html": null}, "participants": [], "merge_commit": null
    })
}

#[tokio::test]
async fn bitbucket_reads_a_declined_or_strange_pull_request_a_stopped_status_and_needs_a_login() {
    let stub = Stub::start(vec![
        Canned::json(
            "GET",
            "/repositories/acme/web/pullrequests/1",
            200,
            bb_pr(1, "DECLINED"),
        ),
        Canned::json(
            "GET",
            "/repositories/acme/web/pullrequests/2",
            200,
            bb_pr(2, "ODD"),
        ),
        Canned::json(
            "GET",
            "/repositories/acme/web/pullrequests/3",
            200,
            bb_pr(3, "OPEN"),
        ),
        Canned::json(
            "GET",
            "/repositories/acme/web/commit/abc/statuses",
            200,
            json!({"values": [{"key": "ci", "state": "STOPPED"}, {"state": "NEW"}]}),
        ),
        Canned::empty("GET", "/user", 200),
        Canned::json(
            "POST",
            "/repositories/acme/web/pullrequests/1/comments/2/resolve",
            200,
            json!({}),
        ),
    ])
    .await;
    let dir = tempfile::tempdir().unwrap();
    let tokens = TokenStore::file_only(CodeHostKind::Bitbucket, "bitbucket.org", dir.path());
    tokens.store("ada", "a-word-the-stub-ignores").unwrap();
    let api = Arc::new(BitbucketApi::with_base_url(stub.base_url(), tokens));
    assert_eq!(api.host(), "bitbucket.org");
    assert_eq!(api.bound_account(), None);
    assert!(CodeHost::capabilities(&*api)
        .review_events
        .contains(&ReviewEvent::RequestChanges));
    let same = Arc::clone(&api).for_account(None);
    assert!(Arc::ptr_eq(&same, &(api.clone() as Arc<dyn CodeHost>)));
    let bound = Arc::clone(&api).for_account(Some("ADA"));

    let repo = RepoRef {
        host: "bitbucket.org".into(),
        owner: "acme".into(),
        name: "web".into(),
    };
    let declined = bound.get_pr(&repo, 1).await.unwrap();
    assert_eq!((declined.state, declined.author), (PrState::Closed, None));
    assert_eq!(declined.url, "", "no html link, no url");
    assert_eq!(
        bound.get_pr(&repo, 2).await.unwrap().state,
        PrState::Unknown
    );
    let checks = bound.checks(&repo, 3).await.unwrap();
    assert_eq!(
        checks
            .iter()
            .map(|c| (c.name.as_str(), c.status.as_str(), c.conclusion.as_deref()))
            .collect::<Vec<_>>(),
        [
            ("ci", "completed", Some("cancelled")),
            ("status", "queued", None)
        ]
    );
    let err = bound.account().await.unwrap_err();
    assert!(
        matches!(&err, CodeHostError::Transport(w) if w.contains("answered something unexpected")),
        "{err}"
    );
    let bad = bound
        .resolve_review_thread("no-hashes", true)
        .await
        .unwrap_err();
    assert!(matches!(&bad, CodeHostError::NotFound(_)), "{bad}");
    assert_eq!(
        stub.calls()[0]
            .authorization
            .as_deref()
            .map(|a| a.starts_with("Basic ")),
        Some(true),
        "the bound login beside the token, as a Basic credential"
    );

    // Nothing stored and nothing bound: git's helper answers, and its
    // password is a credential only when it carries the login — else the
    // refusal names the account page and the variable.
    if no_env_token(CodeHostKind::Bitbucket) {
        for (password, ok) in [("ada:from-the-helper", true), ("no-login-in-it", false)] {
            let helper = TokenStore::file_only(
                CodeHostKind::Bitbucket,
                "bitbucket.org",
                dir.path().join("nothing-stored"),
            )
            .with_git(Arc::new(HelperSaying(password)));
            let api = BitbucketApi::with_base_url(stub.base_url(), helper);
            let answer = api.resolve_review_thread("acme/web#1#2", true).await;
            if ok {
                assert!(answer.is_ok(), "{answer:?}");
                let last = stub.calls().pop().unwrap();
                assert!(last.path.ends_with("/comments/2/resolve"), "{last:?}");
            } else {
                let err = answer.unwrap_err();
                assert!(
                    matches!(&err, CodeHostError::NotAuthenticated(w) if w.contains("needs the login")),
                    "{err}"
                );
            }
        }
    }

    // Two stored logins and none bound: the token alone cannot say whose it
    // is, and the refusal names the account page and the variable.
    if no_env_token(CodeHostKind::Bitbucket) {
        let two = TokenStore::file_only(CodeHostKind::Bitbucket, "bitbucket.org", dir.path());
        two.store("bob", "another-word").unwrap();
        let unbound = BitbucketApi::with_base_url(stub.base_url(), two);
        let err = unbound.get_pr(&repo, 1).await.unwrap_err();
        assert!(
            matches!(&err, CodeHostError::NotAuthenticated(w) if w.contains("@ada") && w.contains("@bob")),
            "{err}"
        );
    }
}
