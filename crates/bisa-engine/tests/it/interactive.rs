//! A harness a person opened in a desktop terminal, as a roster session: the
//! tab is the row. The desk's doors — `exited`, `close`, `abort`, `answered`
//! — and what each leaves in the roster and on disk.

use crate::common;

use bisa_engine::interactive::{ExitReport, InteractiveError, OpenInteractive, Opened, ENV_SECRET};
use bisa_engine::{
    Engine, EngineConfig, EnginePayload, ExecutionOutcome, LiveRunId, SessionState, SubmitRequest,
};
use bisa_harness::mock::MockAdapter;
use bisa_harness::{
    InteractiveLaunch, LaunchFile, LifecycleEvent, ProgressEvent, ReportingPlan, SessionEvent,
    SubagentId,
};
use bisa_store::FileScope;
use common::*;
use std::time::Duration;

/// A mock with an interactive form and a plan that writes one file — so the
/// desk has something to materialise and something to clean up.
fn reporting_mock() -> MockAdapter {
    MockAdapter {
        id: "mock".into(),
        interactive: Some(InteractiveLaunch::new("mock")),
        reporting: ReportingPlan {
            files: vec![LaunchFile {
                name: "reporter.json".into(),
                contents: "{}".into(),
            }],
            args: vec!["--report".into()],
            ..Default::default()
        },
        ..Default::default()
    }
}

/// An engine whose finished engine-driven sessions leave after one second —
/// short enough to prove a held interactive row does **not**.
fn engine(dir: &tempfile::TempDir) -> Engine {
    Engine::start(
        workspace(dir),
        catalog_with(vec![reporting_mock()]),
        EngineConfig {
            retain_ended_secs: 1,
            ..design_off_config()
        },
    )
    .unwrap()
}

/// A goal of the workspace's, for a session to stand in: the desk opens a
/// session only where a terminal can open.
fn a_goal(engine: &Engine) -> String {
    engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..SubmitRequest::captured("sit in a terminal")
        })
        .expect("a goal")
        .id
        .to_string()
}

fn open(engine: &Engine) -> Opened {
    let goal = a_goal(engine);
    engine
        .inner()
        .interactive
        .open(
            engine.inner(),
            OpenInteractive {
                scope: FileScope::Goal,
                id: goal,
                harness: "mock".into(),
            },
        )
        .expect("the mock has an interactive form")
        .expect("the mock has a plan")
}

