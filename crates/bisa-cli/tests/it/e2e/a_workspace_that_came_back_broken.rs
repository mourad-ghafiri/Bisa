//! The daemon ends abruptly and the workspace it leaves is torn: the member
//! file is not JSON any more, the `general` channel's snapshot is garbage,
//! a journal's last line was cut mid-write. The next start opens all the
//! same — the torn files are moved under `quarantine/` and made again, the
//! torn line is skipped — and `bisa status` answers; `bisa workspace check`
//! lists what was moved and what is torn, and exits 1; `bisa workspace
//! reindex`, with the node stopped, prints its stages and rebuilds. Before
//! this the node refused the workspace at the door over any one of those
//! files, and a person's way back was deleting the folder.

use super::sealed::Sealed;
use bisa_store::Paths;
use std::io::Write;

/// Every file under `dir` with the extension, depth first.
fn files_with(dir: &std::path::Path, ext: &str) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(files_with(&path, ext));
        } else if path.extension().is_some_and(|e| e == ext) {
            out.push(path);
        }
    }
    out.sort();
    out
}

#[test]
fn a_workspace_torn_by_a_crash_opens_again_and_the_check_names_what_was_moved() {
    let mut ws = Sealed::bare();
    ws.start();
    let goal = ws.json(&["new", "keep a thing", "--mode", "manual"])["goal"]
        .as_str()
        .expect("the goal")
        .to_string();
    ws.crash();

    // The crash's leavings: two files that are not what they should be, and
    // a journal whose last line was never finished.
    let paths = Paths::new(ws.data());
    std::fs::write(paths.members_file(), "{not json").unwrap();
    let general = paths
        .state_dir(Paths::NS_CHANNELS)
        .join(format!("{}-general.json", bisa_core::kind::KIND_CHANNEL));
    assert!(general.is_file(), "{}", general.display());
    std::fs::write(&general, "garbage").unwrap();
    let journal = files_with(&ws.data(), "jsonl")
        .into_iter()
        .find(|p| p.starts_with(paths.goals_dir()))
        .expect("the goal's journal");
    std::fs::OpenOptions::new()
        .append(true)
        .open(&journal)
        .unwrap()
        .write_all(b"{\"type\":\"torn")
        .unwrap();

    // The check, before any open: what is torn, named, and exit 1.
    let before = ws.bisa(&["workspace", "check"]);
    assert_eq!(
        before.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&before.stderr)
    );
    let said = String::from_utf8_lossy(&before.stdout);
    assert!(said.contains("members.json"), "{said}");
    assert!(said.contains("-general.json"), "{said}");
    assert!(said.contains("a line a crash tore"), "{said}");

    // The node starts all the same, and the goal is where it was.
    ws.start();
    assert_eq!(ws.json(&["status", &goal])["goal"]["id"], goal.as_str());
    let (status, info) = ws.call("GET", "/workspace", &[], None);
    assert_eq!(status, 200, "{info}");
    let problems = info["problems"].as_array().expect("a list");
    assert!(
        problems.iter().filter(|p| p["kind"] == "recreated").count() >= 2,
        "the member file and the channel, made again: {problems:?}"
    );
    let (status, channels) = ws.call("GET", "/channels", &[], None);
    assert_eq!(status, 200, "{channels}");

    // The check beside the running node — it takes no lock — now lists the
    // quarantine, and still exits 1 until a person deals with it.
    let after = ws.bisa(&["--json", "workspace", "check"]);
    assert_eq!(after.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&after.stdout).expect("json");
    let kinds: Vec<&str> = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|f| f["kind"].as_str())
        .collect();
    assert!(
        kinds.contains(&"quarantined") && kinds.contains(&"torn_tail"),
        "{report}"
    );
    assert!(!kinds.contains(&"owner_key_unreadable"), "{report}");

    // The doctor says to look, in one line, and its own verdict stands.
    let doctor = ws.bisa(&["doctor"]);
    assert!(
        String::from_utf8_lossy(&doctor.stdout).contains("bisa workspace check"),
        "{}",
        String::from_utf8_lossy(&doctor.stdout)
    );

    // With the node stopped, the rebuild prints its stages and problems.
    ws.stop();
    let rebuilt = ws.json(&["workspace", "reindex"]);
    assert_eq!(rebuilt["rebuilt"], true, "{rebuilt}");
    ws.start();
    assert_eq!(ws.json(&["status", &goal])["goal"]["id"], goal.as_str());
    ws.stop();
}
