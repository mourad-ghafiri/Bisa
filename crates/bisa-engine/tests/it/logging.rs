//! The engine's side of the diagnostic log: the registry's words are the
//! crate's, the defaults resolve to errors only, a write of a `logging.*`
//! key reaches the handle, and a fresh workspace has no file yet.
//!
//! The subscriber is thread-local (`set_default`) so no test sets the
//! process's, and the folder is the fixture's tempdir, never a real one.

use bisa_core::settings::Kind;
use bisa_core::{SettingDef, SettingScope};
use bisa_engine::logging::{self, KEYS};
use bisa_engine::{Engine, EngineConfig};
use bisa_harness::HarnessCatalog;
use bisa_log::{build, list, LogConfig, LogLevel, LogRotation, Process};
use bisa_store::{MemoryKeyStore, Workspace};
use serde_json::json;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

fn choices(key: &str) -> &'static [&'static str] {
    match SettingDef::lookup(key).expect("a registered key").kind {
        Kind::Choice(words) => words,
        ref other => panic!("{key} is not a choice: {other:?}"),
    }
}

#[test]
fn the_registrys_words_are_the_log_crates_and_every_key_is_machine_scope() {
    assert_eq!(choices("logging.level"), &LogLevel::WORDS[..]);
    assert_eq!(choices("logging.rotation"), &LogRotation::WORDS[..]);
    for key in KEYS {
        let def = SettingDef::lookup(key).expect(key);
        assert!(
            def.scopes.allows(SettingScope::Machine),
            "{key} is held on this machine"
        );
        assert!(
            !def.scopes.allows(SettingScope::Workspace),
            "{key} never syncs"
        );
        assert!(
            !def.scopes.allows(SettingScope::Project),
            "{key} is no project's"
        );
    }
    let defaults: Vec<bisa_core::ResolvedSetting> = bisa_core::resolve_settings(&[]);
    assert_eq!(logging::config_from(&defaults), LogConfig::default());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_write_of_a_logging_key_reaches_the_handle_and_a_fresh_workspace_has_no_file() {
    let dir = tempfile::tempdir().unwrap();
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    let (handle, slot) = build(Process::Node, "0.0.0-test");
    let _guard = tracing_subscriber::registry().with(slot).set_default();
    handle
        .attach(ws.paths().logs_dir(), LogConfig::default())
        .unwrap();
    let engine = Engine::start(
        ws,
        HarnessCatalog::new(),
        EngineConfig {
            design_enabled: false,
            events_enabled: false,
            log: Some(handle.clone()),
            ..Default::default()
        },
    )
    .unwrap();

    // The start applied the machine's settings: the defaults.
    assert_eq!(handle.config(), LogConfig::default());
    let listing = logging::listing(engine.inner());
    assert_eq!(listing.files().len(), 1, "the *log started* file");
    assert_eq!(listing.families[0].process, Process::Node);
    assert!(listing.crashes.is_empty());
    assert!(logging::latest_crash(engine.inner()).is_none());
    assert!(logging::crash(engine.inner(), "notes.json").is_none());

    engine
        .set_setting(SettingScope::Machine, None, "logging.level", json!("debug"))
        .unwrap();
    assert_eq!(handle.config().level, LogLevel::Debug);
    engine
        .set_setting(
            SettingScope::Machine,
            None,
            "logging.rotation",
            json!("hourly"),
        )
        .unwrap();
    assert_eq!(handle.config().rotation, LogRotation::Hourly);
    engine
        .set_setting(SettingScope::Machine, None, "logging.enabled", json!(false))
        .unwrap();
    assert!(!handle.config().enabled);
    // Switched off, a child is told no folder; on, the store's.
    assert!(logging::child_env(engine.inner()).is_empty());
    engine
        .unset_setting(SettingScope::Machine, None, "logging.enabled")
        .unwrap();
    assert!(handle.config().enabled);
    let env = logging::child_env(engine.inner());
    assert_eq!(
        env.get(bisa_log::LOG_DIR_ENV).map(String::as_str),
        Some(logging::dir(engine.inner()).display().to_string().as_str())
    );
    assert!(logging::dir(engine.inner()).starts_with(dir.path()));

    // A workspace-scope write of a machine key is refused by the registry
    // and moves nothing.
    assert!(engine
        .set_setting(
            SettingScope::Workspace,
            None,
            "logging.level",
            json!("trace")
        )
        .is_err());
    assert_eq!(handle.config().level, LogLevel::Debug);

    // Every file the listing names is one the crate wrote under the
    // fixture, in the node's own folder, and never anywhere else.
    let listing = list(&logging::dir(engine.inner())).unwrap();
    for family in &listing.families {
        assert!(family.dir.starts_with(dir.path()));
        for file in &family.files {
            assert_eq!(family.process, Process::Node);
            assert!(file.name.starts_with("node."));
        }
    }
}

#[test]
fn an_engine_without_a_log_reads_the_settings_for_nothing_and_still_names_the_folder() {
    let dir = tempfile::tempdir().unwrap();
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _enter = rt.enter();
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
    assert!(engine.inner().log.is_none());
    assert!(
        logging::listing(engine.inner()).files().is_empty(),
        "nothing wrote a file"
    );
    let env = logging::child_env(engine.inner());
    assert!(env[bisa_log::LOG_DIR_ENV].starts_with(&dir.path().display().to_string()));
}
