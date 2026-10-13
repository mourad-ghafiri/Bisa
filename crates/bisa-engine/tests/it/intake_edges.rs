//! The intake socket at its edges, op by op: what a session is refused in
//! words — a scope it names wrongly, a thing that is not there, a question
//! shaped wrongly — and the answers the engine gives beside the happy paths
//! the other suites walk. Everything here goes through the engine's own
//! socket, the way a harness session's tools do.

use crate::common::{self, intake_roundtrip};
use bisa_core::{ClosureReason, DecisionAnswer, GoalId, OwnerScope, StepKind};
use bisa_decision::ScriptedProvider;
use bisa_engine::Engine;
use bisa_store::NewNote;
use serde_json::{json, Value};
use std::sync::Arc;

fn engine(dir: &tempfile::TempDir) -> Engine {
    common::engine_with(dir, vec![common::yielding("mock", json!({"ok": true}))])
}

async fn op(engine: &Engine, req: Value) -> Value {
    intake_roundtrip(engine.socket_path(), req).await
}

/// The refusal's words, joined.
fn refused(reply: &Value) -> String {
    assert_eq!(reply["ok"], false, "a refusal: {reply}");
    reply["errors"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join("; ")
}

/// The script's replies once it has recorded `n` of them: a run finishes on
/// the reply that ends its item, a moment before the script's last ops land.
async fn replies_until(script: &bisa_harness::mock::IntakeScript, n: usize) -> Vec<Value> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let replies = script.replies();
        if replies.len() >= n || std::time::Instant::now() > deadline {
            assert_eq!(replies.len(), n, "{replies:?}");
            return replies;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

fn goal(engine: &Engine) -> GoalId {
    engine
        .submit_goal(common::guided("ship the release"))
        .unwrap()
        .id
}

/// A ULID nobody recorded anything under.
const NOBODY: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

/// A scope, a thing or an id that is not there is refused in the op's own
/// words, never as an envelope the engine could not read.
#[tokio::test(flavor = "multi_thread")]
async fn what_is_not_there_is_refused_in_words() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let g = goal(&engine).to_string();

    assert!(refused(
        &op(
            &engine,
            json!({"op": "progress", "work_item": NOBODY, "verb": "v", "object": "o"})
        )
        .await
    )
    .contains("unknown work item"));
    assert!(refused(&op(&engine, json!({"op": "get_goal"})).await)
        .contains("needs `work_item` or `goal`"));
    assert!(
        refused(&op(&engine, json!({"op": "get_goal", "goal": "nope"})).await)
            .contains("not a ULID")
    );
    assert!(
        refused(&op(&engine, json!({"op": "get_goal", "goal": NOBODY})).await)
            .contains("unknown goal")
    );
    assert!(refused(
        &op(
            &engine,
            json!({"op": "add_note", "goal": NOBODY, "text": "x"})
        )
        .await
    )
    .contains("unknown goal"));
    assert!(
        refused(&op(&engine, json!({"op": "get_run", "work_item": NOBODY})).await)
            .contains("unknown work item")
    );
    assert!(refused(
        &op(
            &engine,
            json!({"op": "result_submit", "work_item": NOBODY, "output": {}})
        )
        .await
    )
    .contains("unknown work item"));
    assert!(refused(
        &op(
            &engine,
            json!({"op": "spawn_sub_goal", "goal": "nope", "statement": "child"})
        )
        .await
    )
    .contains("not a ULID"));
    assert!(refused(&op(&engine, json!({"op": "validate_workflow", "agent": "workflow-agent", "goal": NOBODY, "workflow": {"name": "w", "steps": []}})).await).contains("unknown goal"));
    assert!(
        refused(&op(&engine, json!({"op": "list_staff", "agent": "nobody"})).await)
            .contains("nobody")
    );
    assert!(refused(&op(&engine, json!({"op": "install_catalog_entry", "agent": "general-agent", "kind": "nonsense", "slug": "x"})).await).contains("nonsense"));
    assert!(refused(&op(&engine, json!({"op": "install_catalog_entry", "agent": "general-agent", "kind": "agent", "slug": "x", "goal": "nope"})).await).contains("not a ULID"));
    assert!(refused(&op(&engine, json!({"op": "assign", "agent": "general-agent", "goal": "nope", "assignees": ["agent:developer"]})).await).contains("not a ULID"));
    assert!(refused(&op(&engine, json!({"op": "assign", "agent": "general-agent", "goal": g, "project": "nope", "assignees": ["agent:general-agent"]})).await).contains("project is not a ULID"));
    assert!(refused(&op(&engine, json!({"op": "assign", "agent": "general-agent", "goal": g, "project": NOBODY, "assignees": ["agent:general-agent"]})).await).contains("unknown project"));
    assert!(refused(
        &op(
            &engine,
            json!({"op": "revise_statement", "goal": "nope", "statement": "s"})
        )
        .await
    )
    .contains("not a ULID"));
    assert!(refused(&op(&engine, json!({"op": "propose_workflow", "goal": NOBODY, "agent": "workflow-agent", "workflow": {"name": "w", "steps": []}})).await).contains("unknown goal"));
    assert!(refused(&op(&engine, json!({"op": "amend_workflow", "goal": NOBODY, "agent": "workflow-agent", "workflow": {"name": "w", "steps": []}})).await).contains("unknown goal"));
    assert!(refused(
        &op(
            &engine,
            json!({"op": "create_project", "agent": "general-agent", "goal": "nope", "slug": "web"})
        )
        .await
    )
    .contains("not a ULID"));
    assert!(refused(&op(&engine, json!({"op": "create_project", "agent": "general-agent", "goal": g, "slug": "web", "assignees": ["nobody:x"]})).await).contains("nobody:x"));
    assert!(refused(&op(&engine, json!({"op": "create_project", "agent": "general-agent", "goal": g, "slug": "web", "assignees": ["agent:nobody"]})).await).contains("nobody"));
    let made = op(
        &engine,
        json!({"op": "create_project", "agent": "general-agent", "goal": g, "slug": "web"}),
    )
    .await;
    assert_eq!(made["ok"], true, "{made}");
    assert!(
        !refused(
            &op(
                &engine,
                json!({"op": "create_project", "agent": "general-agent", "goal": g, "slug": "web"})
            )
            .await
        )
        .is_empty(),
        "the slug is taken"
    );
    assert!(refused(
        &op(
            &engine,
            json!({"op": "call_connector", "connector": "Not A Slug", "operation": "list"})
        )
        .await
    )
    .contains("not a connector slug"));
    assert!(refused(
        &op(
            &engine,
            json!({"op": "call_connector", "connector": "jira", "operation": "Not An Op"})
        )
        .await
    )
    .contains("not an operation id"));
    assert!(
        !refused(
            &op(
                &engine,
                json!({"op": "call_connector", "connector": "jira", "operation": "list"})
            )
            .await
        )
        .is_empty(),
        "no such connector"
    );
}

/// A question is shaped before it reaches a person; a gate is answered
/// once; a resolved gate answers at once; a signal names its scope.
#[tokio::test(flavor = "multi_thread")]
async fn questions_gates_and_signals_at_their_edges() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let g = goal(&engine).to_string();

    assert!(refused(
        &op(
            &engine,
            json!({"op": "ask_human", "goal": NOBODY, "question": "?"})
        )
        .await
    )
    .contains("unknown goal"));
    let said = refused(&op(&engine, json!({"op": "ask_human", "goal": g, "question": "?", "expects": "decision", "options": [{"id": "a", "label": "A"}]})).await);
    assert!(said.contains("approve/decline"), "{said}");
    assert!(refused(
        &op(
            &engine,
            json!({"op": "ask_human", "goal": g, "question": "?", "expects": "weird"})
        )
        .await
    )
    .contains("expects must be"));
    assert!(
        refused(&op(&engine, json!({"op": "await_decision", "gate": "nobody"})).await)
            .contains("unknown gate")
    );

    let asked = op(
        &engine,
        json!({"op": "ask_human", "goal": g, "question": "ship it?", "expects": "decision"}),
    )
    .await;
    let gate = asked["gate"].as_str().expect("a gate").to_string();
    engine.decide(&gate, true, None, None, None).unwrap();
    let resolved = op(&engine, json!({"op": "await_decision", "gate": gate})).await;
    assert_eq!(resolved["ok"], true, "{resolved}");
    assert_eq!(resolved["approve"], true, "{resolved}");

    let raised = op(&engine, json!({"op": "emit_signal", "goal": g, "name": "post.drafted", "signal_scope": "workspace"})).await;
    assert_eq!(raised["ok"], true, "{raised}");
    assert!(refused(&op(&engine, json!({"op": "emit_signal", "goal": g, "name": "post.drafted", "signal_scope": "goal:nope"})).await).contains("not a goal id"));
    assert!(refused(&op(&engine, json!({"op": "emit_signal", "goal": g, "name": "post.drafted", "signal_scope": "nonsense"})).await).contains("must be `workspace` or `goal:<ulid>`"));
    let own = op(
        &engine,
        json!({"op": "emit_signal", "goal": g, "name": "post.drafted"}),
    )
    .await;
    assert_eq!(own["ok"], true, "the goal's own scope: {own}");
}

