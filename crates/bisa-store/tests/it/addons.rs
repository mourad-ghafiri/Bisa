//! Addons: a folder becomes a record and a bundle, and every rule is
//! refused before a byte is written.
//!
//! Fixtures live in `tempfile` directories released by `Drop`; nothing here
//! deletes anything itself.

use bisa_core::addon::{MAX_ADDON_FILES, MAX_ADDON_FILE_BYTES, SDK_FILE_NAME};
use bisa_core::{AddonId, AddonPermission, Origin};
use bisa_store::{CatalogKind, MemoryKeyStore, Paths, Workspace};
use std::path::{Path, PathBuf};

fn ws() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    (dir, ws)
}

fn id(s: &str) -> AddonId {
    AddonId::new(s).unwrap()
}

/// A manifest with every permission a test may want to grant.
fn manifest(id: &str) -> String {
    format!(
        r#"{{"id":"{id}","name":"Byte","description":"a widget","version":"1.0.0","license":"MIT",
            "tags":["tool"],"window":{{"width":200,"height":120}},
            "permissions":["storage","theme",{{"network":{{"hosts":["api.example.com"]}}}}]}}"#
    )
}

/// A folder inside `root`: the manifest and `(path, bytes)` files.
fn folder(root: &Path, name: &str, manifest: &str, files: &[(&str, &[u8])]) -> PathBuf {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("addon.json"), manifest).unwrap();
    for (rel, bytes) in files {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }
    dir
}

const PAGE: &[u8] =
    b"<!doctype html><script src=\"bisa-addon.js\"></script><script src=\"main.js\"></script>";

fn plain(root: &Path, name: &str, id: &str) -> PathBuf {
    folder(
        root,
        name,
        &manifest(id),
        &[
            ("index.html", PAGE),
            ("main.js", b"// x"),
            ("img/a.png", b"png"),
        ],
    )
}

#[test]
fn a_folder_installs_as_a_record_and_a_bundle_with_the_grants_chosen() {
    let (_dir, ws) = ws();
    let src = tempfile::tempdir().unwrap();
    let from = plain(src.path(), "byte", "acme.byte");
    let granted = vec![AddonPermission::Storage];
    let addon = ws.install_addon(&from, granted.clone(), false).unwrap();
    assert_eq!(addon.id().as_str(), "acme.byte");
    assert_eq!(addon.record.origin, Origin::Local);
    assert!(!addon.record.enabled);
    assert_eq!(addon.record.granted, granted);
    assert!(addon.files_present && !addon.is_active());

    let paths = Paths::new(ws.root());
    let a = id("acme.byte");
    assert!(paths.addon_record(&a).is_file());
    assert!(paths.addon_files_dir(&a).join("img/a.png").is_file());
    assert!(
        !paths.addon_files_dir(&a).join("addon.json").exists(),
        "the manifest is the record's, not a bundle file"
    );
    assert!(
        paths
            .state_dir(Paths::NS_ADDONS)
            .join("33407-acme.byte.json")
            .is_file(),
        "the record is snapshotted under the addon kind"
    );
    let listed = ws.list_addons().unwrap();
    assert_eq!(listed, vec![addon.clone()]);
    assert_eq!(ws.get_addon(&a).unwrap(), addon);
    assert!(
        !std::fs::read_dir(paths.addons_dir())
            .unwrap()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().starts_with(".staging")),
        "no staging folder is left behind"
    );
}

#[test]
fn enabling_granting_and_removing_write_the_record_and_its_snapshot() {
    let (_dir, ws) = ws();
    let src = tempfile::tempdir().unwrap();
    let from = plain(src.path(), "byte", "acme.byte");
    ws.install_addon(&from, vec![], false).unwrap();
    let a = id("acme.byte");

    let on = ws.set_addon_enabled(&a, true).unwrap();
    assert!(on.is_active());
    assert!(ws.get_addon(&a).unwrap().record.enabled);

    let net = AddonPermission::Network {
        hosts: vec!["api.example.com".into()],
    };
    let granted = ws
        .set_addon_grants(&a, vec![AddonPermission::Theme, net.clone()])
        .unwrap();
    assert_eq!(granted.record.network_hosts().unwrap(), ["api.example.com"]);

    // A grant never declared, or a declared one widened, is refused by word.
    let err = ws
        .set_addon_grants(&a, vec![AddonPermission::Notify])
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("notify") && err.contains("never declared"),
        "{err}"
    );
    let widened = AddonPermission::Network {
        hosts: vec!["evil.example".into()],
    };
    let err = ws
        .set_addon_grants(&a, vec![widened])
        .unwrap_err()
        .to_string();
    assert!(err.contains("network"), "{err}");
    assert_eq!(
        ws.get_addon(&a).unwrap().record.granted,
        vec![AddonPermission::Theme, net]
    );

    let paths = Paths::new(ws.root());
    let snapshot = paths
        .state_dir(Paths::NS_ADDONS)
        .join("33407-acme.byte.json");
    ws.remove_addon(&a).unwrap();
    assert!(!paths.addon_dir(&a).exists());
    assert!(!snapshot.exists());
    assert!(ws.list_addons().unwrap().is_empty());
    assert!(matches!(
        ws.get_addon(&a),
        Err(bisa_store::StoreError::DefinitionNotFound { kind: "addon", .. })
    ));
    assert!(ws.remove_addon(&a).is_err());
}

