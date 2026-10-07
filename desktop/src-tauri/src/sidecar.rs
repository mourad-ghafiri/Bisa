//! Sidecar management: spawn the `bisa` binary as a local node and keep
//! it alive for the lifetime of the app — *keep*: a watchdog thread reads the
//! child once a second and, when it exits without being asked, starts it
//! again on the **same port** after a delay that doubles to thirty seconds
//! and resets once the node has been healthy for a minute. Every respawn
//! reaches the webview as the `node:restarted` event, so the page reads its
//! lists again and the node's own restart recovery (the engine's boot walk)
//! is what the person sees on the goal, not a dead stream.
//!
//! **The node lives and dies with this desktop.** It is started *leashed*:
//! its stdin is a pipe the shell holds (`BISA_STOP_ON_STDIN_CLOSE=1`), so
//! when the shell goes — a quit, a crash, a Force Quit — the pipe closes and
//! the node stops as gracefully as on `SIGTERM`, and no stray node is left
//! holding the workspace against the next launch. Before a node is spawned
//! the shell asks the binary who holds the workspace (`bisa paths --json`):
//! a node this desktop itself started earlier and left behind is stopped
//! first; one a person runs themselves is named to the webview and never
//! signalled, the watchdog starting the shell's own the moment it goes.
//!
//! **A boot is waited for while it works.** The node says its phases on
//! stdout as JSON lines (`{"boot": {"phase": …}}` — opening the workspace,
//! rebuilding the index so far, picking up what was running); the shell
//! relays each to the webview (`node:boot`) and waits as long as the node
//! keeps speaking, within a budget since its last word and a hard cap. A
//! node that ends or falls silent is stopped gracefully and the failure —
//! why, and when the next try comes — reaches the webview (`node:failed`),
//! never a silence under *waiting for the node…*.
//!
//! Resolution order for the API base:
//! 1. `BISA_API_BASE` env — use an already-running node, spawn nothing
//!    (UI development against a live daemon).
//! 2. Spawn `bisa --json node --listen 127.0.0.1:<free port>`, binary found
//!    via `BISA_BIN` env → the node the bundle carries beside this executable
//!    (`scripts/macos/lib.sh` puts it there) → `PATH` → `../target/debug/bisa`
//!    (repo dev). A binary that fails hands the next attempt to the next one.
//!
//! The node is started from the app's `setup` ([`boot`]), never from
//! `main`: a second launch of the app leaves `build()` through the
//! single-instance plugin (`second_launch.rs`), and a node spawned before
//! that would be nobody's. The window opens at once: nothing here waits on
//! the node's health in `setup` — the watchdog does, tick by tick, so a
//! long rebuild after an upgrade is a line in the sidebar and not a node
//! killed every twenty seconds.

use crate::sync::Locked;
use bisa_log::{ChildExit, CrashKind, CrashReport, Handle, Process};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager};

pub struct NodeState {
    inner: Mutex<NodeInner>,
}

struct NodeInner {
    child: Option<Child>,
    /// The leash: the child's stdin, held while it runs. Dropped, it is the
    /// child's cue to stop (`BISA_STOP_ON_STDIN_CLOSE`).
    leash: Option<ChildStdin>,
    /// The child's last stderr lines, kept by the thread that reads them —
    /// what a crash report says the node said before it died.
    tail: Option<StderrTail>,
    /// The child's boot words, kept by the thread that reads its stdout.
    boot: Option<BootTail>,
    port: u16,
    api_base: String,
    /// What the node requires on every request. Minted here and handed to
    /// the sidecar through `BISA_API_TOKEN`; read from the same
    /// variable when the node is external.
    token: String,
    external: bool,
    /// The shell asked the child to go (a restart, the quit): its exit is
    /// not a crash, and the watchdog leaves it alone.
    expected_exit: bool,
    /// How many times in a row the watchdog has had to start the node
    /// again; the delay doubles with it and it resets after a healthy minute.
    crashes: u32,
    /// When the current child was seen healthy first — the minute counts from here.
    healthy_since: Option<Instant>,
    /// The current child since it was spawned, until it answers its health
    /// check. `None` once it has, or when there is no child.
    booting: Option<Booting>,
    /// Why there is no child, while there is none. The window opens all the
    /// same, the webview says this, and the watchdog keeps trying — a node
    /// that cannot start must never be an app that vanishes.
    failed: Option<Failure>,
    /// Where the node may be, in the order it is tried, and which is next:
    /// a binary that failed hands the next attempt to the one after it.
    candidates: Vec<PathBuf>,
    next: usize,
    /// The workspace's folders, as the binary named them before any node
    /// ran — where the shell keeps the record of the node it spawned, and
    /// what it reveals when the node cannot be asked.
    dirs: Option<WorkspaceDirs>,
    /// The sequence of the last boot word relayed to the webview.
    relayed: u64,
}

/// A child between its spawn and its first answer.
#[derive(Clone, Copy, Debug)]
struct Booting {
    spawned: Instant,
    /// Unix seconds at the spawn — what the own record says.
    spawned_at: u64,
    /// Not the first start of this app: a respawn, or a restart asked for —
    /// the webview hears `node:restarted` when it answers.
    restart: bool,
    /// The restart was asked for (Settings › Node), not a crash's.
    requested: bool,
}

impl NodeInner {
    /// Record why there is no child, and what the watchdog says of it.
    fn fail(&mut self, failure: Failure) -> NodeFailed {
        self.failed = Some(failure.clone());
        NodeFailed {
            failure,
            attempt: self.crashes,
            next_in_secs: self.next_delay().as_secs(),
        }
    }

    /// When the watchdog tries again: somebody else's node is asked after
    /// on a steady beat; anything else on the doubling curve.
    fn next_delay(&self) -> Duration {
        match self.failed {
            Some(Failure::HeldByOther { .. }) => HELD_POLL,
            _ => restart_delay(self.crashes),
        }
    }

    /// The child ended before it answered: no child, one more crash, the
    /// next binary next time, and the failure with what it said.
    fn boot_ended(&mut self, pid: u32, exit: Exit) -> Watch {
        let said = self.tail.take().map(|t| t.lines()).unwrap_or_default();
        self.child = None;
        self.leash = None;
        self.boot = None;
        self.booting = None;
        self.crashes = self.crashes.saturating_add(1);
        self.next = next_candidate(self.next, self.candidates.len());
        let failed = self.fail(Failure::Exited {
            how: exit.words(),
            said,
        });
        tracing::error!(target: "bisa_desktop", pid, "{}", failed.failure.words());
        Watch::Failed(failed)
    }

    /// A healthy child ended unasked: no child, one more crash, and the
    /// exit to log and report once the lock is let go.
    fn crashed(&mut self, pid: u32, exit: Exit) -> (NodeExit, NodeFailed) {
        let stderr = self.tail.take().map(|t| t.lines()).unwrap_or_default();
        self.child = None;
        self.leash = None;
        self.boot = None;
        self.crashes = self.crashes.saturating_add(1);
        let failed = self.fail(Failure::Exited {
            how: exit.words(),
            said: stderr.clone(),
        });
        (NodeExit { pid, exit, stderr }, failed)
    }
}

/// The workspace's folders, as `bisa paths --json` names them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WorkspaceDirs {
    pub data_dir: PathBuf,
    pub logs_dir: PathBuf,
}

/// What the shell knows of the node as its supervisor — the footer's node
/// overlay reads it at rest (`node_status`), and `restart_node` answers it
/// after a restart. What the node knows of itself is `GET /node`'s.
#[derive(Serialize, Clone)]
pub struct NodeStatus {
    pub running: bool,
    pub pid: Option<u32>,
    pub port: u16,
    /// A node a person started (`BISA_API_BASE`), not this app's child.
    pub external: bool,
    /// Unasked-for restarts in a row — reset once the node stays healthy.
    pub restarts: u32,
    /// How long the child has been up and answering, in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub healthy_secs: Option<u64>,
    /// The child is up and not yet answering: it is booting.
    pub booting: bool,
    /// Why there is no node, while there is none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<Failure>,
}