/// Review notes are read from the projects a session can see, and a note
/// that is no note is said so; recall needs an identity, keeps a hash and
/// refuses a stale write with the value that stands.
#[tokio::test(flavor = "multi_thread")]
async fn review_notes_and_recall_at_their_edges() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let g = goal(&engine).to_string();

    assert!(refused(
        &op(
            &engine,
            json!({"op": "review_notes_list", "goal": g, "project": "nope"})
        )
        .await
    )
    .contains("project is not a ULID"));
    assert!(refused(
        &op(
            &engine,
            json!({"op": "review_notes_list", "goal": g, "project": NOBODY})
        )
        .await
    )
    .contains("unknown project"));
    let none = op(&engine, json!({"op": "review_notes_list", "goal": g})).await;
    assert_eq!(none["ok"], true, "{none}");
    assert_eq!(none["notes"], json!([]));
    assert!(refused(
        &op(
            &engine,
            json!({"op": "review_notes_list", "scope": "general"})
        )
        .await
    )
    .contains("pass `project`"));
    assert!(refused(
        &op(
            &engine,
            json!({"op": "review_note_resolve", "note": "x", "goal": g})
        )
        .await
    )
    .contains("not a review note id"));
    assert!(refused(
        &op(
            &engine,
            json!({"op": "review_note_resolve", "note": NOBODY, "goal": g})
        )
        .await
    )
    .contains("no such review note"));

    assert!(refused(
        &op(
            &engine,
            json!({"op": "recall_store", "slug": "style", "value": "tabs"})
        )
        .await
    )
    .contains("agent identity"));
    assert!(
        refused(&op(&engine, json!({"op": "recall_get", "slug": "style"})).await)
            .contains("agent identity")
    );
    assert!(refused(&op(&engine, json!({"op": "recall_list"})).await).contains("agent identity"));
    let stored = op(
        &engine,
        json!({"op": "recall_store", "agent": "general-agent", "slug": "style", "value": "tabs"}),
    )
    .await;
    assert_eq!(stored["ok"], true, "{stored}");
    let stale = op(&engine, json!({"op": "recall_store", "agent": "general-agent", "slug": "style", "value": "spaces", "base_hash": "wrong"})).await;
    assert_eq!(stale["ok"], false, "{stale}");
    assert_eq!(stale["current_value"], "tabs", "{stale}");
    assert_eq!(stale["current_hash"], stored["hash"], "{stale}");
    let got = op(
        &engine,
        json!({"op": "recall_get", "agent": "general-agent", "slug": "style"}),
    )
    .await;
    assert_eq!(got["value"], "tabs", "{got}");
    let missing = op(
        &engine,
        json!({"op": "recall_get", "agent": "general-agent", "slug": "nothing"}),
    )
    .await;
    assert_eq!(missing["ok"], true);
    assert!(missing["value"].is_null(), "{missing}");
    let listed = op(
        &engine,
        json!({"op": "recall_list", "agent": "general-agent"}),
    )
    .await;
    assert_eq!(listed["records"][0]["slug"], "style", "{listed}");
}

