//! Bitbucket Cloud's API implementation against the loopback stub: the Basic
//! credential it carries, the requests it makes, what it reads back. Nothing
//! here reaches the network.

#[allow(dead_code)]
use crate::support;

use bisa_codehost::bitbucket::{BitbucketApi, HOST};
use bisa_codehost::creds::TokenStore;
use bisa_codehost::{
    CodeHost, CodeHostError, CodeHostKind, MergeStrategy, PrCreate, PrFilter, PrState, RepoRef,
    Review, ReviewComment, ReviewEvent,
};
use serde_json::json;
use support::{Canned, Stub};

fn repo() -> RepoRef {
    RepoRef {
        host: HOST.into(),
        owner: "acme".into(),
        name: "web".into(),
    }
}

const PRS: &str = "/repositories/acme/web/pullrequests";

async fn host(stub: &Stub) -> (BitbucketApi, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let tokens = TokenStore::file_only(CodeHostKind::Bitbucket, HOST, dir.path());
    tokens.store("ada", "app-pass").unwrap();
    (BitbucketApi::with_base_url(stub.base_url(), tokens), dir)
}

fn no_env_token() -> bool {
    std::env::var(CodeHostKind::Bitbucket.env_var()).map_or(true, |v| v.trim().is_empty())
}

fn pr(id: u64, state: &str) -> serde_json::Value {
    json!({
        "id": id, "title": "t", "state": state, "draft": false,
        "source": {"branch": {"name": "work/x"}, "commit": {"hash": "abc"}},
        "destination": {"branch": {"name": "main"}},
        "author": {"nickname": "ada"},
        "links": {"html": {"href": format!("https://bitbucket.org/acme/web/pull-requests/{id}")}},
        "participants": [{"user": {"nickname": "alice"}, "approved": true, "state": "approved", "participated_on": "2026-01-01T00:00:00Z"}],
        "merge_commit": if state == "MERGED" { json!({"hash": "deadbeef"}) } else { json!(null) }
    })
}

#[tokio::test]
async fn every_request_carries_the_stored_login_and_token_as_a_basic_credential() {
    let stub = Stub::start(vec![Canned::json(
        "GET",
        &format!("{PRS}/4"),
        200,
        pr(4, "OPEN"),
    )])
    .await;
    let (bb, _dir) = host(&stub).await;
    let got = bb.get_pr(&repo(), 4).await.unwrap();
    assert_eq!(
        (
            got.number,
            got.state,
            got.author.as_deref(),
            got.mergeable,
            got.head_sha.as_str()
        ),
        (4, PrState::Open, Some("ada"), Some(true), "abc")
    );
    let call = &stub.calls()[0];
    assert!(!call.bearer);
    if no_env_token() {
        // "ada:app-pass", base64.
        assert_eq!(
            call.authorization.as_deref(),
            Some("Basic YWRhOmFwcC1wYXNz")
        );
    }
}

#[tokio::test]
async fn a_token_is_checked_beside_the_login_it_belongs_to() {
    let stub = Stub::start(vec![
        Canned::json(
            "GET",
            "/user",
            200,
            json!({"nickname": "ada", "display_name": "Ada"}),
        ),
        Canned::json(
            "GET",
            "/user/permissions/workspaces",
            200,
            json!({"values": [{"workspace": {"slug": "acme"}}]}),
        ),
    ])
    .await;
    let (bb, _dir) = host(&stub).await;
    let me = bb.verify_token("candidate", Some("ada")).await.unwrap();
    assert_eq!(
        (me.login.as_str(), me.organizations),
        ("ada", vec!["acme".to_string()])
    );
    assert_eq!(
        stub.calls()[0].authorization.as_deref(),
        Some("Basic YWRhOmNhbmRpZGF0ZQ=="),
        "ada:candidate"
    );
    let err = bb.verify_token("candidate", None).await.unwrap_err();
    assert!(
        matches!(&err, CodeHostError::NotAuthenticated(m) if m.contains("login")),
        "no login, no check: {err}"
    );
    let stub = Stub::start(vec![Canned::json(
        "GET",
        "/user",
        200,
        json!({"nickname": "ada"}),
    )])
    .await;
    let (bb, _dir) = host(&stub).await;
    assert_eq!(
        bb.verify_token("ada:candidate", None).await.unwrap().login,
        "ada",
        "a login:token pair says who"
    );
}

