//! A branch going out from the command line while a node runs: the verbs
//! that publish — `workstream push`, and the pull request's — asked of the
//! node, each under the project's own word on publishing. By itself, the
//! push is done; by hand, it is refused by name; asked first, the gate is
//! the goal's and is decided from the command line, and what was approved
//! and then did not go out is **said** — to the person at the terminal, and
//! on the workstream's row of the Inbox.
//!
//! The one `origin` is another project's tree on this disk, or a folder that
//! is no repository at all: nothing leaves the machine, and no code host is
//! reached — a checkout whose origin is a folder has none, which is what the
//! pull request's verbs answer.

use super::sealed::Sealed;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Output;

const COMMITTER: &str = "A Journey <journey@example.test>";

/// A project that is a repository with one commit: its id and its tree.
fn the_shelf(ws: &Sealed) -> (String, PathBuf) {
    let made = ws.json(&["project", "new", "shelf", "--committer", COMMITTER]);
    let project = made["project"]["id"]
        .as_str()
        .expect("the project")
        .to_string();
    let tree = PathBuf::from(made["path"].as_str().expect("its tree"));
    std::fs::write(tree.join("plan.md"), "measure twice\n").expect("the plan");
    ws.ok(&[
        "project", "commit", &project, "-m", "the plan", "--path", "plan.md",
    ]);
    (project, tree)
}

/// A clone of the shelf — its origin the shelf's own tree — publishing as
/// `publish` says, with a workstream on a branch of its own that holds one
/// commit. The clone's id, the workstream's.
fn a_branch(
    ws: &Sealed,
    origin: &Path,
    slug: &str,
    publish: &str,
    goal: Option<&str>,
) -> (String, String) {
    let cloned = ws.json(&[
        "project",
        "clone",
        &origin.to_string_lossy(),
        "--slug",
        slug,
        "--publish",
        publish,
        "--committer",
        COMMITTER,
    ]);
    let project = cloned["project"]["id"]
        .as_str()
        .expect("the clone")
        .to_string();
    let branch = format!("new:feature/{slug}");
    let mut open = vec![
        "workstream",
        "open",
        project.as_str(),
        "--from",
        branch.as_str(),
    ];
    if let Some(goal) = goal {
        ws.ok(&["project", "attach", &project, goal]);
        open.extend(["--goal", goal]);
    }
    let opened = ws.json(&open);
    let workstream = opened["workstream"]["id"]
        .as_str()
        .expect("the workstream")
        .to_string();
    let checkout = PathBuf::from(opened["path"].as_str().expect("its checkout"));
    std::fs::write(checkout.join(format!("{slug}.md")), "one more board\n").expect("a file");
    ws.ok(&["workstream", "commit", &workstream, "-m", slug]);
    (project, workstream)
}

/// The branches at the shelf, as the node reads them.
fn at_the_origin(ws: &Sealed, shelf: &str) -> String {
    let (status, branches) = ws.call(
        "GET",
        &format!("/workstreams/{shelf}/git/branches"),
        &[],
        None,
    );
    assert_eq!(status, 200, "{branches}");
    branches.to_string()
}

