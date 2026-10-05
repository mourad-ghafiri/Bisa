//! Interactive sessions: a harness a person opened in a desktop terminal,
//! registered here so it is a roster session like any other.
//!
//! The engine never drives such a session — the person at the keyboard does —
//! so its status cannot come from a `HarnessSession` stream. It comes from the
//! harness's **own lifecycle hooks**, which run a small reporter
//! (`bisa session report`) that posts [`SessionEvent`]s to the node under
//! a per-session secret. Those events reach [`crate::presence::Presence`]
//! through [`InteractiveDesk::report`], the one door, and fold exactly as an
//! engine-driven session's do: the same nine states, the same sub-agents, the
//! same `session_state` frame. Nothing on the desktop infers anything from
//! terminal bytes.
//!
//! What each harness can report, and how it is asked to, is the adapter's
//! business ([`bisa_harness::HarnessAdapter::interactive_reporting`] and
//! [`bisa_harness::HarnessAdapter::translate_report`]); this desk only
//! materialises the recipe — the files under `run/interactive/<session>/`, the
//! arguments, the environment — and keeps the secret. A harness with no recipe
//! opens as a plain terminal and is never a roster session.
//!
//! **The tab is the row.** An interactive session's row lives exactly as long
//! as the terminal tab behind it, and the desktop host is the only thing that
//! ends it, through two doors under the session's secret: `exit` — the
//! process ended by itself; the row moves to *done* or *failed* and is
//! **held** (no retention clock) beside the tab that still shows the exit —
//! and `close` — the tab is gone; the row leaves the roster at once. A
//! person's *Abort* from the roster ends the row with the ordinary retention,
//! as a fallback for a desktop that is not running; the desktop closes the
//! tab on the `aborted` frame and the `close` that follows forgets it sooner.
//!
//! Everything here is in memory. A node restart forgets every interactive
//! session, and a harness still running in a terminal cannot register again —
//! its secret is baked into the live process — so its hooks are refused from
//! then on and its tab reads as a plain terminal until it is closed. The
//! files of the sessions a process forgot are put away when the next one
//! starts ([`put_away_what_was_left`]).
//!
//! **A session stands where a terminal can open.** The scope and the id are
//! held to the store's own placement rule before anything is written or
//! registered, so a row never names a goal, a checkout, a work item or a run
//! the workspace does not have.

use crate::events::ExecutionOutcome;
use crate::presence::SessionMeta;
use crate::registry::{LiveRunId, SessionKind};
use crate::Inner;
use bisa_core::{GoalId, ProjectId, RunId, WorkItemId, WorkstreamId};
use bisa_harness::{PullSource, ReportingContext, SessionEvent};
use bisa_store::FileScope;
use dashmap::DashMap;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

/// The environment variable that names the session to a hook.
pub const ENV_SESSION: &str = "BISA_SESSION";
/// The environment variable carrying the session's secret to a hook.
pub const ENV_SECRET: &str = "BISA_SESSION_SECRET";

/// What the desktop shell says when it opens a harness: where, and which one.
#[derive(Debug, Clone)]
pub struct OpenInteractive {
    pub scope: FileScope,
    pub id: String,
    pub harness: String,
}

/// What the shell gets back: the session's handle and the plan it applies to
/// the command it is about to run. The secret travels once, in `env`, where
/// the hooks — and the shell's own reporter — read it.
#[derive(Debug, Clone)]
pub struct Opened {
    pub session: LiveRunId,
    /// Environment the harness process is given (the session and its secret,
    /// and the proxy the platform follows).
    pub env: BTreeMap<String, String>,
    /// Names the harness must not inherit — the proxy variables the person
    /// said not to use.
    pub env_remove: Vec<String>,
    /// Arguments appended to the harness's interactive command.
    pub args: Vec<String>,
    /// Whether the embedded terminal should watch the harness's own
    /// notifications (OSC 9) for an approval prompt and report it.
    pub intercept_approval_notifications: bool,
}

/// How the process behind a session ended, as the PTY host saw it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExitReport {
    /// The exit status, when the process exited on its own.
    pub code: Option<i32>,
    /// The signal that ended it, named — a process the host had to end, or
    /// one the OS took down.
    pub signal: Option<String>,
}

