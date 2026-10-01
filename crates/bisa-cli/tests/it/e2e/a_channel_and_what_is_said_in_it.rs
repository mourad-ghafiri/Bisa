//! A channel and what is said in it, while a node runs: a channel made from
//! the command line with its roster, edited without losing the half of the
//! roster an edit did not name; a message that names an agent answered by
//! it, its words carried to whoever listens as they are written and landing
//! with the thinking that led to them; a message that names nobody answered
//! by the General Agent; a reaction and a retraction; what an agent made
//! posted as an artifact from where it works, read back with its bytes and
//! under its own name, and a person's beside it; the read marks; and all of
//! it as it was after the node went and came back.

use super::sealed::{Sealed, AGENT_HARNESS};
use serde_json::{json, Value};

const SHEET: &str = "quarter,total\nQ3,42\n";

/// Scout answers a question, and makes a sheet when asked for the numbers;
/// the General Agent answers what names nobody.
fn the_script() -> Value {
    json!({ "turns": [
        {
            "scope": "conversation", "agent": "scout", "when": "how deep", "times": 1,
            "think": "the drawings say two metres",
            "say": ["Two ", "metres, ", "by the drawings."],
        },
        {
            "scope": "conversation", "agent": "scout", "when": "the numbers", "times": 1,
            "write": [{ "path": "q3.csv", "content": SHEET }],
            "tools": [{
                "name": "post_message",
                "arguments": {
                    "content": "The numbers, as a sheet.",
                    "artifacts": [{ "path": "q3.csv", "title": "Q3 numbers" }],
                },
            }],
            "say": ["Posted."],
        },
        {
            "scope": "conversation", "agent": "general-agent", "when": "anybody", "times": 1,
            "say": ["I am here."],
        },
    ]})
}

/// The messages of a scope as the node lists them, oldest first — read over
/// the socket, which moves no read mark; `bisa msgs` does.
fn messages(ws: &Sealed, scope: &str) -> Vec<Value> {
    let (status, room) = ws.call("GET", &format!("/channels/{scope}/messages"), &[], None);
    assert_eq!(status, 200, "{room}");
    room["messages"].as_array().cloned().unwrap_or_default()
}

fn by<'a>(all: &'a [Value], author: &str) -> Vec<&'a Value> {
    all.iter().filter(|m| m["author"] == author).collect()
}

fn pubkey(ws: &Sealed, agent: &str) -> String {
    ws.json(&["agent", "show", agent])["agent"]["pubkey"]
        .as_str()
        .unwrap_or_else(|| panic!("{agent}'s key"))
        .to_string()
}

/// A channel's row of the list of channels, as the node answers it.
fn listed(ws: &Sealed, channel: &str) -> Value {
    let (status, rooms) = ws.call("GET", "/channels", &[], None);
    assert_eq!(status, 200, "{rooms}");
    rooms["channels"]
        .as_array()
        .and_then(|all| all.iter().find(|c| c["channel"]["id"] == channel))
        .unwrap_or_else(|| panic!("{channel} among the channels: {rooms}"))
        .clone()
}

/// How much of a channel the person has not read, as the node counts it.
fn unread(ws: &Sealed, channel: &str) -> u64 {
    let row = listed(ws, channel);
    row["unread_count"]
        .as_u64()
        .unwrap_or_else(|| panic!("a count: {row}"))
}

/// What the list says was last said in a channel: `(author, first words)`,
/// or nothing yet.
fn last_said(ws: &Sealed, channel: &str) -> Option<(String, String)> {
    let row = listed(ws, channel);
    let latest = &row["latest"];
    if latest.is_null() {
        return None;
    }
    assert!(latest["at"].as_u64().is_some(), "it says when: {row}");
    Some((
        latest["author"].as_str().unwrap_or_default().to_string(),
        latest["snippet"].as_str().unwrap_or_default().to_string(),
    ))
}

