//! The Inbox and the Pulse, through the binary: what concerns a person is one
//! row a thing — an ask on a goal's row until it is answered, a run that
//! failed as a notice until it is read, an agent's reply as an unread message
//! until the conversation is opened — and the feed of everything that
//! happened is paged newest first, as far back as the workspace goes, each
//! fact once. A conversation's row sits under what the conversation is
//! about and says which; it is found by a word said in it, and read by its
//! id whatever a page of the list holds.

use super::sealed::Sealed;
use serde_json::{json, Value};

/// An agent that answers whoever speaks to it.
fn answering() -> Value {
    json!({ "turns": [{ "scope": "conversation", "say": ["Here is what I found."] }] })
}

/// A question to a person, then an end that fails on purpose.
fn asks_then_fails() -> Value {
    json!({
        "name": "Ask, then fail",
        "steps": [
            { "id": "ask", "name": "Go on?", "kind": "human", "prompt": "Go on?",
              "options": [{ "id": "yes", "label": "Yes" }, { "id": "no", "label": "No" }],
              "then": ["stop"] },
            { "id": "stop", "name": "Stop here", "kind": "end", "finish": "failed" },
        ],
    })
}

/// The inbox's row for `key`, when it has one.
fn row(ws: &Sealed, key: &str, filter: &str) -> Option<Value> {
    let (status, inbox) = ws.call("GET", &format!("/inbox?filter={filter}"), &[], None);
    assert_eq!(status, 200, "{inbox}");
    inbox["rows"]
        .as_array()?
        .iter()
        .find(|row| row["key"] == key)
        .cloned()
}

/// One page of the feed: its rows, and the cursor of the page after.
fn page(ws: &Sealed, query: &str) -> (Vec<Value>, Value) {
    let (status, pulse) = ws.call("GET", &format!("/pulse?{query}"), &[], None);
    assert_eq!(status, 200, "{pulse}");
    (
        pulse["rows"].as_array().cloned().unwrap_or_default(),
        pulse["next"].clone(),
    )
}

#[test]
fn what_concerns_a_person_is_a_row_until_it_is_answered_read_or_opened() {
    let mut ws = Sealed::with_script(&answering());
    let file = ws.file("ask.json", &asks_then_fails().to_string());
    let workflow = ws.json(&["workflow", "new", "--from", &file.to_string_lossy()])["workflow"]
        ["id"]
        .as_str()
        .expect("its id")
        .to_string();
    let general = ws.json(&["agent", "list"])["agents"]
        .as_array()
        .and_then(|all| all.iter().find(|a| a["id"] == "general-agent"))
        .and_then(|a| a["pubkey"].as_str())
        .expect("the General Agent's key")
        .to_string();
    ws.start();
    assert_eq!(
        ws.json(&["inbox"])["rows"],
        json!([]),
        "a fresh workspace asks nothing"
    );

    // An ask: the goal's row needs its person.
    let goal = ws.json(&["new", "decide something", "--workflow", &workflow])["goal"]
        .as_str()
        .expect("the goal")
        .to_string();
    let asking = ws.until("the goal's row to ask", || {
        let row = row(&ws, &goal, "needs_you")?;
        (row["needs_action"].as_array()?.len() == 1).then_some(row)
    });
    assert_eq!(asking["kind"], "goal");
    assert_eq!(asking["title"], "decide something");
    assert_eq!(asking["needs_action"][0]["gate_kind"], "escalation");
    assert_eq!(asking["handled"], false);

    // Answered: nothing is owed, and what the run came to is a notice —
    // the row is no longer among what needs the person, and is unread.
    let answered = ws.json(&["step", "answer", &goal, "ask", "-o", "yes"]);
    assert_eq!(
        answered["status"], "failed",
        "the workflow ends as failed: {answered}"
    );
    let noticed = ws.until("the failure to be a notice on the row", || {
        let row = row(&ws, &goal, "all")?;
        (row["unread_notices"].as_u64()? >= 1).then_some(row)
    });
    assert_eq!(noticed["needs_action"], json!([]));
    assert_eq!(noticed["read"], false);
    assert!(
        noticed["notices"]
            .as_array()
            .is_some_and(|all| all.iter().any(|n| n["notice"] == "run_failed")),
        "{noticed}"
    );
    assert!(
        row(&ws, &goal, "needs_you").is_none(),
        "it asks nothing any more"
    );
    assert!(row(&ws, &goal, "unread").is_some());

    // Read: the notice stays, and no longer counts.
    ws.ok(&["read", &goal]);
    let read = row(&ws, &goal, "all").expect("the row");
    assert_eq!(read["read"], true, "{read}");
    assert_eq!(read["unread_notices"], 0);
    assert!(row(&ws, &goal, "unread").is_none());
    // And unread again, by hand.
    ws.ok(&["unread", &goal]);
    assert!(row(&ws, &goal, "unread").is_some());

    // An agent's reply is an unread message until the conversation is opened.
    let channel = ws.json(&["dm", "send", &general, "--text", "what did you find?"])["channel"]
        .as_str()
        .expect("the direct channel")
        .to_string();
    let replied = ws.until("the reply to be unread", || {
        let row = row(&ws, &channel, "unread")?;
        (row["unread_count"].as_u64()? == 1).then_some(row)
    });
    assert_eq!(replied["kind"], "dm", "{replied}");
    ws.ok(&["msgs", &channel]);
    assert!(
        row(&ws, &channel, "unread").is_none(),
        "reading is acknowledging"
    );

    // A word that is no filter is refused by name, never answered with everything.
    let (status, refused) = ws.call("GET", "/inbox?filter=everything", &[], None);
    assert_eq!(status, 400, "{refused}");
    ws.stop();
}

