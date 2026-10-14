//! Pull requests and accounts over HTTP, at their edges (ide/08): what the
//! code host behind a project can do, its open pull requests, a request with
//! no title, a draft the General Agent suggests, a pull request opened and
//! its checks, a review, a thread resolved, a merge landed and one held by
//! the gate, an account added under the login the host answers, and the
//! identity a repository nobody commits in is offered from its account.
//!
//! The code host is the in-memory fake, standing for GitHub and claiming a
//! bare repository on this disk by its name; the General Agent is a mock
//! echoing its prompt; the engine's git sees no config of the machine's.

use bisa_codehost::fake::FakeCodeHost;
use bisa_codehost::{CodeHostCapabilities, CodeHostKind, MergeStrategy, ReviewEvent};
use bisa_core::AgentId;
use bisa_engine::{Engine, EngineConfig};
use bisa_harness::mock::MockAdapter;
use bisa_harness::HarnessCatalog;
use bisa_node::{serve, NodeConfig};
use bisa_store::{FileKeyStore, NewProject, Paths, Workspace};
use http_body_util::{BodyExt as _, Full};
use hyper::body::Bytes;
use hyper::Request;
use hyper_util::rt::TokioIo;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UnixStream;

const TOKEN: &str = "test-token-0123456789abcdef";

struct Booted {
    dir: tempfile::TempDir,
    socket: PathBuf,
    pid: String,
    root: PathBuf,
    ws: Arc<Workspace>,
    _stop: tokio::sync::oneshot::Sender<()>,
}

fn isolated_git(dir: &Path) -> bisa_vcs::Git {
    bisa_vcs::Git::new()
        .with_env("GIT_CONFIG_GLOBAL", dir.join("no-global.gitconfig"))
        .with_env("GIT_CONFIG_NOSYSTEM", "1")
}

/// The fake standing for GitHub, with every capability the doors need,
/// signed in as `you` — a person the host knows by name and address.
fn github_fake() -> Arc<FakeCodeHost> {
    Arc::new(FakeCodeHost {
        kind: Some(CodeHostKind::GitHub),
        ..FakeCodeHost::with_capabilities(
            "github.com",
            CodeHostCapabilities {
                draft_prs: true,
                reviewers: true,
                labels: true,
                merge_strategies: vec![MergeStrategy::Merge],
                check_runs: true,
                review_comments: true,
                review_threads: true,
                review_thread_replies: true,
                delete_branch: true,
                review_events: vec![ReviewEvent::Approve, ReviewEvent::Comment],
            },
        )
        .signed_in_as("you")
        .known_as("You Person", Some("you@example.test"), Some(9))
    })
}

