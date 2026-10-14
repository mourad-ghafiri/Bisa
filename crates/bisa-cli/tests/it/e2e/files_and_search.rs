//! A project's files through the binary **while a node runs**: listed and
//! read, made, saved with the hash of what was read, moved, copied, searched
//! and replaced — a save that lost the race refused with what is there now,
//! an edit made between a replacement's preview and its apply left alone and
//! said, and everything that would leave the root refused unread.
//!
//! The root is a project the journey made; nothing is removed from it — a
//! delete is asked only where it is refused — until the node is stopped,
//! when the same verbs run on an engine of this process's own: made,
//! moved, copied, removed, listed, shown, searched and replaced without a
//! node, the answer the same.

use super::sealed::Sealed;
use serde_json::{json, Value};
use std::path::PathBuf;

/// The texts the journey saves, each with the SHA-256 of its bytes — written
/// down, so what the node answers is held to a number it did not compute.
const NOTHING: (&str, &str) = (
    "",
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
);
const THREE_BOARDS: (&str, &str) = (
    "three boards\n",
    "c62275748bb03765e0348ef359a900f5dfa44904a9329781ca10bb204b958e22",
);
const AS_GIVEN: (&str, &str) = (
    "a\r\nb with no end",
    "826c3df71d99dbb94ff353ffcf5c73ec31dab16ee4d23f604ddc08df0836d110",
);

/// A project of the journey's own, its id — which is its primary
/// workstream's — and its tree.
fn a_project(ws: &Sealed) -> (String, PathBuf) {
    let made = ws.json(&[
        "project",
        "new",
        "shelf",
        "--publish",
        "manual",
        "--committer",
        "A Journey <journey@example.test>",
    ]);
    (
        made["project"]["id"]
            .as_str()
            .expect("the project")
            .to_string(),
        PathBuf::from(made["path"].as_str().expect("its tree")),
    )
}

/// The file route of a root, for one path.
fn file_at(root: &str, path: &str) -> String {
    format!(
        "/ide/file/workstream/{root}?path={}",
        path.replace('/', "%2F").replace(' ', "%20")
    )
}

/// Read a file through the node: its text and its hash.
fn read(ws: &Sealed, root: &str, path: &str) -> (String, String) {
    let (status, file) = ws.call("GET", &file_at(root, path), &[], None);
    assert_eq!(status, 200, "{path}: {file}");
    (
        file["text"].as_str().unwrap_or_default().to_string(),
        file["hash"].as_str().expect("its hash").to_string(),
    )
}

/// Save `text` over what was read as `base`, or make the file when there
/// is no base.
fn save(ws: &Sealed, root: &str, path: &str, text: &str, base: Option<&str>) -> (u16, Value) {
    let body = match base {
        Some(base) => json!({ "text": text, "base_hash": base }),
        None => json!({ "text": text }),
    };
    ws.call("PUT", &file_at(root, path), &[], Some(&body))
}

/// What a refused verb said.
fn refusal(ws: &Sealed, args: &[&str]) -> String {
    let refused = ws.bisa(args);
    assert!(!refused.status.success(), "{args:?} is refused");
    let said = String::from_utf8_lossy(&refused.stderr).to_string();
    assert!(
        !said.contains("another engine holds this workspace"),
        "the node was asked: {said}"
    );
    said
}