/// A note is read and written by its id — a stale write names the hash
/// that stands — and a session in no conversation about one must name it.
#[tokio::test(flavor = "multi_thread")]
async fn notes_at_their_edges() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let note = engine
        .workspace()
        .create_note(NewNote {
            scope: OwnerScope::Workspace,
            title: "Plan".into(),
            body: "# Plan\n".into(),
        })
        .unwrap();
    let id = note.id.to_string();
    assert!(refused(&op(&engine, json!({"op": "note_read"})).await).contains("No note given"));
    assert!(
        refused(&op(&engine, json!({"op": "note_read", "note": "nope"})).await)
            .contains("not a note id")
    );
    assert!(
        refused(&op(&engine, json!({"op": "note_read", "note": NOBODY})).await).contains("not")
    );
    let read = op(&engine, json!({"op": "note_read", "note": id})).await;
    assert_eq!(read["title"], "Plan", "{read}");
    let hash = read["hash"].as_str().unwrap().to_string();
    let stale = refused(
        &op(
            &engine,
            json!({"op": "note_write", "note": id, "text": "# Other\n", "base_hash": "wrong"}),
        )
        .await,
    );
    assert!(stale.contains(&hash), "the hash that stands: {stale}");
    let written = op(
        &engine,
        json!({"op": "note_write", "note": id, "text": "# Other\n", "base_hash": hash}),
    )
    .await;
    assert_eq!(written["ok"], true, "{written}");
    let appended = op(
        &engine,
        json!({"op": "note_append", "note": id, "text": "more", "agent": "general-agent"}),
    )
    .await;
    assert_eq!(appended["title"], "Plan", "{appended}");
    assert!(refused(
        &op(
            &engine,
            json!({"op": "note_append", "note": NOBODY, "text": "more"})
        )
        .await
    )
    .contains("not"));
}

