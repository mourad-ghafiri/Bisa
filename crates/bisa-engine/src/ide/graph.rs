//! The commit graph (ide/05): lane layout over the graph log, laid
//! out once per root in the background and served in windows.
//!
//! Layout is a left fold over the log in topological order, so the layout of
//! the first *n* rows is the prefix of the layout of all of them. That is what
//! lets the first screen paint from a 1,000-commit log while the whole log is
//! still being laid out, without a seam when the rest arrives.

use crate::ide::files::writable_root;
use crate::projects::blocking;
use crate::{EngineError, Inner};
use bisa_store::FileScope;
pub use bisa_vcs::git::RefScope;
use bisa_vcs::git::{self, CommitId, GraphCommit, GraphRef};
use std::collections::HashSet;

use bisa_cache::Bounded;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// How many commits the first pass lays out before the whole log is asked for.
pub const FIRST_SCREEN: usize = 1_000;
/// The most rows one window returns.
pub const WINDOW_CAP: usize = 2_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// This row's commit has a second (third, …) parent in lane `to`.
    Fork,
    /// A lane converged on this row's commit from lane `from`.
    Merge,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GraphEdge {
    pub from: u16,
    pub to: u16,
    pub kind: EdgeKind,
}

/// One row of the laid-out graph.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GraphRow {
    pub id: CommitId,
    pub short: String,
    pub lane: u16,
    /// Lanes holding an expected parent after this row — the lines that
    /// continue below it. The row above's `passing` is what enters from the top.
    pub passing: Vec<u16>,
    pub edges: Vec<GraphEdge>,
    pub refs: Vec<GraphRef>,
    pub author: String,
    pub timestamp: u64,
    pub subject: String,
    pub parents: u16,
}

fn first_free(lanes: &[Option<CommitId>]) -> usize {
    lanes
        .iter()
        .position(Option::is_none)
        .unwrap_or(lanes.len())
}

fn set_lane(lanes: &mut Vec<Option<CommitId>>, i: usize, v: Option<CommitId>) {
    if i >= lanes.len() {
        lanes.resize(i + 1, None);
    }
    lanes[i] = v;
}

/// Lane assignment, one pass, exactly the algorithm in ide/05.
pub fn layout(commits: &[GraphCommit]) -> Vec<GraphRow> {
    let mut lanes: Vec<Option<CommitId>> = Vec::new();
    let mut rows = Vec::with_capacity(commits.len());
    for c in commits {
        let claimed: Vec<usize> = lanes
            .iter()
            .enumerate()
            .filter(|(_, l)| l.as_ref() == Some(&c.id))
            .map(|(i, _)| i)
            .collect();
        let my_lane = claimed
            .first()
            .copied()
            .unwrap_or_else(|| first_free(&lanes));
        let mut edges = Vec::new();
        for other in claimed.iter().skip(1) {
            edges.push(GraphEdge {
                from: *other as u16,
                to: my_lane as u16,
                kind: EdgeKind::Merge,
            });
            set_lane(&mut lanes, *other, None);
        }
        set_lane(&mut lanes, my_lane, c.parents.first().cloned());
        for p in c.parents.iter().skip(1) {
            let lane = lanes
                .iter()
                .position(|l| l.as_ref() == Some(p))
                .unwrap_or_else(|| first_free(&lanes));
            set_lane(&mut lanes, lane, Some(p.clone()));
            edges.push(GraphEdge {
                from: my_lane as u16,
                to: lane as u16,
                kind: EdgeKind::Fork,
            });
        }
        // Trailing empty lanes never come back cheaper than they left.
        while lanes.last().is_some_and(Option::is_none) {
            lanes.pop();
        }
        rows.push(GraphRow {
            id: c.id.clone(),
            short: c.short.clone(),
            lane: my_lane as u16,
            passing: lanes
                .iter()
                .enumerate()
                .filter(|(_, l)| l.is_some())
                .map(|(i, _)| i as u16)
                .collect(),
            edges,
            refs: c.refs.clone(),
            author: c.author.clone(),
            timestamp: c.timestamp,
            subject: c.subject.clone(),
            parents: c.parents.len() as u16,
        });
    }
    rows
}

