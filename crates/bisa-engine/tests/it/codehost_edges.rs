//! Code hosts, accounts, profiles and the connection facts at their edges
//! (ide/04, ide/08): an account added for each kind and a kind this build
//! lacks said by name; a helper's credential resolving a kind and the
//! inspection offering every account it knows; a checkout pinned to an
//! account and a project whose folder went; the pull request doors where
//! the host has no drafts, where the request is already open, where the
//! cache answers, where a delete after a merge is refused; a workstream
//! born from a pull request; a host without checks or threads; the
//! connection facts over SSH without ssh, over HTTPS with a pinned
//! username, and the probes with a fake ssh; the profiles beside a
//! person's own includes, odd files and a git too old; and a long branch
//! with a wide diff said so in the pull request brief.
//!
//! Fakes only: the code host is the in-memory fake, SSH is a scripted fake
//! over a temp directory, the engine's git sees one temp global file, and
//! `origin` is a bare repository on this disk whose name is the fake's host.
//! Nothing here reads `~/.ssh`, a keychain or a helper of the machine's, and
//! nothing is deleted: a folder in the way is moved aside.

use crate::common;

use bisa_codehost::fake::FakeCodeHost;
use bisa_codehost::{CodeHostCapabilities, CodeHostKind, MergeStrategy, ReviewEvent};
use bisa_core::{
    AgentId, ProfileSpec, Project, PublishPolicy, Slug, WorkstreamId, WorkstreamSource,
    WorkstreamState,
};
use bisa_engine::codehost::{self, Connection, PrRequest, PrReviews, ReviewSummary, ReviewThread};
use bisa_engine::ide::connection::{self, AccountSource, CautionId, Transport};
use bisa_engine::{gitprofiles, projects, ssh, Engine, EngineConfig, EngineError};
use bisa_harness::mock::MockAdapter;
use bisa_ssh::exec::Program;
use bisa_ssh::{FakeSsh, Ssh};
use bisa_store::NewProject;
use std::path::{Path, PathBuf};
use std::sync::Arc;

struct Rig {
    dir: tempfile::TempDir,
    engine: Engine,
    /// Every prompt the General Agent's mock received, in order.
    prompts: Arc<std::sync::Mutex<Vec<String>>>,
}

/// A git whose global file is the test's own, under `dir`.
fn own_git(dir: &Path) -> bisa_vcs::Git {
    bisa_vcs::Git::new()
        .with_env("GIT_CONFIG_GLOBAL", dir.join("global.gitconfig"))
        .with_env("GIT_CONFIG_NOSYSTEM", "1")
}

