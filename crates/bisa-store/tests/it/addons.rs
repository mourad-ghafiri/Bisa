//! Addons: a folder becomes a record and a bundle, and every rule is
//! refused before a byte is written.
//!
//! Fixtures live in `tempfile` directories released by `Drop`; nothing here
//! deletes anything itself.

use bisa_core::addon::{MAX_ADDON_FILES, MAX_ADDON_FILE_BYTES, SDK_FILE_NAME};
use bisa_core::{AddonId, AddonPermission, AddonRecord, Origin};
use bisa_store::snapshots::SnapshotStore;
use bisa_store::{AddonChange, CatalogKind, GrantChange, MemoryKeyStore, Paths, Workspace};
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

/// The snapshot a record was written as, read back as the event it is.
fn snapshot_of(ws: &Workspace, id: &str) -> nostr::event::Event {
    let path = Paths::new(ws.root())
        .state_dir(Paths::NS_ADDONS)
        .join(format!("33407-{id}.json"));
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
fn two_switches_of_one_addon_at_once_both_land_under_the_write_lock() {
    let (_dir, ws) = ws();
    let src = tempfile::tempdir().unwrap();
    let from = plain(src.path(), "byte", "acme.byte");
    ws.install_addon(&from, vec![], false).unwrap();
    let a = id("acme.byte");
    // Eight writers at once — four switches, four grants — each a read, a
    // judgement and a write. Without one writer at a time, two read the
    // same revision and the second's snapshot is refused as stale behind a
    // record file it had already rewritten.
    std::thread::scope(|s| {
        for i in 0..8 {
            let ws = &ws;
            let a = &a;
            s.spawn(move || {
                let outcome = if i % 2 == 0 {
                    ws.set_addon_enabled(a, i % 4 == 0).map(|_| ())
                } else {
                    let granted = if i % 4 == 1 {
                        vec![AddonPermission::Theme]
                    } else {
                        vec![]
                    };
                    ws.set_addon_grants(a, granted).map(|_| ())
                };
                outcome.unwrap_or_else(|e| panic!("writer {i}: {e}"));
            });
        }
    });
    let after = ws.get_addon(&a).unwrap();
    let event = snapshot_of(&ws, "acme.byte");
    let stored: AddonRecord = serde_json::from_str(&event.content).unwrap();
    assert_eq!(stored, after.record, "the snapshot is the record file");
    assert!(SnapshotStore::revision_of(&event) >= 2);
}

#[test]
fn a_patch_of_both_fields_is_judged_whole_and_one_that_changes_nothing_writes_nothing() {
    let (_dir, ws) = ws();
    let src = tempfile::tempdir().unwrap();
    let from = plain(src.path(), "byte", "acme.byte");
    ws.install_addon(&from, vec![], false).unwrap();
    let a = id("acme.byte");
    let paths = Paths::new(ws.root());
    let record_file = paths.addon_record(&a);
    let snapshot = paths
        .state_dir(Paths::NS_ADDONS)
        .join("33407-acme.byte.json");
    let on_disk = || {
        (
            std::fs::read(&record_file).unwrap(),
            std::fs::read(&snapshot).unwrap(),
        )
    };
    let before = on_disk();

    // An undeclared grant beside a switch: refused by word, and the switch
    // beside it never moved — nothing was written.
    let err = ws
        .update_addon(&a, Some(true), Some(vec![AddonPermission::Notify]))
        .unwrap_err()
        .to_string();
    assert!(err.contains("notify"), "{err}");
    assert_eq!(on_disk(), before, "a refused patch writes nothing");

    // Nothing to change: nothing written, nothing to say.
    let (entry, changes) = ws.update_addon(&a, Some(false), Some(vec![])).unwrap();
    assert!(!entry.record.enabled && changes.is_empty());
    assert_eq!(
        on_disk(),
        before,
        "a patch that changed nothing writes nothing"
    );

    // Both at once: one write, two changes in the order the bus says them.
    let (entry, changes) = ws
        .update_addon(&a, Some(true), Some(vec![AddonPermission::Theme]))
        .unwrap();
    assert!(entry.is_active());
    assert_eq!(changes, vec![AddonChange::Enabled, AddonChange::Grants]);
    assert_eq!(
        SnapshotStore::revision_of(&snapshot_of(&ws, "acme.byte")),
        2,
        "one snapshot for the two changes"
    );
    let (_, changes) = ws.update_addon(&a, Some(false), None).unwrap();
    assert_eq!(changes, vec![AddonChange::Disabled]);
    let (_, changes) = ws.update_addon(&a, None, Some(vec![])).unwrap();
    assert_eq!(changes, vec![AddonChange::Grants]);
    assert_eq!(
        SnapshotStore::revision_of(&snapshot_of(&ws, "acme.byte")),
        4
    );
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
    assert_eq!(offers.len(), 18);
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
    assert_eq!(entries.len(), 18);
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

#[test]
fn a_peers_record_never_overwrites_a_local_one() {
    // The same person on two machines, the clock installed and on in both;
    // the first switches it off and takes its grant back, and the second
    // ingests that later record — and keeps its own switch and grant: the
    // record file is this machine's, the peer's snapshot lands in the
    // snapshot store alone, and the next local write republishes past it.
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
    alpha.install(CatalogKind::Addon, "clock").unwrap();
    beta.install(CatalogKind::Addon, "clock").unwrap();
    alpha.set_addon_enabled(&id("clock"), false).unwrap();
    alpha.set_addon_grants(&id("clock"), vec![]).unwrap();
    let event = snapshot_of(&alpha, "clock");
    assert_eq!(SnapshotStore::revision_of(&event), 3);
    let outcome = beta.ingest_remote_event(&event).unwrap();
    assert!(
        !format!("{outcome:?}").to_lowercase().contains("rejected"),
        "{outcome:?}"
    );
    let clock = beta.get_addon(&id("clock")).unwrap();
    assert!(clock.is_active(), "the local switch stands");
    assert_eq!(clock.record.granted, vec![AddonPermission::Storage]);
    let (_, changes) = beta.update_addon(&id("clock"), None, Some(vec![])).unwrap();
    assert_eq!(changes, vec![AddonChange::Grants]);
    let mine = snapshot_of(&beta, "clock");
    assert_eq!(
        SnapshotStore::revision_of(&mine),
        4,
        "past the peer's revision"
    );
    let stored: AddonRecord = serde_json::from_str(&mine.content).unwrap();
    assert!(stored.enabled && stored.granted.is_empty());
}

#[test]
fn a_manifest_that_is_a_folder_or_too_large_a_name_that_is_not_text_and_a_folder_too_deep_are_refused(
) {
    let (_dir, ws) = ws();
    let src = tempfile::tempdir().unwrap();
    let folderish = src.path().join("folderish");
    std::fs::create_dir_all(folderish.join("addon.json")).unwrap();
    assert!(ws.validate_addon_dir(&folderish).is_err());
    let huge = src.path().join("huge");
    std::fs::create_dir_all(&huge).unwrap();
    std::fs::File::create(huge.join("addon.json"))
        .unwrap()
        .set_len(bisa_core::addon::MAX_MANIFEST_BYTES + 1)
        .unwrap();
    assert!(ws.validate_addon_dir(&huge).is_err());
    // Eight folders deep is the limit, and the limit is allowed — the constant, the refusal's own
    // sentence and the docs all say eight; the walk used to stop at seven.
    let eight = plain(src.path(), "eight", "acme.eight");
    let mut nested = eight.clone();
    for level in ["a", "b", "c", "d", "e", "f", "g", "h"] {
        nested = nested.join(level);
    }
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(nested.join("x.js"), b"// x").unwrap();
    let (_, problems) = ws.validate_addon_dir(&eight).unwrap();
    assert!(problems.is_empty(), "{problems:?}");
    let installed = ws.install_addon(&eight, vec![], false).unwrap();
    assert!(ws
        .get_addon_file(installed.id(), "a/b/c/d/e/f/g/h/x.js")
        .unwrap()
        .is_file());
    let deep = plain(src.path(), "deep", "acme.deep");
    let mut nested = deep.clone();
    for level in ["a", "b", "c", "d", "e", "f", "g", "h", "i"] {
        nested = nested.join(level);
    }
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(nested.join("x.js"), b"// x").unwrap();
    let (_, problems) = ws.validate_addon_dir(&deep).unwrap();
    assert!(
        problems.iter().any(|p| p.field.as_deref() == Some("files")),
        "{problems:?}"
    );
    assert!(ws.install_addon(&deep, vec![], false).is_err());
    let reserved = plain(src.path(), "reserved", "state");
    let (_, problems) = ws.validate_addon_dir(&reserved).unwrap();
    assert!(
        problems.iter().any(|p| p.field.as_deref() == Some("id")),
        "{problems:?}"
    );
    // APFS refuses a name that is not UTF-8 (EILSEQ); the arm is held where
    // a filesystem allows one.
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::ffi::OsStrExt;
        let odd = plain(src.path(), "odd", "acme.odd");
        std::fs::write(odd.join(std::ffi::OsStr::from_bytes(b"\xff.js")), b"// x").unwrap();
        let (_, problems) = ws.validate_addon_dir(&odd).unwrap();
        assert!(
            problems.iter().any(|p| p.field.as_deref() == Some("files")),
            "{problems:?}"
        );
    }
}

/// Every `DefinitionRemoved` the store said since `rx` was last drained.
fn removals(
    rx: &mut tokio::sync::broadcast::Receiver<bisa_store::StoreEvent>,
) -> Vec<(u16, String)> {
    let mut out = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        if let bisa_store::StoreEvent::DefinitionRemoved { kind, d } = ev {
            out.push((kind, d));
        }
    }
    out
}