/// A laid-out graph for one root.
#[derive(Clone, Debug)]
pub struct Laid {
    pub rows: Vec<GraphRow>,
    /// False while only the first screen is laid out.
    pub done: bool,
    pub fingerprint: String,
}

/// How many roots keep a laid-out log at once; the oldest goes when a new one comes.
const LAID_ROOTS: usize = 16;

/// The per-root cache and the set of roots being relaid right now.
pub struct GraphRegistry {
    /// A bounded record: a laid-out log is the whole history of a root, and
    /// a person moving between many roots must not keep every one of them
    /// in memory for the node's life — the oldest laid goes first.
    laid: Mutex<Bounded<String, Arc<Laid>>>,
    running: Mutex<HashSet<String>>,
}

impl Default for GraphRegistry {
    fn default() -> Self {
        Self {
            laid: Mutex::new(Bounded::new(LAID_ROOTS)),
            running: Mutex::new(HashSet::new()),
        }
    }
}

impl GraphRegistry {
    fn get(&self, key: &str) -> Option<Arc<Laid>> {
        self.laid.lock().ok()?.get(key).cloned()
    }
    fn put(&self, key: &str, laid: Laid) {
        if let Ok(mut m) = self.laid.lock() {
            if let Some((old, _)) = m.insert(key.to_string(), Arc::new(laid)) {
                tracing::debug!(target: "bisa_engine::ide", "the laid-out graph of {old} is let go for {key}");
            }
        }
    }
    fn start(&self, key: &str) -> bool {
        self.running
            .lock()
            .map(|mut r| r.insert(key.to_string()))
            .unwrap_or(false)
    }
    fn finish(&self, key: &str) {
        if let Ok(mut r) = self.running.lock() {
            r.remove(key);
        }
    }
    fn is_running(&self, key: &str) -> bool {
        self.running
            .lock()
            .map(|r| r.contains(key))
            .unwrap_or(false)
    }
}

/// What one request gets back.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Window {
    pub total: usize,
    /// The whole log is laid out.
    pub done: bool,
    /// The repository moved on since these rows were laid out; a relayout is
    /// running and the next request will see it.
    pub stale: bool,
    pub from: usize,
    pub rows: Vec<GraphRow>,
}

/// The rows a search found: their indexes in the laid-out log, so a client
/// can ask for exactly the window that holds one.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Matches {
    /// Row indexes, ascending, from `from` on.
    pub indices: Vec<usize>,
    /// How many rows were looked at — the whole laid log, or the first
    /// screen while the tail is still being laid out.
    pub searched: usize,
    /// The whole log was searched.
    pub done: bool,
    /// The search stopped at `limit`; more rows match below the last index.
    pub truncated: bool,
}

/// The most matches one search answers with.
pub const MATCH_CAP: usize = 5_000;

/// Does a row match a query? Subject, author, id prefix and ref names,
/// case-insensitively — the same rule the client dims by, so a row it
/// highlights is a row this finds.
pub fn row_matches(row: &GraphRow, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let lower = |s: &str| s.to_lowercase();
    lower(&row.subject).contains(needle)
        || lower(&row.author).contains(needle)
        || lower(row.id.as_str()).starts_with(needle)
        || lower(&row.short).starts_with(needle)
        || row.refs.iter().any(|r| lower(&r.name).contains(needle))
}

/// Search laid-out rows from `from`, answering at most `limit` indexes.
/// Pure: what the route does with the cache, over a slice.
pub fn matches(rows: &[GraphRow], q: &str, from: usize, limit: usize, done: bool) -> Matches {
    let needle = q.trim().to_lowercase();
    let limit = limit.clamp(1, MATCH_CAP);
    let mut indices = Vec::new();
    let mut truncated = false;
    for (i, row) in rows.iter().enumerate().skip(from) {
        if row_matches(row, &needle) {
            if indices.len() == limit {
                truncated = true;
                break;
            }
            indices.push(i);
        }
    }
    Matches {
        indices,
        searched: rows.len(),
        done,
        truncated,
    }
}

