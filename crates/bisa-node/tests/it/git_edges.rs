//! Git in the IDE over HTTP, at its edges (ide/04, ide/07): the consented
//! doors a person's own request opens — a branch made without switching, a
//! worktree's own branch renamed and the record following, a plain rebase,
//! a revert and a pick, remotes added and removed, a discard too big to be
//! a hunk, an amend on no branch and one a goal hears of, a preview of a
//! revision git cannot merge, a delete on the remote held by the gate; the
//! project doors where the folder is no repository, or a broken one, or
//! gone; a forced push with a lease where it is allowed and where it is
//! not; a copy's detail and its close; and the notes repository pulled and
//! described.
//!
//! Every repository is made by the test inside a `tempfile` directory and
//! dies with it; "origin" is a bare repository on this disk. The General
//! Agent is a mock echoing its prompt. The engine's `git` sees no config of
//! the machine's.

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
    _stop: tokio::sync::oneshot::Sender<()>,
}

fn isolated_git(dir: &Path) -> bisa_vcs::Git {
    bisa_vcs::Git::new()
        .with_env("GIT_CONFIG_GLOBAL", dir.join("no-global.gitconfig"))
        .with_env("GIT_CONFIG_NOSYSTEM", "1")
}

/// A node over a workspace with one managed project recorded as git — its
/// folder empty until a test makes it a repository — the General Agent on
/// a mock, and a sealed `git`.
async fn boot() -> Booted {
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
            ..Default::default()
        },
    )
    .expect("engine");
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
        _stop: stop,
    }
}

/// A request with the bearer in its header — which is what a consented
/// verb takes as the person's own click.
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

/// The project's folder made a repository on `main` with one commit and a
/// local identity, and a bare `origin` beside the workspace.
fn seed_repo(dir: &Path, root: &Path) -> PathBuf {
    raw_git(root, &["init", "--quiet", "-b", "main"]);
    raw_git(root, &["config", "user.name", "Bisa Test"]);
    raw_git(root, &["config", "user.email", "test@example.invalid"]);
    std::fs::write(root.join("a.txt"), "one\n").unwrap();
    raw_git(root, &["add", "-A"]);
    raw_git(root, &["commit", "-m", "first", "--quiet"]);
    let origin = dir.join("origin.git");
    std::fs::create_dir_all(&origin).unwrap();
    raw_git(
        &origin,
        &["init", "--bare", "--quiet", "--initial-branch", "main"],
    );
    raw_git(root, &["remote", "add", "origin", origin.to_str().unwrap()]);
    origin
}

fn commit_file(dir: &Path, name: &str, text: &str, message: &str) -> String {
    std::fs::write(dir.join(name), text).unwrap();
    raw_git(dir, &["add", "-A"]);
    raw_git(dir, &["commit", "--quiet", "-m", message]);
    raw_git(dir, &["rev-parse", "HEAD"])
}

/// A worktree workstream of the project on a new branch, for `goal` when
/// one is named.
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

