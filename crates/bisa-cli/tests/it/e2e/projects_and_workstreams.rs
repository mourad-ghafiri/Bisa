//! Projects and workstreams through the binary **while a node runs** — as
//! one does whenever the desktop is open: a project comes in each of its four
//! ways, is attached to a goal, carried, shown and forgotten; a workstream
//! is opened from each source a machine alone can give, renamed, dated,
//! placed on the Board and closed — and all of it is there when the node is
//! back.
//!
//! Every repository here is one the platform made in the journey's own
//! folder; the one `origin` is another project's tree on this disk. Nothing
//! is pushed, no pull request is asked for — the binary reaches no code host
//! — and no tree is removed.

use super::sealed::{Listening, Sealed};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const COMMITTER: &str = "A Journey <journey@example.test>";

/// The id of what a verb made, under `key`.
fn id_of(made: &Value, key: &str) -> String {
    made[key]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("the {key} it made: {made}"))
        .to_string()
}

fn path_of(made: &Value) -> PathBuf {
    PathBuf::from(
        made["path"]
            .as_str()
            .unwrap_or_else(|| panic!("where it is: {made}")),
    )
}

/// A folder of the journey's own, outside the workspace, holding one file.
fn a_folder(ws: &Sealed, name: &str, file: &str, text: &str) -> PathBuf {
    let beside = ws.file(&format!("{name}.beside"), "");
    let folder = beside.parent().expect("the journey's files").join(name);
    std::fs::create_dir_all(&folder).expect("a folder of the journey's own");
    std::fs::write(folder.join(file), text).expect("a file in it");
    folder
}

/// A new project that is a repository with a first commit, so a branch has
/// somewhere to start.
fn a_project(ws: &Sealed, slug: &str) -> (String, PathBuf) {
    let made = ws.json(&[
        "project",
        "new",
        slug,
        "--publish",
        "manual",
        "--committer",
        COMMITTER,
    ]);
    assert_eq!(made["project"]["slug"], slug, "{made}");
    assert_eq!(made["project"]["vcs"]["type"], "git", "{made}");
    (id_of(&made, "project"), path_of(&made))
}

/// A file committed on a checkout, through the node as the desktop does it.
fn committed(ws: &Sealed, workstream: &str, checkout: &Path, name: &str, text: &str) {
    std::fs::write(checkout.join(name), text).expect("a file in the checkout");
    let (status, done) = ws.call(
        "POST",
        &format!("/workstreams/{workstream}/git/commit"),
        &[],
        Some(&json!({ "message": format!("add {name}"), "paths": [name] })),
    );
    assert_eq!(status, 200, "{done}");
}