#[test]
fn an_addon_whose_record_is_damaged_or_gone_is_still_removed_and_its_id_is_free_again() {
    // A record cut short, or a folder left with no record at all, used to wedge the id for good:
    // the list skipped it, an install was refused as "already installed; remove it first", and the
    // removal answered not found.
    let (_dir, ws) = ws();
    let src = tempfile::tempdir().unwrap();
    let from = plain(src.path(), "byte", "acme.byte");
    let paths = Paths::new(ws.root());
    let mut rx = ws.subscribe_store_events();
    for damage in ["cut short", "gone"] {
        ws.install_addon(&from, vec![], false).unwrap();
        let record = paths.addon_record(&id("acme.byte"));
        match damage {
            "cut short" => std::fs::write(&record, b"{\"manifest\":").unwrap(),
            _ => std::fs::remove_file(&record).unwrap(),
        }
        assert!(ws.get_addon(&id("acme.byte")).is_err(), "{damage}");
        assert!(
            ws.install_addon(&from, vec![], false).is_err(),
            "{damage}: the folder still holds the id"
        );
        removals(&mut rx);
        ws.remove_addon(&id("acme.byte"))
            .unwrap_or_else(|e| panic!("{damage}: {e}"));
        assert!(!paths.addon_dir(&id("acme.byte")).exists(), "{damage}");
        assert!(
            !paths
                .state_dir(Paths::NS_ADDONS)
                .join("33407-acme.byte.json")
                .exists(),
            "{damage}: its snapshot goes with it"
        );
        assert_eq!(
            removals(&mut rx),
            vec![(33407, "acme.byte".to_string())],
            "{damage}: the removal is said once on the store's stream"
        );
    }
    // The id is free: the folder installs again.
    ws.install_addon(&from, vec![], false).unwrap();
    // What was never here is still not found, and says nothing.
    let err = ws.remove_addon(&id("acme.nothing")).unwrap_err();
    assert!(
        matches!(err, bisa_store::StoreError::DefinitionNotFound { .. }),
        "{err}"
    );
    assert!(removals(&mut rx).is_empty());
    // Nothing is left under a staging name.
    let leftovers: Vec<_> = std::fs::read_dir(paths.addons_dir())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with('.'))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn a_write_that_landed_is_answered_as_landed_over_a_snapshot_that_will_not_read() {
    // The record was written, then reading the old snapshot's revision failed: the patch answered
    // an error for a change that was on disk, nothing was announced, and every later patch did the
    // same. A snapshot that will not read is moved aside and the count starts again.
    let (_dir, ws) = ws();
    let src = tempfile::tempdir().unwrap();
    let from = plain(src.path(), "byte", "acme.byte");
    ws.install_addon(&from, vec![], false).unwrap();
    let snapshot = Paths::new(ws.root())
        .state_dir(Paths::NS_ADDONS)
        .join("33407-acme.byte.json");
    std::fs::write(&snapshot, b"{x").unwrap();
    let (addon, changes) = ws.update_addon(&id("acme.byte"), Some(true), None).unwrap();
    assert!(addon.record.enabled);
    assert_eq!(changes, vec![AddonChange::Enabled]);
    assert!(ws.get_addon(&id("acme.byte")).unwrap().record.enabled);
    let event = snapshot_of(&ws, "acme.byte");
    let stored: AddonRecord = serde_json::from_str(&event.content).unwrap();
    assert!(stored.enabled, "the snapshot is written anew");
    // And the next patch is an ordinary one.
    let (_, changes) = ws
        .update_addon(&id("acme.byte"), Some(false), None)
        .unwrap();
    assert_eq!(changes, vec![AddonChange::Disabled]);
    assert_eq!(
        SnapshotStore::revision_of(&snapshot_of(&ws, "acme.byte")),
        2
    );
}

