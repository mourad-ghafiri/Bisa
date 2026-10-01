//! Adapter integration tests driven by stub shell scripts that speak each
//! harness's wire format — CI-safe, no real harness binaries required.

use std::path::{Path, PathBuf};
use std::time::Duration;

use bisa_adapters::util::Shared;
use bisa_core::ToolTier;
use bisa_harness::{
    HarnessAdapter, HarnessError, LifecycleEvent, Outcome, ProgressEvent, SessionEvent, SessionSpec,
};
use futures::StreamExt;

fn write_stub(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/bash\n{body}")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn spec(dir: &Path, prompt: &str) -> SessionSpec {
    SessionSpec {
        work_item: None,
        cwd: dir.to_path_buf(),
        prompt: prompt.to_string(),
        model: None,
        effort: None,
        mcp_servers: vec![],
        env: Default::default(),
        env_remove: Vec::new(),
        tier_ceiling: ToolTier::Exec,
        output_schema: None,
        skills: vec![],
    }
}

/// Collect events until a Lifecycle::Ended (any is_terminal) or timeout.
/// How long a stub subprocess gets to produce its events.
///
/// This is a hang-guard, not a performance assertion: the stubs echo two lines
/// and exit, so any real failure shows up as "no events" rather than as a slow
/// one. It is generous because a tight bound turns machine load into a red
/// suite — these tests spawn shells, and a loaded laptop can make a 10s bound
/// lose to scheduling while proving nothing about the adapter.
const STUB_DEADLINE: Duration = Duration::from_secs(60);

async fn collect_until_end(
    stream: &mut (impl futures::Stream<Item = SessionEvent> + Unpin),
    max: Duration,
) -> Vec<SessionEvent> {
    let mut events = Vec::new();
    let deadline = tokio::time::Instant::now() + max;
    loop {
        let next = tokio::time::timeout_at(deadline, stream.next()).await;
        match next {
            Ok(Some(ev)) => {
                let is_end = matches!(ev, SessionEvent::Lifecycle(LifecycleEvent::Ended { .. }));
                events.push(ev);
                if is_end {
                    return events;
                }
            }
            Ok(None) => return events,
            Err(_) => panic!(
                "timed out waiting for Ended; got {} events: {events:?}",
                events.len()
            ),
        }
    }
}

fn has_text_delta(events: &[SessionEvent], needle: &str) -> bool {
    events.iter().any(|e| {
        matches!(e, SessionEvent::Progress(ProgressEvent::TextDelta { text }) if text.contains(needle))
    })
}

fn has_thinking_delta(events: &[SessionEvent], needle: &str) -> bool {
    events.iter().any(|e| {
        matches!(e, SessionEvent::Progress(ProgressEvent::ThinkingDelta { text }) if text.contains(needle))
    })
}

fn ended_completed(events: &[SessionEvent]) -> bool {
    events.iter().any(|e| {
        matches!(
            e,
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: Outcome::Completed,
                ..
            })
        )
    })
}

// ---------------------------------------------------------------------------
// claude-code
// ---------------------------------------------------------------------------

#[tokio::test]
async fn claude_code_stub_full_turn() {
    let dir = tempfile::tempdir().unwrap();
    let stub = write_stub(
        dir.path(),
        "claude",
        r#"
# Speak the stream-json protocol: read the first user line, then emit a turn.
read -r _line
echo '{"type":"system","subtype":"init","session_id":"sess-42","model":"m"}'
echo '{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"working on it"}}}'
echo '{"type":"assistant","message":{"content":[{"type":"text","text":"working on it"},{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}]}}'
echo '{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","is_error":false}]}}'
echo '{"type":"result","subtype":"success","is_error":false,"session_id":"sess-42","total_cost_usd":0.05,"usage":{"input_tokens":100,"output_tokens":50}}'
# Stay alive for potential follow-ups until stdin closes.
while read -r _l; do :; done
"#,
    );
    let adapter = bisa_adapters::claude_code::ClaudeCodeAdapter {
        program: stub.display().to_string(),
        ..Default::default()
    };
    let session = adapter
        .launch(spec(dir.path(), "do the thing"))
        .await
        .unwrap();
    let mut stream = session.subscribe();
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;

    assert!(has_text_delta(&events, "working on it"));
    assert!(events.iter().any(|e| matches!(
        e,
        SessionEvent::Progress(ProgressEvent::ToolStarted { name, tier: ToolTier::Exec, .. }) if name == "Bash"
    )));
    assert!(events.iter().any(|e| matches!(
        e,
        SessionEvent::Progress(ProgressEvent::ToolEnded { name, ok: true }) if name == "Bash"
    )));
    assert!(events.iter().any(|e| matches!(
        e,
        SessionEvent::Progress(ProgressEvent::CostDelta {
            input_tokens: 100,
            output_tokens: 50,
            usd_cents: 5
        })
    )));
    // Turn end is non-terminal: the session accepts follow-ups.
    assert!(events.iter().any(|e| matches!(
        e,
        SessionEvent::Lifecycle(LifecycleEvent::Ended {
            outcome: Outcome::Completed,
            is_terminal: false
        })
    )));

    // Resume token carries the native session id.
    let token = session.resume_token().expect("resume token");
    assert_eq!(token.adapter_id, "claude-code");
    assert_eq!(token.native_id, "sess-42");

    // Steer is honestly unsupported.
    assert!(matches!(
        session.steer("x".into()).await,
        Err(HarnessError::NotSupported("steer"))
    ));

    // Dispose closes stdin; the stub exits; terminal end follows.
    let mut stream2 = session.subscribe();
    session.dispose().await.unwrap();
    let tail = collect_until_end(&mut stream2, STUB_DEADLINE).await;
    assert!(
        tail.iter().any(|e| e.is_terminal_end()),
        "expected terminal end, got {tail:?}"
    );
}

#[tokio::test]
async fn claude_code_probe_unavailable_for_missing_binary() {
    let adapter = bisa_adapters::claude_code::ClaudeCodeAdapter {
        program: "definitely-not-a-real-binary-xyz".into(),
        ..Default::default()
    };
    let probe = adapter.probe().await;
    assert!(!probe.available);
    let err = adapter
        .launch(spec(Path::new("/tmp"), "x"))
        .await
        .err()
        .unwrap();
    assert!(err.is_unavailable(), "{err:?}");
}