#[test]
fn a_file_is_made_read_saved_moved_and_copied_and_never_written_over_unseen() {
    let mut ws = Sealed::bare();
    ws.start();
    let (root, tree) = a_project(&ws);
    // A verb is given whole — `["files", "new"]` — so every one this journey
    // runs is named where it is run.
    let files = |verb: [&str; 2]| {
        vec![
            verb[0].to_string(),
            verb[1].to_string(),
            "workstream".to_string(),
            root.clone(),
        ]
    };
    let run = |ws: &Sealed, verb: [&str; 2], more: &[&str]| -> Value {
        let mut args = files(verb);
        args.extend(more.iter().map(|word| word.to_string()));
        ws.json(&args.iter().map(String::as_str).collect::<Vec<_>>())
    };
    let refused = |ws: &Sealed, verb: [&str; 2], more: &[&str]| -> String {
        let mut args = files(verb);
        args.extend(more.iter().map(|word| word.to_string()));
        refusal(ws, &args.iter().map(String::as_str).collect::<Vec<_>>())
    };

    // Made: a file, with the folder above it; a name that is taken is refused.
    let made = run(&ws, ["files", "new"], &["--path", "docs/shelf.md"]);
    assert_eq!(made, json!({ "path": "docs/shelf.md", "kind": "file" }));
    assert!(tree.join("docs/shelf.md").is_file());
    refused(&ws, ["files", "new"], &["--path", "docs/shelf.md"]);
    refused(&ws, ["files", "new"], &["--path", "docs", "--dir"]);
    // Listed: the folder it made and the file in it, from the root down.
    let listed = run(&ws, ["files", "tree"], &["--depth", "2"]).to_string();
    assert!(
        listed.contains("docs") && listed.contains("shelf.md"),
        "{listed}"
    );
    let under = run(&ws, ["files", "tree"], &["--path", "docs"]).to_string();
    assert!(under.contains("shelf.md"), "{under}");
    refused(&ws, ["files", "tree"], &["--path", "../elsewhere"]);

    // Saved with the hash of what was read; without one it is a creation,
    // and a creation over what exists is refused.
    let (text, empty) = read(&ws, &root, "docs/shelf.md");
    assert_eq!((text.as_str(), empty.as_str()), NOTHING);
    let (status, over) = save(&ws, &root, "docs/shelf.md", THREE_BOARDS.0, None);
    assert_eq!(
        status, 400,
        "there is no writing over what was not read: {over}"
    );
    let (status, saved) = save(&ws, &root, "docs/shelf.md", THREE_BOARDS.0, Some(&empty));
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["hash"], THREE_BOARDS.1);
    assert_eq!(
        run(&ws, ["files", "show"], &["--path", "docs/shelf.md"])["file"]["text"],
        THREE_BOARDS.0
    );

    // A save that lost the race is refused with what is there now, so the
    // loser can merge without asking again.
    let (status, lost) = save(&ws, &root, "docs/shelf.md", "two boards\n", Some(&empty));
    assert_eq!(status, 409, "{lost}");
    assert_eq!(lost["current_text"], THREE_BOARDS.0, "{lost}");
    assert_eq!(lost["current_hash"], THREE_BOARDS.1);
    assert_eq!(
        std::fs::read_to_string(tree.join("docs/shelf.md")).expect("the file"),
        "three boards\n",
        "a refused save wrote nothing"
    );

    // What is saved is what was given, byte for byte.
    let (status, saved) = save(&ws, &root, "docs/as-given.txt", AS_GIVEN.0, None);
    assert_eq!(status, 200, "{saved}");
    assert_eq!(
        std::fs::read(tree.join("docs/as-given.txt")).expect("the file"),
        AS_GIVEN.0.as_bytes()
    );
    assert_eq!(
        read(&ws, &root, "docs/as-given.txt"),
        (AS_GIVEN.0.to_string(), AS_GIVEN.1.to_string())
    );

    // A text over the editor's bound is refused by its size, and nothing is made.
    let (status, huge) = save(
        &ws,
        &root,
        "docs/huge.txt",
        &"x".repeat(3 * 1024 * 1024),
        None,
    );
    assert_eq!(status, 413, "{}", huge["error"]);
    assert!(!tree.join("docs/huge.txt").exists());

    // Moved and copied; onto what exists, and into itself, refused — and a
    // refused move makes no folder on its way.
    assert_eq!(
        run(&ws, ["files", "mv"], &["docs/shelf.md", "docs/boards.md"]),
        json!({ "from": "docs/shelf.md", "path": "docs/boards.md" })
    );
    assert!(!tree.join("docs/shelf.md").exists() && tree.join("docs/boards.md").is_file());
    assert_eq!(
        run(&ws, ["files", "cp"], &["docs/boards.md", "notes/copy.md"]),
        json!({ "from": "docs/boards.md", "path": "notes/copy.md" })
    );
    assert_eq!(
        std::fs::read_to_string(tree.join("notes/copy.md")).expect("the copy"),
        "three boards\n"
    );
    refused(&ws, ["files", "mv"], &["docs/boards.md", "notes/copy.md"]);
    refused(&ws, ["files", "cp"], &["docs/boards.md", "notes/copy.md"]);
    refused(&ws, ["files", "mv"], &["docs", "docs/inner/docs"]);
    assert!(
        !tree.join("docs/inner").exists(),
        "a move that is refused leaves no folder behind it"
    );
    refused(&ws, ["files", "mv"], &["nothing-here.md", "somewhere.md"]);

    // Nothing leaves the root: a way up, an absolute path, the root itself.
    let outside = ws.file("outside.md", "not the project's\n");
    for path in [
        "../outside.md",
        "docs/../../outside.md",
        &outside.to_string_lossy(),
    ] {
        refused(&ws, ["files", "show"], &["--path", path]);
        refused(&ws, ["files", "new"], &["--path", path]);
        refused(&ws, ["files", "mv"], &["docs/boards.md", path]);
        refused(&ws, ["files", "cp"], &["docs/boards.md", path]);
        refused(&ws, ["files", "rm"], &["--path", path]);
        let (status, said) = save(&ws, &root, path, "taken\n", None);
        assert!(status == 400 || status == 403, "{path}: {status} {said}");
    }
    assert_eq!(
        std::fs::read_to_string(&outside).expect("the file outside"),
        "not the project's\n"
    );

    // A folder that holds something goes only when asked whole: the refusal
    // counts what it holds, and nothing went.
    let said = refused(&ws, ["files", "rm"], &["--path", "docs"]);
    assert!(said.contains('2'), "it holds two files: {said}");
    assert!(tree.join("docs/boards.md").is_file() && tree.join("docs/as-given.txt").is_file());

    // The tree, as it is listed — and the same when the node is back.
    let paths = |ws: &Sealed| -> Vec<String> {
        let mut args = files(["files", "tree"]);
        args.extend(["--depth".to_string(), "3".to_string()]);
        let listed = ws.json(&args.iter().map(String::as_str).collect::<Vec<_>>());
        let mut found = Vec::new();
        let mut walk = vec![listed["tree"]["entries"].clone()];
        while let Some(level) = walk.pop() {
            for entry in level.as_array().into_iter().flatten() {
                if let Some(path) = entry["path"].as_str() {
                    if !path.starts_with(".git") {
                        found.push(path.to_string());
                    }
                }
                walk.push(entry["children"].clone());
            }
        }
        found.sort();
        found
    };
    let before = paths(&ws);
    assert_eq!(
        before,
        vec![
            "docs",
            "docs/as-given.txt",
            "docs/boards.md",
            "notes",
            "notes/copy.md"
        ]
    );
    // Unlinked, not the OS Trash, when the journey removes something below:
    // a fixture never leaves its folder.
    ws.ok(&["settings", "set", "machine", "editor.delete.trash", "false"]);
    ws.stop();
    ws.start();
    assert_eq!(paths(&ws), before);
    ws.stop();

    // Without a node: the same verbs, on an engine of this process's own.
    assert_eq!(
        run(&ws, ["files", "new"], &["--path", "offline/note.md"]),
        json!({ "path": "offline/note.md", "kind": "file" })
    );
    assert_eq!(
        run(&ws, ["files", "new"], &["--path", "offline/empty", "--dir"]),
        json!({ "path": "offline/empty", "kind": "dir" })
    );
    assert_eq!(
        run(
            &ws,
            ["files", "mv"],
            &["offline/note.md", "offline/moved.md"]
        ),
        json!({ "from": "offline/note.md", "path": "offline/moved.md" })
    );
    assert_eq!(
        run(
            &ws,
            ["files", "cp"],
            &["offline/moved.md", "offline/copy.md"]
        ),
        json!({ "from": "offline/moved.md", "path": "offline/copy.md" })
    );
    assert_eq!(
        run(&ws, ["files", "rm"], &["--path", "offline/copy.md"]),
        json!({ "path": "offline/copy.md", "disposal": "unlink" })
    );
    assert!(!tree.join("offline/copy.md").exists() && tree.join("offline/moved.md").is_file());
    refused(&ws, ["files", "rm"], &["--path", "offline"]);
    refused(&ws, ["files", "mv"], &["offline/moved.md", "../away.md"]);
    // A goal's folder is written under its own scratch, and a removal there
    // reads the workspace's disposal — a goal belongs to no project.
    let captured = ws.json(&["new", "a goal with a folder", "--mode", "manual"]);
    let goal = captured["goal"]
        .as_str()
        .unwrap_or_else(|| panic!("the goal: {captured}"))
        .to_string();
    assert_eq!(
        ws.json(&["files", "new", "goal", &goal, "--path", "draft.md"]),
        json!({ "path": "draft.md", "kind": "file" })
    );
    assert_eq!(
        ws.json(&["files", "rm", "goal", &goal, "--path", "draft.md"]),
        json!({ "path": "draft.md", "disposal": "unlink" })
    );

    // Listed without a node: a link is marked, an empty folder said empty,
    // depth said when more lies below it, and the entry budget when it cuts.
    #[cfg(unix)]
    std::os::unix::fs::symlink("moved.md", tree.join("offline/alias.md")).unwrap();
    let offline = run(&ws, ["files", "tree"], &["--path", "offline"]);
    let entries = offline["tree"]["entries"].as_array().expect("the entries");
    assert!(
        entries.iter().any(|e| e["path"] == "offline/moved.md"),
        "{offline}"
    );
    #[cfg(unix)]
    assert!(
        entries
            .iter()
            .any(|e| e["path"] == "offline/alias.md" && e["symlink"] == true),
        "{offline}"
    );
    let empty = run(&ws, ["files", "tree"], &["--path", "offline/empty"]);
    assert_eq!(empty["tree"]["entries"], json!([]), "{empty}");
    let shallow = run(&ws, ["files", "tree"], &["--depth", "1"]);
    assert_eq!(shallow["tree"]["deeper"], true, "{shallow}");
    std::fs::create_dir_all(tree.join("many")).unwrap();
    for n in 0..2_050 {
        std::fs::write(tree.join(format!("many/f{n:04}.txt")), "x").unwrap();
    }
    let cut = run(&ws, ["files", "tree"], &["--path", "many"]);
    assert_eq!(cut["tree"]["truncated"], true, "{cut}");

    // Shown without a node: text; a picture is named and sized, never
    // sprayed; a file past the read's bound is shown as far as it goes.
    assert_eq!(
        run(&ws, ["files", "show"], &["--path", "offline/moved.md"])["file"]["text"],
        ""
    );
    std::fs::write(
        tree.join("offline/pic.png"),
        b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR",
    )
    .unwrap();
    let pic = run(&ws, ["files", "show"], &["--path", "offline/pic.png"]);
    assert_eq!(pic["file"]["binary"], true, "{pic}");
    assert_eq!(pic["file"]["text"], Value::Null, "{pic}");
    std::fs::write(tree.join("offline/long.txt"), "y".repeat(300 * 1024)).unwrap();
    let long = run(&ws, ["files", "show"], &["--path", "offline/long.txt"]);
    assert_eq!(long["file"]["truncated"], true, "{long}");
    assert_eq!(long["file"]["size"], 300 * 1024, "{long}");
}