#[test]
fn the_feed_is_paged_newest_first_as_far_back_as_the_workspace_goes() {
    let mut ws = Sealed::with_script(&answering());
    let file = ws.file("ask.json", &asks_then_fails().to_string());
    let workflow = ws.json(&["workflow", "new", "--from", &file.to_string_lossy()])["workflow"]
        ["id"]
        .as_str()
        .expect("its id")
        .to_string();
    ws.start();
    for n in 0..4 {
        let goal = ws.json(&["new", &format!("goal number {n}"), "--workflow", &workflow])["goal"]
            .as_str()
            .expect("the goal")
            .to_string();
        ws.until("the run to ask", || {
            (ws.json(&["status", &goal])["status"] == "waiting").then_some(())
        });
        ws.ok(&["step", "answer", &goal, "ask", "-o", "no"]);
    }
    ws.ok(&["msg", "general", "four goals, all failed on purpose"]);

    // Everything, in one page.
    let (whole, next) = page(&ws, "limit=500");
    assert_eq!(next, Value::Null, "the last page names no page after it");
    assert!(
        whole.len() >= 12,
        "captures, runs, steps and a message: {}",
        whole.len()
    );
    let stamp = |row: &Value| {
        (
            row["at"].as_u64().unwrap_or(0),
            row["seq"].as_u64().unwrap_or(0),
        )
    };
    assert!(
        whole
            .windows(2)
            .all(|pair| stamp(&pair[0]) >= stamp(&pair[1])),
        "newest first"
    );

    // The same, five at a time: every fact once, in the same order. The walk
    // starts at the newest fact the whole page had — bounded, since the
    // workspace goes on writing facts while the pages are read (a failed
    // run's notice, a row of the repair), and a fact that landed after the
    // whole page was read is no fact of it.
    let mut paged: Vec<Value> = Vec::new();
    let newest = stamp(&whole[0]);
    let mut query = format!("limit=5&before={}&before_seq={}", newest.0, newest.1 + 1);
    for _ in 0..whole.len() {
        let (rows, next) = page(&ws, &query);
        assert!(rows.len() <= 5);
        paged.extend(rows);
        if next.is_null() {
            break;
        }
        query = format!("limit=5&before={}&before_seq={}", next["at"], next["seq"]);
    }
    assert_eq!(paged.len(), whole.len(), "every fact once");
    assert_eq!(paged, whole);

    // One concept is its own feed.
    let (goals, _) = page(&ws, "limit=500&concept=goals");
    assert!(!goals.is_empty() && goals.len() < whole.len());
    assert!(
        goals.iter().all(|row| row["concept"] == "goals"),
        "{goals:?}"
    );
    let (channels, _) = page(&ws, "limit=500&concept=channels");
    assert!(
        channels.iter().any(|row| row
            .to_string()
            .contains("four goals, all failed on purpose")),
        "{channels:?}"
    );
    // A word that is no concept is refused by name, and a cursor that is
    // no number in the same shape: a status, and words.
    for query in ["concept=everything", "before=yesterday", "limit=-1"] {
        let (status, refused) = ws.call("GET", &format!("/pulse?{query}"), &[], None);
        assert_eq!(status, 400, "{query}: {refused}");
        assert!(
            refused["error"]
                .as_str()
                .is_some_and(|said| !said.is_empty()),
            "{query} is refused in words: {refused}"
        );
    }

    // The verb reads the same feed.
    let said = ws.json(&["pulse", "--limit", "3"]);
    assert_eq!(said["rows"].as_array().map(Vec::len), Some(3), "{said}");

    // And it is there when the node is back: the feed is the workspace's.
    ws.stop();
    ws.start();
    let (again, _) = page(&ws, "limit=500");
    assert_eq!(
        &again[again.len() - whole.len()..],
        &whole[..],
        "what was there is there"
    );
    ws.stop();
}