impl ExitReport {
    /// What the row says of the end. A signal outranks the status the PTY
    /// reports beside it (a signal death reads as status 1 there); nothing
    /// known at all is a failure with its reason, never *aborted* — that word
    /// is a person's decision.
    pub fn outcome(&self) -> ExecutionOutcome {
        match (&self.signal, self.code) {
            (Some(signal), _) => ExecutionOutcome::Failed {
                reason: format!("ended by {signal}"),
            },
            (None, Some(0)) => ExecutionOutcome::Completed,
            (None, Some(code)) => ExecutionOutcome::Failed {
                reason: format!("exited with status {code}"),
            },
            (None, None) => ExecutionOutcome::Failed {
                reason: "exit status unknown".into(),
            },
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InteractiveError {
    #[error("no harness {0:?} in this workspace")]
    UnknownHarness(String),
    #[error("{0} has no interactive form")]
    NotInteractive(String),
    #[error("no interactive session {0}")]
    UnknownSession(LiveRunId),
    #[error("the session secret does not match")]
    BadSecret,
    #[error("{0}")]
    Io(String),
    #[error(transparent)]
    Store(#[from] bisa_store::StoreError),
}

struct Row {
    secret: String,
    files_dir: Option<PathBuf>,
    /// The task reading a harness's own event stream, when its status is
    /// pulled rather than reported.
    puller: Option<tokio::task::JoinHandle<()>>,
}

/// The desk: every interactive session this engine knows, with its secret.
#[derive(Default)]
pub struct InteractiveDesk {
    rows: DashMap<LiveRunId, Row>,
}

/// Where a session's reporter files live.
fn files_dir(inner: &Inner, session: LiveRunId) -> PathBuf {
    inner
        .ws
        .paths()
        .run_dir()
        .join("interactive")
        .join(session.to_string())
}

/// A fresh secret: 32 random bytes as hex, from the generator the
/// workspace's identities are made with — the one the node mints its
/// control-plane token from. No second source, and none that can be guessed
/// from the clock.
pub(crate) fn mint_secret() -> String {
    nostr::key::Keys::generate().secret_key().to_secret_hex()
}

/// Constant-time equality, so a wrong secret costs the same whatever prefix
/// it got right.
fn same(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        diff |= usize::from(x ^ y);
    }
    diff == 0
}

/// A loopback port nothing is listening on right now, for a harness that
/// serves its own events.
fn free_port() -> Option<u16> {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .ok()
        .map(|a| a.port())
}

/// The argv that reports one hook payload for `harness`: this very binary,
/// in its reporter personality.
fn reporter_argv(harness: &str) -> Vec<String> {
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "bisa".to_string());
    vec![
        exe,
        "session".into(),
        "report".into(),
        "--harness".into(),
        harness.to_string(),
    ]
}

/// `terminal.status_reporting`: off means every harness opens as a plain
/// terminal, with nothing added to its command.
fn reporting_enabled(inner: &Inner) -> bool {
    inner
        .ws
        .settings(None)
        .unwrap_or_default()
        .iter()
        .find(|r| r.key == "terminal.status_reporting")
        .and_then(|r| r.value.as_bool())
        .unwrap_or(true)
}

/// What the placement says the session is about.
struct About {
    work_item: Option<WorkItemId>,
    goal: Option<GoalId>,
    run: Option<RunId>,
    workstream: Option<WorkstreamId>,
    project: Option<ProjectId>,
}

/// What the session is about — and that it can be: the store resolves the
/// scope's folder by the rule a terminal is placed by (`file_root`), which
/// reads the id and looks the thing up. A refusal is the store's own.
fn about(inner: &Inner, scope: FileScope, id: &str) -> Result<About, InteractiveError> {
    inner.ws.file_root(scope, id)?;
    let nothing = About {
        work_item: None,
        goal: None,
        run: None,
        workstream: None,
        project: None,
    };
    Ok(match scope {
        FileScope::Workstream => {
            let wid = WorkstreamId::from_str(id).ok();
            let project = match wid {
                Some(wid) => Some(inner.ws.get_workstream(wid)?.project),
                None => None,
            };
            About {
                workstream: wid,
                project,
                ..nothing
            }
        }
        FileScope::Goal => About {
            goal: GoalId::from_str(id).ok(),
            ..nothing
        },
        FileScope::WorkItem => About {
            work_item: WorkItemId::from_str(id).ok(),
            ..nothing
        },
        FileScope::Run => About {
            run: RunId::from_str(id).ok(),
            ..nothing
        },
    })
}

/// The files of the sessions the last process knew: every one of them is a
/// session this process does not — the desk is in memory — so the folder is
/// put away at a start, under the engine's lock, rather than kept for ever.
/// A folder that is not there is nothing to do; one that cannot be removed
/// is said and left.
pub(crate) fn put_away_what_was_left(inner: &Inner) {
    let root = inner.ws.paths().run_dir().join("interactive");
    match std::fs::remove_dir_all(&root) {
        Ok(()) => {
            tracing::debug!(target: "bisa_engine", dir = %root.display(), "put away the terminal sessions' files the last process left");
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            tracing::warn!(target: "bisa_engine", dir = %root.display(), "the terminal sessions' files the last process left could not be put away: {e}");
        }
    }
}

impl InteractiveDesk {
    /// Register a harness a person is about to open in a terminal, and answer
    /// the plan that makes it report. A harness with nothing to report still
    /// opens — the shell just gets no session.
    pub fn open(
        &self,
        inner: &Arc<Inner>,
        req: OpenInteractive,
    ) -> Result<Option<Opened>, InteractiveError> {
        let adapter = inner
            .catalog
            .get(&req.harness)
            .ok_or_else(|| InteractiveError::UnknownHarness(req.harness.clone()))?;
        if adapter.interactive().is_none() {
            return Err(InteractiveError::NotInteractive(req.harness.clone()));
        }
        if !reporting_enabled(inner) {
            return Ok(None);
        }
        // Placed first: a session that cannot stand anywhere mints nothing
        // and writes nothing.
        let about = about(inner, req.scope, &req.id)?;
        let session = LiveRunId::mint();
        let dir = files_dir(inner, session);
        let ctx = ReportingContext {
            session: session.to_string(),
            reporter: reporter_argv(&req.harness),
            files_dir: dir.clone(),
            port: free_port(),
            guard: crate::security::terminal_guard_argv(inner, &req.harness),
            guard_timeout_secs: crate::security::terminal_guard_timeout_secs(inner),
        };
        let plan = adapter.interactive_reporting(&ctx);
        if plan.is_none() {
            return Ok(None);
        }

        let mut written = None;
        if !plan.files.is_empty() {
            std::fs::create_dir_all(&dir)
                .map_err(|e| InteractiveError::Io(format!("{}: {e}", dir.display())))?;
            for file in &plan.files {
                let path = dir.join(&file.name);
                write_private(&path, &file.contents)?;
            }
            written = Some(dir);
        }

        let secret = mint_secret();
        inner.presence.register(
            inner,
            session,
            SessionMeta {
                kind: SessionKind::Terminal,
                harness: req.harness.clone(),
                // A harness a person opened is theirs: the platform passes it
                // no model and no effort, and the row claims neither.
                model: None,
                effort: None,
                agent: None,
                session_id: None,
                work_item: about.work_item,
                conversation: None,
                goal: about.goal,
                run: about.run,
                workstream: about.workstream,
                project: about.project,
                transcript_path: None,
            },
        );

        let puller = plan.pull.as_ref().map(|source| match source {
            PullSource::Sse { url } => spawn_sse_puller(
                Arc::clone(inner),
                session,
                Arc::clone(&adapter),
                url.clone(),
            ),
        });
        self.rows.insert(
            session,
            Row {
                secret: secret.clone(),
                files_dir: written,
                puller,
            },
        );

        let mut env = BTreeMap::new();
        env.insert(ENV_SESSION.to_string(), session.to_string());
        env.insert(ENV_SECRET.to_string(), secret);
        // The hooks write their own log under the node's folder, named — a
        // hook runs without `--data-dir` and must not guess.
        env.extend(crate::logging::child_env(inner));
        // The proxy the platform follows, as the harness's own tools read it.
        let (env, env_remove) = crate::network::session_env(inner, env);
        Ok(Some(Opened {
            session,
            env,
            env_remove,
            args: plan.args,
            intercept_approval_notifications: plan.intercept_approval_notifications,
        }))
    }

    fn check(&self, session: LiveRunId, secret: &str) -> Result<(), InteractiveError> {
        let row = self
            .rows
            .get(&session)
            .ok_or(InteractiveError::UnknownSession(session))?;
        if same(&row.secret, secret) {
            Ok(())
        } else {
            Err(InteractiveError::BadSecret)
        }
    }

    /// Fold events a hook reported. The only way anything from outside the
    /// engine reaches a roster row — the SSE puller comes through the same
    /// fold. What a harness says about its own tool calls is redacted
    /// first: a row and the Pulse never show a secret an agent typed.
    pub fn report(
        &self,
        inner: &Inner,
        session: LiveRunId,
        secret: &str,
        events: &[SessionEvent],
    ) -> Result<(), InteractiveError> {
        self.check(session, secret)?;
        fold_reported(inner, session, events);
        Ok(())
    }

    /// Judge one tool call a terminal-hosted harness is about to run — the
    /// guard hook's question. Secret-checked like a report; the verdict is
    /// what the hook prints back to the harness. A call refused will not
    /// run: the row closes it — whether its start was heard already or lands
    /// later, since a harness runs an event's hooks in parallel — and a wait
    /// on it is over.
    pub async fn guard(
        &self,
        inner: &Inner,
        session: LiveRunId,
        secret: &str,
        payload: &serde_json::Value,
    ) -> Result<crate::security::GuardReply, InteractiveError> {
        self.check(session, secret)?;
        let reply = crate::security::guard_hook(inner, session, payload).await;
        if reply.decision.as_deref() == Some("deny") {
            let tool = payload
                .get("tool_name")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let tool_id = payload
                .get("tool_use_id")
                .and_then(|v| v.as_str())
                .filter(|id| !id.is_empty());
            inner.presence.refused_tool(inner, session, tool_id, tool);
        }
        Ok(reply)
    }

    /// The person answered in the terminal — approved, declined or escaped
    /// the harness's dialog. No hook says so, so the tab does, through this
    /// door: every wait of the session and its sub-agents is over and the
    /// row goes back to what it was doing; the harness's next word corrects
    /// it whichever way the person answered.
    pub fn answered(
        &self,
        inner: &Inner,
        session: LiveRunId,
        secret: &str,
    ) -> Result<(), InteractiveError> {
        self.check(session, secret)?;
        inner.presence.answered(inner, session);
        Ok(())
    }

    /// The process behind the session ended by itself: the row moves to the
    /// end the report means and is **held** beside the tab that still shows
    /// the exit — no retention clock. A row already ended (a person's abort
    /// that the process then obeyed) keeps its word.
    pub fn exited(
        &self,
        inner: &Inner,
        session: LiveRunId,
        secret: &str,
        report: &ExitReport,
    ) -> Result<(), InteractiveError> {
        self.check(session, secret)?;
        let already_ended = inner
            .presence
            .get(session)
            .is_some_and(|p| p.state.is_ended());
        if !already_ended {
            inner.presence.hold_ended(inner, session, &report.outcome());
        }
        Ok(())
    }

    /// The terminal tab is gone: whatever the process was doing, the row
    /// leaves the roster now, and the session's files with it. The one door
    /// that forgets an interactive session.
    pub fn close(
        &self,
        inner: &Inner,
        session: LiveRunId,
        secret: &str,
    ) -> Result<(), InteractiveError> {
        self.check(session, secret)?;
        self.dismiss(session);
        inner.presence.forget(inner, session);
        Ok(())
    }

    /// A person aborted the session from the roster. The row ends with the
    /// ordinary retention — the fallback for a desktop that is not running —
    /// and the desk keeps the session, so the desktop's `close` (it closes the
    /// tab on the `aborted` frame) still authenticates and forgets it sooner.
    pub fn abort(&self, inner: &Arc<Inner>, session: LiveRunId) -> bool {
        if !self.rows.contains_key(&session) {
            return false;
        }
        inner
            .presence
            .ended(inner, session, &ExecutionOutcome::Aborted);
        true
    }

    /// Drop the desk's own state for a session: the secret, the event
    /// puller, the reporter files. The roster row is presence's to end — and
    /// when the retention clock takes an aborted row nobody closed, presence
    /// calls here, so the desk keeps nothing of a session the roster forgot.
    pub(crate) fn dismiss(&self, session: LiveRunId) {
        if let Some((_, row)) = self.rows.remove(&session) {
            if let Some(puller) = row.puller {
                puller.abort();
            }
            if let Some(dir) = row.files_dir {
                // The session's own scratch, made by this engine; a folder
                // already gone is nothing to report.
                if let Err(e) = std::fs::remove_dir_all(&dir) {
                    if e.kind() != std::io::ErrorKind::NotFound {
                        tracing::debug!(dir = %dir.display(), "a session's files dir could not be removed: {e}");
                    }
                }
            }
        }
    }
}

fn write_private(path: &std::path::Path, contents: &str) -> Result<(), InteractiveError> {
    let err = |e: std::io::Error| InteractiveError::Io(format!("{}: {e}", path.display()));
    std::fs::write(path, contents).map_err(err)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(err)?;
    }
    Ok(())
}

/// Every event from outside the engine — reported by a hook, or pulled from
/// a harness's own event server — passes the Redactor before the roster.
/// One fold, so no path can forget it.
fn fold_reported(inner: &Inner, session: LiveRunId, events: &[SessionEvent]) {
    for event in events {
        let event = inner.security.redact_event(event.clone());
        inner.presence.apply(inner, session, &event);
    }
}

/// Read a harness's own server-sent events and fold what they say. The
/// harness listens a moment after it starts, so the connection is retried
/// briefly; a connection that closes without an event counts as a failed try
/// too, so a server that accepts and hangs up cannot be dialled for ever —
/// only an event read resets the count. Once it drops for good the task ends
/// and the PTY's exit closes the row. The stream is read through
/// `bisa_http::SseFrames`: decoded once per complete frame, capped.
fn spawn_sse_puller(
    inner: Arc<Inner>,
    session: LiveRunId,
    adapter: Arc<dyn bisa_harness::HarnessAdapter>,
    url: String,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        use futures::StreamExt as _;
        const MAX_ATTEMPTS: u32 = 40;
        let mut attempts = 0u32;
        loop {
            // A harness on this machine: the loopback client, never a proxy.
            let response = match inner.http.loopback().get(&url).send().await {
                Ok(r) if r.status().is_success() => r,
                _ => {
                    attempts += 1;
                    if attempts > MAX_ATTEMPTS {
                        tracing::debug!(%session, "the harness never served its events at {url}");
                        return;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    continue;
                }
            };
            let mut stream = response.bytes_stream();
            let mut frames = bisa_http::SseFrames::default();
            let mut read = 0usize;
            while let Some(chunk) = stream.next().await {
                let Ok(chunk) = chunk else { break };
                if let Err(e) = frames.push(&chunk) {
                    tracing::warn!(%session, "the harness's event stream at {url} is not one the engine reads: {e}");
                    return;
                }
                while let Some(event) = frames.next_event() {
                    read += 1;
                    let Ok(payload) = serde_json::from_str::<serde_json::Value>(&event.data) else {
                        continue;
                    };
                    fold_reported(&inner, session, &adapter.translate_report(&payload));
                }
            }
            // The stream closed: the harness is going away, or restarting. A
            // stream that said something earns a fresh round of retries; one
            // that closed without a word is one more failed try, so a server
            // that accepts and hangs up is given up on like one that refuses.
            if read > 0 {
                attempts = 0;
            } else {
                attempts += 1;
                if attempts > MAX_ATTEMPTS {
                    tracing::debug!(%session, "the harness never served an event at {url}");
                    return;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_compare_in_constant_time_and_differ_per_mint() {
        let a = mint_secret();
        let b = mint_secret();
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
        assert!(same(&a, &a.clone()));
        assert!(!same(&a, &b));
        assert!(!same(&a, &a[..63]));
    }

    #[test]
    fn a_reporter_names_this_binary_in_its_reporter_personality() {
        let argv = reporter_argv("claude-code");
        assert_eq!(
            &argv[1..],
            ["session", "report", "--harness", "claude-code"]
        );
        assert!(!argv[0].is_empty());
    }

    #[test]
    fn a_free_port_is_a_port() {
        assert!(free_port().is_some_and(|p| p > 0));
    }

    #[test]
    fn an_exit_report_reads_as_the_end_it_means() {
        let report = |code: Option<i32>, signal: Option<&str>| {
            ExitReport {
                code,
                signal: signal.map(String::from),
            }
            .outcome()
        };
        assert_eq!(report(Some(0), None), ExecutionOutcome::Completed);
        assert_eq!(
            report(Some(129), None),
            ExecutionOutcome::Failed {
                reason: "exited with status 129".into()
            }
        );
        assert_eq!(
            report(Some(1), Some("Hangup: 1")),
            ExecutionOutcome::Failed {
                reason: "ended by Hangup: 1".into()
            },
            "the signal outranks the status the PTY reports beside it"
        );
        assert_eq!(
            report(None, None),
            ExecutionOutcome::Failed {
                reason: "exit status unknown".into()
            },
            "nothing known is a failure with its reason, never aborted"
        );
    }
}