#[test]
fn a_built_in_whose_files_are_missing_gets_them_from_the_binary_at_any_version() {
    // A record from a peer, or a bundle folder lost on disk: the record stands at the shipped
    // version, so the refresh passed it by, the catalog install answered "nothing to do", an import
    // was refused — and the addon could never be switched on.
    let (_a, alpha) = ws();
    let twin_store = MemoryKeyStore::default();
    bisa_store::KeyStore::set(
        &twin_store,
        "owner",
        &alpha.owner_keys().secret_key().to_secret_hex(),
    )
    .unwrap();
    let b = tempfile::tempdir().unwrap();
    let beta = Workspace::open_with_keystore(b.path(), Box::new(twin_store)).unwrap();
    alpha.install(CatalogKind::Addon, "clock").unwrap();
    alpha
        .update_addon(&id("clock"), Some(false), Some(vec![]))
        .unwrap();
    let event = snapshot_of(&alpha, "clock");
    beta.ingest_remote_event(&event).unwrap();
    assert!(!beta.get_addon(&id("clock")).unwrap().files_present);

    // Installing the catalog entry brings the files, and says an addon was installed.
    let installed = beta.install(CatalogKind::Addon, "clock").unwrap();
    assert_eq!(installed.addons, vec!["clock".to_string()]);
    let clock = beta.get_addon(&id("clock")).unwrap();
    assert!(clock.files_present);
    assert!(
        !clock.record.enabled && clock.record.granted.is_empty(),
        "the switch and the grants the record carried are kept: {:?}",
        clock.record
    );
    assert!(beta
        .get_addon_file(&id("clock"), "index.html")
        .unwrap()
        .is_file());
    beta.set_addon_enabled(&id("clock"), true).unwrap();
    // A second install is the idempotent nothing it always was.
    assert!(beta
        .install(CatalogKind::Addon, "clock")
        .unwrap()
        .addons
        .is_empty());

    // The same at open: a bundle folder lost on disk comes back, the record's choices kept.
    let paths = Paths::new(beta.root());
    std::fs::rename(
        paths.addon_files_dir(&id("clock")),
        b.path().join("lost-files"),
    )
    .unwrap();
    assert!(!beta.get_addon(&id("clock")).unwrap().files_present);
    let again = {
        let keys = MemoryKeyStore::default();
        bisa_store::KeyStore::set(
            &keys,
            "owner",
            &beta.owner_keys().secret_key().to_secret_hex(),
        )
        .unwrap();
        drop(beta);
        Workspace::open_with_keystore(b.path(), Box::new(keys)).unwrap()
    };
    let clock = again.get_addon(&id("clock")).unwrap();
    assert!(clock.files_present && clock.record.enabled);
}