/// The workstreams of a project as `workstream list` says them: `(id, branch or kind, state)`.
fn listed(ws: &Sealed, project: &str) -> Vec<(String, String, String)> {
    ws.json(&["workstream", "list", "--project", project])["workstreams"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|w| {
            (
                w["id"].as_str().unwrap_or_default().to_string(),
                w["kind"]["branch"]
                    .as_str()
                    .or(w["kind"]["kind"].as_str())
                    .unwrap_or_default()
                    .to_string(),
                w["state"]["state"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect()
}

#[test]
fn a_project_comes_in_four_ways_and_is_attached_carried_shown_and_forgotten() {
    let mut ws = Sealed::bare();
    ws.start();

    // New: a folder of the workspace's own, a repository from birth.
    let (shelf, tree) = a_project(&ws, "shelf");
    assert!(tree.join(".git").exists(), "{}", tree.display());
    assert!(tree.starts_with(ws.data()), "{}", tree.display());

    // Imported: the files become the workspace's, and the folder they came
    // from is as it was.
    let folder = a_folder(&ws, "notes", "readme.md", "what the shelf holds\n");
    let imported = ws.json(&[
        "project",
        "import",
        &folder.to_string_lossy(),
        "--slug",
        "notes",
        "--publish",
        "manual",
    ]);
    let notes = path_of(&imported);
    assert!(notes.starts_with(ws.data()), "{}", notes.display());
    assert_eq!(
        std::fs::read_to_string(notes.join("readme.md")).expect("the copy"),
        "what the shelf holds\n"
    );
    assert_eq!(
        std::fs::read_dir(&folder).expect("the source").count(),
        1,
        "nothing was written beside what was imported"
    );

    // Adopted: the folder stays where it lies, and nothing is written into it.
    let elsewhere = a_folder(&ws, "elsewhere", "plan.md", "a plan\n");
    let adopted = ws.json(&[
        "project",
        "adopt",
        &elsewhere.to_string_lossy(),
        "--slug",
        "elsewhere",
        "--publish",
        "manual",
    ]);
    assert_eq!(adopted["project"]["root"]["type"], "external", "{adopted}");
    assert_eq!(
        path_of(&adopted).canonicalize().expect("where it lies"),
        elsewhere.canonicalize().expect("the folder")
    );
    assert_eq!(
        std::fs::read_dir(&elsewhere).expect("the folder").count(),
        1,
        "an adopted folder is written into by nobody"
    );

    // Cloned: from a repository on this disk — the first project's own.
    let cloned = ws.json(&[
        "project",
        "clone",
        &tree.to_string_lossy(),
        "--slug",
        "shelf-again",
        "--publish",
        "manual",
        "--committer",
        COMMITTER,
    ]);
    assert_eq!(cloned["project"]["vcs"]["type"], "git", "{cloned}");
    assert!(path_of(&cloned).join(".git").exists());

    // A slug that is taken, and one that is no slug, are refused in words.
    for (slug, why) in [("shelf", "taken"), ("No Such Slug!", "no slug")] {
        let refused = ws.bisa(&["project", "new", slug]);
        assert!(!refused.status.success(), "{slug} is {why}");
        assert!(
            !String::from_utf8_lossy(&refused.stderr)
                .contains("another engine holds this workspace"),
            "the node was asked, and said why: {}",
            String::from_utf8_lossy(&refused.stderr)
        );
    }

    // Attached to a goal and detached again: one record, nothing moves.
    let goal = ws.json(&["new", "put the shelf up", "--mode", "manual"])["goal"]
        .as_str()
        .expect("the goal")
        .to_string();
    let attached = ws.json(&["project", "attach", &shelf, &goal]);
    assert_eq!(attached["attached_to"], goal.as_str(), "{attached}");
    let of_the_goal = ws.json(&["project", "list", "--goal", &goal]);
    assert_eq!(
        of_the_goal["projects"]
            .as_array()
            .map(|all| all.iter().map(|p| p["slug"].clone()).collect::<Vec<_>>()),
        Some(vec![json!("shelf")]),
        "{of_the_goal}"
    );
    ws.ok(&["project", "detach", &shelf, &goal]);
    assert_eq!(
        ws.json(&["project", "list", "--goal", &goal])["projects"],
        json!([])
    );
    assert!(tree.join(".git").exists(), "detaching moved nothing");

    // Carried by an agent, then by nobody else: `--replace` is exactly these.
    let carried = ws.json(&["project", "assign", &shelf, "agent:general-agent"]);
    assert_eq!(carried["assignees"], json!(["agent:general-agent"]));
    let carried = ws.json(&[
        "project",
        "assign",
        &shelf,
        "agent:workflow-agent",
        "--replace",
    ]);
    assert_eq!(carried["assignees"], json!(["agent:workflow-agent"]));
    let ghost = ws.bisa(&["project", "assign", &shelf, "agent:nobody-here"]);
    assert!(
        !ghost.status.success(),
        "an agent nobody defined carries nothing"
    );

    // A goal is carried the same way, by the same rule: the node is asked,
    // and somebody the workspace does not have carries nothing.
    let carried = ws.json(&["assign", &goal, "agent:general-agent"]);
    assert_eq!(carried["assignees"], json!(["agent:general-agent"]));
    let said = ws.refused(&["assign", &goal, "agent:nobody-here"]);
    assert!(
        said.contains("nobody-here"),
        "it names who is not here: {said}"
    );
    assert_eq!(
        ws.json(&["status", &goal])["goal"]["assignees"],
        json!([{"agent": "general-agent"}]),
        "a refusal changes nothing"
    );
    let cleared = ws.json(&["unassign", &goal]);
    assert_eq!(cleared["assignees"], json!([]));

    // Shown, and listed with the others.
    let shown = ws.json(&["project", "show", &shelf]);
    assert_eq!(shown["project"]["id"], shelf.as_str());
    assert_eq!(
        shown["workstreams"].as_array().map(Vec::len),
        Some(1),
        "its primary, from birth: {shown}"
    );
    let mut slugs: Vec<String> = ws.json(&["project", "list"])["projects"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p["slug"].as_str().map(str::to_string))
        .collect();
    slugs.sort();
    assert_eq!(slugs, vec!["elsewhere", "notes", "shelf", "shelf-again"]);

    // Forgotten: the record goes, the folder stays.
    let forgotten = ws.json(&["project", "rm", &id_of(&imported, "project")]);
    assert_eq!(forgotten["removed_tree"], false, "{forgotten}");
    assert!(
        notes.join("readme.md").exists(),
        "the files are left on disk"
    );

    // And it is all there when the node is back.
    ws.stop();
    ws.start();
    let mut after: Vec<String> = ws.json(&["project", "list"])["projects"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p["slug"].as_str().map(str::to_string))
        .collect();
    after.sort();
    assert_eq!(after, vec!["elsewhere", "shelf", "shelf-again"]);
    ws.stop();
}

#[test]
fn a_workstream_is_opened_from_each_source_placed_on_the_board_and_closed() {
    let mut ws = Sealed::bare();
    ws.start();
    let (shelf, tree) = a_project(&ws, "shelf");
    // The primary's id is its project's.
    committed(&ws, &shelf, &tree, "shelf.md", "three boards\n");
    let base = ws.json(&["project", "show", &shelf])["project"]["vcs"]["default_branch"]
        .as_str()
        .expect("its default branch")
        .to_string();

    // A new branch, named: a checkout of its own beside the primary.
    let opened = ws.json(&[
        "workstream",
        "open",
        &shelf,
        "--from",
        "new:feature/brackets",
    ]);
    let brackets = id_of(&opened, "workstream");
    let checkout = path_of(&opened);
    assert_eq!(opened["workstream"]["kind"]["branch"], "feature/brackets");
    assert_eq!(opened["workstream"]["kind"]["base"], base.as_str());
    assert_eq!(opened["workstream"]["state"]["state"], "open");
    assert!(checkout.join("shelf.md").exists(), "it starts at the base");
    assert_ne!(checkout, tree, "switching touches no tree");

    // A name that exists is refused by name, never checked out in silence.
    let again = ws.bisa(&[
        "workstream",
        "open",
        &shelf,
        "--from",
        "new:feature/brackets",
    ]);
    assert!(!again.status.success());
    assert!(
        String::from_utf8_lossy(&again.stderr).contains("feature/brackets"),
        "{}",
        String::from_utf8_lossy(&again.stderr)
    );

    // A new branch, derived from its label, at a ref.
    let derived = ws.json(&["workstream", "open", &shelf, "--label", "paint"]);
    assert!(
        derived["workstream"]["kind"]["branch"]
            .as_str()
            .is_some_and(|branch| branch.starts_with("work/paint-")),
        "{derived}"
    );

    // A tag made first, and a branch at it.
    let tagged = ws.json(&[
        "workstream",
        "open",
        &shelf,
        "--from",
        &format!("new-tag:v1@{base}"),
    ]);
    assert_eq!(
        tagged["workstream"]["kind"]["branch"], "from/v1",
        "{tagged}"
    );
    // A tag that is there already: a branch at it, named after it — and a
    // second one at the same tag is refused by the name it would take.
    let (status, made) = ws.call(
        "POST",
        &format!("/workstreams/{shelf}/git/tags"),
        &[],
        Some(&json!({ "name": "v2", "target": base })),
    );
    assert_eq!(status, 200, "{made}");
    let at_the_tag = ws.json(&["workstream", "open", &shelf, "--from", "tag:v2"]);
    assert_eq!(
        at_the_tag["workstream"]["kind"]["branch"], "from/v2",
        "{at_the_tag}"
    );
    let twice = ws.bisa(&["workstream", "open", &shelf, "--from", "tag:v2"]);
    assert!(!twice.status.success());
    assert!(
        String::from_utf8_lossy(&twice.stderr).contains("from/v2"),
        "{}",
        String::from_utf8_lossy(&twice.stderr)
    );

    // Work on the branch: the record follows what git says.
    committed(&ws, &brackets, &checkout, "brackets.md", "two a board\n");
    let shown = ws.json(&["workstream", "show", &brackets]);
    assert_eq!(
        shown["workstream"]["state"]["state"], "committed",
        "{shown}"
    );
    assert_eq!(shown["status"]["ahead_of_base"], 1, "{shown}");

    // Closed as a record — the checkout and its branch stay — and the branch
    // opened again, as it is.
    let closed = ws.json(&["workstream", "close", &brackets]);
    assert_eq!(closed["removed_tree"], false, "{closed}");
    assert_eq!(closed["stopped_sessions"], 0, "{closed}");
    assert!(checkout.join("brackets.md").exists());

    // A remote's branch: a project cloned from this one has it as `origin`.
    let cloned = ws.json(&[
        "project",
        "clone",
        &tree.to_string_lossy(),
        "--slug",
        "shelf-again",
        "--publish",
        "manual",
        "--committer",
        COMMITTER,
    ]);
    let again = id_of(&cloned, "project");
    let tracked = ws.json(&[
        "workstream",
        "open",
        &again,
        "--from",
        "remote:origin/feature/brackets",
    ]);
    assert_eq!(tracked["workstream"]["kind"]["branch"], "feature/brackets");
    assert!(
        path_of(&tracked).join("brackets.md").exists(),
        "the remote's branch, as it stands there"
    );
    // A branch the clone has locally — the one it tracks now — cannot be
    // held by two checkouts: git's refusal, in words.
    let twice = ws.bisa(&[
        "workstream",
        "open",
        &again,
        "--from",
        "branch:feature/brackets",
    ]);
    assert!(!twice.status.success());

    // The primary is the project: it is never closed.
    let refused = ws.bisa(&["workstream", "close", &shelf]);
    assert!(!refused.status.success());
    assert!(
        !String::from_utf8_lossy(&refused.stderr).contains("another engine holds"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );

    // A name, a note, a pin and a day: a person's, and the branch untouched.
    let paint = id_of(&derived, "workstream");
    let (status, edited) = ws.call(
        "PATCH",
        &format!("/workstreams/{paint}"),
        &[],
        Some(&json!({ "name": "Paint it", "note": "two coats", "pinned": true, "due": "2027-04-12" })),
    );
    assert_eq!(status, 200, "{edited}");
    assert_eq!(edited["workstream"]["name"], "Paint it", "{edited}");
    assert_eq!(
        edited["workstream"]["board"]["due"], "2027-04-12",
        "{edited}"
    );
    let (status, refused) = ws.call(
        "PATCH",
        &format!("/workstreams/{paint}"),
        &[],
        Some(&json!({ "due": "the twelfth" })),
    );
    assert_eq!(status, 400, "a day is a day of the calendar: {refused}");

    // Placed on the Board: a column is a person's, never the lifecycle.
    let place = |workstream: &str, column: &str, index: u64| {
        ws.call(
            "PUT",
            &format!("/workstreams/{workstream}/board/place"),
            &[],
            Some(&json!({ "column": column, "index": index })),
        )
    };
    let (status, placed) = place(&paint, "todo", 0);
    assert_eq!(status, 200, "{placed}");
    let (status, placed) = place(&id_of(&tagged, "workstream"), "todo", 0);
    assert_eq!(status, 200, "{placed}");
    let column = |ws: &Sealed| -> Vec<String> {
        let mut cards: Vec<(u64, String)> = ws.json(&["workstream", "list", "--project", &shelf])
            ["workstreams"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|w| w["board"]["column"] == "todo")
            .map(|w| {
                (
                    w["board"]["rank"].as_u64().unwrap_or(u64::MAX),
                    w["id"].as_str().unwrap_or_default().to_string(),
                )
            })
            .collect();
        cards.sort();
        cards.into_iter().map(|(_, id)| id).collect()
    };
    assert_eq!(
        column(&ws),
        vec![id_of(&tagged, "workstream"), paint.clone()],
        "the card dropped first in the column is first"
    );
    assert_eq!(
        ws.json(&["workstream", "show", &paint])["workstream"]["state"]["state"],
        "open",
        "placing a card moved nothing else"
    );
    // A closed workstream is Archived and nothing else.
    let (status, refused) = place(&brackets, "doing", 0);
    assert_eq!(status, 400, "{refused}");
    let (status, refused) = place(&paint, "someday", 0);
    assert_eq!(status, 400, "{refused}");

    // What the list says, the primary first.
    let before = listed(&ws, &shelf);
    assert_eq!(before[0].0, shelf, "the primary first: {before:?}");
    assert_eq!(before[0].1, "primary");
    assert_eq!(before.len(), 5, "the primary and four branches: {before:?}");
    assert_eq!(
        before
            .iter()
            .find(|(id, _, _)| *id == brackets)
            .map(|(_, _, state)| state.as_str()),
        Some("closed")
    );

    // And when the node is back it is all as it was.
    ws.stop();
    ws.start();
    assert_eq!(listed(&ws, &shelf), before);
    assert_eq!(column(&ws), vec![id_of(&tagged, "workstream"), paint]);
    ws.stop();
}

/// The slugs `project list` says, in order.
fn slugs(ws: &Sealed) -> Vec<String> {
    let mut slugs: Vec<String> = ws.json(&["project", "list"])["projects"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p["slug"].as_str().map(str::to_string))
        .collect();
    slugs.sort();
    slugs
}

#[test]
fn a_goal_goes_with_the_projects_born_of_it_kept_put_away_or_forgotten() {
    let mut ws = Sealed::bare();
    ws.start();
    let goal_with = |statement: &str, born: &str| -> (String, String, PathBuf) {
        let goal = ws.json(&["new", statement, "--mode", "manual"])["goal"]
            .as_str()
            .expect("the goal")
            .to_string();
        let made = ws.json(&[
            "project",
            "new",
            born,
            "--goal",
            &goal,
            "--publish",
            "manual",
            "--committer",
            COMMITTER,
        ]);
        assert_eq!(
            made["project"]["origin"],
            json!({ "origin": "goal", "goal": goal }),
            "born of the goal it was made for: {made}"
        );
        (goal, id_of(&made, "project"), path_of(&made))
    };
    // One project of the workspace's own, attached to every goal below: a
    // goal that goes only lets it go.
    let (garden, _) = a_project(&ws, "garden");

    // Kept: the goal goes, what was born of it stays, attached to nothing.
    let (goal, shed, _) = goal_with("build the shed", "shed");
    ws.ok(&["project", "attach", &garden, &goal]);
    ws.ok(&["rm", &goal, "--projects", "keep"]);
    assert!(
        !ws.bisa(&["status", &goal]).status.success(),
        "the goal is gone"
    );
    assert_eq!(slugs(&ws), vec!["garden", "shed"]);
    assert_eq!(
        ws.json(&["project", "show", &shed])["project"]["id"],
        shed.as_str()
    );

    // Put away: out of the list, and its files where they were.
    let (goal, fence, fence_tree) = goal_with("mend the fence", "fence");
    ws.ok(&["project", "attach", &garden, &goal]);
    ws.ok(&["rm", &goal, "--projects", "archive"]);
    assert_eq!(
        slugs(&ws),
        vec!["garden", "shed"],
        "a project put away is not listed"
    );
    let put_away = ws.json(&["project", "show", &fence]);
    assert!(
        put_away["project"]["archived"].is_object(),
        "it says when it was put away: {put_away}"
    );
    assert!(fence_tree.join(".git").exists());

    // Forgotten: the record goes, and with no `--tree` the folder stays.
    let (goal, gate, gate_tree) = goal_with("hang the gate", "gate");
    ws.ok(&["project", "attach", &garden, &goal]);
    ws.ok(&["rm", &goal, "--projects", "delete"]);
    assert!(
        !ws.bisa(&["project", "show", &gate]).status.success(),
        "the project is forgotten"
    );
    assert!(
        gate_tree.join(".git").exists(),
        "its folder is left on disk"
    );

    // The one that was only attached is as it was, attached to nothing.
    assert_eq!(slugs(&ws), vec!["garden", "shed"]);
    let (status, row) = ws.call("GET", &format!("/projects/{garden}"), &[], None);
    assert_eq!(status, 200, "{row}");
    assert_eq!(row["goals"], json!([]), "{row}");
    ws.stop();
}

/// What the bus said of projects put away, taken back out and forgotten, in
/// the order it said it: `(fact, project, archived)`.
fn goings(listening: &Listening) -> Vec<(String, String, Option<bool>)> {
    listening
        .heard()
        .frames
        .iter()
        .filter(|frame| frame["stream"] == "engine")
        .map(|frame| &frame["payload"]["payload"])
        .filter(|said| said["type"] == "project_archived" || said["type"] == "project_deleted")
        .map(|said| {
            (
                said["type"].as_str().unwrap_or_default().to_string(),
                said["project"].as_str().unwrap_or_default().to_string(),
                said["archived"].as_bool(),
            )
        })
        .collect()
}

/// A project made as the desktop's dialog makes one — the node's own route.
fn made_by_the_dialog(ws: &Sealed, route: &str, slug: &str) -> Value {
    let (status, made) = ws.call(
        "POST",
        route,
        &[],
        Some(&json!({
            "kind": "new",
            "slug": slug,
            "publish": "manual",
            "git_config": { "user.name": "A Journey", "user.email": "journey@example.test" },
        })),
    );
    assert_eq!(status, 200, "{made}");
    made
}

#[test]
fn a_project_belongs_to_no_goal_unless_its_opener_named_one_and_its_going_is_said_to_whoever_listens(
) {
    let mut ws = Sealed::bare();
    ws.start();
    let listening = ws.listen();
    let goal = ws.json(&["new", "fit out the workshop", "--mode", "manual"])["goal"]
        .as_str()
        .expect("the goal")
        .to_string();

    // The dialog asks nothing about a goal: what it makes is the
    // workspace's, attached to nothing — whatever goals there are.
    let bench = made_by_the_dialog(&ws, "/projects", "bench");
    assert_eq!(
        bench["project"]["origin"],
        json!({ "origin": "workspace" }),
        "{bench}"
    );
    assert_eq!(bench["goals"], json!([]), "{bench}");
    assert_eq!(bench["attached_to"], Value::Null, "{bench}");
    let bench = id_of(&bench, "project");
    assert_eq!(
        ws.json(&["project", "list", "--goal", &goal])["projects"],
        json!([]),
        "a goal is given no project it was not named for"
    );

    // A body cannot say a goal: the key is nobody's, and nothing is made.
    let (status, refused) = ws.call(
        "POST",
        "/projects",
        &[],
        Some(&json!({ "kind": "new", "slug": "lathe", "goal": goal })),
    );
    assert_eq!(status, 400, "{refused}");
    assert_eq!(slugs(&ws), vec!["bench"], "{refused}");

    // The opener stood in a goal — a goal heading's *Import into this
    // goal…*: born of it, attached as it is made.
    let vise = made_by_the_dialog(&ws, &format!("/goals/{goal}/projects"), "vise");
    assert_eq!(
        vise["project"]["origin"],
        json!({ "origin": "goal", "goal": goal }),
        "{vise}"
    );
    assert_eq!(vise["goals"], json!([goal]), "{vise}");
    assert_eq!(vise["attached_to"], goal.as_str(), "{vise}");
    let vise = id_of(&vise, "project");

    // Put away and taken back out through the node, as the rail's dialog
    // does: each is said, with which way it went.
    for archived in [true, false] {
        let (status, row) = ws.call(
            "POST",
            &format!("/projects/{bench}/archive"),
            &[],
            Some(&json!({ "archived": archived })),
        );
        assert_eq!(status, 200, "{row}");
        assert_eq!(
            row["project"]["project"]["archived"].is_object(),
            archived,
            "{row}"
        );
    }
    // From a terminal, with the window open: a goal that goes puts away
    // what was born of it, and a project forgotten is forgotten.
    ws.ok(&["rm", &goal, "--projects", "archive"]);
    ws.ok(&["project", "rm", &bench]);
    let said = ws.until("the bus to have said where each project went", || {
        let said = goings(&listening);
        (said.len() >= 4).then_some(said)
    });
    assert_eq!(
        said,
        vec![
            ("project_archived".to_string(), bench.clone(), Some(true)),
            ("project_archived".to_string(), bench.clone(), Some(false)),
            ("project_archived".to_string(), vise.clone(), Some(true)),
            ("project_deleted".to_string(), bench.clone(), None),
        ],
        "what a workbench standing on either hears, in order"
    );
    assert_eq!(slugs(&ws), Vec::<String>::new(), "none is listed any more");
    drop(listening);
    ws.stop();
}
