//! The three security features through the engine, against fakes only: a
//! mock harness records what it was told, a synthetic token stands in for a
//! secret, and every command below is a string a rule is matched against —
//! nothing in this file is run by a shell except a plain `echo`.
//!
//! The destructive shapes the built-in rules refuse are exercised beside the
//! rules themselves, in `bisa-security`'s own tests. Here the rules under
//! test are a person's own (`fake-tool`, `fake-net`) and the one built-in that
//! is safe to spell — privilege escalation.

use crate::common;

use bisa_core::event::{GuardJudge, GuardVerdict, JournalPayload};
use bisa_core::{
    AgentId, CheckKind, FireOn, Gate, ListenerHost, McpId, McpProvenance, MessageBody,
    RespondPolicy, Schedule, SettingScope, StartOn, StepKind, ToolTier,
};
use bisa_engine::interactive::{OpenInteractive, ENV_SECRET};
use bisa_engine::security::RedactedSession;
use bisa_engine::{Engine, EngineError, EnginePayload, SessionState};
use bisa_harness::mock::{IntakeScript, MockAdapter};
use bisa_harness::{
    InputAnswer, InputRequest, InteractiveLaunch, LifecycleEvent, McpServerConfig, Outcome,
    ProgressEvent, ReportingPlan, SessionEvent, SessionSpec, Steer,
};
use bisa_store::{FileScope, NewAgent, NewMcp, PostOrigin};
use common::*;
use serde_json::{json, Value};
use std::sync::Arc;

/// A GitHub-shaped token that is not one: the built-in rule recognises the
/// shape, and nothing anywhere accepts the value.
const FAKE_TOKEN: &str = "ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
const PLACEHOLDER: &str = "«secret:github_token:";

fn set(engine: &Engine, key: &str, value: Value) {
    engine
        .set_setting(SettingScope::Workspace, None, key, value)
        .unwrap();
}

/// A person's deny/ask/classify rule on commands starting with `program`.
fn command_rule(id: &str, action: &str, program: &str) -> Value {
    json!({
        "id": id,
        "label": format!("{id} rule"),
        "action": action,
        "matcher": { "kind": "command", "regex": format!("^{program}\\b") }
    })
}

/// A worker whose turn stops on one permission for `command`, then — once
/// answered — ends its turn and the session.
fn asking_worker(command: &str) -> MockAdapter {
    asking_for("Bash", command, json!({ "command": command }))
}

/// A worker whose turn stops on one permission for `tool` with `input`,
/// then — once answered — ends its turn and the session.
fn asking_for(tool: &str, summary: &str, input: Value) -> MockAdapter {
    MockAdapter {
        id: "mock".into(),
        input_request: Some(InputRequest::permission(
            "p1",
            tool,
            ToolTier::classify(tool),
            summary,
            input,
        )),
        script: Some(vec![
            SessionEvent::Progress(ProgressEvent::TurnEnded),
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: Outcome::Completed,
                is_terminal: true,
            }),
        ]),
        ..Default::default()
    }
}

/// An asking worker that, once answered, finishes its step: it yields a
/// result through the intake, so a run of such steps ends `Done` and the
/// test can read what each session was answered.
fn asking_worker_yielding(command: &str) -> MockAdapter {
    MockAdapter {
        intake_script: Some(IntakeScript::new(vec![json!({
            "op": "result_submit", "work_item": "{{work_item}}", "output": { "ok": true }
        })])),
        ..asking_worker(command)
    }
}

/// A classifier harness whose every session answers `line`.
fn classifier_saying(line: &str) -> MockAdapter {
    MockAdapter {
        id: "mock-classifier".into(),
        script: Some(vec![
            SessionEvent::Lifecycle(LifecycleEvent::Started),
            SessionEvent::Progress(ProgressEvent::TurnStarted),
            SessionEvent::Progress(ProgressEvent::TextDelta { text: line.into() }),
            SessionEvent::Progress(ProgressEvent::TurnEnded),
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: Outcome::Completed,
                is_terminal: true,
            }),
        ]),
        ..Default::default()
    }
}

fn new_agent(name: &str, harness: &str) -> NewAgent {
    NewAgent {
        name: name.into(),
        photo: None,
        description: None,
        system_prompt: format!("You are {name}."),
        harness: harness.into(),
        models: Default::default(),
        skills: vec![],
        mcps: vec![],
        tags: Default::default(),
        respond: RespondPolicy::OwnerOnly,
        decision_making: false,
    }
}

fn guard_facts(
    engine: &Engine,
    goal: bisa_core::GoalId,
) -> Vec<(GuardVerdict, GuardJudge, Option<String>, String)> {
    engine
        .workspace()
        .journal(&bisa_core::Home::from(goal))
        .unwrap()
        .into_iter()
        .filter_map(|e| match e.payload {
            JournalPayload::Guard {
                verdict,
                by,
                rule,
                subject,
                ..
            } => Some((verdict, by, rule, subject)),
            _ => None,
        })
        .collect()
}

async fn opened_gate(
    rx: &mut tokio::sync::broadcast::Receiver<bisa_engine::EngineEvent>,
) -> (String, String) {
    let opened = wait_for(rx, "escalation gate", |e| {
        matches!(
            &e.payload,
            EnginePayload::GateOpened {
                gate: Gate::Escalation,
                ..
            }
        )
    })
    .await;
    let EnginePayload::GateOpened {
        gate_id, question, ..
    } = opened.payload
    else {
        unreachable!()
    };
    (gate_id, question)
}