/// What `node:restarted` carries: where the node is now, and how many
/// unasked-for restarts in a row this one makes.
#[derive(Serialize, Clone)]
pub struct NodeRestarted {
    pub port: u16,
    pub attempt: u32,
    /// The shell asked (Settings › Node › Restart), not a crash.
    pub requested: bool,
}

/// Why the node is not running — what `node:failed` carries beside the
/// attempt and when the next one comes, and what `node_status` says.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Failure {
    /// The child ended before it answered its health check — refused the
    /// workspace, a loader's refusal, a crash — and how.
    Exited { how: String, said: Vec<String> },
    /// The budget ran out with the child up and silent.
    TimedOut { secs: u64, said: Vec<String> },
    /// No binary could be started; what each said.
    NoBinary { tried: Vec<String> },
    /// Another process — not a node this desktop started — holds the
    /// workspace. Never signalled: the desktop starts its own the moment
    /// that one goes.
    HeldByOther { pid: u32 },
}

impl Failure {
    /// The reason in one sentence — what `api_base` says to a webview that
    /// speaks the older string path, and what the log says.
    pub fn words(&self) -> String {
        match self {
            Failure::Exited { how, said } => {
                format!(
                    "the node {how} before answering its health check{}",
                    said_words(said)
                )
            }
            Failure::TimedOut { secs, said } => {
                format!(
                    "the node did not answer within {secs} s{}",
                    said_words(said)
                )
            }
            Failure::NoBinary { tried } => {
                format!(
                    "could not start the bisa node. Tried:\n{}",
                    tried.join("\n")
                )
            }
            Failure::HeldByOther { pid } => {
                format!("another node holds this workspace (pid {pid})")
            }
        }
    }
}

/// What `node:failed` carries: the failure, which attempt it was, and when
/// the watchdog tries again.
#[derive(Serialize, Clone)]
pub struct NodeFailed {
    #[serde(flatten)]
    pub failure: Failure,
    pub attempt: u32,
    pub next_in_secs: u64,
}

/// One word of the node's boot, as `{"boot": {…}}` on its stdout says it —
/// and what `node:boot` carries; `ready` once the node answers.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct BootWord {
    pub phase: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub of: Option<u64>,
}

impl BootWord {
    fn ready() -> Self {
        BootWord {
            phase: "ready".into(),
            done: None,
            of: None,
        }
    }
}

/// The event names the webview listens for.
pub const NODE_RESTARTED: &str = "node:restarted";
pub const NODE_BOOT: &str = "node:boot";
pub const NODE_FAILED: &str = "node:failed";

/// How long the watchdog waits before the nth respawn in a row: 1 s doubling
/// to 30 s, so a node that dies at once does not spin, and one that dies
/// once is back before anybody notices. Pure.
pub fn restart_delay(attempt: u32) -> Duration {
    let secs = 1u64 << attempt.min(5);
    Duration::from_secs(secs.min(30))
}

/// How long a node has to stay up for the crash count to reset.
const HEALTHY_AFTER: Duration = Duration::from_secs(60);
/// How often the watchdog reads the child.
const WATCH_EVERY: Duration = Duration::from_secs(1);
/// How often the shell asks again when somebody else's node holds the
/// workspace — a node a person runs themselves, never signalled.
const HELD_POLL: Duration = Duration::from_secs(5);
/// How long a booting node may go without a word — a boot line or its
/// health answer — before it is given up on. A rebuild that keeps reporting
/// is waited for.
pub const BOOT_BUDGET: Duration = Duration::from_secs(180);
/// How long a boot may take in all, however much it says.
pub const BOOT_CAP: Duration = Duration::from_secs(15 * 60);
/// How long a node asked to go — for a restart — may take before it is killed.
const RESTART_GRACE: Duration = Duration::from_secs(10);
/// The same, at the quit.
const QUIT_GRACE: Duration = Duration::from_secs(5);
/// The same, for a boot given up on.
const ABANDON_GRACE: Duration = Duration::from_secs(5);
/// How long a stray node of this desktop's own is given to stop on
/// `SIGTERM`, then on `SIGKILL`.
const RECLAIM_TERM_GRACE: Duration = Duration::from_secs(10);
const RECLAIM_KILL_GRACE: Duration = Duration::from_secs(2);
/// How far the record's spawn moment and the process's start may be apart
/// and still be one process.
pub(crate) const RECLAIM_START_SLACK: u64 = 120;

/// The boot, from `setup` and never from `main` (one Bisa at a time,
/// `second_launch.rs`): the log file first, under the workspace the `bisa`
/// binary names, so a node that fails to start is a line in the file and
/// not on a stderr nobody reads; then the node, spawned and left to boot —
/// the watchdog hears its first answer, then attaches the file again from
/// the node's own word when nothing was writing yet. A node that cannot
/// start is not a reason for no window: the state says why, the shell
/// opens, and the watchdog keeps trying ([`NodeState::supervise`]).
pub fn boot<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> NodeState {
    let log = app.state::<Handle>();
    let dirs = match crate::logging::attach_from_binary(&log) {
        Ok(answer) => {
            tracing::debug!(target: "bisa_desktop", dir = %answer.logs_dir.display(), "log folder named by the binary");
            Some(WorkspaceDirs {
                data_dir: answer.data_dir,
                logs_dir: answer.logs_dir,
            })
        }
        Err(e) => {
            tracing::warn!(target: "bisa_desktop", "no log folder from the binary: {e}");
            None
        }
    };
    let node = NodeState::start(dirs);
    match node.api_base() {
        Ok(base) => {
            tracing::info!(
                target: "bisa_desktop",
                version = env!("CARGO_PKG_VERSION"),
                node = %base,
                booting = node.status().booting,
                "desktop started"
            );
        }
        Err(reason) => {
            tracing::error!(
                target: "bisa_desktop",
                version = env!("CARGO_PKG_VERSION"),
                "desktop started without a node: {reason}"
            );
        }
    }
    node
}

/// What one tick of the watchdog found.
enum Watch {
    /// Nothing to do: an external node, a healthy child, a child still booting.
    Nothing,
    /// The child answered its health check for the first time.
    Healthy { restarted: Option<NodeRestarted> },
    /// A booting child fell silent past its budget: stop it, then say so.
    Abandon {
        child: Child,
        leash: Option<ChildStdin>,
        said: Vec<String>,
        secs: u64,
    },
    /// The child ended or could not start; the watchdog owes a start after
    /// the delay.
    Failed(NodeFailed),
    /// No child and a failure recorded: the watchdog owes a start.
    NeedsStart { delay: Duration },
}

