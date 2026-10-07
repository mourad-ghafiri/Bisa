//! Settings at their edges (ide/13): a layer file that does not parse, a
//! project scope for a project that is gone, and several writers to one
//! scope at once — every key each of them set is there afterwards.

use bisa_core::SettingScope as Scope;
use bisa_store::{MemoryKeyStore, ProblemKind, Workspace};
use serde_json::json;
use std::sync::Arc;

fn ws() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    (dir, ws)
}

/// A layer file that does not parse: the layer's own read refuses by name;
/// the resolution stands on the other layers and names the file as a
/// problem rather than failing every reader of `settings()` — an engine, a
/// pump, a logger at boot; and a write repairs the scope by moving the torn
/// file aside whole, never writing over the evidence.
#[test]
fn a_settings_file_that_does_not_parse_is_named_costs_its_values_and_is_moved_aside_by_a_write() {
    let (_d, ws) = ws();
    ws.set_setting(Scope::Workspace, None, "editor.tab_size", json!(2))
        .unwrap();
    let path = ws.paths().workspace_settings();
    let torn = "{ \"editor.tab_size\": 2, ";
    std::fs::write(&path, torn).unwrap();
    let read = ws.settings_layer(Scope::Workspace, None).unwrap_err();
    assert!(read.to_string().contains("not a settings file"), "{read}");
    assert!(
        read.to_string().contains(&path.display().to_string()),
        "names the file: {read}"
    );
    // The resolution does not fail: the torn layer costs its values and is
    // named, and the defaults stand where it would have spoken.
    let resolved = ws.settings(None).unwrap();
    let tab = resolved
        .iter()
        .find(|r| r.key == "editor.tab_size")
        .expect("the key");
    assert_ne!(tab.value, json!(2), "the torn layer's value is not applied");
    assert!(
        ws.problems()
            .iter()
            .any(|p| p.kind == ProblemKind::SettingsLayerUnreadable
                && p.path == path.display().to_string()),
        "{:?}",
        ws.problems()
    );
    // A write repairs the scope: the torn file is moved aside with its bytes
    // as they were, and the layer is written anew with this one key.
    ws.set_setting(Scope::Workspace, None, "editor.tab_size", json!(4))
        .unwrap();
    let moved = ws
        .problems()
        .iter()
        .find_map(|p| (p.kind == ProblemKind::Quarantined).then(|| p.quarantined.clone()))
        .flatten()
        .expect("moved aside");
    assert_eq!(std::fs::read_to_string(moved).unwrap(), torn);
    let layer = ws.settings_layer(Scope::Workspace, None).unwrap();
    assert_eq!(layer.get("editor.tab_size"), Some(&json!(4)));
}

#[test]
fn a_project_scope_for_a_project_that_is_gone_is_refused() {
    let (_d, ws) = ws();
    let project = ws
        .create_project(bisa_store::NewProject::managed("web-app").unwrap())
        .unwrap();
    ws.set_setting(
        Scope::Project,
        Some(project.id),
        "editor.tab_size",
        json!(2),
    )
    .unwrap();
    ws.delete_project(project.id).unwrap();
    assert!(ws
        .set_setting(
            Scope::Project,
            Some(project.id),
            "editor.tab_size",
            json!(4)
        )
        .is_err());
    assert!(ws.settings_layer(Scope::Project, Some(project.id)).is_err());
}

/// A write is read the layer, change one key, write the layer; six writers
/// on six keys, five rounds each, and every key holds its last write.
#[test]
fn several_writers_to_one_scope_at_once_lose_no_key() {
    let (_d, ws) = ws();
    let ws = Arc::new(ws);
    let keys = [
        "logging.keep_files",
        "cache.codehost.ttl_ms",
        "cache.codehost.max_entries",
        "events.tick_secs",
        "cache.harness_models.ttl_ms",
        "cache.path_index.max_roots",
    ];
    std::thread::scope(|scope| {
        for (n, key) in keys.iter().enumerate() {
            let ws = Arc::clone(&ws);
            scope.spawn(move || {
                for round in 0..5u64 {
                    ws.set_setting(Scope::Machine, None, key, json!(10 + n as u64 + round))
                        .unwrap();
                }
            });
        }
    });
    let layer = ws.settings_layer(Scope::Machine, None).unwrap();
    for (n, key) in keys.iter().enumerate() {
        assert_eq!(
            layer.get(*key),
            Some(&json!(14 + n as u64)),
            "{key} holds its last write"
        );
    }
}
