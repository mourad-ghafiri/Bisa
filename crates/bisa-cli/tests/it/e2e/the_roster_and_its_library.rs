//! The roster and its library from the command line while a node runs: a
//! skill written, a server registered and dialled, an agent defined on both,
//! edited, stood down and up again, a team made of it; what is still used
//! refused by name when it is asked to go, and gone once nothing names it; a
//! team installed from the catalog with everything it brings.
//!
//! Every write is the node's. What stands an agent down is said once in the
//! room everybody is in, and an edit that is refused has stood nobody down.
//!
//! The server dialled is the platform's own — `bisa mcp`, the one a session
//! is handed — started by the node as any registered server is. Nothing
//! leaves the machine, and what goes is a record of the journey's own
//! workspace.

use super::sealed::{Sealed, AGENT_HARNESS};
use serde_json::{json, Value};

/// How many times the room everybody is in was told that somebody came or
/// went.
fn told(ws: &Sealed) -> usize {
    let (status, room) = ws.call("GET", "/channels/general/messages", &[], None);
    assert_eq!(status, 200, "{room}");
    room["messages"]
        .as_array()
        .unwrap_or_else(|| panic!("the room's messages: {room}"))
        .iter()
        .filter(|message| message["body_kind"] == "membership")
        .count()
}

/// Where the daemon's engine takes what a session's tools say: the one
/// socket of its own under the workspace's `run` folder.
fn the_intake(ws: &Sealed) -> String {
    let run = ws.data().join("run");
    let sockets: Vec<String> = std::fs::read_dir(&run)
        .unwrap_or_else(|e| panic!("reading {}: {e}", run.display()))
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("engine-") && name.ends_with(".sock"))
        .collect();
    assert_eq!(sockets.len(), 1, "one engine, one intake: {sockets:?}");
    run.join(&sockets[0]).to_string_lossy().into_owned()
}

fn ids(of: &Value) -> Vec<String> {
    of.as_array()
        .unwrap_or_else(|| panic!("a list: {of}"))
        .iter()
        .filter_map(|one| one.as_str().map(str::to_string))
        .collect()
}

fn agent(ws: &Sealed, id: &str) -> Value {
    ws.json(&["agent", "show", id])["agent"].clone()
}