// ---------------------------------------------------------------------------
// The Redactor
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn a_message_holding_a_token_reaches_the_harness_redacted_and_the_reply_keeps_the_placeholder(
) {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    let agent = ws.add_agent(new_agent("Scout", "mock")).unwrap();
    let dm = ws.open_dm(std::slice::from_ref(&agent.pubkey)).unwrap();
    let mock = MockAdapter::default();
    let prompts = Arc::clone(&mock.prompts);
    let engine = Engine::start(ws, catalog_with(vec![mock]), design_off_config()).unwrap();
    let mut rx = engine.events();

    engine
        .workspace()
        .post_message(
            dm.id.as_str(),
            MessageBody::post(format!("push with {FAKE_TOKEN} please")),
            None,
            &[],
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap();

    let reply = until("the agent's reply", || {
        engine
            .workspace()
            .messages(dm.id.as_str(), None, 20)
            .unwrap()
            .into_iter()
            .find(|m| m.author == agent.pubkey.as_hex())
    })
    .await;
    let prompt = prompts
        .lock()
        .unwrap()
        .last()
        .cloned()
        .expect("a prompt was sent");
    assert!(
        !prompt.contains(FAKE_TOKEN),
        "the harness must never see the token"
    );
    assert!(
        prompt.contains(PLACEHOLDER),
        "the harness sees a placeholder instead: {prompt}"
    );
    // The mock echoes its prompt: what the agent said carries the placeholder
    // and is stored as such — nothing an agent hands back is restored.
    assert!(!reply.content.contains(FAKE_TOKEN));
    assert!(reply.content.contains(PLACEHOLDER));
    let redacted = wait_for(&mut rx, "a redaction event", |e| {
        matches!(&e.payload, EnginePayload::Redacted { .. })
    })
    .await;
    assert!(
        matches!(&redacted.payload, EnginePayload::Redacted { count, kinds, at } if *count >= 1 && kinds.contains(&"github_token".to_string()) && at == "prompt")
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_steer_and_a_follow_up_are_redacted_like_a_prompt() {
    let dir = tempfile::tempdir().unwrap();
    let mock = MockAdapter::default();
    let follow_ups = Arc::clone(&mock.follow_ups);
    let engine = engine_with(&dir, vec![mock]);
    let adapter = engine.inner().catalog.get("mock").unwrap();
    let spec = SessionSpec {
        work_item: None,
        cwd: dir.path().to_path_buf(),
        prompt: String::new(),
        model: None,
        effort: None,
        mcp_servers: vec![],
        env: Default::default(),
        env_remove: Vec::new(),
        tier_ceiling: ToolTier::Read,
        output_schema: None,
        skills: vec![],
    };
    let raw = adapter.launch(spec).await.unwrap();
    let session = RedactedSession::wrap(raw, Arc::clone(&engine.inner().security));
    session
        .follow_up(Steer {
            text: format!("and use {FAKE_TOKEN}"),
            attachments: vec![],
        })
        .await
        .unwrap();
    let told = follow_ups.lock().unwrap().clone();
    assert_eq!(told.len(), 1);
    assert!(
        !told[0].contains(FAKE_TOKEN) && told[0].contains(PLACEHOLDER),
        "{}",
        told[0]
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn get_goal_answers_over_the_socket_with_the_statement_redacted() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let (goal, _) = goal_on(
        &engine,
        &format!("rotate the key {FAKE_TOKEN} everywhere"),
        new_workflow("w", vec![agent_step("run", "mock")]),
    );
    let reply = intake_roundtrip(
        engine.socket_path(),
        json!({ "op": "get_goal", "goal": goal.id.to_string() }),
    )
    .await;
    let text = reply.to_string();
    assert!(
        !text.contains(FAKE_TOKEN),
        "the MCP reply carried the token: {text}"
    );
    assert!(text.contains(PLACEHOLDER), "{text}");
    // The store still holds what the person wrote: redaction is on the way
    // out, never a rewrite of the record.
    assert!(engine
        .workspace()
        .get_goal(goal.id)
        .unwrap()
        .statement
        .contains(FAKE_TOKEN));
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// The Guard
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn a_persons_deny_rule_refuses_the_call_names_the_rule_and_the_run_goes_on() {
    let dir = tempfile::tempdir().unwrap();
    let worker = asking_worker("fake-tool --go");
    let answered = Arc::clone(&worker.answered);
    let engine = engine_with(&dir, vec![worker]);
    set(
        &engine,
        "security.guard.rules",
        json!([command_rule("no_fake_tool", "deny", "fake-tool")]),
    );
    let mut rx = engine.events();
    let (goal, _) = run_on(
        &engine,
        "guarded",
        new_workflow("guarded", vec![agent_step("run", "mock")]),
    );

    wait_for(&mut rx, "the item ends", |e| {
        matches!(&e.payload, EnginePayload::ExecutionEnded { .. })
    })
    .await;
    let answers = answered.lock().unwrap().clone();
    assert_eq!(answers.len(), 1);
    match &answers[0].1 {
        InputAnswer::Deny { reason } => assert!(reason.contains("no_fake_tool rule"), "{reason}"),
        other => panic!("{other:?}"),
    }
    let facts = guard_facts(&engine, goal.id);
    assert_eq!(facts.len(), 1);
    assert_eq!(
        (facts[0].0, facts[0].1, facts[0].2.as_deref()),
        (GuardVerdict::Denied, GuardJudge::Rule, Some("no_fake_tool"))
    );
    assert_eq!(facts[0].3, "fake-tool --go");
    assert!(engine
        .inner()
        .security
        .recent()
        .iter()
        .any(|d| d.rule.as_deref() == Some("no_fake_tool")));
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_built_in_privilege_rule_refuses_without_asking_anyone() {
    let dir = tempfile::tempdir().unwrap();
    let worker = asking_worker("sudo ls");
    let answered = Arc::clone(&worker.answered);
    let engine = engine_with(&dir, vec![worker]);
    let mut rx = engine.events();
    run_on(
        &engine,
        "sudo",
        new_workflow("sudo", vec![agent_step("run", "mock")]),
    );
    wait_for(&mut rx, "the item ends", |e| {
        matches!(&e.payload, EnginePayload::ExecutionEnded { .. })
    })
    .await;
    let answers = answered.lock().unwrap().clone();
    assert!(
        matches!(&answers[0].1, InputAnswer::Deny { reason } if reason.contains("sudo, doas, su")),
        "{:?}",
        answers[0].1
    );
    assert!(
        engine
            .inner()
            .security
            .recent()
            .iter()
            .all(|d| d.verdict != GuardVerdict::Asked),
        "no gate was opened"
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_ask_rule_opens_a_gate_whose_question_carries_the_placeholder_and_the_person_decides() {
    let dir = tempfile::tempdir().unwrap();
    let worker = asking_worker(&format!("fake-tool push --token {FAKE_TOKEN}"));
    let answered = Arc::clone(&worker.answered);
    let engine = engine_with(&dir, vec![worker]);
    set(
        &engine,
        "security.guard.rules",
        json!([command_rule("ask_fake_tool", "ask", "fake-tool")]),
    );
    let mut rx = engine.events();
    let (goal, _) = run_on(
        &engine,
        "asked",
        new_workflow("asked", vec![agent_step("run", "mock")]),
    );

    let (gate_id, question) = opened_gate(&mut rx).await;
    assert!(question.starts_with("Allow `Bash`?"), "{question}");
    assert!(
        !question.contains(FAKE_TOKEN) && question.contains(PLACEHOLDER),
        "{question}"
    );
    assert!(question.contains("ask_fake_tool rule"));
    let gate = engine.gate(&gate_id).unwrap();
    assert_eq!(gate.subject, "guard:Bash");
    engine.decide(&gate_id, true, None, None, None).unwrap();

    wait_for(&mut rx, "the item ends", |e| {
        matches!(&e.payload, EnginePayload::ExecutionEnded { .. })
    })
    .await;
    let answers = answered.lock().unwrap().clone();
    match &answers[0].1 {
        InputAnswer::Allow { input: Some(input) } => {
            assert_eq!(
                input["command"],
                format!("fake-tool push --token {FAKE_TOKEN}"),
                "what the agent typed is what runs"
            );
        }
        other => panic!("{other:?}"),
    }
    let facts = guard_facts(&engine, goal.id);
    assert!(facts
        .iter()
        .any(|f| f.0 == GuardVerdict::Asked && f.1 == GuardJudge::Rule));
    assert!(facts
        .iter()
        .any(|f| f.0 == GuardVerdict::Allowed && f.1 == GuardJudge::Person));
    assert!(
        facts.iter().all(|f| !f.3.contains(FAKE_TOKEN)),
        "the journal never carries the token"
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_command_carrying_a_placeholder_nobody_can_resolve_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let worker = asking_worker("fake-tool --key «secret:github_token:abcdef»");
    let answered = Arc::clone(&worker.answered);
    let engine = engine_with(&dir, vec![worker]);
    let mut rx = engine.events();
    run_on(
        &engine,
        "stale",
        new_workflow("stale", vec![agent_step("run", "mock")]),
    );
    wait_for(&mut rx, "the item ends", |e| {
        matches!(&e.payload, EnginePayload::ExecutionEnded { .. })
    })
    .await;
    let answers = answered.lock().unwrap().clone();
    assert!(
        matches!(&answers[0].1, InputAnswer::Deny { reason } if reason.contains("cannot resolve")),
        "{:?}",
        answers[0].1
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_placeholder_the_vault_knows_is_restored_only_in_the_input_that_runs() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    let agent = ws.add_agent(new_agent("Scout", "mock")).unwrap();
    let dm = ws.open_dm(std::slice::from_ref(&agent.pubkey)).unwrap();
    let engine = Engine::start(
        ws,
        catalog_with(vec![MockAdapter::default()]),
        design_off_config(),
    )
    .unwrap();
    // A message teaches the vault the token's placeholder.
    engine
        .workspace()
        .post_message(
            dm.id.as_str(),
            MessageBody::post(format!("the key is {FAKE_TOKEN}")),
            None,
            &[],
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap();
    until("the reply", || {
        engine
            .workspace()
            .messages(dm.id.as_str(), None, 20)
            .unwrap()
            .into_iter()
            .find(|m| m.author == agent.pubkey.as_hex())
    })
    .await;
    let placeholder = engine.inner().security.vault().restore("x").text; // warms nothing; the vault is keyed below
    let _ = placeholder;
    let redacted = engine.inner().security.redact(FAKE_TOKEN).text;
    assert!(redacted.starts_with(PLACEHOLDER));

    // The guard, judging a call that quotes the placeholder, hands back the
    // real value to run with — and journals the redacted text.
    let input = json!({ "command": format!("fake-tool --key {redacted}") });
    let judge = bisa_engine::security::Judge::platform(None, None);
    match bisa_engine::security::decide_tool(engine.inner(), "Bash", &input, judge, None).await {
        bisa_engine::security::Outcome::Fallthrough { input } => {
            assert_eq!(input["command"], format!("fake-tool --key {FAKE_TOKEN}"));
        }
        other => panic!("{other:?}"),
    }
    let preview = engine.redact_preview(&format!("again {FAKE_TOKEN}"));
    assert_eq!(preview.count, 1);
    assert_ne!(
        preview.text,
        format!("again {redacted}"),
        "a preview runs on a scratch vault and never learns the real tag"
    );
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// The Classifier
// ---------------------------------------------------------------------------

/// An engine whose General Agent runs on the `mock-classifier` harness — the
/// classifier's default agent — beside the `mock` worker.
fn engine_with_classifier(
    dir: &tempfile::TempDir,
    worker: MockAdapter,
    classifier: MockAdapter,
) -> Engine {
    let ws = workspace(dir);
    drive_on(&ws, &AgentId::general(), "mock-classifier");
    Engine::start(
        ws,
        catalog_with(vec![worker, classifier]),
        design_off_config(),
    )
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_safe_verdict_allows_and_the_classifier_never_sees_the_token() {
    let dir = tempfile::tempdir().unwrap();
    let worker = asking_worker(&format!("fake-net fetch --token {FAKE_TOKEN}"));
    let answered = Arc::clone(&worker.answered);
    let classifier = classifier_saying("SAFE");
    let asked = Arc::clone(&classifier.prompts);
    let engine = engine_with_classifier(&dir, worker, classifier);
    set(
        &engine,
        "security.guard.rules",
        json!([command_rule("classify_fake_net", "classify", "fake-net")]),
    );
    let mut rx = engine.events();
    let (goal, _) = run_on(
        &engine,
        "classified",
        new_workflow("classified", vec![agent_step("run", "mock")]),
    );

    wait_for(&mut rx, "the item ends", |e| {
        matches!(&e.payload, EnginePayload::ExecutionEnded { .. })
    })
    .await;
    let answers = answered.lock().unwrap().clone();
    assert!(
        matches!(&answers[0].1, InputAnswer::Allow { .. }),
        "{:?}",
        answers[0].1
    );
    let prompt = asked
        .lock()
        .unwrap()
        .first()
        .cloned()
        .expect("the classifier was asked");
    assert!(
        prompt.contains("fake-net fetch")
            && !prompt.contains(FAKE_TOKEN)
            && prompt.contains(PLACEHOLDER),
        "{prompt}"
    );
    assert!(
        prompt.contains("SAFE") && prompt.contains("HARMFUL:"),
        "the prompt asks for one of two lines"
    );
    let facts = guard_facts(&engine, goal.id);
    assert!(facts
        .iter()
        .any(|f| f.0 == GuardVerdict::Allowed && f.1 == GuardJudge::Classifier));
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_harmful_verdict_asks_the_person_with_the_classifiers_reason() {
    let dir = tempfile::tempdir().unwrap();
    let worker = asking_worker("fake-net upload ./secrets");
    let answered = Arc::clone(&worker.answered);
    let engine = engine_with_classifier(
        &dir,
        worker,
        classifier_saying("HARMFUL: it uploads the project's secrets."),
    );
    set(
        &engine,
        "security.guard.rules",
        json!([command_rule("classify_fake_net", "classify", "fake-net")]),
    );
    let mut rx = engine.events();
    run_on(
        &engine,
        "harmful",
        new_workflow("harmful", vec![agent_step("run", "mock")]),
    );

    let (gate_id, question) = opened_gate(&mut rx).await;
    assert!(
        question.contains("uploads the project's secrets"),
        "{question}"
    );
    engine
        .decide(&gate_id, false, Some("no"), None, None)
        .unwrap();
    wait_for(&mut rx, "the item ends", |e| {
        matches!(&e.payload, EnginePayload::ExecutionEnded { .. })
    })
    .await;
    assert!(matches!(
        &answered.lock().unwrap()[0].1,
        InputAnswer::Deny { .. }
    ));
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_harmful_verdict_denies_outright_when_settings_say_so() {
    let dir = tempfile::tempdir().unwrap();
    let worker = asking_worker("fake-net upload ./secrets");
    let answered = Arc::clone(&worker.answered);
    let engine = engine_with_classifier(&dir, worker, classifier_saying("HARMFUL: it phones home"));
    set(
        &engine,
        "security.guard.rules",
        json!([command_rule("classify_fake_net", "classify", "fake-net")]),
    );
    set(&engine, "security.classifier.on_harmful", json!("deny"));
    let mut rx = engine.events();
    run_on(
        &engine,
        "denied",
        new_workflow("denied", vec![agent_step("run", "mock")]),
    );
    wait_for(&mut rx, "the item ends", |e| {
        matches!(&e.payload, EnginePayload::ExecutionEnded { .. })
    })
    .await;
    assert!(
        matches!(&answered.lock().unwrap()[0].1, InputAnswer::Deny { reason } if reason.contains("phones home"))
    );
    assert!(
        engine
            .inner()
            .security
            .recent()
            .iter()
            .all(|d| d.verdict != GuardVerdict::Asked),
        "no gate was opened"
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn no_verdict_goes_to_the_person_never_to_an_allow() {
    let dir = tempfile::tempdir().unwrap();
    let worker = asking_worker("fake-net probe");
    let engine = engine_with_classifier(
        &dir,
        worker,
        classifier_saying("I think this is probably fine, but who knows"),
    );
    set(
        &engine,
        "security.guard.rules",
        json!([command_rule("classify_fake_net", "classify", "fake-net")]),
    );
    let mut rx = engine.events();
    run_on(
        &engine,
        "unsure",
        new_workflow("unsure", vec![agent_step("run", "mock")]),
    );
    let (gate_id, question) = opened_gate(&mut rx).await;
    assert!(question.contains("no verdict"), "{question}");
    engine.decide(&gate_id, false, None, None, None).unwrap();
    wait_for(&mut rx, "the item ends", |e| {
        matches!(&e.payload, EnginePayload::ExecutionEnded { .. })
    })
    .await;
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_classifier_that_never_answers_is_no_verdict_within_the_deadline() {
    let dir = tempfile::tempdir().unwrap();
    let worker = asking_worker("fake-net probe");
    // A script with no end: the session stays open and says nothing.
    let silent = MockAdapter {
        id: "mock-classifier".into(),
        script: Some(vec![
            SessionEvent::Lifecycle(LifecycleEvent::Started),
            SessionEvent::Progress(ProgressEvent::TurnStarted),
        ]),
        ..Default::default()
    };
    let engine = engine_with_classifier(&dir, worker, silent);
    set(
        &engine,
        "security.guard.rules",
        json!([command_rule("classify_fake_net", "classify", "fake-net")]),
    );
    set(&engine, "security.classifier.deadline_secs", json!(5));
    let mut rx = engine.events();
    let started = std::time::Instant::now();
    run_on(
        &engine,
        "silent",
        new_workflow("silent", vec![agent_step("run", "mock")]),
    );
    let (gate_id, question) = opened_gate(&mut rx).await;
    assert!(question.contains("did not answer within 5s"), "{question}");
    assert!(started.elapsed() < std::time::Duration::from_secs(15));
    engine.decide(&gate_id, false, None, None, None).unwrap();
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_one_shot_ask_answers_a_read_permission_instead_of_stalling() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::general(), "mock");
    let mock = MockAdapter {
        input_request: Some(InputRequest::permission(
            "r1",
            "Read",
            ToolTier::Read,
            "notes.md",
            json!({ "file_path": "notes.md" }),
        )),
        ..Default::default()
    };
    let answered = Arc::clone(&mock.answered);
    let engine = Engine::start(ws, catalog_with(vec![mock]), design_off_config()).unwrap();
    let answer = bisa_engine::ask::ask_agent_once(
        engine.inner(),
        AgentId::GENERAL,
        bisa_engine::ask::Asking::of(bisa_core::AskPurpose::Classifier),
        "which file?",
        std::time::Duration::from_secs(20),
    )
    .await
    .unwrap();
    assert!(answer.contains("which file?"));
    assert!(
        matches!(answered.lock().unwrap().as_slice(), [(id, InputAnswer::Allow { .. })] if id == "r1")
    );
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// Platform-run commands
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn a_check_step_whose_command_a_rule_refuses_fails_with_the_reason() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    set(
        &engine,
        "security.guard.rules",
        json!([command_rule("no_fake_tool", "deny", "fake-tool")]),
    );
    let (goal, _) = run_on(
        &engine,
        "checked",
        new_workflow(
            "checked",
            vec![step(
                "probe",
                StepKind::Check {
                    check: CheckKind::Command {
                        command: "fake-tool verify".into(),
                    },
                },
            )],
        ),
    );
    let run = finished_run(&engine, goal.id).await;
    let error = run.steps[&sid("probe")]
        .error
        .clone()
        .expect("the step failed");
    assert!(
        error.contains("refused") && error.contains("no_fake_tool rule"),
        "{error}"
    );
    assert!(guard_facts(&engine, goal.id)
        .iter()
        .any(|f| f.0 == GuardVerdict::Denied));
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_check_step_that_runs_journals_the_redacted_command() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let (goal, _) = run_on(
        &engine,
        "echoed",
        new_workflow(
            "echoed",
            vec![step(
                "probe",
                StepKind::Check {
                    check: CheckKind::Command {
                        command: format!("echo {FAKE_TOKEN} >/dev/null"),
                    },
                },
            )],
        ),
    );
    let run = finished_run(&engine, goal.id).await;
    let output = run.steps[&sid("probe")]
        .output
        .clone()
        .expect("the step passed")
        .to_string();
    assert!(
        !output.contains(FAKE_TOKEN),
        "the evidence carried the token: {output}"
    );
    assert!(output.contains(PLACEHOLDER), "{output}");
    engine.shutdown().await;
}

/// A check start runs a shell command on a cadence, so the guard judges it
/// where a person turns the workflow on — before anything is written — and
/// again at every fire.
#[tokio::test(flavor = "multi_thread")]
async fn a_check_start_whose_command_a_rule_refuses_is_refused_at_turn_on() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    set(
        &engine,
        "security.guard.rules",
        json!([command_rule("no_fake_tool", "deny", "fake-tool")]),
    );
    let checking = |name: &str, command: &str| {
        let mut start = step(
            "failing",
            StepKind::Start {
                on: StartOn::Check {
                    command: command.into(),
                    project: None,
                    fire_on: FireOn::StartsFailing,
                    schedule: Schedule::every(300),
                },
                inputs: Default::default(),
                guard: Default::default(),
            },
        );
        start.then = vec![bisa_core::Flow::to(sid("run"))];
        let wf = engine
            .create_workflow(new_workflow(name, vec![start, agent_step("run", "mock")]))
            .unwrap();
        ListenerHost::Workspace { workflow: wf.id }
    };
    let host = checking("refused", "fake-tool check");
    let refused = engine.set_listening(host, Default::default(), None);
    assert!(
        matches!(refused, Err(EngineError::Security(ref r)) if r.contains("no_fake_tool rule")),
        "{refused:?}"
    );
    assert!(
        engine.workspace().listening(&host).unwrap().is_none(),
        "a refused turn-on writes nothing down"
    );
    engine
        .set_listening(checking("plain", "true"), Default::default(), None)
        .expect("an ordinary check listens");
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// Terminal-hosted sessions
// ---------------------------------------------------------------------------

fn terminal_mock() -> MockAdapter {
    MockAdapter {
        id: "mock".into(),
        interactive: Some(InteractiveLaunch::new("mock")),
        reporting: ReportingPlan {
            args: vec!["--report".into()],
            ..Default::default()
        },
        ..Default::default()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_guard_hook_answers_deny_ask_or_nothing_and_a_report_is_redacted() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![terminal_mock()]);
    set(
        &engine,
        "security.guard.rules",
        json!([command_rule("ask_fake_tool", "ask", "fake-tool")]),
    );
    let inner = engine.inner();
    // A session stands where a terminal can open: in a goal the workspace has.
    let goal = engine
        .submit_goal(bisa_engine::SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..bisa_engine::SubmitRequest::captured("sit in a terminal")
        })
        .unwrap()
        .id;
    let opened = inner
        .interactive
        .open(
            inner,
            OpenInteractive {
                scope: FileScope::Goal,
                id: goal.to_string(),
                harness: "mock".into(),
            },
        )
        .unwrap()
        .unwrap();
    let secret = opened.env[ENV_SECRET].clone();
    let hook = |tool: &str, input: Value| json!({ "hook_event_name": "PreToolUse", "tool_name": tool, "tool_input": input, "cwd": dir.path() });

    let deny = inner
        .interactive
        .guard(
            inner,
            opened.session,
            &secret,
            &hook("Bash", json!({ "command": "sudo ls" })),
        )
        .await
        .unwrap();
    assert_eq!(deny.decision.as_deref(), Some("deny"));
    assert!(deny.reason.as_deref().unwrap_or_default().contains("sudo"));

    let dotenv = inner
        .interactive
        .guard(
            inner,
            opened.session,
            &secret,
            &hook("Read", json!({ "file_path": dir.path().join(".env") })),
        )
        .await
        .unwrap();
    assert_eq!(dotenv.decision.as_deref(), Some("deny"));

    let ask = inner
        .interactive
        .guard(
            inner,
            opened.session,
            &secret,
            &hook("Bash", json!({ "command": "fake-tool run" })),
        )
        .await
        .unwrap();
    assert_eq!(
        ask.decision.as_deref(),
        Some("ask"),
        "a terminal session has no goal: the person at the keyboard is asked"
    );

    let quiet = inner
        .interactive
        .guard(
            inner,
            opened.session,
            &secret,
            &hook("Bash", json!({ "command": "ls -la" })),
        )
        .await
        .unwrap();
    assert!(
        quiet.decision.is_none() && quiet.updated_input.is_none(),
        "no opinion: the harness's own prompt stands"
    );

    assert!(inner
        .interactive
        .guard(
            inner,
            opened.session,
            "not-the-secret",
            &hook("Bash", json!({}))
        )
        .await
        .is_err());

    // What the harness reports about itself is redacted before the roster.
    inner
        .interactive
        .report(
            inner,
            opened.session,
            &secret,
            &[SessionEvent::Progress(ProgressEvent::ToolStarted {
                name: "Bash".into(),
                args_summary: format!("curl -H {FAKE_TOKEN}"),
                tier: ToolTier::Exec,
                id: None,
            })],
        )
        .unwrap();
    match inner.presence.get(opened.session).map(|p| p.state) {
        Some(SessionState::Running { args, .. }) => assert!(
            !args.contains(FAKE_TOKEN) && args.contains(PLACEHOLDER),
            "{args}"
        ),
        other => panic!("{other:?}"),
    }
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_status_names_the_rules_the_problems_and_never_a_value() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    set(
        &engine,
        "security.redactor.rules",
        json!([
            { "id": "my_key", "label": "My key", "detector": { "kind": "env_value", "name": "BISA_TEST_FAKE_KEY_THAT_IS_UNSET" } },
            { "id": "broken", "label": "Broken", "detector": { "kind": "pattern", "regex": "(" } }
        ]),
    );
    set(
        &engine,
        "security.guard.builtins_off",
        json!(["publishing"]),
    );
    let status = engine.security_status();
    assert!(status
        .redact_rules
        .iter()
        .any(|r| r.id == "github_token" && r.enabled));
    assert!(status.redact_rules.iter().any(|r| r.id == "my_key"));
    assert!(
        status
            .guard_rules
            .iter()
            .any(|r| r.id == "publishing" && !r.enabled),
        "a switched-off built-in is listed off"
    );
    assert_eq!(status.problems.len(), 1);
    assert_eq!(status.problems[0].rule, "broken");
    assert_eq!(status.classifier.agent, AgentId::GENERAL);
    assert!(status
        .harnesses
        .iter()
        .any(|h| h.id == "mock" && !h.tool_guard));
    let text = serde_json::to_string(&status).unwrap();
    assert!(
        !text.contains("fake-value"),
        "no environment value in the status"
    );
    let preview = engine.guard_preview("Bash", &json!({ "command": "sudo make install" }));
    assert_eq!(preview.verdict, "deny");
    assert_eq!(preview.rule.as_deref(), Some("privilege_escalation"));
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// The inbound funnel: what an agent hands back
// ---------------------------------------------------------------------------

/// Every guard fact with the reason it carried.
fn guard_reasons(
    engine: &Engine,
    goal: bisa_core::GoalId,
) -> Vec<(GuardVerdict, GuardJudge, Option<String>)> {
    engine
        .workspace()
        .journal(&bisa_core::Home::from(goal))
        .unwrap()
        .into_iter()
        .filter_map(|e| match e.payload {
            JournalPayload::Guard {
                verdict,
                by,
                reason,
                ..
            } => Some((verdict, by, reason)),
            _ => None,
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agents_message_and_note_through_the_socket_are_stored_as_placeholders() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let mut rx = engine.events();
    let (goal, _) = goal_on(
        &engine,
        "rotate the key",
        new_workflow("w", vec![agent_step("run", "mock")]),
    );
    // The agent read the token with its own tools and now writes it down:
    // the request half is redacted before any op reads it.
    let reply = intake_roundtrip(
        engine.socket_path(),
        json!({ "op": "add_note", "goal": goal.id.to_string(), "text": format!("found {FAKE_TOKEN} in the log") }),
    )
    .await;
    assert_eq!(reply["ok"], true, "{reply}");
    let reply = intake_roundtrip(
        engine.socket_path(),
        json!({ "op": "post_message", "scope": goal.id.to_string(), "content": format!("the deploy key is {FAKE_TOKEN}") }),
    )
    .await;
    assert_eq!(reply["ok"], true, "{reply}");

    let notes: Vec<String> = engine
        .workspace()
        .journal(&bisa_core::Home::from(goal.id))
        .unwrap()
        .into_iter()
        .filter_map(|e| match e.payload {
            JournalPayload::Note { text } => Some(text),
            _ => None,
        })
        .collect();
    assert!(
        notes.iter().any(|n| n.contains(PLACEHOLDER))
            && notes.iter().all(|n| !n.contains(FAKE_TOKEN)),
        "the note is stored as a placeholder: {notes:?}"
    );
    let posted = engine
        .workspace()
        .messages(goal.id.to_string().as_str(), None, 20)
        .unwrap()
        .into_iter()
        .find(|m| m.content.contains("deploy key"))
        .expect("the agent's message");
    assert!(
        posted.content.contains(PLACEHOLDER) && !posted.content.contains(FAKE_TOKEN),
        "{}",
        posted.content
    );
    let redacted = wait_for(
        &mut rx,
        "the inbound redaction",
        |e| matches!(&e.payload, EnginePayload::Redacted { at, .. } if at == "mcp_request"),
    )
    .await;
    assert!(
        matches!(&redacted.payload, EnginePayload::Redacted { kinds, .. } if kinds.contains(&"github_token".to_string()))
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_chat_reply_quoting_a_token_is_stored_redacted() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    let agent = ws.add_agent(new_agent("Scout", "mock")).unwrap();
    let dm = ws.open_dm(std::slice::from_ref(&agent.pubkey)).unwrap();
    // An agent whose turn says a token it found on its own — in two deltas,
    // so the boundary splits the shape the way a stream does.
    let (head, tail) = FAKE_TOKEN.split_at(10);
    let mock = MockAdapter {
        script: Some(vec![
            SessionEvent::Lifecycle(LifecycleEvent::Started),
            SessionEvent::Progress(ProgressEvent::TurnStarted),
            SessionEvent::Progress(ProgressEvent::TextDelta {
                text: format!("the key is {head}"),
            }),
            SessionEvent::Progress(ProgressEvent::TextDelta {
                text: format!("{tail} — rotate it"),
            }),
            SessionEvent::Progress(ProgressEvent::TurnEnded),
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: Outcome::Completed,
                is_terminal: true,
            }),
        ]),
        ..Default::default()
    };
    let engine = Engine::start(ws, catalog_with(vec![mock]), design_off_config()).unwrap();
    engine
        .workspace()
        .post_message(
            dm.id.as_str(),
            MessageBody::post("what did you find?"),
            None,
            &[],
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap();
    let reply = until("the agent's reply", || {
        engine
            .workspace()
            .messages(dm.id.as_str(), None, 20)
            .unwrap()
            .into_iter()
            .find(|m| m.author == agent.pubkey.as_hex())
    })
    .await;
    assert!(
        !reply.content.contains(FAKE_TOKEN),
        "the reply is stored with the token: {}",
        reply.content
    );
    assert!(
        reply.content.contains(PLACEHOLDER) && reply.content.contains("rotate it"),
        "{}",
        reply.content
    );
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// A person's answer is remembered
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn the_same_question_on_the_same_goal_is_asked_once_and_the_answer_is_remembered() {
    let dir = tempfile::tempdir().unwrap();
    let worker = asking_worker_yielding("fake-tool push --to staging");
    let answered = Arc::clone(&worker.answered);
    let engine = engine_with(&dir, vec![worker]);
    set(
        &engine,
        "security.guard.rules",
        json!([command_rule("ask_fake_tool", "ask", "fake-tool")]),
    );
    let mut rx = engine.events();
    // Two steps, two sessions, one goal — the same call twice.
    let (goal, _) = run_on(
        &engine,
        "asked once",
        new_workflow(
            "asked-once",
            chain(vec![
                agent_step("first", "mock"),
                agent_step("second", "mock"),
            ]),
        ),
    );
    let (gate_id, _) = opened_gate(&mut rx).await;
    engine.decide(&gate_id, true, None, None, None).unwrap();
    let run = finished_run(&engine, goal.id).await;
    assert_eq!(run.outcome, Some(bisa_core::RunOutcome::Done), "{run:?}");

    let answers = answered.lock().unwrap().clone();
    assert_eq!(answers.len(), 2, "both sessions were answered");
    assert!(
        answers
            .iter()
            .all(|(_, a)| matches!(a, InputAnswer::Allow { .. })),
        "{answers:?}"
    );
    let facts = guard_reasons(&engine, goal.id);
    let asked = facts.iter().filter(|f| f.0 == GuardVerdict::Asked).count();
    assert_eq!(asked, 1, "the person was asked exactly once: {facts:?}");
    assert!(
        facts.iter().any(|f| f.0 == GuardVerdict::Allowed
            && f.1 == GuardJudge::Person
            && f.2.as_deref() == Some(bisa_engine::security::REMEMBERED)),
        "the second call was allowed as the person's remembered answer: {facts:?}"
    );
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// The environment as detectors
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn the_status_counts_the_environment_detectors_and_a_switch_disarms_them() {
    // A name that says *token*, set on this test process only; the value is
    // synthetic and never leaves the redactor.
    std::env::set_var("BISA_TEST_FAKE_TOKEN", "fake-env-value-0001");
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let status = engine.security_status();
    assert!(status.env_auto);
    assert!(status.env_detectors >= 1, "{}", status.env_detectors);
    assert!(status
        .redact_rules
        .iter()
        .any(|r| r.id == "env:BISA_TEST_FAKE_TOKEN"
            && r.origin == bisa_engine::security::Origin::Builtin));
    let preview = engine.redact_preview("value fake-env-value-0001 here");
    assert_eq!(preview.count, 1);
    assert_eq!(preview.kinds, vec!["env:BISA_TEST_FAKE_TOKEN"]);
    assert!(!preview.text.contains("fake-env-value-0001"));

    engine
        .set_setting(
            SettingScope::Machine,
            None,
            "security.redactor.env_auto",
            json!(false),
        )
        .unwrap();
    let status = engine.security_status();
    assert!(!status.env_auto);
    assert_eq!(status.env_detectors, 0);
    assert!(!status.redact_rules.iter().any(|r| r.id.starts_with("env:")));
    assert_eq!(
        engine
            .redact_preview("value fake-env-value-0001 here")
            .count,
        0
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_guard_switched_off_says_so_in_the_status_and_judges_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    set(
        &engine,
        "security.guard.rules",
        json!([command_rule("no_fake_tool", "deny", "fake-tool")]),
    );
    set(&engine, "security.guard.enabled", json!(false));
    let status = engine.security_status();
    assert!(!status.guard_enabled);
    assert!(
        status.guard_rules.iter().any(|r| r.id == "no_fake_tool"),
        "the rules are still listed, idle"
    );
    let preview = engine.guard_preview("Bash", &json!({ "command": "fake-tool verify" }));
    assert_eq!(
        preview.verdict, "deny",
        "a preview reads the rules whatever the switch says"
    );
    assert!(
        bisa_engine::security::refuse_by_rules(engine.inner(), "fake-tool verify").is_none(),
        "with the guard off nothing is refused"
    );
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// Above the ceiling: the classifier for an auto goal, a person otherwise
// ---------------------------------------------------------------------------

/// Record the workflow, capture an **auto** goal on it and start the run —
/// designing is off in these engines, so the capture wakes nobody and the
/// named workflow starts at once, as `run_on` does for a manual goal.
fn run_on_auto(
    engine: &Engine,
    statement: &str,
    draft: bisa_store::NewWorkflow,
) -> (bisa_core::Goal, bisa_core::WorkflowRun) {
    let wf = engine.create_workflow(draft).unwrap();
    let goal = engine
        .submit_goal(bisa_engine::SubmitRequest {
            mode: bisa_core::GoalMode::Auto,
            workflow: Some(wf.id),
            ..bisa_engine::SubmitRequest::captured(statement)
        })
        .unwrap();
    assert!(goal.mode.unattended(), "the capture is auto");
    let run = engine
        .start_run(goal.id, std::collections::BTreeMap::new())
        .unwrap();
    (goal, run)
}

/// Keep every step's own ceiling in an auto goal — `goals.auto.ceiling =
/// step` — so a `write` step's command is above it and the classifier's to
/// read, as every auto goal's was before a `write` step ran commands alone.
fn keeps_step_ceiling(engine: &Engine) {
    set(engine, "goals.auto.ceiling", json!("step"));
}

/// A worker that runs one turn and asks for nothing.
fn quiet_worker() -> MockAdapter {
    MockAdapter {
        id: "mock".into(),
        script: Some(vec![
            SessionEvent::Progress(ProgressEvent::TurnEnded),
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: Outcome::Completed,
                is_terminal: true,
            }),
        ]),
        ..Default::default()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_auto_goal_runs_a_call_above_the_ceiling_when_the_classifier_says_safe() {
    let dir = tempfile::tempdir().unwrap();
    // `Exec` above the step's `Write` ceiling, and no rule with an opinion —
    // the step's own ceiling kept, since lifted a `write` step's command
    // would not be above it at all.
    let worker = asking_worker("fake-tool build --release");
    let answered = Arc::clone(&worker.answered);
    let classifier = classifier_saying("SAFE");
    let asked = Arc::clone(&classifier.prompts);
    let engine = engine_with_classifier(&dir, worker, classifier);
    keeps_step_ceiling(&engine);
    let mut rx = engine.events();
    let (goal, _) = run_on_auto(
        &engine,
        "unattended",
        new_workflow("unattended", vec![agent_step("run", "mock")]),
    );
    wait_for(&mut rx, "the item ends", |e| {
        matches!(&e.payload, EnginePayload::ExecutionEnded { .. })
    })
    .await;
    let answers = answered.lock().unwrap().clone();
    assert!(
        matches!(&answers[0].1, InputAnswer::Allow { .. }),
        "the classifier's safe is the answer: {:?}",
        answers[0].1
    );
    let prompt = asked
        .lock()
        .unwrap()
        .first()
        .cloned()
        .expect("the classifier read the call");
    assert!(prompt.contains("fake-tool build"), "{prompt}");
    let facts = guard_facts(&engine, goal.id);
    assert!(
        facts.iter().any(|f| f.0 == GuardVerdict::Allowed
            && f.1 == GuardJudge::Classifier
            && f.2.as_deref() == Some(bisa_engine::security::CEILING_RULE)),
        "recorded as the classifier's allow under the ceiling's name: {facts:?}"
    );
    assert!(
        !facts.iter().any(|f| f.0 == GuardVerdict::Asked),
        "nobody was asked: {facts:?}"
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_auto_goal_asks_above_the_ceiling_when_the_classifier_finds_it_harmful() {
    let dir = tempfile::tempdir().unwrap();
    let worker = asking_worker("fake-tool wipe --everything");
    let answered = Arc::clone(&worker.answered);
    let engine = engine_with_classifier(
        &dir,
        worker,
        classifier_saying("HARMFUL: it wipes the working tree."),
    );
    keeps_step_ceiling(&engine);
    let mut rx = engine.events();
    run_on_auto(
        &engine,
        "harmful above",
        new_workflow("harmful-above", vec![agent_step("run", "mock")]),
    );
    let (gate_id, question) = opened_gate(&mut rx).await;
    assert!(
        question.contains("wipes the working tree"),
        "the classifier's reason reaches the person: {question}"
    );
    assert_eq!(
        engine.gate(&gate_id).unwrap().subject,
        "permission:Bash",
        "the ceiling's ask wears the permission subject"
    );
    engine
        .decide(&gate_id, false, Some("no"), None, None)
        .unwrap();
    wait_for(&mut rx, "the item ends", |e| {
        matches!(&e.payload, EnginePayload::ExecutionEnded { .. })
    })
    .await;
    assert!(matches!(
        &answered.lock().unwrap()[0].1,
        InputAnswer::Deny { .. }
    ));
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_auto_goal_refuses_above_the_ceiling_outright_when_harmful_means_deny() {
    let dir = tempfile::tempdir().unwrap();
    let worker = asking_worker("fake-tool wipe --everything");
    let answered = Arc::clone(&worker.answered);
    let engine = engine_with_classifier(&dir, worker, classifier_saying("HARMFUL: it wipes"));
    set(&engine, "security.classifier.on_harmful", json!("deny"));
    keeps_step_ceiling(&engine);
    let mut rx = engine.events();
    run_on_auto(
        &engine,
        "refused above",
        new_workflow("refused-above", vec![agent_step("run", "mock")]),
    );
    wait_for(&mut rx, "the item ends", |e| {
        matches!(&e.payload, EnginePayload::ExecutionEnded { .. })
    })
    .await;
    assert!(
        matches!(&answered.lock().unwrap()[0].1, InputAnswer::Deny { reason } if reason.contains("wipes"))
    );
    assert!(
        engine
            .inner()
            .security
            .recent()
            .iter()
            .all(|d| d.verdict != GuardVerdict::Asked),
        "no gate was opened"
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_auto_goal_asks_above_the_ceiling_when_the_classifier_is_off() {
    let dir = tempfile::tempdir().unwrap();
    let worker = asking_worker("fake-tool build");
    let engine = engine_with_classifier(&dir, worker, classifier_saying("SAFE"));
    set(&engine, "security.classifier.enabled", json!(false));
    keeps_step_ceiling(&engine);
    let mut rx = engine.events();
    run_on_auto(
        &engine,
        "classifier off",
        new_workflow("classifier-off", vec![agent_step("run", "mock")]),
    );
    let (gate_id, question) = opened_gate(&mut rx).await;
    assert!(
        question.contains("the classifier is off"),
        "the reason is said: {question}"
    );
    engine.decide(&gate_id, false, None, None, None).unwrap();
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_auto_goal_asks_above_the_ceiling_when_the_workspace_says_ask() {
    let dir = tempfile::tempdir().unwrap();
    let worker = asking_worker("fake-tool build");
    let classifier = classifier_saying("SAFE");
    let asked = Arc::clone(&classifier.prompts);
    let engine = engine_with_classifier(&dir, worker, classifier);
    set(&engine, "goals.auto.permissions", json!("ask"));
    keeps_step_ceiling(&engine);
    let mut rx = engine.events();
    run_on_auto(
        &engine,
        "asks anyway",
        new_workflow("asks-anyway", vec![agent_step("run", "mock")]),
    );
    let (gate_id, question) = opened_gate(&mut rx).await;
    assert!(question.contains("Allow `Bash`?"), "{question}");
    assert!(
        asked.lock().unwrap().is_empty(),
        "the classifier was never asked"
    );
    engine.decide(&gate_id, true, None, None, None).unwrap();
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_auto_goal_runs_a_command_on_a_writing_step_with_nobody_asked_and_no_classifier() {
    let dir = tempfile::tempdir().unwrap();
    // `Exec` on a `write` step, no rule with an opinion — and an auto goal,
    // whose `write` step runs commands on its own (`goals.auto.ceiling`'s
    // default): within the ceiling, so neither the classifier nor a person
    // hears of it, and nothing is recorded, as for any read.
    let worker = asking_worker("fake-tool build --release");
    let answered = Arc::clone(&worker.answered);
    let classifier = classifier_saying("SAFE");
    let asked = Arc::clone(&classifier.prompts);
    let engine = engine_with_classifier(&dir, worker, classifier);
    let mut rx = engine.events();
    let (goal, _) = run_on_auto(
        &engine,
        "runs alone",
        new_workflow("runs-alone", vec![agent_step("run", "mock")]),
    );
    let mut gate_opened = false;
    wait_for(&mut rx, "the item ends", |e| {
        gate_opened |= matches!(&e.payload, EnginePayload::GateOpened { .. });
        matches!(&e.payload, EnginePayload::ExecutionEnded { .. })
    })
    .await;
    let answers = answered.lock().unwrap().clone();
    assert!(
        matches!(&answers[0].1, InputAnswer::Allow { .. }),
        "the command runs: {:?}",
        answers[0].1
    );
    assert!(!gate_opened, "nobody was asked");
    assert!(
        asked.lock().unwrap().is_empty(),
        "the classifier never read an ordinary command"
    );
    assert!(
        guard_facts(&engine, goal.id).is_empty(),
        "within the ceiling nothing is recorded: {:?}",
        guard_facts(&engine, goal.id)
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_auto_goals_read_only_step_still_has_the_classifier_read_above_it() {
    let dir = tempfile::tempdir().unwrap();
    // A `read` step is read-only by the design's word: a command above it is
    // the classifier's to read, whatever a `write` step may do alone.
    let worker = asking_worker("fake-tool build");
    let answered = Arc::clone(&worker.answered);
    let classifier = classifier_saying("SAFE");
    let asked = Arc::clone(&classifier.prompts);
    let engine = engine_with_classifier(&dir, worker, classifier);
    let mut rx = engine.events();
    let (goal, _) = run_on_auto(
        &engine,
        "reads only",
        new_workflow(
            "reads-only",
            vec![agent_step_at("look", "mock", ToolTier::Read)],
        ),
    );
    wait_for(&mut rx, "the item ends", |e| {
        matches!(&e.payload, EnginePayload::ExecutionEnded { .. })
    })
    .await;
    assert!(matches!(
        &answered.lock().unwrap()[0].1,
        InputAnswer::Allow { .. }
    ));
    assert_eq!(
        asked.lock().unwrap().len(),
        1,
        "the classifier read the call above the read ceiling"
    );
    let facts = guard_facts(&engine, goal.id);
    assert!(
        facts.iter().any(|f| f.0 == GuardVerdict::Allowed
            && f.1 == GuardJudge::Classifier
            && f.2.as_deref() == Some(bisa_engine::security::CEILING_RULE)),
        "the classifier's allow under the ceiling's name: {facts:?}"
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_classify_rule_still_reads_the_classifier_in_an_auto_goal_whatever_the_ceiling() {
    let dir = tempfile::tempdir().unwrap();
    // The rules come first: a `classify` rule sends the command to the
    // classifier on a `write` step that would otherwise run it alone.
    let worker = asking_worker("fake-tool push --to staging");
    let answered = Arc::clone(&worker.answered);
    let classifier = classifier_saying("SAFE");
    let asked = Arc::clone(&classifier.prompts);
    let engine = engine_with_classifier(&dir, worker, classifier);
    set(
        &engine,
        "security.guard.rules",
        json!([command_rule("classify_fake_tool", "classify", "fake-tool")]),
    );
    let mut rx = engine.events();
    let (goal, _) = run_on_auto(
        &engine,
        "classified first",
        new_workflow("classified-first", vec![agent_step("run", "mock")]),
    );
    wait_for(&mut rx, "the item ends", |e| {
        matches!(&e.payload, EnginePayload::ExecutionEnded { .. })
    })
    .await;
    assert!(matches!(
        &answered.lock().unwrap()[0].1,
        InputAnswer::Allow { .. }
    ));
    assert_eq!(
        asked.lock().unwrap().len(),
        1,
        "a classify rule reads the classifier, lifted ceiling or not"
    );
    let facts = guard_facts(&engine, goal.id);
    assert!(
        facts.iter().any(|f| f.0 == GuardVerdict::Allowed
            && f.1 == GuardJudge::Classifier
            && f.2.as_deref() == Some("classify_fake_tool")),
        "recorded as the classifier's allow under the rule's name: {facts:?}"
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rule_decides_before_the_ceiling_in_an_auto_goal_too() {
    // A person's deny refuses without the classifier; a person's ask asks —
    // before the lifted ceiling too: a `write` step that runs commands on
    // its own still hears a rule's refusal and a rule's question.
    let dir = tempfile::tempdir().unwrap();
    let worker = asking_worker("fake-tool push --to production");
    let answered = Arc::clone(&worker.answered);
    let classifier = classifier_saying("SAFE");
    let asked = Arc::clone(&classifier.prompts);
    let engine = engine_with_classifier(&dir, worker, classifier);
    set(
        &engine,
        "security.guard.rules",
        json!([command_rule("deny_fake_tool", "deny", "fake-tool")]),
    );
    let mut rx = engine.events();
    let (goal, _) = run_on_auto(
        &engine,
        "rules first",
        new_workflow("rules-first", vec![agent_step("run", "mock")]),
    );
    wait_for(&mut rx, "the item ends", |e| {
        matches!(&e.payload, EnginePayload::ExecutionEnded { .. })
    })
    .await;
    assert!(matches!(
        &answered.lock().unwrap()[0].1,
        InputAnswer::Deny { .. }
    ));
    assert!(
        asked.lock().unwrap().is_empty(),
        "a rule's refusal never reaches the classifier"
    );
    let facts = guard_facts(&engine, goal.id);
    assert!(facts
        .iter()
        .any(|f| f.0 == GuardVerdict::Denied && f.1 == GuardJudge::Rule));
    engine.shutdown().await;

    let dir = tempfile::tempdir().unwrap();
    let worker = asking_worker("fake-tool push --to production");
    let engine = engine_with_classifier(&dir, worker, classifier_saying("SAFE"));
    set(
        &engine,
        "security.guard.rules",
        json!([command_rule("ask_fake_tool", "ask", "fake-tool")]),
    );
    let mut rx = engine.events();
    run_on_auto(
        &engine,
        "asked first",
        new_workflow("asked-first", vec![agent_step("run", "mock")]),
    );
    let (gate_id, question) = opened_gate(&mut rx).await;
    assert!(question.contains("asks you first"), "{question}");
    assert_eq!(
        engine.gate(&gate_id).unwrap().subject,
        "guard:Bash",
        "a rule's ask wears the guard subject, whatever the mode"
    );
    engine.decide(&gate_id, false, None, None, None).unwrap();
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_answer_above_the_ceiling_is_remembered_on_the_goal() {
    let dir = tempfile::tempdir().unwrap();
    let worker = asking_worker_yielding("fake-tool build");
    let answered = Arc::clone(&worker.answered);
    let engine = engine_with(&dir, vec![worker]);
    let mut rx = engine.events();
    // A manual goal: the person answers above the ceiling — once.
    let (goal, _) = run_on(
        &engine,
        "remembered ceiling",
        new_workflow(
            "remembered-ceiling",
            chain(vec![
                agent_step("first", "mock"),
                agent_step("second", "mock"),
            ]),
        ),
    );
    let (gate_id, _) = opened_gate(&mut rx).await;
    assert_eq!(engine.gate(&gate_id).unwrap().subject, "permission:Bash");
    engine.decide(&gate_id, true, None, None, None).unwrap();
    let run = finished_run(&engine, goal.id).await;
    assert_eq!(run.outcome, Some(bisa_core::RunOutcome::Done), "{run:?}");
    let answers = answered.lock().unwrap().clone();
    assert_eq!(answers.len(), 2, "both sessions were answered");
    assert!(answers
        .iter()
        .all(|(_, a)| matches!(a, InputAnswer::Allow { .. })));
    let facts = guard_reasons(&engine, goal.id);
    assert_eq!(
        facts.iter().filter(|f| f.0 == GuardVerdict::Asked).count(),
        1,
        "the person was asked exactly once: {facts:?}"
    );
    assert!(
        facts.iter().any(|f| f.0 == GuardVerdict::Allowed
            && f.1 == GuardJudge::Person
            && f.2.as_deref() == Some(bisa_engine::security::REMEMBERED)),
        "the second call was the person's remembered answer: {facts:?}"
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_auto_goals_step_is_told_the_run_is_unattended_and_a_manual_ones_is_not() {
    let dir = tempfile::tempdir().unwrap();
    let worker = quiet_worker();
    let prompts = Arc::clone(&worker.prompts);
    let engine = engine_with(&dir, vec![worker]);
    let mut rx = engine.events();
    let (goal, _) = run_on_auto(
        &engine,
        "briefed",
        new_workflow("briefed", vec![agent_step("run", "mock")]),
    );
    finished_run(&engine, goal.id).await;
    let first = prompts.lock().unwrap().first().cloned().expect("a prompt");
    assert!(
        first.contains("This goal runs unattended"),
        "an auto goal's step hears it: {first}"
    );

    let (goal, _) = run_on(
        &engine,
        "watched",
        new_workflow("watched", vec![agent_step("run", "mock")]),
    );
    wait_for(&mut rx, "the item ends", |e| {
        matches!(&e.payload, EnginePayload::ExecutionEnded { .. })
    })
    .await;
    finished_run(&engine, goal.id).await;
    let last = prompts.lock().unwrap().last().cloned().expect("a prompt");
    assert!(
        !last.contains("This goal runs unattended"),
        "a manual goal's step is watched: {last}"
    );
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// Installed MCP servers
// ---------------------------------------------------------------------------

/// A person's rule on a tool name — exact, or a prefix ending in `*`.
fn tool_rule(id: &str, action: &str, name: &str) -> Value {
    json!({
        "id": id,
        "label": format!("{id} rule"),
        "action": action,
        "matcher": { "kind": "tool", "name": name }
    })
}

/// A judged harness puts an installed MCP server's tool through the same
/// funnel as a command: no rule names it, so the step's ceiling does — a
/// tool that creates is `Exec`, above a `Write` step, and the person is
/// asked under `tier_ceiling`; a person's own rule on the server's prefix
/// lets the same tool run without a question.
#[tokio::test(flavor = "multi_thread")]
async fn an_installed_mcp_servers_tool_is_judged_and_a_persons_prefix_rule_allows_it() {
    let dir = tempfile::tempdir().unwrap();
    let worker = asking_for(
        "mcp__gh__create_issue",
        "create_issue acme/site",
        json!({ "owner": "acme", "repo": "site", "title": "Fix the footer" }),
    );
    let answered = Arc::clone(&worker.answered);
    let engine = engine_with(&dir, vec![worker]);
    let mut rx = engine.events();
    let (goal, _) = run_on(
        &engine,
        "issue",
        new_workflow("issue", vec![agent_step("run", "mock")]),
    );

    let (gate_id, question) = opened_gate(&mut rx).await;
    assert!(
        question.starts_with("Allow `mcp__gh__create_issue`?"),
        "{question}"
    );
    assert!(question.contains("Fix the footer"), "{question}");
    assert_eq!(
        engine.gate(&gate_id).unwrap().subject,
        "permission:mcp__gh__create_issue"
    );
    engine.decide(&gate_id, true, None, None, None).unwrap();
    wait_for(&mut rx, "the item ends", |e| {
        matches!(&e.payload, EnginePayload::ExecutionEnded { .. })
    })
    .await;
    let facts = guard_facts(&engine, goal.id);
    assert!(
        facts
            .iter()
            .any(|f| f.0 == GuardVerdict::Asked && f.2.as_deref() == Some("tier_ceiling")),
        "{facts:?}"
    );
    assert!(matches!(
        &answered.lock().unwrap()[0].1,
        InputAnswer::Allow { .. }
    ));

    // The person allows the whole server: no gate, the rule's allow.
    set(
        &engine,
        "security.guard.rules",
        json!([tool_rule("gh_ok", "allow", "mcp__gh__*")]),
    );
    let (goal, _) = run_on(
        &engine,
        "issue again",
        new_workflow("issue-again", vec![agent_step("run", "mock")]),
    );
    wait_for(&mut rx, "the second item ends", |e| {
        matches!(&e.payload, EnginePayload::ExecutionEnded { .. })
    })
    .await;
    let facts = guard_facts(&engine, goal.id);
    assert_eq!(facts.len(), 1, "{facts:?}");
    assert_eq!(
        (facts[0].0, facts[0].1, facts[0].2.as_deref()),
        (GuardVerdict::Allowed, GuardJudge::Rule, Some("gh_ok"))
    );
    let answers = answered.lock().unwrap().clone();
    assert_eq!(answers.len(), 2);
    assert!(matches!(&answers[1].1, InputAnswer::Allow { .. }));
    engine.shutdown().await;
}

/// The MCP servers a launch was handed, by name.
fn mounted(spec: &SessionSpec) -> Vec<String> {
    spec.mcp_servers
        .iter()
        .map(|m| m.name().to_string())
        .collect()
}

/// An installed server is mounted where the guard judges the harness's tool
/// calls; an observed harness is not handed it — and is told — unless the
/// person sets `security.mcp.observed` to `allow`. The platform's own server
/// rides everywhere.
#[tokio::test(flavor = "multi_thread")]
async fn an_installed_mcp_server_rides_only_where_the_guard_judges_the_harness() {
    let dir = tempfile::tempdir().unwrap();
    let observed = MockAdapter {
        id: "mock".into(),
        ..Default::default()
    };
    let judged = MockAdapter {
        id: "mock-judged".into(),
        caps: MockAdapter::default().caps | bisa_core::HarnessCaps::TOOL_GUARD,
        ..Default::default()
    };
    let observed_launches = Arc::clone(&observed.launches);
    let observed_prompts = Arc::clone(&observed.prompts);
    let judged_launches = Arc::clone(&judged.launches);
    let judged_prompts = Arc::clone(&judged.prompts);
    let engine = engine_with(&dir, vec![observed, judged]);
    let ws = engine.workspace();
    let gh = McpId::new("gh").unwrap();
    ws.create_mcp(NewMcp {
        id: gh.clone(),
        description: "A code host".into(),
        tags: Default::default(),
        transport: McpServerConfig::Stdio {
            name: "gh".into(),
            command: "gh-mcp".into(),
            args: vec![],
            env: Default::default(),
            cwd: None,
        },
    })
    .unwrap();
    let mut on_observed = new_agent("Observed", "mock");
    on_observed.mcps = vec![gh.clone()];
    let on_observed = ws.add_agent(on_observed).unwrap();
    let mut on_judged = new_agent("Judged", "mock-judged");
    on_judged.mcps = vec![gh.clone()];
    let on_judged = ws.add_agent(on_judged).unwrap();

    run_on(
        &engine,
        "observed",
        new_workflow(
            "observed",
            vec![assigned_agent_step("run", "mock", on_observed.id.as_str())],
        ),
    );
    let spec = until("the observed launch", || {
        observed_launches.lock().unwrap().first().cloned()
    })
    .await;
    assert_eq!(mounted(&spec), ["bisa"], "the installed server is kept off");
    let prompt = until("the observed prompt", || {
        observed_prompts.lock().unwrap().first().cloned()
    })
    .await;
    assert!(
        prompt.contains("`gh`")
            && prompt.contains("not mounted in this session")
            && prompt.contains("security.mcp.observed"),
        "{prompt}"
    );

    run_on(
        &engine,
        "judged",
        new_workflow(
            "judged",
            vec![assigned_agent_step(
                "run",
                "mock-judged",
                on_judged.id.as_str(),
            )],
        ),
    );
    let spec = until("the judged launch", || {
        judged_launches.lock().unwrap().first().cloned()
    })
    .await;
    assert_eq!(mounted(&spec), ["bisa", "gh"]);
    assert_eq!(spec.mcp_servers[0].provenance, McpProvenance::Platform);
    assert_eq!(spec.mcp_servers[1].provenance, McpProvenance::Installed);
    let prompt = until("the judged prompt", || {
        judged_prompts.lock().unwrap().first().cloned()
    })
    .await;
    assert!(!prompt.contains("not mounted"), "{prompt}");

    set(&engine, "security.mcp.observed", json!("allow"));
    run_on(
        &engine,
        "observed, allowed",
        new_workflow(
            "observed-allowed",
            vec![assigned_agent_step("run", "mock", on_observed.id.as_str())],
        ),
    );
    let spec = until("the second observed launch", || {
        observed_launches.lock().unwrap().get(1).cloned()
    })
    .await;
    assert_eq!(mounted(&spec), ["bisa", "gh"], "the person allowed it");
    engine.shutdown().await;
}

/// The classifier is one more session of the general agent's harness, and a
/// harness may ask permission for its own calls before it answers. Such an
/// ask is about no goal and no conversation, so it is refused in words — the
/// classifier never gets a second classifier — and a classifier that then
/// says nothing is no verdict: the call goes to the person, never to an
/// allow. One extra session per guarded call is the cost, counted here.
#[tokio::test(flavor = "multi_thread")]
async fn a_classifier_that_asks_permission_itself_is_refused_and_no_verdict_goes_to_the_person() {
    let dir = tempfile::tempdir().unwrap();
    let worker = asking_worker("fake-net probe");
    // The classifier's own session runs the very command it was asked about
    // and stops for permission, then ends without a word.
    let classifier = MockAdapter {
        id: "mock-classifier".into(),
        input_request: Some(InputRequest::permission(
            "c1",
            "Bash",
            ToolTier::Exec,
            "fake-net probe",
            json!({ "command": "fake-net probe" }),
        )),
        script: Some(vec![
            SessionEvent::Lifecycle(LifecycleEvent::Started),
            SessionEvent::Progress(ProgressEvent::TurnStarted),
            SessionEvent::Progress(ProgressEvent::TurnEnded),
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: Outcome::Completed,
                is_terminal: true,
            }),
        ]),
        ..Default::default()
    };
    let classifier_answers = Arc::clone(&classifier.answered);
    let classifier_sessions = Arc::clone(&classifier.prompts);
    let engine = engine_with_classifier(&dir, worker, classifier);
    set(
        &engine,
        "security.guard.rules",
        json!([command_rule("classify_fake_net", "classify", "fake-net")]),
    );
    let mut rx = engine.events();
    run_on(
        &engine,
        "asks-itself",
        new_workflow("asks-itself", vec![agent_step("run", "mock")]),
    );
    let (gate_id, question) = opened_gate(&mut rx).await;
    assert!(question.contains("no verdict"), "{question}");

    let answers = classifier_answers.lock().unwrap().clone();
    assert_eq!(
        answers.len(),
        1,
        "the classifier's own ask was answered once: {answers:?}"
    );
    match &answers[0].1 {
        InputAnswer::Deny { reason } => assert!(
            reason.contains("outside any goal"),
            "refused because nobody is there to ask, not by a rule: {reason}"
        ),
        other => panic!("the classifier's ask was not refused: {other:?}"),
    }
    assert_eq!(
        classifier_sessions.lock().unwrap().len(),
        1,
        "one guarded call costs one classifier session, and the classifier's own call costs none"
    );
    engine.decide(&gate_id, false, None, None, None).unwrap();
    wait_for(&mut rx, "the item ends", |e| {
        matches!(&e.payload, EnginePayload::ExecutionEnded { .. })
    })
    .await;
    engine.shutdown().await;
}

/// The classifier's verdicts are cached per subject; with the classifier set
/// to the Decision-Making Agent, a verdict is worth what the decision settings
/// were when it was given. A `decisions.*` change ends every cached verdict,
/// as a `security.*` change does.
#[tokio::test(flavor = "multi_thread")]
async fn a_decisions_setting_change_ends_the_cached_verdicts() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with_classifier(&dir, quiet_worker(), classifier_saying("SAFE"));
    let inner = engine.inner();
    let judge = bisa_engine::security::Judge {
        classifier: true,
        ..bisa_engine::security::Judge::platform(None, None)
    };
    let input = json!({ "command": "fake-net probe" });
    set(
        &engine,
        "security.guard.rules",
        json!([command_rule("classify_fake_net", "classify", "fake-net")]),
    );
    let first =
        bisa_engine::security::decide_tool(inner, "Bash", &input, judge.clone(), None).await;
    assert!(
        matches!(first, bisa_engine::security::Outcome::Allow { .. }),
        "{first:?}"
    );
    let cached_before = inner.security.cached_verdicts();
    assert_eq!(cached_before, 1, "the verdict is cached");
    set(&engine, "decisions.confidence.security", json!(0.99));
    assert_eq!(inner.security.cached_verdicts(), 0, "the change ended it");
    engine.shutdown().await;
}

/// A settings layer that cannot be read — a torn write, a hand edit — is not
/// *no rules here*: the status names it as a problem, the log says so, and
/// the rules of the layers that do read still apply.
#[tokio::test(flavor = "multi_thread")]
async fn an_unreadable_settings_layer_is_a_named_problem_never_a_policy_that_fails_open() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with_classifier(&dir, quiet_worker(), classifier_saying("SAFE"));
    set(
        &engine,
        "security.guard.rules",
        json!([command_rule("deny_fake_net", "deny", "fake-net")]),
    );
    assert!(engine.security_status().problems.is_empty());

    // The machine layer is torn under the running engine.
    let machine = engine.workspace().paths().machine_settings();
    std::fs::write(&machine, "{ \"security.guard.builtins_off\": [").unwrap();
    engine.inner().security.invalidate();
    let status = engine.security_status();
    let named: Vec<&str> = status.problems.iter().map(|p| p.rule.as_str()).collect();
    assert!(
        named.iter().any(|r| r.contains("Machine")),
        "the unreadable layer is a problem on the status: {named:?}"
    );
    assert!(
        status
            .guard_rules
            .iter()
            .any(|r| r.id == "deny_fake_net" && r.enabled),
        "the workspace's rules still apply"
    );
    assert!(
        status
            .guard_rules
            .iter()
            .any(|r| r.id == "publishing" && r.enabled),
        "and the built-ins stand, since nothing readable switched one off"
    );
    engine.shutdown().await;
}