#[tokio::test]
async fn creating_listing_checks_and_merging_speak_bitbuckets_words() {
    let stub = Stub::start(vec![
        Canned::json("POST", PRS, 201, pr(5, "OPEN")),
        Canned::json("GET", PRS, 200, json!({"values": [pr(4, "MERGED")]})),
        Canned::json("GET", &format!("{PRS}/4"), 200, pr(4, "OPEN")),
        Canned::json("GET", "/repositories/acme/web/commit/abc/statuses", 200, json!({"values": [{"key": "ci", "state": "SUCCESSFUL", "url": "u"}, {"name": "build", "state": "INPROGRESS"}]})),
        Canned::json("POST", &format!("{PRS}/4/merge"), 200, pr(4, "MERGED")),
        Canned::empty("DELETE", "/repositories/acme/web/refs/branches/work/x", 204),
        Canned::json("POST", &format!("{PRS}/6/merge"), 555, json!({"error": {"message": "Merge conflict"}})),
    ])
    .await;
    let (bb, _dir) = host(&stub).await;
    let created = bb
        .create_pr(
            &repo(),
            PrCreate {
                title: "Add".into(),
                body: "b".into(),
                head: "work/x".into(),
                base: "main".into(),
                draft: true,
                reviewers: vec![],
                labels: vec![],
            },
        )
        .await
        .unwrap();
    assert_eq!(created.number, 5);
    assert_eq!(stub.calls()[0].body["source"]["branch"]["name"], "work/x");
    assert_eq!(stub.calls()[0].body["draft"], true);
    let merged = bb
        .list_prs(
            &repo(),
            PrFilter {
                state: Some(PrState::Merged),
                head: Some("work/x".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(merged[0].state, PrState::Merged);
    assert_eq!(
        stub.calls()[1].query.as_deref(),
        Some("pagelen=50&state=MERGED&q=source.branch.name%3D%22work%2Fx%22")
    );
    let checks = bb.checks(&repo(), 4).await.unwrap();
    assert_eq!(
        (
            checks[0].name.as_str(),
            checks[0].conclusion.as_deref(),
            checks[1].status.as_str()
        ),
        ("ci", Some("success"), "in_progress")
    );
    let outcome = bb.merge(&repo(), 4, MergeStrategy::Squash).await.unwrap();
    assert_eq!(
        (outcome.merged, outcome.sha.as_deref()),
        (true, Some("deadbeef"))
    );
    assert_eq!(stub.calls()[4].body["merge_strategy"], "squash");
    bb.delete_branch(&repo(), "work/x").await.unwrap();
    let refused = bb
        .merge(&repo(), 6, MergeStrategy::Merge)
        .await
        .unwrap_err();
    assert!(
        matches!(&refused, CodeHostError::Refused(m) if m.contains("Merge conflict")),
        "{refused}"
    );
    assert!(matches!(
        bb.merge(&repo(), 6, MergeStrategy::Rebase).await,
        Err(CodeHostError::Unsupported(_))
    ));
}

#[tokio::test]
async fn a_review_approves_requests_changes_or_comments_and_threads_are_inline_comments_that_resolve(
) {
    let stub = Stub::start(vec![
        Canned::json("POST", &format!("{PRS}/4/approve"), 200, json!({})),
        Canned::json("POST", &format!("{PRS}/4/comments"), 201, json!({})),
        Canned::json("POST", &format!("{PRS}/4/comments"), 201, json!({})),
        Canned::json("POST", &format!("{PRS}/4/request-changes"), 200, json!({})),
        Canned::json("POST", &format!("{PRS}/4/comments"), 201, json!({})),
        Canned::json("GET", &format!("{PRS}/4"), 200, pr(4, "OPEN")),
        Canned::json("GET", &format!("{PRS}/4/comments"), 200, json!({"values": [
            {"id": 1, "content": {"raw": "rename"}, "user": {"nickname": "alice"}, "inline": {"path": "a.rs", "to": 12}, "resolution": null},
            {"id": 2, "content": {"raw": "done"}, "user": {"nickname": "ada"}, "parent": {"id": 1}},
            {"id": 3, "content": {"raw": "general"}, "user": {"nickname": "bob"}}
        ]})),
        Canned::json("POST", &format!("{PRS}/4/comments/1/resolve"), 200, json!({})),
        Canned::empty("DELETE", &format!("{PRS}/4/comments/1/resolve"), 204),
        Canned::json("POST", &format!("{PRS}/4/comments"), 201, json!({"id": 5})),
    ])
    .await;
    let (bb, _dir) = host(&stub).await;
    bb.submit_review(
        &repo(),
        4,
        Review {
            event: ReviewEvent::Approve,
            body: Some("ship".into()),
            comments: vec![ReviewComment {
                path: "a.rs".into(),
                line: 12,
                side: Default::default(),
                start_line: None,
                start_side: None,
                body: "nit".into(),
            }],
        },
    )
    .await
    .unwrap();
    bb.submit_review(
        &repo(),
        4,
        Review {
            event: ReviewEvent::RequestChanges,
            body: Some("no".into()),
            comments: vec![],
        },
    )
    .await
    .unwrap();
    let calls = stub.calls();
    assert_eq!(calls[1].body["content"]["raw"], "ship");
    assert_eq!(calls[2].body["inline"]["to"], 12);
    assert_eq!(calls[3].path, format!("{PRS}/4/request-changes"));
    let out = bb.pr_reviews(&repo(), 4).await.unwrap();
    assert_eq!(
        (
            out.reviews[0].author.as_deref(),
            out.reviews[0].state.as_str()
        ),
        (Some("alice"), "approved")
    );
    assert_eq!(out.threads.len(), 1);
    assert_eq!(
        (
            out.threads[0].id.as_str(),
            out.threads[0].comments.len(),
            out.threads[0].is_resolved
        ),
        ("acme/web#4#1", 2, false)
    );
    bb.resolve_review_thread("acme/web#4#1", true)
        .await
        .unwrap();
    bb.resolve_review_thread("acme/web#4#1", false)
        .await
        .unwrap();
    assert_eq!(stub.calls()[8].method, "DELETE");
    // A reply is a comment whose parent is the thread's root comment.
    bb.reply_review_thread("acme/web#4#1", "Renamed in abc123.")
        .await
        .unwrap();
    let reply = &stub.calls()[9];
    assert_eq!(
        (
            reply.body["parent"]["id"].as_u64(),
            reply.body["content"]["raw"].as_str()
        ),
        (Some(1), Some("Renamed in abc123."))
    );
    assert!(matches!(
        bb.reply_review_thread("acme/web#4#not", "x").await,
        Err(CodeHostError::NotFound(_))
    ));
    assert!(matches!(
        bb.submit_review(
            &repo(),
            4,
            Review {
                event: ReviewEvent::Comment,
                body: None,
                comments: vec![]
            }
        )
        .await,
        Err(CodeHostError::Refused(_))
    ));
}

#[tokio::test]
async fn repository_access_is_found_plus_the_permission_and_a_404_is_not_found() {
    let stub = Stub::start(vec![
        Canned::json(
            "GET",
            "/repositories/acme/web",
            200,
            json!({"full_name": "acme/web"}),
        ),
        Canned::json(
            "GET",
            "/user/permissions/repositories",
            200,
            json!({"values": [{"permission": "write"}]}),
        ),
        Canned::json(
            "GET",
            "/repositories/acme/web",
            404,
            json!({"error": {"message": "Repository acme/web not found"}}),
        ),
    ])
    .await;
    let (bb, _dir) = host(&stub).await;
    let access = bb.repo_access(&repo()).await.unwrap();
    assert!(access.found && access.push);
    let gone = bb.repo_access(&repo()).await.unwrap();
    assert!(!gone.found);
    assert!(bb
        .detect(&bisa_codehost::RemoteUrl::parse(
            "git@bitbucket.org:acme/web.git"
        ))
        .is_some());
}

#[tokio::test]
async fn statuses_map_to_typed_errors_a_rate_limit_is_a_wait_and_a_strange_answer_is_said() {
    let pr_at = |n: u64| format!("{PRS}/{n}");
    let said = |m: &str| json!({"type": "error", "error": {"message": m}});
    let stub = Stub::start(vec![
        Canned::json("GET", "/user", 401, said("Token is invalid or expired")),
        Canned::json("GET", &pr_at(404), 404, said("no such pull request")),
        Canned::json("GET", &pr_at(403), 403, said("forbidden")),
        Canned::json(
            "POST",
            &format!("{}/merge", pr_at(1)),
            400,
            said("has conflicts"),
        ),
        Canned::json(
            "POST",
            &format!("{}/merge", pr_at(2)),
            409,
            said("already merging"),
        ),
        Canned::json(
            "POST",
            &format!("{}/merge", pr_at(3)),
            555,
            said("merge timed out"),
        ),
        Canned::json("GET", &pr_at(429), 429, said("rate limited")),
        Canned::json("GET", &pr_at(500), 503, json!({})),
        Canned::json("GET", &pr_at(200), 200, json!(["not", "a", "pull request"])),
    ])
    .await;
    let (bb, _dir) = host(&stub).await;
    let r = repo();

    let e = bb.account().await.unwrap_err();
    assert!(
        matches!(e, CodeHostError::NotAuthenticated(_))
            && e.to_string().contains("invalid or expired"),
        "{e}"
    );
    assert!(matches!(
        bb.get_pr(&r, 404).await,
        Err(CodeHostError::NotFound(_))
    ));
    assert!(matches!(
        bb.get_pr(&r, 403).await,
        Err(CodeHostError::NotAuthenticated(_))
    ));
    for (n, why) in [
        (1, "has conflicts"),
        (2, "already merging"),
        (3, "merge timed out"),
    ] {
        let refused = bb.merge(&r, n, MergeStrategy::Merge).await.unwrap_err();
        assert!(
            matches!(refused, CodeHostError::Refused(_)) && refused.to_string().contains(why),
            "{n}: {refused}"
        );
    }
    assert!(
        matches!(
            bb.merge(&r, 9, MergeStrategy::Rebase).await,
            Err(CodeHostError::Unsupported(_))
        ),
        "a rebase merge is refused before Bitbucket is asked"
    );
    let limited = bb.get_pr(&r, 429).await.unwrap_err();
    assert!(
        matches!(limited, CodeHostError::Transport(_))
            && limited.to_string().contains("rate limit"),
        "{limited}"
    );
    let down = bb.get_pr(&r, 500).await.unwrap_err();
    assert!(
        matches!(down, CodeHostError::Transport(_)) && down.to_string().contains("503"),
        "an answer with no message names its status: {down}"
    );
    let strange = bb.get_pr(&r, 200).await.unwrap_err();
    assert!(
        strange
            .to_string()
            .contains("answered something unexpected"),
        "{strange}"
    );
    for c in stub.calls() {
        assert!(
            !c.path.contains("app-pass"),
            "no credential in a path: {}",
            c.path
        );
    }
    assert!(
        !stub.calls().iter().any(|c| c.path.ends_with("/9/merge")),
        "the rebase never left the machine"
    );
}
