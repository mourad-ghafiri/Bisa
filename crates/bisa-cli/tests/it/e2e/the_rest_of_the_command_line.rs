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

/// The one file under `dir` whose name ends with `suffix` — a run's snapshot
/// by its id, wherever the store filed it.
fn file_ending_with(dir: &std::path::Path, suffix: &str) -> Option<std::path::PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = file_ending_with(&path, suffix) {
                return Some(found);
            }
        } else if path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().ends_with(suffix))
        {
            return Some(path);
        }
    }
    None
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
    // The feed, through the node, before the goal exists.
    assert!(ws.json(&["pulse"])["rows"].is_array());

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
    // The inbox for a person: a question with no options offered.
    ws.ok(&["inbox"]);
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
    let run_id = at_oil["run"]["id"].as_str().expect("the run").to_string();
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

    // An answer aimed at a goal goes through the node, and a done goal with
    // nothing pending is refused in words — never a stray decision recorded
    // against a question long settled; an id that is neither a goal nor a
    // pending gate is refused too.
    let nothing_pending = ws.bisa(&["answer", &goal, "yes"]);
    assert!(
        !nothing_pending.status.success(),
        "{}",
        said_by(&nothing_pending)
    );
    assert!(
        said_by(&nothing_pending).contains("no pending gate"),
        "{}",
        said_by(&nothing_pending)
    );
    ws.refused(&["answer", "nonsense", "yes"]);
    let none_going = ws.json(&["workflow", "restart", &copy]);
    assert_eq!(none_going["runs"], json!([]), "{none_going}");
    let _ = ws.bisa(&["workflow", "hook-secret", "workflow", &copy, "fit"]);
    ws.refused(&["workflow", "hook-secret", "workflow", &copy, "not a step!"]);
    ws.stop();

    // --- a run file this build cannot read costs that goal's run, never the list -----
    let torn = file_ending_with(&ws.data(), &format!("-{run_id}.json")).expect("the run's file");
    let as_written = std::fs::read(&torn).unwrap();
    std::fs::write(&torn, "{torn").unwrap();
    let found = ws.json(&["search"]);
    let row = found["matches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["goal"] == goal.as_str())
        .unwrap_or_else(|| panic!("the goal is still listed: {found}"));
    assert_eq!(row["run_unreadable"], json!(true), "{row}");
    assert!(row["status"].is_string(), "drawn without its run: {row}");
    ws.ok(&["search"]);
    // The file as it was, for the verbs below to read the run again.
    std::fs::write(&torn, as_written).unwrap();

    // --- without a node: the same verbs on the workspace itself ---------------------
    ws.refused(&["design", &goal]);
    ws.ok(&["archive", "workflow", &copy]);
    ws.ok(&["archive", "workflow", &copy, "--undo"]);
    ws.ok(&["archive", "goal", &goal]);
    ws.ok(&["archive", "goal", &goal, "--undo"]);
    assert!(ws.json(&["status", &goal])["status"].is_string());
    ws.refused(&["archive", "workflow", "not-an-id"]);
    ws.refused(&["archive", "thing", &goal]);
    ws.refused(&["rm", &goal, "--projects", "nope"]);
    // A goal that is done has nothing left to amend; the draft is read all the same.
    let late = ws.bisa(&["amend", &goal, "--from", &amended.to_string_lossy()]);
    assert!(!late.status.success(), "{}", said_by(&late));
    let set = ws.json(&["governance", "set", "approval", "admins"]);
    assert_eq!(set["governance"]["approval"]["policy"], "admins", "{set}");
    let listed = ws.json(&["governance", "set", "publish", "ada, bob"]);
    assert!(listed["governance"]["publish"].is_object(), "{listed}");
    ws.json(&["governance", "show"]);
    let note = ws.file("note.txt", "hello\n");
    ws.ok(&[
        "msg",
        "general",
        "a file for you",
        "--attach",
        &note.to_string_lossy(),
    ]);
    ws.refused(&["msg", "general", "a file", "--attach", "/nowhere/none.txt"]);
    let made = ws.json(&[
        "project",
        "new",
        "shelf",
        "--committer",
        "A Journey <journey@example.test>",
    ]);
    let project = made["project"]["id"]
        .as_str()
        .expect("the project")
        .to_string();
    ws.ok(&["archive", "project", &project]);
    ws.ok(&["archive", "project", &project, "--undo"]);

    // The smaller verbs, with no node: a server asked for with no socket, a
    // scope that is two scopes, a guided goal before its proposal, a scope
    // that is none, a tag count with nothing tagged, the workspace checked,
    // the catalog filtered to nothing and an entry shown, a quiet channel
    // read and unread, the feed's goals, a language flag with no language,
    // and a restart of a goal that is done.
    ws.refused(&["mcp"]);
    ws.refused(&[
        "mcp",
        "--socket",
        "nowhere",
        "--work-item",
        "01ARZ3NDEKTSV4RRFFQ69G5FAV",
        "--goal",
        &goal,
    ]);
    let guided = ws.json(&["new", "think about it", "--mode", "guided"])["goal"]
        .as_str()
        .expect("the guided goal")
        .to_string();
    ws.json(&["status", &guided]);
    ws.refused(&["settings", "set", "bogus", "k", "v"]);
    ws.json(&["tags", "--entity", "goal"]);
    ws.ok(&["workspace", "check"]);
    assert_eq!(
        ws.json(&["catalog", "list", "--tag", "nothing-wears-this"])["entries"],
        json!([])
    );
    let entries = ws.json(&["catalog", "list"]);
    for e in entries["entries"]
        .as_array()
        .expect("the catalog")
        .iter()
        .take(3)
    {
        ws.json(&[
            "catalog",
            "show",
            e["kind"].as_str().expect("a kind"),
            e["slug"].as_str().expect("a slug"),
        ]);
    }
    ws.json(&["channels", "create", "quiet"]);
    assert_eq!(ws.json(&["msgs", "quiet"])["messages"], json!([]));
    ws.ok(&["unread", "quiet"]);
    ws.ok(&["read", "quiet"]);
    assert_eq!(
        ws.json(&["channels", "list", "--tag", "nothing-wears-this"])["channels"],
        json!([])
    );
    ws.json(&["channels", "list"]);
    assert_eq!(ws.json(&["dm", "list"])["dms"], json!([]));
    ws.json(&["pulse"]);
    ws.json(&["pulse", "--concept", "runs"]);
    let _ = ws.bisa(&["status", &goal, "--lang"]);
    let restarted = ws.bisa(&["restart", &goal]);
    assert!(!restarted.status.success(), "{}", said_by(&restarted));

    // A goal run with no node and watched to the gate it waits at: the gate
    // is said, with no terminal to answer it at, and the wait is handed off.
    let approve = ws.file(
        "approve.json",
        &json!({"name": "Approve it", "steps": [
            {"id": "ok", "name": "Ok?", "kind": "approval", "prompt": "Go?", "then": ["after"]},
            {"id": "after", "name": "After", "kind": "agent",
             "assignee": {"agent": "general-agent"}, "instructions": "say so"},
        ]})
        .to_string(),
    );
    let wf = ws.json(&["workflow", "new", "--from", &approve.to_string_lossy()])["workflow"]["id"]
        .as_str()
        .expect("the workflow")
        .to_string();
    let gated = ws.json(&["new", "approve this", "--workflow", &wf, "--mode", "manual"])["goal"]
        .as_str()
        .expect("the gated goal")
        .to_string();
    let watched = ws.bisa(&["run", &gated, "--watch"]);
    assert!(watched.status.success(), "{}", said_by(&watched));
    let again = ws.bisa(&["run", &gated]);
    assert!(again.status.success(), "{}", said_by(&again));
    let again = ws.bisa(&["run", &gated, "--watch"]);
    assert!(again.status.success(), "{}", said_by(&again));
    // A goal left to its designer with no node: the designer is awaited here,
    // its work said as it happens, and the wait ends with its word.
    let _ = ws.bisa(&["new", "design this one"]);
    // The inbox with no node: the goal that waits is a row.
    let inbox = ws.json(&["inbox"]);
    assert!(
        inbox["rows"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|r| r["key"] == gated.as_str())),
        "{inbox}"
    );
    // Approved by its goal's id: the run moves on and is followed to its end.
    let decided = ws.bisa(&["approve", &gated]);
    assert!(decided.status.success(), "{}", said_by(&decided));

    // Words printed for a person: the guided goal's status, the pulse, the
    // inbox with a channel nobody read, the catalog's entries.
    ws.ok(&["status", &guided]);
    ws.ok(&["pulse"]);
    ws.ok(&["msg", "quiet", "a word nobody has read"]);
    ws.ok(&["unread", "quiet"]);
    let rows = ws.json(&["inbox"]);
    assert!(
        rows["rows"]
            .as_array()
            .is_some_and(|r| r.iter().any(|x| x["key"] == "quiet")),
        "{rows}"
    );
    ws.ok(&["inbox"]);
    ws.ok(&["channels", "list"]);
    for e in entries["entries"]
        .as_array()
        .expect("the catalog")
        .iter()
        .take(3)
    {
        ws.ok(&[
            "catalog",
            "show",
            e["kind"].as_str().expect("a kind"),
            e["slug"].as_str().expect("a slug"),
        ]);
    }

    // A goal's documents from files, and one that is not there; a statement
    // that is none.
    let doc = ws.file("plan.md", "measure twice\n");
    ws.ok(&[
        "new",
        "read the plan",
        "--document",
        &doc.to_string_lossy(),
        "--mode",
        "manual",
    ]);
    ws.refused(&[
        "new",
        "read nothing",
        "--document",
        "/nowhere/none.md",
        "--mode",
        "manual",
    ]);
    ws.refused(&["new", "   ", "--mode", "manual"]);

    // Workflow verbs with no node: a definition from stdin, a slug nobody
    // installed, a name that is none, templates filtered to none, a one-step
    // agent workflow run and watched to its end, its run stopped and
    // restarted, its runs restarted when none goes, and a goal given its
    // own design from a draft.
    let agent_wf = json!({"name": "Think", "steps": [
        {"id": "think", "name": "Think", "kind": "agent",
         "assignee": {"agent": "general-agent"}, "instructions": "think"},
    ]});
    let think = ws.json_stdin(&["workflow", "new", "--from", "-"], &agent_wf.to_string())
        ["workflow"]["id"]
        .as_str()
        .expect("the workflow")
        .to_string();
    ws.refused(&["workflow", "run", "no-such-slug"]);
    ws.refused(&["workflow", "run", ""]);
    ws.json(&[
        "workflow",
        "list",
        "--templates",
        "--tag",
        "nothing-wears-this",
    ]);
    ws.json(&["workflow", "list", "--tag", "nothing-wears-this"]);
    let watched = ws.bisa(&["workflow", "run", &think, "--watch"]);
    assert!(watched.status.success(), "{}", said_by(&watched));
    let started = ws.json(&["workflow", "run", &think]);
    let run = started["run"].as_str().expect("the run").to_string();
    let _ = ws.bisa(&["workflow", "stop", &think, "--run", &run]);
    let _ = ws.bisa(&["workflow", "restart", &think, "--run", &run]);
    let restarted = ws.json(&["workflow", "restart", &think]);
    assert!(restarted["runs"].is_array(), "{restarted}");
    ws.refused(&["workflow", "use", &guided]);
    ws.json(&[
        "workflow",
        "use",
        &guided,
        "--from",
        &approve.to_string_lossy(),
    ]);
    ws.ok(&["rm", &goal, "--projects", "keep"]);

    // The node once more, with the releases door pointed at nothing and an
    // opt-in file that is no table: both are said, neither stops the start.
    std::fs::write(ws.data().join("a2a.toml"), "not = [a table").expect("the file");
    ws.start_in(&[("BISA_RELEASES_API", "http://127.0.0.1:9")]);
    ws.stop();
    std::fs::write(
        ws.data().join("a2a.toml"),
        "public_base_url = \"http://127.0.0.1:9\"\n",
    )
    .expect("the file");
    ws.start();
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

