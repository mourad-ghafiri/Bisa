//! `gh`, asked through the scripted runner: which argv each operation runs,
//! what it reads back, and when it steps aside. No program is spawned.

use bisa_codehost::cli::fake::FakeCli;
use bisa_codehost::cli::gh::GhCli;
use bisa_codehost::cli::{CliProgram, CliStep, CodeHostCli, Fallback};
use bisa_codehost::creds::CliToken;
use bisa_codehost::{
    CodeHostError, MergeStrategy, PrCreate, PrFilter, PrState, RepoRef, Review, ReviewComment,
    ReviewEvent,
};
use std::sync::Arc;

const PR_JSON: &str = r#"{"number":12,"url":"https://github.com/acme/web/pull/12","title":"t","state":"OPEN","isDraft":true,"mergeable":"MERGEABLE","headRefName":"work/x","headRefOid":"abc","baseRefName":"main","author":{"id":"MDQ6VXNlcjU4MzIzMQ==","is_bot":false,"login":"octocat","name":"The Octocat"},"mergeCommit":null}"#;
const MERGED_JSON: &str = r#"{"number":12,"url":"u","title":"t","state":"MERGED","isDraft":false,"mergeable":"UNKNOWN","headRefName":"work/x","headRefOid":"abc","baseRefName":"main","author":{"id":"MDQ6VXNlcjU4MzIzMQ==","is_bot":false,"login":"octocat","name":"The Octocat"},"mergeCommit":{"oid":"deadbeef"}}"#;
const STATUS: &str = "github.com\n  ✓ Logged in to github.com account octocat (keyring)\n  - Active account: true\n  - Git operations protocol: https\n  - Token: gho_************************************\n";

fn repo() -> RepoRef {
    RepoRef {
        host: "github.com".into(),
        owner: "acme".into(),
        name: "web".into(),
    }
}

fn installed() -> FakeCli {
    FakeCli::new().with_program(CliProgram::Gh, "/opt/homebrew/bin/gh")
}

fn gh_over(cli: FakeCli) -> (GhCli, Arc<FakeCli>) {
    let cli = Arc::new(cli);
    (GhCli::new(cli.clone(), "github.com"), cli)
}

#[tokio::test]
async fn the_probe_says_installed_version_and_accounts_and_the_token_is_a_secret() {
    let (absent, _) = gh_over(FakeCli::new());
    let probe = absent.probe().await;
    assert!(!probe.installed && probe.accounts.is_empty() && probe.version.is_none());
    assert!(absent.active_login().await.is_none());

    let (present, cli) = gh_over(
        installed()
            .always(
                CliProgram::Gh,
                &["--version"],
                0,
                "gh version 2.63.2 (2024-12-05)\n",
                "",
            )
            .always(CliProgram::Gh, &["auth", "status"], 0, STATUS, "")
            .answer(CliProgram::Gh, &["auth", "token"], 0, "gho_secret\n", ""),
    );
    let probe = present.probe().await;
    assert_eq!(probe.path.as_deref(), Some("/opt/homebrew/bin/gh"));
    assert_eq!(probe.version.as_deref(), Some("2.63.2"));
    assert_eq!(probe.accounts.len(), 1);
    assert_eq!(probe.active().unwrap().login, "octocat");
    assert_eq!(present.active_login().await.as_deref(), Some("octocat"));
    let (who, token) = present
        .token(None)
        .await
        .expect("the active account's token");
    assert_eq!(who, "octocat");
    assert_eq!(token.expose(), "gho_secret");
    assert!(!format!("{token:?}").contains("gho_"));
    assert!(
        cli.lines()
            .iter()
            .any(|l| l == "gh auth token --hostname github.com"),
        "{:?}",
        cli.lines()
    );

    let (unsigned, _) = gh_over(
        installed()
            .always(CliProgram::Gh, &["--version"], 0, "gh version 2.63.2", "")
            .always(
                CliProgram::Gh,
                &["auth", "status"],
                1,
                "",
                "You are not logged into any GitHub hosts. To log in, run: gh auth login\n",
            ),
    );
    let probe = unsigned.probe().await;
    assert!(probe.installed && !probe.signed_in());
    assert!(probe.detail.as_deref().unwrap().contains("not logged into"));
}