/// The ids of the rows under one source of the Inbox.
fn under(ws: &Sealed, source: &str) -> Vec<String> {
    let (status, inbox) = ws.call("GET", &format!("/inbox?source={source}"), &[], None);
    assert_eq!(status, 200, "{inbox}");
    inbox["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| row["key"].as_str().map(str::to_string))
        .collect()
}

/// The ids of the conversations a list names.
fn conversations(ws: &Sealed, query: &str) -> Vec<String> {
    let (status, listed) = ws.call("GET", &format!("/conversations?{query}"), &[], None);
    assert_eq!(status, 200, "{query}: {listed}");
    listed["conversations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c["id"].as_str().map(str::to_string))
        .collect()
}

#[test]
fn a_conversation_sits_under_what_it_is_about_is_found_by_a_word_said_and_read_by_its_id() {
    let mut ws = Sealed::with_script(&answering());
    ws.start();
    let goal = ws.json(&["new", "paint the door", "--mode", "manual"])["goal"]
        .as_str()
        .expect("the goal")
        .to_string();
    let project = ws.json(&[
        "project",
        "new",
        "door",
        "--publish",
        "manual",
        "--committer",
        "A Journey <journey@example.test>",
    ])["project"]["id"]
        .as_str()
        .expect("the project")
        .to_string();

    // Started as a surface of the desktop starts one: by what it is about
    // and nothing else — untitled, opened at once.
    let mut started: Vec<(String, Value, &str)> = Vec::new();
    for (origin, source, word) in [
        (json!({ "kind": "goal", "id": goal }), "goals", "primer"),
        (
            json!({ "kind": "project", "id": project }),
            "projects",
            "hinges",
        ),
        (json!({ "kind": "workspace" }), "messages", "weather"),
    ] {
        let (status, made) = ws.call(
            "POST",
            "/conversations",
            &[],
            Some(&json!({ "origin": origin })),
        );
        assert_eq!(status, 201, "{made}");
        let id = made["conversation"]["id"]
            .as_str()
            .unwrap_or_else(|| panic!("the conversation: {made}"))
            .to_string();
        assert!(made["conversation"].get("title").is_none(), "{made}");
        let (status, said) = ws.call(
            "POST",
            &format!("/conversations/{id}/messages"),
            &[],
            Some(&json!({ "content": format!("what of the {word}?") })),
        );
        assert_eq!(status, 200, "{said}");

        // The agent's answer is unread: the conversation is a row, under
        // what it is about, saying which.
        let unread = ws.until("the answer to be unread", || {
            let row = row(&ws, &id, "unread")?;
            (row["unread_count"].as_u64()? == 1).then_some(row)
        });
        assert_eq!(unread["kind"], "conversation", "{unread}");
        assert_eq!(unread["source"], source, "{unread}");
        assert_eq!(unread["origin"], origin, "{unread}");
        for tab in ["messages", "projects", "workflows", "goals", "people"] {
            assert_eq!(
                under(&ws, tab).contains(&id),
                tab == source,
                "a conversation about {origin} under {tab}"
            );
        }
        started.push((id, origin, word));
    }

    // Found by a word said in it — the node reads the messages, not the
    // titles alone — by itself and within what it is about.
    for (id, origin, word) in &started {
        assert_eq!(conversations(&ws, &format!("q={word}")), vec![id.clone()]);
        let kind = origin["kind"].as_str().expect("its kind");
        let narrowed = match origin["id"].as_str() {
            Some(about) => format!("origin={kind}&id={about}"),
            None => format!("origin={kind}"),
        };
        assert_eq!(
            conversations(&ws, &format!("{narrowed}&q={word}")),
            vec![id.clone()]
        );
        assert_eq!(
            conversations(&ws, &format!("{narrowed}&q=zygomorphic")),
            Vec::<String>::new()
        );
    }
    // The verb finds the same.
    let found = ws.json(&["conversation", "list", "--q", "hinges"]);
    assert_eq!(
        found["conversations"]
            .as_array()
            .map(|all| all.iter().map(|c| c["id"].clone()).collect::<Vec<_>>()),
        Some(vec![json!(started[1].0)]),
        "{found}"
    );

    // Read by its id whatever a page holds: a page of one names one, and
    // each of the three is still read.
    let page = conversations(&ws, "limit=1");
    assert_eq!(page.len(), 1);
    for (id, origin, _) in &started {
        let (status, one) = ws.call("GET", &format!("/conversations/{id}"), &[], None);
        assert_eq!(status, 200, "{one}");
        assert_eq!(one["conversation"]["origin"], *origin, "{one}");
        assert_eq!(one["conversation"]["message_count"], 2, "{one}");
    }

    // Opened, it concerns its person no longer; deleted, it is not found —
    // by the node that runs, and the others stand.
    let (about_goal, _, _) = &started[0];
    ws.ok(&["conversation", "show", about_goal]);
    assert!(!under(&ws, "goals").contains(about_goal));
    ws.ok(&["conversation", "delete", about_goal]);
    let (status, gone) = ws.call("GET", &format!("/conversations/{about_goal}"), &[], None);
    assert_eq!(status, 404, "{gone}");
    assert_eq!(conversations(&ws, "q=primer"), Vec::<String>::new());
    let mut left = conversations(&ws, "limit=100");
    left.sort();
    let mut expected = vec![started[1].0.clone(), started[2].0.clone()];
    expected.sort();
    assert_eq!(left, expected);
    ws.stop();
}
