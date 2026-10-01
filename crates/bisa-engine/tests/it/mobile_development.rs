//! Mobile development (ide/19): the mobile tools answer through the engine
//! from the fake the test hands in — none configured is said as such; the
//! workspace's word on who may ask is checked before anything runs; a
//! platform that is off hides its devices; the toolchain is examined once
//! and again on *Check again* or a `mobile_development.*` write; a capture is stored
//! as an attachment and answered by path; the run line names the resolved
//! Flutter and the checkout; the designer reads a `Mobile:` line only when
//! the feature is on.

use crate::common::{
    catalog_with, design_off_config, drive_on, guided, intake_roundtrip, until, wait_for, workspace,
};
use bisa_core::settings::Scope as SettingScope;
use bisa_core::{AgentId, SkillId};
use bisa_engine::mobile_development::{
    facts_of_tree, DeviceState, MobileDevelopmentChange, MOBILE_DEVELOPMENT_SKILL, NOBODY_MAY,
    NOT_ASSIGNED, NO_TOOLS, OFF, PLATFORM_OFF,
};
use bisa_engine::{Engine, EngineConfig, EnginePayload};
use bisa_harness::mock::MockAdapter;
use bisa_harness::HarnessCatalog;
use bisa_mobile_development::fake::{emulator, png_fixture, simulator};
use bisa_mobile_development::{FakeMobileDevelopment, FlutterInfo, Toolchain};
use bisa_store::{CatalogKind, NewAgent};
use serde_json::json;
use std::sync::Arc;

fn fake() -> Arc<FakeMobileDevelopment> {
    Arc::new(
        FakeMobileDevelopment::new()
            .with_toolchain(Toolchain {
                flutter: FlutterInfo {
                    installed: true,
                    path: Some("/opt/flutter/bin/flutter".into()),
                    version: Some("3.24.3".into()),
                    channel: Some("stable".into()),
                    dart: Some("3.5.3".into()),
                },
                ..Toolchain::default()
            })
            .with_devices(vec![
                simulator("AAAA-1", "iPhone 16", DeviceState::Shutdown),
                emulator("Pixel_8", "Pixel 8", DeviceState::Shutdown),
            ])
            .with_screenshot(png_fixture(1170, 2532)),
    )
}

fn engine_with_mobile(dir: &tempfile::TempDir, tools: Arc<FakeMobileDevelopment>) -> Engine {
    let config = EngineConfig {
        mobile_development: Some(tools),
        ..design_off_config()
    };
    let engine = Engine::start(workspace(dir), catalog_with(vec![]), config).unwrap();
    engine
        .workspace()
        .set_setting(
            SettingScope::Machine,
            None,
            "mobile_development.enabled",
            json!(true),
        )
        .unwrap();
    engine
}

fn scout(skills: Vec<SkillId>) -> NewAgent {
    NewAgent {
        name: "Scout".into(),
        photo: None,
        description: None,
        system_prompt: "You are Scout.".into(),
        harness: "chat-harness".into(),
        models: Default::default(),
        skills,
        mcps: vec![],
        tags: Default::default(),
        respond: Default::default(),
        decision_making: false,
    }
}

