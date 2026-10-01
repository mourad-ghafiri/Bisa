//! The GitHub implementation against a stub on the loopback interface: every
//! trait method hits the documented endpoint with the documented body, every
//! status maps to the typed error it should, and the rate limit is a wait, not
//! a bad token. Nothing here reaches the network; the stub records what was
//! asked and answers what the test canned.

use crate::support;

use bisa_codehost::creds::TokenStore;
use bisa_codehost::github::{GitHubApi, HOST};
use bisa_codehost::CodeHostKind;

/// A file store for github.com under a temp dir — never the workspace's.
fn store(dir: &tempfile::TempDir) -> TokenStore {
    TokenStore::file_only(CodeHostKind::GitHub, HOST, dir.path())
}
use bisa_codehost::{
    CodeHost, CodeHostError, MergeStrategy, PrCreate, PrFilter, PrState, RemoteUrl, RepoRef,
    Review, ReviewComment, ReviewEvent, Side,
};
use serde_json::json;
use std::sync::Arc;
use support::{Canned, Stub};

fn repo() -> RepoRef {
    RepoRef {
        host: "github.com".into(),
        owner: "acme".into(),
        name: "web".into(),
    }
}

/// A GitHub code host pointed at the stub, with one account's token in a file
/// store under a temp dir (the environment's token, if the developer has one,
/// is sent instead — the stub does not care which; it checks that one is sent).
async fn host(stub: &Stub) -> (GitHubApi, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let tokens = store(&dir);
    tokens.store("stub", "ghp_stub").unwrap();
    (GitHubApi::with_base_url(stub.base_url(), tokens), dir)
}

fn no_env_token() -> bool {
    std::env::var("BISA_GITHUB_TOKEN").map_or(true, |v| v.trim().is_empty())
}

fn pull(number: u64, state: &str, merged_at: Option<&str>) -> serde_json::Value {
    json!({
        "number": number,
        "html_url": format!("https://github.com/acme/web/pull/{number}"),
        "title": format!("PR {number}"),
        "state": state,
        "draft": false,
        "merged": merged_at.is_some(),
        "merged_at": merged_at,
        "mergeable": true,
        "head": {"ref": "feature/x", "sha": "a".repeat(40)},
        "base": {"ref": "main", "sha": "b".repeat(40)},
        "user": {"login": "octocat"},
    })
}

#[tokio::test]
async fn every_request_carries_the_token_and_the_api_headers() {
    let stub = Stub::start(vec![Canned::json(
        "GET",
        "/repos/acme/web/pulls/7",
        200,
        pull(7, "open", None),
    )])
    .await;
    let (gh, _dir) = host(&stub).await;
    gh.get_pr(&repo(), 7).await.unwrap();
    let calls = stub.calls();
    assert_eq!(calls.len(), 1);
    assert!(calls[0].bearer, "a bearer token is sent");
    assert_eq!(
        calls[0].header("accept").as_deref(),
        Some("application/vnd.github+json")
    );
    assert_eq!(
        calls[0].header("x-github-api-version").as_deref(),
        Some("2022-11-28")
    );
    assert!(calls[0]
        .header("user-agent")
        .as_deref()
        .is_some_and(|ua| ua.contains("bisa")));
}

