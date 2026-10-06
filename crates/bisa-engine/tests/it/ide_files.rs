//! The IDE's write path (ide/03): compare-and-swap saves, explorer mutations,
//! the writable-root boundary, and the watcher.

use bisa_engine::events::FileChangeKind;
use bisa_engine::ide::files::{self, Disposal, EntryKind};
use bisa_engine::ide::watch;
use bisa_engine::{Engine, EngineConfig, EngineError, EnginePayload};
use bisa_harness::HarnessCatalog;
use bisa_store::{content_hash, FileScope, MemoryKeyStore, NewProject, Workspace};
use std::time::Duration;

fn engine(dir: &tempfile::TempDir) -> (Engine, String, std::path::PathBuf) {
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    let project = ws
        .create_project(NewProject::managed("web").unwrap())
        .unwrap();
    let root = ws.project_root_path(&project);
    std::fs::create_dir_all(&root).unwrap();
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
    (engine, project.id.to_string(), root)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_save_is_compare_and_swap_and_a_conflict_carries_the_current_text() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, pid, root) = engine(&dir);
    let inner = engine.inner();

    // Create: no base hash, path absent.
    let w = files::write_file(
        inner,
        FileScope::Workstream,
        &pid,
        "src/main.rs",
        "fn main() {}\n",
        None,
    )
    .unwrap();
    assert!(w.created);
    assert_eq!(w.path, "src/main.rs");
    assert_eq!(w.hash, content_hash(b"fn main() {}\n"));
    assert_eq!(
        std::fs::read_to_string(root.join("src/main.rs")).unwrap(),
        "fn main() {}\n"
    );

    // Create again: refused, nothing overwritten.
    let err = files::write_file(
        inner,
        FileScope::Workstream,
        &pid,
        "src/main.rs",
        "other",
        None,
    )
    .unwrap_err();
    assert!(matches!(err, EngineError::Invalid(_)), "{err}");
    assert_eq!(
        std::fs::read_to_string(root.join("src/main.rs")).unwrap(),
        "fn main() {}\n"
    );

    // Save with the right hash.
    let w2 = files::write_file(
        inner,
        FileScope::Workstream,
        &pid,
        "src/main.rs",
        "fn main() { run() }\n",
        Some(&w.hash),
    )
    .unwrap();
    assert!(!w2.created);

    // An agent wrote in between: the stale hash is a conflict with the current text attached.
    std::fs::write(root.join("src/main.rs"), "// the agent was here\n").unwrap();
    let err = files::write_file(
        inner,
        FileScope::Workstream,
        &pid,
        "src/main.rs",
        "mine",
        Some(&w2.hash),
    )
    .unwrap_err();
    match err {
        EngineError::FileConflict {
            path,
            current_hash,
            current_text,
        } => {
            assert_eq!(path, "src/main.rs");
            assert_eq!(current_text, "// the agent was here\n");
            assert_eq!(current_hash, content_hash(current_text.as_bytes()));
        }
        other => panic!("expected a conflict, got {other:?}"),
    }
    assert!(err_is_refusal());
    assert_eq!(
        std::fs::read_to_string(root.join("src/main.rs")).unwrap(),
        "// the agent was here\n",
        "a refused save wrote nothing"
    );
    engine.shutdown().await;
}

fn err_is_refusal() -> bool {
    EngineError::FileConflict {
        path: String::new(),
        current_hash: String::new(),
        current_text: String::new(),
    }
    .is_refusal()
}