/// With no node running, a conversation's record is the whole of it: one
/// is started, listed and narrowed, titled and untitled, put away and
/// taken back out, read for its mode and deleted — and every word that is
/// no conversation, no origin, no id or no mode is refused by name.
#[test]
fn conversations_are_kept_whole_with_no_node_running() {
    let ws = Sealed::bare();
    let empty = ws.json(&["conversation", "list"]);
    assert_eq!(empty["conversations"], json!([]), "{empty}");

    let made = ws.json(&["conversation", "new", "workspace", "--title", "Plans"]);
    let id = made["conversation"]["id"]
        .as_str()
        .expect("the id")
        .to_string();
    assert_eq!(made["conversation"]["title"], "Plans");
    assert_eq!(made["conversation"]["origin"]["kind"], "workspace");
    let untitled = ws.json(&["conversation", "new", "node"]);
    assert!(untitled["conversation"]["title"].is_null(), "{untitled}");

    let listed = ws.json(&["conversation", "list"]);
    assert_eq!(
        listed["conversations"].as_array().unwrap().len(),
        2,
        "{listed}"
    );
    let narrowed = ws.json(&["conversation", "list", "--origin", "workspace"]);
    assert_eq!(
        narrowed["conversations"].as_array().unwrap().len(),
        1,
        "{narrowed}"
    );
    assert_eq!(narrowed["conversations"][0]["id"], id.as_str());

    let renamed = ws.json(&["conversation", "rename", &id, "The plan"]);
    assert_eq!(renamed["conversation"]["title"], "The plan");
    let cleared = ws.json(&["conversation", "rename", &id]);
    assert!(cleared["conversation"]["title"].is_null(), "{cleared}");
    let away = ws.json(&["conversation", "archive", &id]);
    assert_eq!(away["conversation"]["archived"], true);
    let back = ws.json(&["conversation", "unarchive", &id]);
    assert_eq!(back["conversation"]["archived"], false);
    let mode = ws.json(&["conversation", "mode", &id]);
    assert_eq!(mode["conversation"]["mode"], "manual", "{mode}");
    ws.refused(&["conversation", "mode", &id, "sideways"]);
    ws.ok(&["conversation", "delete", &id]);
    assert_eq!(
        ws.json(&["conversation", "list"])["conversations"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // The words refused by name.
    ws.refused(&["conversation", "show", "not-an-id"]);
    ws.refused(&["conversation", "new", "planet"]);
    ws.refused(&[
        "conversation",
        "new",
        "workspace",
        "01ARZ3NDEKTSV4RRFFQ69G5FAV",
    ]);
    ws.refused(&["conversation", "new", "goal"]);
    ws.refused(&["conversation", "new", "workstream", "not-a-workstream"]);
    ws.refused(&["conversation", "new", "goal", "not-a-goal"]);
    ws.refused(&["conversation", "list", "--id", "01ARZ3NDEKTSV4RRFFQ69G5FAV"]);
    ws.refused(&[
        "conversation",
        "list",
        "--origin",
        "goal",
        "--id",
        "not-a-goal",
    ]);
    ws.refused(&["conversation", "list", "--project", "not-a-project"]);
}
