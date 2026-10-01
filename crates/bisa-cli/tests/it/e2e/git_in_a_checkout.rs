//! Git in a checkout through the binary **while a node runs**: what changed
//! is seen, staged by file and by hunk, committed by somebody who is set to
//! commit; what a person asks that moves the tree is kept first, under a
//! recovery point that puts it back; and a branch goes out to its origin
//! when its project lets it, through the gate when its project asks.
//!
//! Every repository is one the platform made in the journey's own folder,
//! and the one `origin` is another project's tree on this disk: nothing
//! leaves the machine. Nothing here throws a change away — the verbs that do
//! are the version-control crate's own tests', against repositories of
//! theirs.

use super::sealed::Sealed;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const COMMITTER: &str = "A Journey <journey@example.test>";

/// A project that is a repository, with who commits in it set: its id —
/// which is its primary workstream's — and its tree.
fn a_project(ws: &Sealed, slug: &str, publish: &str, more: &[&str]) -> (String, PathBuf) {
    let mut args = vec!["project", "new", slug, "--publish", publish];
    args.extend(more);
    let made = ws.json(&args);
    (
        made["project"]["id"]
            .as_str()
            .unwrap_or_else(|| panic!("the project: {made}"))
            .to_string(),
        PathBuf::from(made["path"].as_str().expect("its tree")),
    )
}

/// The changed paths of a project's own tree, each with its two letters:
/// the index's and the working tree's — git's own, `.` where a side has
/// nothing to say and `?` for a path it has never seen.
fn changed(ws: &Sealed, project: &str) -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = ws.json(&["project", "files", project])["files"]
        .as_array()
        .expect("the files")
        .iter()
        .map(|file| {
            (
                file["path"].as_str().unwrap_or_default().to_string(),
                format!(
                    "{}{}",
                    file["index"].as_str().unwrap_or("?"),
                    file["worktree"].as_str().unwrap_or("?")
                ),
            )
        })
        .collect();
    files.sort();
    files
}

/// A call of a route of a checkout, answered 200.
fn asked(ws: &Sealed, method: &str, checkout: &str, route: &str, body: Option<Value>) -> Value {
    let (status, answer) = ws.call(
        method,
        &format!("/workstreams/{checkout}/git/{route}"),
        &[],
        body.as_ref(),
    );
    assert_eq!(status, 200, "{method} {route}: {answer}");
    answer
}

fn write(tree: &Path, name: &str, text: &str) {
    std::fs::write(tree.join(name), text).expect("a file in the checkout");
}

/// Ten lines, numbered.
fn ten_lines() -> String {
    (1..=10).map(|n| format!("line {n}\n")).collect()
}