#[tokio::test(flavor = "multi_thread")]
async fn writes_stay_inside_the_writable_root() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, pid, root) = engine(&dir);
    let inner = engine.inner();
    for bad in ["../escape.txt", "/etc/hosts", "src/../../x"] {
        let err =
            files::write_file(inner, FileScope::Workstream, &pid, bad, "x", None).unwrap_err();
        assert!(err.is_refusal(), "{bad}: {err}");
    }
    // A symlink pointing out of the tree is one refusal too.
    std::os::unix::fs::symlink(dir.path(), root.join("out")).unwrap();
    let err = files::write_file(
        inner,
        FileScope::Workstream,
        &pid,
        "out/journal.jsonl",
        "x",
        None,
    )
    .unwrap_err();
    assert!(err.is_refusal(), "{err}");
    assert!(!dir.path().join("journal.jsonl").exists());

    // A goal's writable root is its scratch/ folder, never its journal.
    let goal = inner
        .ws
        .create_goal(bisa_store::NewGoal::captured("edit me"))
        .unwrap();
    let gid = goal.id.to_string();
    files::write_file(inner, FileScope::Goal, &gid, "notes.md", "# notes\n", None).unwrap();
    assert!(inner
        .ws
        .paths()
        .goal(goal.id)
        .scratch()
        .join("notes.md")
        .exists());
    let err =
        files::write_file(inner, FileScope::Goal, &gid, "../journal.jsonl", "x", None).unwrap_err();
    assert!(err.is_refusal(), "{err}");
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn create_move_and_delete_are_confined_and_announced() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, pid, root) = engine(&dir);
    let inner = engine.inner();
    let mut bus = engine.events();

    files::create_entry(inner, FileScope::Workstream, &pid, "docs", EntryKind::Dir).unwrap();
    files::create_entry(
        inner,
        FileScope::Workstream,
        &pid,
        "docs/a.md",
        EntryKind::File,
    )
    .unwrap();
    assert!(root.join("docs/a.md").is_file());
    assert!(files::create_entry(
        inner,
        FileScope::Workstream,
        &pid,
        "docs/a.md",
        EntryKind::File
    )
    .is_err());

    let moved =
        files::move_entry(inner, FileScope::Workstream, &pid, "docs/a.md", "docs/b.md").unwrap();
    assert_eq!(moved, "docs/b.md");
    assert!(!root.join("docs/a.md").exists() && root.join("docs/b.md").exists());
    assert!(files::move_entry(inner, FileScope::Workstream, &pid, "docs/b.md", "../b.md").is_err());
    assert!(files::move_entry(inner, FileScope::Workstream, &pid, "nope", "x").is_err());

    // A directory needs the confirmation; the root itself is never removed.
    let err = files::delete_entry(
        inner,
        FileScope::Workstream,
        &pid,
        "docs",
        false,
        Disposal::Unlink,
    )
    .unwrap_err();
    assert!(err.to_string().contains("recursive=true"), "{err}");
    assert!(files::delete_entry(
        inner,
        FileScope::Workstream,
        &pid,
        "",
        true,
        Disposal::Unlink
    )
    .is_err());
    assert!(files::delete_entry(
        inner,
        FileScope::Workstream,
        &pid,
        ".",
        true,
        Disposal::Unlink
    )
    .is_err());
    assert!(root.is_dir());
    let gone = files::delete_entry(
        inner,
        FileScope::Workstream,
        &pid,
        "docs/b.md",
        false,
        Disposal::Unlink,
    )
    .unwrap();
    assert_eq!(gone.disposal, Disposal::Unlink);
    assert!(!root.join("docs/b.md").exists());
    files::delete_entry(
        inner,
        FileScope::Workstream,
        &pid,
        "docs",
        true,
        Disposal::Unlink,
    )
    .unwrap();
    assert!(!root.join("docs").exists());

    // Every mutation announced itself, in order.
    let mut kinds = Vec::new();
    while let Ok(Ok(e)) = tokio::time::timeout(Duration::from_millis(200), bus.recv()).await {
        if let EnginePayload::FileChanged {
            kind, path, from, ..
        } = e.payload
        {
            kinds.push((kind, path, from));
        }
    }
    assert_eq!(
        kinds,
        vec![
            (FileChangeKind::Created, "docs".to_string(), None),
            (FileChangeKind::Created, "docs/a.md".to_string(), None),
            (
                FileChangeKind::Renamed,
                "docs/b.md".to_string(),
                Some("docs/a.md".to_string())
            ),
            (FileChangeKind::Removed, "docs/b.md".to_string(), None),
            (FileChangeKind::Removed, "docs".to_string(), None),
        ]
    );
    engine.shutdown().await;
}