/// The goal's own words: a statement revised with or without a reason, never
/// on a closed goal; a message with attachments needs an agent and a folder,
/// and a mention nobody resolves is refused.
#[tokio::test(flavor = "multi_thread")]
async fn statements_and_messages_at_their_edges() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let g = goal(&engine);
    let revised = op(&engine, json!({"op": "revise_statement", "goal": g.to_string(), "statement": "ship the release by Friday"})).await;
    assert_eq!(revised["ok"], true, "{revised}");
    assert!(refused(
        &op(
            &engine,
            json!({"op": "revise_statement", "goal": NOBODY, "statement": "s"})
        )
        .await
    )
    .contains("unknown goal"));
    engine
        .close_goal(g, ClosureReason::Abandoned { rationale: None })
        .unwrap();
    assert!(refused(
        &op(
            &engine,
            json!({"op": "revise_statement", "goal": g.to_string(), "statement": "s"})
        )
        .await
    )
    .contains("closed"));

    let g = goal(&engine).to_string();
    assert!(refused(
        &op(
            &engine,
            json!({"op": "post_message", "scope": g, "content": "hi", "attachments": ["x.txt"]})
        )
        .await
    )
    .contains("only an agent may attach"));
    let said = refused(&op(&engine, json!({"op": "post_message", "scope": "general", "content": "hi", "agent": "general-agent", "attachments": ["x.txt"]})).await);
    assert!(
        said.contains("attach") || said.contains("no folder"),
        "{said}"
    );
    assert!(!refused(&op(&engine, json!({"op": "post_message", "scope": g, "content": "hi", "agent": "general-agent", "mentions": ["@nobody-here"]})).await).is_empty());
}

/// The judge over the socket: a request that asks nothing is refused; an
/// agent it is off for is told so; else its answer, sure or not, or that it
/// gave none.
#[tokio::test(flavor = "multi_thread")]
async fn the_judge_over_the_socket() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let question = |q: Value| json!({"op": "decide", "agent": "general-agent", "request": {"state": "a page", "questions": q}});
    assert!(refused(&op(&engine, question(json!({}))).await).contains("at least one question"));
    let asked = question(json!({"q": {"type": "noul", "instructions": "is it about AI?"}}));
    assert!(refused(&op(&engine, asked.clone()).await).contains("off for this agent"));

    let ws = engine.workspace();
    let mut general = ws.get_agent(&bisa_core::AgentId::general()).unwrap();
    general.decision_making = true;
    ws.update_agent(general).unwrap();
    engine.stand_in_decision_provider(Some(Arc::new(ScriptedProvider::new(vec![
        ScriptedProvider::answers("q", DecisionAnswer::Noul { noul: 0.97 }),
        ScriptedProvider::answers("q", DecisionAnswer::Noul { noul: 0.5 }),
    ]))));
    let sure = op(&engine, asked.clone()).await;
    assert_eq!(sure["result"]["sure"], true, "{sure}");
    let unsure = op(&engine, asked.clone()).await;
    assert_eq!(unsure["result"]["sure"], false, "{unsure}");
    assert!(
        refused(&op(&engine, asked).await).contains("no judgement"),
        "the script ran out"
    );
}

