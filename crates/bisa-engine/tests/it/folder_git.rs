//! The folder repositories — the notes' and the drawings': the first record
//! makes one, a commit records every change, nobody set to commit is refused
//! with the folder's own door, a push goes to a **local bare origin** — a
//! temp directory, no remote in sight — and the General Agent (a mock here)
//! drafts a message. The git handle is isolated from the developer's global
//! config, so "nobody is set to commit" can be produced on purpose. Every
//! case runs once per folder; the drawings' repository also ignores its
//! `state/` and offers no pull.

use bisa_core::{AgentId, OwnerScope};
use bisa_engine::folder_git::{Folder, FolderGit};
use bisa_engine::{drawings, notes, Engine, EngineConfig, EngineError, Inner};
use bisa_harness::mock::MockAdapter;
use bisa_harness::HarnessCatalog;
use bisa_store::{MemoryKeyStore, NewDrawing, NewNote, Workspace};
use bisa_vcs::{Git, VcsError};
use std::sync::Arc;

fn engine(dir: &tempfile::TempDir) -> Engine {
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    // The General Agent runs on the mock, which answers a prompt with `echo: <prompt>`.
    let adapter = MockAdapter {
        id: "notes-mock".into(),
        ..Default::default()
    };
    let mut def = ws.get_agent(&AgentId::general()).unwrap();
    def.harness = adapter.id.clone();
    ws.update_agent(def).unwrap();
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::new(adapter) as Arc<_>);
    let git = Git::new()
        .with_env("GIT_CONFIG_GLOBAL", dir.path().join("no-global.gitconfig"))
        .with_env("GIT_CONFIG_NOSYSTEM", "1");
    Engine::start(
        ws,
        catalog,
        EngineConfig {
            design_enabled: false,
            git: Some(git),
            ..Default::default()
        },
    )
    .unwrap()
}

/// A bare repository the tests push to — `origin` by path, no network.
fn bare_origin(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let origin = dir.path().join("origin.git");
    let out = std::process::Command::new("git")
        .args(["init", "--bare", "--quiet", origin.to_str().unwrap()])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git on PATH");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    origin
}

/// One record in the folder under test: a note, or a drawing.
fn a_record(engine: &Engine, folder: Folder, title: &str) {
    match folder {
        Folder::Notes => {
            notes::create(
                engine.inner(),
                NewNote {
                    scope: OwnerScope::Workspace,
                    title: title.into(),
                    body: "words\n".into(),
                },
            )
            .unwrap();
        }
        Folder::Drawings => {
            drawings::create(
                engine.inner(),
                NewDrawing {
                    scope: OwnerScope::Workspace,
                    title: title.into(),
                    scene: None,
                },
            )
            .unwrap();
        }
    }
}

/// The repository under test.
fn repo(inner: &Inner, folder: Folder) -> &FolderGit {
    match folder {
        Folder::Notes => &inner.notes_git,
        Folder::Drawings => &inner.drawings_git,
    }
}

const BOTH: [Folder; 2] = [Folder::Notes, Folder::Drawings];

#[tokio::test]
async fn the_first_record_makes_the_repository_and_a_commit_records_every_change() {
    for folder in BOTH {
        let dir = tempfile::tempdir().unwrap();
        let engine = engine(&dir);
        let inner = engine.inner();
        let git = repo(inner, folder);
        let root = folder.root(engine.workspace().paths());
        assert!(
            !root.join(".git").exists(),
            "no repository before any record"
        );

        a_record(&engine, folder, "First");
        assert!(
            root.join(".git").is_dir(),
            "the first record made the repository"
        );
        let s = git.status(inner).await.unwrap();
        assert_eq!(
            s.changed, 1,
            "{folder:?}: the new file counts, nothing else"
        );
        assert_eq!(s.last_commit, None);
        assert_eq!(s.remote, None);
        if folder == Folder::Drawings {
            let exclude = std::fs::read_to_string(FolderGit::exclude_file(&root)).unwrap();
            assert!(
                exclude.lines().any(|l| l == "state/"),
                "the snapshots are never the export: {exclude}"
            );
            assert!(
                root.join("state").is_dir(),
                "the snapshot is there, ignored"
            );
        }

        // Nobody is set to commit: the isolated handle sees no global pair.
        match git.commit(inner, "Records").await {
            Err(EngineError::RepoIdentityUnset { what }) => assert_eq!(what, folder.word()),
            other => panic!("{other:?}"),
        }
        git.set_identity(inner, "Ada", "ada@example.com")
            .await
            .unwrap();
        let c = git.commit(inner, "Add the first record").await.unwrap();
        assert_eq!(c.subject, "Add the first record");
        let s = git.status(inner).await.unwrap();
        assert_eq!(s.changed, 0, "everything was taken");
        assert_eq!(
            s.last_commit.as_ref().map(|c| c.subject.as_str()),
            Some("Add the first record")
        );
        assert_eq!(s.identity.name.as_deref(), Some("Ada"));

        // Nothing changed: no empty commit.
        assert!(matches!(
            git.commit(inner, "again").await,
            Err(EngineError::NothingToCommit(_))
        ));
        assert!(
            matches!(
                git.commit(inner, "   ").await,
                Err(EngineError::NothingToCommit(_))
            ),
            "a commit needs a message"
        );

        // A second record is a change.
        a_record(&engine, folder, "Second");
        let s = git.status(inner).await.unwrap();
        assert_eq!(s.changed, 1);
    }
}