/// A move that is refused leaves nothing behind it: into its own subtree,
/// onto something that exists, of the root itself, of nothing. A rename that
/// only changes a name's case is a rename, on a disk that folds case too.
#[tokio::test(flavor = "multi_thread")]
async fn a_refused_move_writes_nothing_and_a_case_only_rename_is_a_rename() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, pid, root) = engine(&dir);
    let inner = engine.inner();
    let ws = FileScope::Workstream;
    files::write_file(inner, ws, &pid, "src/lib/a.rs", "a\n", None).unwrap();
    files::write_file(inner, ws, &pid, "src/b.rs", "b\n", None).unwrap();
    let tree = |root: &std::path::Path| -> Vec<String> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                out.push(path.strip_prefix(root).unwrap().display().to_string());
                if path.is_dir() {
                    stack.push(path);
                }
            }
        }
        out.sort();
        out
    };
    let before = tree(&root);

    for (from, to, why) in [
        ("src", "src/lib/moved/src", "into its own subtree"),
        ("src", "src/inner", "into itself"),
        ("src/b.rs", "src/lib/a.rs", "onto a file that exists"),
        ("src/b.rs", "src/lib", "onto a folder that exists"),
        ("src/ghost.rs", "src/new/ghost.rs", "of nothing"),
        ("", "elsewhere", "of the root"),
        (".", "elsewhere", "of the root, spelt with a dot"),
        ("src/b.rs", "../outside.rs", "out of the root"),
    ] {
        let refused = files::move_entry(inner, ws, &pid, from, to);
        assert!(refused.is_err(), "{why}: {from:?} → {to:?}");
        assert_eq!(
            tree(&root),
            before,
            "{why}: a refused move made or moved nothing"
        );
    }

    // A rename that only changes the case. Whether the disk folds case is
    // the machine's; the rename holds either way.
    let folds = root.join("SRC").exists();
    let to = files::move_entry(inner, ws, &pid, "src/b.rs", "src/B.rs").unwrap();
    assert_eq!(to, "src/B.rs");
    let names: Vec<String> = std::fs::read_dir(root.join("src"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        names.contains(&"B.rs".to_string()),
        "{names:?} (folds case: {folds})"
    );
    assert!(!names.contains(&"b.rs".to_string()), "{names:?}");
    assert_eq!(
        std::fs::read_to_string(root.join("src/B.rs")).unwrap(),
        "b\n"
    );
    // The same name again is nothing to do, and is said as what exists.
    assert!(files::move_entry(inner, ws, &pid, "src/B.rs", "src/B.rs").is_err());
    engine.shutdown().await;
}

/// What is saved is what was given, byte for byte: a BOM, CRLF line ends, no
/// final newline, nothing at all, and bytes that are no text. The hash is of
/// those bytes, so the next save's compare-and-swap is against the truth.
#[tokio::test(flavor = "multi_thread")]
async fn a_save_keeps_every_byte_it_was_given() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, pid, root) = engine(&dir);
    let inner = engine.inner();
    let ws = FileScope::Workstream;
    let cases: [(&str, &[u8]); 6] = [
        ("bom.txt", b"\xEF\xBB\xBFwith a mark\n"),
        ("crlf.txt", b"one\r\ntwo\r\n"),
        ("mixed.txt", b"one\r\ntwo\nthree\r"),
        ("no-newline.txt", b"ends here"),
        ("empty.txt", b""),
        ("not-text.bin", &[0x00, 0xFF, 0xFE, 0x80, 0x0A, 0xC3, 0x28]),
    ];
    for (name, bytes) in cases {
        let w = files::write_bytes(inner, ws, &pid, name, bytes, None).unwrap();
        assert!(w.created, "{name}");
        assert_eq!(
            std::fs::read(root.join(name)).unwrap(),
            bytes,
            "{name} on disk"
        );
        assert_eq!(
            w.hash,
            content_hash(bytes),
            "{name}: the hash is of the bytes"
        );
        // Saved again over itself with the hash it was given: no conflict, no change.
        let again = files::write_bytes(inner, ws, &pid, name, bytes, Some(&w.hash)).unwrap();
        assert!(!again.created);
        assert_eq!(again.hash, w.hash);
        assert_eq!(std::fs::read(root.join(name)).unwrap(), bytes);
        let raw = files::raw_file(inner, ws, &pid, name).unwrap();
        assert_eq!(raw.size, bytes.len() as u64, "{name}");
    }
    // A stale hash on a file that moved under the editor is the conflict, and nothing is written.
    std::fs::write(root.join("crlf.txt"), b"changed elsewhere\r\n").unwrap();
    let stale = content_hash(b"one\r\ntwo\r\n");
    let refused = files::write_bytes(inner, ws, &pid, "crlf.txt", b"mine\r\n", Some(&stale));
    assert!(refused.is_err());
    assert_eq!(
        std::fs::read(root.join("crlf.txt")).unwrap(),
        b"changed elsewhere\r\n"
    );
    engine.shutdown().await;
}