impl NodeState {
    /// The state, poison-tolerant: a thread that panicked while holding it
    /// (the watchdog's, a command's) must not take the supervisor with it.
    fn lock(&self) -> std::sync::MutexGuard<'_, NodeInner> {
        self.inner.locked()
    }

    /// Start the node — spawn it and leave it to boot — or record why it
    /// could not: the shell opens either way, and
    /// [`supervise`](Self::supervise) keeps trying and hears the first answer.
    pub fn start(dirs: Option<WorkspaceDirs>) -> Self {
        if let Ok(base) = std::env::var("BISA_API_BASE") {
            let base = base.trim_end_matches('/').to_string();
            let port = base
                .rsplit(':')
                .next()
                .and_then(|p| p.parse().ok())
                .unwrap_or(0);
            return Self {
                inner: Mutex::new(NodeInner {
                    child: None,
                    leash: None,
                    tail: None,
                    boot: None,
                    port,
                    api_base: base,
                    token: std::env::var("BISA_API_TOKEN").unwrap_or_default(),
                    external: true,
                    expected_exit: false,
                    crashes: 0,
                    healthy_since: Some(Instant::now()),
                    booting: None,
                    failed: None,
                    candidates: Vec::new(),
                    next: 0,
                    dirs,
                    relayed: 0,
                }),
            };
        }
        let (token, failed) = match mint_token() {
            Ok(token) => (token, None),
            Err(e) => (
                String::new(),
                Some(Failure::NoBinary {
                    tried: vec![format!("no token: {e}")],
                }),
            ),
        };
        let state = Self {
            inner: Mutex::new(NodeInner {
                child: None,
                leash: None,
                tail: None,
                boot: None,
                port: 0,
                api_base: String::new(),
                token,
                external: false,
                expected_exit: false,
                crashes: 0,
                healthy_since: None,
                booting: None,
                failed,
                candidates: candidate_binaries(),
                next: 0,
                dirs,
                relayed: 0,
            }),
        };
        if state.lock().failed.is_none() {
            state.launch(false, false);
        }
        state
    }

    /// Spawn the node with the next candidate binary, after taking the
    /// workspace back from a stray node of this desktop's own. `restart` is
    /// whether this is not the first start of the app (the webview hears
    /// `node:restarted` when the node answers); `requested`, whether a
    /// person asked for it. Answers the failure, recorded, when no node was
    /// spawned — no binary, or somebody else holding the workspace — and
    /// nothing when a child is now booting.
    fn launch(&self, restart: bool, requested: bool) -> Option<NodeFailed> {
        // What the launch needs, read and released: the reclaim below runs
        // the binary and may wait on a stray node, and a command on the main
        // thread must not wait with it.
        let (bin, dirs, token, keep) = {
            let mut inner = self.lock();
            if inner.external || inner.child.is_some() {
                return None;
            }
            if inner.token.is_empty() {
                match mint_token() {
                    Ok(minted) => inner.token = minted,
                    Err(e) => {
                        return Some(inner.fail(Failure::NoBinary {
                            tried: vec![format!("no token: {e}")],
                        }));
                    }
                }
            }
            let next = inner.next;
            let bin = inner.candidates.get(next).cloned();
            let Some(bin) = bin else {
                return Some(inner.fail(Failure::NoBinary {
                    tried: vec!["no bisa binary on this machine".to_string()],
                }));
            };
            (
                bin,
                inner.dirs.clone(),
                inner.token.clone(),
                (inner.port != 0).then_some(inner.port),
            )
        };
        match reclaim(&bin, dirs.as_ref()) {
            Reclaim::Spawn => {}
            Reclaim::StopOurs(pid) => {
                tracing::warn!(target: "bisa_desktop", pid, "a node this desktop started earlier still holds the workspace; stopping it");
                stop_stray(pid);
                if let Some(dirs) = &dirs {
                    remove_own_record(&dirs.data_dir);
                }
            }
            Reclaim::HeldByOther(pid) => {
                tracing::warn!(target: "bisa_desktop", pid, "another node holds the workspace; the desktop starts its own when it goes");
                return Some(self.lock().fail(Failure::HeldByOther { pid }));
            }
        }
        let spawned = spawn_node(&bin, &token, keep);
        let mut inner = self.lock();
        match spawned {
            Ok(spawned) => {
                let port = spawned.port;
                tracing::info!(target: "bisa_desktop", pid = spawned.child.id(), port, bin = %bin.display(), "node spawned; waiting for its first answer");
                inner.child = Some(spawned.child);
                inner.leash = Some(spawned.leash);
                inner.tail = Some(spawned.tail);
                inner.boot = Some(spawned.boot);
                inner.relayed = 0;
                inner.port = port;
                inner.api_base = format!("http://127.0.0.1:{port}");
                inner.healthy_since = None;
                inner.booting = Some(Booting {
                    spawned: Instant::now(),
                    spawned_at: now_secs(),
                    restart,
                    requested,
                });
                inner.failed = None;
                None
            }
            Err(e) => {
                tracing::error!(target: "bisa_desktop", bin = %bin.display(), "the node could not be spawned: {e}");
                inner.next = next_candidate(inner.next, inner.candidates.len());
                Some(inner.fail(Failure::NoBinary {
                    tried: vec![format!("{}: {e}", bin.display())],
                }))
            }
        }
    }

    /// One tick of the watchdog: what the child is doing, read under the
    /// lock; its health probed outside it, so no command waits on a probe.
    fn watch(&self) -> Watch {
        let (pid, port) = {
            let mut guard = self.lock();
            let inner = &mut *guard;
            if inner.external || inner.expected_exit {
                return Watch::Nothing;
            }
            let Some(child) = inner.child.as_mut() else {
                if inner.failed.is_some() {
                    return Watch::NeedsStart {
                        delay: inner.next_delay(),
                    };
                }
                return Watch::Nothing;
            };
            let pid = child.id();
            let exit = match child.try_wait() {
                Ok(Some(status)) => Some(Exit::of(&status)),
                Ok(None) => None,
                Err(_) => Some(Exit::unknown()),
            };
            match (inner.booting, exit) {
                (Some(_), None) => (pid, inner.port),
                (Some(_), Some(exit)) => return inner.boot_ended(pid, exit),
                (None, None) => {
                    if let Some(since) = inner.healthy_since {
                        if since.elapsed() >= HEALTHY_AFTER {
                            inner.crashes = 0;
                        }
                    }
                    return Watch::Nothing;
                }
                (None, Some(exit)) => {
                    let (died, failed) = inner.crashed(pid, exit);
                    drop(guard);
                    died.log(failed.attempt);
                    return Watch::Failed(failed);
                }
            }
        };
        let healthy = probe_health(port);
        let mut guard = self.lock();
        let inner = &mut *guard;
        // A restart or the quit may have taken the child meanwhile.
        let Some(booting) = inner.booting else {
            return Watch::Nothing;
        };
        if inner.child.as_ref().map(|c| c.id()) != Some(pid) {
            return Watch::Nothing;
        }
        if healthy {
            inner.booting = None;
            inner.healthy_since = Some(Instant::now());
            inner.failed = None;
            return Watch::Healthy {
                restarted: booting.restart.then_some(NodeRestarted {
                    port,
                    attempt: inner.crashes,
                    requested: booting.requested,
                }),
            };
        }
        let last_word = inner.boot.as_ref().and_then(|b| b.last_word_at());
        if boot_budget_exhausted(Instant::now(), booting.spawned, last_word) {
            let said = inner.tail.take().map(|t| t.lines()).unwrap_or_default();
            let child = inner.child.take().expect("the child was just read");
            let leash = inner.leash.take();
            inner.boot = None;
            inner.booting = None;
            inner.crashes = inner.crashes.saturating_add(1);
            inner.next = next_candidate(inner.next, inner.candidates.len());
            return Watch::Abandon {
                child,
                leash,
                said,
                secs: booting.spawned.elapsed().as_secs(),
            };
        }
        Watch::Nothing
    }

    /// A boot word the webview has not heard yet.
    fn boot_word_to_relay(&self) -> Option<BootWord> {
        let mut inner = self.lock();
        let (seq, word) = inner.boot.as_ref()?.latest()?;
        if seq <= inner.relayed {
            return None;
        }
        inner.relayed = seq;
        Some(word)
    }

    /// The record of the node this desktop spawned, for the next launch to
    /// take the workspace back from if this one never ends it.
    fn write_own_record(&self) {
        let inner = self.lock();
        let (Some(dirs), Some(child)) = (&inner.dirs, &inner.child) else {
            return;
        };
        let spawned_at = inner.booting.map(|b| b.spawned_at).unwrap_or_else(now_secs);
        let record = OwnRecord {
            pid: child.id(),
            spawned_at,
            desktop_pid: std::process::id(),
            port: inner.port,
        };
        if let Err(e) = record.write(&dirs.data_dir) {
            tracing::warn!(target: "bisa_desktop", "the node's record could not be written: {e}");
        }
    }

    /// The watchdog: one thread for the life of the app, reading the child
    /// once a second. A booting child is probed until it answers — then the
    /// shell's log is attached from the node's own word when nothing was
    /// writing yet, the record of the spawn is written, and the webview
    /// hears `node:boot` *ready* (and `node:restarted` when this was not the
    /// first start). Every boot word the child says reaches the webview as
    /// it is said. An exit the shell did not ask for is an `error` line with
    /// its code or signal and the node's last stderr lines, a crash report
    /// with the same, a `node:failed` event naming the next try, and a
    /// respawn after [`restart_delay`]. A boot that falls silent past its
    /// budget is stopped gracefully and said the same way.
    pub fn supervise<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
        std::thread::Builder::new()
            .name("node-watchdog".into())
            .spawn(move || loop {
                std::thread::sleep(WATCH_EVERY);
                let state = app.state::<NodeState>();
                if let Some(word) = state.boot_word_to_relay() {
                    if let Err(e) = app.emit(NODE_BOOT, word) {
                        tracing::warn!(target: "bisa_desktop", "a boot word did not reach the webview: {e}");
                    }
                }
                match state.watch() {
                    Watch::Nothing => {}
                    Watch::Healthy { restarted } => {
                        tracing::info!(target: "bisa_desktop", "the node answers");
                        reattach_log(&app, &state);
                        state.write_own_record();
                        if let Err(e) = app.emit(NODE_BOOT, BootWord::ready()) {
                            tracing::warn!(target: "bisa_desktop", "the node's readiness did not reach the webview: {e}");
                        }
                        if let Some(restarted) = restarted {
                            tracing::info!(target: "bisa_desktop", port = restarted.port, attempt = restarted.attempt, requested = restarted.requested, "node restarted");
                            if let Err(e) = app.emit(NODE_RESTARTED, restarted) {
                                tracing::warn!(target: "bisa_desktop", "the restart did not reach the webview: {e}");
                            }
                        }
                    }
                    Watch::Abandon {
                        mut child,
                        leash,
                        said,
                        secs,
                    } => {
                        tracing::error!(target: "bisa_desktop", pid = child.id(), secs, "the node did not answer within its boot budget; stopping it");
                        stop(&mut child, leash, ABANDON_GRACE);
                        let failed = state.lock().fail(Failure::TimedOut { secs, said });
                        state.say_failed(&app, failed);
                    }
                    Watch::Failed(failed) => {
                        state.say_failed(&app, failed);
                    }
                    Watch::NeedsStart { delay } => {
                        tracing::warn!(target: "bisa_desktop", delay_secs = delay.as_secs(), "the node is not running; starting it");
                        std::thread::sleep(delay);
                        if let Some(failed) = state.launch(true, false) {
                            tracing::warn!(target: "bisa_desktop", next_in_secs = failed.next_in_secs, "{}", failed.failure.words());
                            if let Err(e) = app.emit(NODE_FAILED, failed) {
                                tracing::warn!(target: "bisa_desktop", "the failure did not reach the webview: {e}");
                            }
                        }
                    }
                }
            })
            .expect("the watchdog thread spawns");
    }

    /// A failure said to the webview, the delay waited, and the node
    /// started again — a start that fails in turn is the next tick's to say.
    fn say_failed<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>, failed: NodeFailed) {
        let delay = Duration::from_secs(failed.next_in_secs);
        if let Err(e) = app.emit(NODE_FAILED, failed) {
            tracing::warn!(target: "bisa_desktop", "the failure did not reach the webview: {e}");
        }
        std::thread::sleep(delay);
        self.launch(true, false);
    }

    /// Where the node answers — or why there is no node to answer yet. A
    /// node still booting is a node: its base is answered, and the first
    /// calls are refused until it is up, which the webview reads as the
    /// node's boot line.
    pub fn api_base(&self) -> Result<String, String> {
        let inner = self.lock();
        if inner.external || inner.child.is_some() {
            return Ok(inner.api_base.clone());
        }
        match &inner.failed {
            Some(failure) => Err(format!("the node did not start: {}", failure.words())),
            None => Ok(inner.api_base.clone()),
        }
    }

    pub fn api_token(&self) -> String {
        self.lock().token.clone()
    }

    /// The workspace's folders, as the binary named them; none when no
    /// binary answered.
    pub fn dirs(&self) -> Option<WorkspaceDirs> {
        self.lock().dirs.clone()
    }

    pub fn status(&self) -> NodeStatus {
        let mut inner = self.lock();
        let external = inner.external;
        let port = inner.port;
        let booting = inner.booting.is_some();
        let (running, pid) = match inner.child.as_mut() {
            Some(c) => match c.try_wait() {
                Ok(None) => (true, Some(c.id())),
                _ => (false, None),
            },
            None => (external, None),
        };
        NodeStatus {
            running: running && !booting,
            pid,
            port,
            external,
            restarts: inner.crashes,
            healthy_secs: running
                .then(|| inner.healthy_since.map(|s| s.elapsed().as_secs()))
                .flatten(),
            booting: running && booting,
            failure: inner.failed.clone(),
        }
    }

    /// A restart the shell was asked for: the child goes, gracefully, and
    /// comes back on its port; the exit is expected, so the watchdog stays
    /// out of it until the new child boots. Answers the status; the webview
    /// hears `node:restarted` from the watchdog once the node answers. Runs
    /// off the main thread (`restart_node` is an async command), since the
    /// stop may take its grace.
    pub fn restart(&self) -> Result<NodeStatus, String> {
        let (child, leash) = {
            let mut inner = self.lock();
            if inner.external {
                return Err("node is external (BISA_API_BASE) — restart it yourself".into());
            }
            inner.expected_exit = true;
            inner.booting = None;
            inner.boot = None;
            inner.tail = None;
            (inner.child.take(), inner.leash.take())
        };
        if let Some(mut child) = child {
            stop(&mut child, leash, RESTART_GRACE);
        }
        {
            let mut inner = self.lock();
            inner.expected_exit = false;
            inner.crashes = 0;
            inner.failed = None;
        }
        self.launch(true, true);
        Ok(self.status())
    }

    /// The quit: the child asked to go and given its grace, then killed;
    /// the record of its spawn removed, since nothing is left to take back.
    pub fn shutdown(&self) {
        let (child, leash, dirs) = {
            let mut inner = self.lock();
            inner.expected_exit = true;
            (inner.child.take(), inner.leash.take(), inner.dirs.clone())
        };
        if let Some(mut child) = child {
            stop(&mut child, leash, QUIT_GRACE);
        }
        if let Some(dirs) = dirs {
            remove_own_record(&dirs.data_dir);
        }
    }
}