#[test]
fn grants_are_kept_in_the_order_declared_and_once_each_so_the_same_grants_are_no_change() {
    let (_dir, ws) = ws();
    let src = tempfile::tempdir().unwrap();
    let from = plain(src.path(), "byte", "acme.byte");
    let network = AddonPermission::Network {
        hosts: vec!["api.example.com".into()],
    };
    // Installed in another order and twice over: stored as declared, once.
    let addon = ws
        .install_addon(
            &from,
            vec![network.clone(), AddonPermission::Storage, network.clone()],
            false,
        )
        .unwrap();
    assert_eq!(
        addon.record.granted,
        vec![AddonPermission::Storage, network.clone()]
    );
    let rev = SnapshotStore::revision_of(&snapshot_of(&ws, "acme.byte"));
    // The same grants said in another order are nothing new: nothing written, nothing to announce.
    let (_, changes) = ws
        .update_addon(
            &id("acme.byte"),
            None,
            Some(vec![network.clone(), AddonPermission::Storage]),
        )
        .unwrap();
    assert!(changes.is_empty(), "{changes:?}");
    assert_eq!(
        SnapshotStore::revision_of(&snapshot_of(&ws, "acme.byte")),
        rev
    );
    // A real change is still one, and lands as declared.
    let (addon, changes) = ws
        .update_addon(
            &id("acme.byte"),
            None,
            Some(vec![
                AddonPermission::Theme,
                AddonPermission::Storage,
                AddonPermission::Theme,
            ]),
        )
        .unwrap();
    assert_eq!(changes, vec![AddonChange::Grants]);
    assert_eq!(
        addon.record.granted,
        vec![AddonPermission::Storage, AddonPermission::Theme]
    );
}

