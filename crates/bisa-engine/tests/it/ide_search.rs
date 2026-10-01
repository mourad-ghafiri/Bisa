//! Content search and replace (ide/12): ripgrep's crates, bounded, and a
//! replace that is a preview and then one compare-and-swap write per file.

use bisa_engine::ide::search::{
    apply_replace, preview_replace, search, CaseMode, ReplaceOutcome, SearchHit, SearchQuery,
};
use bisa_engine::{Engine, EngineConfig};
use bisa_harness::HarnessCatalog;
use bisa_store::{FileScope, MemoryKeyStore, NewProject, Workspace};

fn engine(dir: &tempfile::TempDir) -> (Engine, String, std::path::PathBuf) {
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    let project = ws
        .create_project(NewProject::managed("web").unwrap())
        .unwrap();
    let root = ws.project_root_path(&project);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("target")).unwrap();
    std::fs::write(
        root.join("src/main.rs"),
        "fn main() {\n    let total = cart_total();\n    println!(\"{total}\");\n}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn cart_total() -> u32 { 42 }\npub fn Total() {}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("README.md"),
        "# Cart\n\nThe total is computed by cart_total.\n",
    )
    .unwrap();
    std::fs::write(root.join("target/junk.rs"), "cart_total everywhere\n").unwrap();
    std::fs::write(root.join(".gitignore"), "target/\n").unwrap();
    std::fs::write(root.join("blob.bin"), [b'c', b'a', b'r', b't', 0, b'x']).unwrap();
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

fn query(q: &str) -> SearchQuery {
    SearchQuery {
        q: q.into(),
        regex: false,
        case: CaseMode::Smart,
        word: false,
        include: vec![],
        exclude: vec![],
        limit: None,
    }
}

fn run(
    engine: &Engine,
    pid: &str,
    q: &SearchQuery,
) -> (Vec<SearchHit>, bisa_engine::ide::search::SearchSummary) {
    let mut hits = Vec::new();
    let summary = search(engine.inner(), FileScope::Workstream, pid, q, |h| {
        hits.push(h);
        true
    })
    .unwrap();
    hits.sort_by(|a, b| (&a.path, a.line).cmp(&(&b.path, b.line)));
    (hits, summary)
}

