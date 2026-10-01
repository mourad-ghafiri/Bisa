//! GitLab's API implementation against the loopback stub: the requests it
//! makes, the token it carries, what it reads back. Nothing here reaches the
//! network; the stub records what was asked and answers what the test canned.

#[allow(dead_code)]
use crate::support;

use bisa_codehost::creds::TokenStore;
use bisa_codehost::gitlab::{GitLabApi, HOST};
use bisa_codehost::{
    CodeHost, CodeHostError, CodeHostKind, MergeStrategy, PrCreate, PrFilter, PrState, RepoRef,
    Review, ReviewComment, ReviewEvent,
};
use serde_json::json;
use support::{Canned, Stub};

fn repo() -> RepoRef {
    RepoRef {
        host: HOST.into(),
        owner: "acme/platform".into(),
        name: "web".into(),
    }
}

const PROJECT: &str = "/projects/acme%2Fplatform%2Fweb";

async fn host(stub: &Stub) -> (GitLabApi, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let tokens = TokenStore::file_only(CodeHostKind::GitLab, HOST, dir.path());
    tokens.store("stub", "glpat-stub").unwrap();
    (GitLabApi::with_base_url(stub.base_url(), tokens), dir)
}

fn no_env_token() -> bool {
    std::env::var(CodeHostKind::GitLab.env_var()).map_or(true, |v| v.trim().is_empty())
}

fn mr(iid: u64, state: &str) -> serde_json::Value {
    json!({
        "iid": iid, "web_url": format!("https://gitlab.com/acme/platform/web/-/merge_requests/{iid}"), "title": "t",
        "state": state, "draft": false, "detailed_merge_status": "mergeable", "source_branch": "work/x", "sha": "abc",
        "target_branch": "main", "author": {"username": "ada"}, "merge_commit_sha": if state == "merged" { json!("deadbeef") } else { json!(null) },
        "diff_refs": {"base_sha": "b", "head_sha": "h", "start_sha": "s"}
    })
}

#[tokio::test]
async fn every_request_carries_the_bearer_token_and_a_project_is_its_encoded_path() {
    let stub = Stub::start(vec![Canned::json(
        "GET",
        &format!("{PROJECT}/merge_requests/7"),
        200,
        mr(7, "opened"),
    )])
    .await;
    let (gl, _dir) = host(&stub).await;
    let pr = gl.get_pr(&repo(), 7).await.unwrap();
    assert_eq!(
        (pr.number, pr.state, pr.author.as_deref(), pr.mergeable),
        (7, PrState::Open, Some("ada"), Some(true))
    );
    let call = &stub.calls()[0];
    assert!(call.bearer, "a bearer token: {:?}", call.authorization);
    if no_env_token() {
        assert_eq!(call.authorization.as_deref(), Some("Bearer glpat-stub"));
    }
    assert_eq!(call.path, format!("{PROJECT}/merge_requests/7"));
}

#[tokio::test]
async fn the_account_is_the_user_with_its_scopes_and_groups_best_effort() {
    let stub = Stub::start(vec![
        Canned::json(
            "GET",
            "/user",
            200,
            json!({"username": "ada", "name": "Ada"}),
        ),
        Canned::json(
            "GET",
            "/personal_access_tokens/self",
            200,
            json!({"scopes": ["api", "read_user"]}),
        ),
        Canned::json(
            "GET",
            "/groups",
            200,
            json!([{"full_path": "acme"}, {"full_path": "other"}]),
        ),
    ])
    .await;
    let (gl, _dir) = host(&stub).await;
    let me = gl.account().await.unwrap();
    assert_eq!(
        (me.login.as_str(), me.scopes, me.organizations),
        (
            "ada",
            vec!["api".to_string(), "read_user".to_string()],
            vec!["acme".to_string(), "other".to_string()]
        )
    );
    // A refused token page and a refused groups page cost nothing.
    let stub = Stub::start(vec![Canned::json(
        "GET",
        "/user",
        200,
        json!({"username": "ada"}),
    )])
    .await;
    let (gl, _dir) = host(&stub).await;
    let me = gl.verify_token("glpat-candidate", None).await.unwrap();
    assert!(me.scopes.is_empty() && me.organizations.is_empty());
    if no_env_token() {
        assert_eq!(
            stub.calls()[0].authorization.as_deref(),
            Some("Bearer glpat-candidate")
        );
    }
    assert!(matches!(
        gl.verify_token(" ", None).await,
        Err(CodeHostError::NotAuthenticated(_))
    ));
}

