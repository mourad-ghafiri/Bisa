//! The node lives and dies with the desktop that started it. The shell
//! starts its node leashed — the node's stdin a pipe the shell holds, under
//! `BISA_STOP_ON_STDIN_CLOSE=1` — so when the shell goes, however it goes,
//! the pipe closes and the node stops as gracefully as on SIGTERM: it ends
//! well, leaves no socket, and lets go of the workspace, which `bisa paths
//! --json` names the holder of without a node. Before this a Force Quit of
//! the desktop left the node alive holding the lock, and every later launch
//! was refused until the machine was restarted or the folder deleted. The
//! boot says its phases first, as JSON lines the shell relays to a person.

use super::sealed::Sealed;
use serde_json::json;

#[test]
fn a_leashed_node_stops_when_its_parent_lets_go_and_the_workspace_is_free_again() {
    let mut ws = Sealed::bare();
    ws.start_leashed();
    let pid = ws.pid();
    let (status, node) = ws.call("GET", "/node", &[], None);
    assert_eq!(status, 200, "{node}");

    // Without a node of its own, `paths` names who holds the workspace.
    let paths = ws.json(&["paths"]);
    assert_eq!(paths["engine_holder"]["pid"], json!(pid), "{paths}");
    assert!(
        paths["engine_holder"]["started_at"]
            .as_u64()
            .is_some_and(|t| t > 0),
        "{paths}"
    );

    // The boot said its phases before it served, each a JSON line.
    let out = ws.daemon_out();
    let lines: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).collect();
    let phase = |l: &&str| l.contains("\"boot\"");
    let opening = lines
        .iter()
        .position(|l| phase(l) && l.contains("opening_workspace"));
    let engine = lines
        .iter()
        .position(|l| phase(l) && l.contains("starting_engine"));
    let serving = lines.iter().position(|l| l.contains("\"socket\""));
    assert!(
        matches!((opening, engine, serving), (Some(o), Some(e), Some(s)) if o < e && e < s),
        "the phases come in order, then the socket: {out}"
    );

    // The shell goes: the pipe closes, the node stops on its own.
    ws.drop_leash();
    let paths = ws.json(&["paths"]);
    assert!(
        paths["engine_holder"].is_null(),
        "the workspace is free: {paths}"
    );

    // And a node starts again at once, where before it was refused.
    ws.start();
    let (status, _) = ws.call("GET", "/node", &[], None);
    assert_eq!(status, 200);
    ws.stop();
}
