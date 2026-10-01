//! The platform's tools, through the real `bisa mcp`: what each kind of
//! session is handed, and every tool of it called.
//!
//! A scripted agent is launched by the node in each of the three scopes a
//! session has — a worker on a step's item, the Workflow Agent designing a
//! goal, an agent's turn in a conversation — starts the platform's server as
//! a harness starts it, asks it for its tools, and calls **every one of them
//! with nothing in its hands**. The menu the server lists over the wire is
//! held to the menu that scope is promised (the router `bisa agent-context`
//! prints from), and every call is answered: a result, or a refusal in
//! words — never a server that went away, never a call that hangs. Empty
//! hands are the point: a tool that needs something says what it needs, a
//! tool that needs nothing reads, and nothing is made, moved or sent by an
//! agent that asked for nothing.

use super::sealed::{Sealed, AGENT_HARNESS};
use bisa_mcp::{BisaServer, Scope};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::PathBuf;

/// An agent that, whatever it is asked and wherever it stands, tries its
/// whole menu with nothing in its hands.
fn an_agent_that_tries_everything() -> Value {
    json!({ "turns": [
        { "scope": "work_item", "tools": "every", "say": ["I tried every tool a worker has."] },
        { "scope": "goal", "tools": "every", "say": ["I tried every tool a designer has."] },
        { "scope": "conversation", "tools": "every", "say": ["I tried every tool a turn has."] },
    ]})
}

/// What a session was handed for the platform's server: the words it is
/// started with.
fn server_words(opened: &Value) -> Vec<String> {
    opened["servers"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|server| server["name"] == "bisa")
        .and_then(|server| server["args"].as_array())
        .unwrap_or_else(|| panic!("the session was handed the platform's server: {opened}"))
        .iter()
        .map(|word| word.as_str().unwrap_or_default().to_string())
        .collect()
}

