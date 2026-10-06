//! One node per workspace. With a daemon up, a second `bisa node` over the
//! same workspace is refused at the door — in its own words, naming the
//! holder's pid and where the node answers — before it opens the workspace
//! or binds a socket of its own; the first daemon runs on untouched, and
//! still answers.

use super::sealed::Sealed;
use std::time::{Duration, Instant};

#[test]
fn a_second_node_on_a_held_workspace_is_refused_at_the_door_naming_the_holder() {
    let mut ws = Sealed::bare();
    ws.start();
    let pid = ws.pid();

    let asked = Instant::now();
    let said = ws.refused(&["node"]);
    assert!(
        asked.elapsed() < Duration::from_secs(15),
        "refused at the door, never after a wait on a health check: {said}"
    );
    assert!(
        said.contains("an engine already holds this workspace")
            && said.contains(&format!("pid {pid}")),
        "the refusal names the holder: {said}"
    );
    assert!(
        said.contains("answering on") && said.contains(&ws.socket().display().to_string()),
        "and where the node answers: {said}"
    );

    // The refused start bound nothing: every intake socket under run/ is
    // the first daemon's own.
    let run = bisa_store::Paths::new(ws.data()).run_dir();
    let intakes: Vec<String> = std::fs::read_dir(&run)
        .expect("the run folder")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("engine-") && n.ends_with(".sock"))
        .collect();
    assert!(
        !intakes.is_empty() && intakes.iter().all(|n| n == &format!("engine-{pid}.sock")),
        "only the holder's intake socket stands: {intakes:?}"
    );

    // The first daemon is untouched and still the one that answers.
    let (status, node) = ws.call("GET", "/node", &[], None);
    assert_eq!(status, 200, "{node}");
    assert_eq!(node["pid"], serde_json::json!(pid), "{node}");
    ws.stop();
}