// ---------------------------------------------------------------------------
// pi RPC
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pi_rpc_stub_turn_steer_and_resume_token() {
    let dir = tempfile::tempdir().unwrap();
    let stub = write_stub(
        dir.path(),
        "pi",
        r#"
# JSONL RPC: answer get_state, then run a canned turn per prompt; ack steer.
while read -r line; do
  case "$line" in
    *'"type":"get_state"'*)
      echo '{"type":"response","command":"get_state","success":true,"id":"bisa-state","data":{"sessionId":"pi-7","sessionFile":"/tmp/pi-7.jsonl","isStreaming":false}}'
      ;;
    *'"type":"prompt"'*)
      echo '{"type":"response","command":"prompt","success":true}'
      echo '{"type":"agent_start"}'
      echo '{"type":"tool_execution_start","toolCallId":"c1","toolName":"read","args":{"path":"x"}}'
      echo '{"type":"tool_execution_end","toolCallId":"c1","toolName":"read","isError":false}'
      echo '{"type":"message_end","message":{"role":"assistant","content":[{"type":"text","text":"pi says hi"}]}}'
      echo '{"type":"agent_end"}'
      ;;
    *'"type":"steer"'*)
      echo '{"type":"response","command":"steer","success":true}'
      ;;
  esac
done
"#,
    );
    let adapter = bisa_adapters::pi_rpc::PiRpcAdapter {
        program: stub.display().to_string(),
        extra_args: vec![],
    };
    let session = adapter.launch(spec(dir.path(), "hello pi")).await.unwrap();
    let mut stream = session.subscribe();
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;

    assert!(has_text_delta(&events, "pi says hi"));
    assert!(ended_completed(&events));
    assert!(events.iter().any(|e| matches!(
        e,
        SessionEvent::Progress(ProgressEvent::ToolStarted { name, tier: ToolTier::Read, .. }) if name == "read"
    )));

    // Steer is supported and acked by the stub.
    session.steer("also do Y".into()).await.unwrap();

    let token = session.resume_token().expect("token");
    assert_eq!(token.adapter_id, "pi");
    assert_eq!(token.native_id, "pi-7");
    assert_eq!(
        token.transcript_path.as_deref(),
        Some(Path::new("/tmp/pi-7.jsonl"))
    );
    session.dispose().await.unwrap();
}

// ---------------------------------------------------------------------------
// omp (RPC v2: ready frame, negotiation, chunked frames)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn omp_stub_negotiates_v2_and_reassembles_chunks() {
    let dir = tempfile::tempdir().unwrap();
    // The chunked payload is a message_end event split into two base64 chunks.
    let payload =
        r#"{"type":"message_end","message":{"content":[{"type":"text","text":"chunked hello"}]}}"#;
    let (a, b) = payload.split_at(40);
    let b64a = {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.encode(a)
    };
    let b64b = {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.encode(b)
    };
    let stub = write_stub(
        dir.path(),
        "omp",
        &format!(
            r#"
echo '{{"type":"ready","protocolVersion":1,"supportedProtocolVersions":[1,2],"maxFrameBytes":1048576,"maxReassembledFrameBytes":67108864}}'
while read -r line; do
  case "$line" in
    *'"type":"negotiate_protocol"'*)
      echo '{{"type":"response","command":"negotiate_protocol","success":true,"id":"bisa-proto"}}'
      ;;
    *'"type":"get_state"'*)
      echo '{{"type":"response","command":"get_state","success":true,"id":"bisa-state","data":{{"sessionId":"omp-1"}}}}'
      ;;
    *'"type":"prompt"'*)
      echo '{{"type":"agent_start"}}'
      echo '{{"type":"rpc_chunk","chunkId":"c-1","index":0,"count":2,"byteLength":{len},"data":"{b64a}"}}'
      echo '{{"type":"rpc_chunk","chunkId":"c-1","index":1,"count":2,"byteLength":{len},"data":"{b64b}"}}'
      echo '{{"type":"agent_end","isTerminal":false}}'
      echo '{{"type":"agent_end"}}'
      ;;
  esac
done
"#,
            len = payload.len(),
        ),
    );
    let adapter = bisa_adapters::omp::OmpAdapter {
        program: stub.display().to_string(),
        extra_args: vec![],
    };
    let session = adapter.launch(spec(dir.path(), "hello omp")).await.unwrap();
    let mut stream = session.subscribe();
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;

    // The chunked message_end was reassembled and mapped to a TextDelta.
    assert!(has_text_delta(&events, "chunked hello"), "{events:?}");
    // Exactly one turn end: the isTerminal:false agent_end was suppressed.
    let ends = events
        .iter()
        .filter(|e| matches!(e, SessionEvent::Lifecycle(LifecycleEvent::Ended { .. })))
        .count();
    assert_eq!(ends, 1);
    assert_eq!(session.resume_token().unwrap().native_id, "omp-1");
    session.dispose().await.unwrap();
}

// ---------------------------------------------------------------------------
// ACP (generic JSON-RPC agent)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn acp_stub_handshake_prompt_and_permission() {
    let dir = tempfile::tempdir().unwrap();
    let stub = write_stub(
        dir.path(),
        "acp-agent",
        r#"
while read -r line; do
  case "$line" in
    *'"method":"initialize"'*)
      echo '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{"loadSession":true}}}'
      ;;
    *'"method":"session/new"'*)
      echo '{"jsonrpc":"2.0","id":2,"result":{"sessionId":"acp-9"}}'
      ;;
    *'"method":"session/prompt"'*)
      echo '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"acp-9","update":{"sessionUpdate":"agent_thought_chunk","content":{"type":"text","text":"acp thought"}}}}'
      echo '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"acp-9","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"acp hello"}}}}'
      echo '{"jsonrpc":"2.0","id":77,"method":"session/request_permission","params":{"sessionId":"acp-9","toolCall":{"title":"read file","kind":"read"},"options":[{"optionId":"ok","name":"Allow","kind":"allow_once"},{"optionId":"no","name":"Reject","kind":"reject_once"}]}}'
      # Wait for the permission answer, then finish the turn.
      read -r answer
      case "$answer" in
        *'"optionId":"ok"'*)
          echo '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"acp-9","update":{"sessionUpdate":"tool_call","title":"read file","kind":"read","status":"in_progress"}}}'
          ;;
      esac
      echo '{"jsonrpc":"2.0","id":3,"result":{"stopReason":"end_turn"}}'
      ;;
  esac
