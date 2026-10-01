//! Who a process belongs to — the one walk both machine readers share.
//!
//! The port scanner (`ports.rs`) and the resource read (`stats.rs`) ask the
//! same question of the process table: *which thing the app can name is this
//! pid under?* The things it can name are its roots — this process, the node
//! it spawned, the login shell behind each terminal tab, and the harness
//! sessions the node reports by pid. A process is walked up its parent chain
//! until the chain ends; every root met on the way is remembered, so each
//! reader applies its own rule over one answer: the port scan prefers a
//! terminal wherever one stands on the chain (a server started inside a
//! harness inside a tab is that tab's), the resource read takes the nearest
//! root (a harness's tree is the harness's, not the shell's or the node's).
//!
//! Pure over `parent_of`, so the rule is tested without a process table.

use std::collections::HashMap;

/// A thing the app can name, by the pid that stands for it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Root {
    /// The desktop app itself — this process. The webview's renderer runs
    /// under WebKit's own processes, not under this one, so it is not here.
    Desktop,
    /// The node this shell spawned.
    Node,
    /// The login shell behind a terminal tab.
    Terminal { terminal_id: String },
    /// A harness session the node reports, by its process's pid.
    Session { pid: u32 },
}

/// The roots met on one pid's parent chain.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Chain {
    /// The first root met walking up — the owner for a share of CPU or memory.
    pub nearest: Option<Root>,
    /// The terminal met anywhere on the chain — the owner of a port.
    pub terminal: Option<String>,
}

/// The longest parent chain the walk follows — a guard against a process
/// table that reports a cycle, which no real one does.
const MAX_DEPTH: usize = 64;

/// Every root, keyed by pid.
#[derive(Debug, Clone, Default)]
pub struct Roots {
    by_pid: HashMap<u32, Root>,
}

impl Roots {
    /// The roots of one read: this process, the node, every live terminal's
    /// shell and every harness session with a pid. A pid named twice keeps
    /// the more specific name — a session over a terminal, either over the
    /// node — so the nearest rule reads the finer thing.
    pub fn new(
        desktop: Option<u32>,
        node: Option<u32>,
        terminals: impl IntoIterator<Item = (String, u32)>,
        sessions: impl IntoIterator<Item = u32>,
    ) -> Self {
        let mut by_pid = HashMap::new();
        if let Some(pid) = desktop {
            by_pid.insert(pid, Root::Desktop);
        }
        if let Some(pid) = node {
            by_pid.insert(pid, Root::Node);
        }
        for (terminal_id, pid) in terminals {
            by_pid.insert(pid, Root::Terminal { terminal_id });
        }
        for pid in sessions {
            by_pid.insert(pid, Root::Session { pid });
        }
        Self { by_pid }
    }