#[test]
fn a_bundle_file_resolves_inside_the_bundle_and_never_outside() {
    let (_dir, ws) = ws();
    let src = tempfile::tempdir().unwrap();
    let from = plain(src.path(), "byte", "acme.byte");
    ws.install_addon(&from, vec![], true).unwrap();
    let a = id("acme.byte");
    assert!(ws.get_addon_file(&a, "index.html").unwrap().is_file());
    assert!(ws.get_addon_file(&a, "img/a.png").unwrap().is_file());
    for bad in ["../addon.json", "/etc/hosts", ".hidden", "img/../../x", ""] {
        assert!(ws.get_addon_file(&a, bad).is_err(), "{bad:?}");
    }
    // A path that does not exist still resolves inside: the caller's 404.
    let missing = ws.get_addon_file(&a, "nope.js").unwrap();
    // The boundary is canonicalised before anything is compared against it.
    let files = Paths::new(ws.root())
        .addon_files_dir(&a)
        .canonicalize()
        .unwrap();
    assert!(missing.starts_with(&files), "{}", missing.display());
    #[cfg(unix)]
    {
        // A link planted after the install points out; the boundary holds.
        let link = Paths::new(ws.root()).addon_files_dir(&a).join("leak.css");
        std::os::unix::fs::symlink(src.path().join("byte/addon.json"), &link).unwrap();
        assert!(ws.get_addon_file(&a, "leak.css").is_err());
    }
}

#[test]
fn every_folder_rule_refuses_before_anything_is_written() {
    let (_dir, ws) = ws();
    let src = tempfile::tempdir().unwrap();
    let paths = Paths::new(ws.root());
    let refuse = |name: &str, manifest: &str, files: &[(&str, &[u8])], word: &str| {
        let from = folder(src.path(), name, manifest, files);
        let err = ws
            .install_addon(&from, vec![], false)
            .unwrap_err()
            .to_string();
        assert!(err.contains(word), "{name}: {err}");
        assert!(
            ws.list_addons().unwrap().is_empty() && !paths.addons_dir().join("acme.byte").exists(),
            "{name}: nothing written"
        );
    };
    refuse(
        "no-entry",
        &manifest("acme.byte"),
        &[("main.js", b"")],
        "index.html",
    );
    refuse(
        "reserved",
        &manifest("acme.byte"),
        &[("index.html", PAGE), (SDK_FILE_NAME, b"evil")],
        SDK_FILE_NAME,
    );
    refuse(
        "exe",
        &manifest("acme.byte"),
        &[("index.html", PAGE), ("tool.exe", b"MZ")],
        "tool.exe",
    );
    refuse(
        "dotfile",
        &manifest("acme.byte"),
        &[("index.html", PAGE), (".env", b"SECRET")],
        "hidden",
    );
    let big = vec![0u8; (MAX_ADDON_FILE_BYTES + 1) as usize];
    refuse(
        "big",
        &manifest("acme.byte"),
        &[("index.html", PAGE), ("big.png", &big)],
        "bytes",
    );
    let many: Vec<String> = (0..MAX_ADDON_FILES).map(|i| format!("f{i}.js")).collect();
    let mut files: Vec<(&str, &[u8])> = many.iter().map(|f| (f.as_str(), &b""[..])).collect();
    files.push(("index.html", PAGE));
    refuse("many", &manifest("acme.byte"), &files, "files");
    refuse(
        "bad-manifest",
        "{ not json",
        &[("index.html", PAGE)],
        "parse",
    );
    refuse(
        "problems",
        r#"{"id":"acme.byte","name":"","description":"d","version":"x","license":"MIT"}"#,
        &[("index.html", PAGE)],
        "refused",
    );
    refuse(
        "built-in",
        &manifest("clock"),
        &[("index.html", PAGE)],
        "ships with the platform",
    );
    refuse(
        "state",
        &manifest("state"),
        &[("index.html", PAGE)],
        "keeps for itself",
    );
    #[cfg(unix)]
    {
        let dir = folder(
            src.path(),
            "link",
            &manifest("acme.byte"),
            &[("index.html", PAGE)],
        );
        std::os::unix::fs::symlink(src.path().join("no-entry/main.js"), dir.join("main.js"))
            .unwrap();
        let err = ws
            .install_addon(&dir, vec![], false)
            .unwrap_err()
            .to_string();
        assert!(err.contains("symbolic link"), "{err}");
    }
    // A grant that was never declared refuses the install too.
    let from = plain(src.path(), "grant", "acme.byte");
    let err = ws
        .install_addon(&from, vec![AddonPermission::Notify], false)
        .unwrap_err()
        .to_string();
    assert!(err.contains("notify"), "{err}");
    assert!(ws.list_addons().unwrap().is_empty());
    // A folder without a manifest is not an addon at all.
    let empty = src.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let err = ws
        .install_addon(&empty, vec![], false)
        .unwrap_err()
        .to_string();
    assert!(err.contains("addon.json"), "{err}");
    // Installing twice under one id is refused, not merged.
    ws.install_addon(&from, vec![], false).unwrap();
    let err = ws
        .install_addon(&from, vec![], false)
        .unwrap_err()
        .to_string();
    assert!(err.contains("already installed"), "{err}");
}