#[test]
fn a_search_finds_what_is_there_and_a_replacement_leaves_alone_what_changed_since_its_preview() {
    let mut ws = Sealed::bare();
    ws.start();
    let (root, tree) = a_project(&ws);
    for (path, text) in [
        ("plan/shelf.md", "three boards\nsix brackets\n"),
        ("plan/paint.md", "two coats on the boards\n"),
        ("plan/tools.md", "a saw, a level\n"),
    ] {
        let (status, saved) = save(&ws, &root, path, text, None);
        assert_eq!(status, 200, "{saved}");
    }

    // Found, with where: the path, the line, the column, the line's text.
    let found = ws.json(&["files", "search", "workstream", &root, "boards"]);
    let mut hits: Vec<(String, u64, String)> = found["hits"]
        .as_array()
        .expect("the hits")
        .iter()
        .map(|hit| {
            (
                hit["path"].as_str().unwrap_or_default().to_string(),
                hit["line"].as_u64().unwrap_or(0),
                hit["text"]
                    .as_str()
                    .unwrap_or_default()
                    .trim_end()
                    .to_string(),
            )
        })
        .collect();
    hits.sort();
    assert_eq!(
        hits,
        vec![
            (
                "plan/paint.md".to_string(),
                1,
                "two coats on the boards".to_string()
            ),
            ("plan/shelf.md".to_string(), 1, "three boards".to_string()),
        ]
    );
    assert_eq!(found["summary"]["matches"], 2, "{found}");
    assert_eq!(found["summary"]["files_with_matches"], 2);
    assert_eq!(found["summary"]["truncated"], false);
    let none = ws.json(&["files", "search", "workstream", &root, "a plane"]);
    assert_eq!(none["hits"], json!([]));

    // A pattern that is no pattern is refused before anything is walked.
    let said = refusal(
        &ws,
        &["files", "search", "workstream", &root, "board(", "--regex"],
    );
    assert!(!said.trim().is_empty());

    // A replacement is a preview first: what would change, and nothing written.
    let preview = ws.json(&["files", "replace", "workstream", &root, "boards", "planks"]);
    assert_eq!(preview["applied"], false, "{preview}");
    let previewed: Vec<Value> = preview["files"].as_array().expect("the files").clone();
    assert_eq!(previewed.len(), 2, "{preview}");
    assert_eq!(
        std::fs::read_to_string(tree.join("plan/shelf.md")).expect("the file"),
        "three boards\nsix brackets\n",
        "a preview writes nothing"
    );

    // Somebody edits one of them between the preview and the apply: it is
    // left alone and said, and the other is written.
    let (_, before) = read(&ws, &root, "plan/paint.md");
    let (status, saved) = save(
        &ws,
        &root,
        "plan/paint.md",
        "three coats on the boards\n",
        Some(&before),
    );
    assert_eq!(status, 200, "{saved}");
    let as_previewed: Vec<Value> = previewed
        .iter()
        .map(|file| json!({ "path": file["path"], "base_hash": file["base_hash"] }))
        .collect();
    let (status, applied) = ws.call(
        "POST",
        &format!("/ide/replace/workstream/{root}"),
        &[],
        Some(&json!({
            "q": "boards",
            "replacement": "planks",
            "apply": true,
            "files": as_previewed,
        })),
    );
    assert_eq!(status, 200, "{applied}");
    assert_eq!(
        std::fs::read_to_string(tree.join("plan/shelf.md")).expect("the file"),
        "three planks\nsix brackets\n"
    );
    assert_eq!(
        std::fs::read_to_string(tree.join("plan/paint.md")).expect("the file"),
        "three coats on the boards\n",
        "what changed since the preview is never written over"
    );
    let said = applied.to_string();
    assert!(
        said.contains("plan/paint.md") && said.to_lowercase().contains("changed"),
        "the one left alone is named, and why: {applied}"
    );

    // The verb with `--apply` previews and writes in one go.
    let applied = ws.json(&[
        "files",
        "replace",
        "workstream",
        &root,
        "boards",
        "planks",
        "--apply",
    ]);
    assert_eq!(applied["applied"], true, "{applied}");
    assert_eq!(
        std::fs::read_to_string(tree.join("plan/paint.md")).expect("the file"),
        "three coats on the planks\n"
    );
    assert_eq!(
        ws.json(&["files", "search", "workstream", &root, "boards"])["hits"],
        json!([])
    );
    // Nothing left to change: applied, and no file written.
    let nothing = ws.json(&[
        "files",
        "replace",
        "workstream",
        &root,
        "boards",
        "planks",
        "--apply",
    ]);
    assert_eq!(nothing, json!({ "applied": true, "files": [] }));

    // Narrowed through the node: the globs, the limit, and the case words.
    let narrowed = ws.json(&[
        "files",
        "search",
        "workstream",
        &root,
        "a",
        "--include",
        "plan/*.md",
        "--exclude",
        "plan/tools.md",
        "--limit",
        "1",
        "--case",
        "insensitive",
    ]);
    assert_eq!(narrowed["summary"]["matches"], 1, "{narrowed}");
    assert_eq!(narrowed["summary"]["truncated"], true, "{narrowed}");
    let sensitive = ws.json(&[
        "files",
        "search",
        "workstream",
        &root,
        "PLANKS",
        "--case",
        "sensitive",
    ]);
    assert_eq!(sensitive["hits"], json!([]), "{sensitive}");
    refusal(
        &ws,
        &[
            "files",
            "search",
            "workstream",
            &root,
            "a",
            "--case",
            "loud",
        ],
    );

    // A root whose folder is gone: the node's stream says so, and the verb
    // repeats it.
    let made = ws.json(&["project", "new", "attic", "--publish", "manual"]);
    let other = made["project"]["id"]
        .as_str()
        .expect("the project")
        .to_string();
    let other_tree = PathBuf::from(made["path"].as_str().expect("its tree"));
    std::fs::rename(&other_tree, other_tree.with_extension("aside"))
        .expect("the other tree, aside");
    let said = refusal(&ws, &["files", "search", "workstream", &other, "x"]);
    assert!(said.contains(&other), "the root is named: {said}");
    ws.stop();

    // Without a node: the same search and replace, and the same refusals.
    let offline = ws.json(&[
        "files",
        "search",
        "workstream",
        &root,
        "planks",
        "--case",
        "sensitive",
    ]);
    assert_eq!(offline["summary"]["matches"], 2, "{offline}");
    let offline = ws.json(&[
        "files",
        "search",
        "workstream",
        &root,
        "PLANKS",
        "--case",
        "insensitive",
    ]);
    assert_eq!(offline["summary"]["matches"], 2, "{offline}");
    refusal(
        &ws,
        &[
            "files",
            "search",
            "workstream",
            &root,
            "a",
            "--case",
            "loud",
        ],
    );
    refusal(
        &ws,
        &[
            "files",
            "replace",
            "workstream",
            &root,
            "plank(",
            "x",
            "--regex",
        ],
    );
    let preview = ws.json(&["files", "replace", "workstream", &root, "planks", "boards"]);
    assert_eq!(preview["applied"], false, "{preview}");
    assert_eq!(
        preview["files"].as_array().map(|f| f.len()),
        Some(2),
        "{preview}"
    );
    let applied = ws.json(&[
        "files",
        "replace",
        "workstream",
        &root,
        "planks",
        "boards",
        "--apply",
    ]);
    assert_eq!(applied["applied"], true, "{applied}");
    assert_eq!(
        std::fs::read_to_string(tree.join("plan/paint.md")).expect("the file"),
        "three coats on the boards\n"
    );
}