/// Search a root's whole laid-out log. The root is laid out the way
/// `window` lays it out — the first screen inline, the rest behind — and
/// `done` says whether the tail was there to search yet.
pub async fn search(
    inner: &Arc<Inner>,
    scope: FileScope,
    id: &str,
    refs: RefScope,
    q: &str,
    from: usize,
    limit: usize,
) -> Result<Matches, EngineError> {
    let (laid, _) = laid_for(inner, scope, id, refs, false).await?;
    let key = key_for(scope, id, refs);
    let done = laid.done && !inner.ide_graph.is_running(&key);
    Ok(matches(&laid.rows, q, from, limit, done))
}

/// One cache entry per root *and* ref scope: the whole log and HEAD's reach
/// are two layouts, kept side by side so switching the filter is a lookup.
fn key_for(scope: FileScope, id: &str, refs: RefScope) -> String {
    format!("{scope:?}/{id}/{}", refs.as_str())
}

fn lay(
    root: &Path,
    limit: Option<usize>,
    refs: RefScope,
) -> Result<Vec<GraphRow>, bisa_vcs::VcsError> {
    Ok(layout(&git::graph_log(root, limit, refs)?))
}

/// Lay out the whole log in the background and store it as done.
fn relay_in_background(
    inner: Arc<Inner>,
    key: String,
    root: PathBuf,
    refs: RefScope,
    fingerprint: String,
) {
    if !inner.ide_graph.start(&key) {
        return;
    }
    tokio::spawn(async move {
        let r = blocking({
            let root = root.clone();
            move || lay(&root, None, refs)
        })
        .await;
        match r {
            Ok(rows) => inner.ide_graph.put(
                &key,
                Laid {
                    rows,
                    done: true,
                    fingerprint,
                },
            ),
            Err(e) => tracing::warn!("graph layout of {}: {e}", root.display()),
        }
        inner.ide_graph.finish(&key);
    });
}

/// The laid-out rows for a root, from the cache when the refs have not
/// moved; otherwise the first screen inline and the rest in the background,
/// or a relayout behind the rows already there.
async fn laid_for(
    inner: &Arc<Inner>,
    scope: FileScope,
    id: &str,
    refs: RefScope,
    refresh: bool,
) -> Result<(Arc<Laid>, String), EngineError> {
    let root = writable_root(inner, scope, id)?;
    let key = key_for(scope, id, refs);
    let fingerprint = blocking({
        let root = root.clone();
        move || git::refs_fingerprint(&root)
    })
    .await?;

    let cached = inner.ide_graph.get(&key);
    let laid = match cached {
        Some(l) if l.fingerprint == fingerprint && !refresh => l,
        Some(l) if l.rows.len() >= FIRST_SCREEN => {
            // Stale, or asked to, on a log too big to lay out inline: relay
            // behind the rows already on screen.
            relay_in_background(
                Arc::clone(inner),
                key.clone(),
                root,
                refs,
                fingerprint.clone(),
            );
            l
        }
        // A stale log that fits the first screen is laid out again inline:
        // the fresh rows answer this request, as the first one did.
        Some(_) | None => {
            let rows = blocking({
                let root = root.clone();
                move || lay(&root, Some(FIRST_SCREEN), refs)
            })
            .await?;
            let done = rows.len() < FIRST_SCREEN;
            let laid = Laid {
                rows,
                done,
                fingerprint: fingerprint.clone(),
            };
            inner.ide_graph.put(&key, laid.clone());
            if !done {
                relay_in_background(
                    Arc::clone(inner),
                    key.clone(),
                    root,
                    refs,
                    fingerprint.clone(),
                );
            }
            Arc::new(laid)
        }
    };
    Ok((laid, fingerprint))
}

