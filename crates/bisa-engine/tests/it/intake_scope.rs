//! The envelope as the real MCP client builds it, against the real intake:
//! a conversation session hands its scope id out as its `goal` candidate on
//! every request (`bisa_mcp::Scope::apply`), and a channel's or a DM's id is
//! no ULID — so every op that takes a candidate must read it as one
//! (`GoalCandidate`) and never fail the envelope. What the fake engine of
//! `bisa-mcp`'s tests and the hand-built envelopes of `browser.rs` both
//! missed, pinned here from both sides at once.

use crate::common::{engine_with, intake_roundtrip, until};
use bisa_core::browser::PLATFORM_FAULT;
use bisa_core::{ConversationOrigin, GoalMode};
use bisa_engine::browser::{BrowserHome, BrowserHomeScope, BrowserResult, NOBODY_HOME};
use bisa_engine::SubmitRequest;
use bisa_mcp::Scope;
use bisa_store::NewConversation;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// The words a reply must never start with: an envelope the engine could
/// not read. A refusal in the op's own words is fine — a channel has no
/// goal, the Decision-Making Agent is off, nobody is home — a fault is not.
fn is_fault(reply: &Value) -> bool {
    reply["errors"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .any(|e| e.starts_with(PLATFORM_FAULT) || e.starts_with("bad request"))
}

/// A conversation session's envelope for `op`, exactly as the client sends
/// it: the scope id rides as the candidate `goal`.
fn in_conversation(scope: &str, mut op: Value) -> Value {
    Scope::Conversation {
        scope: scope.to_string(),
        agent: "general-agent".to_string(),
        goal: None,
    }
    .apply_for_test(&mut op);
    assert_eq!(op["goal"], json!(scope), "the scope id is the candidate");
    op
}

/// Every op that reads a goal candidate, with the least body it needs.
fn candidate_ops() -> Vec<(&'static str, Value)> {
    vec![
        (
            "browser",
            json!({"op": "browser", "request": {"action": "tabs"}}),
        ),
        (
            "decide",
            json!({"op": "decide", "request": {"state": "a page", "questions": {"fits": {"type": "noul", "instructions": "is it about AI?"}}}}),
        ),
        (
            "post_message",
            json!({"op": "post_message", "content": "from the channel"}),
        ),
        (
            "emit_signal",
            json!({"op": "emit_signal", "name": "post.drafted"}),
        ),
    ]
}

#[tokio::test(flavor = "multi_thread")]
async fn a_channel_scoped_session_never_fails_the_envelope_on_any_op_that_takes_a_goal_candidate() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    let socket = engine.socket_path().to_path_buf();
    // No desktop has read the list: a browser op answers nobody home at once.
    for (op, body) in candidate_ops() {
        let reply = intake_roundtrip(&socket, in_conversation("general", body)).await;
        assert!(!is_fault(&reply), "{op} in a channel: {reply}");
    }
    let browser = intake_roundtrip(
        &socket,
        in_conversation(
            "general",
            json!({"op": "browser", "request": {"action": "tabs"}}),
        ),
    )
    .await;
    assert_eq!(browser["ok"], json!(true), "{browser}");
    assert_eq!(
        browser["result"]["error"],
        json!(NOBODY_HOME),
        "the refusal is the browser's own sentence, not the envelope's"
    );
    let posted = intake_roundtrip(
        &socket,
        in_conversation(
            "general",
            json!({"op": "post_message", "content": "from the channel"}),
        ),
    )
    .await;
    assert_eq!(
        posted["ok"],
        json!(true),
        "a channel's turn posts: {posted}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_direct_message_and_a_conversation_about_nothing_are_candidates_that_name_no_goal() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    let ws = engine.workspace();
    let socket = engine.socket_path().to_path_buf();
    let scout = ws.add_agent(crate::browser::scout(vec![])).unwrap();
    let dm = ws.open_dm(std::slice::from_ref(&scout.pubkey)).unwrap();
    let about_workspace = ws
        .create_conversation(NewConversation {
            origin: ConversationOrigin::Workspace,
            title: None,
            mode: bisa_core::ConversationMode::Auto,
        })
        .unwrap();
    for scope in [dm.id.to_string(), about_workspace.id.to_string()] {
        for (op, body) in candidate_ops() {
            let reply = intake_roundtrip(&socket, in_conversation(&scope, body)).await;
            assert!(!is_fault(&reply), "{op} in {scope}: {reply}");
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_conversation_about_a_goal_names_that_goal_through_its_candidate_alone() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    let ws = engine.workspace();
    // A desktop is home, so the request parks and its home can be read.
    engine.inner().browser.pending();
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: GoalMode::Guided,
            ..SubmitRequest::captured("Write the post")
        })
        .unwrap();
    let about_goal = ws
        .create_conversation(NewConversation {
            origin: ConversationOrigin::Goal { id: goal.id },
            title: None,
            mode: bisa_core::ConversationMode::Auto,
        })
        .unwrap();
    // No `scope` on the wire: the candidate is all the engine has, and it is
    // a conversation's id, not a goal's — resolved through the origin.
    let op =
        json!({"op": "browser", "goal": about_goal.id.to_string(), "request": {"action": "tabs"}});
    let socket = engine.socket_path().to_path_buf();
    let call = tokio::spawn(async move { intake_roundtrip(&socket, op).await });
    let pending = until("a parked request", || {
        let p = engine.inner().browser.pending();
        (!p.is_empty()).then_some(p)
    })
    .await;
    assert_eq!(
        pending[0].scope.home,
        Some(BrowserHome::new(BrowserHomeScope::Goal, goal.id)),
        "a conversation about a goal is at home beside the goal"
    );
    engine
        .inner()
        .browser
        .answer(&pending[0].id, BrowserResult::refused("enough"));
    let reply = call.await.unwrap();
    assert!(!is_fault(&reply), "{reply}");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_op_that_requires_a_goal_still_refuses_a_slug_in_its_own_words() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    let socket = engine.socket_path().to_path_buf();
    let reply = intake_roundtrip(
        &socket,
        in_conversation("general", json!({"op": "get_goal"})),
    )
    .await;
    assert_eq!(reply["ok"], json!(false));
    assert_eq!(reply["errors"], json!(["goal is not a ULID"]), "{reply}");
    let unknown = intake_roundtrip(
        &socket,
        json!({"op": "get_goal", "goal": ulid::Ulid::from_datetime(std::time::SystemTime::now()).to_string()}),
    )
    .await;
    assert_eq!(unknown["errors"], json!(["unknown goal"]), "{unknown}");
}

/// One line on the socket, as bytes — for an envelope that is not even JSON.
async fn raw_roundtrip(socket: &std::path::Path, line: &str) -> Value {
    let stream = tokio::net::UnixStream::connect(socket).await.unwrap();
    let (read, mut write) = stream.into_split();
    write
        .write_all(format!("{line}\n").as_bytes())
        .await
        .unwrap();
    write.flush().await.unwrap();
    let mut reply = String::new();
    BufReader::new(read).read_line(&mut reply).await.unwrap();
    serde_json::from_str(reply.trim()).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn an_envelope_the_engine_cannot_read_is_a_platform_fault_naming_the_op() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    let socket = engine.socket_path().to_path_buf();
    // A browser op with no request at all.
    let reply = intake_roundtrip(&socket, json!({"op": "browser", "scope": "general"})).await;
    assert_eq!(reply["ok"], json!(false));
    let words = reply["errors"][0].as_str().unwrap();
    assert!(words.starts_with(PLATFORM_FAULT), "{words}");
    assert!(words.contains("`browser`"), "the op is named: {words}");
    assert!(
        words.contains("not a policy refusal") && words.contains("continue"),
        "{words}"
    );
    assert!(!words.contains("general"), "the body is not quoted back");
    // An op nobody knows.
    let unknown = intake_roundtrip(&socket, json!({"op": "teleport"})).await;
    let words = unknown["errors"][0].as_str().unwrap();
    assert!(
        words.starts_with(PLATFORM_FAULT) && words.contains("`teleport`"),
        "{words}"
    );
    // Not JSON at all: no op to name.
    let garbage = raw_roundtrip(&socket, "not json").await;
    let words = garbage["errors"][0].as_str().unwrap();
    assert!(
        words.starts_with(PLATFORM_FAULT) && words.contains("this request ("),
        "{words}"
    );
}

/// A request line over the intake's limit is refused in words that name the
/// limit — a refusal the caller can act on, never a platform fault — and the
/// door closes behind it: what followed the cut line is never read as a
/// request.
#[tokio::test(flavor = "multi_thread")]
async fn a_request_line_over_the_limit_is_refused_naming_the_limit_and_the_door_closes() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    let socket = engine.socket_path().to_path_buf();
    let limit = bisa_engine::intake::MAX_REQUEST_LINE_BYTES;
    let stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
    let (read, mut write) = stream.into_split();
    let mut line = String::from(r#"{"op":"get_goal","goal":""#);
    line.push_str(&"x".repeat(limit));
    line.push_str("\"}\n{\"op\":\"teleport\"}\n");
    write.write_all(line.as_bytes()).await.unwrap();
    write.flush().await.unwrap();
    let mut reader = BufReader::new(read);
    let mut reply = String::new();
    reader.read_line(&mut reply).await.unwrap();
    let reply: Value = serde_json::from_str(reply.trim()).unwrap();
    assert_eq!(reply["ok"], json!(false), "{reply}");
    let words = reply["errors"][0].as_str().unwrap();
    assert!(
        words.contains(&limit.to_string()),
        "the limit is named: {words}"
    );
    assert!(
        !words.starts_with(PLATFORM_FAULT),
        "a refusal, not a platform fault: {words}"
    );
    let mut more = String::new();
    assert_eq!(
        reader.read_line(&mut more).await.unwrap(),
        0,
        "the door closed; the line after the cut one was never answered: {more}"
    );
    engine.shutdown().await;
}

/// A worker of a run of the workspace orients with `get_run` — its run, the
/// run's own items, the ceiling it spends against — and is refused
/// `get_goal` by name; what it writes lands on the run: a note on the run's
/// journal, a question in the Inbox under the run's home, answered through
/// the run even once the live gate is gone, and never answered twice.
#[tokio::test(flavor = "multi_thread")]
async fn a_worker_of_a_workspace_run_orients_with_get_run_and_writes_to_the_run() {
    use crate::common::{agent_step, items_at, new_workflow, notes_at, workspace_run};
    let dir = tempfile::tempdir().unwrap();
    // A session that starts and never ends: its item stays in progress.
    let quiet = bisa_harness::mock::MockAdapter {
        id: "mock".into(),
        script: Some(vec![bisa_harness::SessionEvent::Lifecycle(
            bisa_harness::LifecycleEvent::Started,
        )]),
        ..Default::default()
    };
    let engine = engine_with(&dir, vec![quiet]);
    let socket = engine.socket_path().to_path_buf();
    let (_, run) = workspace_run(
        &engine,
        new_workflow("nightly", vec![agent_step("build", "mock")]),
    );
    let home = run.home();
    let item = until("the run's item in progress", || {
        items_at(&engine, &home).into_iter().find(|i| {
            matches!(
                i.state,
                bisa_core::workitem::WorkItemState::InProgress { .. }
            )
        })
    })
    .await;

    let oriented = intake_roundtrip(
        &socket,
        json!({"op": "get_run", "work_item": item.id.to_string()}),
    )
    .await;
    assert_eq!(oriented["ok"], json!(true), "{oriented}");
    assert_eq!(oriented["run"]["id"], json!(run.id.to_string()));
    assert_eq!(oriented["run"]["scope"], json!("workspace"));
    assert_eq!(oriented["run"]["steps"][0]["id"], json!("build"));
    assert_eq!(oriented["home"], json!(format!("run:{}", run.id)));
    assert_eq!(
        oriented["goal"],
        Value::Null,
        "a run of the workspace has no goal"
    );
    assert!(oriented["budget"].is_object(), "{oriented}");
    assert_eq!(oriented["work_items"].as_array().unwrap().len(), 1);
    let by_id =
        intake_roundtrip(&socket, json!({"op": "get_run", "run": run.id.to_string()})).await;
    assert_eq!(by_id["run"], oriented["run"], "{by_id}");
    for (body, words) in [
        (
            json!({"op": "get_run"}),
            "request needs `work_item` or `run`",
        ),
        (
            json!({"op": "get_run", "run": "nightly"}),
            "run is not a ULID",
        ),
        (
            json!({"op": "get_run", "run": ulid::Ulid::from_parts(3, 3).to_string()}),
            "unknown run",
        ),
    ] {
        let reply = intake_roundtrip(&socket, body).await;
        assert_eq!(reply["errors"], json!([words]), "{reply}");
    }

    let refused = intake_roundtrip(
        &socket,
        json!({"op": "get_goal", "work_item": item.id.to_string()}),
    )
    .await;
    assert_eq!(refused["ok"], json!(false));
    let words = refused["errors"][0].as_str().unwrap();
    assert!(
        words.contains("get_run"),
        "the refusal names the op to use: {words}"
    );

    let noted = intake_roundtrip(
        &socket,
        json!({"op": "add_note", "work_item": item.id.to_string(), "text": "halfway there"}),
    )
    .await;
    assert_eq!(noted["ok"], json!(true), "{noted}");
    assert!(notes_at(&engine, &home).contains(&"halfway there".to_string()));

    let asked = intake_roundtrip(
        &socket,
        json!({"op": "ask_human", "work_item": item.id.to_string(),
               "question": "which region?", "expects": "answer"}),
    )
    .await;
    assert_eq!(asked["ok"], json!(true), "{asked}");
    let gate_id = asked["gate"].as_str().unwrap().to_string();
    let gate = engine
        .inbox()
        .into_iter()
        .find(|g| g.id == gate_id)
        .expect("the question is in the Inbox");
    assert_eq!(gate.home, home, "asked on the run, not on a goal");
    // A restarted process has no live gate: the answer goes through the run.
    engine.inner().gates.withdraw(&gate_id);
    let out = engine
        .decide_durable(
            &home,
            true,
            None,
            Some(&bisa_core::Answer::text("Europe")),
            None,
            None,
            None,
        )
        .unwrap();
    assert_eq!(out.home, home);
    assert_eq!(out.gate, bisa_core::Gate::Escalation);
    let again = engine
        .decide_durable(
            &home,
            true,
            None,
            Some(&bisa_core::Answer::text("Europe")),
            None,
            None,
            None,
        )
        .unwrap_err();
    assert!(
        again.to_string().contains("no pending gate"),
        "answered once: {again}"
    );
    engine.shutdown().await;
}
