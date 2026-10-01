//! What floats over the app and what stands beside it, through the binary
//! while a node runs: an addon imported from a folder of the person's own —
//! refused whole when it is not one — granted by the words its manifest
//! declares, switched on and off, its files served to its window with no
//! token and to nobody once it is off, removed; a built-in installed from the
//! catalog. A drawing an agent reads, files a sibling of and erases from with
//! no window open, and is told at once that nobody is at the canvas when it
//! asks for what only the canvas can do. A note an agent adds to, never over;
//! the notes' repository committed by somebody set to commit; the pets the
//! platform ships, which are put away and never removed.
//!
//! Nothing here opens a window, and nothing is taken off disk but what the
//! verbs themselves remove from the journey's own workspace.

use super::sealed::Sealed;
use serde_json::{json, Value};
use std::path::PathBuf;

/// A folder of the person's own holding an addon: a manifest, a page, a
/// script. `manifest` is written as it is given.
fn a_folder_with(ws: &Sealed, name: &str, manifest: &str) -> PathBuf {
    let beside = ws.file(&format!("{name}.beside"), "");
    let folder = beside.parent().expect("the journey's files").join(name);
    std::fs::create_dir_all(&folder).expect("a folder of the journey's own");
    std::fs::write(folder.join("addon.json"), manifest).expect("the manifest");
    std::fs::write(
        folder.join("index.html"),
        "<!doctype html><script src=\"bisa-addon.js\"></script><script src=\"main.js\"></script>",
    )
    .expect("the page");
    std::fs::write(folder.join("main.js"), "// nothing yet").expect("the script");
    folder
}

fn the_manifest(id: &str) -> String {
    json!({
        "id": id, "name": "Byte", "description": "a widget", "version": "1.0.0", "license": "MIT",
        "permissions": ["storage", { "network": { "hosts": ["api.example.com"] } }],
    })
    .to_string()
}

/// The ids of the addons installed, as the verb lists them.
fn installed(ws: &Sealed) -> Vec<String> {
    ws.json(&["addon", "list"])["addons"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| a["id"].as_str().map(str::to_string))
        .collect()
}

/// What a caller from outside — no token — is answered for one of an
/// addon's files.
fn served(ws: &Sealed, id: &str, file: &str) -> u16 {
    ws.call_with("GET", &format!("/addons/{id}/files/{file}"), &[], None)
        .0
}