#[test]
fn validating_a_folder_names_every_problem_and_writes_nothing() {
    let (_dir, ws) = ws();
    let src = tempfile::tempdir().unwrap();
    let from = folder(
        src.path(),
        "bad",
        r#"{"id":"clock","name":"","description":"d","version":"1.0.0","license":"MIT","permissions":[{"network":{"hosts":["localhost"]}}]}"#,
        &[("main.js", b"")],
    );
    let (manifest, problems) = ws.validate_addon_dir(&from).unwrap();
    assert_eq!(manifest.id.as_str(), "clock");
    let fields: Vec<&str> = problems.iter().filter_map(|p| p.field.as_deref()).collect();
    assert!(fields.contains(&"name"), "{fields:?}");
    assert!(fields.contains(&"permissions.0"), "{fields:?}");
    assert!(fields.contains(&"files"), "the missing entry: {fields:?}");
    assert!(fields.contains(&"id"), "a built-in's id: {fields:?}");
    assert!(ws.list_addons().unwrap().is_empty());
    let good = plain(src.path(), "good", "acme.byte");
    assert!(ws.validate_addon_dir(&good).unwrap().1.is_empty());
    assert!(ws.list_addons().unwrap().is_empty());
    let offers = ws.list_addon_offers().unwrap();
    assert_eq!(offers.len(), 13);
    assert!(offers
        .iter()
        .all(|o| !o.installed && o.slug == o.manifest.id.as_str()));
}

#[test]
fn a_catalog_addon_installs_enabled_with_its_declared_grants_and_only_once() {
    let (_dir, ws) = ws();
    let installed = ws.install(CatalogKind::Addon, "weather").unwrap();
    assert_eq!(installed.addons, vec!["weather".to_string()]);
    let weather = ws.get_addon(&id("weather")).unwrap();
    assert!(weather.is_active());
    assert_eq!(
        weather.record.origin,
        Origin::Catalog {
            slug: "weather".into()
        }
    );
    assert_eq!(weather.record.granted, weather.record.manifest.permissions);
    assert_eq!(
        weather.record.network_hosts().unwrap(),
        ["api.open-meteo.com", "geocoding-api.open-meteo.com"]
    );
    assert!(ws
        .install(CatalogKind::Addon, "weather")
        .unwrap()
        .is_empty());
    let entries = ws.catalog_entries(Some(CatalogKind::Addon)).unwrap();
    assert_eq!(entries.len(), 13);
    let row = entries.iter().find(|e| e.slug == "weather").unwrap();
    assert!(row.installed && row.requires.is_empty());
    assert!(
        !entries
            .iter()
            .find(|e| e.slug == "clock")
            .unwrap()
            .installed
    );
    // A person's own addon may not take a built-in's id, and a built-in
    // removed can be installed again.
    ws.remove_addon(&id("weather")).unwrap();
    assert!(!ws
        .install(CatalogKind::Addon, "weather")
        .unwrap()
        .is_empty());
    assert!("addon".parse::<CatalogKind>().unwrap() == CatalogKind::Addon);
}