    /// Walk `start` up its parents and say which roots were met.
    pub fn chain(&self, start: u32, parent_of: &impl Fn(u32) -> Option<u32>) -> Chain {
        let mut chain = Chain::default();
        let mut pid = start;
        for _ in 0..MAX_DEPTH {
            if let Some(root) = self.by_pid.get(&pid) {
                if chain.nearest.is_none() {
                    chain.nearest = Some(root.clone());
                }
                if let Root::Terminal { terminal_id } = root {
                    if chain.terminal.is_none() {
                        chain.terminal = Some(terminal_id.clone());
                    }
                }
            }
            match parent_of(pid) {
                Some(parent) if parent != pid && parent > 1 => pid = parent,
                _ => break,
            }
        }
        chain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(edges: &[(u32, u32)]) -> impl Fn(u32) -> Option<u32> {
        let map: HashMap<u32, u32> = edges.iter().copied().collect();
        move |pid| map.get(&pid).copied()
    }

    #[test]
    fn the_nearest_root_owns_a_process_and_a_harness_under_the_node_is_the_sessions() {
        // desktop 10 → node 20 → harness 30 (session) → its child 31
        let parent = tree(&[(20, 10), (30, 20), (31, 30)]);
        let roots = Roots::new(Some(10), Some(20), [], [30]);
        assert_eq!(
            roots.chain(31, &parent).nearest,
            Some(Root::Session { pid: 30 })
        );
        assert_eq!(
            roots.chain(30, &parent).nearest,
            Some(Root::Session { pid: 30 })
        );
        assert_eq!(roots.chain(20, &parent).nearest, Some(Root::Node));
        assert_eq!(roots.chain(10, &parent).nearest, Some(Root::Desktop));
    }

    #[test]
    fn a_terminal_is_remembered_past_a_nearer_session() {
        // shell 100 (term-1) → harness 200 (session) → server 300
        let parent = tree(&[(300, 200), (200, 100), (100, 1)]);
        let roots = Roots::new(None, None, [("term-1".to_string(), 100)], [200]);
        let chain = roots.chain(300, &parent);
        assert_eq!(chain.nearest, Some(Root::Session { pid: 200 }));
        assert_eq!(chain.terminal.as_deref(), Some("term-1"));
    }

    #[test]
    fn a_process_under_nothing_has_no_chain() {
        let parent = tree(&[(900, 1)]);
        let roots = Roots::new(Some(10), None, [], []);
        assert_eq!(roots.chain(900, &parent), Chain::default());
    }

    #[test]
    fn the_walk_stops_at_launchd_and_at_a_parent_the_table_does_not_know() {
        // 400 → 1 (launchd): pid 1 is never asked about, so a root standing
        // for pid 1 or 0 could never be met — nothing the app names lives there.
        let parent = tree(&[(400, 1), (401, 0)]);
        let roots = Roots::new(Some(1), Some(0), [], []);
        assert_eq!(roots.chain(400, &parent), Chain::default());
        assert_eq!(roots.chain(401, &parent), Chain::default());
        // A parent the table has no row for ends the walk the same way.
        let orphan = tree(&[(500, 499)]);
        let roots = Roots::new(Some(10), None, [], []);
        assert_eq!(roots.chain(500, &orphan), Chain::default());
    }

    #[test]
    fn a_root_deeper_than_the_walk_reaches_is_not_met() {
        // A chain one longer than MAX_DEPTH with the root at its far end: the
        // guard against a runaway table holds, and the process reads as nobody's.
        let edges: Vec<(u32, u32)> = (0..MAX_DEPTH as u32)
            .map(|i| (1000 + i, 1000 + i + 1))
            .collect();
        let parent = tree(&edges);
        // The walk looks at MAX_DEPTH pids, 1000 to 1000 + MAX_DEPTH - 1; the next one up is out of reach.
        let far = 1000 + MAX_DEPTH as u32;
        let roots = Roots::new(None, None, [("far".to_string(), far)], []);
        assert_eq!(roots.chain(1000, &parent), Chain::default());
        // One step nearer and it is met.
        let roots = Roots::new(None, None, [("near".to_string(), far - 1)], []);
        assert_eq!(roots.chain(1000, &parent).terminal.as_deref(), Some("near"));
    }

    #[test]
    fn the_nearest_terminal_is_the_one_remembered_when_two_stand_on_a_chain() {
        // shell 100 (outer) → shell 200 (inner, a terminal opened from a terminal) → 300
        let parent = tree(&[(300, 200), (200, 100), (100, 1)]);
        let roots = Roots::new(
            None,
            None,
            [("outer".to_string(), 100), ("inner".to_string(), 200)],
            [],
        );
        let chain = roots.chain(300, &parent);
        assert_eq!(
            chain.terminal.as_deref(),
            Some("inner"),
            "the first terminal met walking up"
        );
        assert_eq!(
            chain.nearest,
            Some(Root::Terminal {
                terminal_id: "inner".to_string()
            })
        );
    }

    #[test]
    fn a_cyclic_process_table_ends_the_walk() {
        let parent = tree(&[(300, 200), (200, 300)]);
        let roots = Roots::new(None, None, [], []);
        assert_eq!(roots.chain(300, &parent), Chain::default());
    }

    #[test]
    fn a_pid_named_twice_keeps_the_finer_name() {
        let roots = Roots::new(Some(10), Some(10), [("t".to_string(), 10)], [10]);
        let parent = tree(&[]);
        assert_eq!(
            roots.chain(10, &parent).nearest,
            Some(Root::Session { pid: 10 })
        );
        assert_eq!(
            roots.by_pid.len(),
            1,
            "one pid is one root, whatever it was named"
        );
    }
}