done
"#,
    );
    let adapter = bisa_adapters::acp::AcpAdapter::new(
        "acp:stub",
        "Stub ACP",
        stub.display().to_string(),
        vec![],
    );
    let session = adapter.launch(spec(dir.path(), "hello acp")).await.unwrap();
    let mut stream = session.subscribe();
    // The adapter surfaces the request and waits: nothing is answered until
    // the engine (here, the test) says so. The stub only emits `tool_call`
    // after an allow answer, which is the proof the answer reached it.
    let mut before = Vec::new();
    let request_id = loop {
        let ev = tokio::time::timeout(STUB_DEADLINE, stream.next())
            .await
            .expect("an input request before the deadline")
            .expect("stream open");
        if let SessionEvent::Lifecycle(LifecycleEvent::InputRequested { request }) = &ev {
            assert!(
                matches!(&request.kind, bisa_harness::InputKind::Permission { tool_name, .. } if tool_name == "read file")
            );
            let id = request.id.clone();
            before.push(ev);
            break id;
        }
        assert!(
            !ev.is_terminal_end(),
            "the session ended without asking: {before:?}"
        );
        before.push(ev);
    };
    assert_eq!(session.phase(), bisa_harness::Phase::AwaitingInput);
    session
        .answer(&request_id, bisa_harness::InputAnswer::ALLOW)
        .await
        .unwrap();
    let mut events = before;
    events.extend(collect_until_end(&mut stream, STUB_DEADLINE).await);

    assert!(has_text_delta(&events, "acp hello"), "{events:?}");
    assert!(
        has_thinking_delta(&events, "acp thought"),
        "the agent's thought streams beside its words: {events:?}"
    );
    assert!(events.iter().any(|e| matches!(
        e,
        SessionEvent::Lifecycle(LifecycleEvent::InputResolved { id }) if id == &request_id
    )));
    assert!(events.iter().any(|e| matches!(
        e,
        SessionEvent::Progress(ProgressEvent::ToolStarted { name, .. }) if name == "read file"
    )));
    assert!(ended_completed(&events));
    assert_eq!(session.resume_token().unwrap().native_id, "acp-9");
    session.dispose().await.unwrap();
}

/// The engine launches a session with no prompt and prompts it at once —
/// before an ACP agent has answered `initialize`, let alone `session/new`.
/// The prompt is the session's first: kept while the session starts, sent
/// the moment it exists, never refused as busy. A second one given while the
/// first still waits is.
#[tokio::test]
async fn acp_a_prompt_given_while_the_session_starts_is_its_first() {
    let dir = tempfile::tempdir().unwrap();
    let stub = write_stub(
        dir.path(),
        "acp-agent",
        r#"
while read -r line; do
  case "$line" in
    *'"method":"initialize"'*)
      echo '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1}}'
      ;;
    *'"method":"session/new"'*)
      echo '{"jsonrpc":"2.0","id":2,"result":{"sessionId":"acp-3"}}'
      ;;
    *'"method":"session/prompt"'*'first words'*)
      echo '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"acp-3","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"heard the first words"}}}}'
      id=$(printf '%s' "$line" | sed -E 's/.*"id":([0-9]+).*/\1/')
      echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"stopReason\":\"end_turn\"}}"
      ;;
  esac
done
"#,
    );
    let adapter = bisa_adapters::acp::AcpAdapter::new(
        "acp:stub",
        "Stub ACP",
        stub.display().to_string(),
        vec![],
    );
    let session = adapter.launch(spec(dir.path(), "")).await.unwrap();
    let mut stream = session.subscribe();
    session
        .prompt("first words".into())
        .await
        .expect("the first prompt is kept for the session, not refused");
    assert!(
        matches!(
            session.prompt("second words".into()).await,
            Err(HarnessError::Busy)
        ),
        "one prompt waits for the session; another is refused"
    );
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;
    assert!(
        has_text_delta(&events, "heard the first words"),
        "{events:?}"
    );
    assert!(ended_completed(&events), "{events:?}");
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, SessionEvent::Progress(ProgressEvent::TurnStarted)))
            .count(),
        1,
        "one turn: {events:?}"
    );
    session.dispose().await.unwrap();
}

/// A session loaded again is the session that was asked for. The answer to
/// `session/load` is empty — the protocol has the agent replay the whole
/// conversation as `session/update` first, then answer with no id
/// (https://agentclientprotocol.com/protocol/session-setup, read
/// 2026-09-29) — so the id is the one in the token, and what is replayed is
/// the past: said once already, never streamed as the revived session's own
/// words. A prompt then reaches the agent under that id.
#[tokio::test]
async fn acp_a_loaded_session_keeps_its_id_and_its_replay_is_not_said_again() {
    let dir = tempfile::tempdir().unwrap();
    let stub = write_stub(
        dir.path(),
        "acp-agent",
        r#"
while read -r line; do
  case "$line" in
    *'"method":"initialize"'*)
      echo '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{"loadSession":true}}}'
      ;;
    *'"method":"session/load"'*'"sessionId":"acp-7"'*)
      echo '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"acp-7","update":{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"what was asked before"}}}}'
      echo '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"acp-7","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"what was said before"}}}}'
      echo '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"acp-7","update":{"sessionUpdate":"tool_call","toolCallId":"c1","title":"an old tool","kind":"read","status":"completed"}}}'
      echo '{"jsonrpc":"2.0","id":2,"result":{}}'
      ;;
    *'"method":"session/prompt"'*'"sessionId":"acp-7"'*)
      echo '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"acp-7","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"said after the load"}}}}'
      id=$(printf '%s' "$line" | sed -E 's/.*"id":([0-9]+).*/\1/')
      echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"stopReason\":\"end_turn\"}}"
      ;;
  esac
done
"#,
    );
    let adapter = bisa_adapters::acp::AcpAdapter::new(
        "acp:stub",
        "Stub ACP",
        stub.display().to_string(),
        vec![],
    );
    let token = bisa_harness::ResumeToken {
        adapter_id: "acp:stub".into(),
        native_id: "acp-7".into(),
        cwd: dir.path().to_path_buf(),
        transcript_path: None,
        model: None,
        effort: None,
    };
    let session = adapter.attach(&token).await.unwrap();
    let mut stream = session.subscribe();

    // Loaded: idle, under the id it was asked for.
    let deadline = tokio::time::Instant::now() + STUB_DEADLINE;
    while session.phase() != bisa_harness::Phase::Idle {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the session never finished loading: {:?}",
            session.phase()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(session.resume_token().unwrap().native_id, "acp-7");

    // A prompt is taken, not refused as busy, and its answer streams.
    session.prompt("and now?".into()).await.unwrap();
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;
    assert!(has_text_delta(&events, "said after the load"), "{events:?}");
    assert!(ended_completed(&events), "{events:?}");
    assert!(
        !has_text_delta(&events, "what was said before"),
        "the replay is the past, not the session's words: {events:?}"
    );
    assert!(
        !events.iter().any(|e| matches!(
            e,
            SessionEvent::Progress(ProgressEvent::ToolStarted { name, .. }) if name == "an old tool"
        )),
        "a tool that ran before the load did not run again: {events:?}"
    );
    session.dispose().await.unwrap();
}

