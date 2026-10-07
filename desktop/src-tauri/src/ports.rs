//! Listening ports, attributed to the shells and harnesses this app knows
//! about (ADR-0054).
//!
//! A dev server started in a rail's shell, or by a harness the node runs, is
//! a process this app never spawned itself — a grandchild of a PTY's login
//! shell, or a child of a harness CLI. Nothing in the app can know its port
//! except by asking the OS, so this module does: every listening TCP socket
//! on the machine, each walked up its parent chain until the chain reaches a
//! process the app can name — a terminal's shell (the registry keeps the pid)
//! or a harness the caller names by pid — or runs out, in which case the port
//! is somebody else's and is dropped.
//!
//! Two crates rather than `lsof`: `listeners` reads the socket table the way
//! each OS exposes it, `sysinfo` reads the process table; neither shells out,
//! and both answer in tens of milliseconds, which is what a five-second poll
//! from the rail can afford. The walk itself is `attribution.rs`'s, shared
//! with the resource read; this module's rule over it is *a terminal wins*.
//!
//! Stopping a port means `SIGTERM` to the process that listens — never the
//! shell above it, which is the person's, and never this app or its node,
//! which are refused by pid. The pid is re-checked against the socket table
//! first, because a pid the rail saw five seconds ago may have been recycled.

use serde::Serialize;
use std::collections::HashSet;
use std::sync::Arc;
use sysinfo::{Pid, ProcessRefreshKind, RefreshKind, Signal, System};
use tauri::State;

use crate::attribution::{Root, Roots};
use crate::sidecar::NodeState;
use crate::terminal::TerminalRegistry;

/// Who a port is traced back to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PortRoot {
    /// A descendant of the shell behind this terminal id.
    Terminal { terminal_id: String },
    /// A descendant of (or the very) process the caller named.
    Pid { pid: u32 },
}

/// One listening TCP port and the process holding it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListeningPort {
    pub port: u16,
    pub pid: u32,
    /// The process name as the OS reports it (`node`, `vite`, `python3`).
    pub process: String,
    pub root: PortRoot,
}

/// A listening socket as the scan sees it: pid, name, port.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Socket {
    pub pid: u32,
    pub process: String,
    pub port: u16,
}

/// Trace each socket to a root, dropping the ones that reach none.
///
/// Pure over its inputs so the rule is tested without a process table:
/// `parent_of` answers a pid's parent, `roots` are the things the app can
/// name. A terminal wins over a named session when a chain passes both — a
/// harness launched inside a rail's shell is that shell's; a socket under
/// the app itself or its node is nobody's to open or stop and is dropped.
/// Two sockets on the same pid and port (an IPv4 and an IPv6 bind) are one
/// row.
pub fn attribute(
    sockets: &[Socket],
    parent_of: impl Fn(u32) -> Option<u32>,
    roots: &Roots,
) -> Vec<ListeningPort> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for s in sockets {
        if !seen.insert((s.pid, s.port)) {
            continue;
        }
        let Some(root) = port_root(roots.chain(s.pid, &parent_of)) else {
            continue;
        };
        out.push(ListeningPort {
            port: s.port,
            pid: s.pid,
            process: s.process.clone(),
            root,
        });
    }
    out.sort_by_key(|p| (p.port, p.pid));
    out
}

/// The port scan's rule over a chain: the terminal wherever it stands, else
/// the nearest session; the app and the node own no port anyone may touch.
fn port_root(chain: crate::attribution::Chain) -> Option<PortRoot> {
    if let Some(terminal_id) = chain.terminal {
        return Some(PortRoot::Terminal { terminal_id });
    }
    match chain.nearest {
        Some(Root::Session { pid }) => Some(PortRoot::Pid { pid }),
        _ => None,
    }
}

/// The process table, read once per scan — and once per question the
/// sidecar asks of a pid (`sidecar.rs`: a stray node's name and start).
pub(crate) fn processes() -> System {
    System::new_with_specifics(RefreshKind::new().with_processes(ProcessRefreshKind::new()))
}

/// Every listening TCP socket the OS reports.
fn sockets() -> Result<Vec<Socket>, String> {
    let all = listeners::get_all().map_err(|e| format!("could not read the socket table: {e}"))?;
    Ok(all
        .into_iter()
        .map(|l| Socket {
            pid: l.process.pid,
            process: l.process.name,
            port: l.socket.port(),
        })
        .collect())
}

fn scan(roots: Roots) -> Result<Vec<ListeningPort>, String> {
    let sockets = sockets()?;
    let sys = processes();
    let parent_of = |pid: u32| {
        sys.process(Pid::from_u32(pid))
            .and_then(|p| p.parent())
            .map(|p| p.as_u32())
    };
    Ok(attribute(&sockets, parent_of, &roots))
}

/// The listening ports that trace back to a rail's shell or to one of `roots`
/// — the pids of the harness sessions the node reports.
#[tauri::command]
pub async fn listening_ports(
    roots: Vec<u32>,
    node: State<'_, NodeState>,
    registry: State<'_, Arc<TerminalRegistry>>,
) -> Result<Vec<ListeningPort>, String> {
    let roots = Roots::new(
        Some(std::process::id()),
        node.status().pid,
        registry.pids(),
        roots,
    );
    tauri::async_runtime::spawn_blocking(move || scan(roots))
        .await
        .map_err(|e| format!("the port scan did not finish: {e}"))?
}