#[tokio::test]
async fn creating_a_pull_request_runs_pr_create_then_reads_it_back() {
    let (gh, cli) = gh_over(installed()
        .answer(CliProgram::Gh, &["pr", "create"], 0, "Creating pull request for work/x into main in acme/web\n\nhttps://github.com/acme/web/pull/12\n", "")
        .answer(CliProgram::Gh, &["pr", "view", "12"], 0, PR_JSON, ""));
    let pr = gh
        .create_pr(
            &repo(),
            PrCreate {
                title: "Add checkout".into(),
                body: "Closes #4".into(),
                head: "work/x".into(),
                base: "main".into(),
                draft: true,
                reviewers: vec!["alice".into()],
                labels: vec!["feature".into()],
            },
        )
        .await
        .unwrap();
    assert_eq!(
        (pr.number, pr.is_draft, pr.state),
        (12, true, PrState::Open)
    );
    let lines = cli.lines();
    assert_eq!(
        lines[0],
        "gh pr create -R github.com/acme/web --title Add checkout --body Closes #4 --head work/x --base main --draft --reviewer alice --label feature"
    );
    assert!(lines[1].starts_with("gh pr view 12 -R github.com/acme/web --json "));
    assert_eq!(
        cli.calls()[0].env("GH_HOST"),
        None,
        "github.com needs no GH_HOST"
    );

    // Created but not readable: the host's, never a fallback (the API would create another).
    let (gh, _) = gh_over(
        installed()
            .answer(
                CliProgram::Gh,
                &["pr", "create"],
                0,
                "https://github.com/acme/web/pull/13\n",
                "",
            )
            .answer(
                CliProgram::Gh,
                &["pr", "view", "13"],
                1,
                "",
                "dial tcp: no route to host",
            ),
    );
    let err = gh
        .create_pr(&repo(), PrCreate::default())
        .await
        .unwrap_err();
    assert!(
        matches!(&err, CliStep::Host(CodeHostError::Transport(m)) if m.contains("#13 was created")),
        "{err:?}"
    );
}