/// The socket itself: a parent made, a stale socket file removed, a name
/// too long for a socket given a short one in the temp directory, and a
/// place that cannot be bound said as the disk's error; an empty line on a
/// connection is nothing said.
#[tokio::test(flavor = "multi_thread")]
async fn the_socket_is_bound_where_it_can_be() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir().unwrap();
    let preferred = dir.path().join("run").join("intake.sock");
    let (listener, at) = bisa_engine::intake::bind(&preferred).unwrap();
    assert_eq!(at, preferred);
    drop(listener);
    assert!(
        preferred.exists(),
        "the socket file stays until the next bind"
    );
    let (_listener, at) = bisa_engine::intake::bind(&preferred).unwrap();
    assert_eq!(at, preferred, "the stale file was removed first");

    let long = dir.path().join("x".repeat(120)).join("intake.sock");
    let (_listener, at) = bisa_engine::intake::bind(&long).unwrap();
    assert_ne!(at, long);
    assert!(at.starts_with(std::env::temp_dir()), "{}", at.display());
    assert!(at
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("itf-"));

    let sealed = dir.path().join("sealed");
    std::fs::create_dir(&sealed).unwrap();
    std::fs::set_permissions(&sealed, std::fs::Permissions::from_mode(0o500)).unwrap();
    assert!(bisa_engine::intake::bind(&sealed.join("intake.sock")).is_err());
    std::fs::set_permissions(&sealed, std::fs::Permissions::from_mode(0o700)).unwrap();

    let engine = engine(&dir);
    let stream = tokio::net::UnixStream::connect(engine.socket_path())
        .await
        .unwrap();
    let (read, mut write) = stream.into_split();
    use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _};
    write.write_all(b"\n{\"op\":\"get_goal\"}\n").await.unwrap();
    let mut lines = tokio::io::BufReader::new(read).lines();
    let reply: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    assert!(refused(&reply).contains("needs `work_item` or `goal`"));
}

/// The overview names a goal waiting on a person when its run stands at a
/// human step.
#[tokio::test(flavor = "multi_thread")]
async fn the_overview_names_a_goal_waiting_on_a_person() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let draft = common::new_workflow(
        "ask first",
        vec![common::step(
            "ask",
            StepKind::Human {
                prompt: "which?".into(),
                options: vec![],
                multi: false,
                assignee: None,
            },
        )],
    );
    let (g, _run) = common::run_on(&engine, "ask the person", draft);
    common::step_in_state(&engine, g.id, "ask", "waiting").await;
    let overview = op(
        &engine,
        json!({"op": "workspace_overview", "agent": "general-agent"}),
    )
    .await;
    assert_eq!(overview["ok"], true, "{overview}");
    let waiting = overview["goals"]["waiting_on_human"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        waiting.iter().any(|w| w == &json!(g.id.to_string())),
        "{overview}"
    );
}