fn raw_git(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("spawn git");
    assert!(
        out.status.success(),
        "git {args:?} in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

impl Rig {
    fn start(
        hosts: Vec<Arc<FakeCodeHost>>,
        ssh: Option<FakeSsh>,
        git: Option<bisa_vcs::Git>,
    ) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mock = MockAdapter {
            id: "codehost-mock".into(),
            ..Default::default()
        };
        let ws = common::workspace(&dir);
        let mut general = ws.get_agent(&AgentId::general()).unwrap();
        general.harness = mock.id.clone();
        ws.update_agent(general).unwrap();
        let keys = dir.path().join("keys");
        std::fs::create_dir_all(&keys).unwrap();
        let prompts = Arc::clone(&mock.prompts);
        let engine = Engine::start(
            ws,
            common::catalog_with(vec![mock]),
            EngineConfig {
                design_enabled: false,
                events_enabled: false,
                git: Some(git.unwrap_or_else(|| own_git(dir.path()))),
                ssh: ssh.map(|fake| Ssh::new(Arc::new(fake), &keys, dir.path())),
                code_hosts: Some(bisa_codehost::CodeHostRegistry::new(
                    hosts
                        .into_iter()
                        .map(|h| h as Arc<dyn bisa_codehost::CodeHost>)
                        .collect(),
                )),
                ..Default::default()
            },
        )
        .unwrap();
        Self {
            dir,
            engine,
            prompts,
        }
    }

    fn inner(&self) -> &bisa_engine::Inner {
        self.engine.inner()
    }

    /// A bare repository beside the workspace named `name` — the fake's host
    /// when the fake should claim it — whose HEAD names `branch`.
    fn bare(&self, name: &str, branch: &str) -> PathBuf {
        let origin = self.dir.path().join(format!("{name}.git"));
        std::fs::create_dir_all(&origin).unwrap();
        raw_git(
            &origin,
            &["init", "--bare", "--quiet", "--initial-branch", branch],
        );
        origin
    }

    /// A git project with one commit, a local identity and `origin` at `url`.
    async fn project(&self, slug: &str, url: &str, publish: PublishPolicy) -> (Project, PathBuf) {
        let ws = self.engine.workspace();
        let mut project = ws
            .create_project(NewProject::managed(slug).unwrap())
            .unwrap();
        project.publish = publish;
        let project = ws.update_project(project).unwrap();
        let project = projects::init_git(self.inner(), &project)
            .await
            .expect("init_git");
        let root = ws.project_root_path(&project);
        raw_git(&root, &["config", "user.name", "Bisa Test"]);
        raw_git(&root, &["config", "user.email", "test@example.invalid"]);
        std::fs::write(root.join("README.md"), "baseline\n").unwrap();
        raw_git(&root, &["add", "-A"]);
        raw_git(&root, &["commit", "--quiet", "-m", "baseline"]);
        raw_git(&root, &["remote", "add", "origin", url]);
        (ws.get_project(project.id).unwrap(), root)
    }

    /// A worktree workstream on `branch` with one commit of its own.
    async fn branch(&self, project: &Project, branch: &str) -> (WorkstreamId, PathBuf) {
        let opened = projects::open_workstream_for(
            self.inner(),
            project,
            projects::WorkstreamRequest {
                goal: None,
                work_item: None,
                label: branch.into(),
                agent: None,
                source: WorkstreamSource::NewBranch {
                    name: Some(branch.into()),
                    start: None,
                },
                base: None,
            },
        )
        .await
        .unwrap();
        let path = self
            .engine
            .workspace()
            .workstream_checkout(&opened.workstream)
            .unwrap();
        std::fs::write(
            path.join(format!("{}.md", branch.replace('/', "-"))),
            "one\n",
        )
        .unwrap();
        projects::commit_workstream(self.inner(), opened.workstream.id, "one more")
            .await
            .unwrap();
        (opened.workstream.id, path)
    }

    fn tokens(&self, kind: CodeHostKind) -> bisa_codehost::creds::TokenStore {
        bisa_codehost::creds::TokenStore::file_only(
            kind,
            kind.public_host(),
            self.engine
                .workspace()
                .paths()
                .codehost_tokens_root()
                .join(kind.as_str()),
        )
    }
}

fn kinded(kind: CodeHostKind, login: &str) -> Arc<FakeCodeHost> {
    Arc::new(FakeCodeHost::of_kind(kind, kind.public_host()).signed_in_as(login))
}

fn full() -> CodeHostCapabilities {
    CodeHostCapabilities {
        draft_prs: false,
        reviewers: true,
        labels: true,
        merge_strategies: vec![MergeStrategy::Merge],
        check_runs: true,
        review_comments: true,
        review_threads: true,
        review_thread_replies: true,
        delete_branch: true,
        review_events: vec![ReviewEvent::Approve, ReviewEvent::Comment],
    }
}

fn acme(account: Option<&str>, key: Option<&str>, email: &str) -> ProfileSpec {
    ProfileSpec {
        label: "Acme".into(),
        host: "github.com".into(),
        owner: "acme".into(),
        aliases: vec![],
        name: "Bisa Test".into(),
        email: email.into(),
        ssh_key: key.map(str::to_string),
        account: account.map(str::to_string),
    }
}

// --- the accounts, per kind -------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn an_account_is_added_for_each_kind_and_a_kind_this_build_lacks_is_said_by_name() {
    let rig = Rig::start(
        vec![
            kinded(CodeHostKind::GitLab, "ada"),
            kinded(CodeHostKind::Bitbucket, "bob"),
        ],
        None,
        None,
    );
    let inner = rig.inner();
    let (login, connection) =
        codehost::add_account(inner, CodeHostKind::GitLab, "a-word-for-the-test", None)
            .await
            .unwrap();
    assert_eq!(login, "ada");
    assert!(matches!(connection, Connection::Connected { .. }));
    let (login, _) =
        codehost::add_account(inner, CodeHostKind::Bitbucket, "another-word", Some("bob"))
            .await
            .unwrap();
    assert_eq!(login, "bob");
    let err = codehost::add_account(inner, CodeHostKind::GitHub, "a-word", None)
        .await
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("this build has no GitHub code host"),
        "{err}"
    );

    // With a stored account and no default, the health says the store answers
    // for that one; the sign-in plan without a CLI runner is the install.
    let health = codehost::health(inner, CodeHostKind::GitLab).await.unwrap();
    assert_eq!(
        health.resolves.map(|r| (r.login, r.source)),
        Some((
            Some("ada".to_string()),
            bisa_codehost::creds::TokenSource::File
        ))
    );
    assert!(health.cli.is_none(), "no runner, no CLI probe");
    assert!(matches!(
        codehost::login_plan(inner, CodeHostKind::GitHub).await,
        codehost::LoginPlan::Install { .. }
    ));
    assert!(matches!(
        codehost::check_account(inner, CodeHostKind::GitLab, "ada")
            .await
            .unwrap(),
        Connection::Connected { .. }
    ));
    codehost::forget_account(inner, CodeHostKind::GitLab, "ada").unwrap();
    assert!(matches!(
        codehost::check_account(inner, CodeHostKind::GitLab, "ada")
            .await
            .unwrap(),
        Connection::NoToken
    ));

    // A registry with one fake of no kind: that one answers for any kind.
    let fake = Arc::new(FakeCodeHost::minimal("fake.example").signed_in_as("you"));
    let plain = Rig::start(vec![fake], None, None);
    let (login, _) = codehost::add_account(plain.inner(), CodeHostKind::GitHub, "a-word", None)
        .await
        .unwrap();
    assert_eq!(login, "you");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_helpers_credential_resolves_a_kind_and_an_inspection_offers_every_account_it_knows() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("helper.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\nprintf 'username=%s\\npassword=%s\\n' someone a-word-the-helper-holds\n",
    )
    .unwrap();
    let global = dir.path().join("global.gitconfig");
    std::fs::write(
        &global,
        format!("[credential]\n\thelper = !sh {}\n", script.display()),
    )
    .unwrap();
    let git = bisa_vcs::Git::new()
        .with_env("GIT_CONFIG_GLOBAL", global)
        .with_env("GIT_CONFIG_NOSYSTEM", "1");
    let rig = Rig::start(vec![kinded(CodeHostKind::GitHub, "ada")], None, Some(git));
    let inner = rig.inner();
    if std::env::var(CodeHostKind::GitHub.env_var()).is_ok_and(|v| !v.trim().is_empty()) {
        return;
    }
    let health = codehost::health(inner, CodeHostKind::GitHub).await.unwrap();
    assert_eq!(health.accounts.helpers, ["sh"]);
    assert_eq!(
        health.resolves.map(|r| (r.login, r.source)),
        Some((
            Some("someone".to_string()),
            bisa_codehost::creds::TokenSource::Git
        )),
        "nothing stored, no CLI: git's helper answers"
    );

    gitprofiles::put(
        inner,
        Slug::new("acme").unwrap(),
        acme(Some("ada-acme"), None, "ada@acme.example"),
    )
    .await
    .unwrap();
    codehost::set_default_account(inner, CodeHostKind::GitHub, Some("deflt"))
        .await
        .unwrap();
    rig.tokens(CodeHostKind::GitHub)
        .store("stored-one", "a-word-for-the-test")
        .unwrap();
    let inspected = codehost::inspect(
        inner,
        codehost::InspectSource::Url("https://github.com/acme/web.git".into()),
    )
    .await
    .unwrap();
    assert_eq!(
        inspected
            .accounts
            .iter()
            .map(|a| (a.login.as_str(), a.source))
            .collect::<Vec<_>>(),
        [
            ("ada-acme", codehost::AccountChoiceSource::Profile),
            ("deflt", codehost::AccountChoiceSource::Default),
            ("stored-one", codehost::AccountChoiceSource::Stored),
            ("someone", codehost::AccountChoiceSource::GitHelper),
        ]
    );
    assert_eq!(inspected.suggested.as_deref(), Some("ada-acme"));
    assert_eq!(
        inspected.profile.map(|p| (p.slug, p.account)),
        Some(("acme".to_string(), Some("ada-acme".to_string())))
    );
    assert!(inspected.cautions.is_empty());

    let err = codehost::inspect(inner, codehost::InspectSource::Url("   ".into()))
        .await
        .unwrap_err();
    assert!(matches!(err, EngineError::Invalid(_)), "{err}");
    let plain = dir.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let none = codehost::inspect(inner, codehost::InspectSource::Path(plain))
        .await
        .unwrap();
    assert!(none.remote.is_none() && none.code_host.is_none());
    let (_, root) = rig
        .project(
            "web",
            "https://github.com/acme/web.git",
            PublishPolicy::Auto,
        )
        .await;
    let read = codehost::inspect(inner, codehost::InspectSource::Path(root))
        .await
        .unwrap();
    assert_eq!(read.code_host.map(|h| h.kind), Some(CodeHostKind::GitHub));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_checkout_pinned_to_an_account_resolves_it_and_a_project_whose_folder_went_is_said() {
    let fake = kinded(CodeHostKind::GitHub, "ada");
    let rig = Rig::start(vec![fake.clone()], None, None);
    let inner = rig.inner();
    let (project, root) = rig
        .project(
            "web",
            "https://github.com/acme/web.git",
            PublishPolicy::Auto,
        )
        .await;
    raw_git(&root, &["config", "codehost.account", "Pinned"]);
    let caps = codehost::capabilities(inner, project.id).await.unwrap();
    assert_eq!(
        caps.map(|(id, repo, _)| (id.0, repo.slug())),
        Some(("github", "acme/web".to_string()))
    );
    let wid = WorkstreamId::primary_of(project.id);
    assert_eq!(
        connection::resolved_account(inner, wid).await.unwrap(),
        Some((CodeHostKind::GitHub, "Pinned".to_string())),
        "the pin as written"
    );
    let empty = codehost::list_open_prs(inner, project.id).await.unwrap();
    assert!(empty.is_empty());
    assert!(
        fake.asked_as().iter().all(|l| l == "pinned"),
        "the host was bound to the pin, lowercased: {:?}",
        fake.asked_as()
    );

    // The record of a clone names the host the fake stands for.
    let origin = rig.bare("github.com", "main");
    raw_git(
        &root,
        &["push", "--quiet", origin.to_str().unwrap(), "HEAD:main"],
    );
    let cloned = rig
        .engine
        .workspace()
        .create_project(NewProject::managed("twin").unwrap())
        .unwrap();
    let cloned = projects::clone(inner, &cloned, origin.to_str().unwrap(), None)
        .await
        .unwrap();
    match cloned.vcs {
        bisa_core::Vcs::Git { code_host, .. } => {
            let host = code_host.expect("the fake claims the bare origin by its name");
            assert_eq!(
                (host.kind.as_str(), host.owner_repo.as_str()),
                ("github", "local/github.com")
            );
        }
        other => panic!("{other:?}"),
    }

    // A config git cannot read: git's own error, never a code host's absence.
    let config = root.join(".git").join("config");
    let kept = std::fs::read_to_string(&config).unwrap();
    std::fs::write(&config, format!("{kept}[core]\n\tbare = maybe\n")).unwrap();
    let err = codehost::capabilities(inner, project.id).await.unwrap_err();
    assert!(matches!(&err, EngineError::Vcs(_)), "{err}");
    std::fs::write(&config, kept).unwrap();

    // The folder gone: said as such, never as a code host's absence.
    std::fs::rename(&root, root.with_extension("moved-aside")).unwrap();
    let err = codehost::capabilities(inner, project.id).await.unwrap_err();
    assert!(
        matches!(&err, EngineError::Invalid(t) if t.to_string().contains("no folder")),
        "{err}"
    );
}