#[test]
fn an_addon_is_imported_granted_switched_served_and_removed_and_a_built_in_comes_from_the_catalog()
{
    let mut ws = Sealed::bare();
    ws.start();
    let listening = ws.listen();
    assert_eq!(
        installed(&ws),
        Vec::<String>::new(),
        "none until one is asked for"
    );

    // --- refused whole, before a byte is copied -----------------------------------
    let no_manifest = a_folder_with(&ws, "not-an-addon", "this is no manifest");
    let refused = ws.bisa(&["addon", "import", &no_manifest.to_string_lossy()]);
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("addon.json"),
        "the file is named: {}",
        String::from_utf8_lossy(&refused.stderr)
    );
    let no_id = a_folder_with(&ws, "no-id", &the_manifest("Not An Id!"));
    assert!(!ws
        .bisa(&["addon", "import", &no_id.to_string_lossy()])
        .status
        .success());
    let folder = a_folder_with(&ws, "byte", &the_manifest("acme.byte"));
    let never_declared = ws.bisa(&[
        "addon",
        "import",
        &folder.to_string_lossy(),
        "--grant",
        "clipboard",
    ]);
    assert!(
        !never_declared.status.success(),
        "a permission the manifest never declared is no grant"
    );
    assert_eq!(
        installed(&ws),
        Vec::<String>::new(),
        "nothing was installed"
    );

    // --- imported: off, with nothing granted ------------------------------------------
    let imported = ws.json(&["addon", "import", &folder.to_string_lossy()]);
    let addon = &imported["addon"];
    assert_eq!(addon["id"], "acme.byte", "{imported}");
    assert_eq!(addon["enabled"], false, "{imported}");
    assert_eq!(addon["active"], false, "{imported}");
    assert_eq!(addon["granted"], json!([]), "{imported}");
    assert_eq!(addon["files_present"], true, "{imported}");
    assert_eq!(installed(&ws), vec!["acme.byte"]);
    let shown = ws.json(&["addon", "show", "acme.byte"]);
    assert_eq!(shown["addon"]["manifest"]["name"], "Byte", "{shown}");
    assert_eq!(
        served(&ws, "acme.byte", "index.html"),
        404,
        "an addon that is off serves nothing"
    );
    // The folder it came from is as it was.
    assert_eq!(
        std::fs::read_dir(&folder).expect("the source").count(),
        3,
        "an import reads its source and writes nothing into it"
    );

    // --- granted by the words its manifest declares, and taken back --------------------
    let granted = ws.json(&["addon", "grant", "acme.byte", "storage", "network"]);
    assert_eq!(
        granted["addon"]["granted"],
        json!(["storage", { "network": { "hosts": ["api.example.com"] } }]),
        "{granted}"
    );
    assert!(
        !ws.bisa(&["addon", "grant", "acme.byte", "clipboard"])
            .status
            .success(),
        "a word it never declared"
    );
    let revoked = ws.json(&["addon", "revoke", "acme.byte", "storage"]);
    assert_eq!(
        revoked["addon"]["granted"],
        json!([{ "network": { "hosts": ["api.example.com"] } }]),
        "{revoked}"
    );

    // --- on: its files are its window's, with no token ---------------------------------
    let on = ws.json(&["addon", "enable", "acme.byte"]);
    assert_eq!(on["addon"]["active"], true, "{on}");
    assert_eq!(served(&ws, "acme.byte", "index.html"), 200);
    assert_eq!(served(&ws, "acme.byte", "main.js"), 200);
    assert_eq!(
        served(&ws, "acme.byte", "bisa-addon.js"),
        200,
        "the bridge every addon loads"
    );
    assert_eq!(served(&ws, "acme.byte", "nothing-here.js"), 404);
    assert_eq!(
        served(&ws, "acme.byte", "..%2Faddon.json"),
        404,
        "nothing outside the bundle"
    );
    assert_eq!(served(&ws, "acme.nobody", "index.html"), 404);
    // Its record and its list are not its window's: they take the token.
    assert_eq!(ws.call_with("GET", "/addons", &[], None).0, 401);

    let off = ws.json(&["addon", "disable", "acme.byte"]);
    assert_eq!(off["addon"]["active"], false, "{off}");
    assert_eq!(served(&ws, "acme.byte", "index.html"), 404);

    // --- a built-in, from the catalog --------------------------------------------------
    let clock = ws.json(&["addon", "install", "clock"]);
    assert_eq!(
        clock["installed"].as_array().map(Vec::len),
        Some(1),
        "{clock}"
    );
    let again = ws.json(&["addon", "install", "clock"]);
    assert_eq!(again["installed"], json!([]), "installed once: {again}");
    assert_eq!(installed(&ws).len(), 2);

    // --- removed: its record, its bundle, its files ------------------------------------
    let removed = ws.json(&["addon", "remove", "acme.byte"]);
    assert_eq!(removed["removed"], "acme.byte", "{removed}");
    assert!(!installed(&ws).contains(&"acme.byte".to_string()));
    assert!(!ws.bisa(&["addon", "show", "acme.byte"]).status.success());
    assert!(
        !ws.bisa(&["addon", "remove", "acme.byte"]).status.success(),
        "twice is not found"
    );

    // Every change was said to whoever listens — what the desktop's layer
    // re-reads on.
    ws.until("the changes to have been said", || {
        (listening
            .engine_events()
            .iter()
            .filter(|fact| *fact == "addons_changed")
            .count()
            >= 5)
            .then_some(())
    });

    // And the built-in is there when the node is back.
    drop(listening);
    ws.stop();
    ws.start();
    assert_eq!(installed(&ws).len(), 1);
    ws.stop();
}