/// The menu the scope those words name is promised: the router's own, built
/// as `bisa agent-context` builds it — no socket is opened to list it.
fn promised(words: &[String]) -> BTreeSet<String> {
    let named = |flag: &str| {
        words
            .iter()
            .position(|word| word == flag)
            .and_then(|at| words.get(at + 1))
            .cloned()
    };
    let scope = match (
        named("--work-item"),
        named("--goal"),
        named("--conversation"),
    ) {
        (Some(item), _, _) => Scope::WorkItem(item),
        (None, goal, Some(scope)) => Scope::Conversation {
            scope,
            agent: named("--agent").expect("a turn says who speaks"),
            goal,
        },
        (None, Some(goal), None) => Scope::Goal {
            goal,
            agent: named("--agent"),
        },
        (None, None, None) => panic!("the server was handed no scope: {words:?}"),
    };
    BisaServer::new(PathBuf::from("/nonexistent/the-journeys.sock"), scope)
        .tool_manifest()
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

/// One session of a scope, once it has tried its menu: what it was handed,
/// what its server listed, and what every call came to.
struct Tried {
    scope: &'static str,
    words: Vec<String>,
    listed: BTreeSet<String>,
    calls: Vec<Value>,
}

fn tried(ws: &Sealed, scope: &'static str) -> Tried {
    let opened = ws.until("a session of the scope", || {
        ws.recorded("session_new")
            .into_iter()
            .find(|opened| opened["scope"] == scope)
    });
    let session = opened["session"].clone();
    let started = ws.until("its server to have started", || {
        ws.recorded("server_started")
            .into_iter()
            .find(|started| started["session"] == session)
    });
    let listed: BTreeSet<String> = started["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("the server listed its tools: {started}"))
        .iter()
        .map(|name| name.as_str().unwrap_or_default().to_string())
        .collect();
    // The turn is over once the agent said so: every call before it is in
    // the record.
    ws.until("the agent to have tried its whole menu", || {
        let calls: Vec<Value> = ws
            .recorded("tool")
            .into_iter()
            .filter(|call| call["session"] == session)
            .collect();
        (calls.len() == listed.len()).then_some(())
    });
    let calls = ws
        .recorded("tool")
        .into_iter()
        .filter(|call| call["session"] == session)
        .collect();
    Tried {
        scope,
        words: server_words(&opened),
        listed,
        calls,
    }
}

impl Tried {
    fn call(&self, tool: &str) -> &Value {
        self.calls
            .iter()
            .find(|call| call["name"] == tool)
            .unwrap_or_else(|| panic!("{tool} was not called in the {} scope", self.scope))
    }

    /// What a call was answered, in words: the result's text, or the
    /// refusal's message.
    fn said(&self, tool: &str) -> String {
        let result = &self.call(tool)["result"];
        let text: Vec<&str> = result["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|block| block["text"].as_str())
            .collect();
        match result["error"].as_str() {
            Some(refusal) => refusal.to_string(),
            None => text.join("\n"),
        }
    }

    fn holds(&self) {
        let scope = self.scope;
        // --- the menu: what the wire lists is what the scope is promised --
        let promised = promised(&self.words);
        assert_eq!(
            self.listed, promised,
            "the {scope} scope's server lists the menu it is promised"
        );
        assert!(
            self.listed.len() > 20,
            "a menu, not a stub: {:?}",
            self.listed
        );
        // --- every tool was called, once, and every call was answered ------
        let called: BTreeSet<String> = self
            .calls
            .iter()
            .map(|call| call["name"].as_str().unwrap_or_default().to_string())
            .collect();
        assert_eq!(called, self.listed, "every tool of the {scope} menu");
        for call in &self.calls {
            let tool = call["name"].as_str().unwrap_or_default();
            let said = self.said(tool);
            assert!(
                !said.contains("the server ended") && !said.contains("stopped listening"),
                "{tool} took the server with it in the {scope} scope: {said}"
            );
            assert!(
                !said.contains("panicked"),
                "{tool} panicked in the {scope} scope: {said}"
            );
            // A tool with nothing in its hands reads, or says what it
            // needs: an answer with no word in it is neither — but for the
            // hook, whose empty answer is *no objection*.
            if tool != "_Stop" {
                assert!(
                    !said.trim().is_empty(),
                    "{tool} answered nothing in the {scope} scope: {call}"
                );
            }
        }
        // --- on every menu, and answered the same wherever it is called ----
        assert_eq!(
            self.call("_Stop")["failed"],
            false,
            "the hook objects to nothing"
        );
        assert_eq!(self.said("_Stop"), "");
        assert_eq!(
            self.call("mobile_development_status")["failed"],
            true,
            "mobile development is off as a first launch leaves it, and the tool says so \
             before it looks at this machine: {}",
            self.said("mobile_development_status")
        );
        assert_eq!(
            self.call("browser_tabs")["failed"],
            true,
            "no desktop is open, and the tool says so at once: {}",
            self.said("browser_tabs")
        );
        assert_eq!(
            self.call("list_connectors")["failed"],
            false,
            "the connectors are every session's to read — a worker's too: {}",
            self.said("list_connectors")
        );
        assert_eq!(
            self.call("post_message")["failed"],
            true,
            "nothing is said by a call that says nothing: {}",
            self.said("post_message")
        );
    }
}

#[test]
fn every_tool_a_session_is_handed_answers_an_agent_with_nothing_in_its_hands() {
    let mut ws = Sealed::with_script(&an_agent_that_tries_everything());
    // A worker of the workspace's own, on the scripted harness.
    ws.ok(&[
        "agent",
        "add",
        "--name",
        "Writer",
        "--harness",
        AGENT_HARNESS,
        "--prompt",
        "You write what you are asked to.",
    ]);
    let one_step = ws.file(
        "one-step.json",
        &json!({
            "name": "One step of work",
            "steps": [{
                "id": "write", "name": "Write", "kind": "agent",
                "instructions": "Write it.", "assignee": { "agent": "writer" },
            }],
        })
        .to_string(),
    );
    let made = ws.json(&["workflow", "new", "--from", &one_step.to_string_lossy()]);
    assert_eq!(made["problems"], json!([]), "{made}");
    let workflow = made["workflow"]["id"].as_str().expect("its id").to_string();
    ws.start();

    // --- a worker on a step's item ---------------------------------------------------
    // The run ends failed — its worker yielded nothing — and the verb says
    // so; what is read here is what the worker's tools answered.
    ws.bisa(&["workflow", "run", &workflow]);
    let worker = tried(&ws, "work_item");
    worker.holds();
    for tool in ["get_run", "yield_result"] {
        assert!(worker.listed.contains(tool), "a worker's own: {tool}");
    }
    assert_eq!(
        worker.call("get_run")["failed"],
        false,
        "{}",
        worker.said("get_run")
    );
    assert!(
        worker.said("get_run").contains(&workflow),
        "the run it works in, read through the tool: {}",
        worker.said("get_run")
    );
    assert_eq!(
        worker.call("yield_result")["failed"],
        true,
        "a result with nothing in it is no result: {}",
        worker.said("yield_result")
    );

    // --- the Workflow Agent designing a goal -----------------------------------------
    let goal = ws.json(&["new", "hang the door"])["goal"]
        .as_str()
        .expect("the goal")
        .to_string();
    let designer = tried(&ws, "goal");
    designer.holds();
    assert!(designer.listed.contains("propose_workflow"));
    assert_eq!(
        designer.call("propose_workflow")["failed"],
        true,
        "nothing proposed is no proposal: {}",
        designer.said("propose_workflow")
    );
    assert_eq!(designer.call("get_goal")["failed"], false);
    assert!(
        designer.said("get_goal").contains("hang the door"),
        "the goal it designs, read through the tool: {}",
        designer.said("get_goal")
    );
    ws.until("the design to stall, nothing having been proposed", || {
        let (_, view) = ws.call("GET", &format!("/goals/{goal}"), &[], None);
        (view["guidance"]["design"]["status"] == "stalled").then_some(())
    });
    assert_eq!(ws.json(&["status", &goal])["status"], "draft");

    // --- an agent's turn in a conversation ---------------------------------------------
    let (status, talk) = ws.call(
        "POST",
        "/conversations",
        &[],
        Some(&json!({ "origin": { "kind": "workspace" } })),
    );
    assert_eq!(status, 201, "{talk}");
    let talk = talk["conversation"]["id"]
        .as_str()
        .expect("its id")
        .to_string();
    let (status, said) = ws.call(
        "POST",
        &format!("/conversations/{talk}/messages"),
        &[],
        Some(&json!({ "content": "what can you do?" })),
    );
    assert_eq!(status, 200, "{said}");
    let turn = tried(&ws, "conversation");
    turn.holds();
    // The General Agent's own, which no other turn is handed.
    for tool in ["workspace_overview", "list_staff", "capture_goal"] {
        assert!(
            turn.listed.contains(tool),
            "the General Agent's own: {tool}"
        );
    }
    assert_eq!(turn.call("list_staff")["failed"], false);
    assert!(
        turn.said("list_staff").contains("writer"),
        "the roster, read through the tool: {}",
        turn.said("list_staff")
    );
    assert_eq!(
        turn.call("capture_goal")["failed"],
        true,
        "a goal of no words is no goal: {}",
        turn.said("capture_goal")
    );

    // --- three menus, one common set ----------------------------------------------------
    let common: BTreeSet<&String> = worker
        .listed
        .iter()
        .filter(|tool| designer.listed.contains(*tool) && turn.listed.contains(*tool))
        .collect();
    for tool in [
        "ask_human",
        "post_message",
        "recall_list",
        "decide",
        "_Stop",
    ] {
        assert!(
            common.iter().any(|name| name.as_str() == tool),
            "on every menu: {tool}"
        );
    }
    assert!(
        !designer.listed.contains("yield_result") && !turn.listed.contains("yield_result"),
        "only a worker yields a result"
    );
    assert!(
        !worker.listed.contains("propose_workflow"),
        "a worker designs nobody's goal"
    );

    // --- empty hands made nothing -------------------------------------------------------
    assert_eq!(ws.json(&["project", "list"])["projects"], json!([]));
    let (_, goals) = ws.call("GET", "/goals", &[], None);
    assert_eq!(
        goals["goals"].as_array().map(Vec::len),
        Some(1),
        "the one goal a person captured, and none an agent did: {goals}"
    );
    let (_, asks) = ws.call("GET", "/inbox", &[], None);
    assert!(
        !asks.to_string().contains("ask_human"),
        "nobody was asked a question of no words: {asks}"
    );
    ws.stop();
}
