//! A settings write goes through the engine and is announced on the bus.

use bisa_core::settings::Origin;
use bisa_core::SettingScope;
use bisa_engine::{Engine, EngineConfig, EnginePayload};
use bisa_harness::HarnessCatalog;
use bisa_store::{MemoryKeyStore, Workspace};
use serde_json::json;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread")]
async fn a_write_is_resolved_back_and_announced() {
    let dir = tempfile::tempdir().unwrap();
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    let engine = Engine::start(
        ws,
        HarnessCatalog::new(),
        EngineConfig {
            design_enabled: false,
            events_enabled: false,
            ..Default::default()
        },
    )
    .unwrap();
    let mut bus = engine.events();

    let r = engine
        .set_setting(SettingScope::Workspace, None, "editor.tab_size", json!(2))
        .unwrap();
    assert_eq!(r.value, json!(2));
    assert_eq!(r.origin, Origin::Workspace);

    let event = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let e = bus.recv().await.unwrap();
            if let EnginePayload::SettingsChanged {
                scope,
                keys,
                project,
            } = e.payload
            {
                return (scope, keys, project);
            }
        }
    })
    .await
    .expect("settings.changed on the bus");
    assert_eq!(
        event,
        (
            "workspace".to_string(),
            vec!["editor.tab_size".to_string()],
            None
        )
    );

    // A refused write announces nothing and changes nothing.
    assert!(engine
        .set_setting(SettingScope::Project, None, "editor.font_size", json!(14))
        .is_err());
    let r = engine
        .unset_setting(SettingScope::Workspace, None, "editor.tab_size")
        .unwrap();
    assert_eq!(r.origin, Origin::Default);
    engine.shutdown().await;
}

/// What the save route is sized from is what the registry lets the setting
/// say: a bound raised there and not here would refuse a save at a size the
/// read side had called editable. And the bounds nobody set are the
/// registry's own defaults, said once.
#[test]
fn the_editors_bounds_are_the_registrys() {
    use bisa_core::settings::{Kind, REGISTRY};
    use bisa_engine::ide::files::EditorCaps;
    const MIB: u64 = 1024 * 1024;
    let of = |key: &str| {
        let def = REGISTRY
            .iter()
            .find(|def| def.key == key)
            .unwrap_or_else(|| panic!("`{key}` is a setting"));
        let Kind::Integer { max, .. } = def.kind else {
            panic!("`{key}` is an integer: {:?}", def.kind);
        };
        (
            def.default.as_u64().expect("its default") * MIB,
            max as u64 * MIB,
        )
    };
    let (editable, most_editable) = of("editor.large_file.editable_mib");
    let (refuse, most_refused) = of("editor.large_file.refuse_mib");
    assert_eq!(
        EditorCaps::UNSET,
        EditorCaps { editable, refuse },
        "as nobody set them"
    );
    assert_eq!(EditorCaps::MOST_EDITABLE, most_editable);
    assert!(most_editable <= most_refused, "what is edited is read");
}