/// Names that are not what they seem resolve inside the root or are refused —
/// and a refusal writes nothing, anywhere.
#[tokio::test(flavor = "multi_thread")]
async fn odd_paths_stay_inside_the_root_or_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, pid, root) = engine(&dir);
    let inner = engine.inner();
    let ws = FileScope::Workstream;
    files::create_entry(inner, ws, &pid, "docs", EntryKind::Dir).unwrap();
    #[cfg(unix)]
    {
        // A link to itself, and two that point at each other: never followed round.
        std::os::unix::fs::symlink("self", root.join("self")).unwrap();
        std::os::unix::fs::symlink("pong", root.join("ping")).unwrap();
        std::os::unix::fs::symlink("ping", root.join("pong")).unwrap();
    }
    let outside_before: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();

    for bad in [
        "/etc/hosts-copy",
        "docs/../../escape.txt",
        "..",
        "docs/nul\0byte.txt",
        "self/inside.txt",
        "ping/inside.txt",
    ] {
        let refused = files::write_file(inner, ws, &pid, bad, "x", None);
        assert!(refused.is_err(), "{bad:?} must be refused");
    }
    // A directory is not a file, however it is spelt.
    for dirish in ["docs", "docs/", "./docs/."] {
        assert!(
            files::write_file(inner, ws, &pid, dirish, "x", None).is_err(),
            "{dirish:?}"
        );
    }
    // A backslash is a character of a name here, never a separator: the file
    // lands in the root under that one name, and nowhere else.
    #[cfg(unix)]
    {
        let w = files::write_file(inner, ws, &pid, "docs\\win.txt", "x", None).unwrap();
        assert_eq!(w.path, "docs\\win.txt");
        assert!(root.join("docs\\win.txt").is_file());
        assert!(!root.join("docs").join("win.txt").exists());
    }
    // A dot segment and a doubled slash are the same place.
    let w = files::write_file(inner, ws, &pid, "./docs//a.md", "a", None).unwrap();
    assert_eq!(w.path, "docs/a.md");
    let outside_after: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(
        outside_after, outside_before,
        "nothing was written beside the workspace's own"
    );
    engine.shutdown().await;
}

/// A folder that holds something goes only when the caller says so, and the
/// refusal counts what it holds; an empty one, a file and a link go as asked.
#[tokio::test(flavor = "multi_thread")]
async fn a_folder_that_holds_something_is_deleted_only_when_asked_to() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, pid, root) = engine(&dir);
    let inner = engine.inner();
    let ws = FileScope::Workstream;
    files::write_file(inner, ws, &pid, "pkg/a.txt", "a", None).unwrap();
    files::write_file(inner, ws, &pid, "pkg/b.txt", "b", None).unwrap();
    files::create_entry(inner, ws, &pid, "hollow", EntryKind::Dir).unwrap();

    let refused = files::delete_entry(inner, ws, &pid, "pkg", false, Disposal::Unlink)
        .unwrap_err()
        .to_string();
    assert!(refused.contains("holding 2 entries"), "{refused}");
    assert!(root.join("pkg/a.txt").is_file(), "nothing went");
    // An empty folder asks the same question: a folder is never deleted by a slip.
    assert!(files::delete_entry(inner, ws, &pid, "hollow", false, Disposal::Unlink).is_err());
    files::delete_entry(inner, ws, &pid, "hollow", true, Disposal::Unlink).unwrap();
    files::delete_entry(inner, ws, &pid, "pkg", true, Disposal::Unlink).unwrap();
    assert!(!root.join("pkg").exists() && !root.join("hollow").exists());
    // Gone is gone: a second delete is refused in words, and the root never goes.
    assert!(files::delete_entry(inner, ws, &pid, "pkg", true, Disposal::Unlink).is_err());
    for the_root in ["", ".", "./", "  "] {
        assert!(files::delete_entry(inner, ws, &pid, the_root, true, Disposal::Unlink).is_err());
    }
    assert!(root.is_dir());
    engine.shutdown().await;
}

