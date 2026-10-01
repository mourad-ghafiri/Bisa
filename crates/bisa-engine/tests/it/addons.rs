//! Addons through the engine: every door says what moved on the bus, and the
//! network broker refuses by name before a socket opens. No request leaves
//! this process — every path below stops at a rule, which is the point.

use crate::common::*;
use bisa_core::{AddonId, AddonPermission, SettingScope};
use bisa_engine::addons::{addon_fetch, cap_chunks, AddonsChange, MAX_FETCH_BYTES};
use bisa_engine::{admin, EnginePayload};
use bisa_harness::mock::MockAdapter;
use bisa_store::CatalogKind;
use serde_json::json;

fn id(s: &str) -> AddonId {
    AddonId::new(s).unwrap()
}

fn is_change(e: &bisa_engine::EngineEvent, what: &str, addon: &str) -> bool {
    matches!(
        &e.payload,
        EnginePayload::AddonsChanged { what: change }
            if change.as_str() == what && change.id().as_str() == addon
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn every_door_says_what_moved() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let mut rx = engine.events();
    let inner = engine.inner();

    let installed = admin::install_catalog_entry(inner, CatalogKind::Addon, "clock").unwrap();
    assert_eq!(installed.addons, vec!["clock".to_string()]);
    wait_for(&mut rx, "installed", |e| is_change(e, "installed", "clock")).await;

    admin::set_addon_enabled(inner, &id("clock"), false).unwrap();
    wait_for(&mut rx, "disabled", |e| is_change(e, "disabled", "clock")).await;
    admin::set_addon_enabled(inner, &id("clock"), true).unwrap();
    wait_for(&mut rx, "enabled", |e| is_change(e, "enabled", "clock")).await;

    admin::set_addon_grants(inner, &id("clock"), vec![]).unwrap();
    wait_for(&mut rx, "grants", |e| is_change(e, "grants", "clock")).await;
    assert!(engine
        .workspace()
        .get_addon(&id("clock"))
        .unwrap()
        .record
        .granted
        .is_empty());

    admin::remove_addon(inner, &id("clock")).unwrap();
    wait_for(&mut rx, "removed", |e| is_change(e, "removed", "clock")).await;
    assert!(engine.workspace().get_addon(&id("clock")).is_err());

    // A folder of the person's own goes through the same door, with the
    // grants they chose and nothing more.
    let src = tempfile::tempdir().unwrap();
    let folder = src.path().join("byte");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        folder.join("addon.json"),
        r#"{"id":"acme.byte","name":"Byte","description":"d","version":"1.0.0","license":"MIT","permissions":["storage","notify"]}"#,
    )
    .unwrap();
    std::fs::write(
        folder.join("index.html"),
        "<script src=\"bisa-addon.js\"></script>",
    )
    .unwrap();
    let addon = admin::install_addon(inner, &folder, vec![AddonPermission::Storage], true).unwrap();
    assert_eq!(addon.record.granted, vec![AddonPermission::Storage]);
    wait_for(&mut rx, "installed", |e| {
        is_change(e, "installed", "acme.byte")
    })
    .await;
    let change = AddonsChange::Installed {
        id: id("acme.byte"),
    };
    assert_eq!(change.as_str(), "installed");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_broker_refuses_by_name_before_anything_leaves() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let inner = engine.inner();
    let weather = id("weather");
    let url = "https://api.open-meteo.com/v1/forecast?latitude=1&longitude=2";

    // Unknown addon: the store's 404.
    let err = addon_fetch(inner, &weather, url, None).await.unwrap_err();
    assert!(err.to_string().contains("not found"), "{err}");

    admin::install_catalog_entry(inner, CatalogKind::Addon, "weather").unwrap();

    // The URL's shape and scheme, before any host is looked at.
    for (bad, word) in [
        ("http://api.open-meteo.com/v1", "https"),
        ("api.open-meteo.com/v1", "not a URL"),
        ("https://user@api.open-meteo.com/v1", "not a URL"),
        ("ftp://api.open-meteo.com/v1", "https"),
    ] {
        let err = addon_fetch(inner, &weather, bad, None).await.unwrap_err();
        assert!(err.to_string().contains(word), "{bad}: {err}");
    }
    // This machine, never — whatever the addon declared.
    for local in [
        "https://127.0.0.1:4477/health",
        "https://localhost/",
        "https://[::1]/",
    ] {
        let err = addon_fetch(inner, &weather, local, None).await.unwrap_err();
        assert!(err.to_string().contains("this machine"), "{local}: {err}");
    }
    // A host it did not declare.
    let err = addon_fetch(inner, &weather, "https://example.com/x", None)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("does not declare") || err.to_string().contains("not a host"),
        "{err}"
    );
    // An Accept header that is not printable ASCII.
    let err = addon_fetch(inner, &weather, url, Some("text/\u{7}"))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("Accept"), "{err}");

    // The person's deny list wins over the declaration, and the refusal is
    // read the way a connector's is.
    engine
        .workspace()
        .set_setting(
            SettingScope::Workspace,
            None,
            "security.net.deny_hosts",
            json!(["api.open-meteo.com"]),
        )
        .unwrap();
    inner.security.invalidate();
    let err = addon_fetch(inner, &weather, url, None).await.unwrap_err();
    assert!(err.to_string().contains("deny_hosts"), "{err}");

    // Not granted: the grant is what opens the broker, not the declaration.
    admin::set_addon_grants(inner, &weather, vec![AddonPermission::Storage]).unwrap();
    let err = addon_fetch(inner, &weather, url, None).await.unwrap_err();
    assert!(err.to_string().contains("not granted"), "{err}");

    // Disabled: nothing is done for an addon that does not run.
    admin::set_addon_enabled(inner, &weather, false).unwrap();
    let err = addon_fetch(inner, &weather, url, None).await.unwrap_err();
    assert!(err.to_string().contains("not enabled"), "{err}");

    // The machine's switch, before everything.
    engine
        .workspace()
        .set_setting(SettingScope::Machine, None, "addons.enabled", json!(false))
        .unwrap();
    let err = addon_fetch(inner, &weather, url, None).await.unwrap_err();
    assert!(err.to_string().contains("switched off"), "{err}");
    assert!(!bisa_engine::addons::addons_enabled(inner));
}

#[test]
fn the_body_cap_is_one_megabyte_and_says_when_it_cut() {
    let big = vec![b'x'; MAX_FETCH_BYTES + 5];
    let (kept, truncated) = cap_chunks(&[&big], MAX_FETCH_BYTES);
    assert_eq!(kept.len(), MAX_FETCH_BYTES);
    assert!(truncated);
    let (kept, truncated) = cap_chunks(&[b"ok"], MAX_FETCH_BYTES);
    assert_eq!(kept, b"ok");
    assert!(!truncated);
}
