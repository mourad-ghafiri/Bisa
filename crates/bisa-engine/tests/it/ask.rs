//! Asking an agent one question, proven without an LLM.
//!
//! The mock harness answers a prompt with `echo: <prompt>` deltas and ends
//! its turn, which is the shape of a real single-turn reply — so these tests
//! exercise the whole path: resolve the agent, launch through the executor,
//! accumulate the text, settle on the turn end, dispose.

use bisa_core::{AgentId, AskPurpose, SessionOrigin};
use bisa_engine::ask::{ask_agent_once, Asking};
use bisa_engine::registry::SessionKind;
use bisa_engine::{Engine, EngineConfig, SessionState};
use bisa_harness::mock::{Close, MockAdapter};
use bisa_harness::{HarnessCatalog, LifecycleEvent, Outcome, SessionEvent};
use bisa_store::{MemoryKeyStore, Workspace};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// What these questions are for — one word on the roster row, nothing else.
fn asking() -> Asking {
    Asking::of(AskPurpose::Decision { point: None })
}

fn config() -> EngineConfig {
    EngineConfig {
        // These tests are about one question, not about the guided cycle.
        design_enabled: false,
        ..Default::default()
    }
}

/// An engine whose core agent runs on the mock handed in.
fn engine_on(dir: &tempfile::TempDir, adapter: MockAdapter) -> (Engine, Arc<MockAdapter>) {
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    let mut def = ws.get_agent(&AgentId::general()).unwrap();
    def.harness = adapter.id.clone();
    ws.update_agent(def).unwrap();

    let adapter = Arc::new(adapter);
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::clone(&adapter) as Arc<_>);
    (Engine::start(ws, catalog, config()).unwrap(), adapter)
}

#[tokio::test]
async fn asking_once_returns_the_text_of_the_turn() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_on(
        &dir,
        MockAdapter {
            id: "ask-harness".into(),
            ..Default::default()
        },
    );

    let answer = ask_agent_once(
        engine.inner(),
        AgentId::GENERAL,
        asking(),
        "write a commit message",
        Duration::from_secs(20),
    )
    .await
    .expect("an answer");
    assert!(
        answer.contains("write a commit message"),
        "the turn's own text, not a summary of it: {answer:?}"
    );

    // Nothing was injected that could act on the workspace. A session that
    // only suggests must not be able to commit.
    let spec = adapter.launches().pop().expect("one launch");
    assert!(spec.mcp_servers.is_empty(), "{:?}", spec.mcp_servers);
    assert_eq!(spec.tier_ceiling, bisa_core::ToolTier::Read);
    assert_eq!(spec.work_item, None);

    // Disposed rather than parked: the second question gets its own session
    // instead of finding the first one still lying around.
    ask_agent_once(
        engine.inner(),
        AgentId::GENERAL,
        asking(),
        "and another",
        Duration::from_secs(20),
    )
    .await
    .expect("a second answer");
    assert_eq!(adapter.launches().len(), 2);
    assert!(
        engine
            .inner()
            .registry
            .list()
            .iter()
            .all(|a| a.status != bisa_engine::AgentStatus::Running),
        "no session is left running"
    );

    engine.shutdown().await;
}

/// A harness that never ends its turn costs the caller the deadline and then
/// an **error**. The failure mode this rules out is the quiet one: returning
/// `Ok("")`, which a caller would happily put in a commit message box.
#[tokio::test]
async fn a_timeout_is_an_error_and_never_an_empty_answer() {
    let dir = tempfile::tempdir().unwrap();
    // A script with no terminal `Ended` leaves the session running forever.
    let (engine, _adapter) = engine_on(
        &dir,
        MockAdapter {
            id: "silent-harness".into(),
            script: Some(vec![SessionEvent::Progress(
                bisa_harness::ProgressEvent::TurnStarted,
            )]),
            ..Default::default()
        },
    );

    let started = Instant::now();
    let err = ask_agent_once(
        engine.inner(),
        AgentId::GENERAL,
        asking(),
        "say nothing",
        Duration::from_secs(2),
    )
    .await
    .expect_err("a silent harness must not look like an empty answer");
    assert!(err.to_string().contains("did not answer"), "{err}");
    assert!(
        started.elapsed() < Duration::from_secs(15),
        "the deadline is the deadline"
    );

    engine.shutdown().await;
}

/// A turn that ends without saying anything is also an error: an empty
/// suggestion is not a suggestion.
#[tokio::test]
async fn a_silent_turn_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, _adapter) = engine_on(
        &dir,
        MockAdapter {
            id: "mute-harness".into(),
            script: Some(vec![SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: Outcome::Completed,
                is_terminal: true,
            })]),
            ..Default::default()
        },
    );

    let err = ask_agent_once(
        engine.inner(),
        AgentId::GENERAL,
        asking(),
        "nothing to say",
        Duration::from_secs(20),
    )
    .await
    .expect_err("an empty turn is not an answer");
    assert!(err.to_string().contains("without saying anything"), "{err}");

    engine.shutdown().await;
}

/// No harness on the host is a refusal with a reason, not a fabricated string.
#[tokio::test]
async fn an_unavailable_harness_refuses_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, _adapter) = engine_on(
        &dir,
        MockAdapter {
            id: "absent-harness".into(),
            available: false,
            ..Default::default()
        },
    );

    let err = ask_agent_once(
        engine.inner(),
        AgentId::GENERAL,
        asking(),
        "anybody there?",
        Duration::from_secs(20),
    )
    .await
    .expect_err("nothing to launch");
    assert!(err.to_string().contains("no session for agent"), "{err}");

    // And an agent that does not exist at all.
    let err = ask_agent_once(
        engine.inner(),
        "no-such-agent",
        asking(),
        "hello?",
        Duration::from_secs(20),
    )
    .await
    .expect_err("nobody to ask");
    assert!(err.to_string().contains("missing or disabled"), "{err}");

    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// An ask is a row of the roster