/// Every folder the desk has made for a session, by name.
fn session_folders(engine: &Engine) -> Vec<String> {
    let root = engine.inner().ws.paths().run_dir().join("interactive");
    let mut names: Vec<String> = std::fs::read_dir(root)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn secret(opened: &Opened) -> String {
    opened.env[ENV_SECRET].clone()
}

fn files_dir(engine: &Engine, session: LiveRunId) -> std::path::PathBuf {
    engine
        .inner()
        .ws
        .paths()
        .run_dir()
        .join("interactive")
        .join(session.to_string())
}

fn state_of(engine: &Engine, session: LiveRunId) -> Option<SessionState> {
    engine.inner().presence.get(session).map(|p| p.state)
}

fn exit(code: Option<i32>, signal: Option<&str>) -> ExitReport {
    ExitReport {
        code,
        signal: signal.map(String::from),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn opening_registers_a_starting_row_with_its_files_and_its_secret_in_the_environment() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let opened = open(&engine);
    assert_eq!(
        state_of(&engine, opened.session),
        Some(SessionState::Starting)
    );
    assert_eq!(opened.args, vec!["--report".to_string()]);
    assert_eq!(opened.env["BISA_SESSION"], opened.session.to_string());
    assert_eq!(
        secret(&opened).len(),
        64,
        "the secret travels once, in the environment"
    );
    assert!(files_dir(&engine, opened.session)
        .join("reporter.json")
        .is_file());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_process_that_ends_by_itself_leaves_a_held_row_until_its_tab_closes() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let mut rx = engine.events();
    let opened = open(&engine);
    let desk = &engine.inner().interactive;

    desk.exited(
        engine.inner(),
        opened.session,
        &secret(&opened),
        &exit(Some(0), None),
    )
    .unwrap();
    assert_eq!(state_of(&engine, opened.session), Some(SessionState::Done));

    // Longer than the retention window: an engine session would be gone by
    // now. This row is the tab's, and the tab is still open.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(
        state_of(&engine, opened.session),
        Some(SessionState::Done),
        "held, not retained"
    );

    desk.close(engine.inner(), opened.session, &secret(&opened))
        .unwrap();
    assert_eq!(
        state_of(&engine, opened.session),
        None,
        "the tab closed: the row is gone at once"
    );
    assert!(
        !files_dir(&engine, opened.session).exists(),
        "and its files with it"
    );

    let gone = until("the roster says the session is gone", || {
        while let Ok(ev) = rx.try_recv() {
            if matches!(ev.payload, EnginePayload::SessionGone { live_run } if live_run == opened.session) {
                return Some(true);
            }
        }
        None
    })
    .await;
    assert!(gone);

    // Every door is shut behind it.
    assert!(matches!(
        desk.close(engine.inner(), opened.session, &secret(&opened)),
        Err(InteractiveError::UnknownSession(_))
    ));
    assert!(matches!(
        desk.exited(
            engine.inner(),
            opened.session,
            &secret(&opened),
            &exit(Some(0), None)
        ),
        Err(InteractiveError::UnknownSession(_))
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn closing_a_live_tab_forgets_the_row_without_ever_calling_it_failed() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let mut rx = engine.events();
    let opened = open(&engine);

    engine
        .inner()
        .interactive
        .close(engine.inner(), opened.session, &secret(&opened))
        .unwrap();
    assert_eq!(state_of(&engine, opened.session), None);

    // The frames about this session: the registration, then gone — no ended
    // state in between. A person closing a tab did not fail anything.
    let mut words = Vec::new();
    let mut gone = false;
    while let Ok(ev) = rx.try_recv() {
        match ev.payload {
            EnginePayload::SessionState { live_run, presence } if live_run == opened.session => {
                words.push(presence.state.as_str().to_string());
            }
            EnginePayload::SessionGone { live_run } if live_run == opened.session => gone = true,
            _ => {}
        }
    }
    assert_eq!(words, vec!["starting"]);
    assert!(gone);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_exit_reads_as_what_it_means_and_a_signal_outranks_its_status() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let desk = &engine.inner().interactive;

    let failed = open(&engine);
    desk.exited(
        engine.inner(),
        failed.session,
        &secret(&failed),
        &exit(Some(129), None),
    )
    .unwrap();
    assert_eq!(
        state_of(&engine, failed.session),
        Some(SessionState::Failed {
            reason: "exited with status 129".into()
        })
    );

    let signalled = open(&engine);
    desk.exited(
        engine.inner(),
        signalled.session,
        &secret(&signalled),
        &exit(Some(1), Some("Hangup: 1")),
    )
    .unwrap();
    assert_eq!(
        state_of(&engine, signalled.session),
        Some(SessionState::Failed {
            reason: "ended by Hangup: 1".into()
        })
    );

    let unknown = open(&engine);
    desk.exited(
        engine.inner(),
        unknown.session,
        &secret(&unknown),
        &exit(None, None),
    )
    .unwrap();
    assert_eq!(
        state_of(&engine, unknown.session),
        Some(SessionState::Failed {
            reason: "exit status unknown".into()
        }),
        "nothing known is a failure, never a person's abort"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_abort_keeps_its_word_through_the_exit_that_follows_and_the_close_still_authenticates() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let opened = open(&engine);
    let desk = &engine.inner().interactive;

    assert!(
        desk.abort(engine.inner(), opened.session),
        "the desk knows the session"
    );
    assert_eq!(
        state_of(&engine, opened.session),
        Some(SessionState::Aborted)
    );

    // The process obeyed the abort and the host reported how it ended: the
    // row already said *aborted*, and a person's decision is not overwritten
    // by the code the process chose on its way out.
    desk.exited(
        engine.inner(),
        opened.session,
        &secret(&opened),
        &exit(Some(129), None),
    )
    .unwrap();
    assert_eq!(
        state_of(&engine, opened.session),
        Some(SessionState::Aborted)
    );

    // The desktop closes the tab on the aborted frame; that close is the
    // desk's, still, and forgets the row ahead of the retention clock.
    desk.close(engine.inner(), opened.session, &secret(&opened))
        .unwrap();
    assert_eq!(state_of(&engine, opened.session), None);
    assert!(
        !desk.abort(engine.inner(), opened.session),
        "nothing left to abort"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_aborted_row_nobody_closes_leaves_on_the_retention_clock() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let opened = open(&engine);
    assert!(engine
        .inner()
        .interactive
        .abort(engine.inner(), opened.session));
    let gone = until("the aborted row leaves on its own", || {
        (state_of(&engine, opened.session).is_none()).then_some(true)
    })
    .await;
    assert!(
        gone,
        "a desktop that is not running still gets a roster that forgets"
    );
    // And the desk with it: a session the roster dropped keeps no secret, no
    // files and no reader of its events — nobody is left to close it. The
    // row leaves first and the desk's state a moment after, so the files are
    // waited for, not read the instant the row is gone.
    until("its files to go with the row", || {
        (!files_dir(&engine, opened.session).exists()).then_some(())
    })
    .await;
    assert!(matches!(
        engine
            .inner()
            .interactive
            .close(engine.inner(), opened.session, &secret(&opened)),
        Err(InteractiveError::UnknownSession(_))
    ));
}

/// A session stands where a terminal can open, and nowhere else: an id that
/// is no id, or one the workspace does not have, is refused in the store's
/// own words before anything is written or registered.
#[tokio::test(flavor = "multi_thread")]
async fn a_session_is_opened_only_where_a_terminal_can_open() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    // Well formed, and the id of nothing the workspace has.
    let absent = bisa_core::GoalId::from_ulid(ulid::Ulid::from_parts(1, 1)).to_string();
    for (scope, id) in [
        (FileScope::Goal, "not an id".to_string()),
        (FileScope::Goal, absent.clone()),
        (FileScope::Workstream, "not an id".to_string()),
        (FileScope::Workstream, absent.clone()),
        (FileScope::WorkItem, "not an id".to_string()),
        (FileScope::Run, "not an id".to_string()),
        (FileScope::Run, absent.clone()),
    ] {
        let refused = engine.inner().interactive.open(
            engine.inner(),
            OpenInteractive {
                scope,
                id: id.clone(),
                harness: "mock".into(),
            },
        );
        assert!(
            matches!(refused, Err(InteractiveError::Store(_))),
            "{scope} {id:?}: {refused:?}"
        );
    }
    assert!(
        engine.inner().presence.snapshot().is_empty(),
        "no row for any of them"
    );
    assert_eq!(
        session_folders(&engine),
        Vec::<String>::new(),
        "and nothing written for a session that never was"
    );
}

/// What the last process left under `run/interactive/` belongs to sessions
/// this one does not know — a node that restarts forgets every one of them
/// — so a start puts it away rather than keep it for ever.
#[tokio::test(flavor = "multi_thread")]
async fn a_start_puts_away_the_session_files_the_last_process_left() {
    let dir = tempfile::tempdir().unwrap();
    let left = {
        let engine = engine(&dir);
        let opened = open(&engine);
        let left = files_dir(&engine, opened.session);
        assert!(left.join("reporter.json").is_file());
        engine.shutdown().await;
        left
    };
    assert!(
        left.is_dir(),
        "a process that ends without its tabs closing leaves them"
    );
    let engine = engine(&dir);
    assert!(!left.exists(), "gone at the next start");
    assert_eq!(session_folders(&engine), Vec::<String>::new());
    // And the desk works as before.
    let opened = open(&engine);
    assert!(files_dir(&engine, opened.session)
        .join("reporter.json")
        .is_file());
    engine.shutdown().await;
}

/// A hook is a call of its own and can land after the exit did — the one
/// that says the harness started among them. A terminal's row that ended
/// stays ended until its tab closes: nothing a hook says opens it again.
#[tokio::test(flavor = "multi_thread")]
async fn a_hook_that_lands_after_the_exit_never_opens_the_row_again() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let desk = &engine.inner().interactive;
    let opened = open(&engine);
    desk.exited(
        engine.inner(),
        opened.session,
        &secret(&opened),
        &exit(Some(0), None),
    )
    .unwrap();
    desk.report(
        engine.inner(),
        opened.session,
        &secret(&opened),
        &[
            SessionEvent::Lifecycle(LifecycleEvent::Started),
            SessionEvent::Progress(ProgressEvent::TurnStarted),
        ],
    )
    .unwrap();
    assert_eq!(
        state_of(&engine, opened.session),
        Some(SessionState::Done),
        "the outcome a person reads beside the tab"
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_doors_take_only_the_sessions_own_secret() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let opened = open(&engine);
    let other = open(&engine);
    let desk = &engine.inner().interactive;

    assert!(matches!(
        desk.exited(
            engine.inner(),
            opened.session,
            &secret(&other),
            &exit(Some(0), None)
        ),
        Err(InteractiveError::BadSecret)
    ));
    assert!(matches!(
        desk.close(engine.inner(), opened.session, "not-a-secret"),
        Err(InteractiveError::BadSecret)
    ));
    assert_eq!(
        state_of(&engine, opened.session),
        Some(SessionState::Starting),
        "nothing moved"
    );
    assert!(matches!(
        desk.close(engine.inner(), LiveRunId::mint(), &secret(&opened)),
        Err(InteractiveError::UnknownSession(_))
    ));
    assert!(matches!(
        desk.answered(engine.inner(), opened.session, &secret(&other)),
        Err(InteractiveError::BadSecret)
    ));
}

/// No hook says how the person answered a permission dialog — only that it
/// showed, and later that the tool ran. The tab that showed it says so,
/// through the `answered` door: every wait of the session ends at once and
/// the row lands back on the call it already announced; a sub-agent's wait
/// ends the same way, the sub-agent back to *thinking*.
#[tokio::test(flavor = "multi_thread")]
async fn the_answered_door_ends_every_wait_and_lands_the_row_on_its_open_tool() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let desk = &engine.inner().interactive;
    let opened = open(&engine);
    let tool = |id: &str| {
        SessionEvent::Progress(ProgressEvent::ToolStarted {
            name: "Bash".into(),
            args_summary: "cargo test".into(),
            tier: bisa_core::ToolTier::Exec,
            id: Some(id.into()),
        })
    };
    let asks = |id: &str| {
        SessionEvent::Lifecycle(LifecycleEvent::InputRequested {
            request: bisa_harness::InputRequest::permission(
                id,
                "Bash",
                bisa_core::ToolTier::Exec,
                "cargo test",
                serde_json::json!({"command": "cargo test"}),
            ),
        })
    };
    desk.report(
        engine.inner(),
        opened.session,
        &secret(&opened),
        &[
            SessionEvent::Lifecycle(LifecycleEvent::Started),
            SessionEvent::Progress(ProgressEvent::TurnStarted),
            tool("toolu_1"),
            asks("toolu_1"),
        ],
    )
    .unwrap();
    assert!(
        matches!(
            state_of(&engine, opened.session),
            Some(SessionState::Waiting { .. })
        ),
        "the dialog is up"
    );

    desk.answered(engine.inner(), opened.session, &secret(&opened))
        .unwrap();
    assert!(
        matches!(
            state_of(&engine, opened.session),
            Some(SessionState::Running { ref tool, .. }) if tool == "Bash"
        ),
        "back on the call it announced: {:?}",
        state_of(&engine, opened.session)
    );

    // A sub-agent's dialog: its hand, not the session's — and the same door
    // drops it.
    spawn_child(&engine, opened.session, "agent-1");
    desk.report(
        engine.inner(),
        opened.session,
        &secret(&opened),
        &[SessionEvent::Lifecycle(LifecycleEvent::InputRequested {
            request: bisa_harness::InputRequest::permission(
                "toolu_s",
                "Read",
                bisa_core::ToolTier::Read,
                "README",
                serde_json::json!({"file_path": "README"}),
            )
            .raised_by(Some(SubagentId("agent-1".into()))),
        })],
    )
    .unwrap();
    let row = engine.inner().presence.get(opened.session).unwrap();
    assert!(
        matches!(row.state, SessionState::Running { ref tool, .. } if tool == "Bash"),
        "the session keeps its word: {:?}",
        row.state
    );
    assert!(
        matches!(row.children[0].state, SessionState::Waiting { .. }),
        "the hand is the sub-agent's: {:?}",
        row.children[0].state
    );
    assert!(row.wait().is_some_and(|(_, child)| child.is_some()));
    desk.answered(engine.inner(), opened.session, &secret(&opened))
        .unwrap();
    let row = engine.inner().presence.get(opened.session).unwrap();
    assert_eq!(row.children[0].state, SessionState::Thinking);
    assert!(row.wait().is_none(), "{row:?}");
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_harness_that_cannot_report_or_is_not_interactive_opens_no_session() {
    let dir = tempfile::tempdir().unwrap();
    let plain = MockAdapter {
        id: "plain".into(),
        interactive: Some(InteractiveLaunch::new("plain")),
        ..Default::default()
    };
    let protocol = MockAdapter {
        id: "protocol".into(),
        ..Default::default()
    };
    let engine = Engine::start(
        workspace(&dir),
        catalog_with(vec![plain, protocol]),
        design_off_config(),
    )
    .unwrap();
    let goal = a_goal(&engine);
    let ask = |harness: &str| {
        engine.inner().interactive.open(
            engine.inner(),
            OpenInteractive {
                scope: FileScope::Goal,
                id: goal.clone(),
                harness: harness.into(),
            },
        )
    };
    assert!(
        ask("plain").unwrap().is_none(),
        "an empty plan is a plain terminal"
    );
    assert!(matches!(
        ask("protocol"),
        Err(InteractiveError::NotInteractive(_))
    ));
    assert!(matches!(
        ask("nope"),
        Err(InteractiveError::UnknownHarness(_))
    ));
    assert!(
        engine.inner().presence.snapshot().is_empty(),
        "no row for any of them"
    );
    let _ = ExecutionOutcome::Completed;
}

/// A sub-agent announced under `session`, straight into the fold.
fn spawn_child(engine: &Engine, session: LiveRunId, id: &str) {
    engine.inner().presence.apply(
        engine.inner(),
        session,
        &SessionEvent::Progress(ProgressEvent::SubagentStarted {
            id: SubagentId(id.into()),
            name: "explore".into(),
            description: "look".into(),
        }),
    );
}

fn children_of(engine: &Engine, session: LiveRunId) -> usize {
    engine
        .inner()
        .presence
        .get(session)
        .map(|p| p.children.len())
        .unwrap_or(0)
}

#[tokio::test(flavor = "multi_thread")]
async fn an_exit_and_an_abort_each_take_the_sub_agents_with_the_row() {
    // The PTY's exit and a person's abort both end the row through the same
    // door the stream's end does: a sub-agent still thinking under a row
    // that read *done* was the ghost the rail kept showing.
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let desk = &engine.inner().interactive;

    let exited = open(&engine);
    spawn_child(&engine, exited.session, "t1");
    assert_eq!(children_of(&engine, exited.session), 1);
    desk.exited(
        engine.inner(),
        exited.session,
        &secret(&exited),
        &exit(Some(0), None),
    )
    .unwrap();
    assert_eq!(state_of(&engine, exited.session), Some(SessionState::Done));
    assert_eq!(
        children_of(&engine, exited.session),
        0,
        "a row that ends takes its children"
    );

    let aborted = open(&engine);
    spawn_child(&engine, aborted.session, "t2");
    assert!(desk.abort(engine.inner(), aborted.session));
    assert_eq!(
        state_of(&engine, aborted.session),
        Some(SessionState::Aborted)
    );
    assert_eq!(children_of(&engine, aborted.session), 0);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_terminal_row_whose_process_is_gone_is_swept_as_failed() {
    // The pid is a process this test started and has already waited for —
    // gone, so not ours — never a signal to anything, and never a pid the
    // test did not make.
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let opened = open(&engine);
    spawn_child(&engine, opened.session, "t1");
    let mut child = std::process::Command::new("true")
        .spawn()
        .expect("a process to have ended");
    let pid = child.id();
    child.wait().expect("waited");
    engine.inner().presence.apply(
        engine.inner(),
        opened.session,
        &SessionEvent::Lifecycle(LifecycleEvent::ProcessStarted { pid: Some(pid) }),
    );
    assert_eq!(
        engine.inner().presence.sweep_dead(engine.inner()),
        1,
        "one row ended"
    );
    assert!(matches!(
        state_of(&engine, opened.session),
        Some(SessionState::Failed { reason }) if reason == "the process is gone"
    ));
    assert_eq!(children_of(&engine, opened.session), 0);

    // A row with no pid reported is never swept: nothing is known about it.
    let quiet = open(&engine);
    assert_eq!(engine.inner().presence.sweep_dead(engine.inner()), 0);
    assert_eq!(
        state_of(&engine, quiet.session),
        Some(SessionState::Starting)
    );
    engine.shutdown().await;
}
