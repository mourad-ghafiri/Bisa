//! Stopping every session that works on something — the agents the engine
//! runs and the harnesses a person opened in a terminal — when the thing is
//! stopped, closed, archived or deleted, or a workstream is closed.
//!
//! One selection rule ([`stops`]) over the roster — every kind of session,
//! a conversation's turn included: a row belongs to a goal
//! when it names the goal, to a run when it names the run, to a project when
//! it names the project or one of its workstreams, to a workstream when it
//! names that workstream, and to the workstreams a retirement names besides. One stop per kind: an engine
//! session is aborted in the registry, its item's stop mark is signalled so
//! the driver aborts the harness process at once (`executor::stop_item`),
//! a turn's or a wake's session is aborted and let go of, and its row
//! ended; an interactive session is ended on the desk, and the
//! desktop terminates its harness by closing the tab on the `aborted` frame
//! as it already does.
//!
//! **A row that reads *aborted* has no harness behind it**, and the verb
//! that stopped it says so only once that is true: every row told to stop
//! enters the [`Stopping`] ledger with its harness child's pid, leaves it
//! when its driver has torn the session down — the process gone — and
//! [`await_stopped`] watches the ledger, the roster and the in-flight marks
//! until nothing of the scope is left or the deadline
//! (`EngineConfig::stop_deadline_ms`) passes; at the deadline what still
//! has a process of ours is terminated — the whole group, `SIGTERM` then
//! `SIGKILL` — and counted, and what has none is reported, never a refusal.

use crate::events::ExecutionOutcome;
use crate::presence::SessionPresence;
use crate::registry::{LiveRunId, SessionKind};
use crate::{executor, Inner};
use bisa_core::{GoalId, Home, ProjectId, RunId, WorkstreamId};
use dashmap::DashMap;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

/// What a stop is for: what the rows' end is said as, and what the goals
/// spawned by the thing are told.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndCause {
    /// The thing is stopped and stays: a goal reads `draft` again.
    Stop,
    /// Stopped to be started again at once.
    Restart,
    /// Closed for good.
    Close,
    /// Archived or deleted.
    Retire,
    /// The engine itself is stopping.
    Shutdown,
}

impl EndCause {
    /// What the goals spawned by a thing ended for this cause are ended
    /// for: a stop or a restart stops them, a close or a retirement closes
    /// them — the parent goes for good, so does what it spawned.
    pub fn for_child(self) -> Self {
        match self {
            EndCause::Stop | EndCause::Restart | EndCause::Shutdown => EndCause::Stop,
            EndCause::Close | EndCause::Retire => EndCause::Close,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            EndCause::Stop => "stop",
            EndCause::Restart => "restart",
            EndCause::Close => "close",
            EndCause::Retire => "retire",
            EndCause::Shutdown => "shutdown",
        }
    }
}

/// What ending work did — the block every verb that stops, closes, retires
/// or aborts answers, so one sentence on the desktop words it: the sessions
/// told to stop; the harnesses that ignored it and were terminated at the
/// deadline; the sessions that could not be ended — still live when the
/// wait ran out, with no process of ours to terminate; the goals spawned by
/// the thing, stopped or closed with it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[schemars(rename = "SessionsStopped")]
pub struct Ended {
    pub sessions: usize,
    pub terminated: usize,
    pub still_live: usize,
    pub children: Vec<GoalId>,
}

impl Ended {
    /// What a wait answered, over `stopped` rows told to stop.
    pub fn of(settled: Settled) -> Self {
        Self {
            sessions: settled.stopped,
            terminated: settled.terminated,
            still_live: settled.still_live,
            children: Vec::new(),
        }
    }

    /// Fold what a child's end did into the parent's.
    pub fn add(&mut self, other: Ended) {
        self.sessions += other.sessions;
        self.terminated += other.terminated;
        self.still_live += other.still_live;
        self.children.extend(other.children);
    }
}

/// A session told to stop whose harness process is not yet known to be
/// gone: what its driver will tear down, or what the deadline terminates.
#[derive(Clone, Debug)]
pub struct StoppingEntry {
    pub kind: SessionKind,
    pub place: Place,
    /// The durable row, when the driver keeps one, so its pid is cleared
    /// once the process is terminated here.
    pub session_id: Option<String>,
    pub pid: Option<u32>,
    pub pid_seen_at: Option<u64>,
    pub since: std::time::Instant,
    pub cause: EndCause,
}

impl StoppingEntry {
    /// A child the engine spawned leads a group of its own; a terminal's
    /// child is the desktop's, one pid.
    fn group(&self) -> bool {
        self.kind != SessionKind::Terminal
    }
}

/// The ledger of sessions told to stop whose process is not yet known to be
/// gone (module doc). Entered by [`stop_row`] with the row's pid before the
/// row is ended — the end clears the roster's — and left by the driver's
/// teardown ([`ended`], [`Driven`], `Presence::forget`, an ask's end) or by
/// the deadline ([`await_stopped`]). An entry left for ten minutes is a
/// driver that never said: dropped with a warning, so the map stays small.
#[derive(Default)]
pub struct Stopping {
    entries: DashMap<LiveRunId, StoppingEntry>,
}

/// How long an entry may stand before it is a driver that never said.
const STOPPING_STALE: Duration = Duration::from_secs(600);

