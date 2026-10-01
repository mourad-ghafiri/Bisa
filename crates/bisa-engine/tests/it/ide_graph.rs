//! The commit graph over a real repository (ide/05): a branch that forks and
//! merges back lays out in two lanes; the cache answers until a ref moves.

use bisa_engine::ide::graph::{self, EdgeKind};
use bisa_engine::{Engine, EngineConfig};
use bisa_harness::HarnessCatalog;
use bisa_store::{FileScope, MemoryKeyStore, NewProject, Workspace};
use bisa_vcs::git;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

fn raw_git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("spawn git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn engine(dir: &tempfile::TempDir) -> (Engine, String, std::path::PathBuf) {
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    let project = ws
        .create_project(NewProject::managed("web").unwrap())
        .unwrap();
    let root = ws.project_root_path(&project);
    std::fs::create_dir_all(&root).unwrap();
    let engine = Engine::start(
        ws,
        HarnessCatalog::new(),
        EngineConfig {
            design_enabled: false,
            events_enabled: false,
            ..Default::default()
        },
    )
    .unwrap();
    (engine, project.id.to_string(), root)
}

/// The one filter the History view offers: `head` walks only what HEAD
/// reaches, so a branch nobody merged is left out; the two scopes are two
/// layouts cached apart, and the search honours the scope it is asked with.
#[tokio::test(flavor = "multi_thread")]
async fn only_the_current_branch_leaves_the_other_branches_commits_out() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, pid, root) = engine(&dir);
    let inner = engine.inner();
    git::init(&root).unwrap();
    raw_git(&root, &["config", "user.name", "Bisa Test"]);
    raw_git(&root, &["config", "user.email", "test@example.invalid"]);
    std::fs::write(root.join("a.txt"), "one\n").unwrap();
    git::add_all(&root).unwrap();
    git::commit(&root, "first", false).unwrap();
    let side = dir.path().join("side-wt");
    let base = git::default_branch(&root).unwrap();
    git::worktree_add(&root, &side, "side", &base).unwrap();
    std::fs::write(side.join("s.txt"), "side\n").unwrap();
    git::add_all(&side).unwrap();
    git::commit(&side, "only on side", false).unwrap();
    std::fs::write(root.join("a.txt"), "one\ntwo\n").unwrap();
    git::add_all(&root).unwrap();
    git::commit(&root, "main again", false).unwrap();

    let all = graph::window(
        inner,
        FileScope::Workstream,
        &pid,
        graph::RefScope::All,
        0,
        100,
        false,
    )
    .await
    .unwrap();
    assert_eq!(all.total, 3, "{all:#?}");
    let head = graph::window(
        inner,
        FileScope::Workstream,
        &pid,
        graph::RefScope::Head,
        0,
        100,
        false,
    )
    .await
    .unwrap();
    assert_eq!(head.total, 2, "{head:#?}");
    assert!(head.rows.iter().all(|r| r.subject != "only on side"));
    assert_eq!(head.rows[0].subject, "main again");

    let found = graph::search(
        inner,
        FileScope::Workstream,
        &pid,
        graph::RefScope::Head,
        "side",
        0,
        10,
    )
    .await
    .unwrap();
    assert!(found.indices.is_empty(), "{found:?}");
    let found = graph::search(
        inner,
        FileScope::Workstream,
        &pid,
        graph::RefScope::All,
        "side",
        0,
        10,
    )
    .await
    .unwrap();
    assert_eq!(found.indices.len(), 1);
    assert_eq!(
        "head".parse::<graph::RefScope>().unwrap(),
        graph::RefScope::Head
    );
    assert!("mine".parse::<graph::RefScope>().is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_merge_lays_out_in_two_lanes_and_the_cache_follows_the_refs() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, pid, root) = engine(&dir);
    let inner = engine.inner();

    // An empty repository has a graph with no rows, not an error.
    git::init(&root).unwrap();
    raw_git(&root, &["config", "user.name", "Bisa Test"]);
    raw_git(&root, &["config", "user.email", "test@example.invalid"]);
    let w = graph::window(
        inner,
        FileScope::Workstream,
        &pid,
        graph::RefScope::All,
        0,
        100,
        false,
    )
    .await
    .unwrap();
    assert_eq!(w.total, 0);
    assert!(w.done);

    std::fs::write(root.join("a.txt"), "one\n").unwrap();
    git::add_all(&root).unwrap();
    let c1 = git::commit(&root, "first", false).unwrap();
    // A feature branch in its own worktree — no checkout in the fixture.
    let feature = dir.path().join("feature-wt");
    let base = git::default_branch(&root).unwrap();
    git::worktree_add(&root, &feature, "feature", &base).unwrap();
    std::fs::write(feature.join("b.txt"), "two\n").unwrap();
    git::add_all(&feature).unwrap();
    let c2 = git::commit(&feature, "feature work", false).unwrap();
    std::fs::write(root.join("a.txt"), "one\nmore\n").unwrap();
    git::add_all(&root).unwrap();
    let c3 = git::commit(&root, "main work", false).unwrap();
    raw_git(
        &root,
        &["merge", "--no-ff", "-m", "merge feature", "feature"],
    );
    let c4 = git::head(&root).unwrap();

    // Refs moved: the first request lays out inline (four commits < the first screen).
    let w = graph::window(
        inner,
        FileScope::Workstream,
        &pid,
        graph::RefScope::All,
        0,
        100,
        false,
    )
    .await
    .unwrap();
    assert_eq!(w.total, 4, "{w:#?}");
    assert!(w.done);
    assert!(!w.stale);
    let rows = &w.rows;
    assert_eq!(rows[0].id, c4);
    assert_eq!(rows[0].lane, 0);
    assert_eq!(rows[0].parents, 2);
    assert!(
        rows[0]
            .edges
            .iter()
            .any(|e| e.kind == EdgeKind::Fork && e.to == 1),
        "{rows:#?}"
    );
    assert_eq!(rows[0].passing, vec![0, 1]);
    // Topological order puts both middle commits before the root; whichever
    // comes first, they sit in different lanes and the root merges them.
    let middle: Vec<_> = rows[1..3].iter().map(|r| (r.id.clone(), r.lane)).collect();
    assert!(middle.iter().any(|(id, _)| *id == c2));
    assert!(middle.iter().any(|(id, _)| *id == c3));
    assert_ne!(middle[0].1, middle[1].1);
    assert_eq!(rows[3].id, c1);
    assert_eq!(rows[3].lane, 0);
    assert!(
        rows[3].edges.iter().any(|e| e.kind == EdgeKind::Merge),
        "{rows:#?}"
    );
    assert!(rows[3].passing.is_empty());
    // Decorations came through.
    assert!(rows[0].refs.iter().any(|r| r.kind == "head"));
    assert!(rows[0]
        .refs
        .iter()
        .any(|r| r.name == base && r.kind == "branch"));

    // Windows are honest about their bounds.
    let w = graph::window(
        inner,
        FileScope::Workstream,
        &pid,
        graph::RefScope::All,
        3,
        10,
        false,
    )
    .await
    .unwrap();
    assert_eq!(w.from, 3);
    assert_eq!(w.rows.len(), 1);
    let w = graph::window(
        inner,
        FileScope::Workstream,
        &pid,
        graph::RefScope::All,
        99,
        10,
        false,
    )
    .await
    .unwrap();
    assert!(w.rows.is_empty());

    // A new commit changes the fingerprint: the stale rows are served once
    // while the relayout runs, and the next requests see five rows.
    std::fs::write(root.join("c.txt"), "three\n").unwrap();
    git::add_all(&root).unwrap();
    git::commit(&root, "fifth", false).unwrap();
    let w = graph::window(
        inner,
        FileScope::Workstream,
        &pid,
        graph::RefScope::All,
        0,
        100,
        false,
    )
    .await
    .unwrap();
    assert!(w.stale || w.total == 5, "{w:#?}");
    let mut seen = w.total;
    for _ in 0..50 {
        if seen == 5 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        seen = graph::window(
            inner,
            FileScope::Workstream,
            &pid,
            graph::RefScope::All,
            0,
            100,
            false,
        )
        .await
        .unwrap()
        .total;
    }
    assert_eq!(seen, 5, "the relayout landed");

    // The inspector: files against the first parent, the patch, the refs.
    let (detail, diff, truncated) =
        bisa_engine::ide::git::commit(inner, pid.parse().unwrap(), c4.to_string())
            .await
            .unwrap();
    assert_eq!(detail.subject, "merge feature");
    assert_eq!(detail.parents.len(), 2);
    assert_eq!(
        detail
            .files
            .iter()
            .map(|f| f.path.display().to_string())
            .collect::<Vec<_>>(),
        vec!["b.txt"],
        "against the first parent, the merge brought b.txt"
    );
    assert!(diff.contains("+two"), "{diff}");
    assert!(!truncated);
    let (root_detail, root_diff, _) =
        bisa_engine::ide::git::commit(inner, pid.parse().unwrap(), c1.to_string())
            .await
            .unwrap();
    assert_eq!(
        root_detail.files.len(),
        1,
        "a root commit diffs against the empty tree"
    );
    assert!(root_diff.contains("+one"));
    assert!(
        bisa_engine::ide::git::commit(inner, pid.parse().unwrap(), "not a sha".into())
            .await
            .is_err()
    );

    // One file of a commit, in the three views' two shapes (ide/05): the
    // merge's b.txt as a patch of its own, and as two sides — nothing at the
    // first parent, the file at the commit.
    let (file_diff, cut) = bisa_engine::ide::git::commit_file(
        inner,
        pid.parse().unwrap(),
        c4.to_string(),
        "b.txt".into(),
    )
    .await
    .unwrap();
    assert!(
        file_diff.contains("+two") && !file_diff.contains("a.txt"),
        "{file_diff}"
    );
    assert!(!cut);
    let sides = bisa_engine::ide::git::commit_file_sides(
        inner,
        pid.parse().unwrap(),
        c4.to_string(),
        "b.txt".into(),
    )
    .await
    .unwrap();
    assert_eq!(sides.original.text, None, "the first parent had no b.txt");
    assert_eq!(sides.modified.text.as_deref(), Some("two\n"));
    // A modified file reads both sides; the root commit's file has no left side.
    let sides = bisa_engine::ide::git::commit_file_sides(
        inner,
        pid.parse().unwrap(),
        c3.to_string(),
        "a.txt".into(),
    )
    .await
    .unwrap();
    assert_eq!(sides.original.text.as_deref(), Some("one\n"));
    assert_eq!(sides.modified.text.as_deref(), Some("one\nmore\n"));
    let sides = bisa_engine::ide::git::commit_file_sides(
        inner,
        pid.parse().unwrap(),
        c1.to_string(),
        "a.txt".into(),
    )
    .await
    .unwrap();
    assert_eq!(sides.original, bisa_engine::ide::git::SideText::default());
    assert!(
        bisa_engine::ide::git::commit_file(
            inner,
            pid.parse().unwrap(),
            c4.to_string(),
            "a.txt".into()
        )
        .await
        .is_err(),
        "a path the commit did not touch is refused"
    );
}