// ---------------------------------------------------------------------------
// codex (one-shot per turn)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn codex_stub_oneshot_turn() {
    let dir = tempfile::tempdir().unwrap();
    let stub = write_stub(
        dir.path(),
        "codex",
        r#"
# argv: exec --json <prompt>   (or: exec resume <id> --json <prompt>)
echo '{"type":"thread.started","thread_id":"cx-3"}'
echo '{"type":"turn.started"}'
echo '{"type":"item.started","item":{"id":"item_1","type":"command_execution","command":"ls -la","status":"in_progress"}}'
echo '{"type":"item.completed","item":{"id":"item_1","type":"command_execution","command":"ls -la","exit_code":0,"status":"completed"}}'
echo '{"type":"item.completed","item":{"id":"item_2","type":"agent_message","text":"codex done"}}'
echo '{"type":"turn.completed","usage":{"input_tokens":10,"output_tokens":5}}'
exit 0
"#,
    );
    let adapter = bisa_adapters::codex::CodexAdapter {
        program: stub.display().to_string(),
    };
    let session = adapter
        .launch(spec(dir.path(), "list files"))
        .await
        .unwrap();
    let mut stream = session.subscribe();
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;

    assert!(has_text_delta(&events, "codex done"));
    assert!(events.iter().any(|e| matches!(
        e,
        SessionEvent::Progress(ProgressEvent::ToolStarted { name, tier: ToolTier::Exec, .. }) if name == "shell"
    )));
    assert!(ended_completed(&events));
    let token = session.resume_token().expect("token");
    assert_eq!(token.native_id, "cx-3");

    // A second prompt resumes by native id (`exec resume cx-3`).
    let mut stream2 = session.subscribe();
    session.prompt("again".into()).await.unwrap();
    let events2 = collect_until_end(&mut stream2, STUB_DEADLINE).await;
    assert!(ended_completed(&events2));
    session.dispose().await.unwrap();
}

// ---------------------------------------------------------------------------
// opencode (one-shot per turn)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn opencode_stub_oneshot_turn() {
    let dir = tempfile::tempdir().unwrap();
    let stub = write_stub(
        dir.path(),
        "opencode",
        r#"
echo '{"type":"text","sessionID":"ses_1","part":{"type":"text","text":"oc hello"}}'
echo '{"type":"tool_use","sessionID":"ses_1","part":{"type":"tool","callID":"c1","tool":"bash","state":{"status":"completed","input":{"command":"ls"},"output":"ok"}}}'
exit 0
"#,
    );
    let adapter = bisa_adapters::opencode::OpencodeAdapter {
        program: stub.display().to_string(),
        ..Default::default()
    };
    let session = adapter.launch(spec(dir.path(), "hi")).await.unwrap();
    let mut stream = session.subscribe();
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;

    assert!(has_text_delta(&events, "oc hello"), "{events:?}");
    assert!(events.iter().any(|e| matches!(
        e,
        SessionEvent::Progress(ProgressEvent::ToolStarted { name, .. }) if name == "bash"
    )));
    assert!(ended_completed(&events));
    assert_eq!(session.resume_token().unwrap().native_id, "ses_1");
    session.dispose().await.unwrap();
}

// ---------------------------------------------------------------------------
// custom-json
// ---------------------------------------------------------------------------

#[tokio::test]
async fn custom_json_stub_contract_and_malformed_resilience() {
    let dir = tempfile::tempdir().unwrap();
    let stub = write_stub(
        dir.path(),
        "my-agent",
        r#"
read -r _prompt
echo '{"type":"started"}'
echo 'this line is not json at all'
echo '{"type":"text","text":"custom says hi"}'
echo '{"broken json'
echo '{"type":"tool","name":"write","args":{"path":"a.txt"}}'
echo '{"type":"tool_end","name":"write","ok":true}'
echo '{"type":"result","output":{"answer":42}}'
echo '{"type":"ended","outcome":"completed"}'
while read -r _l; do :; done
"#,
    );
    let spec_custom = bisa_harness::CustomHarnessSpec {
        id: "my-agent".into(),
        label: "My Agent".into(),
        command: stub.display().to_string(),
        args: vec![],
        resume_args: vec![],
        env: Default::default(),
        install_hint: None,
    };
    let adapter = bisa_adapters::custom_json::CustomJsonAdapter::new(spec_custom);
    assert_eq!(adapter.id(), "custom:my-agent");
    let session = adapter.launch(spec(dir.path(), "go")).await.unwrap();
    let mut stream = session.subscribe();
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;

    assert!(has_text_delta(&events, "custom says hi"));
    assert!(events.iter().any(|e| matches!(
        e,
        SessionEvent::Progress(ProgressEvent::ToolStarted { name, tier: ToolTier::Write, .. }) if name == "write"
    )));
    // The structured result passes through as Raw.
    assert!(events.iter().any(|e| matches!(
        e,
        SessionEvent::Raw(v) if v.get("type").and_then(|t| t.as_str()) == Some("result")
    )));
    assert!(ended_completed(&events));
    session.dispose().await.unwrap();
}

// ---------------------------------------------------------------------------
// GitHub Copilot CLI and Grok Build (their own ids, over the one ACP door)
// ---------------------------------------------------------------------------