// --- the pull request doors -----------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn the_pull_request_doors_at_their_edges() {
    let fake = Arc::new(FakeCodeHost {
        kind: Some(CodeHostKind::GitHub),
        ..FakeCodeHost::with_capabilities("github.com", full()).signed_in_as("you")
    });
    let rig = Rig::start(vec![fake.clone()], None, None);
    let inner = rig.inner();
    let origin = rig.bare("github.com", "main");
    let (project, root) = rig
        .project("web", origin.to_str().unwrap(), PublishPolicy::Auto)
        .await;
    raw_git(&root, &["push", "--quiet", "origin", "HEAD:main"]);
    let (wid, _) = rig.branch(&project, "feature/a").await;

    // No drafts on this host: refused after the push, before the host.
    let err = codehost::open_pr(
        inner,
        wid,
        PrRequest {
            title: "Add".into(),
            draft: true,
            ..Default::default()
        },
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("draft pull requests"), "{err}");
    assert!(matches!(
        rig.engine.workspace().get_workstream(wid).unwrap().state,
        WorkstreamState::Pushed
    ));
    // Reviewers go with the request when the host takes them.
    let pr = codehost::open_pr(
        inner,
        wid,
        PrRequest {
            title: "Add".into(),
            reviewers: vec!["bob".into()],
            labels: vec!["ready".into()],
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(pr.number, 1);
    // Already open: the table's refusal, before the host is asked again.
    let asked = fake.asked_as().len();
    codehost::open_pr(inner, wid, PrRequest::default())
        .await
        .unwrap_err();
    assert_eq!(fake.asked_as().len(), asked, "the host was not asked again");

    // Checks and reviews: the host once, the cache after.
    fake.state
        .checks
        .lock()
        .unwrap()
        .push(bisa_codehost::CheckRun {
            name: "ci".into(),
            status: "completed".into(),
            conclusion: Some("success".into()),
            url: None,
            summary: None,
        });
    assert_eq!(codehost::checks(inner, wid).await.unwrap().len(), 1);
    let reviews = codehost::pr_reviews(inner, wid).await.unwrap();
    assert!(reviews.reviews.is_empty());
    let asked = fake.asked_as().len();
    assert_eq!(codehost::checks(inner, wid).await.unwrap().len(), 1);
    codehost::pr_reviews(inner, wid).await.unwrap();
    assert_eq!(
        fake.asked_as().len(),
        asked,
        "the cache answers the second reads; the host is not bound again"
    );
    let source = codehost::review_source(inner, wid).await.unwrap();
    assert!(
        matches!(&source, bisa_engine::content::ContentSource::CodeHost { number: 1, repo, .. } if repo == "local/github.com"),
        "{source:?}"
    );
    let words = codehost::review_words(&PrReviews {
        reviews: vec![
            ReviewSummary {
                author: Some("bob".into()),
                state: "approved".into(),
                body: " fine ".into(),
                submitted_at: None,
            },
            ReviewSummary {
                author: None,
                state: "commented".into(),
                body: "   ".into(),
                submitted_at: None,
            },
        ],
        threads: vec![ReviewThread {
            id: "T1".into(),
            path: None,
            line: None,
            is_resolved: false,
            is_outdated: false,
            comments: vec![
                bisa_codehost::ReviewThreadComment {
                    author: None,
                    body: "rename".into(),
                    created_at: None,
                },
                bisa_codehost::ReviewThreadComment {
                    author: Some("ada".into()),
                    body: "".into(),
                    created_at: None,
                },
            ],
        }],
    });
    assert_eq!(
        words,
        "review by bob (approved):\nfine\n\ncomment by someone on the pull request:\nrename\n\n",
        "a wordless review or comment is left out; nobody is someone"
    );

    // A merge that lands, and a branch delete the host refuses afterwards:
    // reported, never fatal.
    fake.refuse_branch_deletes(Some("protected"));
    let out = codehost::merge(inner, wid, MergeStrategy::Merge, true)
        .await
        .unwrap();
    assert!(out.merged);
    assert_eq!(out.remote_branch_deleted, Some(false));
    assert!(fake.state.deleted_branches.lock().unwrap().is_empty());

    // A workstream born from a pull request on the host: fetched from the
    // branch the fake names, the record linked at birth.
    raw_git(
        &root,
        &["push", "--quiet", "origin", "HEAD:refs/heads/work/x"],
    );
    let repo = bisa_codehost::RepoRef {
        host: "github.com".into(),
        owner: "local".into(),
        name: "github.com".into(),
    };
    let seeded = fake.seed_pr(&repo, "alice", "work/x", "main");
    let born = projects::open_workstream_for(
        inner,
        &project,
        projects::WorkstreamRequest {
            goal: None,
            work_item: None,
            label: "from the pr".into(),
            agent: None,
            source: WorkstreamSource::PullRequest {
                number: seeded.number,
            },
            base: None,
        },
    )
    .await
    .unwrap();
    assert!(
        matches!(born.workstream.state, WorkstreamState::PrOpen { number, .. } if number == seeded.number),
        "{:?}",
        born.workstream.state
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_host_without_checks_or_threads_answers_empty() {
    let fake = Arc::new(FakeCodeHost {
        kind: Some(CodeHostKind::GitHub),
        ..FakeCodeHost::minimal("github.com")
    });
    let rig = Rig::start(vec![fake.clone()], None, None);
    let inner = rig.inner();
    let origin = rig.bare("github.com", "main");
    let (project, root) = rig
        .project("web", origin.to_str().unwrap(), PublishPolicy::Auto)
        .await;
    raw_git(&root, &["push", "--quiet", "origin", "HEAD:main"]);
    let (wid, _) = rig.branch(&project, "feature/b").await;
    projects::open_pr(inner, wid, "Add", "").await.unwrap();
    assert!(codehost::checks(inner, wid).await.unwrap().is_empty());
    assert_eq!(
        codehost::pr_reviews(inner, wid).await.unwrap(),
        PrReviews::default()
    );
}

// --- the connection facts ---------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn the_connection_facts_without_ssh_over_https_and_under_every_sign_in_word() {
    let rig = Rig::start(
        vec![
            kinded(CodeHostKind::GitHub, "ada"),
            kinded(CodeHostKind::Bitbucket, "bob"),
            Arc::new(FakeCodeHost::minimal("fake.example")),
        ],
        None,
        None,
    );
    let inner = rig.inner();
    if std::env::var(CodeHostKind::GitHub.env_var()).is_ok_and(|v| !v.trim().is_empty()) {
        return;
    }

    // An SSH alias with a profile's command, and no ssh on this engine: the
    // key is read off the command, nothing else is known.
    let (project, root) = rig
        .project("alias", "git@github-work:acme/web.git", PublishPolicy::Auto)
        .await;
    // The key is read off the command; the program itself is none, so the
    // probe's `git ls-remote` fails on this machine instead of running ssh.
    raw_git(
        &root,
        &[
            "config",
            "core.sshCommand",
            "/usr/bin/false -i /keys/id_work -o IdentitiesOnly=yes",
        ],
    );
    let wid = WorkstreamId::primary_of(project.id);
    let facts = connection::connection(inner, wid).await.unwrap();
    match &facts.transport {
        Transport::Ssh {
            key,
            key_name,
            identities_only,
            ssh_configured,
            loaded,
            ..
        } => {
            assert_eq!(key.as_deref(), Some(Path::new("/keys/id_work")));
            assert_eq!(key_name.as_deref(), Some("id_work"));
            assert!(*identities_only && !ssh_configured && loaded.is_none());
        }
        other => panic!("{other:?}"),
    }
    assert!(
        facts.code_host.is_none(),
        "an alias nobody resolves is no host"
    );
    assert_eq!(
        connection::resolved_account(inner, wid).await.unwrap(),
        None
    );
    let check = connection::check(inner, wid).await.unwrap();
    assert!(check.code_host.is_none() && check.access.is_none() && check.ssh.is_none());
    assert_eq!(
        check.ls_remote.map(|l| l.ok),
        Some(false),
        "ls-remote through a command that is no ssh fails, and says so"
    );

    // HTTPS with a pinned credential username that is the account's: no caution.
    let (project, root) = rig
        .project(
            "https",
            "https://github.com/acme/web.git",
            PublishPolicy::Auto,
        )
        .await;
    raw_git(&root, &["config", "credential.username", "ADA"]);
    rig.tokens(CodeHostKind::GitHub)
        .store("ada", "a-word-for-the-test")
        .unwrap();
    let wid = WorkstreamId::primary_of(project.id);
    let facts = connection::connection(inner, wid).await.unwrap();
    assert!(
        matches!(&facts.transport, Transport::Https { credential_username: Some(u), .. } if u == "ADA"),
        "{:?}",
        facts.transport
    );
    assert_eq!(facts.account.source, AccountSource::OnlyStored);
    assert!(facts.cautions.is_empty(), "{:?}", facts.cautions);

    // A profile for the owner whose email differs from the repository's own,
    // with a key no other profile shares: one caution, about the author.
    gitprofiles::put(
        inner,
        Slug::new("acme").unwrap(),
        acme(None, Some("/keys/id_acme"), "other@acme.example"),
    )
    .await
    .unwrap();
    let facts = connection::connection(inner, wid).await.unwrap();
    assert_eq!(
        facts.cautions.iter().map(|c| c.id).collect::<Vec<_>>(),
        [CautionId::IdentityDiffersFromProfile]
    );
    gitprofiles::put(
        inner,
        Slug::new("acme").unwrap(),
        acme(None, None, "other@acme.example"),
    )
    .await
    .unwrap();
    let facts = connection::connection(inner, wid).await.unwrap();
    assert_eq!(
        facts.cautions.len(),
        1,
        "the same profile without a key: the same one caution"
    );

    // A checkout with no remote at all: nothing to probe, nothing to say.
    let (project, _) = rig
        .project("alone", "/nowhere/alone.git", PublishPolicy::Auto)
        .await;
    let wid = WorkstreamId::primary_of(project.id);
    raw_git(
        &rig.engine.workspace().project_root_path(&project),
        &["remote", "remove", "origin"],
    );
    let check = connection::check(inner, wid).await.unwrap();
    assert!(check.ls_remote.is_none() && check.code_host.is_none());

    // The sign-in words name the kind's door: a token page for Bitbucket, the
    // host's own panel for a host of no kind.
    let (project, root) = rig
        .project(
            "bb",
            "https://bitbucket.org/acme/web.git",
            PublishPolicy::Auto,
        )
        .await;
    raw_git(&root, &["config", "credential.username", "bob"]);
    let facts = connection::connection(inner, WorkstreamId::primary_of(project.id))
        .await
        .unwrap();
    assert!(
        matches!(&facts.transport, Transport::Https { credential_username: Some(u), .. } if u == "bob")
    );
    assert_eq!(
        facts.account.login, None,
        "a username git pushes as is no account"
    );
    let caution = facts
        .cautions
        .iter()
        .find(|c| c.id == CautionId::NoAccount)
        .expect("nobody is signed in to Bitbucket");
    assert!(
        caution.sentence.contains("add a token under Settings"),
        "{}",
        caution.sentence
    );
    let (project, _) = rig
        .project(
            "own",
            "https://fake.example/acme/web.git",
            PublishPolicy::Auto,
        )
        .await;
    let facts = connection::connection(inner, WorkstreamId::primary_of(project.id))
        .await
        .unwrap();
    let caution = facts
        .cautions
        .iter()
        .find(|c| c.id == CautionId::NoAccount)
        .expect("nobody is signed in to the fake");
    assert!(
        caution
            .sentence
            .contains("sign in on your code host's panel"),
        "{}",
        caution.sentence
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_probes_run_the_handshake_through_the_fake_ssh_and_say_when_it_did_not_run() {
    let fake = FakeSsh::new()
        .answer(Program::SshAdd, &["-l"], 1, "", "The agent has no identities.\n")
        .answer(Program::SshAdd, &["-l"], 1, "", "The agent has no identities.\n")
        .answer(Program::SshAdd, &["-l"], 1, "", "The agent has no identities.\n")
        .answer(Program::SshAdd, &["-l"], 1, "", "The agent has no identities.\n")
        .answer(
            Program::Ssh,
            &["-T", "git@github-work"],
            1,
            "",
            "Hi ada! You've successfully authenticated, but GitHub does not provide shell access.\n",
        );
    let rig = Rig::start(vec![kinded(CodeHostKind::GitHub, "ada")], Some(fake), None);
    let inner = rig.inner();
    assert_eq!(
        ssh::dir(inner),
        Some(rig.dir.path().join("keys").as_path()),
        "the directory this engine reads"
    );
    let (project, root) = rig
        .project("alias", "git@github-work:acme/web.git", PublishPolicy::Auto)
        .await;
    // `git ls-remote` would run ssh: a command that is none keeps the probe
    // on this machine and fails at once.
    raw_git(&root, &["config", "core.sshCommand", "/usr/bin/false"]);
    let wid = WorkstreamId::primary_of(project.id);
    let check = connection::check(inner, wid).await.unwrap();
    assert!(
        matches!(
            check.ssh,
            Some(bisa_ssh::HostGreeting::Authenticated { .. })
        ),
        "{:?}",
        check.ssh
    );
    assert_eq!(check.ls_remote.map(|l| l.ok), Some(false));
    // The alias `ssh -G` never answered: the facts stay unknown, and the
    // handshake with no answer left is no greeting at all.
    let check = connection::check(inner, wid).await.unwrap();
    assert!(check.ssh.is_none(), "{:?}", check.ssh);
    let facts = connection::connection(inner, wid).await.unwrap();
    assert!(
        matches!(
            &facts.transport,
            Transport::Ssh {
                ssh_configured: true,
                key: None,
                loaded: None,
                ..
            }
        ),
        "{:?}",
        facts.transport
    );
}

// --- profiles -----------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn profiles_beside_a_persons_own_include_odd_files_and_a_git_too_old_are_read_in_kind() {
    let rig = Rig::start(vec![], None, None);
    let inner = rig.inner();
    let git = own_git(rig.dir.path());
    let dir = rig.engine.workspace().paths().git_profiles_dir();
    std::fs::create_dir_all(&dir).unwrap();
    gitprofiles::put(
        inner,
        Slug::new("acme").unwrap(),
        acme(None, None, "ada@acme.example"),
    )
    .await
    .unwrap();
    // A person's own include, a profile include whose file is no slug, one
    // whose file is gone, and one whose globs read as no profile.
    git.include_set(
        bisa_vcs::ConfigScope::Global,
        "gitdir:/somewhere/",
        Path::new("/somewhere/own.gitconfig"),
    )
    .unwrap();
    git.include_set(
        bisa_vcs::ConfigScope::Global,
        "hasconfig:remote.*.url:https://github.com/x/**",
        &dir.join("Not A Slug.gitconfig"),
    )
    .unwrap();
    git.include_set(
        bisa_vcs::ConfigScope::Global,
        "hasconfig:remote.*.url:https://github.com/gone/**",
        &dir.join("gone.gitconfig"),
    )
    .unwrap();
    gitprofiles::put(
        inner,
        Slug::new("odd").unwrap(),
        acme(None, None, "odd@acme.example"),
    )
    .await
    .unwrap();
    git.include_set(
        bisa_vcs::ConfigScope::Global,
        "hasconfig:remote.*.url:https://github.com/odd",
        &dir.join("odd.gitconfig"),
    )
    .unwrap();
    let view = gitprofiles::list(inner).await.unwrap();
    assert_eq!(
        view.profiles
            .iter()
            .map(|p| p.profile.slug.to_string())
            .collect::<Vec<_>>(),
        ["acme"],
        "the odd ones are passed over: {:?}",
        view.profiles
    );
    assert_eq!(
        view.foreign_includes
            .iter()
            .map(|f| f.condition.as_str())
            .collect::<Vec<_>>(),
        ["gitdir:/somewhere/"]
    );

    // A profile whose file is a folder cannot be removed as a file: said.
    std::fs::create_dir_all(dir.join("dirp.gitconfig").join("inside")).unwrap();
    let err = gitprofiles::remove(inner, Slug::new("dirp").unwrap())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("dirp.gitconfig"), "{err}");

    // The profiles folder nobody may write: a save is refused where it writes.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        let err = gitprofiles::put(
            inner,
            Slug::new("locked").unwrap(),
            acme(None, None, "locked@acme.example"),
        )
        .await
        .unwrap_err();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(err, EngineError::Vcs(_)), "{err}");
    }

    // A git older than 2.36 refuses before anything is written.
    let old = rig.dir.path().join("old-git");
    std::fs::write(
        &old,
        "#!/bin/sh\ncase \" $* \" in *' --version '*) echo 'git version 2.30.0';; *) exec git \"$@\";; esac\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&old, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let elder = Rig::start(
        vec![],
        None,
        Some(own_git(rig.dir.path()).with_program(&old)),
    );
    let err = gitprofiles::put(
        elder.inner(),
        Slug::new("late").unwrap(),
        acme(None, None, "late@acme.example"),
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("2.30.0"), "{err}");
    assert!(!elder
        .engine
        .workspace()
        .paths()
        .git_profiles_dir()
        .join("late.gitconfig")
        .exists());
}