#[test]
fn a_built_in_the_binary_outgrew_is_refreshed_at_open_and_one_at_its_version_is_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let open =
        || Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    let ws = open();
    ws.install(CatalogKind::Addon, "weather").unwrap();
    ws.install(CatalogKind::Addon, "clock").unwrap();
    ws.set_addon_enabled(&id("weather"), false).unwrap();
    let paths = Paths::new(ws.root());
    let shipped = ws.get_addon(&id("weather")).unwrap().record;
    let clock_before = ws.get_addon(&id("clock")).unwrap();
    // Age the installed weather: an older version than the platform's — the
    // built-ins wear the platform's version — a permission the shipped manifest
    // never declares — granted — a stale page and a file of its own.
    let mut aged: serde_json::Value =
        serde_json::from_slice(&std::fs::read(paths.addon_record(&id("weather"))).unwrap())
            .unwrap();
    aged["manifest"]["version"] = "0.0.1".into();
    aged["manifest"]["permissions"]
        .as_array_mut()
        .unwrap()
        .push("clipboard_write".into());
    aged["granted"]
        .as_array_mut()
        .unwrap()
        .push("clipboard_write".into());
    std::fs::write(
        paths.addon_record(&id("weather")),
        serde_json::to_vec(&aged).unwrap(),
    )
    .unwrap();
    let files = paths.addon_files_dir(&id("weather"));
    std::fs::write(files.join("main.js"), b"// stale").unwrap();
    std::fs::write(files.join("old.js"), b"// gone").unwrap();
    drop(ws);

    let ws = open();
    let weather = ws.get_addon(&id("weather")).unwrap();
    assert_eq!(
        weather.record.manifest, shipped.manifest,
        "the shipped manifest again"
    );
    assert!(!weather.record.enabled, "the person's switch stays");
    assert_eq!(weather.record.installed_at, shipped.installed_at);
    assert_eq!(
        weather.record.granted, shipped.granted,
        "a grant the new manifest does not declare is dropped; the rest stay"
    );
    assert!(weather.files_present);
    assert_ne!(std::fs::read(files.join("main.js")).unwrap(), b"// stale");
    assert!(
        !files.join("old.js").exists(),
        "the whole bundle is the shipped one"
    );
    assert!(
        !std::fs::read_dir(paths.addons_dir())
            .unwrap()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().starts_with('.')),
        "no staging or retired folder stays behind"
    );
    assert_eq!(
        ws.get_addon(&id("clock")).unwrap(),
        clock_before,
        "a built-in at its shipped version is untouched"
    );
    // A second open changes nothing more.
    drop(ws);
    let ws = open();
    assert_eq!(ws.get_addon(&id("weather")).unwrap(), weather);
}

#[test]
fn a_record_from_a_peer_lists_with_its_files_absent_and_cannot_be_enabled() {
    // The same person on two machines: one installs a built-in; the other —
    // opened with the same owner key — ingests the snapshot the first wrote,
    // and holds the record alone.
    let (_a, alpha) = ws();
    let twin_store = MemoryKeyStore::default();
    bisa_store::KeyStore::set(
        &twin_store,
        "owner",
        &alpha.owner_keys().secret_key().to_secret_hex(),
    )
    .unwrap();
    let _b = tempfile::tempdir().unwrap();
    let beta = Workspace::open_with_keystore(_b.path(), Box::new(twin_store)).unwrap();
    assert_eq!(beta.owner_principal(), alpha.owner_principal());
    alpha.install(CatalogKind::Addon, "clock").unwrap();
    let snapshot = Paths::new(alpha.root())
        .state_dir(Paths::NS_ADDONS)
        .join("33407-clock.json");
    let event: nostr::event::Event =
        serde_json::from_slice(&std::fs::read(&snapshot).unwrap()).unwrap();
    let outcome = beta.ingest_remote_event(&event).unwrap();
    assert!(
        !format!("{outcome:?}").to_lowercase().contains("rejected"),
        "{outcome:?}"
    );
    let clock = beta.get_addon(&id("clock")).unwrap();
    assert!(!clock.files_present && !clock.is_active());
    assert_eq!(clock.record.manifest.name, "Clock");
    let err = beta
        .set_addon_enabled(&id("clock"), true)
        .unwrap_err()
        .to_string();
    assert!(err.contains("no files on this machine"), "{err}");
    assert!(beta.get_addon_file(&id("clock"), "index.html").is_err());
    // Installing the catalog entry here brings the files and keeps the id.
    assert!(!Paths::new(beta.root())
        .addon_files_dir(&id("clock"))
        .exists());
}