#[tokio::test]
async fn listing_checks_reviews_merging_and_deleting_run_the_verbs_they_have() {
    let (gh, cli) = gh_over(installed()
        .answer(CliProgram::Gh, &["pr", "list"], 0, &format!("[{PR_JSON}]"), "")
        .answer(CliProgram::Gh, &["pr", "checks"], 8, r#"[{"name":"ci","state":"IN_PROGRESS","bucket":"pending","link":"","description":""},{"name":"lint","state":"SUCCESS","bucket":"pass","link":"https://x","description":""}]"#, "")
        .answer(CliProgram::Gh, &["pr", "review", "--approve"], 0, "", "")
        .answer(CliProgram::Gh, &["api", "graphql"], 0, r#"{"data":{"repository":{"pullRequest":{"reviews":{"nodes":[{"author":{"login":"alice"},"state":"APPROVED","body":"ship","submittedAt":"2026-01-01T00:00:00Z"}]},"reviewThreads":{"nodes":[{"id":"T1","isResolved":false,"isOutdated":false,"path":"a.rs","line":3,"comments":{"nodes":[{"author":{"login":"alice"},"body":"rename","createdAt":null}]}}]}}}}}"#, "")
        .answer(CliProgram::Gh, &["api", "graphql", "id=T1"], 0, r#"{"data":{"resolveReviewThread":{"thread":{"id":"T1"}}}}"#, "")
        .answer(CliProgram::Gh, &["api", "graphql", "body="], 0, r#"{"data":{"addPullRequestReviewThreadReply":{"comment":{"id":"C1"}}}}"#, "")
        .answer(CliProgram::Gh, &["pr", "merge", "--squash"], 0, "", "")
        .answer(CliProgram::Gh, &["pr", "view", "12"], 0, MERGED_JSON, "")
        .answer(CliProgram::Gh, &["api", "--method", "DELETE"], 0, "", ""));

    let merged = gh
        .list_prs(
            &repo(),
            PrFilter {
                state: Some(PrState::Merged),
                head: Some("work/x".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(merged.len(), 1);
    let checks = gh.checks(&repo(), 12).await.unwrap();
    assert_eq!(
        checks.len(),
        2,
        "exit 8 with JSON is an answer: checks are still running"
    );
    assert_eq!(
        (checks[0].status.as_str(), checks[1].conclusion.as_deref()),
        ("in_progress", Some("success"))
    );
    gh.submit_review(
        &repo(),
        12,
        Review {
            event: ReviewEvent::Approve,
            body: Some("Looks good".into()),
            comments: vec![],
        },
    )
    .await
    .unwrap();
    let reviews = gh.pr_reviews(&repo(), 12).await.unwrap();
    assert_eq!(
        (
            reviews.reviews[0].state.as_str(),
            reviews.threads[0].id.as_str(),
            reviews.threads[0].line
        ),
        ("approved", "T1", Some(3))
    );
    gh.resolve_review_thread("T1", true).await.unwrap();
    gh.reply_review_thread("T1", "Renamed in abc123.")
        .await
        .unwrap();
    let outcome = gh.merge(&repo(), 12, MergeStrategy::Squash).await.unwrap();
    assert_eq!(
        (outcome.merged, outcome.sha.as_deref()),
        (true, Some("deadbeef"))
    );
    gh.delete_branch(&repo(), "work/x").await.unwrap();

    let lines = cli.lines();
    assert!(
        lines[0].starts_with("gh pr list -R github.com/acme/web --state merged --limit 50 --json ")
            && lines[0].ends_with(" --head work/x"),
        "{}",
        lines[0]
    );
    assert_eq!(
        lines[1],
        "gh pr checks 12 -R github.com/acme/web --json name,state,bucket,link,description"
    );
    assert_eq!(
        lines[2],
        "gh pr review 12 -R github.com/acme/web --approve --body Looks good"
    );
    assert!(
        lines[3].starts_with("gh api graphql -f query=")
            && lines[3].contains("-f owner=acme -f name=web -F number=12"),
        "{}",
        lines[3]
    );
    assert!(
        lines[4].contains("resolveReviewThread") && lines[4].ends_with("-f id=T1"),
        "{}",
        lines[4]
    );
    assert!(
        lines[5].contains("addPullRequestReviewThreadReply")
            && lines[5].ends_with("-f id=T1 -f body=Renamed in abc123."),
        "{}",
        lines[5]
    );
    assert_eq!(lines[6], "gh pr merge 12 -R github.com/acme/web --squash");
    assert_eq!(
        lines[8],
        "gh api --method DELETE repos/acme/web/git/refs/heads/work/x"
    );
    assert_eq!(cli.unconsumed(), 0);
}

#[tokio::test]
async fn an_inline_review_goes_through_gh_api_with_the_rest_body_on_stdin() {
    let (gh, cli) = gh_over(installed().answer(
        CliProgram::Gh,
        &["api", "--method", "POST", "--input", "-"],
        0,
        "{}",
        "",
    ));
    let review = Review {
        event: ReviewEvent::Comment,
        body: Some("One nit".into()),
        comments: vec![ReviewComment {
            path: "a.rs".into(),
            line: 3,
            side: Default::default(),
            start_line: None,
            start_side: None,
            body: "rename".into(),
        }],
    };
    gh.submit_review(&repo(), 12, review).await.unwrap();
    let call = &cli.calls()[0];
    assert_eq!(
        call.line(),
        "gh api --method POST repos/acme/web/pulls/12/reviews --input -"
    );
    let body: serde_json::Value = serde_json::from_str(call.stdin.as_deref().unwrap()).unwrap();
    assert_eq!(body["event"], "COMMENT");
    assert_eq!(body["comments"][0]["path"], "a.rs");
    let wordless = gh
        .submit_review(
            &repo(),
            12,
            Review {
                event: ReviewEvent::Comment,
                body: None,
                comments: vec![],
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(wordless, CliStep::Host(CodeHostError::Refused(_))),
        "a wordless comment is refused before anything runs"
    );
}

#[tokio::test]
async fn repository_access_reads_permissions_and_a_404_is_not_found_not_an_error() {
    let (gh, _) = gh_over(
        installed()
            .answer(
                CliProgram::Gh,
                &["api", "repos/acme/web"],
                0,
                r#"{"permissions":{"push":true}}"#,
                "",
            )
            .answer(
                CliProgram::Gh,
                &["api", "repos/acme/web"],
                1,
                "",
                "gh: Not Found (HTTP 404)",
            ),
    );
    let access = gh.repo_access(&repo()).await.unwrap();
    assert!(access.found && access.push);
    let gone = gh.repo_access(&repo()).await.unwrap();
    assert!(!gone.found && !gone.push);
}

#[tokio::test]
async fn an_enterprise_host_travels_as_gh_host_and_a_missing_account_is_a_fallback() {
    let cli = Arc::new(installed().answer(CliProgram::Gh, &["pr", "view"], 0, PR_JSON, ""));
    let ghe = GhCli::new(cli.clone(), "GitHub.Acme.internal");
    ghe.get_pr(&repo(), 12).await.unwrap();
    assert_eq!(cli.calls()[0].env("GH_HOST"), Some("github.acme.internal"));

    let cli = Arc::new(installed().answer(
        CliProgram::Gh,
        &["auth", "token", "--user", "nobody"],
        1,
        "",
        "no oauth token",
    ));
    let bound = Arc::new(GhCli::new(cli.clone(), "github.com")).for_account(Some("nobody"));
    let err = bound.get_pr(&repo(), 12).await.unwrap_err();
    assert!(
        matches!(&err, CliStep::Fallback(Fallback::NoSuchAccount(l)) if l == "nobody"),
        "{err:?}"
    );
    assert_eq!(
        cli.lines(),
        ["gh auth token --hostname github.com --user nobody"],
        "nothing else ran"
    );

    let (unsigned, _) = gh_over(installed().answer(
        CliProgram::Gh,
        &["pr", "view"],
        1,
        "",
        "To get started with GitHub CLI, please run:  gh auth login",
    ));
    assert!(matches!(
        unsigned.get_pr(&repo(), 12).await.unwrap_err(),
        CliStep::Fallback(Fallback::NotSignedIn)
    ));
    let (absent, _) = gh_over(FakeCli::new());
    assert!(matches!(
        absent.get_pr(&repo(), 12).await.unwrap_err(),
        CliStep::Fallback(Fallback::NotInstalled)
    ));
}