impl Stopping {
    /// The row was told to stop: remembered with its process, for the wait.
    pub fn begin(&self, row: &SessionPresence, process: Option<(u32, u64)>, cause: EndCause) {
        self.sweep_stale();
        let (pid, pid_seen_at) = match process {
            Some((pid, at)) => (Some(pid), Some(at)),
            None => (None, None),
        };
        self.entries.insert(
            row.id,
            StoppingEntry {
                kind: row.kind,
                place: Place {
                    goal: row.goal,
                    run: row.run,
                    project: row.project,
                    workstream: row.workstream,
                },
                session_id: row.session_id.as_ref().map(ToString::to_string),
                pid,
                pid_seen_at,
                since: std::time::Instant::now(),
                cause,
            },
        );
    }

    /// A late word of the process from the driver, after the stop.
    pub fn process_seen(&self, session_id: &str, pid: u32, at: u64) {
        for mut entry in self.entries.iter_mut() {
            if entry.session_id.as_deref() == Some(session_id) {
                entry.pid = Some(pid);
                entry.pid_seen_at = Some(at);
            }
        }
    }

    /// The driver tore the session down: the process is gone. Whether the
    /// run was being waited for.
    pub fn gone(&self, run: LiveRunId) -> bool {
        self.entries.remove(&run).is_some()
    }

    /// [`Self::gone`], by the durable row's id.
    pub fn gone_session(&self, session_id: &str) {
        self.entries
            .retain(|_, entry| entry.session_id.as_deref() != Some(session_id));
    }

    /// The entries a scope's wait watches.
    pub fn in_scope(
        &self,
        scope: Scope,
        workstreams: &HashSet<WorkstreamId>,
    ) -> Vec<(LiveRunId, StoppingEntry)> {
        self.entries
            .iter()
            .filter(|entry| match scope {
                Scope::Session(id) => *entry.key() == id,
                _ => stops(entry.place, scope, workstreams),
            })
            .map(|entry| (*entry.key(), entry.value().clone()))
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn sweep_stale(&self) {
        self.entries.retain(|run, entry| {
            let fresh = entry.since.elapsed() < STOPPING_STALE;
            if !fresh {
                tracing::warn!(target: "bisa_engine", session = %run, "a session told to stop was never torn down by its driver; forgetting it");
            }
            fresh
        });
    }
}

/// A driver's stop: the signal, and the fact beside it — so a stop one wait
/// consumed is still read by the next, and a driver that is not waiting
/// reads it where it stands.
#[derive(Default)]
pub struct StopSignal {
    notify: tokio::sync::Notify,
    stopped: std::sync::atomic::AtomicBool,
}

impl StopSignal {
    /// Stop: the fact first, then the signal. Idempotent.
    pub(crate) fn stop(&self) {
        self.stopped
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.notify.notify_one();
    }

    pub(crate) fn is_stopped(&self) -> bool {
        self.stopped.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Resolves once stopped — at once when the stop already came, whether
    /// or not an earlier wait consumed the signal.
    pub(crate) async fn stopped(&self) {
        if self.is_stopped() {
            return;
        }
        self.notify.notified().await;
    }
}

/// The stop of every session driven without a work item — the Workflow
/// Agent's wakes and the one-shot asks — by its run: what ending the run's
/// row tells the driver by ([`stop_row`]). A worker's stop is its item's
/// in-flight mark. The map is shared with every [`Driving`], which removes
/// its own entry on the way out, so a stop after the driver left tells
/// nobody.
#[derive(Default)]
pub struct Drivers {
    stops: Arc<DashMap<LiveRunId, Arc<StopSignal>>>,
}

impl Drivers {
    /// Tell the driver of `run` to stop. Answers whether one was driving it;
    /// a stop that comes before the driver listens is kept for it.
    pub fn stop(&self, run: LiveRunId) -> bool {
        match self.stops.get(&run) {
            Some(stop) => {
                stop.stop();
                true
            }
            None => false,
        }
    }

    /// Whether a driver is driving `run` right now.
    pub fn is_driving(&self, run: LiveRunId) -> bool {
        self.stops.contains_key(&run)
    }
}

/// A driver's standing in [`Drivers`] for the life of its loop: begun
/// before the launch, so a stop that lands while the harness starts is
/// told to somebody, and dropped on every exit — answered, failed,
/// panicked — so the map is the live drivers' alone.
pub struct Driving {
    stops: Arc<DashMap<LiveRunId, Arc<StopSignal>>>,
    run: LiveRunId,
    stop: Arc<StopSignal>,
}

impl Driving {
    pub fn begin(drivers: &Drivers, run: LiveRunId) -> Self {
        let stop = Arc::new(StopSignal::default());
        drivers.stops.insert(run, Arc::clone(&stop));
        Self {
            stops: Arc::clone(&drivers.stops),
            run,
            stop,
        }
    }

    /// Resolves once the run was stopped; a stop that came before the driver
    /// listened is kept for it.
    pub async fn stopped(&self) {
        self.stop.stopped().await;
    }