/// Several entries go as one act (ide/03): every one is checked before
/// anything goes, an entry named twice goes once, one under a folder of the
/// batch goes with the folder — each answered as asked — and a failure
/// midway answers what went and names the first entry that did not.
#[tokio::test(flavor = "multi_thread")]
async fn several_entries_go_as_one_act_and_a_refusal_before_them_touches_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, pid, root) = engine(&dir);
    let inner = engine.inner();
    let ws = FileScope::Workstream;
    let mut bus = engine.events();
    let ask = |path: &str, recursive: bool| files::ToDelete {
        path: path.to_string(),
        recursive,
    };
    for path in ["a.txt", "b.txt", "c.txt", "pkg/inner.txt", "pkg/deep/x.txt"] {
        files::write_file(inner, ws, &pid, path, "x", None).unwrap();
    }
    files::create_entry(inner, ws, &pid, "hollow", EntryKind::Dir).unwrap();
    while bus.try_recv().is_ok() {}

    // A refusal anywhere in the list refuses the whole list: nothing goes,
    // nothing is announced.
    for bad in [
        ask("../x", false),
        ask("", true),
        ask(".", true),
        ask("nope", false),
        ask("hollow", false),
        ask("pkg", false),
    ] {
        let err = files::delete_entries(
            inner,
            ws,
            &pid,
            &[ask("a.txt", false), bad.clone()],
            Disposal::Unlink,
        )
        .unwrap_err();
        assert!(
            root.join("a.txt").is_file(),
            "nothing went for {bad:?}: {err}"
        );
    }
    assert!(
        files::delete_entries(inner, ws, &pid, &[], Disposal::Unlink).is_err(),
        "a batch naming nothing is a refusal"
    );
    assert!(bus.try_recv().is_err(), "a refused batch announces nothing");

    // A file named twice, a folder with a file under it named apart, another file.
    let gone = files::delete_entries(
        inner,
        ws,
        &pid,
        &[
            ask("a.txt", false),
            ask("pkg/inner.txt", false),
            ask("pkg", true),
            ask("a.txt", false),
            ask("b.txt", false),
        ],
        Disposal::Unlink,
    )
    .unwrap();
    assert!(gone.halted.is_none(), "{:?}", gone.halted);
    let answered: Vec<&str> = gone.deleted.iter().map(|d| d.path.as_str()).collect();
    assert_eq!(
        answered,
        vec!["a.txt", "pkg/inner.txt", "pkg", "a.txt", "b.txt"],
        "every asked path, as asked"
    );
    assert!(gone.deleted.iter().all(|d| d.disposal == Disposal::Unlink));
    assert!(!root.join("a.txt").exists() && !root.join("pkg").exists());
    assert!(!root.join("b.txt").exists());
    assert!(
        root.join("c.txt").is_file() && root.join("hollow").is_dir(),
        "what was not asked stays"
    );
    // One announcement per move: the folder's, never its child's apart; a
    // path named twice once.
    let mut removed = Vec::new();
    while let Ok(Ok(e)) = tokio::time::timeout(Duration::from_millis(200), bus.recv()).await {
        if let EnginePayload::FileChanged {
            kind: FileChangeKind::Removed,
            path,
            ..
        } = e.payload
        {
            removed.push(path);
        }
    }
    assert_eq!(removed, vec!["a.txt", "pkg", "b.txt"]);

    // A failure midway: what went before it is answered and announced, the
    // entry it halted on is named, the one after it is left alone.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        files::write_file(inner, ws, &pid, "locked/held.txt", "x", None).unwrap();
        files::write_file(inner, ws, &pid, "d.txt", "x", None).unwrap();
        while bus.try_recv().is_ok() {}
        let locked = root.join("locked");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
        let outcome = files::delete_entries(
            inner,
            ws,
            &pid,
            &[
                ask("c.txt", false),
                ask("locked/held.txt", false),
                ask("d.txt", false),
            ],
            Disposal::Unlink,
        );
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        let outcome = outcome.unwrap();
        if root.join("locked/held.txt").is_file() {
            let halted = outcome.halted.expect("the locked file halted the batch");
            assert_eq!(halted.path, "locked/held.txt");
            let answered: Vec<&str> = outcome.deleted.iter().map(|d| d.path.as_str()).collect();
            assert_eq!(answered, vec!["c.txt"], "what went before the halt");
            assert!(
                root.join("d.txt").is_file(),
                "the entry after the halt was left alone"
            );
            let mut removed = Vec::new();
            while let Ok(Ok(e)) = tokio::time::timeout(Duration::from_millis(200), bus.recv()).await
            {
                if let EnginePayload::FileChanged {
                    kind: FileChangeKind::Removed,
                    path,
                    ..
                } = e.payload
                {
                    removed.push(path);
                }
            }
            assert_eq!(removed, vec!["c.txt"], "only what went is announced");
        } else {
            // A read-only folder holds nothing back from root: the batch
            // simply went, and the halt is not exercised here.
            eprintln!("the read-only folder held nothing back (running as root?)");
            assert!(outcome.halted.is_none());
        }
    }
    engine.shutdown().await;
}