/// An ACP agent that chooses its model: it opens on `small`, which takes two
/// levels; `large` takes others. It keeps what it was started with and what
/// it was told, in order, in `<stub>.seen`.
fn acp_stub_that_chooses_its_model(dir: &Path, name: &str) -> PathBuf {
    write_stub(
        dir,
        name,
        r#"
seen="$0.seen"
echo "args: $*" > "$seen"
echo "allow_all: ${COPILOT_ALLOW_ALL:-unset}" >> "$seen"
while read -r line; do
  case "$line" in
    *'"method":"initialize"'*)
      echo '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{"loadSession":true}}}'
      ;;
    *'"method":"session/new"'*)
      echo '{"jsonrpc":"2.0","id":2,"result":{"sessionId":"s-1","configOptions":[{"id":"model","category":"model","type":"select","currentValue":"small","options":[{"value":"small"},{"value":"large"}]},{"id":"effort","category":"thought_level","type":"select","currentValue":"low","options":[{"value":"low"},{"value":"medium"}]}]}}'
      ;;
    *'"method":"session/set_config_option"'*'"configId":"model"'*)
      echo "set model: $line" >> "$seen"
      echo '{"jsonrpc":"2.0","id":5,"result":{"configOptions":[{"id":"model","category":"model","type":"select","currentValue":"large","options":[{"value":"small"},{"value":"large"}]},{"id":"effort","category":"thought_level","type":"select","currentValue":"low","options":[{"value":"low"},{"value":"high"},{"value":"max"}]}]}}'
      ;;
    *'"method":"session/set_config_option"'*'"configId":"effort"'*)
      echo "set effort: $line" >> "$seen"
      echo '{"jsonrpc":"2.0","id":4,"result":{"configOptions":[]}}'
      ;;
    *'"method":"session/prompt"'*)
      echo "prompt" >> "$seen"
      echo '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s-1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"hello from the stub"}}}}'
      echo '{"jsonrpc":"2.0","id":3,"result":{"stopReason":"end_turn"}}'
      ;;
  esac
done
"#,
    )
}

/// What the stub kept, a line each.
fn seen_by(stub: &Path) -> Vec<String> {
    let path = PathBuf::from(format!("{}.seen", stub.display()));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .lines()
        .map(str::to_string)
        .collect()
}

/// A turn on `large` at `high`, through one of the two adapters: the model
/// is set, then the effort — fitted to the levels the model's answer offers
/// — then the prompt; the token says what the session ran with.
async fn a_turn_on_a_model_at_a_level(
    adapter: &dyn HarnessAdapter,
    dir: &Path,
    stub: &Path,
) -> Vec<String> {
    let mut asked = spec(dir, "hello");
    asked.model = Some("large".into());
    asked.effort = Some(bisa_harness::Effort::High);
    // An agent's own environment that would allow every tool unasked.
    asked.env.insert("COPILOT_ALLOW_ALL".into(), "true".into());
    let session = adapter.launch(asked).await.unwrap();
    let mut stream = session.subscribe();
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;
    assert!(has_text_delta(&events, "hello from the stub"), "{events:?}");
    assert!(ended_completed(&events), "{events:?}");
    let token = session.resume_token().unwrap();
    assert_eq!(token.adapter_id, adapter.id());
    assert_eq!(token.native_id, "s-1");
    assert_eq!(token.model.as_deref(), Some("large"));
    assert_eq!(token.effort, Some(bisa_harness::Effort::High));
    session.dispose().await.unwrap();

    let seen = seen_by(stub);
    let order: Vec<&str> = seen[2..]
        .iter()
        .map(|l| l.split(':').next().unwrap_or(l))
        .collect();
    assert_eq!(
        order,
        ["set model", "set effort", "prompt"],
        "the model, then the effort, then the prompt: {seen:?}"
    );
    assert!(seen[2].contains(r#""value":"large""#), "{seen:?}");
    assert!(
        seen[3].contains(r#""value":"high""#),
        "the level is held to the levels `large` offers — fitted to the \
         session's first list it would have been `medium`: {seen:?}"
    );
    seen
}

#[tokio::test]
async fn copilot_is_started_as_a_protocol_server_and_set_the_protocols_way() {
    let dir = tempfile::tempdir().unwrap();
    let stub = acp_stub_that_chooses_its_model(dir.path(), "copilot");
    let adapter = bisa_adapters::copilot::CopilotAdapter {
        program: stub.display().to_string(),
    };
    let seen = a_turn_on_a_model_at_a_level(&adapter, dir.path(), &stub).await;
    assert_eq!(
        seen[0], "args: --acp --stdio --no-ask-user",
        "no model, no effort and no allow-all word on the command line"
    );
    assert_eq!(
        seen[1], "allow_all: unset",
        "the variable that allows every tool unasked never reaches the CLI, \
         even from the session's own environment"
    );
}

#[tokio::test]
async fn grok_is_started_as_an_agent_of_its_own_process_and_set_the_protocols_way() {
    let dir = tempfile::tempdir().unwrap();
    let stub = acp_stub_that_chooses_its_model(dir.path(), "grok");
    let adapter = bisa_adapters::grok::GrokAdapter {
        program: stub.display().to_string(),
    };
    let seen = a_turn_on_a_model_at_a_level(&adapter, dir.path(), &stub).await;
    assert_eq!(seen[0], "args: agent --no-leader stdio");
}

/// A model the session does not offer is the model's failure, said before
/// any prompt — the launch walks the agent's plan to its next model instead
/// of running on one nobody asked for.
#[tokio::test]
async fn a_model_the_session_does_not_offer_ends_the_launch_as_model_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let stub = acp_stub_that_chooses_its_model(dir.path(), "copilot");
    let adapter = bisa_adapters::copilot::CopilotAdapter {
        program: stub.display().to_string(),
    };
    // Launched as the engine launches: with no prompt, then prompted.
    let mut asked = spec(dir.path(), "");
    asked.model = Some("huge".into());
    let session = adapter.launch(asked).await.unwrap();
    let mut stream = session.subscribe();
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;
    assert!(
        events.iter().any(|e| matches!(
            e,
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: Outcome::ModelUnavailable { model, .. },
                is_terminal: true,
            }) if model == "huge"
        )),
        "{events:?}"
    );
    assert!(!has_text_delta(&events, "hello from the stub"));

    // The engine prompts a session it has just launched, and this one is
    // already over. The prompt is taken — never refused as *session
    // terminated*, which would hide why — and whoever listens from now on
    // still hears how the session ended, once.
    session
        .prompt("go on".into())
        .await
        .expect("a session that could not begin takes its first prompt");
    let mut late = session.subscribe();
    let after = collect_until_end(&mut late, STUB_DEADLINE).await;
    assert_eq!(after.len(), 1, "the end, and nothing else: {after:?}");
    assert!(
        matches!(
            &after[0],
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: Outcome::ModelUnavailable { model, .. },
                is_terminal: true,
            }) if model == "huge"
        ),
        "{after:?}"
    );

    let seen = seen_by(&stub);
    assert_eq!(
        seen.len(),
        2,
        "nothing was set and nothing prompted: {seen:?}"
    );
}