/// A worker's own ops through a run: progress recorded with and without an
/// outcome, the run and the goal read, memory kept under the item's agent,
/// a sub-goal spawned from a goal's run and from a run of the workspace, a
/// signal raised from each, a file that is not there refused, and a result
/// refused past its attempts or submitted twice.
#[tokio::test(flavor = "multi_thread")]
async fn a_workers_own_ops_through_a_run() {
    use bisa_harness::mock::{IntakeScript, MockAdapter};
    let dir = tempfile::tempdir().unwrap();
    let on_goal = IntakeScript::new(vec![
        json!({"op": "progress", "work_item": "{{work_item}}", "verb": "fixed", "object": "parser"}),
        json!({"op": "progress", "work_item": "{{work_item}}", "verb": "fixed", "object": "parser", "outcome": "green"}),
        json!({"op": "get_run", "work_item": "{{work_item}}"}),
        json!({"op": "get_goal", "work_item": "{{work_item}}"}),
        json!({"op": "recall_store", "work_item": "{{work_item}}", "slug": "k", "value": "v"}),
        json!({"op": "recall_get", "work_item": "{{work_item}}", "slug": "k"}),
        json!({"op": "spawn_sub_goal", "work_item": "{{work_item}}", "statement": "a child"}),
        json!({"op": "emit_signal", "work_item": "{{work_item}}", "name": "post.drafted"}),
        json!({"op": "post_message", "work_item": "{{work_item}}", "scope": "{{goal}}", "content": "see", "attachments": ["missing.txt"]}),
        json!({"op": "result_submit", "work_item": "{{work_item}}", "output": {"ok": true}}),
        json!({"op": "result_submit", "work_item": "{{work_item}}", "output": {"ok": true}}),
    ]);
    let in_workspace = IntakeScript::new(vec![
        json!({"op": "get_run", "work_item": "{{work_item}}"}),
        json!({"op": "spawn_sub_goal", "work_item": "{{work_item}}", "statement": "a child of the run"}),
        json!({"op": "emit_signal", "work_item": "{{work_item}}", "name": "post.drafted"}),
        json!({"op": "post_message", "work_item": "{{work_item}}", "scope": "general", "content": "see", "attachments": ["missing.txt"]}),
        json!({"op": "result_submit", "work_item": "{{work_item}}", "output": {"ok": true}}),
    ]);
    let twice_wrong = IntakeScript::new(vec![
        json!({"op": "result_submit", "work_item": "{{work_item}}", "output": {}}),
        json!({"op": "result_submit", "work_item": "{{work_item}}", "output": {}}),
    ]);
    let mock = |id: &str, script: &IntakeScript| MockAdapter {
        id: id.into(),
        intake_script: Some(script.clone()),
        ..Default::default()
    };
    let engine = common::engine_with(
        &dir,
        vec![mock("mock", &on_goal), mock("mock2", &in_workspace)],
    );

    let (g, _run) = common::run_on(
        &engine,
        "do the work",
        common::new_workflow(
            "one",
            vec![common::assigned_agent_step("work", "mock", "general-agent")],
        ),
    );
    common::finished_run(&engine, g.id).await;
    let replies = replies_until(&on_goal, 11).await;
    assert_eq!(replies[0]["ok"], true, "{:?}", replies[0]);
    assert_eq!(replies[1]["ok"], true, "{:?}", replies[1]);
    assert_eq!(replies[2]["goal"], g.id.to_string(), "{:?}", replies[2]);
    assert!(replies[2]["budget"].is_object(), "{:?}", replies[2]);
    assert_eq!(
        replies[3]["goal"]["id"],
        g.id.to_string(),
        "{:?}",
        replies[3]
    );
    // The step names its assignee, yet the item itself was run by no agent
    // record of its own: recall, which remembers under an agent, is refused.
    assert!(
        refused(&replies[4]).contains("agent identity"),
        "{:?}",
        replies[4]
    );
    assert!(
        refused(&replies[5]).contains("agent identity"),
        "{:?}",
        replies[5]
    );
    // A plain step's item carries no spawn budget: the sub-goal is refused.
    assert!(
        refused(&replies[6]).contains("spawn depth"),
        "{:?}",
        replies[6]
    );
    assert_eq!(replies[7]["ok"], true, "{:?}", replies[7]);
    assert!(refused(&replies[8]).contains("attach"), "{:?}", replies[8]);
    assert_eq!(replies[9]["ok"], true, "{:?}", replies[9]);
    let again = refused(&replies[10]);
    assert!(
        again.contains("already accepted") || again.contains("only an item in progress"),
        "{again}"
    );

    let (_wf, run) = common::workspace_run(
        &engine,
        common::new_workflow(
            "in the workspace",
            vec![common::agent_step("work", "mock2")],
        ),
    );
    common::run_finished(&engine, run.id).await;
    let replies = replies_until(&in_workspace, 5).await;
    assert!(
        replies[0]["goal"].is_null(),
        "a run of the workspace: {:?}",
        replies[0]
    );
    assert!(
        refused(&replies[1]).contains("spawn depth"),
        "{:?}",
        replies[1]
    );
    assert_eq!(replies[2]["ok"], true, "{:?}", replies[2]);
    assert!(refused(&replies[3]).contains("attach"), "{:?}", replies[3]);
    assert_eq!(replies[4]["ok"], true, "{:?}", replies[4]);

    // A workspace that allows no result attempt at all: every submission is
    // refused as exhausted, and the item stays as it is.
    let strict = tempfile::tempdir().unwrap();
    let engine = Engine::start(
        common::workspace(&strict),
        common::catalog_with(vec![mock("mock3", &twice_wrong)]),
        bisa_engine::EngineConfig {
            max_result_attempts: 0,
            ..common::design_off_config()
        },
    )
    .unwrap();
    let (g, _run) = common::run_on(
        &engine,
        "promise an answer",
        common::new_workflow(
            "promising",
            vec![common::agent_step_promising("work", "mock3", &["answer"])],
        ),
    );
    common::finished_run(&engine, g.id).await;
    let replies = replies_until(&twice_wrong, 2).await;
    assert!(
        refused(&replies[0]).contains("attempts exhausted"),
        "{:?}",
        replies[0]
    );
    assert!(
        refused(&replies[1]).contains("attempts exhausted"),
        "{:?}",
        replies[1]
    );
}