impl Drop for NodeState {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// After a restart the node names the workspace again: the shell's log is
/// attached there when nothing was writing yet — the first launch, before
/// the workspace existed — or when the folder moved.
pub(crate) fn reattach_log<R: tauri::Runtime>(app: &tauri::AppHandle<R>, state: &NodeState) {
    let Ok(base) = state.api_base() else {
        return;
    };
    let log = app.state::<Handle>();
    if let Err(e) = crate::logging::attach_from_node(&log, &base, &state.api_token()) {
        tracing::warn!(target: "bisa_desktop", "no log file after the restart: {e}");
    }
}

/// Whether a booting node has gone silent past its budget: nothing heard —
/// no boot word, no answer — for [`BOOT_BUDGET`] since its last word or its
/// spawn, or the whole boot past [`BOOT_CAP`]. Pure.
pub(crate) fn boot_budget_exhausted(
    now: Instant,
    spawned: Instant,
    last_word: Option<Instant>,
) -> bool {
    let heard = last_word.map_or(spawned, |w| w.max(spawned));
    now.saturating_duration_since(heard) > BOOT_BUDGET
        || now.saturating_duration_since(spawned) > BOOT_CAP
}

/// The candidate tried after `current`, round the list.
pub(crate) fn next_candidate(current: usize, count: usize) -> usize {
    if count == 0 {
        0
    } else {
        (current + 1) % count
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// The own record and the reclaim
// ---------------------------------------------------------------------------

/// The node this desktop spawned, as the next launch reads it:
/// `<data_dir>/run/desktop-node.json`. Written when the node answers,
/// removed at the quit — so a record still there at a launch names a node
/// a Force Quit left behind.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) struct OwnRecord {
    pub pid: u32,
    /// Unix seconds at the spawn.
    pub spawned_at: u64,
    pub desktop_pid: u32,
    pub port: u16,
}

impl OwnRecord {
    fn path(data_dir: &Path) -> PathBuf {
        data_dir.join("run").join("desktop-node.json")
    }

    fn read(data_dir: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(Self::path(data_dir)).ok()?;
        serde_json::from_str(&text).ok()
    }

    fn write(&self, data_dir: &Path) -> Result<(), String> {
        let path = Self::path(data_dir);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
    }
}

fn remove_own_record(data_dir: &Path) {
    if let Err(e) = std::fs::remove_file(OwnRecord::path(data_dir)) {
        if e.kind() != std::io::ErrorKind::NotFound {
            tracing::debug!(target: "bisa_desktop", "the node's record could not be removed: {e}");
        }
    }
}

/// What the launch does about the workspace's holder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reclaim {
    /// Nobody holds it: spawn.
    Spawn,
    /// A node this desktop started earlier holds it: stop that one, then spawn.
    StopOurs(u32),
    /// Somebody else's process holds it: spawn nothing, say so.
    HeldByOther(u32),
}

/// What the process table says of a pid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProcessFacts {
    pub name: String,
    /// Unix seconds at the process's start.
    pub started_at: u64,
}