/// A `copilot` on `PATH` is not always the CLI: an editor installs a
/// launcher under the same name, which answers any word with a question and
/// leaves. It is not installed — and had it been launched, a process gone
/// before its session exists has failed to start, never finished a run.
#[tokio::test]
async fn a_launcher_that_only_asks_to_install_the_cli_is_not_the_cli() {
    let dir = tempfile::tempdir().unwrap();
    let launcher = write_stub(
        dir.path(),
        "copilot",
        // It waits for a line before it answers — an answer typed, or the
        // end of its input — so what the adapter writes first is taken.
        r#"read -r _answer
echo "Install GitHub Copilot CLI? ['y/N']"
exit 0
"#,
    );
    let adapter = bisa_adapters::copilot::CopilotAdapter {
        program: launcher.display().to_string(),
    };
    let probe = adapter.probe().await;
    assert!(!probe.available, "{probe:?}");
    assert!(
        probe
            .reason
            .as_deref()
            .is_some_and(|r| r.ends_with("with a version")),
        "{probe:?}"
    );

    let session = adapter.launch(spec(dir.path(), "hello")).await.unwrap();
    let mut stream = session.subscribe();
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;
    assert!(
        events.iter().any(|e| matches!(
            e,
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: Outcome::Failed { error },
                is_terminal: true,
            }) if error.starts_with("the agent ended before its session began")
        )),
        "{events:?}"
    );
    assert!(!ended_completed(&events), "{events:?}");

    // The CLI itself answers with a version, and is there.
    let cli = write_stub(dir.path(), "grok", "echo 'grok 0.1.42 (a1b2c3d)'\n");
    let probe = bisa_adapters::grok::GrokAdapter {
        program: cli.display().to_string(),
    }
    .probe()
    .await;
    assert!(probe.available, "{probe:?}");
    assert_eq!(probe.version.as_deref(), Some("grok 0.1.42 (a1b2c3d)"));
}

/// `grok models` as the CLI prints it: what is listed is the rows, the
/// default first.
#[tokio::test]
async fn grok_lists_the_models_its_cli_prints() {
    let dir = tempfile::tempdir().unwrap();
    let cli = write_stub(
        dir.path(),
        "grok",
        r#"if [ "$1" = "models" ]; then
  echo "You are logged in with grok.com."
  echo
  echo "Default model: grok-b"
  echo
  echo "Available models:"
  echo "  - grok-a"
  echo "  * grok-b (default)"
fi
"#,
    );
    let adapter = bisa_adapters::grok::GrokAdapter {
        program: cli.display().to_string(),
    };
    let models = adapter.models().await;
    assert_eq!(
        models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        ["grok-b", "grok-a"]
    );
    assert_eq!(models[0].efforts, adapter.efforts(Some("grok-b")));
}

// ---------------------------------------------------------------------------
// registration
// ---------------------------------------------------------------------------

#[tokio::test]
async fn register_all_populates_catalog() {
    let mut catalog = bisa_harness::HarnessCatalog::new();
    bisa_adapters::register_all(&mut catalog, bisa_http::Clients::shared());
    for id in [
        "claude-code",
        "codex",
        "pi",
        "omp",
        "opencode",
        "copilot",
        "grok",
        "acp:goose",
        "acp:cursor-agent",
        "acp:omp",
        "acp:opencode",
    ] {
        assert!(catalog.get(id).is_some(), "missing adapter {id}");
    }
    assert_eq!(
        catalog.get("acp:opencode").unwrap().display_name(),
        "OpenCode (ACP)"
    );
    assert_eq!(
        catalog.get("copilot").unwrap().display_name(),
        "GitHub Copilot CLI"
    );
    assert_eq!(catalog.get("grok").unwrap().display_name(), "Grok Build");
    // One row each: an id of its own is not also a generic target.
    for generic in ["acp:copilot", "acp:grok"] {
        assert!(catalog.get(generic).is_none(), "{generic}");
    }
    // Every id the catalog reserves for a compiled-in adapter has one —
    // `acp` and `custom` are namespaces, reserved for their prefixed rows.
    for id in bisa_harness::catalog::BUILTIN_IDS {
        if !matches!(*id, "acp" | "custom") {
            assert!(
                catalog.get(id).is_some(),
                "{id} is reserved and not registered"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Real-binary smoke tests (ignored by default; run with --ignored locally)
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires a real `claude` binary and consumes tokens"]
async fn claude_code_real_smoke() {
    let adapter = bisa_adapters::claude_code::ClaudeCodeAdapter::default();
    if !adapter.probe().await.available {
        eprintln!("claude not installed; skipping");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let session = adapter
        .launch(spec(dir.path(), "Reply with exactly the word: pong"))
        .await
        .unwrap();
    let mut stream = session.subscribe();
    let events = collect_until_end(&mut stream, Duration::from_secs(120)).await;
    assert!(has_text_delta(&events, "pong"), "{events:?}");
    session.dispose().await.unwrap();
}

/// Suppress dead-code warnings for helpers only used in some cfg contexts.
#[allow(dead_code)]
fn _unused(_: &Shared) {}

// ---------------------------------------------------------------------------
// The model taxonomy, end to end through real adapters
// ---------------------------------------------------------------------------

fn spec_with_model(dir: &Path, prompt: &str, model: &str) -> SessionSpec {
    SessionSpec {
        model: Some(model.to_string()),
        ..spec(dir, prompt)
    }
}

/// Launch: a harness that rejects `--model` dies during validation. That is
/// not synchronously observable (see `util`'s "Launch versus mid-run"), so it
/// arrives as a terminal `ModelUnavailable` with no progress before it —
/// which is exactly what a failed launch looks like to the engine.
///
/// Stub wording captured from omp 18.0.3 (`Unknown model: …`).
#[tokio::test]
async fn unknown_model_at_launch_is_model_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let stub = write_stub(
        dir.path(),
        "omp",
        "echo 'Unknown model: acme/turbo-9. Use ACP `session/setModel` for picker-driven selection or list available models with /model.' >&2\nexit 2\n",
    );
    let adapter = bisa_adapters::omp::OmpAdapter {
        program: stub.display().to_string(),
        extra_args: vec![],
    };
    let session = adapter
        .launch(spec_with_model(dir.path(), "go", "acme/turbo-9"))
        .await
        .expect("launch itself succeeds; the harness dies after");
    let mut stream = session.subscribe();
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;
    let found = events.iter().find_map(|e| match e {
        SessionEvent::Lifecycle(LifecycleEvent::Ended {
            outcome: Outcome::ModelUnavailable { model, reason, .. },
            ..
        }) => Some((model.clone(), reason.clone())),
        _ => None,
    });
    let (model, reason) = found.unwrap_or_else(|| panic!("no ModelUnavailable in {events:?}"));
    assert_eq!(model, "acme/turbo-9");
    assert!(reason.starts_with("unknown model id"), "{reason}");
    // Nothing ran: no progress before the wall. That is the launch signature.
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, SessionEvent::Progress(ProgressEvent::TextDelta { .. }))),
        "a launch failure must carry no progress: {events:?}"
    );
}

/// With no model pinned the session still reports honestly — there is simply
/// no model to name beyond "the harness default".
#[tokio::test]
async fn an_unpinned_session_names_the_harness_default() {
    let ctx = bisa_adapters::util::ModelCtx::new("omp", None);
    assert_eq!(ctx.model_name(), "omp default");
}

/// A genuine crash must stay a failure even with a model pinned. This is the
/// regression that matters most: a false positive here silently retries good
/// work on another model and buries the real error.
#[tokio::test]
async fn a_real_crash_stays_failed_even_with_a_model_pinned() {
    let dir = tempfile::tempdir().unwrap();
    let stub = write_stub(
        dir.path(),
        "omp",
        "echo 'thread panicked at src/main.rs:12: index out of bounds' >&2\nexit 101\n",
    );
    let adapter = bisa_adapters::omp::OmpAdapter {
        program: stub.display().to_string(),
        extra_args: vec![],
    };
    let session = adapter
        .launch(spec_with_model(dir.path(), "go", "acme/turbo-9"))
        .await
        .expect("a crash is not a launch error");
    let mut stream = session.subscribe();
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;
    assert!(
        events.iter().any(|e| matches!(
            e,
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: Outcome::Failed { .. },
                ..
            })
        )),
        "expected Failed, got {events:?}"
    );
}

