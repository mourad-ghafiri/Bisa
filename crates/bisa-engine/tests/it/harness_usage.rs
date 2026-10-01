//! A harness account's usage through the engine: the adapter's answer,
//! cached for the TTL, asked again on a refresh, never asked when reads are
//! off, and never kept when it failed. A mock adapter answers and counts
//! how often it was asked; nothing here runs a binary, reaches a provider,
//! or reads a credential.

use bisa_core::SettingScope;
use bisa_engine::{Engine, EngineConfig};
use bisa_harness::mock::MockAdapter;
use bisa_harness::{HarnessCatalog, UsageReport, UsageSource, UsageState, UsageWindow};
use bisa_store::{MemoryKeyStore, Workspace};
use serde_json::json;
use std::sync::atomic::Ordering;
use std::sync::Arc;

fn report(harness: &str) -> UsageState {
    UsageState::Report {
        report: UsageReport {
            harness: harness.into(),
            read_at: 1,
            source: UsageSource::Cli,
            account: None,
            windows: vec![UsageWindow {
                id: "five_hour".into(),
                label: "5h".into(),
                scope: None,
                used_percent: 23.5,
                resets_at: Some(10),
            }],
            extras: vec![],
        },
    }
}

fn engine_with(adapters: Vec<MockAdapter>) -> (tempfile::TempDir, Engine, Vec<Arc<MockAdapter>>) {
    let dir = tempfile::tempdir().unwrap();
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    let mut catalog = HarnessCatalog::new();
    let handles: Vec<Arc<MockAdapter>> = adapters.into_iter().map(Arc::new).collect();
    for a in &handles {
        catalog.register(Arc::clone(a) as Arc<_>);
    }
    let engine = Engine::start(
        ws,
        catalog,
        EngineConfig {
            design_enabled: false,
            ..Default::default()
        },
    )
    .unwrap();
    (dir, engine, handles)
}

#[tokio::test]
async fn the_adapters_answer_is_served_cached_and_refreshed() {
    let (_dir, engine, handles) = engine_with(vec![
        MockAdapter {
            id: "reporting".into(),
            usage: Some(report("reporting")),
            ..Default::default()
        },
        MockAdapter {
            id: "silent".into(),
            ..Default::default()
        },
    ]);
    let reporting = &handles[0];
    let asked = || reporting.usage_asked.load(Ordering::SeqCst);

    let first = engine.harness_usage("reporting", false).await;
    assert_eq!(first, report("reporting"), "the adapter's report, as it is");
    assert_eq!(asked(), 1);
    let second = engine.harness_usage("reporting", false).await;
    assert_eq!(second, first);
    assert_eq!(
        asked(),
        1,
        "the second read is served from the cache — the provider is not asked again"
    );

    let refreshed = engine.harness_usage("reporting", true).await;
    assert_eq!(refreshed, first);
    assert_eq!(asked(), 2, "a refresh asks the source again");

    match engine.harness_usage("silent", false).await {
        UsageState::Unsupported { reason } => assert_eq!(
            reason, "Mock Harness reports no usage limits — its provider does.",
            "the trait's default, in the harness's name"
        ),
        other => panic!("{other:?}"),
    }
    match engine.harness_usage("nobody", false).await {
        UsageState::Unsupported { reason } => assert!(
            reason.starts_with("nobody reports no usage limits"),
            "{reason}"
        ),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_failure_is_not_kept_and_reads_off_asks_nothing() {
    let (_dir, engine, handles) = engine_with(vec![MockAdapter {
        id: "flaky".into(),
        usage: Some(UsageState::failed("the endpoint answered 503")),
        ..Default::default()
    }]);
    let flaky = &handles[0];
    assert!(matches!(
        engine.harness_usage("flaky", false).await,
        UsageState::Failed { .. }
    ));
    assert!(matches!(
        engine.harness_usage("flaky", false).await,
        UsageState::Failed { .. }
    ));
    assert_eq!(
        flaky.usage_asked.load(Ordering::SeqCst),
        2,
        "a failure is asked again, never served from the cache"
    );

    engine
        .set_setting(
            SettingScope::Machine,
            None,
            "harness.usage.reads",
            json!(false),
        )
        .expect("a machine setting");
    assert_eq!(engine.harness_usage("flaky", false).await, UsageState::Off);
    assert_eq!(
        engine.harness_usage("flaky", true).await,
        UsageState::Off,
        "off is off, refresh or not"
    );
    assert_eq!(
        flaky.usage_asked.load(Ordering::SeqCst),
        2,
        "reads off: the adapter is never asked"
    );
}
