//! Content search against a synthesised tree (ide/14).
//!
//! The fixture is built in a temporary directory by this bench — never a
//! user's repository — and is smaller than the budget document's 50k files so
//! a local run stays short; the budget scales linearly and the CI gate uses
//! `SEARCH_BENCH_FILES` to size it. Nothing here deletes anything: the
//! temporary directory is dropped.

use bisa_engine::ide::search::{search, CaseMode, SearchQuery};
use bisa_engine::{Engine, EngineConfig};
use bisa_harness::HarnessCatalog;
use bisa_store::{FileScope, MemoryKeyStore, NewProject, Workspace};
use criterion::{criterion_group, criterion_main, Criterion};

fn fixture(files: usize) -> (tempfile::TempDir, Engine, String) {
    let dir = tempfile::tempdir().unwrap();
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    let project = ws
        .create_project(NewProject::managed("bench").unwrap())
        .unwrap();
    let root = ws.project_root_path(&project);
    for i in 0..files {
        let sub = root.join(format!("mod{}", i % 64));
        std::fs::create_dir_all(&sub).unwrap();
        let mut body = String::with_capacity(2048);
        for line in 0..40 {
            body.push_str(&format!(
                "fn item_{i}_{line}() -> u32 {{ {line} }} // needle{}\n",
                if line % 7 == 0 { "_hit" } else { "" }
            ));
        }
        std::fs::write(sub.join(format!("file{i}.rs")), body).unwrap();
    }
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
    (dir, engine, project.id.to_string())
}

fn bench_search(c: &mut Criterion) {
    let files: usize = std::env::var("SEARCH_BENCH_FILES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2_000);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (_dir, engine, pid) = rt.block_on(async { fixture(files) });
    let inner = engine.inner().clone();
    let query = SearchQuery {
        q: "needle_hit".into(),
        regex: false,
        case: CaseMode::Sensitive,
        word: false,
        include: vec![],
        exclude: vec![],
        limit: Some(500),
    };
    c.bench_function(&format!("search first 500 hits over {files} files"), |b| {
        b.iter(|| {
            let mut n = 0;
            search(&inner, FileScope::Workstream, &pid, &query, |_| {
                n += 1;
                true
            })
            .unwrap();
            n
        })
    });
    rt.block_on(engine.shutdown());
}

criterion_group!(benches, bench_search);
criterion_main!(benches);
