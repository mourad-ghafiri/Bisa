//! Sidecar management: spawn the `bisa` binary as a local node and keep
//! it alive for the lifetime of the app — *keep*: a watchdog thread reads the
//! child once a second and, when it exits without being asked, starts it
//! again on the **same port** after a delay that doubles to thirty seconds
//! and resets once the node has been healthy for a minute. Every respawn
//! reaches the webview as the `node:restarted` event, so the page reads its
//! lists again and the node's own restart recovery (the engine's boot walk)
//! is what the person sees on the goal, not a dead stream.
//!
//! Resolution order for the API base:
//! 1. `BISA_API_BASE` env — use an already-running node, spawn nothing
//!    (UI development against a live daemon).
//! 2. Spawn `bisa node --listen 127.0.0.1:<free port>`, binary found via
//!    `BISA_BIN` env → the node the bundle carries beside this executable
//!    (`scripts/macos/lib.sh` puts it there) → `PATH` → `../target/debug/bisa` (repo dev).
//!
//! The node is started from the app's `setup` ([`boot`]), never from
//! `main`: a second launch of the app leaves `build()` through the
//! single-instance plugin (`second_launch.rs`), and a node spawned before
//! that would be nobody's. A child that ends before it answers — a node
//! refused the workspace another engine holds, a loader's refusal — ends
//! the wait at once, with what it said, rather than at the twenty-second
//! deadline.

use crate::sync::Locked;
use bisa_log::{ChildExit, CrashKind, CrashReport, Handle, Process};
use serde::Serialize;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};

pub struct NodeState {
    inner: Mutex<NodeInner>,
}