/// The decision, pure: the holder the binary named, the record of the node
/// this desktop spawned, and what the process table says of a pid. A holder
/// is ours only when the record names its pid, the process is a `bisa`, and
/// it started when the record says it was spawned — a pid the OS reused is
/// never ours to signal.
pub(crate) fn reclaim_decision(
    holder: Option<crate::logging::Holder>,
    own: Option<&OwnRecord>,
    facts: impl Fn(u32) -> Option<ProcessFacts>,
) -> Reclaim {
    let Some(holder) = holder else {
        return Reclaim::Spawn;
    };
    let Some(own) = own.filter(|own| own.pid == holder.pid) else {
        return Reclaim::HeldByOther(holder.pid);
    };
    match facts(holder.pid) {
        Some(facts)
            if facts.name.contains("bisa")
                && facts.started_at.abs_diff(own.spawned_at) <= RECLAIM_START_SLACK =>
        {
            Reclaim::StopOurs(holder.pid)
        }
        // A lock held by a pid the table does not know, or by something that
        // is not a bisa node, is not ours — whatever the record says.
        _ => Reclaim::HeldByOther(holder.pid),
    }
}

/// The reclaim for one launch: the binary asked who holds the workspace,
/// the own record read beside it. A binary that cannot answer, or no
/// folders, is a spawn — the node's own refusal says the rest.
fn reclaim(bin: &Path, dirs: Option<&WorkspaceDirs>) -> Reclaim {
    let Some(dirs) = dirs else {
        return Reclaim::Spawn;
    };
    let holder = match crate::logging::ask_paths(bin) {
        Ok(answer) => answer.engine_holder,
        Err(e) => {
            tracing::debug!(target: "bisa_desktop", "the holder could not be asked: {e}");
            return Reclaim::Spawn;
        }
    };
    let own = OwnRecord::read(&dirs.data_dir);
    let decision = reclaim_decision(holder, own.as_ref(), process_facts);
    if decision == Reclaim::Spawn && own.is_some() {
        // A record with nobody behind it is stale.
        remove_own_record(&dirs.data_dir);
    }
    decision
}

fn process_facts(pid: u32) -> Option<ProcessFacts> {
    let sys = crate::ports::processes();
    sys.process(sysinfo::Pid::from_u32(pid))
        .map(|p| ProcessFacts {
            name: p.name().to_string_lossy().into_owned(),
            started_at: p.start_time(),
        })
}

/// Stop a stray node of this desktop's own: asked first, killed when it
/// does not go.
fn stop_stray(pid: u32) {
    let alive = |pid: u32| process_facts(pid).is_some();
    if !crate::ports::signal(pid, sysinfo::Signal::Term) {
        return;
    }
    let deadline = Instant::now() + RECLAIM_TERM_GRACE;
    while alive(pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(200));
    }
    if alive(pid) {
        tracing::warn!(target: "bisa_desktop", pid, "the stray node did not stop when asked; killing it");
        crate::ports::signal(pid, sysinfo::Signal::Kill);
        let deadline = Instant::now() + RECLAIM_KILL_GRACE;
        while alive(pid) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

// ---------------------------------------------------------------------------
// The child
// ---------------------------------------------------------------------------

/// How the child ended: its exit code, or the signal that ended it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Exit {
    pub code: Option<i32>,
    pub signal: Option<i32>,
}

impl Exit {
    fn of(status: &std::process::ExitStatus) -> Self {
        #[cfg(unix)]
        let signal = {
            use std::os::unix::process::ExitStatusExt as _;
            status.signal()
        };
        #[cfg(not(unix))]
        let signal = None;
        Self {
            code: status.code(),
            signal,
        }
    }

    /// The child could not be read at all.
    fn unknown() -> Self {
        Self {
            code: None,
            signal: None,
        }
    }

    /// *exited with code 101* · *was ended by signal 11* · *ended, how is
    /// unknown*.
    pub(crate) fn words(&self) -> String {
        match (self.code, self.signal) {
            (Some(code), _) => format!("exited with code {code}"),
            (None, Some(signal)) => format!("was ended by signal {signal}"),
            (None, None) => "ended, how is unknown".to_string(),
        }
    }
}

/// An exit the shell did not ask for, with what the node last said.
pub(crate) struct NodeExit {
    pub pid: u32,
    pub exit: Exit,
    /// The last stderr lines, oldest first.
    pub stderr: Vec<String>,
}

impl NodeExit {
    fn words(&self) -> String {
        self.exit.words()
    }

    /// The error line and the crash report of an unasked-for exit.
    fn log(&self, attempt: u32) {
        tracing::error!(
            target: "bisa_desktop",
            pid = self.pid,
            code = ?self.exit.code,
            signal = ?self.exit.signal,
            stderr_tail = ?self.stderr,
            attempt,
            "the node {}; restarting it",
            self.words()
        );
        self.report();
    }

    /// One crash report of kind `child_exit`, through the process's log. A
    /// report that cannot be written is said and costs nothing else.
    fn report(&self) {
        let report = CrashReport::new(
            CrashKind::ChildExit,
            format!("the node (pid {}) {}", self.pid, self.words()),
        )
        .with_child(ChildExit {
            process: Process::Node,
            pid: self.pid,
            code: self.exit.code,
            signal: self.exit.signal,
            stderr: self.stderr.clone(),
        });
        let Some(log) = bisa_log::current() else {
            tracing::warn!(target: "bisa_desktop", "no crash report for the node's exit: no log is installed");
            return;
        };
        if let Err(e) = log.report_crash(report) {
            tracing::warn!(target: "bisa_desktop", "no crash report for the node's exit: {e}");
        }
    }
}

/// How many of the child's stderr lines the shell keeps.
pub(crate) const TAIL_LINES: usize = 64;

/// The child's last stderr lines, kept by the thread that reads them. The
/// node's own words go to its own file; this is what it printed *outside*
/// the log — a panic's message, an abort's last line (*thread 'main' has
/// overflowed its stack*, *memory allocation of N bytes failed*), a loader's
/// refusal — which is exactly what a death that wrote no line left behind.
#[derive(Clone, Default)]
pub(crate) struct StderrTail {
    lines: Arc<Mutex<VecDeque<String>>>,
}