#[tokio::test]
async fn creating_a_pull_request_posts_it_then_its_reviewers_then_its_labels() {
    let stub = Stub::start(vec![
        Canned::json("POST", "/repos/acme/web/pulls", 201, pull(12, "open", None)),
        Canned::json(
            "POST",
            "/repos/acme/web/pulls/12/requested_reviewers",
            201,
            json!({}),
        ),
        Canned::json("POST", "/repos/acme/web/issues/12/labels", 200, json!([])),
    ])
    .await;
    let (gh, _dir) = host(&stub).await;
    let pr = gh
        .create_pr(
            &repo(),
            PrCreate {
                title: "Dark mode".into(),
                body: "as discussed".into(),
                head: "feature/x".into(),
                base: "main".into(),
                draft: true,
                reviewers: vec!["alice".into()],
                labels: vec!["ui".into()],
            },
        )
        .await
        .unwrap();
    assert_eq!(pr.number, 12);
    assert_eq!(pr.author.as_deref(), Some("octocat"));
    let calls = stub.calls();
    let paths: Vec<_> = calls
        .iter()
        .map(|c| format!("{} {}", c.method, c.path))
        .collect();
    assert_eq!(
        paths,
        [
            "POST /repos/acme/web/pulls",
            "POST /repos/acme/web/pulls/12/requested_reviewers",
            "POST /repos/acme/web/issues/12/labels",
        ]
    );
    assert_eq!(calls[0].body["title"], json!("Dark mode"));
    assert_eq!(calls[0].body["head"], json!("feature/x"));
    assert_eq!(calls[0].body["base"], json!("main"));
    assert_eq!(calls[0].body["draft"], json!(true));
    assert_eq!(calls[1].body["reviewers"], json!(["alice"]));
    assert_eq!(calls[2].body["labels"], json!(["ui"]));
}

