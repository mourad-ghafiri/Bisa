//! The verbs no other journey runs, through the binary **while a node
//! runs** — as one does whenever the desktop is open, which is when a verb
//! that starts an engine of its own is refused for a lock somebody else
//! holds. A goal's design asked for again after it stalled, amended while it
//! waits, moved on by hand and kept for the library; a session stopped from
//! the roster; what an agent remembered, read back by its owner; a commit
//! message suggested; a provider's key kept and forgotten; a peer entered by
//! hand; the index rebuilt when no node holds it; and the git setup — the
//! code hosts' health and sign-in, the accounts, the profiles, what a
//! checkout will use — read and written through the node.
//!
//! Nothing here reaches a code host, a relay or an SSH directory: the git
//! setup writes a global git config that is a file of the journey's own, and
//! the verbs that would talk to a real host or read the person's keys are
//! run only as far as their own refusal.

use super::sealed::{Sealed, AGENT_HARNESS};
use serde_json::{json, Value};

const COMMITTER: &str = "A Journey <journey@example.test>";

/// Everything a verb said, on either stream.
fn said_by(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Two steps a person does by hand.
fn by_hand(second: (&str, &str)) -> Value {
    json!({
        "name": "Hang the door",
        "description": "fit it, then finish it",
        "steps": [
            { "id": "fit", "name": "Fit the door", "kind": "human",
              "prompt": "Fit the door in its frame.", "then": [second.0] },
            { "id": second.0, "name": second.1, "kind": "human",
              "prompt": format!("{}.", second.1) },
        ],
    })
}

/// A designer that thinks aloud and proposes nothing the first time it is
/// woken, and proposes the two steps the next.
fn a_designer_that_stalls_once() -> Value {
    json!({ "turns": [
        { "scope": "goal", "times": 1, "say": ["Let me think about the door."] },
        { "scope": "goal",
          "tools": [{ "name": "propose_workflow",
                      "arguments": { "workflow": by_hand(("look", "Look it over")) } }],
          "say": ["I proposed two steps to do by hand."] },
    ]})
}

/// Where the Workflow Agent stands on a goal, as the node says it.
fn design_of(ws: &Sealed, goal: &str) -> String {
    let (status, view) = ws.call("GET", &format!("/goals/{goal}"), &[], None);
    assert_eq!(status, 200, "{view}");
    view["guidance"]["design"]["status"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

#[test]
fn a_design_is_asked_for_again_amended_moved_on_by_hand_and_kept_for_the_library() {
    let mut ws = Sealed::with_script(&a_designer_that_stalls_once());
    ws.start();

    // The harness the journey's agents run on, as the machine finds it.
    let probed = ws.json(&["harness", "probe", AGENT_HARNESS]);
    assert_eq!(probed["harness"]["id"], AGENT_HARNESS, "{probed}");
    assert_eq!(probed["harness"]["probe"]["available"], true, "{probed}");
    ws.refused(&["harness", "probe", "a-harness-nobody-has"]);

    // What a session can rely on, by its kind: the tools, and the framing.
    let context = ws.json(&["agent-context"]);
    let tools = |kind: &str| -> Vec<&str> {
        context["tools"][kind]
            .as_array()
            .unwrap_or_else(|| panic!("the {kind} tools: {context}"))
            .iter()
            .filter_map(|tool| tool["name"].as_str().or(tool.as_str()))
            .collect()
    };
    assert!(tools("work_item").contains(&"yield_result"), "{context}");
    assert!(tools("goal").contains(&"propose_workflow"), "{context}");
    assert!(tools("conversation").contains(&"note_append"), "{context}");
    assert!(
        context["chat_framing"]
            .as_str()
            .is_some_and(|framing| !framing.is_empty()),
        "{context}"
    );

    // --- a design that stalled is asked for again -----------------------------------
    let goal = ws.json(&["new", "hang the door"])["goal"]
        .as_str()
        .expect("the goal")
        .to_string();
    ws.until("the design to stall", || {
        (design_of(&ws, &goal) == "stalled").then_some(())
    });
    assert_eq!(ws.json(&["status", &goal])["status"], "draft");
    let asked = ws.json(&["design", &goal]);
    assert!(
        asked["guidance"]["design"].is_object(),
        "where the Workflow Agent stands now: {asked}"
    );
    let waiting = ws.until("the designed run to wait at its first step", || {
        let status = ws.json(&["status", &goal]);
        (status["status"] == "waiting").then_some(status)
    });
    let design = waiting["run"]["workflow"]["id"]
        .as_str()
        .expect("the goal's own design")
        .to_string();
    assert_eq!(
        waiting["run"]["steps"]["fit"]["state"]["state"], "waiting",
        "{waiting}"
    );
    ws.refused(&["design", "not-a-goal"]);

    // --- amended while it waits: what has not begun is replaced ---------------------
    let amended = ws.file(
        "amended.json",
        &by_hand(("oil", "Oil the hinges")).to_string(),
    );
    let after = ws.json(&["amend", &goal, "--from", &amended.to_string_lossy()]);
    let steps = &after["run"]["steps"];
    assert!(
        steps.get("oil").is_some(),
        "the new step is the run's: {after}"
    );
    assert!(
        steps.get("look").is_none(),
        "the one it replaced is gone: {after}"
    );
    assert_eq!(
        steps["fit"]["state"]["state"], "waiting",
        "what had begun is as it was: {after}"
    );
    let nothing = ws.file("not-a-workflow.json", "{ \"name\": ");
    ws.refused(&["amend", &goal, "--from", &nothing.to_string_lossy()]);

    // --- moved on by hand ---------------------------------------------------------------
    ws.ok(&["step", "done", &goal, "fit"]);
    let at_oil = ws.json(&["status", &goal]);
    assert_eq!(at_oil["status"], "waiting", "{at_oil}");
    assert_eq!(at_oil["run"]["steps"]["fit"]["state"]["state"], "done");
    assert_eq!(at_oil["run"]["steps"]["oil"]["state"]["state"], "waiting");
    ws.refused(&["step", "done", &goal, "a-step-nobody-wrote"]);
    ws.ok(&["step", "done", &goal, "oil"]);
    assert_eq!(ws.json(&["status", &goal])["status"], "done");
    ws.refused(&["step", "done", &goal, "oil"]);

    // --- the goal's design, kept for the library -------------------------------------
    let kept = ws.json(&["workflow", "promote", &design]);
    let copy = kept["workflow"]["id"]
        .as_str()
        .expect("the library's copy")
        .to_string();
    assert_ne!(copy, design, "a copy of its own: {kept}");
    assert_eq!(
        kept["workflow"]["origin"]["origin"], "workspace",
        "the library's, no goal's: {kept}"
    );
    let library = ws.json(&["workflow", "list"]);
    assert!(
        library["workflows"]
            .as_array()
            .is_some_and(|all| all.iter().any(|w| w["workflow"]["id"] == copy.as_str())),
        "{library}"
    );
    // What is the library's already is not promoted again.
    ws.refused(&["workflow", "promote", &copy]);
    ws.stop();
}

/// An agent that holds a turn open, remembers a thing when asked to, and
/// writes a commit's first line.
fn an_agent_that_holds_remembers_and_describes() -> Value {
    json!({ "turns": [
        { "scope": "conversation", "when": "wait here", "say": ["Waiting."], "hold": true },
        { "scope": "conversation", "when": "remember", "times": 1,
          "tools": [{ "name": "recall_store",
                      "arguments": { "slug": "door/hinges", "value": "Three hinges, brass." } }],
          "say": ["Remembered."] },
        { "scope": "none", "when": "plan.md", "say": ["Add the plan for the door"] },
    ]})
}

#[test]
fn a_session_is_stopped_a_memory_is_read_back_and_a_commit_message_is_suggested() {
    let mut ws = Sealed::with_script(&an_agent_that_holds_remembers_and_describes());
    ws.start();
    let post = |words: &str| -> String {
        let (status, made) = ws.call(
            "POST",
            "/conversations",
            &[],
            Some(&json!({ "origin": { "kind": "workspace" } })),
        );
        assert_eq!(status, 201, "{made}");
        let id = made["conversation"]["id"]
            .as_str()
            .expect("the conversation")
            .to_string();
        let (status, said) = ws.call(
            "POST",
            &format!("/conversations/{id}/messages"),
            &[],
            Some(&json!({ "content": words })),
        );
        assert_eq!(status, 200, "{said}");
        id
    };

    // --- a memory an agent kept, read back by its owner -------------------------------
    assert_eq!(
        ws.json(&["recall", "list", "general-agent"])["records"],
        json!([]),
        "nothing remembered yet"
    );
    let talk = post("remember the hinges");
    ws.until("the agent to have remembered", || {
        ws.json(&["recall", "list", "general-agent"])["records"]
            .as_array()
            .is_some_and(|all| all.iter().any(|r| r["slug"] == "door/hinges"))
            .then_some(())
    });
    let read = ws.json(&["recall", "get", "general-agent", "door/hinges"]);
    assert_eq!(read["record"]["value"], "Three hinges, brass.", "{read}");
    ws.refused(&["recall", "get", "general-agent", "door/handles"]);
    ws.until("that turn to be over", || {
        let (_, room) = ws.call("GET", &format!("/conversations/{talk}/messages"), &[], None);
        room["messages"]
            .as_array()?
            .iter()
            .any(|m| m["content"] == "Remembered.")
            .then_some(())
    });

    // --- a session stopped from the roster ---------------------------------------------
    // The roster says *aborted* — and the harness behind the row is let go
    // of: its process ends, which is what the agent records as its end. A
    // row that only read so, beside an agent still holding its turn, is
    // what this waits out.
    let ends_before = ws.recorded("ended").len();
    let held = post("wait here");
    ws.until("the turn to hold", || {
        (!ws.recorded("holding").is_empty()).then_some(())
    });
    let row_of = |id: &str| -> Option<Value> {
        ws.json(&["sessions", "list"])["sessions"]
            .as_array()?
            .iter()
            .find(|row| row["id"] == id)
            .cloned()
    };
    let live = ws.until("the held session on the roster", || {
        ws.json(&["sessions", "list"])["sessions"]
            .as_array()?
            .iter()
            .find(|row| row["conversation"] == held.as_str())
            .and_then(|row| row["id"].as_str().map(str::to_string))
    });
    let before = row_of(&live).expect("its row");
    assert_eq!(before["kind"], "conversation", "{before}");
    assert!(
        !matches!(
            before["state"]["state"].as_str(),
            Some("done" | "failed" | "aborted")
        ),
        "a turn that holds is live: {before}"
    );
    let listed = said_by(&ws.bisa(&["sessions", "list"]));
    let line = listed
        .lines()
        .find(|line| line.starts_with(live.as_str()))
        .unwrap_or_else(|| panic!("the session's line: {listed}"));
    assert!(
        line.contains("conversation") && line.ends_with(held.as_str()),
        "its state, its kind and what it is a turn of: {line}"
    );
    let stopped = ws.json(&["sessions", "abort", &live]);
    assert_eq!(stopped["aborted"], live.as_str(), "{stopped}");
    ws.until("the agent's process to have ended", || {
        (ws.recorded("ended").len() > ends_before).then_some(())
    });
    let after = row_of(&live).expect("an ended row stays for a while");
    assert_eq!(after["state"]["state"], "aborted", "{after}");
    // Stopped twice is stopped; a row nobody has is said, and so is an id
    // that is none.
    ws.ok(&["sessions", "abort", &live]);
    ws.refused(&["sessions", "abort", "01J000000000000000000000SE"]);
    ws.refused(&["sessions", "abort", "not-a-session"]);
    // The conversation is not over with the turn: the next message is
    // answered by a session of its own.
    let (status, said) = ws.call(
        "POST",
        &format!("/conversations/{held}/messages"),
        &[],
        Some(&json!({ "content": "remember the hinges again" })),
    );
    assert_eq!(status, 200, "{said}");
    ws.until("a fresh session on the conversation", || {
        ws.json(&["sessions", "list"])["sessions"]
            .as_array()?
            .iter()
            .any(|row| row["conversation"] == held.as_str() && row["id"] != live.as_str())
            .then_some(())
    });

    // --- a commit's first line, written by an agent from what is staged ---------------
    let made = ws.json(&["project", "new", "door", "--committer", COMMITTER]);
    let project = made["project"]["id"]
        .as_str()
        .expect("the project")
        .to_string();
    let tree = std::path::PathBuf::from(made["path"].as_str().expect("its tree"));
    let nothing_staged = ws.json(&["project", "message", &project]);
    assert_eq!(nothing_staged["suggested"], false, "{nothing_staged}");
    assert!(
        nothing_staged["error"]
            .as_str()
            .is_some_and(|why| why.contains("staged")),
        "it says why, and writes no message of its own: {nothing_staged}"
    );
    assert_eq!(nothing_staged["message"], "");
    std::fs::write(tree.join("plan.md"), "measure twice\n").expect("a change");
    ws.ok(&["project", "stage", &project, "plan.md"]);
    let suggested = ws.json(&["project", "message", &project]);
    assert_eq!(suggested["suggested"], true, "{suggested}");
    assert_eq!(
        suggested["message"], "Add the plan for the door",
        "{suggested}"
    );
    ws.stop();
}

#[test]
fn a_key_is_kept_and_forgotten_a_peer_is_entered_by_hand_and_the_index_is_rebuilt_with_no_node() {
    const KEY: &str = "jev-journey-key-0000-not-a-real-one";
    let mut ws = Sealed::bare();
    ws.start();

    // --- a remote provider's key: kept by this machine, never read back ---------------
    let file = ws.file("jev.key", &format!("{KEY}\n"));
    let from = format!("@{}", file.display());
    let kept = ws.json(&["decisions", "key", "set", "jev", "--from", &from]);
    assert_eq!(kept, json!({ "key_stored": true }));
    let status = ws.json(&["decisions", "status"]).to_string();
    assert!(
        !status.contains(KEY),
        "the status never says the key: {status}"
    );
    let (_, settings) = ws.call("GET", "/settings", &[], None);
    assert!(!settings.to_string().contains(KEY), "a key is no setting");
    let forgotten = ws.json(&["decisions", "key", "clear", "jev"]);
    assert_eq!(forgotten, json!({ "key_stored": false }));
    assert_eq!(
        ws.json(&["decisions", "key", "clear", "jev"]),
        json!({ "key_stored": false }),
        "forgotten twice is forgotten"
    );
    ws.refused(&[
        "decisions",
        "key",
        "set",
        "a-provider-nobody-is",
        "--from",
        &from,
    ]);
    ws.refused(&["decisions", "key", "clear", "a-provider-nobody-is"]);
    // A key is read from a file or from the input, never from the command
    // line, where a shell would keep it; and an empty one is no key.
    let said = ws.refused(&["decisions", "key", "set", "jev", "--from", KEY]);
    assert!(
        !said.contains(KEY),
        "the refusal does not repeat it: {said}"
    );
    let empty = ws.file("empty.key", "\n");
    ws.refused(&[
        "decisions",
        "key",
        "set",
        "jev",
        "--from",
        &format!("@{}", empty.display()),
    ]);

    // --- who may decide a gate: set through the node that reads it ---------------------
    let before = ws.json(&["governance", "show"]);
    assert_eq!(
        before["governance"]["publish"]["policy"], "owner",
        "{before}"
    );
    let set = ws.json(&["governance", "set", "publish", "members"]);
    assert_eq!(set["governance"]["publish"]["policy"], "members", "{set}");
    let (status, read) = ws.call("GET", "/governance", &[], None);
    assert_eq!(status, 200, "{read}");
    assert_eq!(
        read["governance"]["publish"]["policy"], "members",
        "the node says what the verb set: {read}"
    );
    assert_eq!(
        read["governance"]["approval"], before["governance"]["approval"],
        "a gate nobody named is as it was"
    );
    ws.refused(&["governance", "set", "a-gate-nobody-has", "owner"]);

    // --- a direct peer, entered by hand -------------------------------------------------
    let member = "ab".repeat(32);
    let added = ws.json(&["relay", "peer-add", &member, "node-one", "127.0.0.1:4433"]);
    assert_eq!(added["member"], member.as_str(), "{added}");
    let again = ws.json(&["relay", "peer-add", &member, "node-two", "127.0.0.1:4434"]);
    assert_eq!(again["node_id"], "node-two", "{again}");
    let peers = ws.json(&["settings", "get", "sync.iroh.peers"]);
    let listed = peers.to_string();
    assert!(
        listed.contains("node-two") && !listed.contains("node-one"),
        "one peer a member, the latest said: {peers}"
    );
    ws.refused(&["relay", "peer-add", "not-a-key", "node", "127.0.0.1:1"]);

    // --- the index, rebuilt from the files --------------------------------------------
    let goal = ws.json(&["new", "keep a thing", "--mode", "manual"])["goal"]
        .as_str()
        .expect("the goal")
        .to_string();
    let while_it_runs = ws.refused(&["workspace", "reindex"]);
    assert!(
        while_it_runs.contains("stop"),
        "a node holds the workspace, and the verb says to stop it first: {while_it_runs}"
    );
    ws.stop();
    let rebuilt = ws.json(&["workspace", "reindex"]);
    assert_eq!(rebuilt["rebuilt"], true, "{rebuilt}");
    assert_eq!(
        ws.json(&["status", &goal])["goal"]["id"],
        goal.as_str(),
        "what was there is there, read from the rebuilt index"
    );
    ws.start();
    assert_eq!(ws.json(&["status", &goal])["status"], "draft");
    ws.stop();
}

#[test]
fn the_git_setup_is_read_and_written_through_the_node_that_runs() {
    let mut ws = Sealed::bare();
    // This machine's global git config, for the node: a file of the
    // journey's own, which the profiles and the default account are written to.
    let global = ws.file("gitconfig", "");
    ws.start_in(&[("GIT_CONFIG_GLOBAL", &global.to_string_lossy())]);

    // --- a code host's health, and how to sign in --------------------------------------
    let health = ws.json(&["git", "health"]);
    assert_eq!(health["kind"], "github", "{health}");
    assert_eq!(
        health["cli"]["installed"], false,
        "no code host's own program is on this journey's path: {health}"
    );
    assert_eq!(health["accounts"], json!([]), "{health}");
    assert_eq!(
        health["resolves"],
        Value::Null,
        "nobody would answer: {health}"
    );
    let plan = ws.json(&["git", "login"]);
    assert_eq!(
        plan["kind"], "install",
        "its program is not installed: {plan}"
    );
    let plan = ws.json(&["git", "login", "--host", "bitbucket"]);
    assert_eq!(
        plan["kind"], "token",
        "a host with no program of its own: {plan}"
    );
    ws.refused(&["git", "health", "--host", "a-host-nobody-knows"]);

    // --- the accounts: none stored, a default set and cleared --------------------------
    let accounts = ws.json(&["git", "account", "list"]);
    assert_eq!(accounts["accounts"], json!([]), "{accounts}");
    assert_eq!(accounts["default"], Value::Null, "{accounts}");
    // A token comes in on the input, once: with none there is nothing to
    // add, and the host is never asked.
    let no_token = ws.refused(&["git", "account", "add"]);
    assert!(no_token.contains("stdin"), "{no_token}");
    ws.refused(&["git", "account", "default"]);
    let default = ws.json(&["git", "account", "default", "ada"]);
    assert_eq!(default["default"], "ada", "{default}");
    assert_eq!(ws.json(&["git", "account", "list"])["default"], "ada");
    assert!(
        std::fs::read_to_string(&global)
            .expect("the journey's global config")
            .contains("ada"),
        "written to the global git config the node was given"
    );
    let cleared = ws.json(&["git", "account", "default", "--clear"]);
    assert_eq!(cleared["default"], Value::Null, "{cleared}");
    // A login nobody stored: nothing to check it with, said without a
    // request to the host; and forgetting it forgets nothing.
    let unchecked = ws.json(&["git", "account", "check", "ada"]);
    assert_eq!(unchecked["state"], "no_token", "{unchecked}");
    ws.bisa(&["git", "account", "rm", "ada"]);
    assert_eq!(ws.json(&["git", "account", "list"])["accounts"], json!([]));

    // --- a profile by organization: saved, listed, removed ------------------------------
    assert_eq!(
        ws.json(&["git", "profile", "list"])["profiles"],
        json!([]),
        "none yet"
    );
    let saved = ws.json(&[
        "git",
        "profile",
        "set",
        "acme",
        "--label",
        "Acme",
        "--owner",
        "acme",
        "--name",
        "A Journey",
        "--email",
        "journey@acme.test",
    ]);
    assert_eq!(saved["slug"], "acme", "{saved}");
    assert_eq!(saved["host"], "github.com", "{saved}");
    let listed = ws.json(&["git", "profile", "list"]);
    assert_eq!(
        listed["profiles"].as_array().map(Vec::len),
        Some(1),
        "{listed}"
    );
    assert_eq!(listed["profiles"][0]["email"], "journey@acme.test");
    ws.refused(&[
        "git",
        "profile",
        "set",
        "Not A Slug",
        "--label",
        "x",
        "--owner",
        "x",
        "--name",
        "x",
        "--email",
        "x@example.test",
    ]);
    let removed = ws.json(&["git", "profile", "rm", "acme"]);
    assert_eq!(removed["removed"], "acme", "{removed}");
    assert_eq!(ws.json(&["git", "profile", "list"])["profiles"], json!([]));

    // --- what a checkout will use when it talks to its remote ---------------------------
    let made = ws.json(&["project", "new", "door", "--committer", COMMITTER]);
    let project = made["project"]["id"]
        .as_str()
        .expect("the project")
        .to_string();
    let connection = ws.json(&["git", "connection", &project]);
    assert_eq!(
        connection["remote"],
        Value::Null,
        "it has no remote: {connection}"
    );
    assert_eq!(
        connection["identity"]["email"], "journey@example.test",
        "who commits in it: {connection}"
    );
    ws.refused(&["git", "connection", "not-a-workstream"]);
    ws.stop();
}