impl StderrTail {
    pub(crate) fn push(&self, line: String) {
        let mut lines = self.lines.locked();
        if lines.len() == TAIL_LINES {
            lines.pop_front();
        }
        lines.push_back(line);
    }

    /// The lines, oldest first.
    pub(crate) fn lines(&self) -> Vec<String> {
        self.lines.locked().iter().cloned().collect()
    }

    /// One thread for the child's stderr: every line into the ring and,
    /// at `debug`, into the shell's own log. Ends with the pipe.
    fn follow(self, stderr: std::process::ChildStderr) {
        let spawned = std::thread::Builder::new()
            .name("node-stderr".into())
            .spawn(move || {
                for line in BufReader::new(stderr).lines() {
                    let Ok(line) = line else {
                        break;
                    };
                    tracing::debug!(target: "bisa_desktop::node", "{line}");
                    self.push(line);
                }
            });
        if let Err(e) = spawned {
            tracing::warn!(target: "bisa_desktop", "the node's stderr is not followed: {e}");
        }
    }
}

/// What the child's stdout said about its boot, kept by the thread that
/// reads it: the last word and when it came, and a sequence so the watchdog
/// relays each word once. The pipe is always drained, so a chatty node never
/// blocks on it.
#[derive(Clone, Default)]
pub(crate) struct BootTail {
    seen: Arc<Mutex<BootSeen>>,
}

#[derive(Default)]
struct BootSeen {
    seq: u64,
    last: Option<BootWord>,
    last_at: Option<Instant>,
}

impl BootTail {
    fn push(&self, word: BootWord) {
        let mut seen = self.seen.locked();
        seen.seq += 1;
        seen.last = Some(word);
        seen.last_at = Some(Instant::now());
    }

    /// The last word and its sequence.
    fn latest(&self) -> Option<(u64, BootWord)> {
        let seen = self.seen.locked();
        seen.last.clone().map(|w| (seen.seq, w))
    }

    /// When the last word came.
    fn last_word_at(&self) -> Option<Instant> {
        self.seen.locked().last_at
    }

    /// One thread for the child's stdout: every `{"boot": …}` line into the
    /// tail; every other line — the socket line, a verb's answer — read and
    /// let go. Ends with the pipe.
    fn follow(self, stdout: ChildStdout) {
        let spawned = std::thread::Builder::new()
            .name("node-stdout".into())
            .spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else {
                        break;
                    };
                    if let Some(word) = boot_of(&line) {
                        tracing::info!(target: "bisa_desktop::node", phase = %word.phase, done = ?word.done, of = ?word.of, "the node's boot");
                        self.push(word);
                    }
                }
            });
        if let Err(e) = spawned {
            tracing::warn!(target: "bisa_desktop", "the node's stdout is not followed: {e}");
        }
    }
}

/// The boot word a line of the node's stdout carries, or none for a line
/// about something else.
pub(crate) fn boot_of(line: &str) -> Option<BootWord> {
    let value: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    let boot = value.get("boot")?;
    let phase = boot.get("phase")?.as_str()?.to_string();
    Some(BootWord {
        phase,
        done: boot.get("done").and_then(|v| v.as_u64()),
        of: boot.get("of").and_then(|v| v.as_u64()),
    })
}

/// *; it said: …* — the child's last lines, for a reason the webview shows.
/// Nothing when it said nothing.
fn said_words(lines: &[String]) -> String {
    if lines.is_empty() {
        return String::new();
    }
    format!("; it said: {}", lines.join(" | "))
}

/// A child spawned and left to boot, with its port, its tails and its leash.
pub(crate) struct Spawned {
    pub child: Child,
    pub port: u16,
    pub tail: StderrTail,
    pub boot: BootTail,
    pub leash: ChildStdin,
}