fn state_of(ws: &Sealed, workstream: &str) -> String {
    ws.json(&["workstream", "show", workstream])["workstream"]["state"]["state"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

fn said(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn a_branch_goes_out_from_the_command_line_as_its_project_says() {
    let mut ws = Sealed::bare();
    ws.start();
    let (shelf, tree) = the_shelf(&ws);
    let goal = ws.json(&["new", "hang the shelf", "--mode", "manual"])["goal"]
        .as_str()
        .expect("the goal")
        .to_string();

    // --- by itself: pushed, and the record says so -------------------------------
    let (_, brackets) = a_branch(&ws, &tree, "brackets", "auto", None);
    let pushed = ws.json(&["workstream", "push", &brackets]);
    assert_eq!(pushed["pushed"], true, "{pushed}");
    assert_eq!(pushed["workstream"]["state"]["state"], "pushed", "{pushed}");
    assert!(at_the_origin(&ws, &shelf).contains("feature/brackets"));

    // --- by hand: refused by name, nothing out ----------------------------------
    let (_, paint) = a_branch(&ws, &tree, "paint", "manual", None);
    let words = ws.refused(&["workstream", "push", &paint]);
    assert!(words.contains("manual"), "the policy, by name: {words}");
    assert_eq!(state_of(&ws, &paint), "committed");
    assert!(!at_the_origin(&ws, &shelf).contains("feature/paint"));

    // --- asked first, with nobody to ask: refused in its own words ---------------
    let (_, nobody) = a_branch(&ws, &tree, "nobody", "gated", None);
    let words = ws.refused(&["workstream", "push", &nobody, "--yes"]);
    assert!(
        words.contains("goal"),
        "a gate needs a goal to be asked on: {words}"
    );

    // --- asked first: nothing out until somebody says yes -----------------------
    let (_, screws) = a_branch(&ws, &tree, "screws", "gated", Some(&goal));
    let waiting = ws.bisa(&["--json", "workstream", "push", &screws]);
    assert!(waiting.status.success(), "{}", said(&waiting));
    assert!(
        said(&waiting).contains("no terminal"),
        "no terminal, no yes: {}",
        said(&waiting)
    );
    assert!(!at_the_origin(&ws, &shelf).contains("feature/screws"));
    assert_eq!(state_of(&ws, &screws), "committed");
    // The gate is the goal's, and deciding it there lets the push go.
    ws.ok(&["approve", &goal]);
    ws.until("the branch to be at its origin", || {
        (state_of(&ws, &screws) == "pushed").then_some(())
    });
    assert!(at_the_origin(&ws, &shelf).contains("feature/screws"));

    // --- asked first, and yes said with the verb: the verb waits for the end -----
    let (_, plugs) = a_branch(&ws, &tree, "plugs", "gated", Some(&goal));
    let pushed = ws.json(&["workstream", "push", &plugs, "--yes"]);
    assert_eq!(pushed["pushed"], true, "{pushed}");
    assert_eq!(
        pushed["workstream"]["state"]["state"], "pushed",
        "the record as it stands once the push landed: {pushed}"
    );
    assert!(at_the_origin(&ws, &shelf).contains("feature/plugs"));

    // --- approved, and it did not go out: said, here and in the Inbox ------------
    let (hooks_project, hooks) = a_branch(&ws, &tree, "hooks", "gated", Some(&goal));
    let nowhere = ws.data().join("nowhere.git");
    let (status, moved) = ws.call(
        "PUT",
        &format!("/workstreams/{hooks_project}/git/remotes/origin"),
        &[],
        Some(&json!({"url": nowhere.to_string_lossy()})),
    );
    assert_eq!(status, 200, "{moved}");
    let words = ws.refused(&["workstream", "push", &hooks, "--yes"]);
    assert!(
        words.contains("approved, and it did not go out") && words.contains("push feature/hooks"),
        "{words}"
    );
    assert_eq!(
        state_of(&ws, &hooks),
        "committed",
        "the record still offers the push"
    );
    let row = ws.until("the notice on the workstream's row", || {
        ws.json(&["inbox"])["rows"]
            .as_array()?
            .iter()
            .find(|row| row["key"] == hooks.as_str())
            .cloned()
    });
    let notices: Vec<&str> = row["notices"]
        .as_array()
        .expect("the notices")
        .iter()
        .filter_map(|n| n["notice"].as_str())
        .collect();
    assert_eq!(notices, ["publish_failed"], "{row}");
    assert_eq!(row["notices"][0]["event"]["what"], "push feature/hooks");

    // --- a pull request, where no code host is: answered, and refused in words ---
    let none = ws.json(&["workstream", "pr-view", &plugs]);
    assert_eq!(none["pr"], Value::Null, "{none}");
    let checks = ws.refused(&["workstream", "pr-checks", &plugs]);
    assert!(!checks.is_empty());
    let (_, shims) = a_branch(&ws, &tree, "shims", "auto", None);
    let words = ws.refused(&[
        "workstream",
        "pr",
        &shims,
        "--title",
        "Shims under the shelf",
    ]);
    assert!(!words.is_empty());
    assert!(
        at_the_origin(&ws, &shelf).contains("feature/shims"),
        "opening pushes first: the branch is at its origin whatever the code host said"
    );
    ws.refused(&["workstream", "pr-review", &shims, "--event", "approve"]);
    ws.refused(&["workstream", "pr-merge", &shims, "--yes"]);
    ws.stop();
}