    /// Whether the run was stopped — read between two waits, where a
    /// consumed signal would say nothing.
    pub fn is_stopped(&self) -> bool {
        self.stop.is_stopped()
    }
}

impl Drop for Driving {
    fn drop(&mut self) {
        self.stops.remove(&self.run);
    }
}

/// A session this process drove is over: its row says so. Its harness
/// child is normally gone by now — the adapters' `abort` and `dispose` wait
/// for it — and the row's pid goes with the row; one that outlived its
/// driver keeps its pid on the record, so the wait on its stop still ends
/// it, and a boot after a crash can.
pub fn ended(inner: &Inner, session_id: &str) {
    let lingering = match inner.ws.session_by_id(session_id) {
        Ok(Some(row)) => row
            .pid
            .filter(|pid| child::alive(*pid, row.kind != SessionKind::Terminal)),
        _ => None,
    };
    let result = match lingering {
        Some(pid) => {
            tracing::debug!(session = %session_id, pid, "the session is over but its process is not gone yet; keeping its pid");
            inner.ws.end_session_keeping_process(session_id, now_secs())
        }
        None => inner.ws.end_session(session_id, now_secs()),
    };
    if let Err(e) = result {
        tracing::debug!(session = %session_id, "could not end the session row: {e}");
    }
    // The record first, the ledger after: a stop that read the record live
    // a moment ago and enters the ledger now finds its entry cleared here,
    // and one that reads the record after sees it ended (`stop_row`
    // re-reads it once its entry is in) — whichever way the two interleave,
    // a session whose process is gone is waited on by nobody.
    if lingering.is_none() {
        inner.ending.gone_session(session_id);
    }
}

/// The driver announced the harness child: recorded with the moment, so a
/// later boot can tell this process from another that inherited its pid.
pub fn process_started(inner: &Inner, session_id: &str, pid: Option<u32>) {
    let now = now_secs();
    if let Some(pid) = pid {
        inner.ending.process_seen(session_id, pid, now);
    }
    if let Err(e) = inner.ws.set_session_process(session_id, pid, now) {
        tracing::debug!(session = %session_id, "could not record the session's process: {e}");
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// What a boot does with the sessions the last process was driving: each is
/// ended, and the harness child it announced — when a process with that pid
/// started when the row says it did, so a recycled pid is never touched — is
/// terminated first, `SIGTERM` then `SIGKILL`. A dead node's `claude` kept
/// running and writing into a checkout otherwise, while the walk resumed
/// another session into the same tree. Returns how many rows were ended.
pub fn end_stale(inner: &Arc<Inner>) -> usize {
    let rows = match inner.ws.list_live_sessions() {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(target: "bisa_engine", "cannot list the sessions the last process was driving: {e}");
            return 0;
        }
    };
    let now = now_secs();
    let mut ended_rows = 0;
    for row in rows {
        if let (Some(pid), Some(seen_at)) = (row.pid, row.pid_seen_at) {
            if child::is_still_ours(pid, seen_at, now) {
                tracing::warn!(target: "bisa_engine", session = %row.id, pid, "terminating a harness child the last process left running");
                // A child the engine spawned leads a group of its own — its
                // tools' commands and its MCP server in it; a terminal's
                // child is the desktop's, one pid.
                child::terminate(pid, row.kind != SessionKind::Terminal);
            }
        }
        if let Err(e) = inner.ws.end_session(&row.id, now) {
            tracing::debug!(session = %row.id, "could not end the session row: {e}");
        }
        ended_rows += 1;
    }
    // A row already over whose child outlived its driver — it ignored its
    // stop, the node died before the deadline — keeps its pid for exactly
    // this: ended here, if it is still the process the row saw.
    match inner.ws.list_sessions_with_process() {
        Ok(rows) => {
            for row in rows {
                if let (Some(pid), Some(seen_at)) = (row.pid, row.pid_seen_at) {
                    if child::is_still_ours(pid, seen_at, now) {
                        tracing::warn!(target: "bisa_engine", session = %row.id, pid, "terminating a harness child that outlived its session");
                        child::terminate(pid, row.kind != SessionKind::Terminal);
                    }
                }
                if let Err(e) = inner.ws.set_session_process(&row.id, None, now) {
                    tracing::debug!(session = %row.id, "could not clear the session's process: {e}");
                }
            }
        }
        Err(e) => {
            tracing::warn!(target: "bisa_engine", "cannot list the sessions that still name a process: {e}");
        }
    }
    if ended_rows > 0 {
        tracing::info!(target: "bisa_engine", sessions = ended_rows, "ended the sessions the last process was driving");
    }
    ended_rows
}

/// The one place the engine looks at an OS process by pid. Everything here
/// is checked twice — the pid is alive, and it started when the session row
/// says the child was seen — so the platform never signals a process that
/// merely inherited a number.
pub(crate) mod child {
    use std::time::Duration;

    /// How far apart the row's moment and the process's start may be.
    const START_SLACK_SECS: u64 = 10;
    /// How long a `SIGTERM` is given before a `SIGKILL`.
    const TERM_GRACE: Duration = Duration::from_secs(2);

    /// `ps -o etime=` for the pid, parsed: seconds since the process started.
    /// `None` when there is no such process or `ps` cannot say.
    fn elapsed_secs(pid: u32) -> Option<u64> {
        let out = std::process::Command::new("ps")
            .args(["-o", "etime=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        parse_etime(String::from_utf8_lossy(&out.stdout).trim())
    }

    /// `[[dd-]hh:]mm:ss` → seconds. Pure.
    pub(crate) fn parse_etime(text: &str) -> Option<u64> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        let (days, clock) = match text.split_once('-') {
            Some((d, rest)) => (d.parse::<u64>().ok()?, rest),
            None => (0, text),
        };
        let parts: Vec<u64> = clock
            .split(':')
            .map(|p| p.parse::<u64>())
            .collect::<Result<_, _>>()
            .ok()?;
        let secs = match parts.as_slice() {
            [m, s] => m * 60 + s,
            [h, m, s] => h * 3600 + m * 60 + s,
            _ => return None,
        };
        Some(days * 86_400 + secs)
    }

    /// Whether the process at `pid` is the one a session row saw at
    /// `seen_at`: alive, and started within a few seconds of that moment.
    pub(crate) fn is_still_ours(pid: u32, seen_at: u64, now: u64) -> bool {
        let Some(elapsed) = elapsed_secs(pid) else {
            return false;
        };
        let started = now.saturating_sub(elapsed);
        started.abs_diff(seen_at) <= START_SLACK_SECS
    }

    /// A stored pid as the kernel's: one that does not fit an `i32` names
    /// no process of ours — never a negative number, which `kill` would read
    /// as a process group.
    fn kernel_pid(pid: u32) -> Option<rustix::process::Pid> {
        i32::try_from(pid)
            .ok()
            .and_then(rustix::process::Pid::from_raw)
    }

    /// Whether the process leads a process group of its own — what a child
    /// the engine spawned does, and what a child recorded by an older
    /// engine, or one that put itself elsewhere, does not.
    fn leads_group(pid: rustix::process::Pid) -> bool {
        rustix::process::getpgid(Some(pid)).is_ok_and(|pgid| pgid == pid)
    }

    /// Signal the process, or — `group`, and the process leads one — the
    /// process group it leads: a child the engine spawned is the leader of
    /// its own group, so its tools' commands and the MCP server it was
    /// handed go with it; one that leads none is signalled alone.
    fn signal(pid: u32, group: bool, signal: rustix::process::Signal) -> bool {
        let Some(pid) = kernel_pid(pid) else {
            return false;
        };
        if group && leads_group(pid) {
            rustix::process::kill_process_group(pid, signal).is_ok()
        } else {
            rustix::process::kill_process(pid, signal).is_ok()
        }
    }

    /// Whether the process — or, `group`, anything of the group it leads —
    /// is there.
    pub(crate) fn alive(pid: u32, group: bool) -> bool {
        kernel_pid(pid).is_some_and(|pid| {
            if group && leads_group(pid) {
                rustix::process::test_kill_process_group(pid).is_ok()
            } else {
                rustix::process::test_kill_process(pid).is_ok()
            }
        })
    }

    /// `SIGTERM`, a short grace, then `SIGKILL` if it is still there — to
    /// the process, or to the whole group it leads.
    pub(crate) fn terminate(pid: u32, group: bool) {
        if !signal(pid, group, rustix::process::Signal::TERM) {
            return;
        }
        let deadline = std::time::Instant::now() + TERM_GRACE;
        while alive(pid, group) && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
        if alive(pid, group) {
            signal(pid, group, rustix::process::Signal::KILL);
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn etime_reads_seconds_minutes_hours_and_days() {
            assert_eq!(parse_etime("05:07"), Some(307));
            assert_eq!(parse_etime("01:02:03"), Some(3723));
            assert_eq!(parse_etime("2-01:00:00"), Some(2 * 86_400 + 3600));
            assert_eq!(parse_etime("   00:09 "), Some(9));
            assert_eq!(parse_etime(""), None);
            assert_eq!(parse_etime("nonsense"), None);
        }

        #[test]
        fn a_pid_nobody_runs_is_not_ours() {
            // No process can have this pid on any machine this runs on.
            assert!(!is_still_ours(u32::MAX - 1, 0, 100));
        }
    }
}

/// How often the wait looks at the roster.
const STOP_POLL: Duration = Duration::from_millis(50);

/// What a wait answered: how many rows were told to stop, how many of
/// their processes had to be terminated at the deadline, and how many
/// sessions were still there at the end with no process of ours to
/// terminate — reported, never waited on longer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Settled {
    pub stopped: usize,
    pub still_live: usize,
    pub terminated: usize,
}

/// What the sessions are being stopped for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Goal(GoalId),
    /// One run's sessions — what a run of the workspace, which no goal
    /// holds, stops by.
    Run(RunId),
    Project(ProjectId),
    /// One checkout, closed: what stands in it and nothing in the project's
    /// other checkouts — the primary's sessions above all.
    Workstream(WorkstreamId),
    /// Every session this process drives — what the engine's own stop ends.
    Node,
    /// One row, by its id — what the roster's *Terminate* waits for.
    Session(LiveRunId),
}

/// Where a roster row works: the facts the selection reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Place {
    pub goal: Option<GoalId>,
    pub run: Option<RunId>,
    pub project: Option<ProjectId>,
    pub workstream: Option<WorkstreamId>,
}

/// Whether a session at `place` works on the scope, or in one of `workstreams`.
pub fn stops(place: Place, scope: Scope, workstreams: &HashSet<WorkstreamId>) -> bool {
    let in_workstreams = place.workstream.is_some_and(|w| workstreams.contains(&w));
    match scope {
        Scope::Goal(goal) => place.goal == Some(goal) || in_workstreams,
        Scope::Run(run) => place.run == Some(run) || in_workstreams,
        Scope::Project(project) => place.project == Some(project) || in_workstreams,
        Scope::Workstream(workstream) => place.workstream == Some(workstream) || in_workstreams,
        Scope::Node => true,
        // A row is on its own scope by its id, not by where it works.
        Scope::Session(_) => false,
    }
}

/// Whether a row is on the scope: by where it works, or — one row's own
/// scope — by its id.
fn row_on(row: &SessionPresence, scope: Scope, workstreams: &HashSet<WorkstreamId>) -> bool {
    match scope {
        Scope::Session(id) => row.id == id,
        _ => stops(
            Place {
                goal: row.goal,
                run: row.run,
                project: row.project,
                workstream: row.workstream,
            },
            scope,
            workstreams,
        ),
    }
}

/// Whether an in-flight item's home is on the scope: a worker still
/// launching has a mark and no row yet, and a wait on the scope must see it.
fn in_flight_on(home: &Home, scope: Scope) -> bool {
    match scope {
        Scope::Goal(goal) => home.goal() == Some(goal),
        Scope::Run(run) => home.run() == Some(run),
        Scope::Project(_) | Scope::Workstream(_) | Scope::Session(_) => false,
        Scope::Node => true,
    }
}

/// Stop every live session on the scope and in `workstreams`. Answers how
/// many were stopped. Idempotent: a session already ended is left alone.
pub fn stop_for(
    inner: &Arc<Inner>,
    scope: Scope,
    workstreams: &HashSet<WorkstreamId>,
    cause: EndCause,
) -> usize {
    let mut stopped = 0;
    for row in inner.presence.snapshot() {
        if !row_on(&row, scope, workstreams) || !row.state.is_live() {
            continue;
        }
        if stop_row(inner, &row, cause) {
            stopped += 1;
        }
    }
    stopped
}

/// Stop the session a row of the roster stands for — the one door every
/// stop goes through, so a row never reads *aborted* beside a harness still
/// at work. Each kind is stopped by what drives it:
///
/// - a **terminal**'s row is ended on the desk, and the desktop terminates
///   the harness by closing its tab on the `aborted` frame;
/// - a **worker**'s item is told to stop, and its driver aborts the harness
///   and settles the item failed ([`executor::stop_item`]);
/// - a **conversation** turn's session is let go of, and the next message
///   starts afresh ([`crate::conversation::stop_run`]);
/// - the Workflow Agent's **guided** wake is told to stop, or the session it
///   kept for a follow-up is let go of ([`crate::guided::stop_run`]);
/// - a one-shot **ask**'s loop is told to stop, aborts its harness and
///   answers its caller a refusal ([`crate::ask::stop_run`]).
///
/// The ledger comes first, with the row's process, since ending the row
/// clears it; then the registry's abort — terminal, so nothing the driver
/// does on its way out revives or parks the run; a row the registry does
/// not know is stopped by its kind all the same. Answers whether the row
/// was stopped; idempotent.
pub fn stop_row(inner: &Arc<Inner>, row: &SessionPresence, cause: EndCause) -> bool {
    // Entered in the ledger only when somebody is there to tear the session
    // down and say so — its driver, still driving — or, a terminal's, when
    // its process is known: a row whose driver has already let go of the
    // session (torn down, the row's end a moment behind) is ended here and
    // waited on by nobody, since its process is gone already.
    let record = row
        .session_id
        .as_ref()
        .and_then(|id| inner.ws.session_by_id(&id.to_string()).ok().flatten());
    let process = inner.presence.process_of(row.id).or_else(|| {
        record
            .as_ref()
            .and_then(|record| record.pid.zip(record.pid_seen_at))
    });
    let driven = match &record {
        // The durable record is the driver's own word: live until its
        // teardown ended it — and a record that still names a process,
        // ended or not, is one whose process may be there to wait for.
        Some(record) => record.status == bisa_store::SessionStatus::Live || record.pid.is_some(),
        None => match row.kind {
            SessionKind::Worker => row
                .work_item
                .is_some_and(|item| inner.inflight.contains_key(&item)),
            SessionKind::Guided => {
                inner.driving.is_driving(row.id) || inner.lifecycle.holds(row.id)
            }
            SessionKind::Ask => inner.driving.is_driving(row.id),
            SessionKind::Conversation => crate::conversation::drives(inner, row.id),
            SessionKind::Terminal => process.is_some(),
        },
    };
    if driven {
        inner.ending.begin(row, process, cause);
        // Re-read once the entry is in: a driver that ended the record
        // between the read above and the entry cleared nothing, and would
        // leave a wait that nothing ends (`ended`).
        let over = row.session_id.as_ref().is_some_and(|id| {
            inner
                .ws
                .session_by_id(&id.to_string())
                .ok()
                .flatten()
                .is_some_and(|r| r.status != bisa_store::SessionStatus::Live && r.pid.is_none())
        });
        if over {
            inner.ending.gone(row.id);
        }
    }
    if row.kind == SessionKind::Terminal {
        return inner.interactive.abort(inner, row.id);
    }
    if let Err(e) = inner.registry.abort(row.id) {
        tracing::debug!(target: "bisa_engine", session = %row.id, "the registry does not know the row; stopping it by its kind: {e}");
    }
    match row.kind {
        SessionKind::Worker => {
            if let Some(item) = row.work_item {
                executor::stop_item(inner, item);
            }
        }
        SessionKind::Conversation => crate::conversation::stop_run(inner, row.id),
        SessionKind::Guided => crate::guided::stop_run(inner, row.id),
        SessionKind::Ask => {
            crate::ask::stop_run(inner, row.id);
        }
        SessionKind::Terminal => {}
    }
    // A driver parked on a question it asked hears nothing of the stop
    // until the question goes: withdrawn, it answers the harness and winds
    // down on its next pass.
    withdraw_questions_of(inner, row.id);
    inner
        .presence
        .ended(inner, row.id, &ExecutionOutcome::Aborted);
    true
}

/// Why a question is withdrawn when the session that asked it is stopped.
pub const STOPPED: &str = "the session that asked was stopped";

/// The questions a session was asking die with it: every pending gate the
/// run asked through is withdrawn — its waiting driver hears a refusal that
/// is nobody's, answers the harness, and goes on to hear the stop — and the
/// journal says why, so the Inbox reads the question as settled and a later
/// boot does not withdraw it again. Answers how many were withdrawn.
pub fn withdraw_questions_of(inner: &Arc<Inner>, run: LiveRunId) -> usize {
    let mut withdrawn = 0;
    for gate in inner.gates.pending_for_session(run) {
        if inner.gates.withdraw(&gate.id).is_none() {
            continue;
        }
        let note = format!(
            "a question the session was asking — \"{}\" — was withdrawn: {STOPPED}",
            crate::recovery::first_line(&gate.question)
        );
        crate::recovery::withdraw_question(inner, gate.home, &gate.subject, STOPPED, note);
        withdrawn += 1;
    }
    withdrawn
}

/// What a row reads when its driver left it live.
pub const DRIVER_GONE: &str = "the session's driver went away";

/// A session this process drives, held by its driver from the row's
/// registration to its settle. Dropped while the row is still live — an
/// early return, a panic unwinding through the driver — the row ends as
/// failed and the durable record with it, so no row reads *starting* or
/// *thinking* with nothing behind it. A driver that ended, forgot or parked
/// the row itself drops it to no effect; one that hands the session on for
/// a follow-up disarms it ([`Self::disarm`]).
pub struct Driven {
    inner: Arc<Inner>,
    run: LiveRunId,
    session_id: Option<String>,
    armed: bool,
}

impl Driven {
    pub fn begin(inner: &Arc<Inner>, run: LiveRunId, session_id: Option<String>) -> Self {
        Self {
            inner: Arc::clone(inner),
            run,
            session_id,
            armed: true,
        }
    }

