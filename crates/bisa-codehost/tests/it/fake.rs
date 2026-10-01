//! The trait's contract, driven through the in-memory code host: a pull request
//! is created, read, listed, reviewed and merged, and every capability the
//! fake lacks is a typed `Unsupported`, never a silent no-op.

use bisa_codehost::fake::FakeCodeHost;
use bisa_codehost::{
    CodeHost, CodeHostCapabilities, CodeHostError, CodeHostRegistry, MergeStrategy, PrCreate,
    PrFilter, PrState, RemoteUrl, RepoAccess, Review, ReviewComment, ReviewEvent, ReviewThread,
    ReviewThreadComment,
};
use std::sync::Arc;

fn full() -> CodeHostCapabilities {
    CodeHostCapabilities {
        draft_prs: true,
        reviewers: true,
        labels: true,
        merge_strategies: vec![MergeStrategy::Squash],
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

#[tokio::test]
async fn a_reply_joins_the_thread_as_the_fakes_login_and_a_host_without_the_capability_refuses_it()
{
    let host = FakeCodeHost::with_capabilities("codehost.test", full()).signed_in_as("fixer");
    host.state.threads.lock().unwrap().push(ReviewThread {
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
    host.reply_review_thread("T1", "Renamed in abc123.")
        .await
        .unwrap();
    let threads = host.state.threads.lock().unwrap().clone();
    assert_eq!(threads[0].comments.len(), 2, "the reply joins the thread");
    assert_eq!(
        (
            threads[0].comments[1].author.as_deref(),
            threads[0].comments[1].body.as_str()
        ),
        (Some("fixer"), "Renamed in abc123.")
    );
    assert!(
        !threads[0].is_resolved,
        "a reply alone resolves nothing — that is the resolve verb's"
    );
    assert!(matches!(
        host.reply_review_thread("nope", "x").await,
        Err(CodeHostError::NotFound(_))
    ));
    assert_eq!(
        host.asked_as(),
        ["fixer"],
        "a reply is asked as the bound account"
    );
    let minimal = FakeCodeHost::minimal("codehost.test");
    assert!(matches!(
        minimal.reply_review_thread("T1", "x").await,
        Err(CodeHostError::Unsupported(_))
    ));
    assert!(!minimal.capabilities().review_thread_replies);
}

#[tokio::test]
async fn a_pull_request_round_trips_through_the_trait() {
    let host = FakeCodeHost::with_capabilities("codehost.test", full());
    let repo = host
        .detect(&RemoteUrl::parse("git@codehost.test:acme/web.git"))
        .expect("detected");
    assert_eq!(repo.slug(), "acme/web");
    let pr = host
        .create_pr(
            &repo,
            PrCreate {
                title: "Dark mode".into(),
                body: "as discussed".into(),
                head: "feature/dark-mode".into(),
                base: "main".into(),
                draft: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(pr.number, 1);
    assert!(pr.is_draft);
    assert_eq!(host.get_pr(&repo, 1).await.unwrap().title, "Dark mode");
    assert!(matches!(
        host.get_pr(&repo, 9).await,
        Err(CodeHostError::NotFound(_))
    ));
    let open = host
        .list_prs(
            &repo,
            PrFilter {
                state: Some(PrState::Open),
                head: Some("feature/dark-mode".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(open.len(), 1);
    // The fake's own account opened #1, so it may comment on it and nothing
    // more — the rule every code host holds, held here so the engine sees it.
    let own = host
        .submit_review(
            &repo,
            1,
            Review {
                event: ReviewEvent::RequestChanges,
                body: Some("one thing".into()),
                comments: vec![ReviewComment {
                    path: "a.rs".into(),
                    line: 3,
                    side: Default::default(),
                    start_line: None,
                    start_side: None,
                    body: "rename".into(),
                }],
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(own, CodeHostError::Refused(_))
            && own.to_string().contains("your own pull request"),
        "{own}"
    );
    assert!(
        host.state.reviews.lock().unwrap().is_empty(),
        "a refused review is not recorded"
    );
    host.submit_review(
        &repo,
        1,
        Review {
            event: ReviewEvent::Comment,
            body: Some("one thing".into()),
            comments: vec![],
        },
    )
    .await
    .unwrap();
    let wordless = host
        .submit_review(
            &repo,
            1,
            Review {
                event: ReviewEvent::Comment,
                body: None,
                comments: vec![],
            },
        )
        .await
        .unwrap_err();
    assert!(
        wordless.to_string().contains("body is required"),
        "{wordless}"
    );
    // Somebody else's pull request takes the verdict.
    let theirs = host.seed_pr(&repo, "alice", "feature/theirs", "main");
    host.submit_review(
        &repo,
        theirs.number,
        Review {
            event: ReviewEvent::Approve,
            body: None,
            comments: vec![],
        },
    )
    .await
    .unwrap();
    assert_eq!(host.state.reviews.lock().unwrap().len(), 2);
    assert!(matches!(
        host.merge(&repo, 1, MergeStrategy::Rebase).await,
        Err(CodeHostError::Unsupported(_))
    ));
    let out = host.merge(&repo, 1, MergeStrategy::Squash).await.unwrap();
    assert!(out.merged);
    assert_eq!(host.get_pr(&repo, 1).await.unwrap().state, PrState::Merged);
    assert!(matches!(
        host.merge(&repo, 1, MergeStrategy::Squash).await,
        Err(CodeHostError::Refused(_))
    ));
}

#[tokio::test]
async fn the_minimal_code_host_refuses_what_it_cannot_do_by_type() {
    let host = FakeCodeHost::minimal("codehost.test");
    let repo = host
        .detect(&RemoteUrl::parse("https://codehost.test/acme/web"))
        .unwrap();
    assert!(matches!(
        host.create_pr(
            &repo,
            PrCreate {
                draft: true,
                head: "h".into(),
                base: "b".into(),
                ..Default::default()
            }
        )
        .await,
        Err(CodeHostError::Unsupported(_))
    ));
    assert!(matches!(
        host.checks(&repo, 1).await,
        Err(CodeHostError::Unsupported(_))
    ));
    let caps = host.capabilities();
    assert!(
        !caps.draft_prs
            && !caps.reviewers
            && !caps.labels
            && !caps.check_runs
            && !caps.review_comments
    );
    assert!(caps.merge_strategies.is_empty());
}

#[test]
fn the_registry_picks_a_code_host_by_remote() {
    let reg = CodeHostRegistry::new(vec![
        Arc::new(FakeCodeHost::minimal("a.test")),
        Arc::new(FakeCodeHost::minimal("b.test")),
    ]);
    let (f, repo) = reg
        .detect(&RemoteUrl::parse("git@b.test:o/r.git"))
        .expect("b");
    assert_eq!(f.id().0, "fake");
    assert_eq!(repo.host, "b.test");
    assert!(reg
        .detect(&RemoteUrl::parse("git@c.test:o/r.git"))
        .is_none());
    // A bare repository on disk whose name is the host — the engine tests' origin.
    let (_, local) = reg
        .detect(&RemoteUrl::parse("/tmp/fixtures/a.test"))
        .expect("a local bare origin named for the host");
    assert_eq!(
        (
            local.host.as_str(),
            local.owner.as_str(),
            local.name.as_str()
        ),
        ("a.test", "local", "a.test")
    );
    assert!(reg
        .detect(&RemoteUrl::parse("/tmp/fixtures/elsewhere.git"))
        .is_none());
    let dir = tempfile::tempdir().unwrap();
    let built = std::sync::Arc::new(bisa_codehost::hosts::Hosts::file_only(dir.path())).registry();
    assert_eq!(
        built.ids().iter().map(|i| i.0).collect::<Vec<_>>(),
        ["github", "gitlab", "bitbucket"]
    );
    let of_kind =
        bisa_codehost::fake::FakeCodeHost::of_kind(bisa_codehost::CodeHostKind::GitLab, "gl.test");
    assert_eq!(of_kind.id().0, "gitlab", "a fake may stand in for a kind");
}

#[tokio::test]
async fn a_fake_rebound_to_another_account_shares_its_state_and_says_who_asked() {
    let host: Arc<FakeCodeHost> = Arc::new(
        FakeCodeHost::with_capabilities("codehost.test", full())
            .signed_in_as("ada")
            .with_organizations("ada", &["acme"])
            .with_organizations("bob", &[]),
    );
    let repo = host
        .detect(&RemoteUrl::parse("https://codehost.test/acme/web"))
        .unwrap();
    let pr = host
        .create_pr(
            &repo,
            PrCreate {
                head: "h".into(),
                base: "b".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let as_bob = Arc::clone(&host).for_account(Some("Bob"));
    assert_eq!(
        as_bob
            .get_pr(&repo, pr.number)
            .await
            .unwrap()
            .author
            .as_deref(),
        Some("ada"),
        "the same pull requests"
    );
    assert_eq!(as_bob.account().await.unwrap().login, "bob");
    assert_eq!(host.account().await.unwrap().organizations, ["acme"]);
    assert!(as_bob.account().await.unwrap().organizations.is_empty());
    // Bob is not the author, so his approval is taken; Ada's own would be refused.
    as_bob
        .submit_review(
            &repo,
            pr.number,
            Review {
                event: ReviewEvent::Approve,
                body: None,
                comments: vec![],
            },
        )
        .await
        .unwrap();
    assert_eq!(
        host.state.reviews.lock().unwrap().len(),
        1,
        "the rebound fake wrote into the shared state"
    );
    assert_eq!(host.asked_as(), ["ada", "bob", "bob", "ada", "bob", "bob"]);
    let same = Arc::clone(&host).for_account(Some("ADA"));
    assert_eq!(
        same.account().await.unwrap().login,
        "ada",
        "the same login is the same fake"
    );
    *host.state.access.lock().unwrap() = Some(RepoAccess {
        found: true,
        push: false,
    });
    assert_eq!(
        as_bob.repo_access(&repo).await.unwrap(),
        RepoAccess {
            found: true,
            push: false
        }
    );
}
