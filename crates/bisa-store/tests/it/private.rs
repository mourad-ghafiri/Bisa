//! What the store writes is the owner's alone: the data root `0700`, the
//! settings layers and the MCP registry `0600` from their first byte and
//! tightened at the next open when written wider before, and a key replaced
//! whole — never a moment with no key under its name.

#![cfg(unix)]

use bisa_core::SettingScope as Scope;
use bisa_core::{McpId, McpServerConfig, Tags};
use bisa_store::{FileKeyStore, KeyStore as _, MemoryKeyStore, NewMcp, Paths, Workspace};
use serde_json::json;
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn open(dir: &Path) -> Workspace {
    Workspace::open_with_keystore(dir, Box::new(MemoryKeyStore::default())).unwrap()
}

/// The MCP registry's file — a server's environment and headers — and every
/// settings layer — a proxy login — are the owner's alone from their first
/// write, and the root with them.
#[test]
fn the_mcp_truth_and_every_settings_layer_are_private() {
    let dir = tempfile::tempdir().unwrap();
    let ws = open(dir.path());
    let paths = Paths::new(dir.path());
    assert_eq!(mode(dir.path()), 0o700, "the root");

    ws.set_setting(Scope::Machine, None, "network.http1_only", json!(true))
        .unwrap();
    ws.set_setting(Scope::Workspace, None, "editor.tab_size", json!(2))
        .unwrap();
    assert_eq!(mode(&paths.machine_settings()), 0o600, "machine.json");
    assert_eq!(mode(&paths.workspace_settings()), 0o600, "settings.json");

    let server = ws
        .create_mcp(NewMcp {
            id: McpId::new("srv").unwrap(),
            description: "a server".into(),
            tags: Tags::default(),
            transport: McpServerConfig::Stdio {
                name: "srv".into(),
                command: "npx".into(),
                args: vec![],
                env: BTreeMap::from([("API_KEY".to_string(), "fake-value-0001".to_string())]),
                cwd: None,
            },
        })
        .unwrap();
    assert_eq!(mode(&paths.mcp_file(&server.id)), 0o600, "mcp/<id>.json");
    // No temporary is left beside any of them.
    for parent in [dir.path().to_path_buf(), paths.mcp_dir()] {
        let strays: Vec<String> = std::fs::read_dir(&parent)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp."))
            .collect();
        assert!(strays.is_empty(), "{}: {strays:?}", parent.display());
    }
}

/// A root made before this rule — or by hand — and files written wider are
/// the owner's alone after the next open; a file that is not there is nothing
/// to tighten, and the open never fails on it.
#[test]
fn the_data_root_is_private_after_open_even_when_it_was_not() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("bisa-data");
    std::fs::create_dir_all(root.join("mcp")).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
    let paths = Paths::new(&root);
    std::fs::write(paths.machine_settings(), "{}").unwrap();
    std::fs::set_permissions(
        paths.machine_settings(),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    let server = paths.mcp_dir().join("old.json");
    std::fs::write(&server, "{}").unwrap();
    std::fs::set_permissions(&server, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(mode(&root), 0o755, "the fixture is wide");

    let _ws = open(&root);
    assert_eq!(mode(&root), 0o700, "the root is the owner's alone");
    assert_eq!(mode(&paths.machine_settings()), 0o600);
    assert_eq!(
        mode(&server),
        0o600,
        "an MCP record written wider is tightened"
    );
    assert!(
        !paths.workspace_settings().exists(),
        "a file that is not there is nothing to tighten"
    );
}

/// A key replaced — a refreshed token, a rotated secret — reaches its name
/// by one rename: the old bytes or the new, never none, no temporary left,
/// the file still the owner's alone.
#[test]
fn replacing_a_key_leaves_the_name_whole() {
    let dir = tempfile::tempdir().unwrap();
    let identity = Paths::new(dir.path()).identity_dir();
    let ks = FileKeyStore::new(identity.clone());
    ks.set("hook:a", "aa").unwrap();
    ks.replace("hook:a", "bb").unwrap();
    assert_eq!(ks.get("hook:a").unwrap().as_deref(), Some("bb"));
    // A logical name's `:` is `_` on disk, so a file name stays tame.
    assert_eq!(mode(&identity.join("hook_a.key")), 0o600);
    let names: Vec<String> = std::fs::read_dir(&identity)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["hook_a.key".to_string()], "no temporary left");
    // A name nobody set is replaced into being, as the import path needs.
    ks.replace("hook:b", "cc").unwrap();
    assert_eq!(ks.get("hook:b").unwrap().as_deref(), Some("cc"));
}