fn errors_of(reply: &serde_json::Value) -> Vec<String> {
    reply["errors"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn ask(agent: &str, request: serde_json::Value) -> serde_json::Value {
    json!({"op": "mobile_development", "agent": agent, "request": request})
}

#[tokio::test(flavor = "multi_thread")]
async fn no_tools_configured_is_said_as_such_and_off_is_said_first() {
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::start(workspace(&dir), catalog_with(vec![]), design_off_config()).unwrap();
    let socket = engine.socket_path().to_path_buf();
    // Off by default: the switch is read before the tools are looked for.
    let reply = intake_roundtrip(&socket, ask("general-agent", json!({"action": "devices"}))).await;
    assert_eq!(reply["ok"], json!(false));
    assert_eq!(errors_of(&reply), vec![OFF.to_string()]);
    engine
        .workspace()
        .set_setting(
            SettingScope::Machine,
            None,
            "mobile_development.enabled",
            json!(true),
        )
        .unwrap();
    let reply = intake_roundtrip(&socket, ask("general-agent", json!({"action": "devices"}))).await;
    assert_eq!(errors_of(&reply), vec![NO_TOOLS.to_string()]);
    let status = bisa_engine::mobile_development::status(engine.inner(), false).await;
    assert!(
        matches!(
            status,
            Err(bisa_engine::EngineError::MobileDevelopmentUnavailable(_))
        ),
        "{status:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn nobody_and_unassigned_are_refused_and_the_skill_is_the_assignment() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with_mobile(&dir, fake());
    let ws = engine.workspace();
    let socket = engine.socket_path().to_path_buf();
    // Everyone, by default: an agent with no skill lists the devices.
    let plain = ws.add_agent(scout(vec![])).unwrap();
    let reply = intake_roundtrip(
        &socket,
        ask(plain.id.as_str(), json!({"action": "devices"})),
    )
    .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    assert_eq!(reply["result"]["devices"].as_array().unwrap().len(), 2);
    // Nobody: the core agents too.
    ws.set_setting(
        SettingScope::Workspace,
        None,
        "mobile_development.agents",
        json!("nobody"),
    )
    .unwrap();
    let reply = intake_roundtrip(&socket, ask("general-agent", json!({"action": "devices"}))).await;
    assert_eq!(errors_of(&reply), vec![NOBODY_MAY.to_string()]);
    // Assigned: an agent without the skill is told what to attach; with it,
    // and for a core agent, the list answers.
    ws.set_setting(
        SettingScope::Workspace,
        None,
        "mobile_development.agents",
        json!("assigned"),
    )
    .unwrap();
    let reply = intake_roundtrip(
        &socket,
        ask(plain.id.as_str(), json!({"action": "devices"})),
    )
    .await;
    assert_eq!(errors_of(&reply), vec![NOT_ASSIGNED.to_string()]);
    ws.install(CatalogKind::Skill, MOBILE_DEVELOPMENT_SKILL)
        .unwrap();
    let skilled = ws
        .add_agent(scout(vec![SkillId::new(MOBILE_DEVELOPMENT_SKILL).unwrap()]))
        .unwrap();
    let reply = intake_roundtrip(
        &socket,
        ask(skilled.id.as_str(), json!({"action": "devices"})),
    )
    .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    let reply = intake_roundtrip(&socket, ask("general-agent", json!({"action": "devices"}))).await;
    assert_eq!(
        reply["ok"],
        json!(true),
        "a core agent needs no skill: {reply}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_platform_that_is_off_hides_its_devices_and_refuses_booting_one() {
    let dir = tempfile::tempdir().unwrap();
    let tools = fake();
    let engine = engine_with_mobile(&dir, Arc::clone(&tools));
    let ws = engine.workspace();
    let socket = engine.socket_path().to_path_buf();
    ws.set_setting(
        SettingScope::Machine,
        None,
        "mobile_development.platforms",
        json!("ios"),
    )
    .unwrap();
    let reply = intake_roundtrip(&socket, ask("general-agent", json!({"action": "devices"}))).await;
    let devices = reply["result"]["devices"].as_array().unwrap().clone();
    assert_eq!(devices.len(), 1, "{reply}");
    assert_eq!(devices[0]["id"], json!("AAAA-1"));
    let reply = intake_roundtrip(
        &socket,
        ask(
            "general-agent",
            json!({"action": "boot", "device": "Pixel_8"}),
        ),
    )
    .await;
    assert_eq!(errors_of(&reply), vec![PLATFORM_OFF.to_string()]);
    assert!(
        !tools.calls().iter().any(|c| c.starts_with("boot")),
        "nothing was booted: {:?}",
        tools.calls()
    );
    // Booting a simulator answers it booted, and the bus says the devices changed.
    let mut bus = engine.events();
    let reply = intake_roundtrip(
        &socket,
        ask(
            "general-agent",
            json!({"action": "boot", "device": "AAAA-1"}),
        ),
    )
    .await;
    assert_eq!(
        reply["result"]["device"]["state"],
        json!("booted"),
        "{reply}"
    );
    wait_for(&mut bus, "devices changed", |e| {
        matches!(
            e.payload,
            EnginePayload::MobileDevelopmentChanged {
                what: MobileDevelopmentChange::Devices
            }
        )
    })
    .await;
    let reply = intake_roundtrip(&socket, ask("general-agent", json!({"action": "boot"}))).await;
    assert!(errors_of(&reply)[0].contains("name a device"), "{reply}");
    let reply = intake_roundtrip(
        &socket,
        ask("general-agent", json!({"action": "boot", "device": "ZZZZ"})),
    )
    .await;
    assert!(errors_of(&reply)[0].contains("no device ZZZZ"), "{reply}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_toolchain_is_examined_once_until_check_again_or_a_mobile_setting_write() {
    let dir = tempfile::tempdir().unwrap();
    let tools = fake();
    let engine = engine_with_mobile(&dir, Arc::clone(&tools));
    let mut bus = engine.events();
    let first = bisa_engine::mobile_development::status(engine.inner(), false)
        .await
        .unwrap();
    assert!(first.enabled);
    assert_eq!(first.toolchain.flutter.version.as_deref(), Some("3.24.3"));
    assert!(first.checked_at > 0);
    wait_for(&mut bus, "toolchain examined", |e| {
        matches!(
            e.payload,
            EnginePayload::MobileDevelopmentChanged {
                what: MobileDevelopmentChange::Toolchain
            }
        )
    })
    .await;
    let probes =
        |tools: &FakeMobileDevelopment| tools.calls().iter().filter(|c| *c == "toolchain").count();
    assert_eq!(probes(&tools), 1);
    bisa_engine::mobile_development::status(engine.inner(), false)
        .await
        .unwrap();
    assert_eq!(probes(&tools), 1, "the last look answers within its TTL");
    bisa_engine::mobile_development::status(engine.inner(), true)
        .await
        .unwrap();
    assert_eq!(probes(&tools), 2, "check again examines");
    engine
        .set_setting(
            SettingScope::Machine,
            None,
            "mobile_development.flutter.path",
            json!("/elsewhere/flutter"),
        )
        .unwrap();
    let after = bisa_engine::mobile_development::status(engine.inner(), false)
        .await
        .unwrap();
    assert_eq!(
        probes(&tools),
        3,
        "a mobile_development.* write makes the look stale"
    );
    assert_eq!(
        after.hints.flutter_path.as_deref(),
        Some("/elsewhere/flutter")
    );
    // The op answers the same, through the socket, with `fresh`.
    let socket = engine.socket_path().to_path_buf();
    let reply = intake_roundtrip(
        &socket,
        ask("general-agent", json!({"action": "status", "fresh": true})),
    )
    .await;
    assert_eq!(
        reply["result"]["toolchain"]["flutter"]["channel"],
        json!("stable"),
        "{reply}"
    );
    assert_eq!(probes(&tools), 4);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_screenshot_is_stored_as_an_attachment_and_answered_by_path() {
    let dir = tempfile::tempdir().unwrap();
    let tools = fake();
    let engine = engine_with_mobile(&dir, Arc::clone(&tools));
    let socket = engine.socket_path().to_path_buf();
    let reply = intake_roundtrip(
        &socket,
        ask(
            "general-agent",
            json!({"action": "screenshot", "device": "AAAA-1"}),
        ),
    )
    .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    let shot = &reply["result"];
    let path = std::path::PathBuf::from(shot["path"].as_str().unwrap());
    assert!(
        path.is_file(),
        "the named copy is on disk: {}",
        path.display()
    );
    assert!(
        path.file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("mobile-AAAA1-"),
        "{}",
        path.display()
    );
    assert_eq!(shot["width"], json!(1170));
    assert_eq!(shot["height"], json!(2532));
    let sha = shot["attachment"]["sha256"].as_str().unwrap();
    assert!(
        engine.workspace().attachment_path(sha).is_some(),
        "the blob is stored"
    );
    assert_eq!(std::fs::read(&path).unwrap(), png_fixture(1170, 2532));
    assert!(tools.calls().contains(&"screenshot AAAA-1 png".to_string()));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_run_line_names_the_resolved_flutter_and_the_checkout() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with_mobile(&dir, fake());
    let ws = engine.workspace();
    let project = ws
        .create_project(bisa_store::NewProject::managed("shop").unwrap())
        .unwrap();
    let root = ws.project_root_path(&project);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("pubspec.yaml"),
        "name: shop\ndependencies:\n  flutter:\n    sdk: flutter\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("android")).unwrap();
    let wid = ws.primary_workstream(project.id).unwrap().id;
    let facts = bisa_engine::mobile_development::project_facts(engine.inner(), wid).unwrap();
    assert!(facts.flutter && facts.android && !facts.ios);
    assert_eq!(facts, facts_of_tree(&root));
    let run = bisa_engine::mobile_development::run_command(engine.inner(), wid, "AAAA-1").unwrap();
    assert_eq!(run.command, "/opt/flutter/bin/flutter run -d AAAA-1");
    assert_eq!(run.cwd, root.display().to_string());
    assert_eq!(run.device, "AAAA-1");
    assert!(
        bisa_engine::mobile_development::run_command(engine.inner(), wid, "  ").is_err(),
        "a run names a device"
    );
    // Off: no line.
    ws.set_setting(
        SettingScope::Machine,
        None,
        "mobile_development.enabled",
        json!(false),
    )
    .unwrap();
    let refused = bisa_engine::mobile_development::run_command(engine.inner(), wid, "AAAA-1");
    assert!(
        matches!(refused, Err(bisa_engine::EngineError::Conflict(ref why)) if why.to_string() == OFF),
        "{refused:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_designer_reads_a_mobile_line_only_when_the_feature_is_on() {
    for on in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let ws = workspace(&dir);
        drive_on(&ws, &AgentId::workflow(), "mock-guided");
        let adapter = Arc::new(MockAdapter {
            id: "mock-guided".into(),
            ..Default::default()
        });
        let mut catalog = HarnessCatalog::new();
        catalog.register(Arc::clone(&adapter) as Arc<dyn bisa_harness::HarnessAdapter>);
        let config = EngineConfig {
            design_enabled: true,
            events_enabled: false,
            mobile_development: Some(fake()),
            ..Default::default()
        };
        let engine = Engine::start(ws, catalog, config).unwrap();
        if on {
            engine
                .workspace()
                .set_setting(
                    SettingScope::Machine,
                    None,
                    "mobile_development.enabled",
                    json!(true),
                )
                .unwrap();
            engine
                .workspace()
                .set_setting(
                    SettingScope::Machine,
                    None,
                    "mobile_development.platforms",
                    json!("android"),
                )
                .unwrap();
        }
        engine.submit_goal(guided("ship the shop app")).unwrap();
        let prompt = until("the prompt", || adapter.prompts().first().cloned()).await;
        let goal_block = prompt.split("GOAL\n").nth(1).expect("a GOAL block");
        if on {
            assert!(
                goal_block.contains("Mobile: on (Android only)"),
                "a fact beside the mode, never a probe: {goal_block}"
            );
        } else {
            assert!(!prompt.contains("Mobile:"), "{prompt}");
        }
    }
}

/// A device tool that answers more than a screen is answering something
/// else: the frame is refused with its size, and nothing that size is kept.
#[tokio::test(flavor = "multi_thread")]
async fn a_frame_over_the_bound_is_refused_and_never_kept() {
    let dir = tempfile::tempdir().unwrap();
    let tools = Arc::new(
        FakeMobileDevelopment::new()
            .with_toolchain(Toolchain {
                flutter: FlutterInfo {
                    installed: true,
                    path: Some("/opt/flutter/bin/flutter".into()),
                    version: Some("3.24.3".into()),
                    channel: Some("stable".into()),
                    dart: Some("3.5.3".into()),
                },
                ..Toolchain::default()
            })
            .with_devices(vec![simulator("AAAA-1", "iPhone 16", DeviceState::Booted)])
            .with_screenshot(vec![
                0u8;
                bisa_engine::mobile_development::MAX_FRAME_BYTES + 1
            ]),
    );
    let engine = engine_with_mobile(&dir, Arc::clone(&tools));
    let inner = engine.inner();
    let refused = bisa_engine::mobile_development::frame(
        inner,
        None,
        "AAAA-1",
        bisa_mobile_development::ImageFormat::Png,
    )
    .await
    .unwrap_err();
    assert!(refused.to_string().contains("the most is"), "{refused}");
    let shot = bisa_engine::mobile_development::screenshot(inner, None, "AAAA-1").await;
    assert!(
        shot.is_err(),
        "a screenshot is a frame kept: the same bound holds"
    );
    assert!(
        engine
            .workspace()
            .list_artifacts("general", 10)
            .unwrap()
            .is_empty(),
        "nothing that size is stored"
    );
    engine.shutdown().await;
}
