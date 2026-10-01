//! `glab`, asked through the scripted runner: which argv each operation runs,
//! what it reads back, and when it steps aside. No program is spawned.

use bisa_codehost::cli::fake::FakeCli;
use bisa_codehost::cli::glab::GlabCli;
use bisa_codehost::cli::{CliProgram, CliStep, CodeHostCli, Fallback};
use bisa_codehost::creds::CliToken;
use bisa_codehost::{
    CodeHostError, MergeStrategy, PrCreate, PrState, RepoRef, Review, ReviewEvent,
};
use std::sync::Arc;

const MR_JSON: &str = r#"{"iid":7,"web_url":"https://gitlab.com/acme/platform/web/-/merge_requests/7","title":"t","state":"opened","draft":false,"detailed_merge_status":"mergeable","source_branch":"work/x","sha":"abc","target_branch":"main","author":{"username":"ada.lovelace"},"diff_refs":{"base_sha":"b","head_sha":"h","start_sha":"s"}}"#;
const MERGED_JSON: &str = r#"{"iid":7,"web_url":"u","title":"t","state":"merged","source_branch":"work/x","target_branch":"main","merge_commit_sha":"deadbeef"}"#;
const STATUS: &str = "gitlab.com\n  ✓ Logged in to gitlab.com as ada.lovelace (/Users/me/.config/glab-cli/config.yml)\n  ✓ Git operations for gitlab.com configured to use https protocol.\n  ✓ Token: ****\n";

fn repo() -> RepoRef {
    RepoRef {
        host: "gitlab.com".into(),
        owner: "acme/platform".into(),
        name: "web".into(),
    }
}

fn installed() -> FakeCli {
    FakeCli::new().with_program(CliProgram::Glab, "/opt/homebrew/bin/glab")
}

fn glab_over(cli: FakeCli) -> (GlabCli, Arc<FakeCli>) {
    let cli = Arc::new(cli);
    (GlabCli::new(cli.clone(), "gitlab.com"), cli)
}

#[tokio::test]
async fn the_probe_and_the_token_read_the_status_the_cli_prints_on_stderr() {
    let (present, cli) = glab_over(
        installed()
            .always(
                CliProgram::Glab,
                &["--version"],
                0,
                "glab version 1.50.0 (2025-01-01)\n",
                "",
            )
            .always(
                CliProgram::Glab,
                &["auth", "status", "--show-token"],
                0,
                "",
                &STATUS.replace("Token: ****", "Token: glpat-secret"),
            )
            .always(CliProgram::Glab, &["auth", "status"], 0, "", STATUS),
    );
    let probe = present.probe().await;
    assert!(probe.installed && probe.signed_in());
    assert_eq!(probe.version.as_deref(), Some("1.50.0"));
    assert_eq!(probe.active().unwrap().login, "ada.lovelace");
    assert_eq!(probe.active().unwrap().protocol.as_deref(), Some("https"));
    assert_eq!(
        present.active_login().await.as_deref(),
        Some("ada.lovelace")
    );
    let (who, token) = present.token(None).await.unwrap();
    assert_eq!(
        (who.as_str(), token.expose()),
        ("ada.lovelace", "glpat-secret")
    );
    assert!(
        present.token(Some("somebody-else")).await.is_none(),
        "glab holds one account per host"
    );
    assert!(cli
        .lines()
        .iter()
        .any(|l| l == "glab auth status --hostname gitlab.com --show-token"));
    assert!(!format!("{:?}", present.probe().await).contains("glpat-"));
}