/// The path index is a cached walk, and every write the engine makes under a
/// root is what forgets it: a file written, made, moved, copied and deleted is
/// in the very next index — never the one from before. A project that is
/// deleted takes its cached walk with it.
#[tokio::test(flavor = "multi_thread")]
async fn the_path_index_is_never_stale_after_a_write_and_goes_with_its_project() {
    use bisa_engine::ide::index;

    let dir = tempfile::tempdir().unwrap();
    let (engine, pid, _root) = engine(&dir);
    let inner = engine.inner();
    let ws = FileScope::Workstream;
    let listed = || index::paths(inner, ws, &pid, None).unwrap().paths;

    assert!(listed().is_empty());
    files::write_file(inner, ws, &pid, "a.txt", "a", None).unwrap();
    assert_eq!(
        listed(),
        vec!["a.txt"],
        "a write is seen at once, inside the cache's time"
    );
    files::create_entry(inner, ws, &pid, "src/new.rs", EntryKind::File).unwrap();
    assert_eq!(listed(), vec!["a.txt", "src/new.rs"]);
    files::move_entry(inner, ws, &pid, "a.txt", "src/b.txt").unwrap();
    assert_eq!(listed(), vec!["src/b.txt", "src/new.rs"]);
    files::copy_entry(inner, ws, &pid, "src", "lib").unwrap();
    assert_eq!(
        listed(),
        vec!["lib/b.txt", "lib/new.rs", "src/b.txt", "src/new.rs"]
    );
    files::delete_entry(inner, ws, &pid, "lib", true, Disposal::Unlink).unwrap();
    assert_eq!(listed(), vec!["src/b.txt", "src/new.rs"]);
    // A refused write forgets nothing it should not and invents nothing.
    assert!(files::move_entry(inner, ws, &pid, "src", "src/inside").is_err());
    assert_eq!(listed(), vec!["src/b.txt", "src/new.rs"]);
    // A custom limit is a fresh walk of its own, and says when it stopped.
    let capped = index::paths(inner, ws, &pid, Some(1)).unwrap();
    assert_eq!((capped.paths.len(), capped.truncated), (1, true));
    assert_eq!(
        index::paths(inner, ws, &pid, Some(0)).unwrap().paths.len(),
        1,
        "a limit of nothing is one"
    );
    assert_eq!(listed().len(), 2, "and the shared walk is whole still");

    // Deleted: nothing is served from memory for a project that is no more.
    let project: bisa_core::ProjectId = pid.parse().unwrap();
    engine.delete_project(project, false).await.unwrap();
    assert!(
        index::paths(inner, ws, &pid, None).is_err(),
        "a root that is gone has no index, cached or otherwise"
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_watcher_reports_a_change_made_outside_the_platform() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, pid, root) = engine(&dir);
    let inner = engine.inner();
    let mut bus = engine.events();

    let watched = watch::watch(inner, FileScope::Workstream, &pid).unwrap();
    assert_eq!(
        watched.canonicalize().unwrap(),
        root.canonicalize().unwrap()
    );
    assert_eq!(inner.ide_watch.watched().len(), 1);
    // Watching twice refreshes the lease rather than adding a second watcher.
    watch::watch(inner, FileScope::Workstream, &pid).unwrap();
    assert_eq!(inner.ide_watch.watched().len(), 1);

    // Something outside the platform — an agent, an editor — writes a file.
    tokio::time::sleep(Duration::from_millis(200)).await;
    std::fs::write(root.join("agent.txt"), "hello").unwrap();

    let seen = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let e = bus.recv().await.unwrap();
            if let EnginePayload::FileChanged {
                path, scope, id, ..
            } = e.payload
            {
                if path == "agent.txt" {
                    return (scope, id);
                }
            }
        }
    })
    .await
    .expect("the watcher reported agent.txt");
    assert_eq!(seen, ("workstream".to_string(), pid.clone()));

    assert!(watch::unwatch(inner, FileScope::Workstream, &pid));
    assert!(!watch::unwatch(inner, FileScope::Workstream, &pid));
    assert!(inner.ide_watch.watched().is_empty());
    engine.shutdown().await;
}