// ---------------------------------------------------------------------------

/// [`engine_on`], with finished rows leaving the roster after one second.
fn engine_on_retaining_briefly(
    dir: &tempfile::TempDir,
    adapter: MockAdapter,
) -> (Engine, Arc<MockAdapter>) {
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    let mut def = ws.get_agent(&AgentId::general()).unwrap();
    def.harness = adapter.id.clone();
    ws.update_agent(def).unwrap();
    let adapter = Arc::new(adapter);
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::clone(&adapter) as Arc<_>);
    let config = EngineConfig {
        retain_ended_secs: 1,
        ..config()
    };
    (Engine::start(ws, catalog, config).unwrap(), adapter)
}

fn ask_row(engine: &Engine) -> Option<bisa_engine::SessionPresence> {
    engine
        .inner()
        .presence
        .snapshot()
        .into_iter()
        .find(|r| r.kind == SessionKind::Ask)
}

/// While it runs, a one-shot ask is a row of the roster of its own kind —
/// what it is for, who reads, where it stands — so the harness process a
/// person sees has a name; over, the row reads *done* and leaves after the
/// retention window, its registry entry with it.
#[tokio::test(flavor = "multi_thread")]
async fn a_one_shot_ask_is_a_row_of_the_roster_with_its_purpose_and_leaves_after_retention() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, _adapter) = engine_on_retaining_briefly(
        &dir,
        MockAdapter {
            turn_delay: Duration::from_millis(800),
            ..Default::default()
        },
    );
    let ask = ask_agent_once(
        engine.inner(),
        AgentId::GENERAL,
        asking(),
        "think it over",
        Duration::from_secs(20),
    );
    let seen = async {
        loop {
            if let Some(row) = ask_row(&engine).filter(|r| r.state.is_live()) {
                return row;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    let (answer, row) = tokio::join!(ask, seen);
    answer.expect("an answer");
    assert_eq!(
        row.origin,
        SessionOrigin::Ask {
            purpose: AskPurpose::Decision { point: None }
        }
    );
    assert_eq!(row.agent, Some(AgentId::general()), "who reads");
    assert!(row.cwd.is_some(), "where it stands");
    let wire = serde_json::to_value(&row).unwrap();
    assert_eq!(wire["kind"], serde_json::json!("ask"));
    assert_eq!(wire["origin"]["origin"], serde_json::json!("ask"));
    assert_eq!(
        wire["origin"]["purpose"]["kind"],
        serde_json::json!("decision")
    );

    let done = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match engine.inner().presence.get(row.id) {
                Some(r) if r.state == SessionState::Done => return true,
                Some(_) => tokio::time::sleep(Duration::from_millis(20)).await,
                None => return false,
            }
        }
    })
    .await
    .expect("the row reads done before it leaves");
    assert!(done);
    tokio::time::timeout(Duration::from_secs(5), async {
        while engine.inner().presence.get(row.id).is_some() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the row leaves after retention");
    assert!(
        engine.inner().registry.get(row.id).is_none(),
        "its registry entry went with it"
    );
    engine.shutdown().await;
}

/// *Terminate* on an ask's row reaches it: the harness is told to stop, the
/// caller is answered a refusal, and the row reads *aborted*.
#[tokio::test(flavor = "multi_thread")]
async fn a_terminated_ask_tells_its_harness_to_stop_and_answers_a_refusal() {
    let dir = tempfile::tempdir().unwrap();
    let adapter = MockAdapter {
        script: Some(vec![]),
        ..Default::default()
    };
    let closes = Arc::clone(&adapter.closes);
    let (engine, _adapter) = engine_on(&dir, adapter);
    let ask = ask_agent_once(
        engine.inner(),
        AgentId::GENERAL,
        asking(),
        "never answered",
        Duration::from_secs(20),
    );
    let stop = async {
        let row = loop {
            if let Some(row) = ask_row(&engine).filter(|r| r.state.is_live()) {
                break row;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        bisa_engine::sessions::stop_one(engine.inner(), row.id).unwrap();
        row
    };
    let (err, row) = tokio::join!(ask, stop);
    let err = err.expect_err("a stopped ask answers nothing");
    assert!(err.to_string().contains("was stopped"), "{err}");
    assert!(
        closes.lock().unwrap().contains(&Close::Aborted),
        "the harness was told to stop"
    );
    assert_eq!(
        engine.inner().presence.get(row.id).map(|r| r.state),
        Some(SessionState::Aborted)
    );
    engine.shutdown().await;
}

/// An ask past its deadline is an error — and its row reads *failed* with
/// the reason, for the retention window.
#[tokio::test(flavor = "multi_thread")]
async fn an_ask_past_its_deadline_ends_its_row_failed() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, _adapter) = engine_on(
        &dir,
        MockAdapter {
            script: Some(vec![]),
            ..Default::default()
        },
    );
    let err = ask_agent_once(
        engine.inner(),
        AgentId::GENERAL,
        asking(),
        "take your time",
        Duration::from_secs(1),
    )
    .await
    .expect_err("no answer in time");
    assert!(err.to_string().contains("did not answer"), "{err}");
    let row = ask_row(&engine).expect("the row stays for the retention window");
    assert!(
        matches!(&row.state, SessionState::Failed { reason } if reason.contains("did not answer")),
        "{:?}",
        row.state
    );
    engine.shutdown().await;
}