/// An agent in a conversation about a drawing: it reads the drawing, files a
/// new one beside it, erases a box — and asks for a skeleton to be drawn,
/// which only the canvas can lay out.
fn the_drawing_script() -> Value {
    json!({ "turns": [{
        "scope": "conversation", "when": "tidy the sketch", "times": 1,
        "tools": [
            { "name": "drawing_read", "arguments": {} },
            { "name": "drawing_create", "arguments": { "title": "A second sketch" } },
            { "name": "drawing_erase", "arguments": { "ids": ["a"] } },
            { "name": "drawing_draw", "arguments": { "elements": [
                { "type": "rectangle", "x": 0, "y": 0, "width": 80, "height": 40 }
            ] } },
        ],
        "say": ["Tidied what I could; nobody is at the canvas."],
    }]})
}

fn a_box(id: &str, x: i64) -> Value {
    json!({ "id": id, "type": "rectangle", "x": x, "y": 40, "width": 160, "height": 80 })
}

#[test]
fn a_drawing_is_read_filed_and_erased_with_no_window_and_what_needs_the_canvas_says_nobody_is_home()
{
    let mut ws = Sealed::with_script(&the_drawing_script());
    ws.start();
    let (status, made) = ws.call(
        "POST",
        "/drawings",
        &[],
        Some(&json!({
            "scope": "workspace", "title": "The floor plan",
            "scene": { "elements": [a_box("a", 0), a_box("b", 240)] },
        })),
    );
    assert_eq!(status, 200, "{made}");
    let drawing = made["drawing"]["id"].as_str().expect("its id").to_string();
    let first_hash = made["drawing"]["hash"]
        .as_str()
        .expect("its hash")
        .to_string();

    // A conversation about it: the drawing is chosen for a call naming none.
    let (status, started) = ws.call(
        "POST",
        "/conversations",
        &[],
        Some(&json!({ "origin": { "kind": "drawing", "id": drawing } })),
    );
    assert_eq!(status, 201, "{started}");
    let talk = started["conversation"]["id"]
        .as_str()
        .expect("the conversation")
        .to_string();
    let (status, said) = ws.call(
        "POST",
        &format!("/conversations/{talk}/messages"),
        &[],
        Some(&json!({ "content": "tidy the sketch" })),
    );
    assert_eq!(status, 200, "{said}");
    ws.until("the agent's answer", || {
        let (_, room) = ws.call("GET", &format!("/conversations/{talk}/messages"), &[], None);
        room["messages"]
            .as_array()?
            .iter()
            .any(|m| m["content"] == "Tidied what I could; nobody is at the canvas.")
            .then_some(())
    });

    // What each tool was answered, in the order it was called.
    let calls = ws.recorded("tool");
    let of = |name: &str| -> &Value {
        calls
            .iter()
            .find(|call| call["name"] == name)
            .unwrap_or_else(|| panic!("{name} was called: {calls:#?}"))
    };
    let answer = |name: &str| of(name)["result"].to_string();
    assert_eq!(of("drawing_read")["failed"], false, "{calls:#?}");
    assert!(
        answer("drawing_read").contains("The floor plan"),
        "the conversation's drawing, read with no window: {}",
        answer("drawing_read")
    );
    assert_eq!(of("drawing_create")["failed"], false, "{calls:#?}");
    assert_eq!(of("drawing_erase")["failed"], false, "{calls:#?}");
    assert_eq!(
        of("drawing_draw")["failed"],
        true,
        "a skeleton is the canvas's to lay out: {calls:#?}"
    );
    assert!(
        answer("drawing_draw").contains("none is open"),
        "said at once, in words: {}",
        answer("drawing_draw")
    );

    // The erasure is in the record, and the new drawing beside the first.
    let (status, read) = ws.call("GET", &format!("/drawings/{drawing}"), &[], None);
    assert_eq!(status, 200, "{read}");
    let ids: Vec<&str> = read["drawing"]["scene"]["elements"]
        .as_array()
        .expect("the elements")
        .iter()
        .filter_map(|e| e["id"].as_str())
        .collect();
    assert_eq!(ids, ["b"], "{read}");
    let now_hash = read["drawing"]["hash"]
        .as_str()
        .expect("its hash")
        .to_string();
    assert_ne!(now_hash, first_hash, "the scene moved");
    let (_, listed) = ws.call("GET", "/drawings", &[], None);
    let mut titles: Vec<&str> = listed["drawings"]
        .as_array()
        .expect("the drawings")
        .iter()
        .filter_map(|d| d["title"].as_str())
        .collect();
    titles.sort_unstable();
    assert_eq!(titles, ["A second sketch", "The floor plan"], "{listed}");

    // The person's canvas, still holding the scene as it was, is refused —
    // with what is there now — rather than drawing over the agent's change.
    let (status, conflict) = ws.call(
        "PATCH",
        &format!("/drawings/{drawing}"),
        &[],
        Some(&json!({ "scene": { "elements": [a_box("a", 0)] }, "base_hash": first_hash })),
    );
    assert_eq!(status, 409, "{conflict}");
    let (status, saved) = ws.call(
        "PATCH",
        &format!("/drawings/{drawing}"),
        &[],
        Some(&json!({ "scene": { "elements": [a_box("b", 240), a_box("c", 480)] }, "base_hash": now_hash })),
    );
    assert_eq!(status, 200, "{saved}");

    // Deleted, the drawing takes its conversation with it.
    let (status, gone) = ws.call("DELETE", &format!("/drawings/{drawing}"), &[], None);
    assert!(status == 200 || status == 204, "{status} {gone}");
    let (status, _) = ws.call("GET", &format!("/drawings/{drawing}"), &[], None);
    assert_eq!(status, 404);
    let (status, _) = ws.call("GET", &format!("/conversations/{talk}"), &[], None);
    assert_eq!(status, 404, "a thread about a drawing that is gone");
    ws.stop();
}