#[tokio::test]
async fn creating_a_merge_request_prefixes_draft_and_looks_reviewers_up_by_username() {
    let stub = Stub::start(vec![
        Canned::json(
            "GET",
            "/users",
            200,
            json!([{"id": 42, "username": "alice"}]),
        ),
        Canned::json(
            "POST",
            &format!("{PROJECT}/merge_requests"),
            201,
            mr(8, "opened"),
        ),
    ])
    .await;
    let (gl, _dir) = host(&stub).await;
    let pr = gl
        .create_pr(
            &repo(),
            PrCreate {
                title: "Add".into(),
                body: "b".into(),
                head: "work/x".into(),
                base: "main".into(),
                draft: true,
                reviewers: vec!["alice".into()],
                labels: vec!["x".into(), "y".into()],
            },
        )
        .await
        .unwrap();
    assert_eq!(pr.number, 8);
    let calls = stub.calls();
    assert_eq!(calls[0].query.as_deref(), Some("username=alice"));
    let body = &calls[1].body;
    assert_eq!(body["title"], "Draft: Add");
    assert_eq!(body["source_branch"], "work/x");
    assert_eq!(body["labels"], "x,y");
    assert_eq!(body["reviewer_ids"], json!([42]));
}

#[tokio::test]
async fn checks_are_the_latest_pipelines_jobs_and_none_when_there_is_no_pipeline() {
    let stub = Stub::start(vec![
        Canned::json("GET", &format!("{PROJECT}/merge_requests/7/pipelines"), 200, json!([{"id": 99}, {"id": 98}])),
        Canned::json("GET", &format!("{PROJECT}/pipelines/99/jobs"), 200, json!([{"name": "test", "status": "running", "web_url": "w", "stage": "test"}, {"name": "lint", "status": "failed"}])),
        Canned::json("GET", &format!("{PROJECT}/merge_requests/9/pipelines"), 200, json!([])),
    ])
    .await;
    let (gl, _dir) = host(&stub).await;
    let checks = gl.checks(&repo(), 7).await.unwrap();
    assert_eq!(checks.len(), 2);
    assert_eq!(
        (checks[0].status.as_str(), checks[1].conclusion.as_deref()),
        ("in_progress", Some("failure"))
    );
    assert!(gl.checks(&repo(), 9).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_review_is_an_approval_a_note_and_inline_discussions_and_never_a_request_for_changes() {
    let base = format!("{PROJECT}/merge_requests/7");
    let stub = Stub::start(vec![
        Canned::json("POST", &format!("{base}/approve"), 201, json!({})),
        Canned::json("POST", &format!("{base}/notes"), 201, json!({})),
        Canned::json("GET", &base, 200, mr(7, "opened")),
        Canned::json("POST", &format!("{base}/discussions"), 201, json!({})),
    ])
    .await;
    let (gl, _dir) = host(&stub).await;
    let review = Review {
        event: ReviewEvent::Approve,
        body: Some("ship".into()),
        comments: vec![ReviewComment {
            path: "a.rs".into(),
            line: 3,
            side: Default::default(),
            start_line: None,
            start_side: None,
            body: "rename".into(),
        }],
    };
    gl.submit_review(&repo(), 7, review).await.unwrap();
    let calls = stub.calls();
    assert_eq!(calls[1].body["body"], "ship");
    assert_eq!(calls[3].body["position"]["new_line"], 3);
    assert_eq!(calls[3].body["position"]["head_sha"], "h");
    let err = gl
        .submit_review(
            &repo(),
            7,
            Review {
                event: ReviewEvent::RequestChanges,
                body: Some("no".into()),
                comments: vec![],
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, CodeHostError::Unsupported(_)));
    let err = gl
        .submit_review(
            &repo(),
            7,
            Review {
                event: ReviewEvent::Comment,
                body: None,
                comments: vec![],
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, CodeHostError::Refused(_)),
        "a wordless comment is refused before any request"
    );
    assert_eq!(stub.calls().len(), 4);
}

#[tokio::test]
async fn reviews_are_three_reads_and_a_thread_resolves_by_its_composite_id() {
    let base = format!("{PROJECT}/merge_requests/7");
    let stub = Stub::start(vec![
        Canned::json("GET", &format!("{base}/approvals"), 200, json!({"approved_by": [{"user": {"username": "alice"}}]})),
        Canned::json("GET", &format!("{base}/notes"), 200, json!([{"author": {"username": "bob"}, "body": "fine", "system": false}])),
        Canned::json("GET", &format!("{base}/discussions"), 200, json!([{"id": "d1", "notes": [{"author": {"username": "alice"}, "body": "rename", "resolvable": true, "resolved": false, "position": {"new_path": "a.rs", "new_line": 3}}]}])),
        Canned::json("PUT", &format!("{base}/discussions/d1"), 200, json!({})),
        Canned::json("POST", &format!("{base}/discussions/d1/notes"), 201, json!({"id": 9})),
    ])
    .await;
    let (gl, _dir) = host(&stub).await;
    let out = gl.pr_reviews(&repo(), 7).await.unwrap();
    assert_eq!(out.reviews.len(), 2);
    assert_eq!(out.threads[0].id, "acme/platform/web#7#d1");
    gl.resolve_review_thread(&out.threads[0].id, true)
        .await
        .unwrap();
    assert_eq!(stub.calls()[3].query.as_deref(), Some("resolved=true"));
    assert!(matches!(
        gl.resolve_review_thread("nonsense", true).await,
        Err(CodeHostError::NotFound(_))
    ));
    // A reply is a note on the discussion, by the same composite id.
    gl.reply_review_thread(&out.threads[0].id, "Renamed in abc123.")
        .await
        .unwrap();
    assert_eq!(stub.calls()[4].body["body"], json!("Renamed in abc123."));
    assert!(matches!(
        gl.reply_review_thread("nonsense", "x").await,
        Err(CodeHostError::NotFound(_))
    ));
}

#[tokio::test]
async fn merging_squashes_when_asked_lists_by_gitlabs_words_and_deletes_an_encoded_branch() {
    let base = format!("{PROJECT}/merge_requests");
    let stub = Stub::start(vec![
        Canned::json("PUT", &format!("{base}/7/merge"), 200, mr(7, "merged")),
        Canned::json("GET", &base, 200, json!([mr(7, "merged")])),
        Canned::empty(
            "DELETE",
            &format!("{PROJECT}/repository/branches/work%2Fx"),
            204,
        ),
        Canned::json(
            "PUT",
            &format!("{base}/8/merge"),
            405,
            json!({"message": "405 Method Not Allowed"}),
        ),
    ])
    .await;
    let (gl, _dir) = host(&stub).await;
    let outcome = gl.merge(&repo(), 7, MergeStrategy::Squash).await.unwrap();
    assert_eq!(
        (outcome.merged, outcome.sha.as_deref()),
        (true, Some("deadbeef"))
    );
    assert_eq!(stub.calls()[0].body["squash"], true);
    let merged = gl
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
    assert_eq!(
        stub.calls()[1].query.as_deref(),
        Some("per_page=50&state=merged&source_branch=work%2Fx")
    );
    gl.delete_branch(&repo(), "work/x").await.unwrap();
    let refused = gl
        .merge(&repo(), 8, MergeStrategy::Merge)
        .await
        .unwrap_err();
    assert!(
        matches!(&refused, CodeHostError::Refused(m) if m.contains("405")),
        "{refused}"
    );
    assert!(matches!(
        gl.merge(&repo(), 8, MergeStrategy::Rebase).await,
        Err(CodeHostError::Unsupported(_))
    ));
}

#[tokio::test]
async fn repository_access_reads_the_access_level_and_a_404_is_not_found() {
    let stub = Stub::start(vec![
        Canned::json(
            "GET",
            PROJECT,
            200,
            json!({"permissions": {"project_access": null, "group_access": {"access_level": 40}}}),
        ),
        Canned::json(
            "GET",
            PROJECT,
            200,
            json!({"permissions": {"project_access": {"access_level": 20}, "group_access": null}}),
        ),
        Canned::json(
            "GET",
            PROJECT,
            404,
            json!({"message": "404 Project Not Found"}),
        ),
    ])
    .await;
    let (gl, _dir) = host(&stub).await;
    assert!(
        gl.repo_access(&repo()).await.unwrap().push,
        "a maintainer of the group pushes"
    );
    assert!(
        !gl.repo_access(&repo()).await.unwrap().push,
        "a reporter does not"
    );
    assert!(!gl.repo_access(&repo()).await.unwrap().found);
    assert!(gl
        .detect(&bisa_codehost::RemoteUrl::parse(
            "git@gitlab.com:acme/platform/web.git"
        ))
        .is_some());
    assert!(gl
        .detect(&bisa_codehost::RemoteUrl::parse(
            "git@github.com:acme/web.git"
        ))
        .is_none());
}

#[tokio::test]
async fn statuses_map_to_typed_errors_a_rate_limit_is_a_wait_and_a_strange_answer_is_said() {
    let mr_at = |iid: u64| format!("{PROJECT}/merge_requests/{iid}");
    let stub = Stub::start(vec![
        Canned::json("GET", "/user", 401, json!({"message": "401 Unauthorized"})),
        Canned::json("GET", &mr_at(404), 404, json!({"message": "404 Not found"})),
        Canned::json("GET", &mr_at(403), 403, json!({"message": "403 Forbidden"})),
        Canned::json(
            "PUT",
            &format!("{}/merge", mr_at(1)),
            405,
            json!({"message": "405 Method Not Allowed"}),
        ),
        Canned::json(
            "PUT",
            &format!("{}/merge", mr_at(2)),
            406,
            json!({"message": "Branch cannot be merged"}),
        ),
        Canned::json(
            "PUT",
            &format!("{}/merge", mr_at(3)),
            422,
            json!({"message": {"base": ["has conflicts", "is behind"]}}),
        ),
        Canned::json("GET", &mr_at(429), 429, json!({"message": "Too many"}))
            .header("retry-after", "42"),
        Canned::json("GET", &mr_at(430), 429, json!({"message": "Too many"})),
        Canned::json("GET", &mr_at(500), 502, json!({"message": "bad gateway"})),
        // A proxy's page where a merge request was expected.
        Canned::json("GET", &mr_at(200), 200, json!("<html>maintenance</html>")),
    ])
    .await;
    let (gl, _dir) = host(&stub).await;
    let r = repo();

    let e = gl.account().await.unwrap_err();
    assert!(matches!(e, CodeHostError::NotAuthenticated(_)), "{e}");
    assert!(matches!(
        gl.get_pr(&r, 404).await,
        Err(CodeHostError::NotFound(_))
    ));
    assert!(matches!(
        gl.get_pr(&r, 403).await,
        Err(CodeHostError::NotAuthenticated(_))
    ));
    let mut reasons = Vec::new();
    for iid in [1, 2, 3] {
        let refused = gl.merge(&r, iid, MergeStrategy::Merge).await.unwrap_err();
        assert!(
            matches!(refused, CodeHostError::Refused(_)),
            "{iid}: {refused}"
        );
        reasons.push(refused.to_string());
    }
    assert!(
        reasons[2].contains("base has conflicts, is behind"),
        "a field's reasons are read out: {}",
        reasons[2]
    );
    let limited = gl.get_pr(&r, 429).await.unwrap_err();
    assert!(
        matches!(limited, CodeHostError::Transport(_)) && limited.to_string().contains("in 42s"),
        "a rate limit is a wait, never a bad token: {limited}"
    );
    let limited = gl.get_pr(&r, 430).await.unwrap_err();
    assert!(limited.to_string().contains("in a minute"), "{limited}");
    assert!(matches!(
        gl.get_pr(&r, 500).await,
        Err(CodeHostError::Transport(_))
    ));
    let strange = gl.get_pr(&r, 200).await.unwrap_err();
    assert!(
        matches!(strange, CodeHostError::Transport(_))
            && strange
                .to_string()
                .contains("answered something unexpected"),
        "{strange}"
    );
    for c in stub.calls() {
        assert!(!c.path.contains("glpat-"), "no token in a path: {}", c.path);
    }
}