/// 32 random bytes as hex, from the OS. No dependency: the platform's own
/// randomness is what every key on this machine already comes from.
fn mint_token() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(|e| format!("could not read randomness for the API token: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Spawn the node with `bin`, leashed and speaking its boot: `keep` is the
/// port a restart keeps — the webview's cached base and every open terminal
/// point at it — taken back when it is still free, else a fresh one. The
/// child is not waited for: the watchdog hears its first answer.
fn spawn_node(bin: &Path, token: &str, keep: Option<u16>) -> Result<Spawned, String> {
    let port = match chosen_port(keep, port_is_free) {
        Some(p) => p,
        None => free_port()?,
    };
    let listen = format!("127.0.0.1:{port}");
    // The node probes for harnesses with a bare `PATH` lookup, and an app
    // launched from Finder inherits launchd's `PATH` rather than a shell's —
    // so without this every harness reports as not installed in a bundled app
    // while working fine in a terminal. `None` keeps whatever we inherited.
    // See `login_env`.
    let login_path = crate::login_env::login_path();
    let mut cmd = Command::new(bin);
    // `--json`, so the boot's phases come as lines on stdout; the leash,
    // so the node stops when this shell's end of its stdin goes; stderr is
    // what a death that wrote no log line leaves behind, read to the last line.
    cmd.args(["--json", "node", "--listen", &listen])
        .env("BISA_API_TOKEN", token)
        .env("BISA_STOP_ON_STDIN_CLOSE", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(path) = &login_path {
        cmd.env("PATH", path);
    }
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let leash = child
        .stdin
        .take()
        .ok_or_else(|| "the node's stdin was not piped".to_string())?;
    let tail = StderrTail::default();
    if let Some(stderr) = child.stderr.take() {
        tail.clone().follow(stderr);
    }
    let boot = BootTail::default();
    if let Some(stdout) = child.stdout.take() {
        boot.clone().follow(stdout);
    }
    Ok(Spawned {
        child,
        port,
        tail,
        boot,
        leash,
    })
}

/// The port a restart keeps when it is still free; none means a fresh one.
fn chosen_port(keep: Option<u16>, is_free: impl Fn(u16) -> bool) -> Option<u16> {
    keep.filter(|p| is_free(*p))
}

pub(crate) fn candidate_binaries() -> Vec<PathBuf> {
    candidates_from(
        std::env::var_os("BISA_BIN").map(PathBuf::from),
        std::env::current_exe().ok().as_deref(),
        std::env::current_dir().ok().as_deref(),
        |p| p.is_file(),
    )
}

/// Where the node may be, in the order it is tried: `BISA_BIN` when set, the
/// `bisa` beside this executable (the bundle's — `Contents/MacOS/` on macOS,
/// where `scripts/macos/lib.sh` puts it — so the application is whole
/// without anything on the machine's PATH), the bare name for PATH's own
/// resolution, then the repository's `target/debug/bisa` above the executable
/// and above the working directory. `is_file` is the disk, handed in so the
/// order is held without one.
fn candidates_from(
    bisa_bin: Option<PathBuf>,
    exe: Option<&std::path::Path>,
    cwd: Option<&std::path::Path>,
    is_file: impl Fn(&std::path::Path) -> bool,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(p) = bisa_bin {
        out.push(p);
    }
    if let Some(dir) = exe.and_then(|e| e.parent()) {
        let cand = dir.join("bisa");
        if is_file(&cand) {
            out.push(cand);
        }
    }
    out.push(PathBuf::from("bisa"));
    if let Some(exe) = exe {
        for anc in exe.ancestors().skip(1).take(6) {
            let cand = anc.join("target/debug/bisa");
            if is_file(&cand) {
                out.push(cand);
            }
        }
    }
    if let Some(cwd) = cwd {
        for anc in cwd.ancestors().take(4) {
            let cand = anc.join("target/debug/bisa");
            if is_file(&cand) {
                out.push(cand);
            }
        }
    }
    // A binary reached two ways is tried once.
    let mut seen = std::collections::HashSet::new();
    out.retain(|p| seen.insert(p.clone()));
    out
}

fn port_is_free(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

fn free_port() -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    drop(listener);
    Ok(port)
}

/// End a node this shell spawned, gracefully, and reap it: the leash let
/// go — the node's own cue to stop — and `SIGTERM` beside it for a binary
/// without the leash; then, past `grace`, the kill. An end that fails is a
/// child already gone; a wait that fails is one already reaped.
fn stop(child: &mut Child, leash: Option<ChildStdin>, grace: Duration) {
    drop(leash);
    let pid = child.id();
    if !crate::ports::signal(pid, sysinfo::Signal::Term) {
        tracing::debug!(target: "bisa_desktop::node", pid, "the node took no SIGTERM; it may be gone already");
    }
    let deadline = Instant::now() + grace;
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(_) => break,
        }
    }
    tracing::warn!(target: "bisa_desktop::node", pid, grace_secs = grace.as_secs(), "the node did not stop when asked; killing it");
    if let Err(e) = child.kill() {
        tracing::debug!(target: "bisa_desktop::node", "ending the node: {e}");
    }
    if let Err(e) = child.wait() {
        tracing::debug!(target: "bisa_desktop::node", "reaping the node: {e}");
    }
}

/// One minimal HTTP health probe over a raw TcpStream (loopback only) —
/// avoids an HTTP client dependency in the shell. True on a `200`.
fn probe_health(port: u16) -> bool {
    let Ok(mut s) = TcpStream::connect(("127.0.0.1", port)) else {
        return false;
    };
    let req =
        format!("GET /health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    // A socket that cannot be bounded is not read: the next tick asks again.
    if s.set_read_timeout(Some(Duration::from_secs(2))).is_err()
        || s.write_all(req.as_bytes()).is_err()
    {
        return false;
    }
    let mut buf = String::new();
    if let Err(e) = s.read_to_string(&mut buf) {
        tracing::debug!(target: "bisa_desktop::node", "reading /health: {e}");
    }
    buf.starts_with("HTTP/1.1 200") || buf.starts_with("HTTP/1.0 200")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inner_without_a_node(failed: Option<Failure>) -> NodeInner {
        NodeInner {
            child: None,
            leash: None,
            tail: None,
            boot: None,
            port: 0,
            api_base: String::new(),
            token: "t".into(),
            external: false,
            expected_exit: false,
            crashes: 0,
            healthy_since: None,
            booting: None,
            failed,
            candidates: Vec::new(),
            next: 0,
            dirs: None,
            relayed: 0,
        }
    }

    /// A shell whose node did not start still has a state: the webview is
    /// told why instead of a base, and the watchdog owes a start. Once a
    /// child runs, the reason is gone.
    #[test]
    fn a_failed_start_says_why_and_is_owed_a_retry() {
        let state = NodeState {
            inner: Mutex::new(inner_without_a_node(Some(Failure::NoBinary {
                tried: vec!["no binary".into()],
            }))),
        };
        assert!(matches!(state.watch(), Watch::NeedsStart { .. }));
        let said = state.api_base().unwrap_err();
        assert!(
            said.starts_with("the node did not start: ") && said.contains("no binary"),
            "{said}"
        );
        let status = state.status();
        assert!(!status.running && !status.booting);
        assert!(matches!(status.failure, Some(Failure::NoBinary { .. })));
        {
            let mut inner = state.lock();
            inner.failed = None;
            inner.api_base = "http://127.0.0.1:4477".into();
            inner.port = 4477;
        }
        assert!(matches!(state.watch(), Watch::Nothing));
        assert_eq!(state.api_base().unwrap(), "http://127.0.0.1:4477");
    }

    /// Somebody else's node is asked after on a steady beat, never on the
    /// doubling curve — it is not a crash — and never signalled; a crash
    /// counts, and the delay doubles with it.
    #[test]
    fn a_held_workspace_is_polled_steadily_and_a_crash_doubles_the_wait() {
        let mut inner = inner_without_a_node(None);
        inner.crashes = 3;
        let said = inner.fail(Failure::HeldByOther { pid: 9 });
        assert_eq!(said.next_in_secs, HELD_POLL.as_secs());
        assert_eq!(said.attempt, 3, "the count is left alone");
        let said = inner.fail(Failure::Exited {
            how: "exited with code 1".into(),
            said: vec![],
        });
        assert_eq!(said.next_in_secs, restart_delay(3).as_secs());
        let state = NodeState {
            inner: Mutex::new(inner),
        };
        assert!(matches!(state.watch(), Watch::NeedsStart { delay } if delay == restart_delay(3)));
    }

    #[test]
    fn a_failure_reads_as_one_sentence_and_carries_its_kind_on_the_wire() {
        let exited = Failure::Exited {
            how: "exited with code 1".into(),
            said: vec!["refused".into()],
        };
        assert_eq!(
            exited.words(),
            "the node exited with code 1 before answering its health check; it said: refused"
        );
        let json = serde_json::to_value(&exited).unwrap();
        assert_eq!(json["kind"], "exited");
        assert_eq!(
            Failure::TimedOut {
                secs: 190,
                said: vec![]
            }
            .words(),
            "the node did not answer within 190 s"
        );
        assert_eq!(
            Failure::HeldByOther { pid: 42 }.words(),
            "another node holds this workspace (pid 42)"
        );
        assert_eq!(
            serde_json::to_value(NodeFailed {
                failure: Failure::HeldByOther { pid: 42 },
                attempt: 3,
                next_in_secs: 8
            })
            .unwrap(),
            serde_json::json!({"kind": "held_by_other", "pid": 42, "attempt": 3, "next_in_secs": 8})
        );
    }

    #[test]
    fn a_boot_word_is_read_from_the_nodes_json_line_and_other_lines_are_not() {
        assert_eq!(
            boot_of(r#"{"boot":{"phase":"rebuilding_index","done":25,"of":300}}"#),
            Some(BootWord {
                phase: "rebuilding_index".into(),
                done: Some(25),
                of: Some(300)
            })
        );
        assert_eq!(
            boot_of(r#"{"boot":{"phase":"opening_workspace","done":null,"of":null}}"#),
            Some(BootWord {
                phase: "opening_workspace".into(),
                done: None,
                of: None
            })
        );
        assert_eq!(
            boot_of(r#"{"socket":"/x/node.sock","listen":"127.0.0.1:1"}"#),
            None
        );
        assert_eq!(boot_of("not json"), None);
        assert_eq!(boot_of(""), None);
        let tail = BootTail::default();
        assert_eq!(tail.latest(), None);
        tail.push(boot_of(r#"{"boot":{"phase":"opening_workspace"}}"#).unwrap());
        tail.push(boot_of(r#"{"boot":{"phase":"starting_engine"}}"#).unwrap());
        let (seq, word) = tail.latest().unwrap();
        assert_eq!((seq, word.phase.as_str()), (2, "starting_engine"));
        assert!(tail.last_word_at().is_some());
    }

    #[test]
    fn the_boot_budget_runs_from_the_last_word_and_the_cap_from_the_spawn() {
        let spawned = Instant::now();
        let soon = spawned + Duration::from_secs(10);
        assert!(!boot_budget_exhausted(soon, spawned, None));
        let silent = spawned + BOOT_BUDGET + Duration::from_secs(1);
        assert!(
            boot_budget_exhausted(silent, spawned, None),
            "silent past the budget"
        );
        let spoke = spawned + BOOT_BUDGET - Duration::from_secs(1);
        assert!(
            !boot_budget_exhausted(silent, spawned, Some(spoke)),
            "a word within the budget keeps it alive"
        );
        let late = spawned + BOOT_CAP + Duration::from_secs(1);
        assert!(
            boot_budget_exhausted(late, spawned, Some(late)),
            "the cap holds whatever is said"
        );
        assert_eq!(next_candidate(0, 3), 1);
        assert_eq!(next_candidate(2, 3), 0, "round the list");
        assert_eq!(next_candidate(5, 0), 0);
    }

    #[test]
    fn the_reclaim_stops_only_a_node_the_record_names_and_the_table_confirms() {
        use crate::logging::Holder;
        let own = OwnRecord {
            pid: 500,
            spawned_at: 1_000,
            desktop_pid: 7,
            port: 4477,
        };
        let bisa_at = |started_at: u64| {
            move |pid: u32| {
                (pid == 500).then(|| ProcessFacts {
                    name: "bisa".into(),
                    started_at,
                })
            }
        };
        assert_eq!(
            reclaim_decision(None, Some(&own), bisa_at(1_000)),
            Reclaim::Spawn
        );
        let holder = |pid| {
            Some(Holder {
                pid,
                started_at: 1_000,
            })
        };
        assert_eq!(
            reclaim_decision(holder(500), Some(&own), bisa_at(1_050)),
            Reclaim::StopOurs(500),
            "ours: the record's pid, a bisa, started when the record says"
        );
        assert_eq!(
            reclaim_decision(holder(500), Some(&own), bisa_at(5_000)),
            Reclaim::HeldByOther(500),
            "a pid the OS reused for another bisa is not ours"
        );
        assert_eq!(
            reclaim_decision(holder(500), Some(&own), |_| Some(ProcessFacts {
                name: "python".into(),
                started_at: 1_000
            })),
            Reclaim::HeldByOther(500),
            "not a bisa"
        );
        assert_eq!(
            reclaim_decision(holder(500), Some(&own), |_| None),
            Reclaim::HeldByOther(500),
            "a holder the table does not know is left alone"
        );
        assert_eq!(
            reclaim_decision(holder(501), Some(&own), bisa_at(1_000)),
            Reclaim::HeldByOther(501),
            "a person's own node"
        );
        assert_eq!(
            reclaim_decision(holder(500), None, bisa_at(1_000)),
            Reclaim::HeldByOther(500),
            "no record: nothing is ours"
        );
    }

    #[test]
    fn the_own_record_is_written_under_run_read_back_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(OwnRecord::read(dir.path()), None);
        let record = OwnRecord {
            pid: 500,
            spawned_at: 1_000,
            desktop_pid: 7,
            port: 4477,
        };
        record.write(dir.path()).unwrap();
        assert_eq!(
            OwnRecord::path(dir.path()),
            dir.path().join("run").join("desktop-node.json")
        );
        assert_eq!(OwnRecord::read(dir.path()), Some(record));
        remove_own_record(dir.path());
        assert_eq!(OwnRecord::read(dir.path()), None);
        remove_own_record(dir.path());
    }

    #[test]
    fn the_tail_keeps_the_last_lines_and_an_exit_has_its_words() {
        let tail = StderrTail::default();
        for i in 0..(TAIL_LINES + 3) {
            tail.push(format!("line {i}"));
        }
        let lines = tail.lines();
        assert_eq!(lines.len(), TAIL_LINES);
        assert_eq!(lines[0], "line 3");
        assert_eq!(lines[TAIL_LINES - 1], format!("line {}", TAIL_LINES + 2));
        assert_eq!(said_words(&[]), "");
        assert_eq!(
            said_words(&["a".to_string(), "b".to_string()]),
            "; it said: a | b"
        );

        assert_eq!(
            Exit {
                code: Some(101),
                signal: None
            }
            .words(),
            "exited with code 101"
        );
        assert_eq!(
            Exit {
                code: None,
                signal: Some(11)
            }
            .words(),
            "was ended by signal 11"
        );
        assert_eq!(Exit::unknown().words(), "ended, how is unknown");
    }

    #[test]
    fn the_token_is_thirty_two_random_bytes_as_hex_and_never_the_same_twice() {
        let a = mint_token().unwrap();
        let b = mint_token().unwrap();
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn the_node_is_looked_for_in_order_bisa_bin_the_bundle_the_path_then_the_repository() {
        use std::path::{Path, PathBuf};
        let exe = Path::new("/repo/desktop/src-tauri/target/debug/app");
        let cwd = Path::new("/repo/desktop");
        let on_disk = |p: &Path| {
            p == Path::new("/repo/desktop/src-tauri/target/debug/bisa")
                || p == Path::new("/repo/target/debug/bisa")
        };
        let found = candidates_from(
            Some(PathBuf::from("/opt/bisa")),
            Some(exe),
            Some(cwd),
            on_disk,
        );
        assert_eq!(
            found,
            vec![
                PathBuf::from("/opt/bisa"),
                PathBuf::from("/repo/desktop/src-tauri/target/debug/bisa"),
                PathBuf::from("bisa"),
                PathBuf::from("/repo/target/debug/bisa"),
            ],
            "the environment's word, the bundle's neighbour, PATH, then the repository above the exe and above the cwd — each once"
        );
        let bare = candidates_from(None, None, None, |_| false);
        assert_eq!(
            bare,
            vec![PathBuf::from("bisa")],
            "nothing on disk and nothing set: PATH alone"
        );
        let bundle = candidates_from(
            None,
            Some(Path::new("/Applications/Bisa.app/Contents/MacOS/app")),
            None,
            |p| p == Path::new("/Applications/Bisa.app/Contents/MacOS/bisa"),
        );
        assert_eq!(
            bundle[0],
            PathBuf::from("/Applications/Bisa.app/Contents/MacOS/bisa"),
            "the bundle's node comes before PATH"
        );
    }

    #[test]
    fn a_restart_keeps_its_port_while_it_is_free_and_takes_a_fresh_one_otherwise() {
        assert_eq!(chosen_port(Some(4477), |_| true), Some(4477));
        assert_eq!(
            chosen_port(Some(4477), |_| false),
            None,
            "taken: a fresh one"
        );
        assert_eq!(
            chosen_port(None, |_| true),
            None,
            "a first start has nothing to keep"
        );
        let port = free_port().unwrap();
        assert!(port_is_free(port), "a port just handed out is free again");
        let held = TcpListener::bind("127.0.0.1:0").unwrap();
        let taken = held.local_addr().unwrap().port();
        assert!(
            !port_is_free(taken),
            "a port this process holds is not free"
        );
        assert_eq!(chosen_port(Some(taken), port_is_free), None);
    }

    /// A loopback listener answering `status` to the next few requests.
    fn answer(status: &str) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let reply = format!(
            "HTTP/1.1 {status}
Content-Length: 0
Connection: close

"
        );
        std::thread::spawn(move || {
            for mut s in listener.incoming().take(3).flatten() {
                let mut buf = [0u8; 512];
                let _request = s.read(&mut buf);
                s.write_all(reply.as_bytes())
                    .expect("the shell reads the reply");
            }
        });
        port
    }

    #[test]
    fn the_health_probe_takes_a_200_on_loopback_and_nothing_else() {
        assert!(probe_health(answer("200 OK")));
        assert!(!probe_health(answer("503 Service Unavailable")));
        assert!(!probe_health(free_port().unwrap()), "nobody listening");
    }

    #[test]
    fn the_restart_delay_doubles_from_one_second_to_thirty_and_stays_there() {
        assert_eq!(restart_delay(1), Duration::from_secs(2));
        assert_eq!(restart_delay(2), Duration::from_secs(4));
        assert_eq!(restart_delay(3), Duration::from_secs(8));
        assert_eq!(restart_delay(4), Duration::from_secs(16));
        assert_eq!(restart_delay(5), Duration::from_secs(30));
        assert_eq!(restart_delay(6), Duration::from_secs(30));
        assert_eq!(restart_delay(u32::MAX), Duration::from_secs(30));
        assert_eq!(
            restart_delay(0),
            Duration::from_secs(1),
            "the first respawn is prompt"
        );
    }
}