/// An agent asked about a note: it reads it and adds to it.
fn the_note_script() -> Value {
    json!({ "turns": [{
        "scope": "conversation", "when": "what is missing", "times": 1,
        "tools": [
            { "name": "note_read", "arguments": {} },
            { "name": "note_append", "arguments": { "text": "Order the hinges." } },
        ],
        "say": ["Added what was missing."],
    }]})
}

#[test]
fn a_note_is_added_to_never_over_its_repository_is_committed_and_the_pets_are_the_platforms() {
    let mut ws = Sealed::with_script(&the_note_script());
    ws.start();

    // --- a note, and an agent asked about it --------------------------------------------
    let (status, made) = ws.call(
        "POST",
        "/notes",
        &[],
        Some(&json!({ "scope": "workspace", "title": "The door", "body": "Sand it.\n" })),
    );
    assert_eq!(status, 200, "{made}");
    let note = made["note"]["id"].as_str().expect("its id").to_string();
    let stale = made["note"]["hash"].as_str().expect("its hash").to_string();
    let (status, started) = ws.call(
        "POST",
        "/conversations",
        &[],
        Some(&json!({ "origin": { "kind": "note", "id": note } })),
    );
    assert_eq!(status, 201, "{started}");
    let talk = started["conversation"]["id"]
        .as_str()
        .expect("the conversation")
        .to_string();
    let (status, said) = ws.call(
        "POST",
        &format!("/conversations/{talk}/messages"),
        &[],
        Some(&json!({ "content": "what is missing from this list?" })),
    );
    assert_eq!(status, 200, "{said}");
    let body = ws.until("the agent to have added to the note", || {
        let (_, read) = ws.call("GET", &format!("/notes/{note}"), &[], None);
        let body = read["note"]["body"].as_str()?.to_string();
        body.contains("Order the hinges.").then_some(body)
    });
    assert!(
        body.starts_with("Sand it."),
        "added at the end, nothing before it changed: {body}"
    );
    let read_by_the_agent = ws.recorded("tool");
    assert!(
        read_by_the_agent
            .iter()
            .any(|call| call["name"] == "note_read"
                && call["result"].to_string().contains("Sand it.")),
        "{read_by_the_agent:#?}"
    );
    // The person's editor still holds the note as it was: refused, with
    // what is there now.
    let (status, conflict) = ws.call(
        "PATCH",
        &format!("/notes/{note}"),
        &[],
        Some(&json!({ "body": "mine alone", "base_hash": stale })),
    );
    assert_eq!(status, 409, "{conflict}");
    assert!(
        conflict["current"]
            .as_str()
            .is_some_and(|now| now.contains("Order the hinges.")),
        "{conflict}"
    );

    // --- the notes' repository ------------------------------------------------------------
    let (status, repo) = ws.call("GET", "/notes/git", &[], None);
    assert_eq!(status, 200, "{repo}");
    assert_eq!(repo["changed"], 1, "{repo}");
    assert_eq!(repo["last_commit"], Value::Null, "{repo}");
    let (status, nobody) = ws.call(
        "POST",
        "/notes/git/commit",
        &[],
        Some(&json!({ "message": "Add the door" })),
    );
    assert!(
        (400..500).contains(&status),
        "nobody is set to commit: {status} {nobody}"
    );
    let (status, who) = ws.call(
        "PUT",
        "/notes/git/identity",
        &[],
        Some(&json!({ "name": "A Journey", "email": "journey@example.test" })),
    );
    assert_eq!(status, 200, "{who}");
    let (status, committed) = ws.call(
        "POST",
        "/notes/git/commit",
        &[],
        Some(&json!({ "message": "Add the door" })),
    );
    assert_eq!(status, 200, "{committed}");
    let (_, repo) = ws.call("GET", "/notes/git", &[], None);
    assert_eq!(repo["changed"], 0, "{repo}");
    assert_eq!(repo["last_commit"]["subject"], "Add the door", "{repo}");
    // Nothing is pushed: there is nowhere to push to, and it says so.
    let (status, nowhere) = ws.call("POST", "/notes/git/push", &[], None);
    assert!((400..500).contains(&status), "{status} {nowhere}");

    // Deleted, the note is not found — and its conversation with it.
    let (status, _) = ws.call("DELETE", &format!("/notes/{note}"), &[], None);
    assert_eq!(status, 200);
    let (status, _) = ws.call("GET", &format!("/notes/{note}"), &[], None);
    assert_eq!(status, 404);
    let (status, _) = ws.call("GET", &format!("/conversations/{talk}"), &[], None);
    assert_eq!(status, 404, "a thread about a note that is gone");
    let (_, repo) = ws.call("GET", "/notes/git", &[], None);
    assert_eq!(
        repo["changed"], 1,
        "the deletion is a change to commit: {repo}"
    );

    // --- the pets the platform ships -------------------------------------------------------
    let (status, pets) = ws.call("GET", "/pets", &[], None);
    assert_eq!(status, 200, "{pets}");
    let shipped: Vec<&Value> = pets["pets"].as_array().expect("the pets").iter().collect();
    assert_eq!(shipped.len(), 9, "{pets}");
    assert!(shipped.iter().all(|p| p["origin"] == "catalog"), "{pets}");
    let one = shipped[0]["id"].as_str().expect("its id");
    assert_eq!(
        ws.call("GET", &format!("/pets/{one}/sprite"), &[], None).0,
        200
    );
    let (status, kept) = ws.call("DELETE", &format!("/pets/{one}"), &[], None);
    assert_eq!(status, 409, "a built-in is put away, never removed: {kept}");
    assert_eq!(
        ws.call("DELETE", "/pets/nobody", &[], None).0,
        404,
        "a pet nobody installed is not found"
    );
    assert_eq!(ws.call("GET", "/pets/nobody/sprite", &[], None).0, 404);
    ws.stop();
}