#[tokio::test]
async fn a_push_goes_to_origin_by_path_and_the_status_reads_the_upstream() {
    for folder in BOTH {
        let dir = tempfile::tempdir().unwrap();
        let engine = engine(&dir);
        let inner = engine.inner();
        let git = repo(inner, folder);
        a_record(&engine, folder, "First");
        git.set_identity(inner, "Ada", "ada@example.com")
            .await
            .unwrap();

        // The destination is asked for first (ide/04: no origin → `NoRemote`,
        // nothing committed → `NothingToCommit`): with neither, the remote is
        // the refusal; with one, the missing commit is.
        assert!(
            matches!(
                git.push(inner).await,
                Err(EngineError::Vcs(VcsError::NoRemote(_)))
            ),
            "no origin yet"
        );
        let origin = bare_origin(&dir);
        let s = git
            .set_remote(inner, origin.to_str().unwrap())
            .await
            .unwrap();
        assert_eq!(s.remote.as_deref(), Some(origin.to_str().unwrap()));
        assert!(
            matches!(git.push(inner).await, Err(EngineError::NothingToCommit(_))),
            "nothing committed yet"
        );
        git.commit(inner, "Add the first record").await.unwrap();
        let s = git.push(inner).await.unwrap();
        assert!(
            s.upstream
                .as_deref()
                .is_some_and(|u| u.starts_with("origin/")),
            "{:?}",
            s.upstream
        );
        assert_eq!((s.ahead, s.behind), (0, 0));
        let s = git.fetch(inner).await.unwrap();
        assert_eq!((s.ahead, s.behind), (0, 0));

        a_record(&engine, folder, "Second");
        git.commit(inner, "Add a second").await.unwrap();
        let s = git.status(inner).await.unwrap();
        assert_eq!(s.ahead, 1, "one commit origin lacks");
        assert!(matches!(
            git.set_remote(inner, "  ").await,
            Err(EngineError::Invalid(_))
        ));
    }
}

#[tokio::test]
async fn the_general_agent_drafts_a_message_from_what_changed() {
    for folder in BOTH {
        let dir = tempfile::tempdir().unwrap();
        let engine = engine(&dir);
        let inner = engine.inner();
        let git = repo(inner, folder);
        a_record(&engine, folder, "Why the cache is a cache");
        let message = git.suggest(inner).await.unwrap();
        assert!(!message.trim().is_empty(), "the mock echoes the prompt");
        assert!(
            message.contains("Why the cache is a cache") || message.contains("Staged files"),
            "drafted from the change: {message}"
        );
        let s = git.status(inner).await.unwrap();
        assert_eq!(s.changed, 1, "suggesting stages, never commits");
        assert_eq!(s.last_commit, None);
    }
}

#[test]
fn only_the_notes_repository_offers_a_pull() {
    // The refusal itself is the node's to show (405 on the missing route and
    // on the engine's `PullNotOffered`); the folder's word is what it reads.
    assert!(Folder::Notes.pulls());
    assert!(!Folder::Drawings.pulls());
    assert_eq!(Folder::Drawings.word(), "drawings");
}