// --- the pull request brief -----------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn a_long_branch_and_a_wide_diff_are_said_so_in_the_pull_request_brief() {
    let rig = Rig::start(vec![kinded(CodeHostKind::GitHub, "ada")], None, None);
    let inner = rig.inner();
    let origin = rig.bare("github.com", "main");
    let (project, root) = rig
        .project("web", origin.to_str().unwrap(), PublishPolicy::Auto)
        .await;
    raw_git(&root, &["push", "--quiet", "origin", "HEAD:main"]);
    let (wid, path) = rig.branch(&project, "feature/long").await;
    std::fs::write(path.join("wide.txt"), "x".repeat(60_000)).unwrap();
    raw_git(&path, &["add", "-A"]);
    raw_git(&path, &["commit", "--quiet", "-m", "wide"]);
    for n in 0..50 {
        raw_git(
            &path,
            &[
                "commit",
                "--quiet",
                "--allow-empty",
                "-m",
                &format!("step {n}"),
            ],
        );
    }
    let draft = projects::suggest_pull_request(inner, wid).await.unwrap();
    assert!(!draft.title.is_empty());
    let prompt = rig.prompts.lock().unwrap().last().cloned().unwrap();
    assert!(
        prompt.contains("older commits not listed"),
        "fifty is the budget"
    );
    assert!(prompt.contains("diff truncated"), "the diff was cut");
}