/// Projects and conversations as scopes: review notes read through a
/// project named, a goal's own thread, and a conversation about a project;
/// a goal read by a worker's item that is gone; a file an agent attaches
/// from its own scratch folder is stored with the message.
#[tokio::test(flavor = "multi_thread")]
async fn projects_and_conversations_as_scopes() {
    use bisa_core::ConversationOrigin;
    use bisa_store::NewConversation;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let g = goal(&engine);
    assert!(
        refused(&op(&engine, json!({"op": "get_goal", "work_item": NOBODY})).await)
            .contains("unknown work item")
    );
    let made = op(&engine, json!({"op": "create_project", "agent": "general-agent", "goal": g.to_string(), "slug": "web"})).await;
    let project = made["project"]
        .as_str()
        .expect("the project's id")
        .to_string();
    let by_project = op(
        &engine,
        json!({"op": "review_notes_list", "goal": g.to_string(), "project": project}),
    )
    .await;
    assert_eq!(by_project["notes"], json!([]), "{by_project}");
    let through_goal = op(
        &engine,
        json!({"op": "review_notes_list", "goal": g.to_string()}),
    )
    .await;
    assert_eq!(through_goal["ok"], true, "{through_goal}");
    let in_thread = op(
        &engine,
        json!({"op": "review_notes_list", "scope": g.to_string()}),
    )
    .await;
    assert_eq!(
        in_thread["ok"], true,
        "a goal's thread is the goal: {in_thread}"
    );
    let about = engine
        .workspace()
        .create_conversation(NewConversation {
            origin: ConversationOrigin::Project {
                id: project.parse().unwrap(),
            },
            title: None,
            mode: bisa_core::ConversationMode::Auto,
        })
        .unwrap();
    let in_project = op(
        &engine,
        json!({"op": "review_notes_list", "scope": about.id.to_string()}),
    )
    .await;
    assert_eq!(in_project["ok"], true, "{in_project}");
    assert!(refused(&op(&engine, json!({"op": "review_note_resolve", "note": NOBODY, "goal": g.to_string(), "project": project})).await).contains("no such review note"));
    assert!(refused(
        &op(
            &engine,
            json!({"op": "note_append", "note": "nope", "text": "x"})
        )
        .await
    )
    .contains("not a note id"));

    let scratch = engine
        .workspace()
        .paths()
        .agent(&bisa_core::AgentId::general())
        .scratch();
    std::fs::create_dir_all(&scratch).unwrap();
    std::fs::write(scratch.join("notes.md"), "# notes\n").unwrap();
    let posted = op(&engine, json!({"op": "post_message", "scope": g.to_string(), "content": "see the notes", "agent": "general-agent", "attachments": ["notes.md"]})).await;
    assert_eq!(posted["ok"], true, "{posted}");
    let folder = refused(&op(&engine, json!({"op": "post_message", "scope": g.to_string(), "content": "see", "agent": "general-agent", "attachments": ["."]})).await);
    assert!(folder.contains("attach"), "{folder}");
}

/// The Workflow Agent's reads and writes at their edges: a catalog template
/// read before it is installed, a reference that is nothing, a library
/// workflow saved with staff it does not have, an amendment staffed with
/// nobody, and the templates listed beside a goal's own design.
#[tokio::test(flavor = "multi_thread")]
async fn the_workflow_agents_reads_and_writes_at_their_edges() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let g = goal(&engine);
    let template = op(
        &engine,
        json!({"op": "get_workflow", "agent": "workflow-agent", "workflow": "software-feature"}),
    )
    .await;
    assert_eq!(template["ok"], true, "{template}");
    assert_eq!(template["installed"], false, "{template}");
    assert!(refused(&op(&engine, json!({"op": "get_workflow", "agent": "workflow-agent", "workflow": "nothing-of-the-sort"})).await).contains("no workflow"));
    let not_a_ref = refused(&op(&engine, json!({"op": "save_workflow", "agent": "workflow-agent", "workflow": "not a reference!", "revision": 1, "definition": {"name": "w", "steps": []}})).await);
    assert!(!not_a_ref.is_empty(), "{not_a_ref}");
    let unstaffed = json!({"name": "Unstaffed", "steps": [
        {"id": "work", "name": "Work", "kind": "agent", "instructions": "do", "assignee": "agent:nobody-here"}
    ]});
    let (_g2, wf) = common::goal_on(
        &engine,
        "a library workflow",
        common::new_workflow("library", vec![common::agent_step("work", "mock")]),
    );
    let refused_save = op(&engine, json!({"op": "save_workflow", "agent": "workflow-agent", "workflow": wf.id.to_string(), "revision": wf.revision, "definition": unstaffed})).await;
    assert_eq!(refused_save["ok"], false, "{refused_save}");
    let refused_amend = op(&engine, json!({"op": "amend_workflow", "goal": g.to_string(), "agent": "workflow-agent", "workflow": unstaffed})).await;
    assert_eq!(refused_amend["ok"], false, "{refused_amend}");
    engine
        .design_workflow(
            g,
            common::new_workflow("own design", vec![common::agent_step("work", "mock")]),
            None,
        )
        .unwrap();
    let listed = op(
        &engine,
        json!({"op": "list_workflow_templates", "agent": "workflow-agent"}),
    )
    .await;
    assert!(
        listed["workflows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["origin"]["goal"] == json!(g.to_string())),
        "{listed}"
    );
}

