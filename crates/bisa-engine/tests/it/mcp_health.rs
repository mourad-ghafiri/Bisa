//! What the installed MCP servers answered when last checked (06 § MCP
//! servers), against a canned probe: a probe is kept and told to the bus, a
//! second ask joins the first, an edit forgets the answer, a disabled or a
//! reserved-name server is never dialed, and a draft transport is dialed
//! without an entry.

use crate::common::{catalog_with, design_off_config, workspace};
use bisa_core::{McpId, McpServerConfig};
use bisa_engine::events::EnginePayload;
use bisa_engine::mcp_health::{McpHealthState, McpHealthView, DISABLED};
use bisa_engine::{Engine, EngineConfig};
use bisa_mcp_probe::fake::FakeProbe;
use bisa_store::NewMcp;
use std::collections::BTreeMap;
use std::sync::Arc;

fn docs(name: &str) -> McpServerConfig {
    McpServerConfig::Http {
        name: name.into(),
        url: "https://mcp.example.test/mcp".into(),
        headers: BTreeMap::from([("Authorization".to_string(), "Bearer t".to_string())]),
    }
}

fn engine_with_probe(dir: &tempfile::TempDir, probe: Arc<FakeProbe>) -> Engine {
    let config = EngineConfig {
        mcp_probe: Some(probe),
        ..design_off_config()
    };
    Engine::start(workspace(dir), catalog_with(vec![]), config).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_probe_is_kept_as_the_servers_health_and_told_to_the_bus() {
    let dir = tempfile::tempdir().unwrap();
    let probe = Arc::new(FakeProbe::answering(vec![FakeProbe::ok_report("http", 3)]));
    let engine = engine_with_probe(&dir, Arc::clone(&probe));
    let ws = engine.workspace();
    let id = McpId::new("docs").unwrap();
    ws.create_mcp(NewMcp {
        id: id.clone(),
        description: String::new(),
        tags: Default::default(),
        transport: docs("docs"),
    })
    .unwrap();
    let mut bus = engine.events();
    assert_eq!(
        engine.inner().mcp_health.view_of(&id),
        McpHealthView::unknown(),
        "nothing until asked"
    );

    let report = engine
        .inner()
        .mcp_health
        .probe_mcp(engine.inner(), &id, None)
        .await
        .unwrap();
    assert!(report.ok);
    let view = engine.inner().mcp_health.view_of(&id);
    assert_eq!(view.state, McpHealthState::Ok);
    assert_eq!(view.tool_count, Some(3));
    assert_eq!(view.protocol_version.as_deref(), Some("2026-07-28"));
    assert!(view.checked_at.is_some());
    assert_eq!(probe.asked().len(), 1);
    assert_eq!(
        probe.asked()[0].name(),
        "docs",
        "the registered transport was dialed, headers and all"
    );
    let frame = crate::common::until("the probed frame", || {
        while let Ok(e) = bus.try_recv() {
            if let EnginePayload::McpProbed { id: who, ok } = e.payload.clone() {
                return Some((who, ok));
            }
        }
        None
    })
    .await;
    assert_eq!(frame, (id.clone(), true));

    // An edit forgets the answer; a removal too.
    let mut def = ws.get_mcp(&id).unwrap();
    def.description = "moved".into();
    bisa_engine::directory::update_mcp(engine.inner(), def).unwrap();
    assert_eq!(
        engine.inner().mcp_health.view_of(&id).state,
        McpHealthState::Unknown,
        "an edited server's health is not the old shape's"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_disabled_or_reserved_server_is_never_dialed_and_a_draft_is_dialed_without_an_entry() {
    let dir = tempfile::tempdir().unwrap();
    let probe = Arc::new(FakeProbe::answering(vec![FakeProbe::ok_report("stdio", 1)]));
    let engine = engine_with_probe(&dir, Arc::clone(&probe));
    let ws = engine.workspace();
    let id = McpId::new("off").unwrap();
    ws.create_mcp(NewMcp {
        id: id.clone(),
        description: String::new(),
        tags: Default::default(),
        transport: docs("off"),
    })
    .unwrap();
    let mut def = ws.get_mcp(&id).unwrap();
    def.enabled = false;
    ws.update_mcp(def).unwrap();
    let report = engine
        .inner()
        .mcp_health
        .probe_mcp(engine.inner(), &id, None)
        .await
        .unwrap();
    assert!(!report.ok);
    assert_eq!(report.error.as_deref(), Some(DISABLED));
    assert!(
        probe.asked().is_empty(),
        "a disabled server is answered in words, never dialed"
    );
    assert_eq!(
        engine.inner().mcp_health.view_of(&id).state,
        McpHealthState::Unknown,
        "a refusal is not a health"
    );

    let reserved = McpServerConfig::Stdio {
        name: "bisa".into(),
        command: "bisa".into(),
        args: vec![],
        env: BTreeMap::new(),
        cwd: None,
    };
    let report = engine
        .inner()
        .mcp_health
        .probe_transport(&reserved, None)
        .await;
    assert!(!report.ok && probe.asked().is_empty());

    let draft = engine
        .inner()
        .mcp_health
        .probe_transport(&docs("draft"), Some(3))
        .await;
    assert!(
        draft.ok,
        "a draft transport is dialed without a registry entry"
    );
    assert_eq!(probe.asked().len(), 1);
    assert!(
        ws.get_mcp(&McpId::new("draft").unwrap()).is_err(),
        "and nothing was registered by it"
    );

    let unknown = engine
        .inner()
        .mcp_health
        .probe_mcp(engine.inner(), &McpId::new("nobody").unwrap(), None)
        .await;
    assert!(unknown.is_err(), "an unknown id is the store's refusal");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_probe_is_a_health_too_and_says_where_it_stopped() {
    let dir = tempfile::tempdir().unwrap();
    let failed = bisa_mcp_probe::McpProbeReport::failed(
        &docs("docs"),
        bisa_mcp_probe::McpProbeStage::Initialize,
        "no answer within 10 s",
        std::time::Duration::from_secs(10),
    );
    let probe = Arc::new(FakeProbe::answering(vec![failed]));
    let engine = engine_with_probe(&dir, probe);
    let ws = engine.workspace();
    let id = McpId::new("docs").unwrap();
    ws.create_mcp(NewMcp {
        id: id.clone(),
        description: String::new(),
        tags: Default::default(),
        transport: docs("docs"),
    })
    .unwrap();
    let report = engine
        .inner()
        .mcp_health
        .probe_mcp(engine.inner(), &id, None)
        .await
        .unwrap();
    assert!(!report.ok);
    let view = engine.inner().mcp_health.view_of(&id);
    assert_eq!(view.state, McpHealthState::Failing);
    assert_eq!(view.stage, Some(bisa_mcp_probe::McpProbeStage::Initialize));
    assert_eq!(view.tool_count, None);
    assert!(view.error.as_deref().unwrap_or("").contains("no answer"));
}
