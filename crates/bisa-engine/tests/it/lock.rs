//! One engine per workspace.

use bisa_engine::{Engine, EngineConfig, EngineError};
use bisa_harness::HarnessCatalog;
use bisa_store::{MemoryKeyStore, Paths, Workspace};

fn workspace(dir: &tempfile::TempDir) -> Workspace {
    Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap()
}

fn config() -> EngineConfig {
    EngineConfig {
        design_enabled: false,
        events_enabled: false,
        ..Default::default()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_engine_on_one_workspace_is_refused_with_the_holder() {
    let dir = tempfile::tempdir().unwrap();
    let first = Engine::start(workspace(&dir), HarnessCatalog::new(), config()).expect("first");

    let err = match Engine::start(workspace(&dir), HarnessCatalog::new(), config()) {
        Ok(_) => panic!("a second engine started on a held workspace"),
        Err(e) => e,
    };
    match &err {
        EngineError::Locked { path, holder } => {
            assert!(path.ends_with("engine.lock"), "{path}");
            let holder = holder
                .as_ref()
                .expect("the holder is recorded in the lock file");
            assert_eq!(holder.pid, std::process::id());
            assert!(holder.started_at > 0);
        }
        other => panic!("expected Locked, got {other:?}"),
    }
    assert!(
        !err.is_refusal(),
        "a held lock is a fault of the environment, not a refusal"
    );
    // The refused start bound nothing: only the first engine's socket exists.
    let sockets = std::fs::read_dir(Paths::new(dir.path()).run_dir())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().ends_with(".sock"))
        .count();
    assert_eq!(sockets, 1);

    first.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn shutting_down_releases_the_workspace_to_the_next_engine() {
    let dir = tempfile::tempdir().unwrap();
    let first = Engine::start(workspace(&dir), HarnessCatalog::new(), config()).expect("first");
    first.shutdown().await;
    let second = Engine::start(workspace(&dir), HarnessCatalog::new(), config())
        .expect("the lock is released with the engine that held it");
    second.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_holder_can_be_read_without_taking_the_lock() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path());
    assert_eq!(
        Engine::lock_holder(&paths).unwrap(),
        None,
        "a fresh workspace is nobody's"
    );

    let engine = Engine::start(workspace(&dir), HarnessCatalog::new(), config()).unwrap();
    let holder = Engine::lock_holder(&paths)
        .unwrap()
        .expect("held while the engine runs");
    assert_eq!(holder.pid, std::process::id());
    // Reading did not steal it: a second engine is still refused.
    assert!(matches!(
        Engine::start(workspace(&dir), HarnessCatalog::new(), config()),
        Err(EngineError::Locked { .. })
    ));
    engine.shutdown().await;
    assert_eq!(Engine::lock_holder(&paths).unwrap(), None);
}

#[test]
fn the_lock_file_is_private_to_the_owner() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path());
    let _lock = bisa_engine::EngineLock::acquire(&paths).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(paths.engine_lock())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "run/engine.lock is 0600");
    }
}