#[test]
fn a_change_is_seen_staged_by_file_and_by_hunk_and_committed_by_somebody_who_may() {
    let mut ws = Sealed::bare();
    ws.start();
    let (shelf, tree) = a_project(&ws, "shelf", "manual", &["--committer", COMMITTER]);

    // Seen: what git has something to say about, and nothing else.
    assert_eq!(changed(&ws, &shelf), vec![]);
    write(&tree, "plan.md", &ten_lines());
    write(&tree, "notes.md", "measure twice\n");
    assert_eq!(
        changed(&ws, &shelf),
        vec![
            ("notes.md".to_string(), "??".to_string()),
            ("plan.md".to_string(), "??".to_string())
        ]
    );

    // Staged by file, taken back out, staged again: the index alone moves.
    let staged = ws.json(&["project", "stage", &shelf, "plan.md", "notes.md"]);
    assert_eq!(
        staged["files"].as_array().map(Vec::len),
        Some(2),
        "{staged}"
    );
    ws.ok(&["project", "unstage", &shelf, "notes.md"]);
    assert_eq!(
        changed(&ws, &shelf),
        vec![
            ("notes.md".to_string(), "??".to_string()),
            ("plan.md".to_string(), "A.".to_string())
        ]
    );
    assert_eq!(
        std::fs::read_to_string(tree.join("notes.md")).expect("the file"),
        "measure twice\n",
        "the file itself is as it was"
    );

    // Committed: what is staged, and nothing more.
    let committed = ws.json(&["project", "commit", &shelf, "-m", "the plan"]);
    assert_eq!(
        committed["short"].as_str().map(str::len),
        Some(7),
        "{committed}"
    );
    assert_eq!(
        changed(&ws, &shelf),
        vec![("notes.md".to_string(), "??".to_string())]
    );
    // With nothing staged there is nothing to commit, and it says so.
    let nothing = ws.bisa(&["project", "commit", &shelf, "-m", "again"]);
    assert!(!nothing.status.success());
    assert!(
        !String::from_utf8_lossy(&nothing.stderr).contains("another engine holds"),
        "{}",
        String::from_utf8_lossy(&nothing.stderr)
    );
    // Named, a path is staged on the way.
    ws.ok(&[
        "project",
        "commit",
        &shelf,
        "-m",
        "the notes",
        "--path",
        "notes.md",
    ]);
    assert_eq!(changed(&ws, &shelf), vec![]);

    // Staged by hunk: two changes far apart in one file are two hunks, and
    // one of them is committed while the other stays the tree's.
    let edited = ten_lines()
        .replace("line 1\n", "line one\n")
        .replace("line 10\n", "line ten\n");
    write(&tree, "plan.md", &edited);
    let patch = ws.json(&["project", "diff", &shelf, "plan.md"])["diff"]
        .as_str()
        .expect("the patch")
        .to_string();
    let hunks: Vec<&str> = patch.split("\n@@ ").collect();
    assert_eq!(hunks.len(), 3, "a header and two hunks:\n{patch}");
    let first = format!("{}\n@@ {}\n", hunks[0], hunks[1].trim_end_matches('\n'));
    asked(
        &ws,
        "POST",
        &shelf,
        "hunk",
        Some(json!({ "patch": first, "reverse": false })),
    );
    assert_eq!(
        changed(&ws, &shelf),
        vec![("plan.md".to_string(), "MM".to_string())],
        "part of it staged, part of it not"
    );
    let staged = ws.json(&["project", "diff", &shelf, "plan.md", "--staged"])["diff"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(
        staged.contains("+line one") && !staged.contains("+line ten"),
        "{staged}"
    );
    ws.ok(&["project", "commit", &shelf, "-m", "the first line"]);
    assert_eq!(
        changed(&ws, &shelf),
        vec![("plan.md".to_string(), ".M".to_string())]
    );
    assert_eq!(
        std::fs::read_to_string(tree.join("plan.md")).expect("the file"),
        edited,
        "staging a hunk never touches the file"
    );

    // Who commits here, and the repository's own config — never the machine's.
    let who = ws.json(&["project", "identity", &shelf]);
    assert_eq!(who["name"], "A Journey", "{who}");
    assert_eq!(who["source"], "local", "{who}");
    let set = ws.json(&[
        "project",
        "identity",
        &shelf,
        "--name",
        "Another Journey",
        "--email",
        "another@example.test",
    ]);
    assert_eq!(set["email"], "another@example.test", "{set}");
    let config = ws.json(&["project", "git-config", &shelf, "--set", "pull.rebase=true"]);
    let local = |config: &Value, key: &str| -> Value {
        config["entries"]
            .as_array()
            .and_then(|all| all.iter().find(|entry| entry["key"] == key))
            .map(|entry| entry["local"].clone())
            .unwrap_or(Value::Null)
    };
    assert_eq!(local(&config, "pull.rebase"), "true", "{config}");
    assert_eq!(local(&config, "user.name"), "Another Journey", "{config}");
    let config = ws.json(&["project", "git-config", &shelf, "--unset", "pull.rebase"]);
    assert_eq!(local(&config, "pull.rebase"), Value::Null, "{config}");
    let refused = ws.bisa(&[
        "project",
        "git-config",
        &shelf,
        "--set",
        "core.hooksPath=/somewhere",
    ]);
    assert!(
        !refused.status.success(),
        "a key the platform does not write is refused"
    );

    // A repository nobody commits in refuses a commit, names the remedy, and
    // commits once somebody is set.
    let (bare, bare_tree) = a_project(&ws, "nobody", "manual", &[]);
    assert_eq!(ws.json(&["project", "identity", &bare])["source"], "none");
    write(&bare_tree, "first.md", "a start\n");
    let refused = ws.bisa(&[
        "project", "commit", &bare, "-m", "a start", "--path", "first.md",
    ]);
    assert!(!refused.status.success());
    assert_eq!(
        changed(&ws, &bare),
        vec![("first.md".to_string(), "??".to_string())],
        "a refused commit leaves the index as it was"
    );
    ws.ok(&[
        "project",
        "identity",
        &bare,
        "--name",
        "A Journey",
        "--email",
        "journey@example.test",
    ]);
    ws.ok(&[
        "project", "commit", &bare, "-m", "a start", "--path", "first.md",
    ]);
    assert_eq!(changed(&ws, &bare), vec![]);

    // A workstream's own commit takes everything in its checkout.
    let opened = ws.json(&["workstream", "open", &shelf, "--from", "new:feature/paint"]);
    let paint = opened["workstream"]["id"]
        .as_str()
        .expect("the workstream")
        .to_string();
    let checkout = PathBuf::from(opened["path"].as_str().expect("its checkout"));
    write(&checkout, "paint.md", "two coats\n");
    write(&checkout, "brushes.md", "one wide, one fine\n");
    let committed = ws.json(&["workstream", "commit", &paint, "-m", "paint"]);
    assert_eq!(
        committed["short"].as_str().map(str::len),
        Some(7),
        "{committed}"
    );
    let shown = ws.json(&["workstream", "show", &paint]);
    assert_eq!(shown["workstream"]["state"]["state"], "committed");
    assert_eq!(shown["status"]["clean"], true, "{shown}");
    assert_eq!(shown["status"]["ahead_of_base"], 1);
    ws.stop();
}

#[test]
fn what_moves_the_tree_is_kept_first_and_a_branch_goes_out_when_its_project_lets_it() {
    let mut ws = Sealed::bare();
    ws.start();
    let (shelf, tree) = a_project(&ws, "shelf", "manual", &["--committer", COMMITTER]);
    write(&tree, "plan.md", &ten_lines());
    ws.ok(&[
        "project", "commit", &shelf, "-m", "the plan", "--path", "plan.md",
    ]);
    let recovery = |ws: &Sealed, checkout: &str| -> Vec<Value> {
        asked(ws, "GET", checkout, "recovery", None)["recovery"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    };
    assert_eq!(recovery(&ws, &shelf), Vec::<Value>::new());

    // Put aside: the tree is clean, the change is an entry of the stash,
    // and what was there is a recovery point made before anything moved.
    write(
        &tree,
        "plan.md",
        &ten_lines().replace("line 5\n", "line five\n"),
    );
    asked(
        &ws,
        "POST",
        &shelf,
        "stash",
        Some(json!({ "message": "line five" })),
    );
    assert_eq!(
        std::fs::read_to_string(tree.join("plan.md")).expect("the file"),
        ten_lines()
    );
    let stashes = asked(&ws, "GET", &shelf, "stashes", None)["stashes"]
        .as_array()
        .cloned()
        .expect("the stash list");
    assert_eq!(stashes.len(), 1, "{stashes:?}");
    assert_eq!(stashes[0]["message"], "line five");
    let kept = recovery(&ws, &shelf);
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(
        kept[0]["kind"], "tree",
        "the index and the tree, as they were"
    );

    // Taken back: the change is the tree's again, the entry is gone from the
    // list and kept as a recovery point of its own.
    let sha = stashes[0]["commit"].as_str().expect("its commit");
    let (status, popped) = ws.call(
        "POST",
        &format!("/workstreams/{shelf}/git/stashes/{sha}/pop"),
        &[],
        Some(&json!({ "index": 0 })),
    );
    assert_eq!(status, 200, "{popped}");
    assert!(std::fs::read_to_string(tree.join("plan.md"))
        .expect("the file")
        .contains("line five"));
    assert_eq!(
        asked(&ws, "GET", &shelf, "stashes", None)["stashes"],
        json!([])
    );
    assert!(
        recovery(&ws, &shelf)
            .iter()
            .any(|point| point["kind"] == "stash"),
        "a popped entry can be put back"
    );
    // An entry named by a place that is no longer its own is refused.
    let (status, moved) = ws.call(
        "POST",
        &format!("/workstreams/{shelf}/git/stashes/{sha}/apply"),
        &[],
        Some(&json!({ "index": 0 })),
    );
    assert_eq!(status, 409, "{moved}");
    assert_eq!(moved["code"], "stash_moved", "{moved}");
    ws.ok(&[
        "project",
        "commit",
        &shelf,
        "-m",
        "line five",
        "--path",
        "plan.md",
    ]);

    // A person's request is proved by the token in the request's own
    // header: in the address it opens a stream, and moves no tree.
    let token = std::fs::read_to_string(ws.data().join("run/token")).expect("the node's token");
    let (status, refused) = ws.call_with(
        "POST",
        &format!("/workstreams/{shelf}/git/stash?token={}", token.trim()),
        &[],
        Some(&json!({})),
    );
    assert_eq!(status, 401, "{refused}");

    // A branch merged into the one it came from: a merge commit, asked for
    // by name, with where HEAD stood kept first.
    let opened = ws.json(&["workstream", "open", &shelf, "--from", "new:feature/paint"]);
    let paint = opened["workstream"]["id"]
        .as_str()
        .expect("the workstream")
        .to_string();
    let checkout = PathBuf::from(opened["path"].as_str().expect("its checkout"));
    write(&checkout, "paint.md", "two coats\n");
    ws.ok(&["workstream", "commit", &paint, "-m", "paint"]);
    let before = recovery(&ws, &shelf).len();
    let preview = asked(
        &ws,
        "GET",
        &shelf,
        "merge-preview?source=feature%2Fpaint",
        None,
    );
    assert_eq!(preview["clean"], true, "{preview}");
    asked(
        &ws,
        "POST",
        &shelf,
        "merge",
        Some(json!({ "source": "feature/paint", "mode": "no_ff", "message": "Merge the paint" })),
    );
    assert!(
        tree.join("paint.md").is_file(),
        "what the branch held is here"
    );
    assert_eq!(recovery(&ws, &shelf).len(), before + 1);

    // Out to its origin. A project that publishes by hand refuses, by name…
    let (status, refused) = ws.call("POST", &format!("/workstreams/{paint}/push"), &[], None);
    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["code"], "publish_manual", "{refused}");

    // …one that publishes by itself pushes: a clone of the shelf, whose
    // origin is the shelf's own tree on this disk.
    let cloned = ws.json(&[
        "project",
        "clone",
        &tree.to_string_lossy(),
        "--slug",
        "shelf-again",
        "--publish",
        "auto",
        "--committer",
        COMMITTER,
    ]);
    let again = cloned["project"]["id"]
        .as_str()
        .expect("the clone")
        .to_string();
    let opened = ws.json(&[
        "workstream",
        "open",
        &again,
        "--from",
        "new:feature/brackets",
    ]);
    let brackets = opened["workstream"]["id"]
        .as_str()
        .expect("the workstream")
        .to_string();
    write(
        &PathBuf::from(opened["path"].as_str().expect("its checkout")),
        "brackets.md",
        "two a board\n",
    );
    ws.ok(&["workstream", "commit", &brackets, "-m", "brackets"]);
    let (status, pushed) = ws.call("POST", &format!("/workstreams/{brackets}/push"), &[], None);
    assert_eq!(status, 200, "{pushed}");
    assert_eq!(pushed["pushed"], true, "{pushed}");
    assert_eq!(pushed["workstream"]["state"]["state"], "pushed", "{pushed}");
    let at_the_origin = asked(&ws, "GET", &shelf, "branches", None);
    assert!(
        at_the_origin.to_string().contains("feature/brackets"),
        "the branch is at its origin: {at_the_origin}"
    );

    // …and one that asks first opens a gate on the goal the work is for,
    // and pushes once its person says yes.
    let gated = ws.json(&[
        "project",
        "clone",
        &tree.to_string_lossy(),
        "--slug",
        "shelf-gated",
        "--publish",
        "gated",
        "--committer",
        COMMITTER,
    ]);
    let gated = gated["project"]["id"]
        .as_str()
        .expect("the clone")
        .to_string();
    let goal = ws.json(&["new", "hang the shelf", "--mode", "manual"])["goal"]
        .as_str()
        .expect("the goal")
        .to_string();
    ws.ok(&["project", "attach", &gated, &goal]);
    let opened = ws.json(&[
        "workstream",
        "open",
        &gated,
        "--goal",
        &goal,
        "--from",
        "new:feature/screws",
    ]);
    let screws = opened["workstream"]["id"]
        .as_str()
        .expect("the workstream")
        .to_string();
    write(
        &PathBuf::from(opened["path"].as_str().expect("its checkout")),
        "screws.md",
        "eight\n",
    );
    ws.ok(&["workstream", "commit", &screws, "-m", "screws"]);
    let (status, waiting) = ws.call("POST", &format!("/workstreams/{screws}/push"), &[], None);
    assert_eq!(status, 202, "{waiting}");
    assert_eq!(waiting["pushed"], false, "nothing has left: {waiting}");
    assert!(
        !asked(&ws, "GET", &shelf, "branches", None)
            .to_string()
            .contains("feature/screws"),
        "nothing is at the origin before its person says so"
    );
    ws.ok(&["approve", &goal]);
    ws.until("the branch to be at its origin", || {
        let state = ws.json(&["workstream", "show", &screws])["workstream"]["state"]["state"]
            .as_str()
            .map(str::to_string)?;
        (state == "pushed").then_some(())
    });
    assert!(asked(&ws, "GET", &shelf, "branches", None)
        .to_string()
        .contains("feature/screws"));
    ws.stop();
}