#[test]
fn what_a_crash_left_under_a_staging_name_is_cleared_at_open_and_a_fresh_one_is_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let keys = MemoryKeyStore::default;
    let ws = Workspace::open_with_keystore(dir.path(), Box::new(keys())).unwrap();
    let secret = ws.owner_keys().secret_key().to_secret_hex();
    let addons = Paths::new(ws.root()).addons_dir();
    drop(ws);
    std::fs::create_dir_all(addons.join(".staging-999-1/files")).unwrap();
    std::fs::write(addons.join(".staging-999-1/files/index.html"), PAGE).unwrap();
    std::fs::create_dir_all(addons.join(".staging-999-2")).unwrap();
    // The first is a day old — nothing is still assembling it; the second was made just now, by
    // another process of this workspace for all the open can tell.
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(86_400);
    std::fs::File::open(addons.join(".staging-999-1"))
        .unwrap()
        .set_modified(old)
        .unwrap();
    let again = keys();
    bisa_store::KeyStore::set(&again, "owner", &secret).unwrap();
    let ws = Workspace::open_with_keystore(dir.path(), Box::new(again)).unwrap();
    assert!(
        !addons.join(".staging-999-1").exists(),
        "the stale one is cleared"
    );
    assert!(
        addons.join(".staging-999-2").exists(),
        "the fresh one may be in use"
    );
    assert!(ws.list_addons().unwrap().is_empty());
}

#[test]
fn an_addons_folder_that_cannot_be_listed_never_stops_the_workspace_from_opening() {
    let dir = tempfile::tempdir().unwrap();
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    let secret = ws.owner_keys().secret_key().to_secret_hex();
    let addons = Paths::new(ws.root()).addons_dir();
    drop(ws);
    // Where the folder should be, a file: listing it fails.
    if addons.exists() {
        std::fs::rename(&addons, dir.path().join("addons-aside")).unwrap();
    }
    std::fs::write(&addons, b"not a folder").unwrap();
    let keys = MemoryKeyStore::default();
    bisa_store::KeyStore::set(&keys, "owner", &secret).unwrap();
    let ws = Workspace::open_with_keystore(dir.path(), Box::new(keys))
        .expect("the built-ins' refresh is warned past, never a stop to the open");
    assert!(ws.list_addons().is_err(), "the read itself still says so");
}

#[test]
fn a_finder_folder_installs_and_its_ds_store_is_left_behind() {
    let (_dir, ws) = ws();
    let src = tempfile::tempdir().unwrap();
    let from = plain(src.path(), "byte", "acme.byte");
    std::fs::write(from.join(".DS_Store"), b"\0\0\0\x01Bud1").unwrap();
    std::fs::write(from.join("img/.DS_Store"), b"\0\0\0\x01Bud1").unwrap();
    let (_, problems) = ws.validate_addon_dir(&from).unwrap();
    assert!(problems.is_empty(), "{problems:?}");
    let addon = ws.install_addon(&from, vec![], false).unwrap();
    let files = Paths::new(ws.root()).addon_files_dir(addon.id());
    assert!(!files.join(".DS_Store").exists() && !files.join("img/.DS_Store").exists());
    // Any other dotfile is still refused: a bundle carries nothing hidden.
    std::fs::write(from.join(".env"), b"KEY=1").unwrap();
    let (_, problems) = ws.validate_addon_dir(&from).unwrap();
    assert!(
        problems.iter().any(|p| p.field.as_deref() == Some("files")),
        "{problems:?}"
    );
}