#[tokio::test(flavor = "multi_thread")]
async fn literal_search_honours_gitignore_skips_binaries_and_carries_context() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, pid, _root) = engine(&dir);
    let (hits, summary) = run(&engine, &pid, &query("cart_total"));
    let paths: Vec<&str> = hits.iter().map(|h| h.path.as_str()).collect();
    assert_eq!(
        paths,
        vec!["README.md", "src/lib.rs", "src/main.rs"],
        "{hits:?}"
    );
    assert!(!paths.contains(&"target/junk.rs"), ".gitignore is honoured");
    assert!(
        !paths.contains(&"blob.bin"),
        "a NUL byte ends a file's scan"
    );
    let main = hits.iter().find(|h| h.path == "src/main.rs").unwrap();
    assert_eq!(main.line, 2);
    assert_eq!(main.column, 17);
    assert_eq!(main.before.as_deref(), Some("fn main() {"));
    assert_eq!(main.after.as_deref(), Some("    println!(\"{total}\");"));
    assert_eq!(summary.matches, 3);
    assert_eq!(summary.files_with_matches, 3);
    assert!(!summary.truncated);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn case_word_regex_globs_and_the_limit() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, pid, _root) = engine(&dir);
    // Smart case: an upper-case letter in the query makes it sensitive.
    let (hits, _) = run(&engine, &pid, &query("Total"));
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].path, "src/lib.rs");
    let mut q = query("Total");
    q.case = CaseMode::Insensitive;
    let (hits, _) = run(&engine, &pid, &q);
    assert!(hits.len() >= 4, "{hits:?}");
    // Whole word: `total` alone, not inside `cart_total`.
    let mut q = query("total");
    q.word = true;
    q.case = CaseMode::Sensitive;
    let (hits, _) = run(&engine, &pid, &q);
    assert_eq!(
        hits.iter()
            .map(|h| (h.path.as_str(), h.line))
            .collect::<Vec<_>>(),
        vec![("README.md", 3), ("src/main.rs", 2), ("src/main.rs", 3)]
    );
    // A literal query is literal; a regex query is a regex.
    let (hits, _) = run(&engine, &pid, &query("fn .*_total"));
    assert!(hits.is_empty());
    let mut q = query("fn .*_total");
    q.regex = true;
    let (hits, _) = run(&engine, &pid, &q);
    assert_eq!(hits.len(), 1);
    let mut q = query("[");
    q.regex = true;
    assert!(
        search(engine.inner(), FileScope::Workstream, &pid, &q, |_| true).is_err(),
        "a bad regex is a refusal, not a panic"
    );
    // Globs.
    let mut q = query("cart_total");
    q.include = vec!["*.md".into()];
    let (hits, _) = run(&engine, &pid, &q);
    assert_eq!(
        hits.iter().map(|h| h.path.as_str()).collect::<Vec<_>>(),
        vec!["README.md"]
    );
    let mut q = query("cart_total");
    q.exclude = vec!["src/**".into()];
    let (hits, _) = run(&engine, &pid, &q);
    assert_eq!(
        hits.iter().map(|h| h.path.as_str()).collect::<Vec<_>>(),
        vec!["README.md"]
    );
    // The limit truncates and says so.
    let mut q = query("cart_total");
    q.limit = Some(2);
    let (hits, summary) = run(&engine, &pid, &q);
    assert_eq!(hits.len(), 2);
    assert!(summary.truncated);
    // The sink can stop the scan.
    let mut seen = 0;
    search(
        engine.inner(),
        FileScope::Workstream,
        &pid,
        &query("cart_total"),
        |_| {
            seen += 1;
            false
        },
    )
    .unwrap();
    assert_eq!(seen, 1);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn replace_previews_then_writes_by_compare_and_swap() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, pid, root) = engine(&dir);
    let inner = engine.inner();
    let q = query("cart_total");
    let previews = preview_replace(inner, FileScope::Workstream, &pid, &q, "basket_total").unwrap();
    let mut paths: Vec<&str> = previews.iter().map(|p| p.path.as_str()).collect();
    paths.sort();
    assert_eq!(paths, vec!["README.md", "src/lib.rs", "src/main.rs"]);
    let main = previews.iter().find(|p| p.path == "src/main.rs").unwrap();
    assert_eq!(main.changes.len(), 1);
    assert_eq!(main.changes[0].line, 2);
    assert_eq!(main.changes[0].after, "    let total = basket_total();");
    // Nothing was written by a preview.
    assert!(std::fs::read_to_string(root.join("src/main.rs"))
        .unwrap()
        .contains("cart_total"));

    // An agent edits one file between preview and apply.
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn cart_total() -> u32 { 43 }\n",
    )
    .unwrap();
    let files: Vec<(String, String)> = previews
        .iter()
        .map(|p| (p.path.clone(), p.base_hash.clone()))
        .collect();
    let outcomes = apply_replace(
        inner,
        FileScope::Workstream,
        &pid,
        &q,
        "basket_total",
        &files,
    )
    .unwrap();
    let skipped: Vec<&str> = outcomes
        .iter()
        .filter_map(|o| match o {
            ReplaceOutcome::SkippedChanged { path, .. } => Some(path.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(skipped, vec!["src/lib.rs"], "{outcomes:?}");
    let applied: Vec<&str> = outcomes
        .iter()
        .filter_map(|o| match o {
            ReplaceOutcome::Applied { path, changes, .. } if *changes > 0 => Some(path.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(applied.len(), 2);
    assert!(std::fs::read_to_string(root.join("src/main.rs"))
        .unwrap()
        .contains("basket_total()"));
    assert_eq!(
        std::fs::read_to_string(root.join("src/lib.rs")).unwrap(),
        "pub fn cart_total() -> u32 { 43 }\n",
        "the file that moved on was not clobbered"
    );
    engine.shutdown().await;
}

/// The files an apply names are contained like every other path: one that
/// leaves the root is neither read nor hashed — its outcome is a refusal
/// carrying no hash — and one inside is applied as before.
#[tokio::test(flavor = "multi_thread")]
async fn a_replace_never_reads_a_path_outside_the_root() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, pid, root) = engine(&dir);
    let inner = engine.inner();
    let preview = preview_replace(
        inner,
        FileScope::Workstream,
        &pid,
        &query("cart_total"),
        "total",
    )
    .unwrap();
    let inside = preview
        .iter()
        .find(|p| p.path == "src/lib.rs")
        .map(|p| (p.path.clone(), p.base_hash.clone()))
        .expect("src/lib.rs is in the preview");
    let files = vec![
        inside.clone(),
        ("../escape.txt".to_string(), "0".repeat(64)),
        ("/etc/hosts".to_string(), "0".repeat(64)),
    ];
    let outcomes = apply_replace(
        inner,
        FileScope::Workstream,
        &pid,
        &query("cart_total"),
        "total",
        &files,
    )
    .unwrap();
    assert_eq!(outcomes.len(), 3);
    assert!(matches!(&outcomes[0], ReplaceOutcome::Applied { path, .. } if path == "src/lib.rs"));
    for o in &outcomes[1..] {
        match o {
            ReplaceOutcome::Failed { path, error } => {
                assert!(error.contains("leaves"), "{path}: {error}");
            }
            other => panic!("an escaping path was read or hashed: {other:?}"),
        }
    }
    assert!(!root.join("../escape.txt").exists());
    engine.shutdown().await;
}

/// What a search refuses, and what it survives: a pattern that is no regex,
/// one that is nothing, one too large to build; a pattern that would send a
/// backtracking engine round for ever; a reader that stops listening; and
/// the cap a caller's own limit cannot pass.
#[tokio::test(flavor = "multi_thread")]
async fn a_search_refuses_in_words_and_ends_when_told_to() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, pid, root) = engine(&dir);
    let inner = engine.inner();
    std::fs::write(root.join("many.txt"), "needle\n".repeat(500)).unwrap();
    std::fs::write(root.join("evil.txt"), format!("{}!\n", "a".repeat(4000))).unwrap();

    for (pattern, regex, why) in [
        ("", false, "nothing to look for"),
        ("(unclosed", true, "a group that never closes"),
        ("[z-a]", true, "a range that runs backwards"),
        ("*", true, "a star with nothing before it"),
    ] {
        let q = SearchQuery {
            regex,
            ..query(pattern)
        };
        let refused = search(inner, FileScope::Workstream, &pid, &q, |_| true).unwrap_err();
        assert!(
            matches!(refused, bisa_engine::EngineError::Invalid(_)),
            "{why}: {refused}"
        );
        if !pattern.is_empty() {
            assert!(
                bisa_engine::ide::search::regex_for(&q).is_err(),
                "{why}: the route's own check refuses it too, before a stream opens"
            );
        }
    }
    // The same characters, not a regex: looked for as they are, and not found.
    let (hits, summary) = run(&engine, &pid, &query("(unclosed"));
    assert!(hits.is_empty() && summary.matches == 0 && !summary.truncated);
    // A pattern too large to build is refused, not built.
    let huge = SearchQuery {
        regex: true,
        ..query(r"(?:\w{1000}){1000}")
    };
    assert!(search(inner, FileScope::Workstream, &pid, &huge, |_| true).is_err());

    // Nested quantifiers over a line that almost matches: linear here, inside a breath.
    let began = std::time::Instant::now();
    let evil = SearchQuery {
        regex: true,
        ..query("(a+)+$")
    };
    let (hits, _) = run(&engine, &pid, &evil);
    assert!(hits.is_empty(), "the line ends in `!`");
    assert!(
        began.elapsed() < std::time::Duration::from_secs(5),
        "{:?}",
        began.elapsed()
    );

    // One character is a search like any other, and the limit says when it stopped.
    let one = SearchQuery {
        limit: Some(7),
        ..query("n")
    };
    let (hits, summary) = run(&engine, &pid, &one);
    assert_eq!((hits.len(), summary.truncated), (7, true));
    // A limit of nothing is one; a limit past the ceiling is the ceiling.
    let (hits, _) = run(
        &engine,
        &pid,
        &SearchQuery {
            limit: Some(0),
            ..query("needle")
        },
    );
    assert_eq!(hits.len(), 1);
    let (hits, summary) = run(
        &engine,
        &pid,
        &SearchQuery {
            limit: Some(usize::MAX),
            ..query("needle")
        },
    );
    assert_eq!((hits.len(), summary.truncated), (500, false));

    // A reader that stops listening ends the walk at that hit: nothing more is read for nobody.
    let mut heard = 0;
    let summary = search(inner, FileScope::Workstream, &pid, &query("needle"), |_| {
        heard += 1;
        heard < 3
    })
    .unwrap();
    assert_eq!(heard, 3, "the third hit was the last one offered");
    assert!(summary.matches <= 3, "{summary:?}");
    engine.shutdown().await;
}