// `TrashRemover` is deliberately untested: a test that moved a fixture into
// the developer's Trash would leave something behind on a real machine.
// `Disposal::Unlink` on a tempdir is the one disposal a test may exercise.

#[tokio::test(flavor = "multi_thread")]
async fn a_copy_duplicates_a_file_or_a_whole_folder_and_never_lands_inside_itself() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, pid, root) = engine(&dir);
    let inner = engine.inner();
    files::create_entry(inner, FileScope::Workstream, &pid, "src", EntryKind::Dir).unwrap();
    files::write_file(
        inner,
        FileScope::Workstream,
        &pid,
        "src/a.rs",
        "fn a() {}\n",
        None,
    )
    .unwrap();
    files::write_file(
        inner,
        FileScope::Workstream,
        &pid,
        "src/b.rs",
        "fn b() {}\n",
        None,
    )
    .unwrap();

    let copied = files::copy_entry(
        inner,
        FileScope::Workstream,
        &pid,
        "src/a.rs",
        "src/a copy.rs",
    )
    .unwrap();
    assert_eq!(copied, "src/a copy.rs");
    assert_eq!(
        std::fs::read_to_string(root.join("src/a copy.rs")).unwrap(),
        "fn a() {}\n"
    );
    assert!(
        files::copy_entry(
            inner,
            FileScope::Workstream,
            &pid,
            "src/a.rs",
            "src/a copy.rs"
        )
        .is_err(),
        "an existing target is refused"
    );

    let folder = files::copy_entry(inner, FileScope::Workstream, &pid, "src", "src copy").unwrap();
    assert_eq!(folder, "src copy");
    assert!(
        root.join("src copy/a.rs").is_file()
            && root.join("src copy/b.rs").is_file()
            && root.join("src copy/a copy.rs").is_file()
    );
    assert!(
        files::copy_entry(inner, FileScope::Workstream, &pid, "src", "src/inner").is_err(),
        "a folder cannot be copied into itself"
    );
    assert!(
        files::copy_entry(inner, FileScope::Workstream, &pid, "src", "../out").is_err(),
        "the root is the bound"
    );
    assert!(files::copy_entry(inner, FileScope::Workstream, &pid, "nope", "x").is_err());
    engine.shutdown().await;
}

/// A symlink inside the checkout that points outside it is one refusal for
/// every verb that writes — a save, a new entry, a move's and a copy's
/// destination — and nothing lands where the link points.
#[tokio::test(flavor = "multi_thread")]
async fn every_write_verb_refuses_a_symlink_out_of_the_root() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let (engine, pid, root) = engine(&dir);
    let inner = engine.inner();
    std::os::unix::fs::symlink(outside.path(), root.join("out")).unwrap();
    files::create_entry(inner, FileScope::Workstream, &pid, "src", EntryKind::Dir).unwrap();
    files::write_file(
        inner,
        FileScope::Workstream,
        &pid,
        "src/a.rs",
        "fn a() {}\n",
        None,
    )
    .unwrap();

    let created = files::create_entry(
        inner,
        FileScope::Workstream,
        &pid,
        "out/new.txt",
        EntryKind::File,
    );
    assert!(created.unwrap_err().is_refusal());
    let moved = files::move_entry(inner, FileScope::Workstream, &pid, "src/a.rs", "out/a.rs");
    assert!(moved.unwrap_err().is_refusal());
    let copied = files::copy_entry(inner, FileScope::Workstream, &pid, "src/a.rs", "out/b.rs");
    assert!(copied.unwrap_err().is_refusal());
    let saved = files::write_file(inner, FileScope::Workstream, &pid, "out/c.rs", "x", None);
    assert!(saved.unwrap_err().is_refusal());

    assert!(
        root.join("src/a.rs").is_file(),
        "the source stayed where it was"
    );
    assert!(
        std::fs::read_dir(outside.path()).unwrap().next().is_none(),
        "nothing landed where the link points"
    );
    engine.shutdown().await;
}
