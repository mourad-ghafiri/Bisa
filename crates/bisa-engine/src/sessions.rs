//! Stopping every session that works on something — the agents the engine
//! runs and the harnesses a person opened in a terminal — when the thing is
//! archived or deleted, or a workstream is closed.
//!
//! One selection rule ([`stops`]) over the roster — every kind of session,
//! a conversation's turn included: a row belongs to a goal
//! when it names the goal, to a run when it names the run, to a project when
//! it names the project or one of its workstreams, to a workstream when it
//! names that workstream, and to the workstreams a retirement names besides. One stop per kind: an engine
//! session is aborted in the registry, its item's stop mark is signalled so
//! the driver aborts the harness process at once (`executor::stop_item`),
//! and its row ended; an interactive session is ended on the desk, and the
//! desktop terminates its harness by closing the tab on the `aborted` frame
//! as it already does. [`await_stopped`] is the wait a retirement takes
//! before a folder goes: the rows it stopped are no longer live, or a bound
//! passed and the leftovers are reported, never a refusal.

use crate::events::ExecutionOutcome;
use crate::presence::SessionPresence;
use crate::registry::{LiveRunId, SessionKind};
use crate::{executor, Inner};
use bisa_core::{GoalId, ProjectId, RunId, WorkstreamId};
use dashmap::DashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

/// The stop of every session driven without a work item — the Workflow
/// Agent's wakes and the one-shot asks — by its run: what ending the run's
/// row tells the driver by ([`stop_row`]). A worker's stop is its item's
/// in-flight mark. The map is shared with every [`Driving`], which removes
/// its own entry on the way out, so a stop after the driver left tells
/// nobody.
#[derive(Default)]
pub struct Drivers {
    stops: Arc<DashMap<LiveRunId, Arc<tokio::sync::Notify>>>,
}

impl Drivers {
    /// Tell the driver of `run` to stop. Answers whether one was driving it;
    /// a stop that comes before the driver listens is kept for it (`Notify`
    /// holds one permit).
    pub fn stop(&self, run: LiveRunId) -> bool {
        match self.stops.get(&run) {
            Some(stop) => {
                stop.notify_one();
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

/// A driver's standing in [`Drivers`] for the life of its loop: begun at the
/// launch, dropped on every exit — answered, failed, panicked — so the map is
/// the live drivers' alone.
pub struct Driving {
    stops: Arc<DashMap<LiveRunId, Arc<tokio::sync::Notify>>>,
    run: LiveRunId,
    stop: Arc<tokio::sync::Notify>,
}

impl Driving {
    pub fn begin(drivers: &Drivers, run: LiveRunId) -> Self {
        let stop = Arc::new(tokio::sync::Notify::new());
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
        self.stop.notified().await;
    }
}

impl Drop for Driving {
    fn drop(&mut self) {
        self.stops.remove(&self.run);
    }
}

/// A session this process drove is over: its row says so.
pub fn ended(inner: &Inner, session_id: &str) {
    if let Err(e) = inner.ws.end_session(session_id, now_secs()) {
        tracing::debug!(session = %session_id, "could not end the session row: {e}");
    }
}

/// The driver announced the harness child: recorded with the moment, so a
/// later boot can tell this process from another that inherited its pid.
pub fn process_started(inner: &Inner, session_id: &str, pid: Option<u32>) {
    if let Err(e) = inner.ws.set_session_process(session_id, pid, now_secs()) {
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
                child::terminate(pid);
            }
        }
        ended(inner, &row.id);
        ended_rows += 1;
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

    fn signal(pid: u32, signal: rustix::process::Signal) -> bool {
        let Some(pid) = kernel_pid(pid) else {
            return false;
        };
        rustix::process::kill_process(pid, signal).is_ok()
    }

    fn alive(pid: u32) -> bool {
        kernel_pid(pid).is_some_and(|pid| rustix::process::test_kill_process(pid).is_ok())
    }

    /// `SIGTERM`, a short grace, then `SIGKILL` if it is still there.
    pub(crate) fn terminate(pid: u32) {
        if !signal(pid, rustix::process::Signal::TERM) {
            return;
        }
        let deadline = std::time::Instant::now() + TERM_GRACE;
        while alive(pid) && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
        if alive(pid) {
            signal(pid, rustix::process::Signal::KILL);
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

/// How long a retirement waits for the sessions it stopped to end.
pub const STOP_DEADLINE: Duration = Duration::from_secs(5);

/// How often the wait looks at the roster.
const STOP_POLL: Duration = Duration::from_millis(50);

/// What a wait answered: how many rows ended, how many were still live at
/// the deadline — their process was told, their row is aborted; a delete
/// does not wait on a harness that ignores a kill.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Settled {
    pub stopped: usize,
    pub still_live: usize,
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
    }
}

/// Stop every live session on the scope and in `workstreams`. Answers how
/// many were stopped. Idempotent: a session already ended is left alone.
pub fn stop_for(inner: &Arc<Inner>, scope: Scope, workstreams: &HashSet<WorkstreamId>) -> usize {
    let mut stopped = 0;
    for row in inner.presence.snapshot() {
        let place = Place {
            goal: row.goal,
            run: row.run,
            project: row.project,
            workstream: row.workstream,
        };
        if !stops(place, scope, workstreams) || !row.state.is_live() {
            continue;
        }
        if stop_row(inner, &row) {
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
/// The registry's abort comes first: it is terminal, so nothing the driver
/// does on its way out revives or parks the run. Answers whether the row
/// was stopped; idempotent.
pub fn stop_row(inner: &Arc<Inner>, row: &SessionPresence) -> bool {
    if row.kind == SessionKind::Terminal {
        return inner.interactive.abort(inner, row.id);
    }
    if inner.registry.abort(row.id).is_err() {
        return false;
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
pub fn stop_one(inner: &Arc<Inner>, id: LiveRunId) -> Result<(), NoSession> {
    let row = inner.presence.get(id).ok_or(NoSession(id))?;
    if !row.state.is_ended() {
        stop_row(inner, &row);
    }
    Ok(())
}

/// The live rows on the scope and in `workstreams`, right now.
fn live_count(inner: &Inner, scope: Scope, workstreams: &HashSet<WorkstreamId>) -> usize {
    inner
        .presence
        .snapshot()
        .iter()
        .filter(|row| {
            row.state.is_live()
                && stops(
                    Place {
                        goal: row.goal,
                        run: row.run,
                        project: row.project,
                        workstream: row.workstream,
                    },
                    scope,
                    workstreams,
                )
        })
        .count()
}

/// Wait until no live row is left on the scope, or `deadline` passes.
/// `stopped` is what the stop before it answered; `still_live` what the
/// roster still shows at the end.
pub async fn await_stopped(
    inner: &Arc<Inner>,
    scope: Scope,
    workstreams: &HashSet<WorkstreamId>,
    stopped: usize,
    deadline: Duration,
) -> Settled {
    let until = tokio::time::Instant::now() + deadline;
    loop {
        let still_live = live_count(inner, scope, workstreams);
        if still_live == 0 || tokio::time::Instant::now() >= until {
            return Settled {
                stopped,
                still_live,
            };
        }
        tokio::time::sleep(STOP_POLL).await;
    }
}

/// Stop every live session standing in one workstream — what a close does
/// before the checkout goes. Answers how many were stopped.
pub fn stop_workstream(inner: &Arc<Inner>, workstream: WorkstreamId) -> usize {
    stop_for(inner, Scope::Workstream(workstream), &HashSet::new())
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
                still_live: 0
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