/// Mid-run site, typed: Claude Code puts the HTTP status of the failed API
/// call on its `result` frame. No prose is read on this path at all.
///
/// Frame shape captured from the schemas shipped in the claude 2.1.246 binary.
#[tokio::test]
async fn claude_result_api_error_status_is_a_typed_model_failure() {
    let dir = tempfile::tempdir().unwrap();
    let stub = write_stub(
        dir.path(),
        "claude",
        r#"
read -r _line
echo '{"type":"system","subtype":"init","session_id":"s-1"}'
echo '{"type":"result","subtype":"success","is_error":true,"api_error_status":429,"result":"","session_id":"s-1"}'
while read -r _l; do :; done
"#,
    );
    let adapter = bisa_adapters::claude_code::ClaudeCodeAdapter {
        program: stub.display().to_string(),
        ..Default::default()
    };
    let session = adapter
        .launch(spec_with_model(dir.path(), "go", "claude-fable-5"))
        .await
        .unwrap();
    let mut stream = session.subscribe();
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;
    let found = events.iter().find_map(|e| match e {
        SessionEvent::Lifecycle(LifecycleEvent::Ended {
            outcome: Outcome::ModelUnavailable { model, reason, .. },
            ..
        }) => Some((model.clone(), reason.clone())),
        _ => None,
    });
    let (model, reason) = found.unwrap_or_else(|| panic!("no ModelUnavailable in {events:?}"));
    assert_eq!(model, "claude-fable-5");
    assert!(reason.contains("429"), "{reason}");
}

/// Mid-run site, prose: the M5 incident itself. The quota message arrives in
/// the `result` frame's text, and must come out as a retry rather than a
/// failure.
///
/// Wording captured verbatim from the claude 2.1.246 binary.
#[tokio::test]
async fn claude_quota_prose_mid_run_is_model_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let stub = write_stub(
        dir.path(),
        "claude",
        r#"
read -r _line
echo '{"type":"system","subtype":"init","session_id":"s-1"}'
echo '{"type":"result","subtype":"error_during_execution","is_error":true,"errors":["You have reached your Fable 5 limit"],"session_id":"s-1"}'
while read -r _l; do :; done
"#,
    );
    let adapter = bisa_adapters::claude_code::ClaudeCodeAdapter {
        program: stub.display().to_string(),
        ..Default::default()
    };
    let session = adapter
        .launch(spec_with_model(dir.path(), "go", "claude-fable-5"))
        .await
        .unwrap();
    let mut stream = session.subscribe();
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;
    assert!(
        events.iter().any(|e| matches!(
            e,
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: Outcome::ModelUnavailable { .. },
                ..
            })
        )),
        "the M5 incident must not be a Failed: {events:?}"
    );
}

/// A `result` frame that failed for an ordinary reason keeps failing, and now
/// carries the message instead of only its subtype.
#[tokio::test]
async fn claude_ordinary_result_error_still_fails_with_its_message() {
    let dir = tempfile::tempdir().unwrap();
    let stub = write_stub(
        dir.path(),
        "claude",
        r#"
read -r _line
echo '{"type":"system","subtype":"init","session_id":"s-1"}'
echo '{"type":"result","subtype":"error_max_turns","is_error":true,"errors":["ran out of turns"],"session_id":"s-1"}'
while read -r _l; do :; done
"#,
    );
    let adapter = bisa_adapters::claude_code::ClaudeCodeAdapter {
        program: stub.display().to_string(),
        ..Default::default()
    };
    let session = adapter.launch(spec(dir.path(), "go")).await.unwrap();
    let mut stream = session.subscribe();
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;
    let err = events.iter().find_map(|e| match e {
        SessionEvent::Lifecycle(LifecycleEvent::Ended {
            outcome: Outcome::Failed { error },
            ..
        }) => Some(error.clone()),
        _ => None,
    });
    let err = err.unwrap_or_else(|| panic!("expected Failed in {events:?}"));
    assert!(err.contains("error_max_turns"), "{err}");
    assert!(err.contains("ran out of turns"), "{err}");
}