#[tokio::test]
async fn creating_reading_reviewing_and_merging_run_the_verbs_glab_has() {
    let (glab, cli) = glab_over(installed()
        .answer(CliProgram::Glab, &["mr", "create"], 0, "https://gitlab.com/acme/platform/web/-/merge_requests/7\n", "")
        .answer(CliProgram::Glab, &["mr", "view", "7"], 0, MR_JSON, "")
        .answer(CliProgram::Glab, &["mr", "approve", "7"], 0, "", "")
        .answer(CliProgram::Glab, &["mr", "note", "7"], 0, "", "")
        .answer(CliProgram::Glab, &["api", "approvals"], 0, r#"{"approved_by":[{"user":{"username":"alice"}}]}"#, "")
        .answer(CliProgram::Glab, &["api", "notes"], 0, r#"[{"author":{"username":"bob"},"body":"fine","system":false}]"#, "")
        .answer(CliProgram::Glab, &["api", "discussions"], 0, r#"[{"id":"d1","notes":[{"author":{"username":"alice"},"body":"rename","resolvable":true,"resolved":false,"position":{"new_path":"a.rs","new_line":3}}]}]"#, "")
        .answer(CliProgram::Glab, &["api", "--method", "PUT", "discussions/d1?resolved=true"], 0, "{}", "")
        .answer(CliProgram::Glab, &["api", "--method", "POST", "discussions/d1/notes"], 0, r#"{"id":9}"#, "")
        .answer(CliProgram::Glab, &["mr", "merge", "7"], 0, "", "")
        .answer(CliProgram::Glab, &["mr", "view", "7"], 0, MERGED_JSON, "")
        .answer(CliProgram::Glab, &["api", "--method", "DELETE"], 0, "", ""));

    let pr = glab
        .create_pr(
            &repo(),
            PrCreate {
                title: "Add".into(),
                body: "b".into(),
                head: "work/x".into(),
                base: "main".into(),
                draft: true,
                reviewers: vec!["alice".into()],
                labels: vec!["x".into()],
            },
        )
        .await
        .unwrap();
    assert_eq!(
        (pr.number, pr.author.as_deref(), pr.mergeable),
        (7, Some("ada.lovelace"), Some(true))
    );
    glab.submit_review(
        &repo(),
        7,
        Review {
            event: ReviewEvent::Approve,
            body: Some("ship".into()),
            comments: vec![],
        },
    )
    .await
    .unwrap();
    let reviews = glab.pr_reviews(&repo(), 7).await.unwrap();
    assert_eq!(reviews.reviews.len(), 2);
    assert_eq!(reviews.threads[0].id, "acme/platform/web#7#d1");
    glab.resolve_review_thread("acme/platform/web#7#d1", true)
        .await
        .unwrap();
    glab.reply_review_thread("acme/platform/web#7#d1", "Renamed in abc123.")
        .await
        .unwrap();
    let outcome = glab.merge(&repo(), 7, MergeStrategy::Squash).await.unwrap();
    assert_eq!(
        (outcome.merged, outcome.sha.as_deref()),
        (true, Some("deadbeef"))
    );
    glab.delete_branch(&repo(), "work/x").await.unwrap();

    let lines = cli.lines();
    assert_eq!(lines[0], "glab mr create --repo acme/platform/web --title Add --description b --source-branch work/x --target-branch main --yes --draft --reviewer alice --label x");
    assert_eq!(
        lines[1],
        "glab mr view 7 --repo acme/platform/web --output json"
    );
    assert_eq!(lines[2], "glab mr approve 7 --repo acme/platform/web");
    assert_eq!(
        lines[3],
        "glab mr note 7 --repo acme/platform/web --message ship"
    );
    assert_eq!(
        lines[4],
        "glab api --method GET projects/acme%2Fplatform%2Fweb/merge_requests/7/approvals"
    );
    assert_eq!(lines[7], "glab api --method PUT projects/acme%2Fplatform%2Fweb/merge_requests/7/discussions/d1?resolved=true");
    assert_eq!(lines[8], "glab api --method POST projects/acme%2Fplatform%2Fweb/merge_requests/7/discussions/d1/notes --input -");
    assert_eq!(
        cli.calls()[8].stdin.as_deref(),
        Some(r#"{"body":"Renamed in abc123."}"#),
        "the note rides on stdin"
    );
    assert_eq!(
        lines[9],
        "glab mr merge 7 --repo acme/platform/web --yes --squash"
    );
    assert_eq!(
        lines[11],
        "glab api --method DELETE projects/acme%2Fplatform%2Fweb/repository/branches/work%2Fx"
    );
    assert_eq!(cli.unconsumed(), 0);
    assert!(
        cli.calls().iter().all(|c| c.env("GITLAB_HOST").is_none()),
        "gitlab.com needs no GITLAB_HOST"
    );
}

#[tokio::test]
async fn what_gitlab_has_no_verb_for_and_when_the_cli_steps_aside() {
    let (glab, _) = glab_over(installed());
    let err = glab
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
    assert!(
        matches!(err, CliStep::Host(CodeHostError::Unsupported(_))),
        "a request-changes review is not GitLab's, so nobody is asked: {err:?}"
    );
    let err = glab
        .merge(&repo(), 7, MergeStrategy::Rebase)
        .await
        .unwrap_err();
    assert!(matches!(err, CliStep::Host(CodeHostError::Unsupported(_))));

    let cli = Arc::new(installed().always(CliProgram::Glab, &["auth", "status"], 0, "", STATUS));
    let bound =
        Arc::new(GlabCli::new(cli.clone(), "gitlab.com")).for_account(Some("somebody-else"));
    let err = bound.get_pr(&repo(), 7).await.unwrap_err();
    assert!(
        matches!(&err, CliStep::Fallback(Fallback::NoSuchAccount(l)) if l == "somebody-else"),
        "{err:?}"
    );

    let cli = Arc::new(installed().answer(CliProgram::Glab, &["mr", "view"], 0, MR_JSON, ""));
    let own = GlabCli::new(cli.clone(), "git.acme.internal");
    own.get_pr(&repo(), 7).await.unwrap();
    assert_eq!(cli.calls()[0].env("GITLAB_HOST"), Some("git.acme.internal"));

    let (absent, _) = glab_over(FakeCli::new());
    assert!(matches!(
        absent.get_pr(&repo(), 7).await.unwrap_err(),
        CliStep::Fallback(Fallback::NotInstalled)
    ));
    assert!(!absent.probe().await.installed);
    let (unsigned, _) = glab_over(installed().answer(
        CliProgram::Glab,
        &["mr", "view"],
        1,
        "",
        "No GitLab hosts are configured. To log in, run: glab auth login",
    ));
    assert!(matches!(
        unsigned.get_pr(&repo(), 7).await.unwrap_err(),
        CliStep::Fallback(Fallback::NotSignedIn)
    ));
    let (listing, _) = glab_over(installed().answer(
        CliProgram::Glab,
        &["merge_requests?per_page=50&state=merged&source_branch=work%2Fx"],
        0,
        &format!("[{MERGED_JSON}]"),
        "",
    ));
    let merged = listing
        .list_prs(
            &repo(),
            bisa_codehost::PrFilter {
                state: Some(PrState::Merged),
                head: Some("work/x".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(merged[0].state, PrState::Merged);
}