    /// The row is somebody else's from here — kept for a follow-up — and
    /// the guard's drop says nothing.
    pub fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for Driven {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let live = self
            .inner
            .presence
            .get(self.run)
            .is_some_and(|row| row.state.is_live());
        if !live {
            return;
        }
        tracing::warn!(target: "bisa_engine", session = %self.run, "a driver left its session's row live: ending it");
        // The record and the registry first, the row last: the row's change
        // is what a reader sees, and what it then reads must already stand.
        if let Some(id) = &self.session_id {
            ended(&self.inner, id);
        }
        if let Some(agent) = self.inner.registry.get(self.run) {
            if agent.status == crate::registry::AgentStatus::Running {
                crate::debug_on_err(
                    self.inner.registry.mutate(self.run, agent.generation, |a| {
                        a.status = crate::registry::AgentStatus::Idle
                    }),
                    "idling a run its driver left",
                );
            }
        }
        let outcome = ExecutionOutcome::Failed {
            reason: DRIVER_GONE.into(),
        };
        // The retention clock needs a runtime to run on; without one the
        // row goes at once.
        if tokio::runtime::Handle::try_current().is_ok() {
            self.inner.presence.ended(&self.inner, self.run, &outcome);
        } else {
            self.inner.presence.forget(&self.inner, self.run);
        }
    }
}

/// The roster has no row by this id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("no session {0}")]
pub struct NoSession(pub LiveRunId);

/// Stop the one session a row of the roster names — a person's *Terminate*,
/// `bisa sessions abort`. A row already over is left as it ended; a row the
/// roster does not have is said.
pub fn stop_one(inner: &Arc<Inner>, id: LiveRunId) -> Result<bool, NoSession> {
    let row = inner.presence.get(id).ok_or(NoSession(id))?;
    if row.state.is_ended() {
        return Ok(false);
    }
    Ok(stop_row(inner, &row, EndCause::Stop))
}

/// [`stop_one`], waited for: answers once the row's process is gone — or
/// was terminated at the deadline — and says which.
pub async fn stop_one_settled(inner: &Arc<Inner>, id: LiveRunId) -> Result<Ended, NoSession> {
    let told = usize::from(stop_one(inner, id)?);
    let settled = await_stopped(
        inner,
        Scope::Session(id),
        &HashSet::new(),
        told,
        inner.config.stop_deadline(),
    )
    .await;
    Ok(Ended::of(settled))
}

/// The live rows on the scope and in `workstreams`, right now.
fn live_count(inner: &Inner, scope: Scope, workstreams: &HashSet<WorkstreamId>) -> usize {
    inner
        .presence
        .snapshot()
        .iter()
        .filter(|row| row.state.is_live() && row_on(row, scope, workstreams))
        .count()
}

/// What still stands on the scope: live rows, sessions told to stop whose
/// process is not known gone, and items in flight — a worker still
/// launching, with a mark and no row yet.
fn standing(inner: &Inner, scope: Scope, workstreams: &HashSet<WorkstreamId>) -> usize {
    let in_flight = inner
        .inflight
        .iter()
        .filter(|mark| in_flight_on(&mark.home, scope))
        .count();
    live_count(inner, scope, workstreams)
        + inner.ending.in_scope(scope, workstreams).len()
        + in_flight
}

/// A session told to stop whose process is known and gone is over, whatever
/// its driver said of it.
fn settle_gone(inner: &Inner, scope: Scope, workstreams: &HashSet<WorkstreamId>) {
    for (run, entry) in inner.ending.in_scope(scope, workstreams) {
        if let Some(pid) = entry.pid {
            if !child::alive(pid, entry.group()) {
                inner.ending.gone(run);
            }
        }
    }
}

/// Wait until nothing of the scope stands — no live row, no session told to
/// stop whose process is not known gone, no item in flight — or `deadline`
/// passes. At the deadline, every session still waited for whose process
/// is still the one its row saw is terminated — its whole group, `SIGTERM`
/// then `SIGKILL` — and counted, with one note on its home; one with no
/// process of ours is counted as still live and reported. `stopped` is
/// what the stop before it answered.
pub async fn await_stopped(
    inner: &Arc<Inner>,
    scope: Scope,
    workstreams: &HashSet<WorkstreamId>,
    stopped: usize,
    deadline: Duration,
) -> Settled {
    let until = tokio::time::Instant::now() + deadline;
    loop {
        settle_gone(inner, scope, workstreams);
        if standing(inner, scope, workstreams) == 0 {
            return Settled {
                stopped,
                still_live: 0,
                terminated: 0,
            };
        }
        if tokio::time::Instant::now() >= until {
            break;
        }
        tokio::time::sleep(STOP_POLL).await;
    }
    let now = now_secs();
    let mut terminated = 0;
    let mut still_live = 0;
    let mut kills = Vec::new();
    for (run, entry) in inner.ending.in_scope(scope, workstreams) {
        inner.ending.gone(run);
        let ours = entry
            .pid
            .zip(entry.pid_seen_at)
            .filter(|(pid, seen_at)| child::is_still_ours(*pid, *seen_at, now));
        match ours {
            Some((pid, _)) => {
                terminated += 1;
                let group = entry.group();
                tracing::warn!(target: "bisa_engine", session = %run, pid, cause = entry.cause.as_str(), "a session did not stop by the deadline; terminating its process");
                note_terminated(inner, &entry, run, pid, deadline);
                if let Some(id) = &entry.session_id {
                    if let Err(e) = inner.ws.set_session_process(id, None, now) {
                        tracing::debug!(session = %id, "could not clear the session's process: {e}");
                    }
                }
                // `terminate` sleeps through its grace: off the runtime's
                // threads, every leftover at once.
                kills.push(tokio::task::spawn_blocking(move || {
                    child::terminate(pid, group)
                }));
            }
            None => {
                still_live += 1;
                tracing::warn!(target: "bisa_engine", session = %run, kind = entry.kind.as_str(), "a session did not stop by the deadline and has no process of ours to terminate");
            }
        }
    }
    for kill in kills {
        if let Err(e) = kill.await {
            tracing::warn!(target: "bisa_engine", "terminating a session's process panicked: {e}");
        }
    }
    still_live += live_count(inner, scope, workstreams);
    Settled {
        stopped,
        still_live,
        terminated,
    }
}

/// One note on the session's home: the person reads why a process was
/// terminated rather than meeting a harness that went on.
fn note_terminated(
    inner: &Arc<Inner>,
    entry: &StoppingEntry,
    run: LiveRunId,
    pid: u32,
    deadline: Duration,
) {
    let home = entry
        .place
        .goal
        .map(|goal| Home::Goal { goal })
        .or(entry.place.run.map(|run| Home::Run { run }));
    let Some(home) = home else {
        return;
    };
    let text = bisa_core::text!(
        "engine-session-terminated-at-deadline",
        kind = entry.kind.as_str(),
        session = run.to_string(),
        secs = deadline.as_secs(),
        pid = pid
    );
    if let Err(e) = crate::ops::add_note(inner, home, text.to_string(), None) {
        tracing::debug!(target: "bisa_engine", session = %run, "could not note the terminated process: {e}");
    }
}

/// Every session this process drives, stopped and waited for — the engine's
/// own stop: nothing of it runs, and no harness it started stays behind.
pub async fn end_all(inner: &Arc<Inner>) -> Settled {
    let none = HashSet::new();
    let told = stop_for(inner, Scope::Node, &none, EndCause::Shutdown);
    inner.lifecycle.abort_all(inner).await;
    await_stopped(
        inner,
        Scope::Node,
        &none,
        told,
        inner.config.stop_deadline(),
    )
    .await
}

/// Stop every live session standing in one workstream — what a close does
/// before the checkout goes. Answers how many were stopped.
pub fn stop_workstream(inner: &Arc<Inner>, workstream: WorkstreamId) -> usize {
    stop_for(
        inner,
        Scope::Workstream(workstream),
        &HashSet::new(),
        EndCause::Close,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine(dir: &tempfile::TempDir) -> crate::Engine {
        let ws = bisa_store::Workspace::open_with_keystore(
            dir.path(),
            Box::new(bisa_store::MemoryKeyStore::default()),
        )
        .unwrap();
        crate::Engine::start(
            ws,
            bisa_harness::HarnessCatalog::new(),
            crate::EngineConfig {
                design_enabled: false,
                events_enabled: false,
                ..Default::default()
            },
        )
        .unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_wait_answers_at_once_when_nothing_is_live() {
        let dir = tempfile::tempdir().unwrap();
        let engine = engine(&dir);
        let goal = GoalId::from_ulid(ulid::Ulid::from_parts(1, 1));
        let started = std::time::Instant::now();
        let settled = await_stopped(
            engine.inner(),
            Scope::Goal(goal),
            &HashSet::new(),
            3,
            Duration::from_secs(5),
        )
        .await;
        assert_eq!(
            settled,
            Settled {
                stopped: 3,
                still_live: 0,
                terminated: 0
            }
        );
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "no live row: no waiting"
        );
        engine.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_wait_reports_what_is_still_live_at_the_deadline() {
        let dir = tempfile::tempdir().unwrap();
        let engine = engine(&dir);
        let inner = engine.inner();
        let goal = GoalId::from_ulid(ulid::Ulid::from_parts(1, 1));
        // A row that stays live: nobody ends it during the wait.
        let id = crate::registry::LiveRunId::mint();
        inner.presence.register(
            inner,
            id,
            crate::presence::SessionMeta {
                kind: SessionKind::Worker,
                origin: bisa_core::SessionOrigin::Step {
                    step: None,
                    name: None,
                    resumed: false,
                },
                harness: "mock".into(),
                model: None,
                effort: None,
                agent: None,
                session_id: None,
                work_item: None,
                conversation: None,
                goal: Some(goal),
                run: None,
                workstream: None,
                project: None,
                cwd: None,
                transcript_path: None,
            },
        );
        let started = std::time::Instant::now();
        let settled = await_stopped(
            inner,
            Scope::Goal(goal),
            &HashSet::new(),
            0,
            Duration::from_millis(120),
        )
        .await;
        assert_eq!(
            settled.still_live, 1,
            "the row is reported, not waited on for ever"
        );
        assert!(
            started.elapsed() >= Duration::from_millis(100),
            "the bound was honoured"
        );
        engine.shutdown().await;
    }

    fn at(
        goal: Option<GoalId>,
        project: Option<ProjectId>,
        workstream: Option<WorkstreamId>,
    ) -> Place {
        Place {
            goal,
            run: None,
            project,
            workstream,
        }
    }

    #[test]
    fn a_session_stops_for_its_goal_its_project_or_a_named_workstream_and_no_other() {
        let goal = GoalId::from_ulid(ulid::Ulid::from_parts(1, 1));
        let other_goal = GoalId::from_ulid(ulid::Ulid::from_parts(1, 2));
        let project = ProjectId::from_ulid(ulid::Ulid::from_parts(2, 1));
        let wid = WorkstreamId::from_ulid(ulid::Ulid::from_parts(3, 1));
        let other_wid = WorkstreamId::from_ulid(ulid::Ulid::from_parts(3, 2));
        let named: HashSet<WorkstreamId> = [wid].into_iter().collect();
        let none = HashSet::new();
        assert!(stops(at(Some(goal), None, None), Scope::Goal(goal), &none));
        assert!(!stops(
            at(Some(other_goal), None, None),
            Scope::Goal(goal),
            &none
        ));
        assert!(
            stops(
                at(Some(other_goal), None, Some(wid)),
                Scope::Goal(goal),
                &named
            ),
            "a session in a retired project's workstream stops with the goal"
        );
        assert!(stops(
            at(None, Some(project), None),
            Scope::Project(project),
            &none
        ));
        assert!(stops(
            at(None, None, Some(wid)),
            Scope::Project(project),
            &named
        ));
        assert!(!stops(
            at(None, None, Some(wid)),
            Scope::Project(project),
            &none
        ));
        // A closed workstream: what stands in it, and nothing in the project's other checkouts.
        assert!(stops(
            at(None, Some(project), Some(wid)),
            Scope::Workstream(wid),
            &none
        ));
        assert!(
            !stops(
                at(None, Some(project), Some(other_wid)),
                Scope::Workstream(wid),
                &none
            ),
            "the project's other checkout is left alone"
        );
        assert!(
            !stops(at(None, Some(project), None), Scope::Workstream(wid), &none),
            "a session on the project with no checkout — the primary's — is left alone"
        );
        assert!(
            stops(
                at(None, None, Some(other_wid)),
                Scope::Workstream(wid),
                &[other_wid].into_iter().collect()
            ),
            "a workstream named besides stops too"
        );
    }

    /// A run of the workspace's sessions stop by the run: they name no goal,
    /// and another run's are left alone.
    #[test]
    fn a_session_stops_for_its_run_and_no_other() {
        let run = RunId::from_ulid(ulid::Ulid::from_parts(4, 1));
        let other = RunId::from_ulid(ulid::Ulid::from_parts(4, 2));
        let goal = GoalId::from_ulid(ulid::Ulid::from_parts(1, 1));
        let wid = WorkstreamId::from_ulid(ulid::Ulid::from_parts(3, 1));
        let none = HashSet::new();
        let in_run = |run| Place {
            run: Some(run),
            ..Place::default()
        };
        assert!(stops(in_run(run), Scope::Run(run), &none));
        assert!(!stops(in_run(other), Scope::Run(run), &none));
        assert!(
            !stops(in_run(run), Scope::Goal(goal), &none),
            "a run of the workspace is no goal's"
        );
        assert!(
            stops(
                at(None, None, Some(wid)),
                Scope::Run(run),
                &[wid].into_iter().collect()
            ),
            "a session in the run's checkout stops with it"
        );
    }
}