/// A window of the graph for a root. The first request on a root lays out
/// the first screen inline and the rest in the background; later requests
/// are served from the cache, and a changed ref set triggers a relayout while
/// the stale rows stay on screen. `refs` picks the whole log or HEAD's reach.
pub async fn window(
    inner: &Arc<Inner>,
    scope: FileScope,
    id: &str,
    refs: RefScope,
    from: usize,
    count: usize,
    refresh: bool,
) -> Result<Window, EngineError> {
    let (laid, fingerprint) = laid_for(inner, scope, id, refs, refresh).await?;
    let key = key_for(scope, id, refs);
    let count = count.clamp(1, WINDOW_CAP);
    let from = from.min(laid.rows.len());
    let to = (from + count).min(laid.rows.len());
    Ok(Window {
        total: laid.rows.len(),
        done: laid.done && !inner.ide_graph.is_running(&key),
        stale: laid.fingerprint != fingerprint,
        from,
        rows: laid.rows[from..to].to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn id(n: usize) -> CommitId {
        CommitId::new(format!("{n:040x}"))
    }

    fn commit(n: usize, parents: &[usize]) -> GraphCommit {
        GraphCommit {
            id: id(n),
            short: format!("{n:07x}"),
            parents: parents.iter().map(|p| id(*p)).collect(),
            refs: vec![],
            author: "t".into(),
            email: "t@x".into(),
            timestamp: n as u64,
            subject: format!("c{n}"),
        }
    }

    /// A random DAG in topological order: commit i's parents all have larger
    /// indexes (older). Includes octopus merges. Deterministic LCG, no crate.
    fn random_dag(n: usize, seed: u64) -> Vec<GraphCommit> {
        let mut state = seed;
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
                    0..=5 => 1,
                    6..=8 => 2,
                    _ => 3,
                }
                .min(remaining);
                let mut parents = Vec::new();
                let mut tries = 0;
                while parents.len() < want && tries < 20 {
                    tries += 1;
                    let p = i + 1 + next((remaining.max(1) as u64).min(6)) as usize;
                    if p < n && !parents.contains(&p) {
                        parents.push(p);
                    }
                }
                commit(i, &parents)
            })
            .collect()
    }

    #[test]
    fn a_linear_history_is_one_lane() {
        let log: Vec<_> = (0..20)
            .map(|i| commit(i, if i < 19 { &[0][..] } else { &[] }).clone())
            .collect();
        // fix parents: each i has parent i+1
        let log: Vec<_> = log
            .into_iter()
            .enumerate()
            .map(|(i, mut c)| {
                c.parents = if i < 19 { vec![id(i + 1)] } else { vec![] };
                c
            })
            .collect();
        let rows = layout(&log);
        assert!(rows.iter().all(|r| r.lane == 0), "{rows:#?}");
        assert!(rows.iter().all(|r| r.edges.is_empty()));
        assert!(rows[..19].iter().all(|r| r.passing == vec![0]));
        assert!(rows[19].passing.is_empty(), "the root continues to nothing");
    }

    #[test]
    fn a_branch_and_its_merge_fork_and_rejoin() {
        // 0 = merge(1, 2); 1 -> 3; 2 -> 3; 3 root.
        let log = vec![
            commit(0, &[1, 2]),
            commit(1, &[3]),
            commit(2, &[3]),
            commit(3, &[]),
        ];
        let rows = layout(&log);
        assert_eq!(rows[0].lane, 0);
        assert_eq!(
            rows[0].edges,
            vec![GraphEdge {
                from: 0,
                to: 1,
                kind: EdgeKind::Fork
            }]
        );
        assert_eq!(rows[0].passing, vec![0, 1]);
        assert_eq!(rows[1].lane, 0);
        assert_eq!(rows[2].lane, 1);
        // 3 is expected in both lanes: it lands in lane 0 and lane 1 merges in.
        assert_eq!(rows[3].lane, 0);
        assert_eq!(
            rows[3].edges,
            vec![GraphEdge {
                from: 1,
                to: 0,
                kind: EdgeKind::Merge
            }]
        );
        assert!(rows[3].passing.is_empty());
    }

    #[test]
    fn every_commit_gets_one_row_and_every_edge_lands_on_a_real_row() {
        for seed in 1..=25u64 {
            let log = random_dag(120, seed);
            let rows = layout(&log);
            assert_eq!(rows.len(), log.len(), "total");
            let index: HashMap<&CommitId, usize> =
                rows.iter().enumerate().map(|(i, r)| (&r.id, i)).collect();
            // Replay the lanes to check each edge names a real lane holder.
            let mut lanes: Vec<Option<CommitId>> = Vec::new();
            for (i, (row, c)) in rows.iter().zip(&log).enumerate() {
                for e in &row.edges {
                    match e.kind {
                        EdgeKind::Merge => {
                            assert_eq!(
                                lanes.get(e.from as usize).cloned().flatten().as_ref(),
                                Some(&c.id),
                                "seed {seed} row {i}: merge from a lane not holding this commit"
                            );
                            assert_eq!(e.to, row.lane);
                        }
                        EdgeKind::Fork => {
                            assert_eq!(e.from, row.lane);
                        }
                    }
                }
                // Apply the same transitions the layout did.
                for l in lanes.iter_mut() {
                    if l.as_ref() == Some(&c.id) {
                        *l = None;
                    }
                }
                set_lane(&mut lanes, row.lane as usize, c.parents.first().cloned());
                for (e, p) in row
                    .edges
                    .iter()
                    .filter(|e| e.kind == EdgeKind::Fork)
                    .zip(c.parents.iter().skip(1))
                {
                    set_lane(&mut lanes, e.to as usize, Some(p.clone()));
                    // The parent a fork points at is a real row further down.
                    assert!(
                        index.get(p).is_some_and(|&j| j > i),
                        "seed {seed}: fork to a parent with no row below"
                    );
                }
                while lanes.last().is_some_and(Option::is_none) {
                    lanes.pop();
                }
                let expected: Vec<u16> = lanes
                    .iter()
                    .enumerate()
                    .filter(|(_, l)| l.is_some())
                    .map(|(i, _)| i as u16)
                    .collect();
                assert_eq!(row.passing, expected, "seed {seed} row {i}: passing lanes");
                // Bounded width: a lane is open only for a branch still
                // descending — a child at or above this row whose parent is
                // below it. Two children of one parent are two lines until
                // they meet, so lanes may outnumber distinct parents, never
                // open lines; and every parent still to come has a lane.
                let open_lines = log[..=i]
                    .iter()
                    .flat_map(|c| c.parents.iter())
                    .filter(|p| index[p] > i)
                    .count();
                assert!(
                    row.passing.len() <= open_lines,
                    "seed {seed} row {i}: {} lanes for {open_lines} open branches",
                    row.passing.len()
                );
                let pending: HashSet<&CommitId> = lanes.iter().flatten().collect();
                let expected_pending: HashSet<&CommitId> = log[..=i]
                    .iter()
                    .flat_map(|c| c.parents.iter())
                    .filter(|p| index[p] > i)
                    .collect();
                assert_eq!(
                    pending, expected_pending,
                    "seed {seed} row {i}: the lanes hold exactly the parents still to come"
                );
            }
        }
    }

    #[test]
    fn a_first_parent_never_lands_to_the_right_of_its_child() {
        for seed in 30..=40u64 {
            let log = random_dag(80, seed);
            let rows = layout(&log);
            let lane_of: HashMap<&CommitId, u16> = rows.iter().map(|r| (&r.id, r.lane)).collect();
            for (row, c) in rows.iter().zip(&log) {
                if let Some(p) = c.parents.first() {
                    assert!(
                        lane_of[p] <= row.lane,
                        "seed {seed}: first parent moved right"
                    );
                }
            }
        }
    }

    #[test]
    fn the_prefix_of_a_layout_is_the_layout_of_the_prefix() {
        let log = random_dag(300, 7);
        let whole = layout(&log);
        let part = layout(&log[..100]);
        assert_eq!(&whole[..100], &part[..]);
    }

    #[test]
    fn a_search_stops_at_its_cap_says_so_and_goes_on_from_where_it_stopped() {
        // Twelve thousand commits in a line, every one a match for `c`.
        let commits: Vec<GraphCommit> = (0..12_000)
            .map(|i| {
                if i + 1 < 12_000 {
                    commit(i, &[i + 1])
                } else {
                    commit(i, &[])
                }
            })
            .collect();
        let began = std::time::Instant::now();
        let rows = layout(&commits);
        assert_eq!(rows.len(), 12_000);
        assert!(
            rows.iter().all(|r| r.lane == 0),
            "a line is one lane, however long"
        );

        let first = matches(&rows, "c", 0, usize::MAX, true);
        assert_eq!(
            first.indices.len(),
            MATCH_CAP,
            "a caller's own limit reaches the cap and no further"
        );
        assert!(first.truncated, "and the answer says more rows match below");
        assert_eq!(first.indices.last().copied(), Some(MATCH_CAP - 1));
        // Asked again from the row after the last one: the rest, without a row twice.
        let next = matches(&rows, "c", MATCH_CAP, usize::MAX, true);
        assert_eq!(next.indices.first().copied(), Some(MATCH_CAP));
        assert_eq!(next.indices.len(), MATCH_CAP);
        let last = matches(&rows, "c", 2 * MATCH_CAP, usize::MAX, true);
        assert_eq!(
            (last.indices.len(), last.truncated),
            (2_000, false),
            "the end is said as the end"
        );
        // A limit of nothing is one; past the end is nothing, and no panic.
        assert_eq!(matches(&rows, "c", 0, 0, true).indices, vec![0]);
        let past = matches(&rows, "c", 50_000, 10, true);
        assert!(past.indices.is_empty() && !past.truncated);
        // Nothing to look for matches every row; a needle nobody has, none.
        assert_eq!(matches(&rows, "   ", 0, 3, false).indices, vec![0, 1, 2]);
        let none = matches(&rows, "no-such-subject", 0, 10, false);
        assert!(none.indices.is_empty() && !none.truncated && !none.done);
        assert_eq!(none.searched, 12_000);
        assert!(
            began.elapsed() < std::time::Duration::from_secs(10),
            "{:?}",
            began.elapsed()
        );
    }

    #[test]
    fn a_search_finds_rows_by_subject_author_id_prefix_and_ref_anywhere_in_the_log() {
        let mut log: Vec<GraphCommit> = (0..50)
            .map(|i| {
                let mut c = commit(i, if i < 49 { &[0][..] } else { &[] });
                c.parents = if i < 49 { vec![id(i + 1)] } else { vec![] };
                c
            })
            .collect();
        log[3].subject = "Fix the Cart total".into();
        log[20].author = "Grace".into();
        log[41].refs.push(GraphRef {
            name: "feature/sso".into(),
            kind: "branch".into(),
        });
        // Every synthetic id begins with zeros; one distinctive id shows
        // that a query is matched at the start of an id, not inside it.
        let distinct = CommitId::new(format!("deadbeef{:032x}", 47));
        log[46].parents = vec![distinct.clone()];
        log[47].id = distinct;
        log[47].short = "deadbee".into();
        let rows = layout(&log);
        let m = matches(&rows, "cart", 0, 100, true);
        assert_eq!(m.indices, vec![3], "case-insensitive on the subject");
        assert_eq!(m.searched, 50);
        assert!(m.done && !m.truncated);
        assert_eq!(matches(&rows, "grace", 0, 100, true).indices, vec![20]);
        assert_eq!(
            matches(&rows, "SSO", 0, 100, true).indices,
            vec![41],
            "a ref name"
        );
        assert_eq!(
            matches(&rows, "DEAD", 0, 100, true).indices,
            vec![47],
            "an id prefix, case-insensitively"
        );
        assert!(
            matches(&rows, "adbe", 0, 100, true).indices.is_empty(),
            "an id prefix, not a substring"
        );
        assert_eq!(
            matches(&rows, "c", 45, 100, true).indices,
            vec![45, 46, 47, 48, 49],
            "from an offset"
        );
        let capped = matches(&rows, "c", 0, 3, false);
        assert_eq!(capped.indices, vec![0, 1, 2]);
        assert!(capped.truncated, "more match below the last index");
        assert!(!capped.done, "the tail was not there to search yet");
        assert_eq!(
            matches(&rows, "", 0, 100, true).indices.len(),
            50,
            "an empty query matches everything"
        );
        assert!(matches(&rows, "nothing here", 0, 100, true)
            .indices
            .is_empty());
    }
}
