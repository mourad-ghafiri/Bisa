//! Asking an agent one question, proven without an LLM.
//!
//! The mock harness answers a prompt with `echo: <prompt>` deltas and ends
//! its turn, which is the shape of a real single-turn reply — so these tests
//! exercise the whole path: resolve the agent, launch through the executor,
//! accumulate the text, settle on the turn end, dispose.

use bisa_core::AgentId;
use bisa_engine::ask::ask_agent_once;
use bisa_engine::{Engine, EngineConfig};
use bisa_harness::mock::MockAdapter;
use bisa_harness::{HarnessCatalog, LifecycleEvent, Outcome, SessionEvent};
use bisa_store::{MemoryKeyStore, Workspace};
use std::sync::Arc;
use std::time::{Duration, Instant};

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
        "hello?",
        Duration::from_secs(20),
    )
    .await
    .expect_err("nobody to ask");
    assert!(err.to_string().contains("missing or disabled"), "{err}");

    engine.shutdown().await;
}
