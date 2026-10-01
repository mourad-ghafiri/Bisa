//! Effort over the node's socket: what a harness and each of its models take
//! (`GET /harnesses/{id}/models`), a model plan's efforts through the agent
//! routes, and the effort a session runs at on its roster row.
//! Fakes only — every harness is a `MockAdapter` that lists what the test
//! gave it, and every session row is registered by hand: nothing is launched.

use crate::decisions::{request, TOKEN};
use bisa_core::{AgentId, Effort};
use bisa_engine::presence::SessionMeta;
use bisa_engine::registry::SessionKind;
use bisa_engine::{Engine, EngineConfig, LiveRunId};
use bisa_harness::mock::MockAdapter;
use bisa_harness::{HarnessCatalog, ModelInfo};
use bisa_node::{serve, NodeConfig};
use bisa_store::{FileKeyStore, Paths, Workspace};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// What a model that takes `xhigh` takes, and what one that does not.
const FIVE: [Effort; 5] = [
    Effort::Low,
    Effort::Medium,
    Effort::High,
    Effort::Xhigh,
    Effort::Max,
];
const FOUR: [Effort; 4] = [Effort::Low, Effort::Medium, Effort::High, Effort::Max];

/// A node over an engine whose catalog holds `adapters`, prepared before it
/// serves.
async fn boot_with(
    adapters: Vec<MockAdapter>,
    prepare: impl FnOnce(&Engine),
) -> (tempfile::TempDir, PathBuf, tokio::sync::oneshot::Sender<()>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let data = dir.path().to_path_buf();
    let ws = Workspace::open_with_keystore(
        &data,
        Box::new(FileKeyStore::new(Paths::new(&data).identity_dir())),
    )
    .expect("workspace");
    let mut catalog = HarnessCatalog::new();
    for adapter in adapters {
        catalog.register(Arc::new(adapter));
    }
    let engine = Engine::start(
        ws,
        catalog,
        EngineConfig {
            design_enabled: false,
            events_enabled: false,
            ..Default::default()
        },
    )
    .expect("engine");
    prepare(&engine);
    let socket = Paths::new(&data).node_socket();
    let (stop, stop_rx) = tokio::sync::oneshot::channel::<()>();
    let cfg = NodeConfig {
        socket: socket.clone(),
        http: None,
        data_dir: data,
        collab: None,
        fetch_attachment: None,
        token: Some(TOKEN.to_string()),
        #[cfg(feature = "a2a")]
        a2a: None,
    };
    tokio::spawn(serve(engine, cfg, async {
        let _stopped_or_dropped = stop_rx.await;
    }));
    let mut actual = socket.clone();
    for _ in 0..50 {
        if actual.exists() {
            break;
        }
        if let Ok(p) = std::fs::read_to_string(bisa_node::pointer_path(&socket)) {
            actual = PathBuf::from(p.trim());
            if actual.exists() {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(actual.exists(), "node socket never appeared");
    (dir, actual, stop)
}

async fn get(socket: &std::path::Path, path: &str) -> Value {
    let (code, value) = request(socket, "GET", path, None, TOKEN).await;
    assert_eq!(code, 200, "GET {path}: {value}");
    value
}

/// Two models, each with an effort of its own, under a plan that names one.
fn plan_with_efforts() -> Value {
    json!({
        "strategy": "fallback",
        "effort": "high",
        "models": [
            { "model": "claude-opus-5-5[1m]", "weight": 1, "enabled": true, "effort": "max" },
            { "model": "claude-sonnet-5-5[1m]", "weight": 1, "enabled": true,
              "suited_for": "quick edits", "effort": "auto" },
        ],
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn the_models_route_says_what_each_model_takes_and_what_the_harness_takes_for_any_other() {
    let listing = MockAdapter {
        id: "listing".into(),
        listed_models: vec![
            ModelInfo::new("opus-5", Some("Opus 5"), FIVE.to_vec()),
            ModelInfo::new("haiku-4", None, vec![]),
        ],
        efforts: FOUR.to_vec(),
        ..Default::default()
    };
    let silent = MockAdapter {
        id: "silent".into(),
        ..Default::default()
    };
    let (_dir, socket, stop) = boot_with(vec![listing, silent], |_| {}).await;

    // A harness with models: each says its own levels — one of them none —
    // and the harness says what it takes for an id it does not list.
    assert_eq!(
        get(&socket, "/harnesses/listing/models").await,
        json!({
            "harness": "listing",
            "efforts": ["low", "medium", "high", "max"],
            "models": [
                { "id": "opus-5", "label": "Opus 5",
                  "efforts": ["low", "medium", "high", "xhigh", "max"] },
                { "id": "haiku-4", "efforts": [] },
            ],
        })
    );
    // A harness that lists nothing and has no control: unknown models, no
    // levels — both present, both empty.
    assert_eq!(
        get(&socket, "/harnesses/silent/models").await,
        json!({ "harness": "silent", "efforts": [], "models": [] })
    );
    // An id that names no harness answers the same shape, under the id asked.
    assert_eq!(
        get(&socket, "/harnesses/nobody/models").await,
        json!({ "harness": "nobody", "efforts": [], "models": [] })
    );
    let _server_gone = stop.send(());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_plan_with_efforts_round_trips_through_the_agent_routes() {
    let (_dir, socket, stop) = boot_with(vec![MockAdapter::default()], |_| {}).await;
    let plan = plan_with_efforts();

    let (code, made) = request(
        &socket,
        "POST",
        "/agents",
        Some(json!({
            "name": "Scout", "system_prompt": "look around", "harness": "mock",
            "models": plan,
        })),
        TOKEN,
    )
    .await;
    assert_eq!(code, 200, "{made}");
    assert_eq!(made["agent"]["models"], plan, "as it was written");
    let id = made["agent"]["id"].as_str().unwrap().to_string();
    let read = get(&socket, &format!("/agents/{id}")).await;
    assert_eq!(read["agent"]["models"], plan, "and as it is read back");
    let listed = get(&socket, "/agents").await;
    let row = listed["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == json!(id))
        .expect("the agent is listed");
    assert_eq!(row["models"], plan);

    // A plan that names no effort carries none: the key is absent, not null.
    let silent = json!({
        "strategy": "fallback",
        "models": [{ "model": "claude-opus-5-5[1m]", "weight": 1, "enabled": true }],
    });
    let (code, edited) = request(
        &socket,
        "PATCH",
        &format!("/agents/{id}"),
        Some(json!({ "models": silent })),
        TOKEN,
    )
    .await;
    assert_eq!(code, 200, "{edited}");
    assert_eq!(edited["agent"]["models"], silent);
    assert!(edited["agent"]["models"].get("effort").is_none());
    assert!(edited["agent"]["models"]["models"][0]
        .get("effort")
        .is_none());

    // `auto` is a choice a plan may name; a word that is no effort is a 400
    // and changes nothing.
    let mut auto = silent.clone();
    auto["effort"] = json!("auto");
    let (code, edited) = request(
        &socket,
        "PATCH",
        &format!("/agents/{id}"),
        Some(json!({ "models": auto })),
        TOKEN,
    )
    .await;
    assert_eq!(code, 200, "{edited}");
    assert_eq!(edited["agent"]["models"]["effort"], "auto");
    for odd in ["ultra", "x_high", "High", ""] {
        let mut refused = silent.clone();
        refused["effort"] = json!(odd);
        let (code, said) = request(
            &socket,
            "PATCH",
            &format!("/agents/{id}"),
            Some(json!({ "models": refused })),
            TOKEN,
        )
        .await;
        assert_eq!(code, 400, "{odd:?}: {said}");
    }
    let read = get(&socket, &format!("/agents/{id}")).await;
    assert_eq!(read["agent"]["models"], auto, "the refusals wrote nothing");
    let _server_gone = stop.send(());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_core_agent_may_change_the_effort_of_its_plan() {
    let (_dir, socket, stop) = boot_with(vec![MockAdapter::default()], |_| {}).await;
    for id in [AgentId::GENERAL, AgentId::WORKFLOW] {
        let agent = get(&socket, &format!("/agents/{id}")).await["agent"].clone();
        assert!(
            agent["models"].get("effort").is_none(),
            "{id} ships no effort of its own: the setting decides"
        );
        // The plan it has, with an effort — and one on its second model.
        let mut plan = agent["models"].clone();
        plan["effort"] = json!("xhigh");
        plan["models"][1]["effort"] = json!("low");
        let (code, edited) = request(
            &socket,
            "PATCH",
            &format!("/agents/{id}"),
            Some(json!({ "models": plan })),
            TOKEN,
        )
        .await;
        assert_eq!(code, 200, "{id}: {edited}");
        assert_eq!(edited["agent"]["models"], plan);
        let read = get(&socket, &format!("/agents/{id}")).await;
        assert_eq!(read["agent"]["models"]["effort"], "xhigh");
        assert_eq!(read["agent"]["models"]["models"][1]["effort"], "low");
        assert_eq!(
            read["agent"]["models"]["models"], plan["models"],
            "the models are the ones it had"
        );
    }
    let _server_gone = stop.send(());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_row_carries_the_effort_it_runs_at_and_none_when_it_has_none() {
    let at_high = LiveRunId::mint();
    let without = LiveRunId::mint();
    let meta = |model: &str, effort: Option<Effort>| SessionMeta {
        kind: SessionKind::Worker,
        harness: "mock".into(),
        model: Some(model.into()),
        effort,
        agent: None,
        session_id: None,
        work_item: None,
        conversation: None,
        goal: None,
        run: None,
        workstream: None,
        project: None,
        transcript_path: None,
    };
    let (_dir, socket, stop) = boot_with(vec![MockAdapter::default()], |engine| {
        let inner = engine.inner();
        inner
            .presence
            .register(inner, at_high, meta("opus-5", Some(Effort::High)));
        inner
            .presence
            .register(inner, without, meta("haiku-4", None));
    })
    .await;

    let listed = get(&socket, "/sessions").await;
    let rows = listed["sessions"].as_array().unwrap();
    let row = |id: LiveRunId| {
        rows.iter()
            .find(|r| r["id"] == json!(id.to_string()))
            .unwrap_or_else(|| panic!("{id} is on the roster: {listed}"))
    };
    assert_eq!(row(at_high)["model"], "opus-5");
    assert_eq!(row(at_high)["effort"], "high");
    assert_eq!(row(without)["model"], "haiku-4");
    assert!(
        row(without).get("effort").is_none(),
        "no effort, no key: {}",
        row(without)
    );

    // One session's own route says the same.
    let one = get(&socket, &format!("/sessions/{at_high}")).await;
    assert_eq!(one["effort"], "high");
    let other = get(&socket, &format!("/sessions/{without}")).await;
    assert!(other.get("effort").is_none(), "{other}");
    let _server_gone = stop.send(());
}
