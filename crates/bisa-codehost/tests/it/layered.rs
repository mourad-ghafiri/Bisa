//! The layered code host: the CLI first, the API only when the CLI had no
//! say, never when the host itself refused through the CLI. Everything runs
//! against `FakeCli` (no program spawned) and `FakeCodeHost` (no network).

use bisa_codehost::cli::fake::FakeCli;
use bisa_codehost::cli::gh::GhCli;
use bisa_codehost::cli::CliProgram;
use bisa_codehost::fake::FakeCodeHost;
use bisa_codehost::layered::Layered;
use bisa_codehost::{
    CodeHost, CodeHostCapabilities, CodeHostError, CodeHostKind, RepoRef, ReviewThread,
};
use std::sync::Arc;

const PR_JSON: &str = r#"{"number":12,"url":"https://github.com/acme/web/pull/12","title":"From the CLI","state":"OPEN","isDraft":false,"mergeable":"MERGEABLE","headRefName":"work/x","headRefOid":"abc","baseRefName":"main","author":{"login":"octocat"},"mergeCommit":null}"#;

fn repo() -> RepoRef {
    RepoRef {
        host: "github.com".into(),
        owner: "acme".into(),
        name: "web".into(),
    }
}

/// A layered GitHub over a fake API (with one seeded pull request) and `gh`
/// through the scripted runner.
fn layered(cli: Arc<FakeCli>) -> (Arc<Layered>, Arc<FakeCodeHost>) {
    let api =
        Arc::new(FakeCodeHost::of_kind(CodeHostKind::GitHub, "github.com").signed_in_as("you"));
    api.seed_pr(&repo(), "alice", "work/x", "main");
    let gh = GhCli::new(cli, "github.com");
    let host = Arc::new(Layered::new(
        api.clone() as Arc<dyn CodeHost>,
        Some(Arc::new(gh)),
    ));
    (host, api)
}

#[tokio::test]
async fn without_the_cli_installed_the_api_answers_and_the_cli_is_never_run() {
    let cli = Arc::new(FakeCli::new());
    let (host, api) = layered(cli.clone());
    let pr = host.get_pr(&repo(), 1).await.unwrap();
    assert_eq!(pr.title, "work/x into main", "the fake API's pull request");
    assert_eq!(
        api.asked_as(),
        ["you"],
        "the API was asked, as the chain's account"
    );
    assert_eq!(host.id().0, "github");
    assert!(
        cli.lines().is_empty(),
        "a program that is not installed is never asked to run"
    );
}

#[tokio::test]
async fn with_the_cli_signed_in_it_answers_and_the_api_is_not_asked() {
    let cli = Arc::new(
        FakeCli::new()
            .with_program(CliProgram::Gh, "/usr/local/bin/gh")
            .answer(CliProgram::Gh, &["pr", "view", "12"], 0, PR_JSON, ""),
    );
    let (host, api) = layered(cli.clone());
    let pr = host.get_pr(&repo(), 12).await.unwrap();
    assert_eq!(pr.title, "From the CLI");
    assert_eq!(pr.author.as_deref(), Some("octocat"));
    assert!(api.asked_as().is_empty(), "the API was never asked");
    assert_eq!(cli.lines(), ["gh pr view 12 -R github.com/acme/web --json number,url,title,state,isDraft,mergeable,headRefName,headRefOid,baseRefName,author,mergeCommit"]);
}

#[tokio::test]
async fn a_cli_that_is_not_signed_in_falls_through_to_the_api() {
    let cli = Arc::new(
        FakeCli::new()
            .with_program(CliProgram::Gh,"/usr/local/bin/gh")
            .answer(CliProgram::Gh, &["pr", "view"], 1, "", "To get started with GitHub CLI, please run:  gh auth login\nAlternatively, populate the GH_TOKEN environment variable with a GitHub API authentication token."),
    );
    let (host, api) = layered(cli.clone());
    let pr = host.get_pr(&repo(), 1).await.unwrap();
    assert_eq!(pr.title, "work/x into main", "the API's answer");
    assert_eq!(api.asked_as(), ["you"]);
}