#[test]
fn the_roster_and_its_library_are_written_by_the_node_that_runs() {
    let mut ws = Sealed::bare();
    ws.start();

    // --- a skill: written, edited, read back ------------------------------------
    let written = ws.json(&[
        "skill",
        "add",
        "--id",
        "survey-checklist",
        "--name",
        "Survey checklist",
        "--description",
        "When a site is looked over before any work begins.",
        "--markdown",
        "1. Walk the site.\n2. Write down what is there.\n",
    ]);
    assert_eq!(written["skill"]["id"], "survey-checklist", "{written}");
    let edited = ws.json(&[
        "skill",
        "edit",
        "survey-checklist",
        "--description",
        "When a site is looked over before work begins.",
    ]);
    assert_eq!(
        edited["skill"]["markdown"], written["skill"]["markdown"],
        "what an edit left alone is as it was: {edited}"
    );
    assert_eq!(
        ws.json(&["skill", "show", "survey-checklist"])["skill"]["description"],
        "When a site is looked over before work begins."
    );

    // --- a server: registered, dialled, its health the node's -------------------
    let goal = ws.json(&["new", "look the site over", "--mode", "manual"])["goal"]
        .as_str()
        .expect("the goal")
        .to_string();
    let intake = the_intake(&ws);
    let registered = ws.json(&[
        "mcp",
        "add",
        "--id",
        "platform",
        "--name",
        "platform-tools",
        "--description",
        "The platform's own tools, about one goal",
        "--command",
        env!("CARGO_BIN_EXE_bisa"),
        "--arg",
        "mcp",
        "--arg",
        "--socket",
        "--arg",
        &intake,
        "--arg",
        "--goal",
        "--arg",
        &goal,
    ]);
    assert_eq!(registered["mcp"]["id"], "platform", "{registered}");
    let before = ws.json(&["mcp", "list"]);
    assert_eq!(
        before["mcps"][0]["health"]["state"], "unknown",
        "nobody dialled it yet: {before}"
    );
    let dialled = ws.json(&["mcp", "probe", "platform"]);
    let report = &dialled["report"];
    assert_eq!(report["ok"], true, "{dialled}");
    assert_eq!(report["transport"], "stdio", "{dialled}");
    assert!(
        report["tool_count"].as_u64().unwrap_or_default() > 0,
        "the platform's server offers its tools: {dialled}"
    );
    let after = ws.json(&["mcp", "list"]);
    assert_eq!(after["mcps"][0]["health"]["state"], "ok", "{after}");
    assert_eq!(
        after["mcps"][0]["health"]["tool_count"], report["tool_count"],
        "what the probe answered is the server's health: {after}"
    );
    // A server that cannot start is a report too, and a health.
    ws.json(&[
        "mcp",
        "add",
        "--id",
        "nowhere",
        "--name",
        "nowhere",
        "--command",
        "/no/such/server",
    ]);
    let failed = ws.json(&["mcp", "probe", "nowhere"]);
    assert_eq!(failed["report"]["ok"], false, "{failed}");
    let listed = ws.json(&["mcp", "list"]);
    let nowhere = listed["mcps"]
        .as_array()
        .expect("the servers")
        .iter()
        .find(|server| server["id"] == "nowhere")
        .expect("the server that cannot start");
    assert_eq!(nowhere["health"]["state"], "failing", "{listed}");

    // --- an agent on both ------------------------------------------------------
    let made = ws.json(&[
        "agent",
        "add",
        "--name",
        "Scout",
        "--prompt",
        "You look a site over and say what is there.",
        "--harness",
        AGENT_HARNESS,
        "--description",
        "looks ahead",
        "--skill",
        "survey-checklist",
        "--mcp",
        "platform",
    ]);
    let scout = made["agent"]["id"].as_str().expect("the agent").to_string();
    assert_eq!(made["agent"]["enabled"], true, "{made}");
    assert_eq!(made["agent"]["skills"], json!(["survey-checklist"]));
    assert_eq!(made["agent"]["mcps"], json!(["platform"]));

    // --- an edit that is refused has stood nobody down ---------------------------
    let room = told(&ws);
    let words = ws.refused(&[
        "agent",
        "edit",
        &scout,
        "--enabled",
        "false",
        "--skill",
        "a-skill-nobody-wrote",
    ]);
    assert!(words.contains("a-skill-nobody-wrote"), "{words}");
    assert_eq!(agent(&ws, &scout)["enabled"], true);
    assert_eq!(
        told(&ws),
        room,
        "a refused edit told the room somebody left"
    );

    // --- stood down and edited in one write, said once ---------------------------
    let stood_down = ws.json(&[
        "agent",
        "edit",
        &scout,
        "--enabled",
        "false",
        "--name",
        "Scout the First",
    ]);
    assert_eq!(stood_down["agent"]["enabled"], false, "{stood_down}");
    assert_eq!(stood_down["agent"]["name"], "Scout the First");
    assert_eq!(
        stood_down["agent"]["description"], "looks ahead",
        "what an edit left alone is as it was"
    );
    assert_eq!(told(&ws), room + 1, "said once, where it happened");
    ws.json(&["agent", "edit", &scout, "--enabled", "false"]);
    assert_eq!(told(&ws), room + 1, "the same word again is no news");
    ws.json(&["agent", "edit", &scout, "--enabled", "true"]);
    assert_eq!(told(&ws), room + 2, "and back");

    // --- one reference at a time ---------------------------------------------------
    let without = ws.json(&["agent", "skill", "rm", &scout, "survey-checklist"]);
    assert_eq!(without["agent"]["skills"], json!([]), "{without}");
    let with = ws.json(&["agent", "skill", "add", &scout, "survey-checklist"]);
    assert_eq!(with["agent"]["skills"], json!(["survey-checklist"]));
    let again = ws.json(&["agent", "skill", "add", &scout, "survey-checklist"]);
    assert_eq!(
        again["agent"]["skills"],
        json!(["survey-checklist"]),
        "attached twice is attached"
    );
    let words = ws.refused(&["agent", "mcp", "add", &scout, "a-server-nobody-has"]);
    assert!(words.contains("a-server-nobody-has"), "{words}");
    // The servers an agent carries: listed, one taken off, put back.
    let carried = ws.json(&["agent", "mcp", "list", &scout]);
    assert_eq!(carried["mcps"], json!(["platform"]), "{carried}");
    let none = ws.json(&["agent", "mcp", "rm", &scout, "platform"]);
    assert_eq!(none["agent"]["mcps"], json!([]), "{none}");
    assert_eq!(
        ws.json(&["agent", "mcp", "list", &scout])["mcps"],
        json!([]),
        "taken off, it is carried no more"
    );
    let back = ws.json(&["agent", "mcp", "add", &scout, "platform"]);
    assert_eq!(back["agent"]["mcps"], json!(["platform"]), "{back}");

    // --- the agents that are always there --------------------------------------
    let words = ws.refused(&["agent", "edit", "general-agent", "--enabled", "false"]);
    assert!(!words.is_empty());
    assert_eq!(agent(&ws, "general-agent")["enabled"], true);
    ws.refused(&["agent", "rm", "general-agent"]);

    // --- a team of it ------------------------------------------------------------
    let mapper = ws.json(&[
        "agent",
        "add",
        "--name",
        "Mapper",
        "--prompt",
        "You draw what the scout saw.",
        "--harness",
        AGENT_HARNESS,
    ])["agent"]["id"]
        .as_str()
        .expect("the second agent")
        .to_string();
    let team = ws.json(&[
        "team",
        "create",
        "Survey",
        "--purpose",
        "looks a site over",
        "--agent",
        &scout,
    ]);
    let survey = team["team"]["id"].as_str().expect("the team").to_string();
    assert_eq!(team["team"]["members"], json!([{"agent": scout}]), "{team}");
    let joined = ws.json(&["team", "add-member", &survey, "--agent", &mapper]);
    assert_eq!(
        joined["team"]["members"],
        json!([{"agent": scout}, {"agent": mapper}]),
        "{joined}"
    );
    let words = ws.refused(&[
        "team",
        "add-member",
        &survey,
        "--agent",
        "an-agent-nobody-made",
    ]);
    assert!(words.contains("an-agent-nobody-made"), "{words}");
    let left = ws.json(&["team", "remove-member", &survey, "--agent", &mapper]);
    assert_eq!(left["team"]["members"], json!([{"agent": scout}]), "{left}");
    assert_eq!(left["team"]["purpose"], "looks a site over");

    // --- a server edited, stood down and back ------------------------------------
    let edited = ws.json(&[
        "mcp",
        "edit",
        "nowhere",
        "--description",
        "a server that cannot start",
    ]);
    assert_eq!(
        edited["mcp"]["description"], "a server that cannot start",
        "{edited}"
    );
    assert_eq!(edited["mcp"]["name"], "nowhere", "what an edit left alone");
    let off = ws.json(&["mcp", "disable", "nowhere"]);
    assert_eq!(off["mcp"]["enabled"], false, "{off}");
    let on = ws.json(&["mcp", "enable", "nowhere"]);
    assert_eq!(on["mcp"]["enabled"], true, "{on}");

    // --- asked before anything is removed: who points at what -----------------------
    let used = ws.json(&["skill", "usage", "survey-checklist"]);
    assert_eq!(used["kind"], "skill", "{used}");
    assert!(
        used["usage"].to_string().contains(scout.as_str()),
        "the agent that carries it: {used}"
    );
    let used = ws.json(&["mcp", "usage", "platform"]);
    assert!(
        used["usage"].to_string().contains(scout.as_str()),
        "the agent that carries it: {used}"
    );
    assert_eq!(
        ws.json(&["mcp", "usage", "nowhere"])["usage"],
        json!([]),
        "nothing points at it: it can be removed"
    );
    assert_eq!(
        ws.json(&["team", "usage", &survey])["usage"],
        json!([]),
        "nothing names the team"
    );

    // --- what is still used is refused by name, and nothing went -------------------
    let words = ws.refused(&["skill", "rm", "survey-checklist"]);
    assert!(words.contains("Scout the First"), "who carries it: {words}");
    let words = ws.refused(&["mcp", "rm", "platform"]);
    assert!(words.contains("Scout the First"), "who carries it: {words}");
    let words = ws.refused(&["agent", "rm", &scout]);
    assert!(words.contains("Survey"), "what names it: {words}");
    assert_eq!(agent(&ws, &scout)["id"], scout.as_str());
    assert_eq!(
        ws.json(&["skill", "show", "survey-checklist"])["skill"]["id"],
        "survey-checklist"
    );

    // --- a team from the catalog, with what it brings ------------------------------
    let shelf = ws.json(&["catalog", "list", "--kind", "team"]);
    let slug = shelf["entries"][0]["slug"]
        .as_str()
        .unwrap_or_else(|| panic!("the catalog has a team: {shelf}"))
        .to_string();
    let brought = ws.json(&["catalog", "install", "team", &slug]);
    let (agents, teams) = (
        ids(&brought["installed"]["agents"]),
        ids(&brought["installed"]["teams"]),
    );
    assert_eq!(teams.len(), 1, "{brought}");
    assert!(!agents.is_empty(), "a team brings its agents: {brought}");
    let roster = ws.json(&["agent", "list"]);
    for id in &agents {
        let one = roster["agents"]
            .as_array()
            .expect("the roster")
            .iter()
            .find(|agent| agent["id"] == id.as_str())
            .unwrap_or_else(|| panic!("{id} was installed and is not on the roster: {roster}"));
        assert_eq!(
            one["origin"]["catalog"]["slug"],
            id.as_str(),
            "where it came from is on its record: {}",
            one["origin"]
        );
    }
    let members = ws.json(&["team", "show", &teams[0]])["team"]["members"].clone();
    for id in &agents {
        assert!(
            members.to_string().contains(id.as_str()),
            "{id} came with the team and is no member of it: {members}"
        );
    }
    let twice = ws.json(&["catalog", "install", "team", &slug]);
    assert!(
        ids(&twice["installed"]["agents"]).is_empty()
            && ids(&twice["installed"]["teams"]).is_empty(),
        "what is here already is left as it is: {twice}"
    );

    // --- the same after the node went and came back -------------------------------
    let roster = ws.json(&["agent", "list"]);
    ws.stop();
    ws.start();
    assert_eq!(ws.json(&["agent", "list"]), roster);
    assert_eq!(
        ws.json(&["mcp", "list"])["mcps"][0]["health"]["state"],
        "unknown",
        "a health is what this node saw, and this one dialled nothing yet"
    );

    // --- gone once nothing names it, in the order that lets each go ---------------
    let gone = ws.json(&["team", "rm", &survey]);
    assert_eq!(gone["removed"], survey.as_str());
    ws.json(&["agent", "rm", &scout]);
    ws.json(&["agent", "rm", &mapper]);
    ws.json(&["skill", "rm", "survey-checklist"]);
    ws.json(&["mcp", "rm", "platform"]);
    ws.json(&["mcp", "rm", "nowhere"]);
    let roster = ws.json(&["agent", "list"]);
    assert!(
        !roster.to_string().contains(&scout),
        "an agent removed is on no roster: {roster}"
    );
    assert_eq!(ws.json(&["mcp", "list"])["mcps"], json!([]));
    ws.refused(&["skill", "show", "survey-checklist"]);
    ws.stop();
}
