//! Commit-graph layout against a synthesised log (ide/14: full layout of
//! 100k commits < 2 s in the background; the first 1,000 rows < 150 ms).
//!
//! The log is generated in memory — a random DAG with two- and three-parent
//! merges — never read from a user's repository, and nothing touches disk.

use bisa_engine::ide::graph::layout;
use bisa_vcs::git::{CommitId, GraphCommit};
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};

fn synth(n: usize) -> Vec<GraphCommit> {
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = move |m: u64| {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (state >> 33) % m
    };
    (0..n)
        .map(|i| {
            let remaining = n - i - 1;
            let want = match next(10) {
                0..=6 => 1,
                7..=8 => 2,
                _ => 3,
            }
            .min(remaining);
            let mut parents = Vec::new();
            let mut tries = 0;
            while parents.len() < want && tries < 12 {
                tries += 1;
                let p = i + 1 + next(((remaining.max(1)) as u64).min(40)) as usize;
                if p < n && !parents.contains(&p) {
                    parents.push(p);
                }
            }
            GraphCommit {
                id: CommitId::new(format!("{i:040x}")),
                short: format!("{i:07x}"),
                parents: parents
                    .iter()
                    .map(|p| CommitId::new(format!("{p:040x}")))
                    .collect(),
                refs: vec![],
                author: "bench".into(),
                email: "bench@example.invalid".into(),
                timestamp: i as u64,
                subject: format!("commit {i}"),
            }
        })
        .collect()
}

fn bench(c: &mut Criterion) {
    let n: usize = std::env::var("GRAPH_BENCH_COMMITS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000);
    let log = synth(n);
    c.bench_function(&format!("graph_layout_{n}"), |b| {
        b.iter_batched(|| &log, |l| layout(l), BatchSize::LargeInput)
    });
    let first = &log[..1_000.min(log.len())];
    c.bench_function("graph_layout_first_1000", |b| b.iter(|| layout(first)));
}

criterion_group!(benches, bench);
criterion_main!(benches);
