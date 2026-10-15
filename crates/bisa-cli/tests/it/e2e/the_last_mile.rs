//! The last mile of the command line, with no node: a data folder that is
//! a file, a workflow named by a slug nobody installed, a guided goal's
//! status before its proposal, and a catalog entry that brings nothing else
//! with it, installed twice.

use super::sealed::{Sealed, KEPT};

fn said_by(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn the_command_line_refuses_plainly_at_its_last_edges() {
    let ws = Sealed::bare();

    // A data folder that is a file cannot be opened as a workspace. The
    // binary is run by hand here, under a scratch home, since the rig
    // names its own data folder on every call.
    let file = ws.data().with_file_name("not-a-folder");
    std::fs::write(&file, "x").unwrap();
    let home = ws.data().with_file_name("home-for-the-refusal");
    std::fs::create_dir_all(&home).unwrap();
    let mut by_hand = std::process::Command::new(env!("CARGO_BIN_EXE_bisa"));
    by_hand.env_clear();
    for name in KEPT {
        if let Some(value) = std::env::var_os(name) {
            by_hand.env(name, value);
        }
    }
    let refused = by_hand
        .env("HOME", &home)
        .env("TMPDIR", &home)
        .env("PATH", "/usr/bin:/bin")
        .args(["--data-dir", file.to_str().unwrap(), "--file-keys", "inbox"])
        .output()
        .expect("the binary runs");
    assert!(!refused.status.success(), "{}", said_by(&refused));
    assert!(
        said_by(&refused).contains("workspace"),
        "{}",
        said_by(&refused)
    );

    // A slug that names no workflow and no template.
    let refused = ws.refused(&["workflow", "show", "no-such-slug"]);
    assert!(refused.contains("no-such-slug"), "{refused}");

    // A guided goal's status, before the Workflow Agent proposed anything.
    let made = ws.json(&["new", "think about the door", "--mode", "guided"]);
    let goal = made["goal"]["id"]
        .as_str()
        .or_else(|| made["id"].as_str())
        .or_else(|| made["goal"].as_str())
        .unwrap_or_else(|| panic!("the goal's id in {made}"))
        .to_string();
    let status = ws.ok(&["status", &goal]);
    assert!(
        said_by(&status).contains("proposes"),
        "{}",
        said_by(&status)
    );
    // A step of a goal that has no run yet is nobody's to mark.
    let refused = ws.refused(&["step", "done", &goal, "fit"]);
    assert!(refused.contains("run"), "{refused}");

    // A connector from the catalog brings nothing else with it; a workflow
    // template shown once its agents are installed says they are here.
    let alone = ws.ok(&["catalog", "show", "connector", "confluence"]);
    assert!(
        said_by(&alone).contains("nothing else"),
        "{}",
        said_by(&alone)
    );
    let listed = ws.json(&["catalog", "list"]);
    let template = listed["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|e| e["kind"] == "workflow")
        .and_then(|e| e["slug"].as_str())
        .unwrap_or_else(|| panic!("a workflow template in {listed}"))
        .to_string();
    ws.ok(&["catalog", "install", "workflow", &template]);
    let shown = ws.ok(&["catalog", "show", "workflow", &template]);
    assert!(
        said_by(&shown).contains("already here"),
        "{}",
        said_by(&shown)
    );
}