#[test]
fn a_record_that_names_another_id_than_its_folder_is_not_that_addon() {
    let (_dir, ws) = ws();
    let src = tempfile::tempdir().unwrap();
    ws.install_addon(&plain(src.path(), "byte", "acme.byte"), vec![], false)
        .unwrap();
    let paths = Paths::new(ws.root());
    let record = paths.addon_record(&id("acme.byte"));
    let text = std::fs::read_to_string(&record).unwrap();
    std::fs::write(&record, text.replace("\"acme.byte\"", "\"acme.other\"")).unwrap();
    let err = ws.get_addon(&id("acme.byte")).unwrap_err();
    assert!(
        matches!(err, bisa_store::StoreError::Unreadable { .. }),
        "{err}"
    );
    // A patch never writes under the other id.
    assert!(ws.update_addon(&id("acme.byte"), Some(true), None).is_err());
    assert!(!paths.addon_dir(&id("acme.other")).exists());
    // And it can be removed.
    ws.remove_addon(&id("acme.byte")).unwrap();
}

#[test]
fn grants_are_given_and_taken_by_word_under_the_one_lock_so_two_hands_never_undo_each_other() {
    // Granting used to be read-then-replace: the list was read, changed by the caller and written
    // whole, so a grant made elsewhere between the two was lost. A word is now added or taken
    // inside the store, against the record as it stands.
    let (_dir, ws) = ws();
    let src = tempfile::tempdir().unwrap();
    ws.install_addon(&plain(src.path(), "byte", "acme.byte"), vec![], false)
        .unwrap();
    let aid = id("acme.byte");
    let words = |w: &[&str]| w.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let (addon, changes) = ws
        .update_addon_by(
            &aid,
            None,
            GrantChange::Edit {
                grant: words(&["theme"]),
                revoke: vec![],
            },
        )
        .unwrap();
    assert_eq!(changes, vec![AddonChange::Grants]);
    assert_eq!(addon.record.granted, vec![AddonPermission::Theme]);
    // Another hand grants storage; this one's `network` lands beside it, not over it.
    ws.update_addon_by(
        &aid,
        None,
        GrantChange::Edit {
            grant: words(&["storage"]),
            revoke: vec![],
        },
    )
    .unwrap();
    let (addon, _) = ws
        .update_addon_by(
            &aid,
            Some(true),
            GrantChange::Edit {
                grant: words(&["network", "storage"]),
                revoke: words(&["theme"]),
            },
        )
        .unwrap();
    assert!(
        addon.record.enabled,
        "the switch beside it lands in the same write"
    );
    assert_eq!(
        addon.record.granted,
        vec![
            AddonPermission::Storage,
            AddonPermission::Network {
                hosts: vec!["api.example.com".into()]
            }
        ],
        "as declared — the network grant with its hosts"
    );
    // A word given that is already granted, a word taken that is not: no change.
    let (_, changes) = ws
        .update_addon_by(
            &aid,
            None,
            GrantChange::Edit {
                grant: words(&["storage"]),
                revoke: words(&["theme"]),
            },
        )
        .unwrap();
    assert!(changes.is_empty(), "{changes:?}");
    // A word the manifest never declared refuses the whole patch, the switch untouched.
    let err = ws
        .update_addon_by(
            &aid,
            Some(false),
            GrantChange::Edit {
                grant: words(&["clipboard_write"]),
                revoke: vec![],
            },
        )
        .unwrap_err()
        .to_string();
    assert!(err.contains("clipboard_write"), "{err}");
    assert!(ws.get_addon(&aid).unwrap().record.enabled);
    let err = ws
        .update_addon_by(
            &aid,
            None,
            GrantChange::Edit {
                grant: vec![],
                revoke: words(&["nonsense"]),
            },
        )
        .unwrap_err()
        .to_string();
    assert!(err.contains("nonsense"), "{err}");
}

#[test]
fn a_folder_inside_the_workspace_is_no_source_for_an_addon_whichever_door_asks() {
    // The node refused it at its own door; the command line working without a node did not.
    let (_dir, ws) = ws();
    let inside = plain(ws.root(), "byte", "acme.byte");
    let err = ws
        .install_addon(&inside, vec![], false)
        .unwrap_err()
        .to_string();
    assert!(err.contains("inside the workspace"), "{err}");
    assert!(ws
        .validate_addon_dir(&inside)
        .unwrap_err()
        .to_string()
        .contains("inside the workspace"));
    assert!(ws.list_addons().unwrap().is_empty());
    // A folder that is not there is said as such, not as inside or outside anything.
    let missing = ws.root().join("nowhere");
    assert!(ws.install_addon(&missing, vec![], false).is_err());
}