#[tokio::test(flavor = "multi_thread")]
async fn the_interactive_doors_at_their_edges() {
    let b = boot().await;
    let (socket, pid) = (b.socket.clone(), b.pid.clone());
    let origin = seed_repo(b.dir.path(), &b.root);

    // A branch made without switching answers the branches alone.
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/branches"),
        Some(json!({"name": "feature/quiet", "switch": false})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert!(v["recovery"].is_null(), "nothing moved: {v}");
    assert!(v["branches"]
        .as_array()
        .unwrap()
        .iter()
        .any(|br| br["name"] == "feature/quiet"));

    // A worktree's own branch renamed: the record follows.
    let (wid, wt) = worktree(&b, "feature/x", None).await;
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{wid}/git/branches/feature%2Fx/rename"),
        Some(json!({"to": "feature/y"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let (_, shown) = request(&socket, "GET", &format!("/workstreams/{wid}"), None).await;
    assert_eq!(
        shown["workstream"]["kind"]["branch"],
        json!("feature/y"),
        "{shown}"
    );
    // Another branch renamed from the same checkout leaves the record alone.
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{wid}/git/branches/feature%2Fquiet/rename"),
        Some(json!({"to": "feature/quieter"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let (_, shown) = request(&socket, "GET", &format!("/workstreams/{wid}"), None).await;
    assert_eq!(
        shown["workstream"]["kind"]["branch"],
        json!("feature/y"),
        "{shown}"
    );

    // main moves on; the worktree is rebased onto it, plainly.
    commit_file(&b.root, "m.txt", "m\n", "on main");
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{wid}/git/rebase"),
        Some(json!({"upstream": "main"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert!(
        v["recovery"]["ref_name"]
            .as_str()
            .unwrap_or("")
            .contains("rebase"),
        "{v}"
    );
    assert!(
        wt.join("m.txt").is_file(),
        "the worktree stands on main now"
    );

    // A commit reverted with a new one, then picked back.
    let sha = commit_file(&wt, "r.txt", "r\n", "to revert");
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{wid}/git/revert"),
        Some(json!({"commits": [sha]})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert!(!wt.join("r.txt").exists());
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{wid}/git/cherry-pick"),
        Some(json!({"commits": [sha]})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert!(wt.join("r.txt").is_file());

    // Remotes: listed, one refused for no URL, one added, one removed.
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/remotes"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["remotes"].as_array().unwrap().len(), 1);
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/remotes"),
        Some(json!({"name": "upstream", "url": "  "})),
    )
    .await;
    assert_eq!(status, 400, "{v}");
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/remotes"),
        Some(json!({"name": "upstream", "url": origin.to_str().unwrap()})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["remotes"].as_array().unwrap().len(), 2);
    let (status, v) = request(
        &socket,
        "DELETE",
        &format!("/workstreams/{pid}/git/remotes/upstream"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert!(v["recovery"]["ref_name"].is_string(), "consented: {v}");

    // A discard too big to be a hunk never reaches git: the node's own body
    // bound stands in front of the engine's cap.
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/discard"),
        Some(json!({"patch": "x".repeat(bisa_engine::ide::git::MAX_PATCH_BYTES + 1)})),
    )
    .await;
    assert_eq!(status, 413, "{v}");

    // An amend on no branch is refused; one on a branch of a project a goal
    // has is heard by the goal.
    raw_git(&b.root, &["checkout", "--quiet", "--detach"]);
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/amend"),
        Some(json!({"message": "nowhere"})),
    )
    .await;
    assert_eq!(status, 400, "{v}");
    raw_git(&b.root, &["checkout", "--quiet", "main"]);
    let goal = a_goal(&b, "keep the branch tidy").await;
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/amend"),
        Some(json!({"message": "on main, amended"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(
        raw_git(&b.root, &["log", "-1", "--format=%s"]),
        "on main, amended"
    );

    // A preview of a revision git cannot merge is git's refusal.
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/merge-preview?source=no-such-branch"),
        None,
    )
    .await;
    assert!(status == 400 || status == 409, "{status} {v}");

    // A delete on the remote waits for the gate where the project asks first.
    let (status, v) = request(
        &socket,
        "PATCH",
        &format!("/projects/{pid}"),
        Some(json!({"publish": "gated"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let (gated, gated_wt) = worktree(&b, "feature/gated", Some(&goal)).await;
    commit_file(&gated_wt, "g.txt", "g\n", "gated work");
    raw_git(
        &gated_wt,
        &["push", "--quiet", "-u", "origin", "feature/gated"],
    );
    let (status, v) = request(
        &socket,
        "DELETE",
        &format!("/workstreams/{gated}/git/remote-branches/origin/feature%2Fgated"),
        None,
    )
    .await;
    assert_eq!(status, 202, "{v}");
    assert_eq!(v["deleted"], json!(false));
    assert!(v["gate"].is_string(), "{v}");
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{gated}/push-with-lease"),
        None,
    )
    .await;
    assert_eq!(status, 202, "a forced push waits for the gate too: {v}");
    assert_eq!(v["pushed"], json!(false));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_project_git_doors_at_their_edges() {
    let b = boot().await;
    let (socket, pid) = (b.socket.clone(), b.pid.clone());

    // Recorded as git, not yet a repository: said, never an error.
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/status"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["status"]["git"], json!(false));
    assert!(v["status"]["error"].is_string(), "{v}");
    for route in ["git/files", "git/diff?path=a.txt", "diff"] {
        let (status, v) =
            request(&socket, "GET", &format!("/workstreams/{pid}/{route}"), None).await;
        assert_eq!(status, 400, "{route}: {v}");
    }

    seed_repo(b.dir.path(), &b.root);
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/blame?path=a.txt&start=1&end=1"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["lines"].as_array().unwrap().len(), 1);
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/blame?path=a.txt&start=1"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(
        v["lines"].as_array().unwrap().len(),
        1,
        "start alone is one line"
    );

    for route in ["git/commit", "commit"] {
        let (status, v) = request(
            &socket,
            "POST",
            &format!("/workstreams/{pid}/{route}"),
            Some(json!({"message": "   "})),
        )
        .await;
        assert_eq!(status, 400, "{route}: {v}");
    }

    // A draft of what is staged, from the General Agent, who is a mock here.
    std::fs::write(
        b.root.join("drafted.txt"),
        "words
",
    )
    .unwrap();
    raw_git(&b.root, &["add", "drafted.txt"]);
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/message"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["suggested"], json!(true), "{v}");
    raw_git(&b.root, &["commit", "--quiet", "-m", "drafted"]);

    // A forced push with a lease: never the project's own tree, never its
    // default branch; a branch of its own on a project that publishes by
    // itself goes out with its recovery point.
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/push-with-lease"),
        None,
    )
    .await;
    assert_eq!(status, 400, "{v}");
    let (status, v) = request(
        &socket,
        "PATCH",
        &format!("/projects/{pid}"),
        Some(json!({"publish": "auto"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    raw_git(&b.root, &["push", "--quiet", "-u", "origin", "main"]);
    raw_git(&b.root, &["remote", "set-head", "origin", "main"]);
    raw_git(&b.root, &["checkout", "--quiet", "--detach"]);
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/projects/{pid}/workstreams"),
        Some(json!({"source": {"source": "local_branch", "name": "main"}})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let on_main = v["workstream"]["id"].as_str().unwrap().to_string();
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{on_main}/push-with-lease"),
        None,
    )
    .await;
    assert_eq!(status, 400, "the default branch is never forced: {v}");
    let (wid, wt) = worktree(&b, "feature/lease", None).await;
    commit_file(&wt, "l.txt", "l\n", "leased");
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{wid}/push-with-lease"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["pushed"], json!(true));
    assert!(v["recovery"]["ref_name"]
        .as_str()
        .unwrap_or("")
        .contains("push_with_lease"));

    // A checkout git can no longer read, then one that is gone.
    std::fs::write(wt.join(".git"), "gitdir: /nowhere/at/all\n").unwrap();
    for route in ["git/files", "diff"] {
        let (status, v) =
            request(&socket, "GET", &format!("/workstreams/{wid}/{route}"), None).await;
        assert_eq!(status, 400, "{route}: {v}");
    }
    std::fs::rename(&wt, wt.with_extension("moved-aside")).unwrap();
    for route in ["git/files", "diff"] {
        let (status, v) =
            request(&socket, "GET", &format!("/workstreams/{wid}/{route}"), None).await;
        assert_eq!(status, 404, "{route}: {v}");
    }

    // A repository whose index git cannot read: git, with its error.
    let (status, v) = request(
        &socket,
        "POST",
        "/projects",
        Some(json!({"kind": "new", "slug": "broken"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let broken = v["project"]["id"].as_str().unwrap().to_string();
    let broken_root = PathBuf::from(v["path"].as_str().unwrap());
    raw_git(&broken_root, &["init", "--quiet", "-b", "main"]);
    std::fs::write(broken_root.join(".git").join("index"), "not an index").unwrap();
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{broken}/git/status"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["status"]["git"], json!(true), "{v}");
    assert!(v["status"]["error"].is_string(), "{v}");

    // A copy — a plain folder's workstream — has no changes of git's, no
    // branch to force, and closes with its tree pinning nothing.
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("notes.md"), "plain\n").unwrap();
    let (status, v) = request(
        &socket,
        "POST",
        "/projects",
        Some(json!({"kind": "adopt", "slug": "legacy", "path": outside.path().display().to_string()})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let legacy = v["project"]["id"].as_str().unwrap().to_string();
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/projects/{legacy}/workstreams"),
        Some(json!({"source": {"source": "new_branch"}})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let copy = v["workstream"]["id"].as_str().unwrap().to_string();
    assert_eq!(v["workstream"]["kind"]["kind"], json!("copy"), "{v}");
    let (status, v) = request(&socket, "GET", &format!("/workstreams/{copy}"), None).await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["changes"], json!([]));
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{copy}/push-with-lease"),
        None,
    )
    .await;
    assert_eq!(status, 400, "{v}");
    let (status, v) = request(
        &socket,
        "DELETE",
        &format!("/workstreams/{copy}?tree=true"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert!(
        v["recovery"].is_null(),
        "a copy has no repository to pin: {v}"
    );
    assert_eq!(v["removed_tree"], json!(true));

    // A repository made by hand under a record that says none: the git
    // doors read it as git does, and a branch rename or a delete on its
    // remote has no default branch to protect — the policy answers.
    raw_git(outside.path(), &["init", "--quiet", "-b", "main"]);
    raw_git(outside.path(), &["config", "user.name", "Bisa Test"]);
    raw_git(
        outside.path(),
        &["config", "user.email", "test@example.invalid"],
    );
    commit_file(outside.path(), "notes.md", "plain\n", "first");
    let far = b.dir.path().join("far.git");
    std::fs::create_dir_all(&far).unwrap();
    raw_git(
        &far,
        &["init", "--bare", "--quiet", "--initial-branch", "main"],
    );
    raw_git(
        outside.path(),
        &["remote", "add", "origin", far.to_str().unwrap()],
    );
    raw_git(outside.path(), &["push", "--quiet", "-u", "origin", "main"]);
    raw_git(outside.path(), &["branch", "side"]);
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{legacy}/git/branches/side/rename"),
        Some(json!({"to": "aside"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let (status, v) = request(
        &socket,
        "DELETE",
        &format!("/workstreams/{legacy}/git/remote-branches/origin/main"),
        None,
    )
    .await;
    assert!(
        status == 400 || status == 409,
        "the policy, not a default branch: {status} {v}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_notes_repository_is_pulled_and_described() {
    let b = boot().await;
    let socket = b.socket.clone();
    let (status, v) = request(
        &socket,
        "POST",
        "/notes",
        Some(json!({"title": "First", "body": "words", "scope": "workspace"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let (status, v) = request(
        &socket,
        "PUT",
        "/notes/git/identity",
        Some(json!({"name": "Ada", "email": "ada@example.com"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");

    // A draft from the General Agent, who is a mock here.
    let (status, v) = request(&socket, "POST", "/notes/git/message", None).await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["suggested"], json!(true), "{v}");
    assert!(v["message"].as_str().is_some_and(|m| !m.is_empty()));

    let (status, v) = request(
        &socket,
        "POST",
        "/notes/git/commit",
        Some(json!({"message": "Add the first note"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let origin = b.dir.path().join("notes-origin.git");
    std::fs::create_dir_all(&origin).unwrap();
    raw_git(&origin, &["init", "--bare", "--quiet"]);
    let (status, v) = request(
        &socket,
        "PUT",
        "/notes/git/remote",
        Some(json!({"url": origin.to_str().unwrap()})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let (status, v) = request(&socket, "POST", "/notes/git/push", None).await;
    assert_eq!(status, 200, "{v}");

    // A pull is consented, pins what was here first, and says where it stands.
    let (status, v) = request(&socket, "POST", "/notes/git/pull", None).await;
    assert_eq!(status, 200, "{v}");
    assert!(v["recovery"]["ref_name"].is_string(), "{v}");
    assert_eq!(
        v["pull"]["moved"],
        json!(false),
        "nothing new at origin: {v}"
    );
    assert_eq!(v["status"]["behind"], json!(0));
}