/// Mid-run site for the one-shot family (codex, opencode): the process dies
/// non-zero and the stderr tail carries the wall.
///
/// Wording captured from opencode 1.18.23.
#[tokio::test]
async fn opencode_stderr_tail_mid_run_is_model_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let stub = write_stub(
        dir.path(),
        "opencode",
        r#"echo '{"type":"session.created","sessionID":"oc-1"}'
echo 'Pro usage limit reached. It will reset in 3 hours. To continue using this model now, enable usage from your available balance' >&2
exit 1
"#,
    );
    let adapter = bisa_adapters::opencode::OpencodeAdapter {
        program: stub.display().to_string(),
        ..Default::default()
    };
    let session = adapter
        .launch(spec_with_model(dir.path(), "go", "anthropic/claude-opus-5"))
        .await
        .unwrap();
    let mut stream = session.subscribe();
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;
    let found = events.iter().find_map(|e| match e {
        SessionEvent::Lifecycle(LifecycleEvent::Ended {
            outcome:
                Outcome::ModelUnavailable {
                    model, retry_after, ..
                },
            ..
        }) => Some((model.clone(), *retry_after)),
        _ => None,
    });
    let (model, retry_after) = found.unwrap_or_else(|| panic!("no ModelUnavailable in {events:?}"));
    assert_eq!(model, "anthropic/claude-opus-5");
    assert_eq!(retry_after, Some(3 * 3600), "the harness said 3 hours");
}

/// The custom-JSON contract's own error channel, mid-run.
#[tokio::test]
async fn custom_json_ended_failed_can_be_a_model_wall() {
    let dir = tempfile::tempdir().unwrap();
    let stub = write_stub(
        dir.path(),
        "mine",
        r#"
read -r _line
echo '{"type":"ended","outcome":"failed","error":"insufficient_quota"}'
while read -r _l; do :; done
"#,
    );
    let adapter =
        bisa_adapters::custom_json::CustomJsonAdapter::new(bisa_harness::CustomHarnessSpec {
            id: "mine".into(),
            label: "Mine".into(),
            command: stub.display().to_string(),
            args: vec![],
            resume_args: vec![],
            env: Default::default(),
            install_hint: None,
        });
    let session = adapter.launch(spec(dir.path(), "go")).await.unwrap();
    let mut stream = session.subscribe();
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;
    assert!(
        events.iter().any(|e| matches!(
            e,
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: Outcome::ModelUnavailable { .. },
                ..
            })
        )),
        "expected ModelUnavailable in {events:?}"
    );
}

// ---------------------------------------------------------------------------
// The interactive form
// ---------------------------------------------------------------------------

/// **What an adapter offers a person must be the binary it probes.**
///
/// The two invocations are different — `launch` starts a protocol session
/// (`claude -p --output-format stream-json`, `codex exec --json`,
/// `pi --mode rpc`) and `interactive` is the bare command — but they are the
/// same *program*, and the availability check the UI gates its menu on is the
/// probe. An adapter naming a different program here would offer something the
/// probe was never about: the menu says installed, the terminal says command
/// not found.
#[test]
fn every_interactive_adapter_offers_the_program_it_probes() {
    use bisa_adapters::{
        claude_code::ClaudeCodeAdapter, codex::CodexAdapter, copilot::CopilotAdapter,
        grok::GrokAdapter, omp::OmpAdapter, opencode::OpencodeAdapter, pi_rpc::PiRpcAdapter,
    };

    let cases: Vec<(&str, Box<dyn HarnessAdapter>, &str)> = vec![
        (
            "claude-code",
            Box::new(ClaudeCodeAdapter {
                program: "claude-somewhere-else".into(),
                ..Default::default()
            }),
            "claude-somewhere-else",
        ),
        (
            "codex",
            Box::new(CodexAdapter {
                program: "codex-somewhere-else".into(),
            }),
            "codex-somewhere-else",
        ),
        (
            "pi",
            Box::new(PiRpcAdapter {
                program: "pi-somewhere-else".into(),
                extra_args: vec!["--stub".into()],
            }),
            "pi-somewhere-else",
        ),
        (
            "omp",
            Box::new(OmpAdapter {
                program: "omp-somewhere-else".into(),
                extra_args: vec!["--stub".into()],
            }),
            "omp-somewhere-else",
        ),
        (
            "opencode",
            Box::new(OpencodeAdapter {
                program: "opencode-somewhere-else".into(),
                ..Default::default()
            }),
            "opencode-somewhere-else",
        ),
        (
            "copilot",
            Box::new(CopilotAdapter {
                program: "copilot-somewhere-else".into(),
            }),
            "copilot-somewhere-else",
        ),
        (
            "grok",
            Box::new(GrokAdapter {
                program: "grok-somewhere-else".into(),
            }),
            "grok-somewhere-else",
        ),
    ];

    for (id, adapter, program) in cases {
        let launch = adapter
            .interactive()
            .unwrap_or_else(|| panic!("{id} has an interactive form"));
        assert_eq!(
            launch.program, program,
            "{id} offers a program it does not probe"
        );
    }
}

/// **A protocol adapter offers nothing, and that is the whole point.**
///
/// Every `acp:*` adapter exists to put a binary into ACP mode — `goose acp`,
/// `cursor-agent acp`, `omp acp` — and an A2A target is an HTTP endpoint with
/// no local process at all. Attaching a PTY to either shows a person JSON-RPC
/// frames rather than a session, so `interactive()` must stay `None` for them
/// even though both look like ordinary adapters from the outside.
#[test]
fn a_protocol_adapter_has_no_interactive_form() {
    use bisa_adapters::acp::AcpAdapter;

    let acp = AcpAdapter::new("acp:goose", "Goose", "goose", vec!["acp".into()]);
    assert!(
        acp.interactive().is_none(),
        "an ACP adapter's command is a protocol invocation, not a session"
    );
}

/// A harness that speaks ACP under an id of its own does have a terminal
/// form — the bare command a person types — and it carries none of the words
/// that put the binary in protocol mode: `copilot --acp` or `grok agent
/// stdio` in a PTY is a screen of JSON-RPC.
#[test]
fn a_harness_that_speaks_acp_under_its_own_id_is_opened_bare_in_a_terminal() {
    use bisa_adapters::{copilot::CopilotAdapter, grok::GrokAdapter};

    let copilot = CopilotAdapter::default();
    let grok = GrokAdapter::default();
    let cases: [(&dyn HarnessAdapter, Vec<String>); 2] = [
        (&copilot, copilot.command().args),
        (&grok, grok.command().args),
    ];
    for (adapter, protocol_words) in cases {
        let launch = adapter
            .interactive()
            .unwrap_or_else(|| panic!("{} has a terminal form", adapter.id()));
        assert_eq!(launch.program, adapter.id(), "the binary is its id");
        assert!(launch.args.is_empty(), "{}", adapter.id());
        assert_eq!(launch.resume_args, ["--continue"], "{}", adapter.id());
        for word in &protocol_words {
            assert!(
                !launch.args.contains(word) && !launch.resume_args.contains(word),
                "{}: `{word}` is a protocol word",
                adapter.id()
            );
        }
    }
}