#[test]
fn a_channel_is_made_spoken_in_answered_and_read_back() {
    let mut ws = Sealed::with_script(&the_script());
    ws.start();
    let listening = ws.listen();

    // --- the room, and who belongs in it ------------------------------------------
    let scout = ws.json(&[
        "agent",
        "add",
        "--name",
        "Scout",
        "--prompt",
        "You look a site over and say what is there.",
        "--harness",
        AGENT_HARNESS,
    ])["agent"]["id"]
        .as_str()
        .expect("the agent")
        .to_string();
    let survey = ws.json(&["team", "create", "Survey", "--agent", &scout])["team"]["id"]
        .as_str()
        .expect("the team")
        .to_string();
    let made = ws.json(&[
        "channels", "create", "site", "--topic", "the site", "--agent", &scout, "--team", &survey,
    ]);
    let channel = made["channel"]["id"]
        .as_str()
        .expect("the channel")
        .to_string();
    let roster = json!({ "policy": "listed", "agents": [scout], "teams": [survey], "humans": [] });
    assert_eq!(made["channel"]["roster"], roster, "{made}");
    assert_eq!(
        last_said(&ws, &channel),
        None,
        "who joined is nobody's last words"
    );

    // An edit keeps what it does not name: the topic alone, then one half
    // of the roster alone.
    let edited = ws.json(&[
        "channels",
        "edit",
        &channel,
        "--topic",
        "the site, and its survey",
    ]);
    assert_eq!(edited["channel"]["topic"], "the site, and its survey");
    assert_eq!(edited["channel"]["roster"], roster, "{edited}");
    let edited = ws.json(&["channels", "edit", &channel, "--agent", &scout]);
    assert_eq!(
        edited["channel"]["roster"], roster,
        "naming an agent takes no team off the roster: {edited}"
    );
    assert_eq!(edited["channel"]["topic"], "the site, and its survey");
    let out = ws.bisa(&["channels", "edit", &channel, "--team", "a-team-nobody-made"]);
    assert!(!out.status.success(), "a team nobody made is refused");

    // --- a message that names an agent --------------------------------------------
    let (scout_key, general_key) = (pubkey(&ws, &scout), pubkey(&ws, "general-agent"));
    ws.ok(&[
        "msg",
        &channel,
        "how deep is the trench?",
        "--mention",
        &scout,
    ]);
    let all = ws.until("Scout's answer", || {
        let all = messages(&ws, &channel);
        (!by(&all, &scout_key).is_empty()).then_some(all)
    });
    let answer = by(&all, &scout_key)[0];
    assert_eq!(answer["content"], "Two metres, by the drawings.");
    assert_eq!(
        answer["thinking"], "the drawings say two metres",
        "the thinking is kept beside the words"
    );
    assert!(
        by(&all, &general_key).is_empty(),
        "addressed, it is nobody else's to answer: {all:?}"
    );
    // The words were carried as they were written, and they add up.
    let streamed = ws.until("the stream to have been heard", || {
        let frames = listening.heard().frames;
        let of_the_turn: Vec<&Value> = frames
            .iter()
            .filter(|frame| frame["stream"] == "engine")
            .map(|frame| &frame["payload"]["payload"])
            .filter(|said| said["scope"] == channel.as_str() && said["agent"] == scout.as_str())
            .collect();
        let landed = of_the_turn
            .iter()
            .any(|said| said["type"] == "agent_replied" && said["posted"] == true);
        let part = |key: &str| -> String {
            of_the_turn
                .iter()
                .filter(|said| said["type"] == "agent_streamed")
                .filter_map(|said| said[key].as_str())
                .collect()
        };
        landed.then(|| (part("text"), part("thinking")))
    });
    assert_eq!(
        streamed,
        (
            "Two metres, by the drawings.".to_string(),
            "the drawings say two metres".to_string()
        )
    );

    // --- a message that names nobody ----------------------------------------------
    ws.ok(&["msg", &channel, "is anybody there?"]);
    let all = ws.until("the General Agent's answer", || {
        let all = messages(&ws, &channel);
        (!by(&all, &general_key).is_empty()).then_some(all)
    });
    assert_eq!(by(&all, &general_key)[0]["content"], "I am here.");
    assert_eq!(by(&all, &scout_key).len(), 1, "and nobody else: {all:?}");
    let the_generals = (general_key.clone(), "I am here.".to_string());
    assert_eq!(
        last_said(&ws, &channel),
        Some(the_generals.clone()),
        "the list of channels says who spoke last, and what"
    );

    // --- a reaction, and a word taken back ------------------------------------------
    // Whoever listens is told, with every fact about the room, what now
    // stands as its last words — the node's rule, carried: a list that
    // follows the bus mirrors nothing.
    let live = ws.listen();
    let answered = answer["id"].as_str().expect("the answer's id").to_string();
    let (status, reacted) = ws.call(
        "POST",
        &format!("/messages/{answered}/react"),
        &[],
        Some(&json!({ "emoji": "🎯" })),
    );
    assert_eq!(status, 200, "{reacted}");
    let (status, room) = ws.call("GET", &format!("/channels/{channel}/messages"), &[], None);
    assert_eq!(status, 200, "{room}");
    let mine: Vec<&Value> = room["reactions"]
        .as_array()
        .expect("the reactions")
        .iter()
        .filter(|r| r["emoji"] == "🎯" && r["target_id"] == answered.as_str())
        .collect();
    assert_eq!(mine.len(), 1, "{room}");
    let said = ws.json(&["msg", &channel, "forget I asked"]);
    let taken_back = said["id"]
        .as_str()
        .unwrap_or_else(|| panic!("the message's id: {said}"))
        .to_string();
    assert_eq!(
        last_said(&ws, &channel).map(|(_, words)| words),
        Some("forget I asked".to_string()),
        "a reaction is nobody's last words; a post is"
    );
    let (status, gone) = ws.call(
        "POST",
        &format!("/messages/{taken_back}/retract"),
        &[],
        Some(&json!({})),
    );
    assert_eq!(status, 200, "{gone}");
    assert_eq!(
        last_said(&ws, &channel),
        Some(the_generals.clone()),
        "a word taken back is nobody's last: the one before it is"
    );
    // The three facts this step made, by their ids: the General Agent is
    // woken by *forget I asked* too, and what its wake appends — nothing a
    // list calls the last words — is a frame of the room all the same.
    let ours = [
        reacted["id"]
            .as_str()
            .expect("the reaction's id")
            .to_string(),
        taken_back.clone(),
        gone["id"]
            .as_str()
            .expect("the retraction's id")
            .to_string(),
    ];
    let told = ws.until("the three facts said to whoever listens", || {
        let told: Vec<Value> = live
            .heard()
            .frames
            .iter()
            .filter(|frame| {
                frame["stream"] == "conversation"
                    && frame["payload"]["scope"] == channel.as_str()
                    && ours
                        .iter()
                        .any(|id| frame["payload"]["event_id"] == id.as_str())
            })
            .map(|frame| frame["payload"]["latest"]["snippet"].clone())
            .collect();
        (told.len() >= 3).then_some(told)
    });
    assert_eq!(
        told,
        vec![
            json!(the_generals.1),
            json!("forget I asked"),
            json!(the_generals.1)
        ],
        "a reaction leaves the last words standing, a post takes their place, \
         a retraction gives it back"
    );
    drop(live);
    let (_, room) = ws.call("GET", &format!("/channels/{channel}/messages"), &[], None);
    let row = room["messages"]
        .as_array()
        .expect("the messages")
        .iter()
        .find(|m| m["id"] == taken_back.as_str())
        .unwrap_or_else(|| panic!("a retracted message keeps its place: {room}"));
    assert_eq!(row["retracted"], true, "{row}");

    // --- what an agent made, to look at ---------------------------------------------
    ws.ok(&["msg", &channel, "the numbers, please", "--mention", &scout]);
    let sheet = ws.until("the sheet", || {
        messages(&ws, &channel)
            .into_iter()
            .find(|m| m["content"] == "The numbers, as a sheet.")
    });
    let id = sheet["id"].as_str().expect("its id");
    let (status, read) = ws.call("GET", &format!("/messages/{id}"), &[], None);
    assert_eq!(status, 200, "{read}");
    let artifact = &read["message"]["artifacts"][0];
    assert_eq!(artifact["title"], "Q3 numbers", "{read}");
    assert_eq!(artifact["kind"], "sheet");
    assert_eq!(artifact["name"], "q3.csv");
    assert_eq!(artifact["size"], SHEET.len());
    assert_eq!(artifact["present"], true, "its bytes are here");
    assert_eq!(
        artifact["source"],
        Value::Null,
        "an agent's own folder is nobody's root but the agent's"
    );
    let sha = artifact["sha256"].as_str().expect("its address");
    let (status, named) = ws.call(
        "POST",
        &format!("/attachments/{sha}/file"),
        &[],
        Some(&json!({ "name": "q3.csv" })),
    );
    assert_eq!(status, 200, "{named}");
    let copy = std::path::PathBuf::from(named["path"].as_str().expect("the named copy"));
    assert!(
        copy.starts_with(ws.data()) && copy.ends_with(format!("named/{sha}/q3.csv")),
        "{}",
        copy.display()
    );
    assert_eq!(std::fs::read_to_string(&copy).expect("the copy"), SHEET);
    ws.until("the turn that made it to have ended", || {
        (by(&messages(&ws, &channel), &scout_key).len() == 2).then_some(())
    });

    // A person's, from the command line; and the conversation's gallery.
    let mine = ws.file("plan.md", "# The plan\n\nDig, then pour.\n");
    let spec = format!("{}:The plan", mine.display());
    ws.ok(&["msg", &channel, "and mine", "--artifact", &spec]);
    let (status, gallery) = ws.call("GET", &format!("/artifacts/{channel}"), &[], None);
    assert_eq!(status, 200, "{gallery}");
    let titles: Vec<&str> = gallery["artifacts"]
        .as_array()
        .expect("the gallery")
        .iter()
        .filter_map(|a| a["title"].as_str())
        .collect();
    assert_eq!(titles, ["The plan", "Q3 numbers"], "newest first");

    // --- what was read, and what was not --------------------------------------------
    assert!(unread(&ws, &channel) > 0, "agents spoke since it was read");
    let shown = ws.json(&["msgs", &channel]);
    assert_eq!(
        shown["messages"].as_array().map(Vec::len),
        Some(messages(&ws, &channel).len()),
        "the verb that reads shows the same messages: {shown}"
    );
    assert_eq!(unread(&ws, &channel), 0, "and reading them is reading them");
    ws.ok(&["unread", &channel]);
    assert!(unread(&ws, &channel) > 0);
    ws.ok(&["read", &channel]);
    assert_eq!(unread(&ws, &channel), 0);

    // --- the same after the node went and came back ----------------------------------
    let before = ws.json(&["msgs", &channel]);
    drop(listening);
    ws.stop();
    assert_eq!(
        ws.json(&["msgs", &channel]),
        before,
        "read from the files, no node"
    );
    ws.start();
    assert_eq!(ws.json(&["msgs", &channel]), before);
    assert_eq!(unread(&ws, &channel), 0);
    assert_eq!(
        last_said(&ws, &channel).map(|(_, words)| words),
        Some("and mine".to_string()),
        "read back from the index the node rebuilt"
    );
    let kept = ws.json(&["channels", "list"]);
    let room = kept["channels"]
        .as_array()
        .expect("the channels")
        .iter()
        .find(|c| c["id"] == channel.as_str())
        .expect("the channel");
    assert_eq!(room["roster"], roster);
    ws.stop();
}