/// The judge's standing as the settings name it: a provider whose model or
/// endpoint is unnamed cannot be asked and the status says why; a key kept
/// and forgotten; the agent and the endpoint providers described by their
/// own names; a question with its criteria cleaned.
#[tokio::test(flavor = "multi_thread")]
async fn the_judges_standing_as_the_settings_name_it() {
    use bisa_core::{DecisionProviderKind, DecisionRequest, SettingScope};
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let set = |key: &str, value: Value| {
        engine
            .set_setting(SettingScope::Workspace, None, key, value)
            .unwrap()
    };
    set("decisions.provider", json!("agent"));
    set("decisions.agent.id", json!("nobody-here"));
    let status = engine.decider_status().unwrap();
    assert_eq!(status.provider, DecisionProviderKind::Agent);
    assert_eq!(status.answers_as, "nobody-here");
    assert!(
        status
            .problem
            .as_deref()
            .unwrap_or("")
            .contains("nobody-here"),
        "{status:?}"
    );
    set("decisions.agent.id", json!("general-agent"));
    let status = engine.decider_status().unwrap();
    assert!(
        status
            .problem
            .as_deref()
            .unwrap_or("")
            .contains("no harness"),
        "the core agent runs on claude-code here: {status:?}"
    );

    set("decisions.provider", json!("jev"));
    let status = engine.decider_status().unwrap();
    assert_eq!(status.key_stored, Some(false));
    assert!(status.problem.is_some(), "no key yet: {status:?}");
    let request = DecisionRequest::one("s", "q", bisa_core::DecisionQuestion::noul("so?"));
    assert!(
        engine.try_decision(&request).await.is_err(),
        "misconfigured, so refused"
    );
    engine
        .set_decision_key(DecisionProviderKind::Jev, Some("fixture-key-not-real"))
        .unwrap();
    assert_eq!(engine.decider_status().unwrap().key_stored, Some(true));
    engine
        .set_decision_key(DecisionProviderKind::Jev, None)
        .unwrap();
    assert_eq!(engine.decider_status().unwrap().key_stored, Some(false));

    set("decisions.provider", json!("rlcd"));
    let status = engine.decider_status().unwrap();
    assert_eq!(
        status.problem.as_deref(),
        Some("no endpoint is named"),
        "{status:?}"
    );
    set("decisions.rlcd.endpoint", json!("http://127.0.0.1:1"));
    let status = engine.decider_status().unwrap();
    assert_eq!(
        status.problem.as_deref(),
        Some("no model is named"),
        "{status:?}"
    );
    set("decisions.rlcd.model", json!("local"));
    let status = engine.decider_status().unwrap();
    assert_eq!(status.answers_as, "local");
    assert!(
        status
            .problem
            .as_deref()
            .unwrap_or("")
            .contains("no API key"),
        "{status:?}"
    );

    // A question's criteria travel cleaned, like its words.
    set("decisions.provider", json!("harness"));
    let ws = engine.workspace();
    let mut general = ws.get_agent(&bisa_core::AgentId::general()).unwrap();
    general.decision_making = true;
    ws.update_agent(general).unwrap();
    engine.stand_in_decision_provider(Some(Arc::new(ScriptedProvider::new(vec![
        ScriptedProvider::answers("q", DecisionAnswer::Noul { noul: 0.9 }),
    ]))));
    let answered = op(&engine, json!({"op": "decide", "agent": "general-agent", "request": {"state": "a page", "questions": {"q": {"type": "noul", "instructions": "about AI?", "criteria": {"true": "names a model", "false": "names none"}}}}})).await;
    assert_eq!(answered["ok"], true, "{answered}");
}