/// A reply on a thread: the CLI's `gh api graphql` answers when it is signed
/// in; a CLI with no say falls through to the API's reply, which appends the
/// comment on the fake's thread.
#[tokio::test]
async fn a_reply_on_a_thread_goes_through_the_cli_when_it_can_and_the_api_otherwise() {
    let signed_in = Arc::new(
        FakeCli::new()
            .with_program(CliProgram::Gh, "/usr/local/bin/gh")
            .answer(
                CliProgram::Gh,
                &["api", "graphql", "body="],
                0,
                r#"{"data":{"addPullRequestReviewThreadReply":{"comment":{"id":"C1"}}}}"#,
                "",
            ),
    );
    let (host, api) = layered(signed_in.clone());
    host.reply_review_thread("PRRT_1", "Renamed in abc123.")
        .await
        .unwrap();
    assert!(api.asked_as().is_empty(), "the API was never asked");
    assert!(
        signed_in.lines()[0].contains("addPullRequestReviewThreadReply"),
        "{}",
        signed_in.lines()[0]
    );

    let signed_out = Arc::new(
        FakeCli::new()
            .with_program(CliProgram::Gh, "/usr/local/bin/gh")
            .answer(
                CliProgram::Gh,
                &["api", "graphql"],
                1,
                "",
                "To get started with GitHub CLI, please run:  gh auth login",
            ),
    );
    let api = Arc::new(FakeCodeHost {
        caps: CodeHostCapabilities {
            review_threads: true,
            review_thread_replies: true,
            ..Default::default()
        },
        ..FakeCodeHost::of_kind(CodeHostKind::GitHub, "github.com").signed_in_as("you")
    });
    api.state.threads.lock().unwrap().push(ReviewThread {
        id: "PRRT_1".into(),
        path: Some("a.rs".into()),
        line: Some(3),
        is_resolved: false,
        is_outdated: false,
        comments: vec![],
    });
    let host = Layered::new(
        api.clone() as Arc<dyn CodeHost>,
        Some(Arc::new(GhCli::new(signed_out, "github.com"))),
    );
    host.reply_review_thread("PRRT_1", "Renamed in abc123.")
        .await
        .unwrap();
    assert_eq!(
        api.asked_as(),
        ["you"],
        "the API answered as the chain's account"
    );
    assert_eq!(
        api.state.threads.lock().unwrap()[0].comments[0].body,
        "Renamed in abc123."
    );
}

#[tokio::test]
async fn the_hosts_own_refusal_through_the_cli_is_the_answer_and_the_api_is_not_asked_again() {
    let cli = Arc::new(
        FakeCli::new()
            .with_program(CliProgram::Gh,"/usr/local/bin/gh")
            .answer(CliProgram::Gh, &["pr", "view", "99"], 1, "", "GraphQL: Could not resolve to a PullRequest with the number of 99. (repository.pullRequest)"),
    );
    let (host, api) = layered(cli.clone());
    let err = host.get_pr(&repo(), 99).await.unwrap_err();
    assert!(matches!(err, CodeHostError::NotFound(_)), "{err}");
    assert!(
        api.asked_as().is_empty(),
        "a refusal is not a reason to ask twice"
    );
}

#[tokio::test]
async fn binding_an_account_binds_both_layers_and_the_clis_token_travels_in_the_environment_once() {
    let cli = Arc::new(
        FakeCli::new()
            .with_program(CliProgram::Gh, "/usr/local/bin/gh")
            .answer(
                CliProgram::Gh,
                &["auth", "token", "--user", "ada"],
                0,
                "gho_ada_secret\n",
                "",
            )
            .answer(CliProgram::Gh, &["pr", "view", "12"], 0, PR_JSON, ""),
    );
    let (host, api) = layered(cli.clone());
    let bound = host.for_account(Some("Ada"));
    bound.get_pr(&repo(), 12).await.unwrap();
    let calls = cli.calls();
    assert_eq!(
        calls[0].line(),
        "gh auth token --hostname github.com --user ada"
    );
    assert_eq!(
        calls[0].env("GH_TOKEN"),
        None,
        "asking for the token carries none"
    );
    assert_eq!(
        calls[1].env("GH_TOKEN"),
        Some("gho_ada_secret"),
        "the token rides on that one command"
    );
    assert!(api.asked_as().is_empty());

    // An account the CLI does not hold: the API, bound to the same login.
    let cli = Arc::new(
        FakeCli::new()
            .with_program(CliProgram::Gh, "/usr/local/bin/gh")
            .answer(
                CliProgram::Gh,
                &["auth", "token", "--user", "ada"],
                1,
                "",
                "no oauth token found for ada",
            ),
    );
    let (host, api) = layered(cli.clone());
    host.for_account(Some("ada"))
        .get_pr(&repo(), 1)
        .await
        .unwrap();
    assert_eq!(
        api.asked_as(),
        ["ada"],
        "the API layer speaks as the bound account"
    );
}

#[tokio::test]
async fn a_token_is_checked_by_the_api_alone_and_the_probe_is_the_clis() {
    let cli = Arc::new(
        FakeCli::new()
            .with_program(CliProgram::Gh,"/usr/local/bin/gh")
            .answer(CliProgram::Gh, &["--version"], 0, "gh version 2.63.2 (2024-12-05)", "")
            .answer(CliProgram::Gh, &["auth", "status"], 0, "github.com\n  ✓ Logged in to github.com account octocat (keyring)\n  - Active account: true\n  - Git operations protocol: https\n  - Token: gho_****\n", ""),
    );
    let (host, api) = layered(cli.clone());
    let me = host.verify_token("ghp_candidate", None).await.unwrap();
    assert_eq!(me.login, "you", "the fake API checked it");
    assert_eq!(
        cli.lines().len(),
        0,
        "the CLI keeps its own credential; a pasted token is the API's to check"
    );
    let probe = host.probe().await.expect("GitHub has a CLI");
    assert!(probe.installed && probe.signed_in());
    assert_eq!(probe.version.as_deref(), Some("2.63.2"));
    assert_eq!(probe.active().map(|a| a.login.as_str()), Some("octocat"));
    assert!(!format!("{probe:?}").contains("gho_"));
    let api_only = Layered::api_only(api as Arc<dyn CodeHost>);
    assert!(api_only.probe().await.is_none());
}