#[tokio::test]
async fn a_reviewer_who_cannot_be_requested_is_a_note_not_a_failure() {
    let stub = Stub::start(vec![
        Canned::json("POST", "/repos/acme/web/pulls", 201, pull(3, "open", None)),
        Canned::json(
            "POST",
            "/repos/acme/web/pulls/3/requested_reviewers",
            422,
            json!({"message": "Reviews may only be requested from collaborators"}),
        ),
    ])
    .await;
    let (gh, _dir) = host(&stub).await;
    let pr = gh
        .create_pr(
            &repo(),
            PrCreate {
                head: "h".into(),
                base: "b".into(),
                reviewers: vec!["stranger".into()],
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        pr.number, 3,
        "the pull request exists whatever the reviewer said"
    );
}

#[tokio::test]
async fn a_closed_pull_request_with_a_merged_at_is_merged() {
    let stub = Stub::start(vec![Canned::json(
        "GET",
        "/repos/acme/web/pulls/5",
        200,
        pull(5, "closed", Some("2026-01-01T00:00:00Z")),
    )])
    .await;
    let (gh, _dir) = host(&stub).await;
    let pr = gh.get_pr(&repo(), 5).await.unwrap();
    assert_eq!(pr.state, PrState::Merged);
    assert_eq!(pr.head_sha, "a".repeat(40));
}

#[tokio::test]
async fn listing_merged_pull_requests_asks_for_closed_ones_and_keeps_only_the_merged() {
    let both = json!([
        pull(1, "closed", Some("2026-01-01T00:00:00Z")),
        pull(2, "closed", None)
    ]);
    let stub = Stub::start(vec![
        Canned::json("GET", "/repos/acme/web/pulls", 200, both.clone()),
        Canned::json("GET", "/repos/acme/web/pulls", 200, both),
    ])
    .await;
    let (gh, _dir) = host(&stub).await;
    let merged = gh
        .list_prs(
            &repo(),
            PrFilter {
                state: Some(PrState::Merged),
                head: Some("feature/x".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(merged.iter().map(|p| p.number).collect::<Vec<_>>(), [1]);
    let closed = gh
        .list_prs(
            &repo(),
            PrFilter {
                state: Some(PrState::Closed),
                head: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(closed.iter().map(|p| p.number).collect::<Vec<_>>(), [2]);
    let calls = stub.calls();
    let q = calls[0].query.clone().unwrap_or_default();
    assert!(q.contains("state=closed"), "{q}");
    assert!(
        q.contains("head=acme%3Afeature%2Fx") || q.contains("head=acme:feature/x"),
        "{q}"
    );
}

#[tokio::test]
async fn checks_are_read_by_the_head_commit_in_two_round_trips() {
    let stub = Stub::start(vec![
        Canned::json("GET", "/repos/acme/web/pulls/9", 200, pull(9, "open", None)),
        Canned::json(
            "GET",
            &format!("/repos/acme/web/commits/{}/check-runs", "a".repeat(40)),
            200,
            json!({"check_runs": [
                {"name": "build", "status": "completed", "conclusion": "success", "html_url": "https://ci/1", "output": {"title": "ok", "summary": null}},
                {"name": "lint", "status": "in_progress", "conclusion": null, "html_url": null, "output": null},
            ]}),
        ),
    ])
    .await;
    let (gh, _dir) = host(&stub).await;
    let runs = gh.checks(&repo(), 9).await.unwrap();
    assert_eq!(runs.len(), 2);
    assert_eq!(
        runs[0].summary.as_deref(),
        Some("ok"),
        "the title stands in for a missing summary"
    );
    assert_eq!(runs[1].conclusion, None);
    let paths: Vec<_> = stub.calls().iter().map(|c| c.path.clone()).collect();
    assert_eq!(paths.len(), 2);
    assert!(paths[1].ends_with("/check-runs"));
}

#[tokio::test]
async fn a_review_posts_its_verdict_and_inline_comments_with_their_side_and_span() {
    let stub = Stub::start(vec![
        Canned::json(
            "POST",
            "/repos/acme/web/pulls/4/reviews",
            200,
            json!({"id": 1}),
        ),
        Canned::json(
            "POST",
            "/repos/acme/web/pulls/4/reviews",
            200,
            json!({"id": 2}),
        ),
    ])
    .await;
    let (gh, _dir) = host(&stub).await;
    gh.submit_review(
        &repo(),
        4,
        Review {
            event: ReviewEvent::RequestChanges,
            body: Some("one thing".into()),
            comments: vec![
                ReviewComment {
                    path: "src/a.rs".into(),
                    line: 3,
                    side: Side::Right,
                    start_line: None,
                    start_side: None,
                    body: "rename".into(),
                },
                ReviewComment {
                    path: "src/b.rs".into(),
                    line: 12,
                    side: Side::Left,
                    start_line: Some(10),
                    start_side: None,
                    body: "this deletion loses the guard".into(),
                },
            ],
        },
    )
    .await
    .unwrap();
    let body = &stub.calls()[0].body;
    assert_eq!(body["event"], json!("REQUEST_CHANGES"));
    assert_eq!(body["body"], json!("one thing"));
    assert_eq!(body["comments"][0]["path"], json!("src/a.rs"));
    assert_eq!(body["comments"][0]["line"], json!(3));
    assert_eq!(body["comments"][0]["side"], json!("RIGHT"));
    assert!(
        body["comments"][0].get("start_line").is_none(),
        "a one-line comment has no span"
    );
    assert_eq!(body["comments"][1]["side"], json!("LEFT"));
    assert_eq!(body["comments"][1]["start_line"], json!(10));
    assert_eq!(
        body["comments"][1]["start_side"],
        json!("LEFT"),
        "a span's start side follows its end when unsaid"
    );

    // An approval with nothing to add sends no `body` key at all — never `""`.
    gh.submit_review(
        &repo(),
        4,
        Review {
            event: ReviewEvent::Approve,
            body: Some("   ".into()),
            comments: vec![],
        },
    )
    .await
    .unwrap();
    let approve = &stub.calls()[1].body;
    assert_eq!(approve["event"], json!("APPROVE"));
    assert!(approve.get("body").is_none(), "{approve}");
}

#[tokio::test]
async fn a_wordless_comment_or_change_request_is_refused_before_any_request() {
    let stub = Stub::start(vec![]).await;
    let (gh, _dir) = host(&stub).await;
    for event in [ReviewEvent::Comment, ReviewEvent::RequestChanges] {
        let e = gh
            .submit_review(
                &repo(),
                4,
                Review {
                    event,
                    body: None,
                    comments: vec![],
                },
            )
            .await
            .unwrap_err();
        assert!(
            matches!(e, CodeHostError::Refused(_)) && e.to_string().contains("needs words"),
            "{e}"
        );
    }
    assert!(stub.calls().is_empty(), "nothing left for the code host");
}

#[tokio::test]
async fn a_refusal_carries_githubs_own_reason_not_the_statuss_name() {
    // The reviews endpoint answers 422 with `message: Unprocessable Entity` and
    // the actual violation in `errors` — the sentence a person has to read.
    let stub = Stub::start(vec![
        Canned::json(
            "POST",
            "/repos/acme/web/pulls/4/reviews",
            422,
            json!({"message": "Unprocessable Entity", "errors": ["Can not approve your own pull request"], "documentation_url": "https://docs.github.test"}),
        ),
        Canned::json(
            "POST",
            "/repos/acme/web/pulls/5/reviews",
            422,
            json!({"message": "Validation Failed", "errors": [{"resource": "PullRequestReview", "field": "line", "code": "invalid"}]}),
        ),
    ])
    .await;
    let (gh, _dir) = host(&stub).await;
    let own = gh
        .submit_review(
            &repo(),
            4,
            Review {
                event: ReviewEvent::Approve,
                body: None,
                comments: vec![],
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(own, CodeHostError::Refused(_)), "{own}");
    assert_eq!(
        own.to_string(),
        "the code host refused: submit review: Can not approve your own pull request"
    );
    let field = gh
        .submit_review(
            &repo(),
            5,
            Review {
                event: ReviewEvent::Comment,
                body: Some("see line".into()),
                comments: vec![ReviewComment {
                    path: "src/a.rs".into(),
                    line: 999,
                    side: Side::Right,
                    start_line: None,
                    start_side: None,
                    body: "here".into(),
                }],
            },
        )
        .await
        .unwrap_err();
    assert!(
        field
            .to_string()
            .ends_with("submit review: line is invalid"),
        "{field}"
    );
}

#[tokio::test]
async fn reviews_and_threads_are_one_graphql_query() {
    let data = json!({"data": {"repository": {"pullRequest": {
        "reviews": {"nodes": [{"author": {"login": "alice"}, "state": "APPROVED", "body": "ship it", "submittedAt": "2026-01-01T00:00:00Z"}]},
        "reviewThreads": {"nodes": [{
            "id": "PRRT_1", "isResolved": false, "isOutdated": false, "path": "src/a.rs", "line": 3,
            "comments": {"nodes": [{"author": {"login": "alice"}, "body": "rename", "createdAt": null}]}
        }]}
    }}}});
    let stub = Stub::start(vec![Canned::json("POST", "/graphql", 200, data)]).await;
    let (gh, _dir) = host(&stub).await;
    let reviews = gh.pr_reviews(&repo(), 4).await.unwrap();
    assert_eq!(
        reviews.reviews[0].state, "approved",
        "GitHub's shouting is lowered"
    );
    assert_eq!(reviews.threads[0].id, "PRRT_1");
    assert_eq!(reviews.threads[0].comments[0].body, "rename");
    let call = &stub.calls()[0];
    assert!(call.body["query"]
        .as_str()
        .unwrap()
        .contains("reviewThreads"));
    assert_eq!(call.body["variables"]["number"], json!(4));
}

#[tokio::test]
async fn resolving_a_thread_is_a_graphql_mutation_and_its_errors_are_refusals() {
    let stub = Stub::start(vec![
        Canned::json(
            "POST",
            "/graphql",
            200,
            json!({"data": {"resolveReviewThread": {"thread": {"id": "PRRT_1"}}}}),
        ),
        Canned::json(
            "POST",
            "/graphql",
            200,
            json!({"data": null, "errors": [{"message": "Could not resolve to a node"}]}),
        ),
    ])
    .await;
    let (gh, _dir) = host(&stub).await;
    gh.resolve_review_thread("PRRT_1", true).await.unwrap();
    let err = gh
        .resolve_review_thread("PRRT_nope", false)
        .await
        .unwrap_err();
    assert!(matches!(err, CodeHostError::Refused(_)), "{err}");
    let calls = stub.calls();
    assert!(calls[0].body["query"]
        .as_str()
        .unwrap()
        .contains("resolveReviewThread("));
    assert!(calls[1].body["query"]
        .as_str()
        .unwrap()
        .contains("unresolveReviewThread("));
}

#[tokio::test]
async fn replying_on_a_thread_is_one_graphql_mutation_carrying_the_words_and_its_errors_are_refusals(
) {
    let stub = Stub::start(vec![
        Canned::json(
            "POST",
            "/graphql",
            200,
            json!({"data": {"addPullRequestReviewThreadReply": {"comment": {"id": "PRRC_9"}}}}),
        ),
        Canned::json(
            "POST",
            "/graphql",
            200,
            json!({"data": null, "errors": [{"message": "Could not resolve to a node"}]}),
        ),
    ])
    .await;
    let (gh, _dir) = host(&stub).await;
    gh.reply_review_thread("PRRT_1", "Renamed in abc123.")
        .await
        .unwrap();
    let err = gh.reply_review_thread("PRRT_nope", "x").await.unwrap_err();
    assert!(matches!(err, CodeHostError::Refused(_)), "{err}");
    let calls = stub.calls();
    assert!(calls[0].body["query"]
        .as_str()
        .unwrap()
        .contains("addPullRequestReviewThreadReply("));
    assert_eq!(calls[0].body["variables"]["id"], json!("PRRT_1"));
    assert_eq!(
        calls[0].body["variables"]["body"],
        json!("Renamed in abc123.")
    );
}

#[tokio::test]
async fn a_merge_names_its_strategy_and_a_branch_delete_accepts_an_empty_204() {
    let stub = Stub::start(vec![
        Canned::json("PUT", "/repos/acme/web/pulls/4/merge", 200, json!({"merged": true, "sha": "c".repeat(40), "message": "Pull Request successfully merged"})),
        Canned::empty("DELETE", "/repos/acme/web/git/refs/heads/feature/x", 204),
        Canned::json("DELETE", "/repos/acme/web/git/refs/heads/gone", 422, json!({"message": "Reference does not exist"})),
    ])
    .await;
    let (gh, _dir) = host(&stub).await;
    let out = gh.merge(&repo(), 4, MergeStrategy::Squash).await.unwrap();
    assert!(out.merged);
    assert_eq!(out.sha.as_deref(), Some("c".repeat(40).as_str()));
    assert_eq!(stub.calls()[0].body["merge_method"], json!("squash"));
    gh.delete_branch(&repo(), "feature/x").await.unwrap();
    let err = gh.delete_branch(&repo(), "gone").await.unwrap_err();
    assert!(matches!(err, CodeHostError::Refused(_)), "{err}");
}

#[tokio::test]
async fn the_account_is_the_login_the_scopes_header_and_the_organizations_it_can_see() {
    let stub = Stub::start(vec![
        Canned::json("GET", "/user", 200, json!({"login": "octocat"}))
            .header("x-oauth-scopes", "repo, workflow, read:org"),
        Canned::json(
            "GET",
            "/user/orgs",
            200,
            json!([{"login": "acme"}, {"login": "widgets"}]),
        ),
        Canned::json("GET", "/user", 200, json!({"login": "octocat"}))
            .header("x-oauth-scopes", "repo, workflow"),
        Canned::json(
            "GET",
            "/user/orgs",
            403,
            json!({"message": "Resource not accessible"}),
        ),
    ])
    .await;
    let (gh, _dir) = host(&stub).await;
    let me = gh.account().await.unwrap();
    assert_eq!(me.login, "octocat");
    assert_eq!(me.scopes, ["repo", "workflow", "read:org"]);
    assert_eq!(me.organizations, ["acme", "widgets"]);
    let narrow = gh.account().await.unwrap();
    assert!(
        narrow.organizations.is_empty(),
        "a token without read:org sees no organizations, and that is not an error"
    );
    let paths: Vec<_> = stub.calls().iter().map(|c| c.path.clone()).collect();
    assert_eq!(paths, ["/user", "/user/orgs", "/user", "/user/orgs"]);
}

#[tokio::test]
async fn a_bound_account_sends_its_own_token_and_two_stored_with_none_named_is_refused_by_name() {
    if !no_env_token() {
        return;
    }
    let stub = Stub::start(vec![
        Canned::json("GET", "/repos/acme/web/pulls/1", 200, pull(1, "open", None)),
        Canned::json("GET", "/repos/acme/web/pulls/2", 200, pull(2, "open", None)),
        Canned::json("GET", "/repos/acme/web/pulls/3", 200, pull(3, "open", None)),
    ])
    .await;
    let dir = tempfile::tempdir().unwrap();
    let tokens = store(&dir);
    tokens.store("ada", "ghp_ada").unwrap();
    let gh: Arc<GitHubApi> = Arc::new(GitHubApi::with_base_url(stub.base_url(), tokens.clone()));
    gh.get_pr(&repo(), 1).await.unwrap();
    assert_eq!(
        stub.calls()[0].authorization.as_deref(),
        Some("Bearer ghp_ada"),
        "one stored account is the one they meant"
    );

    tokens.store("Bob", "ghp_bob").unwrap();
    let ambiguous = gh.get_pr(&repo(), 2).await.unwrap_err();
    assert!(
        matches!(ambiguous, CodeHostError::NotAuthenticated(_)),
        "{ambiguous}"
    );
    assert!(
        ambiguous.to_string().contains("@ada") && ambiguous.to_string().contains("@bob"),
        "{ambiguous}"
    );
    assert_eq!(
        stub.calls().len(),
        1,
        "nothing was sent without knowing which account"
    );

    let as_bob = Arc::clone(&gh).for_account(Some("BOB"));
    as_bob.get_pr(&repo(), 2).await.unwrap();
    assert_eq!(
        stub.calls()[1].authorization.as_deref(),
        Some("Bearer ghp_bob")
    );
    let as_ada = Arc::clone(&gh).for_account(Some("ada"));
    as_ada.get_pr(&repo(), 3).await.unwrap();
    assert_eq!(
        stub.calls()[2].authorization.as_deref(),
        Some("Bearer ghp_ada")
    );
    let unknown = Arc::clone(&gh)
        .for_account(Some("carol"))
        .get_pr(&repo(), 3)
        .await
        .unwrap_err();
    assert!(unknown.to_string().contains("@carol"), "{unknown}");
    assert_eq!(stub.calls().len(), 3);
}

#[tokio::test]
async fn repository_access_is_read_never_tried_and_a_404_is_not_found() {
    let stub = Stub::start(vec![
        Canned::json("GET", "/repos/acme/web", 200, json!({"full_name": "acme/web", "permissions": {"admin": false, "push": true, "pull": true}})),
        Canned::json("GET", "/repos/acme/web", 200, json!({"full_name": "acme/web", "permissions": {"push": false, "pull": true}})),
        Canned::json("GET", "/repos/acme/web", 404, json!({"message": "Not Found"})),
    ])
    .await;
    let (gh, _dir) = host(&stub).await;
    assert_eq!(
        gh.repo_access(&repo()).await.unwrap(),
        bisa_codehost::RepoAccess {
            found: true,
            push: true
        }
    );
    assert_eq!(
        gh.repo_access(&repo()).await.unwrap(),
        bisa_codehost::RepoAccess {
            found: true,
            push: false
        }
    );
    assert_eq!(
        gh.repo_access(&repo()).await.unwrap(),
        bisa_codehost::RepoAccess {
            found: false,
            push: false
        }
    );
    assert!(
        stub.calls().iter().all(|c| c.method == "GET"),
        "read, never tried"
    );
}

#[test]
fn github_detects_its_own_host_in_every_spelling_and_no_other() {
    let dir = tempfile::tempdir().unwrap();
    let gh = GitHubApi::with_base_url("http://127.0.0.1:1", store(&dir));
    for url in [
        "https://github.com/acme/web.git",
        "git@github.com:acme/web.git",
        "ssh://git@GitHub.com/acme/web",
    ] {
        assert_eq!(
            gh.detect(&RemoteUrl::parse(url)).unwrap().slug(),
            "acme/web",
            "{url}"
        );
    }
    assert!(
        gh.detect(&RemoteUrl::parse("git@github-work:acme/web.git"))
            .is_none(),
        "an alias is not the host until ssh says so"
    );
    assert_eq!(
        gh.detect(&RemoteUrl::parse("git@github-work:acme/web.git").with_host("github.com"))
            .unwrap()
            .slug(),
        "acme/web"
    );
    assert!(gh
        .detect(&RemoteUrl::parse("https://gitlab.com/acme/web.git"))
        .is_none());
    assert!(gh.detect(&RemoteUrl::parse("/tmp/origin.git")).is_none());
}

#[tokio::test]
async fn verifying_a_token_sends_that_token_and_refuses_an_empty_one() {
    let stub = Stub::start(vec![Canned::json(
        "GET",
        "/user",
        200,
        json!({"login": "octocat"}),
    )])
    .await;
    let (gh, _dir) = host(&stub).await;
    assert!(matches!(
        gh.verify_token("  ", None).await,
        Err(CodeHostError::NotAuthenticated(_))
    ));
    assert!(
        stub.calls().is_empty(),
        "an empty token is refused before any request"
    );
    let me = gh.verify_token("ghp_candidate", None).await.unwrap();
    assert_eq!(me.login, "octocat");
    assert!(
        me.scopes.is_empty(),
        "a fine-grained token carries no scope header"
    );
    assert_eq!(
        stub.calls()[0].authorization.as_deref(),
        Some("Bearer ghp_candidate"),
        "the candidate, not the stored token"
    );
}

#[tokio::test]
async fn statuses_map_to_typed_errors_and_a_rate_limit_is_a_wait() {
    let stub = Stub::start(vec![
        Canned::json("GET", "/user", 401, json!({"message": "Bad credentials"})),
        Canned::json(
            "GET",
            "/repos/acme/web/pulls/404",
            404,
            json!({"message": "Not Found"}),
        ),
        Canned::json(
            "PUT",
            "/repos/acme/web/pulls/1/merge",
            405,
            json!({"message": "Pull Request is not mergeable"}),
        ),
        Canned::json(
            "GET",
            "/repos/acme/web/pulls/2",
            403,
            json!({"message": "API rate limit exceeded"}),
        )
        .header("x-ratelimit-remaining", "0")
        .header("retry-after", "30"),
        Canned::json(
            "GET",
            "/repos/acme/web/pulls/3",
            403,
            json!({"message": "Resource not accessible"}),
        ),
        Canned::json(
            "GET",
            "/repos/acme/web/pulls/500",
            500,
            json!({"message": "boom"}),
        ),
    ])
    .await;
    let (gh, _dir) = host(&stub).await;
    let r = repo();
    let e = gh.account().await.unwrap_err();
    assert!(
        matches!(e, CodeHostError::NotAuthenticated(_))
            && e.to_string().contains("Bad credentials"),
        "{e}"
    );
    assert!(matches!(
        gh.get_pr(&r, 404).await,
        Err(CodeHostError::NotFound(_))
    ));
    assert!(matches!(
        gh.merge(&r, 1, MergeStrategy::Merge).await,
        Err(CodeHostError::Refused(_))
    ));
    let limited = gh.get_pr(&r, 2).await.unwrap_err();
    assert!(
        matches!(limited, CodeHostError::Transport(_)),
        "a rate limit is retried, never mistaken for a bad token: {limited}"
    );
    assert!(
        limited.to_string().contains("rate limit") && limited.to_string().contains("in 30s"),
        "{limited}"
    );
    assert!(
        matches!(
            gh.get_pr(&r, 3).await,
            Err(CodeHostError::NotAuthenticated(_))
        ),
        "a plain 403 is a permission refusal"
    );
    assert!(matches!(
        gh.get_pr(&r, 500).await,
        Err(CodeHostError::Transport(_))
    ));
    for c in stub.calls() {
        assert!(!c.path.contains("ghp_"), "no token in a path: {}", c.path);
    }
}

/// The third source: what git's own credential helper holds. Faked here —
/// a `GitCredentials` that answers a fixed token — so no helper, keychain or
/// remote is touched; the stub sees the bearer git handed us.
#[derive(Debug)]
struct FakeGitCredentials(Option<&'static str>);

#[async_trait::async_trait]
impl bisa_codehost::creds::GitCredentials for FakeGitCredentials {
    async fn fill(&self, host: &str) -> Option<bisa_codehost::creds::Credential> {
        assert_eq!(host, HOST, "asked for the store's host");
        self.0.map(|t| bisa_codehost::creds::Credential {
            username: "octocat".to_string(),
            password: bisa_codehost::creds::Secret::new(t),
        })
    }
    async fn helpers(&self) -> Vec<String> {
        vec!["fake".to_string()]
    }
}

#[tokio::test]
async fn gits_credential_helper_is_the_third_source_and_a_stored_token_outranks_it() {
    if !no_env_token() {
        // The environment is the operator's override; on such a machine this
        // ordering cannot be observed, and the chain's unit test covers it.
        return;
    }
    let stub = Stub::start(vec![
        Canned::json("GET", "/repos/acme/web/pulls/1", 200, pull(1, "open", None)),
        Canned::json("GET", "/repos/acme/web/pulls/2", 200, pull(2, "open", None)),
    ])
    .await;
    let dir = tempfile::tempdir().unwrap();
    let from_git = std::sync::Arc::new(FakeGitCredentials(Some("ghp_from_git")));
    let gh = GitHubApi::with_base_url(stub.base_url(), store(&dir).with_git(from_git.clone()));
    gh.get_pr(&repo(), 1).await.unwrap();
    assert_eq!(
        stub.calls()[0].authorization.as_deref(),
        Some("Bearer ghp_from_git"),
        "nothing stored: git's credential is used"
    );

    let tokens = store(&dir).with_git(from_git);
    tokens.store("octocat", "ghp_stored").unwrap();
    let gh = GitHubApi::with_base_url(stub.base_url(), tokens);
    gh.get_pr(&repo(), 2).await.unwrap();
    assert_eq!(
        stub.calls()[1].authorization.as_deref(),
        Some("Bearer ghp_stored"),
        "a token the person stored is the one they meant"
    );
}

#[tokio::test]
async fn with_no_source_at_all_the_refusal_names_the_ways_in_and_sends_nothing() {
    if !no_env_token() {
        return;
    }
    let stub = Stub::start(vec![]).await;
    let dir = tempfile::tempdir().unwrap();
    let gh = GitHubApi::with_base_url(
        stub.base_url(),
        store(&dir).with_git(std::sync::Arc::new(FakeGitCredentials(None))),
    );
    let err = gh.get_pr(&repo(), 1).await.unwrap_err();
    assert!(matches!(err, CodeHostError::NotAuthenticated(_)), "{err}");
    let words = err.to_string();
    assert!(
        words.contains("BISA_GITHUB_TOKEN") && words.contains("credential helper"),
        "{words}"
    );
    assert!(stub.calls().is_empty(), "no request without a token");
}