/// A node over a workspace with one managed project recorded as git, the
/// General Agent on a mock, a sealed `git`, and `hosts` as the code hosts.
async fn boot(hosts: Vec<Arc<FakeCodeHost>>) -> Booted {
    let dir = tempfile::tempdir().expect("tempdir");
    let data = dir.path().to_path_buf();
    let ws = Workspace::open_with_keystore(
        &data,
        Box::new(FileKeyStore::new(Paths::new(&data).identity_dir())),
    )
    .expect("workspace");
    let project = ws
        .create_project(NewProject::managed("web").unwrap())
        .unwrap();
    let project = ws
        .update_project(bisa_core::Project {
            vcs: bisa_core::Vcs::Git {
                default_branch: "main".into(),
                remote: None,
                code_host: None,
            },
            ..project
        })
        .unwrap();
    let root = ws.project_root_path(&project);
    std::fs::create_dir_all(&root).unwrap();
    let pid = project.id.to_string();
    let adapter = MockAdapter {
        id: "mock".into(),
        ..Default::default()
    };
    let mut general = ws.get_agent(&AgentId::general()).unwrap();
    general.harness = adapter.id.clone();
    ws.update_agent(general).unwrap();
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::new(adapter) as Arc<_>);
    let engine = Engine::start(
        ws,
        catalog,
        EngineConfig {
            design_enabled: false,
            git: Some(isolated_git(dir.path())),
            code_hosts: Some(bisa_codehost::CodeHostRegistry::new(
                hosts
                    .into_iter()
                    .map(|h| h as Arc<dyn bisa_codehost::CodeHost>)
                    .collect(),
            )),
            ..Default::default()
        },
    )
    .expect("engine");
    let ws = Arc::clone(&engine.inner().ws);
    let socket = Paths::new(&data).node_socket();
    let (stop, stop_rx) = tokio::sync::oneshot::channel::<()>();
    let cfg = NodeConfig {
        socket: socket.clone(),
        http: None,
        data_dir: data,
        collab: None,
        fetch_attachment: None,
        #[cfg(feature = "a2a")]
        a2a: None,
        token: Some(TOKEN.to_string()),
    };
    tokio::spawn(serve(engine, cfg, async {
        let _stopped_or_dropped = stop_rx.await;
    }));
    let mut actual = socket.clone();
    for _ in 0..50 {
        if actual.exists() {
            break;
        }
        if let Ok(p) = std::fs::read_to_string(bisa_node::pointer_path(&socket)) {
            actual = PathBuf::from(p.trim());
            if actual.exists() {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(actual.exists(), "node socket never appeared");
    Booted {
        dir,
        socket: actual,
        pid,
        root,
        ws,
        _stop: stop,
    }
}

async fn request(socket: &Path, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
    let stream = UnixStream::connect(socket).await.expect("connect");
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .expect("handshake");
    tokio::spawn(conn);
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header(hyper::header::HOST, "localhost")
        .header(hyper::header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header(hyper::header::CONTENT_TYPE, "application/json")
        .body(Full::new(Bytes::from(
            body.map(|b| b.to_string()).unwrap_or_default(),
        )))
        .unwrap();
    let resp = sender.send_request(request).await.expect("request");
    let status = resp.status().as_u16();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
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

/// The project's folder made a repository on `main` with one commit — under
/// a local identity, or under none when `identity` is false — and a bare
/// `origin` named for the fake's host, so the fake claims it.
fn seed_repo(dir: &Path, root: &Path, identity: bool) -> PathBuf {
    raw_git(root, &["init", "--quiet", "-b", "main"]);
    if identity {
        raw_git(root, &["config", "user.name", "Bisa Test"]);
        raw_git(root, &["config", "user.email", "test@example.invalid"]);
    }
    std::fs::write(root.join("a.txt"), "one\n").unwrap();
    raw_git(root, &["add", "-A"]);
    raw_git(
        root,
        &[
            "-c",
            "user.name=Nobody In Particular",
            "-c",
            "user.email=nobody@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "first",
        ],
    );
    let origin = dir.join("github.com.git");
    std::fs::create_dir_all(&origin).unwrap();
    raw_git(
        &origin,
        &["init", "--bare", "--quiet", "--initial-branch", "main"],
    );
    raw_git(root, &["remote", "add", "origin", origin.to_str().unwrap()]);
    raw_git(root, &["push", "--quiet", "origin", "HEAD:main"]);
    origin
}

fn commit_file(dir: &Path, name: &str, text: &str, message: &str) -> String {
    std::fs::write(dir.join(name), text).unwrap();
    raw_git(dir, &["add", "-A"]);
    raw_git(dir, &["commit", "--quiet", "-m", message]);
    raw_git(dir, &["rev-parse", "HEAD"])
}

async fn worktree(b: &Booted, branch: &str, goal: Option<&str>) -> (String, PathBuf) {
    let mut body = json!({"source": {"source": "new_branch", "name": branch}});
    if let Some(goal) = goal {
        body["goal"] = json!(goal);
    }
    let (status, v) = request(
        &b.socket,
        "POST",
        &format!("/projects/{}/workstreams", b.pid),
        Some(body),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    (
        v["workstream"]["id"].as_str().unwrap().to_string(),
        PathBuf::from(v["path"].as_str().unwrap()),
    )
}

async fn a_goal(b: &Booted, statement: &str) -> String {
    let (status, v) = request(
        &b.socket,
        "POST",
        "/goals",
        Some(json!({"statement": statement, "title": "T"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let goal = v["goal"]["id"].as_str().unwrap().to_string();
    let (status, v) = request(
        &b.socket,
        "POST",
        &format!("/goals/{goal}/projects"),
        Some(json!({"kind": "attach", "project": b.pid})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    goal
}

async fn set_publish(b: &Booted, policy: &str) {
    let (status, v) = request(
        &b.socket,
        "PATCH",
        &format!("/projects/{}", b.pid),
        Some(json!({"publish": policy})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_pull_request_doors_over_http_at_their_edges() {
    let fake = github_fake();
    fake.state
        .threads
        .lock()
        .unwrap()
        .push(bisa_codehost::ReviewThread {
            id: "T1".into(),
            path: Some("a.txt".into()),
            line: Some(1),
            is_resolved: false,
            is_outdated: false,
            comments: vec![],
        });
    let b = boot(vec![fake.clone()]).await;
    let (socket, pid) = (b.socket.clone(), b.pid.clone());
    seed_repo(b.dir.path(), &b.root, true);
    set_publish(&b, "auto").await;

    // What the host behind the project can do, and what is open on it.
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/codehost/capabilities/{pid}"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["code_host"], json!("github"), "{v}");
    assert_eq!(v["repo"]["owner"], json!("local"));
    assert_eq!(v["capabilities"]["delete_branch"], json!(true));
    let (status, v) = request(&socket, "GET", &format!("/projects/{pid}/prs"), None).await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["prs"], json!([]));

    // A request with no title is refused before anything is asked; a draft
    // is what the General Agent suggests; the pull request opens.
    let (wid, wt) = worktree(&b, "feature/x", None).await;
    commit_file(&wt, "x.txt", "x\n", "the work");
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{wid}/pr"),
        Some(json!({"title": "   "})),
    )
    .await;
    assert_eq!(status, 400, "{v}");
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{wid}/pr/suggest"),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["suggested"], json!(true), "{v}");
    assert!(v["error"].is_null() && v["title"].as_str().is_some_and(|t| !t.is_empty()));
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{wid}/pr"),
        Some(json!({"title": "Add x", "body": "the work", "labels": ["ready"]})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["opened"], json!(true), "{v}");
    assert_eq!(v["pr"]["number"], json!(1));
    assert_eq!(v["workstream"]["state"]["state"], json!("pr_open"));

    // Its checks, a review, a thread resolved, the merge.
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{wid}/pr/checks"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["checks"], json!([]));
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{wid}/pr/review"),
        Some(json!({"event": "comment", "body": "fine by me"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["reviewed"], json!(true));
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{wid}/pr/threads/T1/resolve"),
        Some(json!({"resolved": true})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(
        (v["thread"].as_str(), v["resolved"].as_bool()),
        (Some("T1"), Some(true))
    );
    assert!(fake.state.threads.lock().unwrap()[0].is_resolved);
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{wid}/pr/merge"),
        Some(json!({"strategy": "merge", "delete_branch": true})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["merged"], json!(true), "{v}");
    assert_eq!(v["remote_branch_deleted"], json!(true));
    assert_eq!(
        fake.state.deleted_branches.lock().unwrap().as_slice(),
        ["feature/x"]
    );

    // An account added: verified with the host, kept under the login it answers.
    let (status, v) = request(
        &socket,
        "PUT",
        "/codehost/github/accounts",
        Some(json!({"token": "a-word-for-the-test"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["login"], json!("you"));
    assert_eq!(v["connection"]["state"], json!("connected"));
    let (status, v) = request(&socket, "GET", "/codehost/github/accounts", None).await;
    assert_eq!(status, 200);
    assert_eq!(v["accounts"][0]["login"], json!("you"));

    // A merge on a project that asks first waits for the gate.
    let goal = a_goal(&b, "ship it").await;
    let (gated, gated_wt) = worktree(&b, "feature/g", Some(&goal)).await;
    commit_file(&gated_wt, "g.txt", "g\n", "gated work");
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{gated}/pr"),
        Some(json!({"title": "Add g"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    set_publish(&b, "gated").await;
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{gated}/pr/merge"),
        Some(json!({"strategy": "merge"})),
    )
    .await;
    assert_eq!(status, 202, "{v}");
    assert_eq!(v["merged"], json!(false));
    assert_eq!(v["status"], json!("awaiting_publish_gate"));
    assert!(v["gate"].is_string(), "{v}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_repository_nobody_commits_in_is_offered_its_accounts_identity_over_http() {
    let b = boot(vec![github_fake()]).await;
    let (socket, pid) = (b.socket.clone(), b.pid.clone());
    // The account chain is the remote's host's: a URL on github.com, which
    // the fake claims by name, and which nothing here ever reaches.
    seed_repo(b.dir.path(), &b.root, false);
    raw_git(
        &b.root,
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/acme/web.git",
        ],
    );
    bisa_codehost::creds::TokenStore::file_only(
        CodeHostKind::GitHub,
        "github.com",
        b.ws.paths().codehost_tokens_root().join("github"),
    )
    .store("you", "a-word-for-the-test")
    .unwrap();
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/identity"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["source"], json!("none"), "{v}");
    assert_eq!(
        v["suggested"],
        json!({"name": "You Person", "email": "you@example.test", "login": "you"}),
        "the one stored account's record, as the host says it: {v}"
    );
}
