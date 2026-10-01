//! What a person says from the command line while a node runs reaches its
//! engine, as what they say in the window does: an agent written to answers,
//! and the answer is read back with the verb that reads.

use super::sealed::Sealed;
use serde_json::{json, Value};

/// An agent that answers whoever speaks to it, in a sentence.
fn answering() -> Value {
    json!({
        "turns": [{ "scope": "conversation", "say": ["Two agents, one channel, nothing installed yet."] }],
    })
}

/// The messages of a scope that are not retracted: who said each, and what.
fn said(ws: &Sealed, scope: &str) -> Vec<(String, String)> {
    ws.json(&["msgs", scope])["messages"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|m| {
                    (
                        m["author"].as_str().unwrap_or_default().to_string(),
                        m["content"].as_str().unwrap_or_default().to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn an_agent_written_to_from_the_command_line_answers_while_a_node_runs() {
    let mut ws = Sealed::with_script(&answering());
    let agents = ws.json(&["agent", "list"]);
    let general = agents["agents"]
        .as_array()
        .and_then(|all| all.iter().find(|a| a["id"] == "general-agent"))
        .and_then(|a| a["pubkey"].as_str())
        .unwrap_or_else(|| panic!("the General Agent's key: {agents}"))
        .to_string();
    ws.start();

    let sent = ws.json(&[
        "dm",
        "send",
        &general,
        "--text",
        "what is in this workspace?",
    ]);
    let channel = sent["channel"]
        .as_str()
        .expect("the direct channel")
        .to_string();

    let conversation = ws.until("the agent's answer", || {
        let all = said(&ws, &channel);
        all.iter()
            .any(|(author, _)| author == &general)
            .then_some(all)
    });
    assert_eq!(
        conversation
            .iter()
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>(),
        vec![
            "what is in this workspace?",
            "Two agents, one channel, nothing installed yet.",
        ]
    );
    assert_ne!(
        conversation[0].0, general,
        "the question is the person's, the answer the agent's"
    );
    // The list of direct messages says who spoke last in each, and what —
    // what the desktop sorts and previews by.
    let (status, rows) = ws.call("GET", "/dms", &[], None);
    assert_eq!(status, 200, "{rows}");
    let row = rows["dms"]
        .as_array()
        .and_then(|all| all.iter().find(|d| d["channel"]["id"] == channel.as_str()))
        .unwrap_or_else(|| panic!("the direct channel among them: {rows}"));
    assert_eq!(row["latest"]["author"], general.as_str(), "{row}");
    assert_eq!(
        row["latest"]["snippet"], "Two agents, one channel, nothing installed yet.",
        "{row}"
    );
    assert!(row["latest"]["at"].as_u64().is_some(), "{row}");
    // It was told what the person wrote, in a session of its own.
    let prompts = ws.recorded("prompt");
    assert!(
        prompts.iter().any(|p| p["scope"] == "conversation"
            && p["agent"] == "general-agent"
            && p["text"]
                .as_str()
                .is_some_and(|t| t.contains("what is in this workspace?"))),
        "{prompts:#?}"
    );

    // A second direct message reuses the conversation.
    let again = ws.json(&[
        "dm",
        "send",
        &general,
        "--text",
        "and what should I install?",
    ]);
    assert_eq!(again["channel"], channel.as_str());
    ws.until("the second answer", || {
        (said(&ws, &channel)
            .iter()
            .filter(|(author, _)| author == &general)
            .count()
            == 2)
            .then_some(())
    });
    ws.stop();

    // With no node the words are kept, and nobody is there to answer them.
    ws.ok(&["dm", "send", &general, "--text", "anybody?"]);
    let kept = said(&ws, &channel);
    assert_eq!(kept.len(), 5, "{kept:?}");
    assert_eq!(kept.last().map(|(_, text)| text.as_str()), Some("anybody?"));
}