struct NodeInner {
    child: Option<Child>,
    /// The child's last stderr lines, kept by the thread that reads them —
    /// what a crash report says the node said before it died.
    tail: Option<StderrTail>,
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
    /// How many times in a row the watchdog has had to respawn; the delay
    /// doubles with it and it resets after a healthy minute.
    crashes: u32,
    /// When the current child was seen healthy first — the minute counts from here.
    healthy_since: Option<Instant>,
    /// Why the last spawn failed, while no child is running. The window
    /// opens all the same, the webview says this, and the watchdog keeps
    /// trying — a node that cannot start must never be an app that vanishes.
    failed: Option<String>,
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

/// The event name the webview listens for.
pub const NODE_RESTARTED: &str = "node:restarted";

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

/// The boot, from `setup` and never from `main` (one Bisa at a time,
/// `second_launch.rs`): the log file first, under the workspace the `bisa`
/// binary names, so a node that fails to start is a line in the file and
/// not on a stderr nobody reads; then the node; then the file again from
/// the node's own word, when nothing was writing yet — the node made the
/// workspace if it was not there. A node that cannot start is not a reason
/// for no window: the state says why, the shell opens, and the watchdog
/// keeps trying ([`NodeState::supervise`]).
pub fn boot<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> NodeState {
    let log = app.state::<Handle>();
    match crate::logging::attach_from_binary(&log) {
        Ok(dir) => {
            tracing::debug!(target: "bisa_desktop", dir = %dir.display(), "log folder named by the binary")
        }
        Err(e) => tracing::warn!(target: "bisa_desktop", "no log folder from the binary: {e}"),
    }
    let node = NodeState::start();
    match node.api_base() {
        Ok(base) => {
            if let Err(e) = crate::logging::attach_from_node(&log, &base, &node.api_token()) {
                tracing::warn!(target: "bisa_desktop", "no log file: {e}");
            }
            tracing::info!(
                target: "bisa_desktop",
                version = env!("CARGO_PKG_VERSION"),
                node = %base,
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

impl NodeState {
    /// The state, poison-tolerant: a thread that panicked while holding it
    /// (the watchdog's, a command's) must not take the supervisor with it.
    fn lock(&self) -> std::sync::MutexGuard<'_, NodeInner> {
        self.inner.locked()
    }

    /// Start the node, or record why it could not: the shell opens either
    /// way, and [`supervise`](Self::supervise) keeps trying.
    pub fn start() -> Self {
        match Self::try_start() {
            Ok(n) => n,
            Err((token, reason)) => {
                tracing::error!(target: "bisa_desktop", "the node did not start: {reason}");
                Self {
                    inner: Mutex::new(NodeInner {
                        child: None,
                        tail: None,
                        port: 0,
                        api_base: String::new(),
                        token,
                        external: false,
                        expected_exit: false,
                        crashes: 0,
                        healthy_since: None,
                        failed: Some(reason),
                    }),
                }
            }
        }
    }

    /// One attempt. A failure carries the token minted for it, so the
    /// retries spawn with the same one the webview will be handed.
    fn try_start() -> Result<Self, (String, String)> {
        if let Ok(base) = std::env::var("BISA_API_BASE") {
            let base = base.trim_end_matches('/').to_string();
            let port = base
                .rsplit(':')
                .next()
                .and_then(|p| p.parse().ok())
                .unwrap_or(0);
            return Ok(Self {
                inner: Mutex::new(NodeInner {
                    child: None,
                    tail: None,
                    port,
                    api_base: base,
                    token: std::env::var("BISA_API_TOKEN").unwrap_or_default(),
                    external: true,
                    expected_exit: false,
                    crashes: 0,
                    healthy_since: None,
                    failed: None,
                }),
            });
        }
        let token = mint_token().map_err(|e| (String::new(), e))?;
        let spawned = spawn_node(&token, None).map_err(|e| (token.clone(), e))?;
        let port = spawned.port;
        Ok(Self {
            inner: Mutex::new(NodeInner {
                child: Some(spawned.child),
                tail: Some(spawned.tail),
                port: spawned.port,
                api_base: format!("http://127.0.0.1:{port}"),
                token,
                external: false,
                expected_exit: false,
                crashes: 0,
                healthy_since: Some(Instant::now()),
                failed: None,
            }),
        })
    }

    /// No child and no external node: the last spawn failed and the
    /// watchdog owes a retry.
    fn needs_start(&self) -> bool {
        let inner = self.lock();
        !inner.external && inner.child.is_none() && inner.failed.is_some()
    }

    /// Whether the child this shell spawned has exited on its own, and how.
    /// `None` when there is nothing to watch: an external node, or a child
    /// the shell itself asked to go.
    fn unexpected_exit(&self) -> Option<NodeExit> {
        let mut inner = self.lock();
        if inner.external || inner.expected_exit {
            return None;
        }
        let c = inner.child.as_mut()?;
        let (pid, exit) = match c.try_wait() {
            Ok(Some(status)) => (c.id(), Exit::of(&status)),
            Ok(None) => {
                if let Some(since) = inner.healthy_since {
                    if since.elapsed() >= HEALTHY_AFTER {
                        inner.crashes = 0;
                    }
                }
                return None;
            }
            Err(_) => (c.id(), Exit::unknown()),
        };
        inner.child = None;
        let stderr = inner.tail.take().map(|t| t.lines()).unwrap_or_default();
        Some(NodeExit { pid, exit, stderr })
    }

    /// Start the child again after an unasked-for exit, on the port it had.
    /// Answers the event the webview hears, or the reason the respawn failed.
    fn respawn(&self) -> Result<NodeRestarted, String> {
        let mut inner = self.lock();
        inner.crashes = inner.crashes.saturating_add(1);
        let attempt = inner.crashes;
        let token = if inner.token.is_empty() {
            // The first start failed before a token was minted.
            let minted = mint_token()?;
            inner.token = minted.clone();
            minted
        } else {
            inner.token.clone()
        };
        let keep = (inner.port != 0).then_some(inner.port);
        let spawned = match spawn_node(&token, keep) {
            Ok(spawned) => spawned,
            Err(e) => {
                inner.failed = Some(e.clone());
                return Err(e);
            }
        };
        let port = spawned.port;
        inner.child = Some(spawned.child);
        inner.tail = Some(spawned.tail);
        inner.port = port;
        inner.api_base = format!("http://127.0.0.1:{port}");
        inner.healthy_since = Some(Instant::now());
        inner.failed = None;
        Ok(NodeRestarted {
            port,
            attempt,
            requested: false,
        })
    }

    /// The watchdog: one thread for the life of the app, reading the child
    /// once a second. An exit the shell did not ask for is an `error` line
    /// with its code or signal and the node's last stderr lines, a crash
    /// report with the same, and a respawn after [`restart_delay`]; every
    /// respawn is announced to the webview and re-attaches the shell's log
    /// when nothing was writing yet.
    pub fn supervise<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
        std::thread::Builder::new()
            .name("node-watchdog".into())
            .spawn(move || loop {
                std::thread::sleep(WATCH_EVERY);
                let state = app.state::<NodeState>();
                let attempt = state.lock().crashes;
                let delay = restart_delay(attempt);
                if state.needs_start() {
                    // The start failed (or the last retry did): try again,
                    // on the same curve a crash gets.
                    tracing::warn!(target: "bisa_desktop", attempt, delay_secs = delay.as_secs(), "the node is not running; starting it");
                } else {
                    let Some(exit) = state.unexpected_exit() else {
                        continue;
                    };
                    let words = exit.words();
                    tracing::error!(
                        target: "bisa_desktop",
                        pid = exit.pid,
                        code = ?exit.exit.code,
                        signal = ?exit.exit.signal,
                        stderr_tail = ?exit.stderr,
                        attempt,
                        delay_secs = delay.as_secs(),
                        "the node {words}; restarting it"
                    );
                    exit.report(&app.state::<Handle>());
                }
                std::thread::sleep(delay);
                match state.respawn() {
                    Ok(restarted) => {
                        tracing::info!(target: "bisa_desktop", port = restarted.port, attempt = restarted.attempt, "node restarted by the watchdog");
                        reattach_log(&app, &state);
                        if let Err(e) = app.emit(NODE_RESTARTED, restarted) {
                            tracing::warn!(target: "bisa_desktop", "the restart did not reach the webview: {e}");
                        }
                    }
                    Err(e) => {
                        tracing::error!(target: "bisa_desktop", "the node could not be restarted: {e}");
                    }
                }
            })
            .expect("the watchdog thread spawns");
    }

    /// Where the node answers — or why there is no node to answer yet.
    pub fn api_base(&self) -> Result<String, String> {
        let inner = self.lock();
        match &inner.failed {
            Some(reason) if inner.child.is_none() && !inner.external => {
                Err(format!("the node did not start: {reason}"))
            }
            _ => Ok(inner.api_base.clone()),
        }
    }

    pub fn api_token(&self) -> String {
        self.lock().token.clone()
    }

    pub fn status(&self) -> NodeStatus {
        let mut inner = self.lock();
        let external = inner.external;
        let port = inner.port;
        let (running, pid) = match inner.child.as_mut() {
            Some(c) => match c.try_wait() {
                Ok(None) => (true, Some(c.id())),
                _ => (false, None),
            },
            None => (external, None),
        };
        NodeStatus {
            running,
            pid,
            port,
            external,
            restarts: inner.crashes,
            healthy_secs: running
                .then(|| inner.healthy_since.map(|s| s.elapsed().as_secs()))
                .flatten(),
        }
    }

    /// A restart the shell was asked for: the child goes and comes back on
    /// its port; the exit is expected, so the watchdog stays out of it.
    /// Answers the status and the event the caller announces.
    pub fn restart(&self) -> Result<(NodeStatus, NodeRestarted), String> {
        let restarted = {
            let mut inner = self.lock();
            if inner.external {
                return Err("node is external (BISA_API_BASE) — restart it yourself".into());
            }
            inner.expected_exit = true;
            if let Some(mut c) = inner.child.take() {
                end(&mut c);
            }
            let token = inner.token.clone();
            let spawned = spawn_node(&token, (inner.port != 0).then_some(inner.port));
            inner.expected_exit = false;
            let spawned = match spawned {
                Ok(s) => s,
                Err(e) => {
                    // The child is gone and nothing replaced it: the watchdog
                    // takes it from here, and the webview hears why.
                    inner.failed = Some(e.clone());
                    return Err(e);
                }
            };
            let port = spawned.port;
            inner.failed = None;
            tracing::info!(target: "bisa_desktop", pid = spawned.child.id(), port, "node restarted");
            inner.child = Some(spawned.child);
            inner.tail = Some(spawned.tail);
            inner.port = port;
            inner.api_base = format!("http://127.0.0.1:{port}");
            inner.crashes = 0;
            inner.healthy_since = Some(Instant::now());
            NodeRestarted {
                port,
                attempt: 0,
                requested: true,
            }
        };
        Ok((self.status(), restarted))
    }

    pub fn shutdown(&self) {
        let mut inner = self.lock();
        inner.expected_exit = true;
        if let Some(mut c) = inner.child.take() {
            end(&mut c);
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

    /// One crash report of kind `child_exit`, through the shell's handle. A
    /// report that cannot be written is said and costs nothing else.
    fn report(&self, log: &Handle) {
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
    /// at `debug`, into the shell's own log. Ends with the pipe. Answers
    /// the thread, for a wait on the child's last words; none when it
    /// could not be started.
    fn follow(self, stderr: std::process::ChildStderr) -> Option<std::thread::JoinHandle<()>> {
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
        match spawned {
            Ok(follower) => Some(follower),
            Err(e) => {
                tracing::warn!(target: "bisa_desktop", "the node's stderr is not followed: {e}");
                None
            }
        }
    }
}

/// Give the stderr follower a moment to read a child's last line after the
/// child ended — bounded, since a grandchild could hold the pipe open.
fn settle(follower: Option<&std::thread::JoinHandle<()>>, budget: Duration) {
    let Some(follower) = follower else {
        return;
    };
    let deadline = Instant::now() + budget;
    while !follower.is_finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// A child that answered its health check, with its port and its tail.
pub(crate) struct Spawned {
    pub child: Child,
    pub port: u16,
    pub tail: StderrTail,
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

/// Start the node. `keep` is the port a restart keeps — the webview's cached
/// base and every open terminal point at it — taken back when it is still
/// free, else a fresh one.
fn spawn_node(token: &str, keep: Option<u16>) -> Result<Spawned, String> {
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
    let mut errors = Vec::new();
    for bin in candidate_binaries() {
        let mut cmd = Command::new(&bin);
        // The node's stdout says nothing; its stderr is what a death that
        // wrote no log line leaves behind, so it is read to the last line.
        cmd.args(["node", "--listen", &listen])
            .env("BISA_API_TOKEN", token)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        if let Some(path) = &login_path {
            cmd.env("PATH", path);
        }
        match cmd.spawn() {
            Ok(mut child) => {
                let tail = StderrTail::default();
                let follower = child
                    .stderr
                    .take()
                    .and_then(|stderr| tail.clone().follow(stderr));
                let awaited =
                    await_health(port, Duration::from_secs(20), || match child.try_wait() {
                        Ok(Some(status)) => Some(Exit::of(&status)),
                        Ok(None) => None,
                        Err(_) => Some(Exit::unknown()),
                    });
                match awaited {
                    Awaited::Healthy => return Ok(Spawned { child, port, tail }),
                    Awaited::Exited(exit) => {
                        // Reaped by `try_wait` already; its last words may
                        // still be on the pipe.
                        settle(follower.as_ref(), Duration::from_millis(200));
                        errors.push(format!(
                            "{}: {} before answering its health check{}",
                            bin.display(),
                            exit.words(),
                            said(&tail.lines())
                        ));
                    }
                    Awaited::TimedOut => {
                        end(&mut child);
                        errors.push(format!(
                            "{}: started but health check failed{}",
                            bin.display(),
                            said(&tail.lines())
                        ));
                    }
                }
            }
            Err(e) => errors.push(format!("{}: {e}", bin.display())),
        }
    }
    Err(format!(
        "could not start the bisa node. Tried:\n{}",
        errors.join("\n")
    ))
}

/// *; it said: …* — the child's last lines after a failed start, for the
/// reason the webview shows. Nothing when it said nothing.
fn said(lines: &[String]) -> String {
    if lines.is_empty() {
        return String::new();
    }
    format!("; it said: {}", lines.join(" | "))
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

/// End a node this shell spawned, and reap it. An end that fails is a child
/// already gone; a wait that fails is one already reaped.
fn end(child: &mut std::process::Child) {
    if let Err(e) = child.kill() {
        tracing::debug!(target: "bisa_desktop::node", "ending the node: {e}");
    }
    if let Err(e) = child.wait() {
        tracing::debug!(target: "bisa_desktop::node", "reaping the node: {e}");
    }
}

/// How a spawn's wait for the health check ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Awaited {
    /// The node answered `200` on `/health`.
    Healthy,
    /// The child ended before it answered — refused the workspace another
    /// engine holds, a loader's refusal, a crash — and how.
    Exited(Exit),
    /// The budget ran out with the child still up and not answering.
    TimedOut,
}

/// Wait for the node to answer its health check, or for the child to end,
/// whichever comes first within `timeout`. `exited` reads the child each
/// tick: a node refused its workspace exits at once, and nobody should
/// wait twenty seconds to hear so. The probe comes first on every tick, so
/// a node that answered and then ended is still a node that answered.
fn await_health(port: u16, timeout: Duration, mut exited: impl FnMut() -> Option<Exit>) -> Awaited {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if probe_health(port) {
            return Awaited::Healthy;
        }
        if let Some(exit) = exited() {
            return Awaited::Exited(exit);
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    Awaited::TimedOut
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

    /// A shell whose node did not start still has a state: the webview is
    /// told why instead of a base, and the watchdog owes a start. Once a
    /// child runs, the reason is gone.
    #[test]
    fn a_failed_start_says_why_and_is_owed_a_retry() {
        let state = NodeState {
            inner: Mutex::new(NodeInner {
                child: None,
                tail: None,
                port: 0,
                api_base: String::new(),
                token: "t".into(),
                external: false,
                expected_exit: false,
                crashes: 0,
                healthy_since: None,
                failed: Some("no binary".into()),
            }),
        };
        assert!(state.needs_start());
        assert_eq!(
            state.api_base().unwrap_err(),
            "the node did not start: no binary"
        );
        assert!(!state.status().running);
        {
            let mut inner = state.lock();
            inner.failed = None;
            inner.api_base = "http://127.0.0.1:4477".into();
            inner.port = 4477;
        }
        assert!(!state.needs_start());
        assert_eq!(state.api_base().unwrap(), "http://127.0.0.1:4477");
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
        assert_eq!(said(&[]), "");
        assert_eq!(
            said(&["a".to_string(), "b".to_string()]),
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

    /// A child that is still up, as far as the wait can tell.
    fn still_up() -> Option<Exit> {
        None
    }

    #[test]
    fn the_health_probe_takes_a_200_on_loopback_and_gives_up_on_anything_else_within_its_budget() {
        assert_eq!(
            await_health(answer("200 OK"), Duration::from_secs(5), still_up),
            Awaited::Healthy
        );
        let started = Instant::now();
        assert_eq!(
            await_health(
                answer("503 Service Unavailable"),
                Duration::from_millis(700),
                still_up
            ),
            Awaited::TimedOut
        );
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(said(&[]), "");
        assert_eq!(
            said(&["a".to_string(), "b".to_string()]),
            "; it said: a | b"
        );
    }

    /// A node refused its workspace exits at once: the wait ends on the
    /// tick that sees it, with how it ended, not at the deadline — while a
    /// child still up and not answering is waited for to the budget, and
    /// one that answered before it ended is a node that answered.
    #[test]
    fn a_child_that_ends_before_answering_ends_the_wait_at_once() {
        let nobody = free_port().unwrap();
        let refused = Exit {
            code: Some(1),
            signal: None,
        };
        let started = Instant::now();
        assert_eq!(
            await_health(nobody, Duration::from_secs(10), || Some(refused)),
            Awaited::Exited(refused)
        );
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "seen on the first tick, not at the ten-second deadline"
        );
        let started = Instant::now();
        assert_eq!(
            await_health(nobody, Duration::from_millis(700), still_up),
            Awaited::TimedOut
        );
        assert!(started.elapsed() >= Duration::from_millis(600));
        assert_eq!(
            await_health(answer("200 OK"), Duration::from_secs(5), || Some(
                Exit::unknown()
            )),
            Awaited::Healthy,
            "the probe is asked before the child is, on every tick"
        );
        // Nothing to wait for settles at once; a follower that is done, too.
        let started = Instant::now();
        settle(None, Duration::from_secs(5));
        let done = std::thread::spawn(|| {});
        done.join().expect("a thread that did nothing");
        assert!(started.elapsed() < Duration::from_secs(1));
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