/// Send `signal` to `pid`, if it still runs. The terminal registry's
/// escalation and the port rail's *Stop* use the same door.
pub(crate) fn signal(pid: u32, signal: Signal) -> bool {
    let sys = processes();
    sys.process(Pid::from_u32(pid))
        .is_some_and(|p| p.kill_with(signal).unwrap_or_else(|| p.kill()))
}

/// `SIGTERM` the process listening on `port`, if it is still `pid`.
///
/// The app's own pid and the node's are refused whatever the rail asks: the
/// person clicking *Stop* meant the dev server, not the desktop they are
/// looking at.
#[tauri::command]
pub async fn stop_port(pid: u32, port: u16, node: State<'_, NodeState>) -> Result<(), String> {
    let protected: HashSet<u32> = [Some(std::process::id()), node.status().pid]
        .into_iter()
        .flatten()
        .collect();
    if protected.contains(&pid) {
        return Err("that port belongs to the app itself".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let still = sockets()?
            .into_iter()
            .any(|s| s.pid == pid && s.port == port);
        if !still {
            return Err(format!(
                "nothing with pid {pid} listens on :{port} any more"
            ));
        }
        let sys = processes();
        let process = sys
            .process(Pid::from_u32(pid))
            .ok_or_else(|| format!("process {pid} is gone"))?;
        let stopped = process
            .kill_with(Signal::Term)
            .unwrap_or_else(|| process.kill());
        if stopped {
            Ok(())
        } else {
            Err(format!(
                "could not stop {} (pid {pid})",
                process.name().to_string_lossy()
            ))
        }
    })
    .await
    .map_err(|e| format!("the stop did not finish: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(edges: &[(u32, u32)]) -> impl Fn(u32) -> Option<u32> {
        let map: std::collections::HashMap<u32, u32> = edges.iter().copied().collect();
        move |pid| map.get(&pid).copied()
    }

    /// The roots a scan names: terminals by shell pid, sessions by pid.
    fn roots(terminals: &[(u32, &str)], sessions: &[u32]) -> Roots {
        Roots::new(
            None,
            None,
            terminals.iter().map(|(pid, id)| (id.to_string(), *pid)),
            sessions.iter().copied(),
        )
    }

    fn sock(pid: u32, port: u16) -> Socket {
        Socket {
            pid,
            process: format!("p{pid}"),
            port,
        }
    }

    #[test]
    fn a_port_is_traced_to_the_terminal_whose_shell_is_its_ancestor() {
        // 1 → 100 (shell of term-1) → 200 (npm) → 300 (vite:5173)
        let parent = tree(&[(300, 200), (200, 100), (100, 1)]);
        let got = attribute(&[sock(300, 5173)], parent, &roots(&[(100, "term-1")], &[]));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].port, 5173);
        assert_eq!(
            got[0].root,
            PortRoot::Terminal {
                terminal_id: "term-1".into()
            }
        );
    }

    #[test]
    fn a_port_is_traced_to_a_named_root_and_the_root_itself_counts() {
        let parent = tree(&[(400, 1), (410, 400)]);
        let got = attribute(
            &[sock(410, 3000), sock(400, 3001)],
            parent,
            &roots(&[], &[400]),
        );
        assert_eq!(
            got.iter().map(|p| p.port).collect::<Vec<_>>(),
            vec![3000, 3001]
        );
        assert!(got.iter().all(|p| p.root == PortRoot::Pid { pid: 400 }));
    }

    #[test]
    fn a_terminal_wins_over_a_named_root_on_the_same_chain() {
        // shell 100 (term-1) → harness 200 (named) → server 300
        let parent = tree(&[(300, 200), (200, 100), (100, 1)]);
        let got = attribute(
            &[sock(300, 8080)],
            parent,
            &roots(&[(100, "term-1")], &[200]),
        );
        assert_eq!(
            got[0].root,
            PortRoot::Terminal {
                terminal_id: "term-1".into()
            }
        );
    }

    #[test]
    fn somebody_elses_port_is_dropped_and_a_dual_stack_bind_is_one_row() {
        let parent = tree(&[(900, 1), (300, 100), (100, 1)]);
        let got = attribute(
            &[sock(900, 22), sock(300, 5173), sock(300, 5173)],
            parent,
            &roots(&[(100, "term-1")], &[]),
        );
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].pid, 300);
    }

    #[test]
    fn a_cyclic_process_table_ends_the_walk() {
        let parent = tree(&[(300, 200), (200, 300)]);
        let got = attribute(&[sock(300, 1)], parent, &roots(&[], &[]));
        assert!(got.is_empty());
    }

    #[test]
    fn a_socket_under_the_app_or_its_node_is_nobodys() {
        // desktop 10 → node 20 (its own :7777) → harness 30 (session, :3000)
        let parent = tree(&[(20, 10), (30, 20)]);
        let roots = Roots::new(Some(10), Some(20), [], [30]);
        let got = attribute(
            &[sock(20, 7777), sock(10, 1420), sock(30, 3000)],
            parent,
            &roots,
        );
        assert_eq!(got.iter().map(|p| p.port).collect::<Vec<_>>(), vec![3000]);
        assert_eq!(got[0].root, PortRoot::Pid { pid: 30 });
    }
}
