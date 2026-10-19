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
        SessionEvent::Progress(ProgressEvent::ToolEnded { name, ok: true, .. }) if name == "Bash"
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
    // OpenCode 1's binary: its version on `--version`, run mode otherwise,
    // the words it was started with kept beside it.
    let stub = write_stub(
        dir.path(),
        "opencode",
        r#"
if [ "$1" = "--version" ]; then echo "1.18.23"; exit 0; fi
echo "args: $*" > "$0.seen"
echo '{"type":"text","sessionID":"ses_1","part":{"type":"text","text":"oc hello"}}'
echo '{"type":"tool_use","sessionID":"ses_1","part":{"type":"tool","callID":"c1","tool":"bash","state":{"status":"completed","input":{"command":"ls"},"output":"ok"}}}'
exit 0
"#,
    );
    let adapter = bisa_adapters::opencode::OpencodeAdapter {
        program: stub.display().to_string(),
        ..Default::default()
    };
    // No probe first: the launch asks the version itself and keeps the wire.
    assert_eq!(adapter.wire_known(), None);
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
    assert_eq!(
        adapter.wire_known(),
        Some(bisa_adapters::opencode::Wire::RunMode)
    );
    assert_eq!(
        seen_by(&stub),
        [format!(
            "args: run --format json --dir {} hi",
            dir.path().display()
        )],
        "OpenCode 1's wire, word for word"
    );
}

/// OpenCode 2's binary, as a stub: `--version` says what 2.0.26 says, `acp`
/// is the ACP handshake with what `session/new` was told kept beside it,
/// and any other word is the wrong wire and exits 9.
fn opencode_two_stub(dir: &Path) -> PathBuf {
    write_stub(
        dir,
        "opencode",
        r#"
if [ "$1" = "--version" ]; then echo "opencode v2.0.26"; exit 0; fi
if [ "$1" = "models" ]; then echo "anthropic/claude-sonnet-5"; echo "openai/gpt-5"; exit 0; fi
seen="$0.seen"
echo "args: $*" > "$seen"
[ "$1" = "acp" ] || exit 9
while read -r line; do
  case "$line" in
    *'"method":"initialize"'*)
      echo '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{"loadSession":true}}}'
      ;;
    *'"method":"session/new"'*)
      echo "new: $line" >> "$seen"
      echo '{"jsonrpc":"2.0","id":2,"result":{"sessionId":"oc-2","configOptions":[]}}'
      ;;
    *'"method":"session/prompt"'*)
      echo '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"oc-2","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"hello from opencode two"}}}}'
      echo '{"jsonrpc":"2.0","id":3,"result":{"stopReason":"end_turn"}}'
      ;;
  esac
done
"#,
    )
}

/// OpenCode 2 — its version says so — is driven through its ACP server:
/// started with the one word `acp`, told the session's directory and its
/// MCP servers in `session/new`, its words streamed, its session the token.
#[tokio::test]
async fn opencode_two_is_driven_through_its_acp_server_with_the_sessions_directory_and_servers() {
    use bisa_adapters::opencode::{OpencodeAdapter, Wire};
    let dir = tempfile::tempdir().unwrap();
    let stub = opencode_two_stub(dir.path());
    let adapter = OpencodeAdapter {
        program: stub.display().to_string(),
        ..Default::default()
    };
    let probe = adapter.probe().await;
    assert!(probe.available, "{probe:?}");
    assert_eq!(probe.version.as_deref(), Some("opencode v2.0.26"));
    assert_eq!(adapter.wire_known(), Some(Wire::Acp));

    let mut asked = spec(dir.path(), "hello");
    asked.mcp_servers = vec![bisa_core::McpMount::platform(
        bisa_core::McpServerConfig::Stdio {
            name: "bisa".into(),
            command: "/usr/local/bin/bisa".into(),
            args: vec![
                "mcp".into(),
                "--socket".into(),
                "/tmp/ws/run/intake.sock".into(),
            ],
            env: Default::default(),
            cwd: None,
        },
    )];
    let session = adapter.launch(asked).await.unwrap();
    let mut stream = session.subscribe();
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;
    assert!(
        has_text_delta(&events, "hello from opencode two"),
        "{events:?}"
    );
    assert!(ended_completed(&events), "{events:?}");
    let token = session.resume_token().unwrap();
    assert_eq!(token.adapter_id, "opencode");
    assert_eq!(token.native_id, "oc-2");
    assert_eq!(token.cwd, dir.path());
    session.dispose().await.unwrap();

    let seen = seen_by(&stub);
    assert_eq!(
        seen[0], "args: acp",
        "the one word, nothing of the run-mode wire"
    );
    assert!(
        seen[1].contains(&format!(r#""cwd":"{}""#, dir.path().display())),
        "the session's own directory, absolute: {seen:?}"
    );
    assert!(
        seen[1].contains(r#""mcpServers":[{"#) && seen[1].contains(r#""name":"bisa""#),
        "the platform's server, the protocol's way: {seen:?}"
    );
}

/// The probe keeps the wire the version decides: OpenCode 1's by its
/// version, and run mode for a binary that answers with no version at all;
/// a binary nobody can find decides nothing.
#[tokio::test]
async fn opencodes_probe_decides_its_wire_by_the_version_it_answers() {
    use bisa_adapters::opencode::{OpencodeAdapter, Wire};
    let dir = tempfile::tempdir().unwrap();
    let one = write_stub(dir.path(), "opencode-one", "echo '1.18.23'\n");
    let adapter = OpencodeAdapter {
        program: one.display().to_string(),
        ..Default::default()
    };
    let probe = adapter.probe().await;
    assert!(probe.available, "{probe:?}");
    assert_eq!(probe.version.as_deref(), Some("1.18.23"));
    assert_eq!(adapter.wire_known(), Some(Wire::RunMode));

    let wordless = write_stub(dir.path(), "opencode-wordless", "echo 'OpenCode'\n");
    let adapter = OpencodeAdapter {
        program: wordless.display().to_string(),
        ..Default::default()
    };
    let probe = adapter.probe().await;
    assert!(probe.available, "{probe:?}");
    assert_eq!(adapter.wire_known(), Some(Wire::RunMode));

    let adapter = OpencodeAdapter {
        program: dir.path().join("opencode-nowhere").display().to_string(),
        ..Default::default()
    };
    let probe = adapter.probe().await;
    assert!(!probe.available, "{probe:?}");
    assert_eq!(adapter.wire_known(), None, "nothing to decide on");
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
// GitHub Copilot CLI, Grok Build and Gemini CLI (their own ids, over the one
// ACP door)
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
    // The process it was is replayed to a late listener too — what a
    // restart would have to end — and then the end, once.
    let ends: Vec<&SessionEvent> = after.iter().filter(|e| e.is_terminal_end()).collect();
    assert_eq!(ends.len(), 1, "the end, once: {after:?}");
    assert!(
        after.iter().all(|e| e.is_terminal_end()
            || matches!(
                e,
                SessionEvent::Lifecycle(LifecycleEvent::ProcessStarted { .. })
            )),
        "the process it was and the end, nothing else: {after:?}"
    );
    assert!(
        matches!(
            ends[0],
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

/// An ACP agent on the draft before config options, as Gemini CLI is: its
/// `session/new` answers `models` — the ids it lists and the one it is on —
/// and no config option at all; the model is set with `session/set_model`,
/// answered empty. It keeps what it was started with and what it was told,
/// in order, in `<stub>.seen`.
fn acp_stub_on_the_model_draft(dir: &Path, name: &str) -> PathBuf {
    write_stub(
        dir,
        name,
        r#"
seen="$0.seen"
echo "args: $*" > "$seen"
while read -r line; do
  case "$line" in
    *'"method":"initialize"'*)
      echo '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{"loadSession":true}}}'
      ;;
    *'"method":"session/new"'*)
      echo '{"jsonrpc":"2.0","id":2,"result":{"sessionId":"s-1","modes":{"availableModes":[{"id":"default","name":"Default"}],"currentModeId":"default"},"models":{"availableModels":[{"modelId":"small","name":"Small"},{"modelId":"large","name":"Large"}],"currentModelId":"small"}}}'
      ;;
    *'"method":"session/set_model"'*)
      echo "set model: $line" >> "$seen"
      echo '{"jsonrpc":"2.0","id":5,"result":{}}'
      ;;
    *'"method":"session/set_config_option"'*)
      echo "set option: $line" >> "$seen"
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

#[tokio::test]
async fn gemini_is_started_in_protocol_mode_and_set_the_drafts_way() {
    let dir = tempfile::tempdir().unwrap();
    let stub = acp_stub_on_the_model_draft(dir.path(), "gemini");
    let adapter = bisa_adapters::gemini::GeminiAdapter {
        program: stub.display().to_string(),
    };
    let mut asked = spec(dir.path(), "hello");
    asked.model = Some("large".into());
    // No effort: the harness has no such control, and the engine asks none.
    asked.effort = None;
    let session = adapter.launch(asked).await.unwrap();
    let mut stream = session.subscribe();
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;
    assert!(has_text_delta(&events, "hello from the stub"), "{events:?}");
    assert!(ended_completed(&events), "{events:?}");
    let token = session.resume_token().unwrap();
    assert_eq!(token.adapter_id, "gemini");
    assert_eq!(token.native_id, "s-1");
    assert_eq!(token.model.as_deref(), Some("large"));
    assert_eq!(token.effort, None);
    session.dispose().await.unwrap();

    let seen = seen_by(&stub);
    assert_eq!(
        seen[0], "args: --acp",
        "the one protocol word: no model, no effort and no approval word on the command line"
    );
    let order: Vec<&str> = seen[1..]
        .iter()
        .map(|l| l.split(':').next().unwrap_or(l))
        .collect();
    assert_eq!(
        order,
        ["set model", "prompt"],
        "the model the draft's way, then the prompt — and no option set: {seen:?}"
    );
    assert!(
        seen[1].contains(r#""method":"session/set_model""#)
            && seen[1].contains(r#""modelId":"large""#),
        "{seen:?}"
    );
}

/// `gemini` is a common word for a binary: the CLI is there only when it
/// answers with a version, as the CLI prints one — the bare number.
#[tokio::test]
async fn gemini_is_there_only_when_it_answers_with_a_version() {
    let dir = tempfile::tempdir().unwrap();
    let cli = write_stub(dir.path(), "gemini", "echo '0.31.0'\n");
    let probe = bisa_adapters::gemini::GeminiAdapter {
        program: cli.display().to_string(),
    }
    .probe()
    .await;
    assert!(probe.available, "{probe:?}");
    assert_eq!(probe.version.as_deref(), Some("0.31.0"));

    let mute = write_stub(dir.path(), "gemini-mute", "echo 'Gemini CLI'\n");
    let probe = bisa_adapters::gemini::GeminiAdapter {
        program: mute.display().to_string(),
    }
    .probe()
    .await;
    assert!(!probe.available, "{probe:?}");
    assert!(
        probe
            .reason
            .as_deref()
            .is_some_and(|r| r.ends_with("with a version")),
        "{probe:?}"
    );
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
        "gemini",
        "muse-code",
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
    assert_eq!(catalog.get("gemini").unwrap().display_name(), "Gemini CLI");
    assert_eq!(
        catalog.get("muse-code").unwrap().display_name(),
        "Muse Code"
    );
    // One row each: an id of its own is not also a generic target.
    for generic in ["acp:copilot", "acp:grok", "acp:gemini", "acp:muse-code"] {
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
        gemini::GeminiAdapter, grok::GrokAdapter, muse_code::MuseCodeAdapter, omp::OmpAdapter,
        opencode::OpencodeAdapter, pi_rpc::PiRpcAdapter,
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
        (
            "gemini",
            Box::new(GeminiAdapter {
                program: "gemini-somewhere-else".into(),
            }),
            "gemini-somewhere-else",
        ),
        (
            "muse-code",
            Box::new(MuseCodeAdapter::with_program("muse-somewhere-else")),
            "muse-somewhere-else",
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
/// form — the bare command a person types, resumed with its own word — and
/// it carries none of the words that put the binary in protocol mode:
/// `copilot --acp`, `grok agent stdio` or `gemini --acp` in a PTY is a
/// screen of JSON-RPC.
#[test]
fn a_harness_that_speaks_acp_under_its_own_id_is_opened_bare_in_a_terminal() {
    use bisa_adapters::{copilot::CopilotAdapter, gemini::GeminiAdapter, grok::GrokAdapter};

    let copilot = CopilotAdapter::default();
    let grok = GrokAdapter::default();
    let gemini = GeminiAdapter::default();
    let cases: [(&dyn HarnessAdapter, Vec<String>, &[&str]); 3] = [
        (&copilot, copilot.command().args, &["--continue"]),
        (&grok, grok.command().args, &["--continue"]),
        (&gemini, gemini.command().args, &["--resume", "latest"]),
    ];
    for (adapter, protocol_words, resume_words) in cases {
        let launch = adapter
            .interactive()
            .unwrap_or_else(|| panic!("{} has a terminal form", adapter.id()));
        assert_eq!(launch.program, adapter.id(), "the binary is its id");
        assert!(launch.args.is_empty(), "{}", adapter.id());
        assert_eq!(launch.resume_args, resume_words, "{}", adapter.id());
        for word in &protocol_words {
            assert!(
                !launch.args.contains(word) && !launch.resume_args.contains(word),
                "{}: `{word}` is a protocol word",
                adapter.id()
            );
        }
    }
}

/// **Muse Code is opened bare too — and its binary is not its id.** The
/// harness is `muse-code`, the CLI `muse`: the terminal form is the bare
/// `muse`, resumed with `resume --last`, and none of the words that make
/// the CLI a session host (`serve`, the trust it is given) reaches a PTY.
#[test]
fn a_harness_with_a_protocol_of_its_own_is_opened_bare_in_a_terminal() {
    use bisa_adapters::muse_code::MuseCodeAdapter;

    let adapter = MuseCodeAdapter::default();
    let launch = adapter
        .interactive()
        .expect("Muse Code has a terminal form");
    assert_eq!(launch.program, "muse");
    assert!(launch.args.is_empty());
    assert_eq!(launch.resume_args, ["resume", "--last"]);
    for word in adapter.command().args {
        assert!(
            !launch.args.contains(&word) && !launch.resume_args.contains(&word),
            "`{word}` is a protocol word"
        );
    }
}

// ---------------------------------------------------------------------------
// A stop ends the harness's process and what it started.

#[cfg(unix)]
fn process_exists(pid: u32) -> bool {
    rustix::process::Pid::from_raw(i32::try_from(pid).expect("a pid fits"))
        .is_some_and(|pid| rustix::process::test_kill_process(pid).is_ok())
}

/// Wait for the process to be gone, reaped included, or give up.
#[cfg(unix)]
async fn process_gone_within(pid: u32, within: Duration) -> bool {
    let until = tokio::time::Instant::now() + within;
    while process_exists(pid) {
        if tokio::time::Instant::now() >= until {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    true
}

/// The pids a stub wrote to `file`, once it has.
#[cfg(unix)]
async fn pids_written(file: &Path) -> Vec<u32> {
    // Generous for the same reason `STUB_DEADLINE` is: a loaded machine
    // starts a shell slowly, and a tight bound would read as a failure.
    for _ in 0..200 {
        if let Ok(text) = std::fs::read_to_string(file) {
            let pids: Vec<u32> = text
                .split_whitespace()
                .filter_map(|w| w.parse().ok())
                .collect();
            if !pids.is_empty() {
                return pids;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the stub never wrote its pids to {}", file.display());
}

#[cfg(unix)]
#[tokio::test]
async fn claude_abort_ends_the_cli_and_its_tool_grandchild() {
    let dir = tempfile::tempdir().unwrap();
    let pids = dir.path().join("pids");
    let stub = write_stub(
        dir.path(),
        "claude",
        &format!(
            r#"
read -r _line
echo '{{"type":"system","subtype":"init","session_id":"sess-42","model":"m"}}'
# A tool's command, running on: what a stop must reach beside the CLI.
sleep 300 &
echo "$! $$" > '{}'
while read -r _l; do :; done
"#,
            pids.display()
        ),
    );
    let adapter = bisa_adapters::claude_code::ClaudeCodeAdapter {
        program: stub.display().to_string(),
        ..Default::default()
    };
    let session = adapter
        .launch(spec(dir.path(), "do the thing"))
        .await
        .unwrap();
    let written = pids_written(&pids).await;
    let (grandchild, child) = (written[0], written[1]);
    assert!(process_exists(grandchild) && process_exists(child));
    let mut stream = session.subscribe();
    let started = std::time::Instant::now();
    session.abort().await.unwrap();
    assert!(
        started.elapsed() < bisa_harness::proc::ABORT_GRACE + Duration::from_secs(2),
        "an abort answers within its grace"
    );
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;
    assert!(
        events.iter().any(|e| matches!(
            e,
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: Outcome::Aborted,
                is_terminal: true
            })
        )),
        "the session ended aborted: {events:?}"
    );
    assert!(
        process_gone_within(child, Duration::from_secs(2)).await,
        "the CLI is gone"
    );
    assert!(
        process_gone_within(grandchild, Duration::from_secs(2)).await,
        "the tool's command went with the CLI's group"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn acp_cancel_on_a_stop_ends_the_session_terminally_and_its_process() {
    let dir = tempfile::tempdir().unwrap();
    let pids = dir.path().join("pids");
    let stub = write_stub(
        dir.path(),
        "acp-agent",
        &format!(
            r#"
echo "$$" > '{}'
while read -r line; do
  case "$line" in
    *'"method":"initialize"'*)
      echo '{{"jsonrpc":"2.0","id":1,"result":{{"protocolVersion":1,"agentCapabilities":{{"loadSession":true}}}}}}'
      ;;
    *'"method":"session/new"'*)
      echo '{{"jsonrpc":"2.0","id":2,"result":{{"sessionId":"acp-9"}}}}'
      ;;
    *'"method":"session/prompt"'*)
      # A turn that holds: nothing answers the prompt until a cancel.
      echo '{{"jsonrpc":"2.0","method":"session/update","params":{{"sessionId":"acp-9","update":{{"sessionUpdate":"agent_message_chunk","content":{{"type":"text","text":"acp hello"}}}}}}}}'
      ;;
    *'"method":"session/cancel"'*)
      echo '{{"jsonrpc":"2.0","id":3,"result":{{"stopReason":"cancelled"}}}}'
      ;;
  esac
done
"#,
            pids.display()
        ),
    );
    let adapter = bisa_adapters::acp::AcpAdapter::new(
        "acp:stub",
        "Stub ACP",
        stub.display().to_string(),
        vec![],
    );
    let session = adapter.launch(spec(dir.path(), "hello acp")).await.unwrap();
    let mut stream = session.subscribe();
    // The turn is under way: its words have streamed.
    let mut before = Vec::new();
    loop {
        let ev = tokio::time::timeout(STUB_DEADLINE, stream.next())
            .await
            .expect("words before the deadline")
            .expect("stream open");
        let said = has_text_delta(std::slice::from_ref(&ev), "acp hello");
        before.push(ev);
        if said {
            break;
        }
    }
    let child = pids_written(&pids).await[0];
    assert!(process_exists(child));
    session.abort().await.unwrap();
    let mut events = before;
    events.extend(collect_until_end(&mut stream, STUB_DEADLINE).await);
    assert!(
        events.iter().any(|e| matches!(
            e,
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: Outcome::Aborted,
                is_terminal: true
            })
        )),
        "a cancelled turn on a stop ends the session for good, never a turn end alone: {events:?}"
    );
    assert!(
        process_gone_within(child, Duration::from_secs(2)).await,
        "the agent's process is gone — a cancel alone left it running"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn dispose_ends_a_cli_that_ignores_eof_past_the_grace() {
    let dir = tempfile::tempdir().unwrap();
    let pids = dir.path().join("pids");
    let stub = write_stub(
        dir.path(),
        "claude",
        &format!(
            r#"
read -r _line
echo '{{"type":"system","subtype":"init","session_id":"sess-42","model":"m"}}'
echo "$$" > '{}'
# Never reads stdin again: EOF means nothing to it.
exec sleep 300
"#,
            pids.display()
        ),
    );
    let adapter = bisa_adapters::claude_code::ClaudeCodeAdapter {
        program: stub.display().to_string(),
        ..Default::default()
    };
    let session = adapter
        .launch(spec(dir.path(), "do the thing"))
        .await
        .unwrap();
    let child = pids_written(&pids).await[0];
    let started = std::time::Instant::now();
    session.dispose().await.unwrap();
    assert!(
        started.elapsed() >= bisa_harness::proc::DISPOSE_GRACE,
        "EOF was given its grace first"
    );
    assert!(
        process_gone_within(child, Duration::from_secs(2)).await,
        "a CLI that ignores EOF is killed past the grace"
    );
}

// ---------------------------------------------------------------------------
// Every session facade answers for itself, every launch carries its
// environment, and every adapter that resumes does so from its own token.

fn token_for(
    adapter_id: &str,
    native_id: &str,
    dir: &Path,
    transcript: Option<PathBuf>,
) -> bisa_harness::ResumeToken {
    bisa_harness::ResumeToken {
        adapter_id: adapter_id.into(),
        native_id: native_id.into(),
        cwd: dir.to_path_buf(),
        transcript_path: transcript,
        model: None,
        effort: None,
    }
}

fn reporting_ctx() -> bisa_harness::ReportingContext {
    bisa_harness::ReportingContext {
        session: "01S".into(),
        reporter: vec!["/bin/bisa".into(), "session".into(), "report".into()],
        files_dir: "/ws/run/interactive/01S".into(),
        port: Some(41234),
        guard: None,
        guard_timeout_secs: 0,
    }
}

/// What a stub was started with, once it has said so — a stub that was
/// just spawned writes its file a moment after the launch returns.
async fn seen_until(stub: &Path, needle: &str) -> Vec<String> {
    let path = PathBuf::from(format!("{}.seen", stub.display()));
    let until = tokio::time::Instant::now() + STUB_DEADLINE;
    loop {
        if let Ok(text) = std::fs::read_to_string(&path) {
            if text.contains(needle) {
                return text.lines().map(str::to_string).collect();
            }
        }
        assert!(
            tokio::time::Instant::now() < until,
            "{} never said {needle:?}: {:?}",
            path.display(),
            std::fs::read_to_string(&path)
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// A spec whose session keeps one variable and is told its MCP server.
fn spec_with_env(dir: &Path, prompt: &str) -> SessionSpec {
    let mut spec = spec(dir, prompt);
    spec.env.insert("BISA_TEST_KEPT".into(), "yes".into());
    spec.env_remove.push("BISA_TEST_GONE".into());
    spec.mcp_servers = vec![bisa_harness::McpMount::platform(
        bisa_harness::McpServerConfig::Stdio {
            name: "bisa".into(),
            command: "/bin/bisa".into(),
            args: vec!["mcp".into()],
            env: Default::default(),
            cwd: Some(dir.display().to_string()),
        },
    )];
    spec
}

#[tokio::test]
async fn claude_code_is_told_its_servers_and_its_environment_and_answers_for_itself() {
    let dir = tempfile::tempdir().unwrap();
    let stub = write_stub(
        dir.path(),
        "claude",
        r#"
printf '%s\n' "$*" > "$0.seen"
echo "kept=$BISA_TEST_KEPT" >> "$0.seen"
read -r _line
echo '{"type":"system","subtype":"init","session_id":"sess-1","model":"m"}'
echo '{"type":"result","subtype":"success","is_error":false,"session_id":"sess-1"}'
while read -r _l; do :; done
"#,
    );
    let adapter = bisa_adapters::claude_code::ClaudeCodeAdapter {
        program: stub.display().to_string(),
        ..Default::default()
    };
    let session = adapter
        .launch(spec_with_env(dir.path(), "do the thing"))
        .await
        .unwrap();
    let mut stream = session.subscribe();
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;
    assert!(ended_completed(&events), "{events:?}");
    let seen = seen_by(&stub);
    assert!(seen[0].contains("--mcp-config"), "{seen:?}");
    assert_eq!(seen[1], "kept=yes");

    assert_eq!(session.phase(), bisa_harness::Phase::Idle);
    assert_eq!(session.snapshot().phase, bisa_harness::Phase::Idle);
    session
        .prompt("".into())
        .await
        .expect("nothing to say is nothing sent");
    session.prompt("again".into()).await.unwrap();
    assert_eq!(session.phase(), bisa_harness::Phase::Turn);
    session.follow_up("and then".into()).await.unwrap();
    assert!(
        matches!(
            session.prompt("once more".into()).await,
            Err(HarnessError::Busy)
        ),
        "a turn is running"
    );
    session.abort().await.unwrap();
    assert!(
        matches!(
            adapter.translate_report(
                &serde_json::json!({"hook_event_name": "Stop", "session_id": "s"})
            )[..],
            [SessionEvent::Progress(ProgressEvent::TurnEnded)]
        ),
        "the adapter's own reporter translation"
    );

    // A launch with nothing to say starts a session that waits.
    let quiet = adapter.launch(spec(dir.path(), "")).await.unwrap();
    assert_eq!(quiet.phase(), bisa_harness::Phase::Idle);
    quiet.dispose().await.unwrap();

    // Revived from its token: the CLI is asked to resume the session it named.
    let token = token_for("claude-code", "sess-1", dir.path(), None);
    let revived = adapter.attach(&token).await.unwrap();
    assert_eq!(revived.phase(), bisa_harness::Phase::Idle);
    let seen = seen_until(&stub, "--resume").await;
    assert!(seen[0].contains("--resume sess-1"), "{seen:?}");
    revived.dispose().await.unwrap();
    let wrong = token_for("codex", "cx-1", dir.path(), None);
    assert!(matches!(
        adapter.attach(&wrong).await.err(),
        Some(HarnessError::Protocol(_))
    ));
}

/// The pi stub: a plain line before its JSON, the state, a turn per prompt,
/// an ack for a steer and a follow-up — and what it was started with kept.
fn pi_like_stub(dir: &Path, name: &str, usage_fixture: &Path) -> PathBuf {
    write_stub(
        dir,
        name,
        &format!(
            r#"
if [ "$1" = "usage" ]; then cat '{usage}'; exit 0; fi
printf '%s\n' "$*" > "$0.seen"
echo "kept=$BISA_TEST_KEPT" >> "$0.seen"
echo "starting up"
echo '{{"type":"ready","protocolVersion":1,"supportedProtocolVersions":[1,2],"maxFrameBytes":1048576,"maxReassembledFrameBytes":67108864}}'
while read -r line; do
  case "$line" in
    *'"type":"negotiate_protocol"'*)
      echo '{{"type":"response","command":"negotiate_protocol","success":true,"id":"bisa-proto"}}'
      ;;
    *'"type":"get_state"'*)
      echo '{{"type":"response","command":"get_state","success":true,"id":"bisa-state","data":{{"sessionId":"s-7","sessionFile":"{transcript}"}}}}'
      ;;
    *'"type":"prompt"'*)
      echo '{{"type":"agent_start"}}'
      echo '{{"type":"rpc_chunk","chunkId":"c-9","index":1,"count":2,"byteLength":4,"data":"YWI="}}'
      echo '{{"type":"rpc_chunk","chunkId":"c-8","index":0,"count":2,"byteLength":4,"data":"YWI="}}'
      echo '{{"type":"message_end","message":{{"role":"assistant","content":[{{"type":"text","text":"said"}}]}}}}'
      echo '{{"type":"agent_end"}}'
      ;;
    *'"type":"steer"'*|*'"type":"follow_up"'*)
      echo '{{"type":"response","command":"steer","success":true}}'
      ;;
  esac
done
"#,
            usage = usage_fixture.display(),
            transcript = dir.join("s-7.jsonl").display(),
        ),
    )
}

#[tokio::test]
async fn omp_and_pi_revive_from_their_tokens_and_answer_for_themselves() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/usage/omp.json");
    let omp_stub = pi_like_stub(dir.path(), "omp", &fixture);
    let pi_stub = pi_like_stub(dir.path(), "pi", &fixture);
    let omp = bisa_adapters::omp::OmpAdapter {
        program: omp_stub.display().to_string(),
        extra_args: vec!["--extra".into()],
    };
    let pi = bisa_adapters::pi_rpc::PiRpcAdapter {
        program: pi_stub.display().to_string(),
        extra_args: vec![],
    };
    for (adapter, stub, id) in [
        (&omp as &dyn HarnessAdapter, &omp_stub, "omp"),
        (&pi as &dyn HarnessAdapter, &pi_stub, "pi"),
    ] {
        let session = adapter
            .launch(spec_with_env(dir.path(), "hello"))
            .await
            .unwrap();
        let mut stream = session.subscribe();
        let events = collect_until_end(&mut stream, STUB_DEADLINE).await;
        assert!(has_text_delta(&events, "said"), "{id}: {events:?}");
        assert!(ended_completed(&events), "{id}: {events:?}");
        let seen = seen_by(stub);
        assert_eq!(seen[1], "kept=yes", "{id}");
        if id == "omp" {
            assert!(seen[0].starts_with("--extra --mode rpc"), "{seen:?}");
        }
        assert_eq!(session.phase(), bisa_harness::Phase::Idle, "{id}");
        assert_eq!(session.snapshot().phase, bisa_harness::Phase::Idle);
        session.follow_up("then".into()).await.unwrap();
        session.steer("left".into()).await.unwrap();
        session.prompt("again".into()).await.unwrap();
        session.abort().await.unwrap();

        let ctx = reporting_ctx();
        assert_ne!(
            adapter.interactive_reporting(&ctx),
            bisa_harness::ReportingPlan::default(),
            "{id}: the pi family reports through its extension"
        );
        assert!(matches!(
            adapter.translate_report(&serde_json::json!({"event": "session_start"}))[..],
            [SessionEvent::Lifecycle(LifecycleEvent::Started)]
        ));

        let wrong = token_for("codex", "cx-1", dir.path(), None);
        assert!(matches!(
            adapter.attach(&wrong).await.err(),
            Some(HarnessError::Protocol(_))
        ));
        let placeless = token_for(id, "s-7", dir.path(), None);
        match adapter.attach(&placeless).await.err() {
            Some(HarnessError::Protocol(why)) => {
                assert!(why.contains("names no transcript"), "{why}")
            }
            other => panic!("{id}: {other:?}"),
        }
        let transcript = dir.path().join("s-7.jsonl");
        let token = token_for(id, "s-7", dir.path(), Some(transcript.clone()));
        let revived = adapter.attach(&token).await.unwrap();
        assert_eq!(revived.phase(), bisa_harness::Phase::Idle, "{id}");
        let seen = seen_until(stub, "--session").await;
        assert!(
            seen[0].contains(&format!("--session {}", transcript.display())),
            "{id}: {seen:?}"
        );
        revived.dispose().await.unwrap();
    }
    match pi.usage().await {
        bisa_harness::UsageState::Unsupported { reason } => {
            assert!(reason.contains("pi"), "{reason}")
        }
        other => panic!("{other:?}"),
    }
    match omp.usage().await {
        bisa_harness::UsageState::Report { report } => {
            assert_eq!(report.harness, "omp");
            assert!(
                report
                    .windows
                    .iter()
                    .any(|w| w.scope.as_deref() == Some("anthropic")),
                "{report:?}"
            );
        }
        other => panic!("{other:?}"),
    }
    // An omp whose usage is no JSON, and one that exits on the question,
    // are failed reads said by name.
    let garbled = write_stub(dir.path(), "omp-garbled", "echo nope\n");
    let silent = write_stub(dir.path(), "omp-silent", "exit 3\n");
    for (program, word) in [(garbled, "not JSON"), (silent, "exited with 3")] {
        let adapter = bisa_adapters::omp::OmpAdapter {
            program: program.display().to_string(),
            extra_args: vec![],
        };
        match adapter.usage().await {
            bisa_harness::UsageState::Failed { reason } => {
                assert!(reason.contains(word), "{reason}")
            }
            other => panic!("{other:?}"),
        }
    }
}

/// The codex stub: `app-server` answers the usage protocol, `exec` a run —
/// a plain line, then the events the test asked for by its mode file.
fn codex_stub(dir: &Path) -> PathBuf {
    let account =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/usage/codex-account.json");
    let limits =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/usage/codex-limits.json");
    write_stub(
        dir,
        "codex",
        &format!(
            r#"
if [ "$1" = "app-server" ]; then
  while read -r line; do
    case "$line" in
      *'"method":"initialize"'*) echo '{{"jsonrpc":"2.0","id":1,"result":{{}}}}' ;;
      *'"method":"account/read"'*) printf '{{"jsonrpc":"2.0","id":2,"result":%s}}\n' "$(tr -d '\n' < '{account}')" ;;
      *'"method":"account/rateLimits/read"'*) printf '{{"jsonrpc":"2.0","id":3,"result":%s}}\n' "$(tr -d '\n' < '{limits}')"; exit 0 ;;
    esac
  done
  exit 0
fi
printf '%s\n' "$*" > "$0.seen"
echo "kept=$BISA_TEST_KEPT" >> "$0.seen"
mode="$(cat "$0.mode" 2>/dev/null || echo quick)"
echo "plain words first"
echo '{{"type":"thread.started","thread_id":"cx-3"}}'
case "$mode" in
  slow) sleep 30 ;;
  wall) echo '{{"type":"turn.failed","error":{{"message":"usage limit reached"}}}}'; sleep 30 ;;
  *) echo '{{"type":"turn.completed","usage":{{"input_tokens":1,"output_tokens":1}}}}' ;;
esac
exit 0
"#,
            account = account.display(),
            limits = limits.display(),
        ),
    )
}

#[tokio::test]
async fn codex_runs_one_process_a_turn_and_the_facade_answers_for_itself() {
    let dir = tempfile::tempdir().unwrap();
    let stub = codex_stub(dir.path());
    let adapter = bisa_adapters::codex::CodexAdapter {
        program: stub.display().to_string(),
    };
    // A quick run: the environment reaches the process, the facade answers.
    let session = adapter
        .launch(spec_with_env(dir.path(), "list files"))
        .await
        .unwrap();
    let mut stream = session.subscribe();
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;
    assert!(ended_completed(&events), "{events:?}");
    assert_eq!(seen_by(&stub)[1], "kept=yes");
    assert_eq!(session.phase(), bisa_harness::Phase::Idle);
    assert_eq!(session.snapshot().phase, bisa_harness::Phase::Idle);
    assert!(matches!(
        session.steer("x".into()).await,
        Err(HarnessError::NotSupported("steer"))
    ));
    assert!(matches!(
        session.follow_up("x".into()).await,
        Err(HarnessError::NotSupported("follow_up"))
    ));
    session
        .abort()
        .await
        .expect("an abort after the run is nothing to end");
    let ctx = reporting_ctx();
    assert_ne!(
        adapter.interactive_reporting(&ctx),
        bisa_harness::ReportingPlan::default()
    );
    assert!(matches!(
        adapter.translate_report(
            &serde_json::json!({"hook_event_name": "Stop", "session_id": "s", "turn_id": "t"})
        )[..],
        [SessionEvent::Progress(ProgressEvent::TurnEnded)]
    ));
    match adapter.usage().await {
        bisa_harness::UsageState::Report { report } => {
            let labels: Vec<&str> = report.windows.iter().map(|w| w.label.as_str()).collect();
            assert_eq!(labels, ["5h", "Weekly"]);
            assert_eq!(report.account.unwrap().plan.as_deref(), Some("pro"));
        }
        other => panic!("{other:?}"),
    }

    // A run that goes on: a second prompt is refused busy, and an abort ends
    // the run's process and the turn, not the session.
    std::fs::write(format!("{}.mode", stub.display()), "slow").unwrap();
    let session = adapter
        .launch(spec(dir.path(), "take your time"))
        .await
        .unwrap();
    assert!(matches!(
        session.prompt("again".into()).await,
        Err(HarnessError::Busy)
    ));
    let mut stream = session.subscribe();
    session.abort().await.unwrap();
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;
    assert!(
        events.iter().any(|e| matches!(
            e,
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: Outcome::Aborted,
                is_terminal: false
            })
        )),
        "{events:?}"
    );
    assert_eq!(session.phase(), bisa_harness::Phase::Idle);
    session.dispose().await.unwrap();

    // A model wall mid-run ends the session; a prompt after it is refused.
    std::fs::write(format!("{}.mode", stub.display()), "wall").unwrap();
    let session = adapter
        .launch(spec(dir.path(), "hit the wall"))
        .await
        .unwrap();
    let mut stream = session.subscribe();
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;
    assert!(
        events.iter().any(|e| matches!(
            e,
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: Outcome::ModelUnavailable { .. },
                is_terminal: true
            })
        )),
        "{events:?}"
    );
    assert!(matches!(
        session.prompt("again".into()).await,
        Err(HarnessError::Terminated)
    ));

    // Revived from its token: the next turn resumes the thread it names; a
    // token of another adapter's is refused; an abort before any run ends nothing.
    std::fs::write(format!("{}.mode", stub.display()), "quick").unwrap();
    let token = token_for("codex", "cx-3", dir.path(), None);
    let revived = adapter.attach(&token).await.unwrap();
    assert_eq!(revived.phase(), bisa_harness::Phase::Idle);
    revived
        .abort()
        .await
        .expect("an abort before any run ends nothing");
    let mut stream = revived.subscribe();
    revived.prompt("go on".into()).await.unwrap();
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;
    assert!(ended_completed(&events), "{events:?}");
    assert!(
        seen_by(&stub)[0].starts_with("exec resume cx-3 --json"),
        "{:?}",
        seen_by(&stub)
    );
    let wrong = token_for("opencode", "ses_1", dir.path(), None);
    assert!(matches!(
        adapter.attach(&wrong).await.err(),
        Some(HarnessError::Protocol(_))
    ));
}

#[tokio::test]
async fn opencode_one_revives_by_its_session_and_the_facade_answers_for_itself() {
    let dir = tempfile::tempdir().unwrap();
    let stub = write_stub(
        dir.path(),
        "opencode",
        r#"
if [ "$1" = "--version" ]; then echo "1.18.23"; exit 0; fi
printf '%s\n' "$*" > "$0.seen"
echo "kept=$BISA_TEST_KEPT" >> "$0.seen"
echo '{"type":"text","sessionID":"ses_1","part":{"type":"text","text":"oc hello"}}'
exit 0
"#,
    );
    let adapter = bisa_adapters::opencode::OpencodeAdapter {
        program: stub.display().to_string(),
        ..Default::default()
    };
    let session = adapter
        .launch(spec_with_env(dir.path(), "hi"))
        .await
        .unwrap();
    let mut stream = session.subscribe();
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;
    assert!(ended_completed(&events), "{events:?}");
    assert_eq!(seen_by(&stub)[1], "kept=yes");
    assert_eq!(session.phase(), bisa_harness::Phase::Idle);
    assert_eq!(session.snapshot().phase, bisa_harness::Phase::Idle);
    assert!(matches!(
        session.steer("x".into()).await,
        Err(HarnessError::NotSupported("steer"))
    ));
    assert!(matches!(
        session.follow_up("x".into()).await,
        Err(HarnessError::NotSupported("follow_up"))
    ));
    let mut stream = session.subscribe();
    session.prompt("again".into()).await.unwrap();
    assert!(ended_completed(
        &collect_until_end(&mut stream, STUB_DEADLINE).await
    ));
    session.abort().await.unwrap();
    // The wire is known now: the next launch asks no version.
    let again = adapter.launch(spec(dir.path(), "")).await.unwrap();
    again.dispose().await.unwrap();

    let token = token_for("opencode", "ses_1", dir.path(), None);
    let revived = adapter.attach(&token).await.unwrap();
    assert_eq!(revived.phase(), bisa_harness::Phase::Idle);
    let mut stream = revived.subscribe();
    revived.prompt("go on".into()).await.unwrap();
    assert!(ended_completed(
        &collect_until_end(&mut stream, STUB_DEADLINE).await
    ));
    assert!(
        seen_by(&stub)[0].contains("--session ses_1"),
        "{:?}",
        seen_by(&stub)
    );
    let wrong = token_for("codex", "cx-1", dir.path(), None);
    assert!(matches!(
        adapter.attach(&wrong).await.err(),
        Some(HarnessError::Protocol(_))
    ));
}

#[tokio::test]
async fn a_custom_harness_resumes_nothing_and_its_facade_answers_for_itself() {
    let dir = tempfile::tempdir().unwrap();
    let stub = write_stub(
        dir.path(),
        "my-agent",
        r#"
read -r _prompt
echo '{"type":"started"}'
echo '{"type":"ended","outcome":"completed"}'
while read -r _l; do :; done
"#,
    );
    let adapter =
        bisa_adapters::custom_json::CustomJsonAdapter::new(bisa_harness::CustomHarnessSpec {
            id: "my-agent".into(),
            label: "My Agent".into(),
            command: stub.display().to_string(),
            args: vec![],
            resume_args: vec![],
            env: Default::default(),
            install_hint: None,
        });
    let token = token_for("custom:my-agent", "x", dir.path(), None);
    assert!(matches!(
        adapter.attach(&token).await.err(),
        Some(HarnessError::NotSupported("resume"))
    ));
    let session = adapter.launch(spec(dir.path(), "go")).await.unwrap();
    let mut stream = session.subscribe();
    collect_until_end(&mut stream, STUB_DEADLINE).await;
    assert_eq!(session.snapshot().phase, session.phase());
    assert!(matches!(
        session.steer("x".into()).await,
        Err(HarnessError::NotSupported("steer"))
    ));
    session.follow_up("then".into()).await.unwrap();
    session.dispose().await.unwrap();
}

/// A pi session launched with nothing to say waits for its first prompt.
#[tokio::test]
async fn pi_launched_with_nothing_to_say_waits() {
    let dir = tempfile::tempdir().unwrap();
    let stub = write_stub(dir.path(), "pi", "while read -r _l; do :; done\n");
    let adapter = bisa_adapters::pi_rpc::PiRpcAdapter {
        program: stub.display().to_string(),
        extra_args: vec![],
    };
    let session = adapter.launch(spec(dir.path(), "")).await.unwrap();
    assert_eq!(session.phase(), bisa_harness::Phase::Idle);
    session.dispose().await.unwrap();
}

/// OpenCode 2 lists its models as its CLI prints them, each taking every
/// level — the session says which it takes.
#[tokio::test]
async fn opencode_two_lists_its_models_with_every_level() {
    use bisa_adapters::opencode::{OpencodeAdapter, Wire};
    let dir = tempfile::tempdir().unwrap();
    let stub = opencode_two_stub(dir.path());
    let adapter = OpencodeAdapter {
        program: stub.display().to_string(),
        ..Default::default()
    };
    assert!(adapter.probe().await.available);
    assert_eq!(adapter.wire_known(), Some(Wire::Acp));
    let models = adapter.models().await;
    let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["anthropic/claude-sonnet-5", "openai/gpt-5"]);
    assert!(models
        .iter()
        .all(|m| m.efforts == bisa_harness::Effort::ALL.to_vec()));
}

/// An omp that speaks only the first protocol version is never asked to
/// negotiate the second: its ready frame passes through and the turn runs.
#[tokio::test]
async fn omp_on_the_first_protocol_version_is_not_asked_to_negotiate() {
    let dir = tempfile::tempdir().unwrap();
    let stub = write_stub(
        dir.path(),
        "omp",
        r#"
echo '{"type":"ready","protocolVersion":1,"supportedProtocolVersions":[1]}'
while read -r line; do
  case "$line" in
    *'"type":"negotiate_protocol"'*) echo '{"type":"response","command":"negotiate_protocol","success":false,"error":"unknown"}'; exit 9 ;;
    *'"type":"get_state"'*)
      echo '{"type":"response","command":"get_state","success":true,"id":"bisa-state","data":{"sessionId":"omp-v1"}}'
      ;;
    *'"type":"prompt"'*)
      echo '{"type":"agent_start"}'
      echo '{"type":"message_end","message":{"role":"assistant","content":[{"type":"text","text":"v1 says hi"}]}}'
      echo '{"type":"agent_end"}'
      ;;
  esac
done
"#,
    );
    let adapter = bisa_adapters::omp::OmpAdapter {
        program: stub.display().to_string(),
        extra_args: vec![],
    };
    let session = adapter.launch(spec(dir.path(), "hello")).await.unwrap();
    let mut stream = session.subscribe();
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;
    assert!(has_text_delta(&events, "v1 says hi"), "{events:?}");
    assert!(ended_completed(&events), "{events:?}");
    assert_eq!(session.resume_token().unwrap().native_id, "omp-v1");
    session.dispose().await.unwrap();
}

// ---------------------------------------------------------------------------
// Muse Code, over the Muse Session Protocol
// ---------------------------------------------------------------------------

/// The frames every stub host answers the handshake with: the `initialize`
/// result naming the schema this adapter was written against, and a model
/// list of two — `small`, the default, taking two levels; `large` three.
fn msp_handshake_lines() -> String {
    format!(
        r#"    *'"method":"initialize"'*)
      echo '{{"jsonrpc":"2.0","id":1,"result":{{"serverInfo":{{"name":"muse-session-server","version":"1.4.10"}},"museHome":"/home/me/.local/share/muse","schema":{{"version":1,"fingerprint":"{fp}"}},"grantedCapabilities":[],"experimentalApi":true,"sessionDurability":"durable"}}}}'
      ;;
    *'"method":"model/list"'*)
      echo "models" >> "$seen"
      echo '{{"jsonrpc":"2.0","id":2,"result":{{"models":[{{"modelId":"small","providerId":"muse","displayLabel":"Small","isDefault":true,"variants":["low","high"]}},{{"modelId":"large","providerId":"muse","displayLabel":"Large","isDefault":false,"variants":["low","high","max"]}}]}}}}'
      ;;
"#,
        fp = bisa_adapters::msp::SCHEMA_FINGERPRINT
    )
}

/// A host that starts a session on the model asked and answers every
/// `turn/start` with one turn: a word, its usage, and its end. It keeps what
/// it was started with and what it was told, in order, in `<stub>.seen`.
fn msp_stub_host(dir: &Path, name: &str, turn: &str) -> PathBuf {
    write_stub(
        dir,
        name,
        &format!(
            r#"
seen="$0.seen"
echo "args: $*" > "$seen"
while read -r line; do
  case "$line" in
{handshake}
    *'"method":"session/start"'*)
      echo "start: $line" >> "$seen"
      echo '{{"jsonrpc":"2.0","method":"session/started","params":{{"session":{{"sessionId":"s-1","status":"idle","modelId":"large"}}}},"emittedAtMs":1}}'
      echo '{{"jsonrpc":"2.0","id":3,"result":{{"session":{{"sessionId":"s-1","status":"idle","turnCount":0,"workspaceRoot":"/work","providerId":"muse","modelId":"large","approvalMode":{{"mode":"promptUnmatched"}}}},"viewCursor":"v4:s-1:3"}}}}'
      ;;
    *'"method":"turn/start"'*)
      echo "turn: $line" >> "$seen"
      id=$(printf '%s' "$line" | sed 's/.*"id":\([0-9]*\).*/\1/')
      echo "{{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{{\"commandId\":\"c\",\"status\":\"accepted\",\"turnId\":\"t-$id\",\"disposition\":\"started\",\"startedNewTurn\":true}}}}"
      echo '{{"jsonrpc":"2.0","method":"turn/started","params":{{"sessionId":"s-1","turnId":"t-1"}}}}'
{turn}
      ;;
    *'"method":"turn/cancel"'*)
      echo "cancel: $line" >> "$seen"
      echo '{{"jsonrpc":"2.0","method":"turn/completed","params":{{"sessionId":"s-1","turnId":"t-1","terminal":"cancelled"}}}}'
      ;;
    *'"method":"approval/decide"'*)
      echo "decide: $line" >> "$seen"
      echo '{{"jsonrpc":"2.0","method":"approval/resolved","params":{{"sessionId":"s-1","approvalId":"a-1","decision":"approved","policyResult":"allow"}}}}'
      echo '{{"jsonrpc":"2.0","method":"item/completed","params":{{"sessionId":"s-1","item":{{"itemId":"i-t","kind":"toolCall","status":"completed","tool":"bash","callId":"call_1","visibleOutput":"done"}}}}}}'
      echo '{{"jsonrpc":"2.0","method":"turn/completed","params":{{"sessionId":"s-1","turnId":"t-1","terminal":"completed"}}}}'
      ;;
    *'"method":"usage/read"'*)
      echo "usage/read" >> "$seen"
      echo '{{"jsonrpc":"2.0","id":2,"result":{{"usage":{{"tier":"everyday","window":{{"usedPercent":12,"windowDurationMins":300,"resetsAtMs":1788782400000}},"weekly":{{"usedPercent":3,"resetsAtMs":1789128000000}}}}}}}}'
      ;;
  esac
done
"#,
            handshake = msp_handshake_lines(),
        ),
    )
}

/// A turn of one word, its usage, and a clean end.
const MSP_WORD_TURN: &str = r#"      echo 'a line of the host that is no frame, passed over'
      echo '{"jsonrpc":"2.0","method":"item/started","params":{"sessionId":"s-1","item":{"itemId":"i-1","kind":"agentMessage","status":"inProgress","text":""}}}'
      echo '{"jsonrpc":"2.0","method":"item/delta","params":{"sessionId":"s-1","itemId":"i-1","field":"text","delta":"hello from the host"}}'
      echo '{"jsonrpc":"2.0","method":"item/completed","params":{"sessionId":"s-1","item":{"itemId":"i-1","kind":"agentMessage","status":"completed","text":"hello from the host"}}}'
      echo '{"jsonrpc":"2.0","method":"session/tokenUsage","params":{"sessionId":"s-1","turnId":"t-1","modelId":"large","usage":{"promptTokens":120,"outputTokens":8,"cost":0.012},"cumulative":{"promptTokens":120,"outputTokens":8,"cost":0.012}}}'
      echo '{"jsonrpc":"2.0","method":"usage/changed","params":{"usage":{"tier":"power","window":{"usedPercent":41,"windowDurationMins":300,"resetsAtMs":1788782400000},"weekly":{"usedPercent":9,"resetsAtMs":1789128000000}}}}'
      echo '{"jsonrpc":"2.0","method":"turn/completed","params":{"sessionId":"s-1","turnId":"t-1","terminal":"completed","durationMs":40,"usage":{"inputTokens":120,"outputTokens":8}}}'"#;

/// A turn that asks before a tool: the call starts, the host asks twice —
/// the notification and the request — and waits on `approval/decide`.
const MSP_ASKING_TURN: &str = r#"      echo '{"jsonrpc":"2.0","method":"item/started","params":{"sessionId":"s-1","item":{"itemId":"i-t","kind":"toolCall","status":"inProgress","tool":"bash","callId":"call_1","args":"{\"command\":\"cargo test\"}"}}}'
      echo '{"jsonrpc":"2.0","method":"approval/requested","params":{"sessionId":"s-1","turnId":"t-1","itemId":"i-t","toolCallId":"call_1","approvalId":"a-1","toolName":"bash","rawArgs":"{\"command\":\"cargo test\"}","subject":{"kind":"shell"},"currentRequirementId":{"approvalId":"a-1","sourceIndex":0},"availableChoices":[{"choiceId":"allow_session","decision":"approvedForSession","scope":"session","label":"Allow for this session"},{"choiceId":"allow_once","decision":"approved","scope":"once","label":"Allow once"},{"choiceId":"abort","decision":"abort","scope":"once","label":"Abort","acceptsFeedback":true}]}}'
      echo '{"jsonrpc":"2.0","id":"ap-1","method":"approval/request","params":{"sessionId":"s-1","turnId":"t-1","itemId":"i-t","toolCallId":"call_1","approvalId":"a-1","toolName":"bash","rawArgs":"{\"command\":\"cargo test\"}","subject":{"kind":"shell"},"currentRequirementId":{"approvalId":"a-1","sourceIndex":0},"availableChoices":[{"choiceId":"allow_session","decision":"approvedForSession","scope":"session","label":"Allow for this session"},{"choiceId":"allow_once","decision":"approved","scope":"once","label":"Allow once"},{"choiceId":"abort","decision":"abort","scope":"once","label":"Abort","acceptsFeedback":true}]}}'"#;

/// A turn that holds: a word, then nothing until a cancel.
const MSP_HOLDING_TURN: &str = r#"      echo '{"jsonrpc":"2.0","method":"item/started","params":{"sessionId":"s-1","item":{"itemId":"i-1","kind":"agentMessage","status":"inProgress","text":""}}}'
      echo '{"jsonrpc":"2.0","method":"item/delta","params":{"sessionId":"s-1","itemId":"i-1","field":"text","delta":"muse hello"}}'"#;

fn muse_on(stub: &Path) -> bisa_adapters::muse_code::MuseCodeAdapter {
    bisa_adapters::muse_code::MuseCodeAdapter::with_program(stub.display().to_string())
}

#[tokio::test]
async fn muse_code_is_started_as_a_trusted_host_on_its_model_and_turned_at_a_fitted_level() {
    let dir = tempfile::tempdir().unwrap();
    let stub = msp_stub_host(dir.path(), "muse", MSP_WORD_TURN);
    let adapter = muse_on(&stub);
    let mut asked = spec_with_env(dir.path(), "hello");
    asked.model = Some("large".into());
    asked.effort = Some(bisa_harness::Effort::Xhigh);
    let session = adapter.launch(asked).await.unwrap();
    let mut stream = session.subscribe();
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;
    assert!(has_text_delta(&events, "hello from the host"), "{events:?}");
    assert!(
        events.iter().any(|e| matches!(e, SessionEvent::Progress(ProgressEvent::ModelChanged { model }) if model == "large")),
        "{events:?}"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            SessionEvent::Progress(ProgressEvent::CostDelta {
                input_tokens: 120,
                output_tokens: 8,
                usd_cents: 1
            })
        )),
        "{events:?}"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: Outcome::Completed,
                is_terminal: false
            })
        )),
        "{events:?}"
    );
    let token = session.resume_token().unwrap();
    assert_eq!(token.adapter_id, "muse-code");
    assert_eq!(token.native_id, "s-1");
    assert_eq!(token.model.as_deref(), Some("large"));
    assert_eq!(token.effort, Some(bisa_harness::Effort::Xhigh));
    assert_eq!(
        session.snapshot().cost.input_tokens,
        120,
        "counted once: the turn's end does not add the turn again"
    );
    // A steer and a follow-up ride the same frame under their words.
    session
        .steer(bisa_harness::Steer {
            text: "also".into(),
            attachments: vec![],
        })
        .await
        .unwrap();
    seen_until(&stub, r#""ifBusy":"steer""#).await;
    session
        .follow_up(bisa_harness::Steer {
            text: "then".into(),
            attachments: vec![],
        })
        .await
        .unwrap();
    seen_until(&stub, r#"then"#).await;
    // The usage the session heard answers the footer without a host.
    match adapter.usage().await {
        bisa_harness::UsageState::Report { report } => {
            assert_eq!(report.account.unwrap().plan.as_deref(), Some("power"));
            assert_eq!(report.windows[0].used_percent, 41.0);
        }
        other => panic!("{other:?}"),
    }
    session.dispose().await.unwrap();

    let seen = seen_by(&stub);
    assert_eq!(
        seen[0], "args: serve --trust-workspace",
        "the host, trusted in the checkout, and nothing else on the command line"
    );
    assert_eq!(
        seen[1], "models",
        "the host's list is read before the session is started"
    );
    let start = &seen[2];
    assert!(start.starts_with("start: "), "{seen:?}");
    assert!(
        start.contains(&format!(r#""workspaceRoot":"{}""#, dir.path().display())),
        "{start}"
    );
    assert!(start.contains(r#""modelId":"large""#), "{start}");
    assert!(
        start.contains(r#""approvalMode":"promptUnmatched""#),
        "{start}"
    );
    assert!(start.contains(r#""mcpServers":{"bisa":{"transport":"stdio","command":"/bin/bisa","args":["mcp"],"env":{}}}"#), "the session's own servers, without the cwd the protocol has no word for: {start}");
    assert!(!start.contains("excludeItems"), "{start}");
    let turn = &seen[3];
    assert!(turn.starts_with("turn: "), "{seen:?}");
    assert!(
        turn.contains(r#""reasoningEffort":"high""#),
        "xhigh is held to the levels `large` lists: {turn}"
    );
    assert!(turn.contains(r#""ifBusy":"queue""#), "{turn}");
    assert!(turn.contains(r#"{"type":"text","text":"hello"}"#), "{turn}");
    assert!(
        !seen.iter().any(|l| l == "usage/read"),
        "a usage heard asks no host: {seen:?}"
    );
}

#[tokio::test]
async fn a_model_the_host_does_not_list_ends_the_launch_as_model_unavailable_before_any_session() {
    let dir = tempfile::tempdir().unwrap();
    let stub = msp_stub_host(dir.path(), "muse", MSP_WORD_TURN);
    let adapter = muse_on(&stub);
    let mut asked = spec(dir.path(), "hello");
    asked.model = Some("huge".into());
    let session = adapter.launch(asked).await.unwrap();
    let mut stream = session.subscribe();
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;
    match events.last() {
        Some(SessionEvent::Lifecycle(LifecycleEvent::Ended {
            outcome: Outcome::ModelUnavailable { model, reason, .. },
            is_terminal: true,
        })) => {
            assert_eq!(model, "huge");
            assert!(reason.contains("does not list this model"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, SessionEvent::Progress(_))),
        "no progress before the wall: {events:?}"
    );
    let seen = seen_until(&stub, "models").await;
    assert!(
        !seen.iter().any(|l| l.starts_with("start:")),
        "no session was started: {seen:?}"
    );
}

#[tokio::test]
async fn muse_code_asks_before_a_tool_and_an_answer_picks_the_once_choice() {
    let dir = tempfile::tempdir().unwrap();
    let stub = msp_stub_host(dir.path(), "muse", MSP_ASKING_TURN);
    let adapter = muse_on(&stub);
    for (answer, choice) in [
        (
            bisa_harness::InputAnswer::ALLOW,
            r#""choiceId":"allow_once""#,
        ),
        (
            bisa_harness::InputAnswer::Deny {
                reason: "not here".into(),
            },
            r#""choiceId":"abort""#,
        ),
    ] {
        let session = adapter
            .launch(spec(dir.path(), "run the tests"))
            .await
            .unwrap();
        let mut stream = session.subscribe();
        let mut before = Vec::new();
        let request = loop {
            let ev = tokio::time::timeout(STUB_DEADLINE, stream.next())
                .await
                .expect("an ask before the deadline")
                .expect("stream open");
            if let SessionEvent::Lifecycle(LifecycleEvent::InputRequested { request }) = &ev {
                let request = request.clone();
                before.push(ev);
                break request;
            }
            before.push(ev);
        };
        assert_eq!(request.id, "a-1");
        match &request.kind {
            bisa_harness::InputKind::Permission {
                tool_name,
                tier,
                input,
                ..
            } => {
                assert_eq!(tool_name, "bash");
                assert_eq!(*tier, ToolTier::Exec);
                assert_eq!(input, &serde_json::json!({ "command": "cargo test" }));
            }
            other => panic!("{other:?}"),
        }
        assert!(
            before.iter().any(|e| matches!(e, SessionEvent::Progress(ProgressEvent::ToolStarted { name, tier: ToolTier::Exec, .. }) if name == "bash")),
            "{before:?}"
        );
        assert_eq!(session.phase(), bisa_harness::Phase::AwaitingInput);
        session.answer("a-1", answer).await.unwrap();
        let mut events = before;
        events.extend(collect_until_end(&mut stream, STUB_DEADLINE).await);
        assert!(events.iter().any(|e| matches!(e, SessionEvent::Lifecycle(LifecycleEvent::InputResolved { id }) if id == "a-1")), "{events:?}");
        assert!(events.iter().any(|e| matches!(e, SessionEvent::Progress(ProgressEvent::ToolEnded { name, ok: true, .. }) if name == "bash")), "{events:?}");
        assert!(
            ended_completed(&events),
            "a refused call is a completed turn, never a failed one: {events:?}"
        );
        session.dispose().await.unwrap();
        let seen = seen_until(&stub, "decide:").await;
        let decide = seen.iter().find(|l| l.starts_with("decide:")).unwrap();
        assert!(decide.contains(choice), "{decide}");
        assert!(
            decide.contains(r#""requirementId":{"approvalId":"a-1","sourceIndex":0}"#),
            "{decide}"
        );
        if choice.contains("abort") {
            assert!(decide.contains(r#""feedback":"not here""#), "{decide}");
        } else {
            assert!(!decide.contains("feedback"), "{decide}");
        }
        assert_eq!(
            seen.iter().filter(|l| l.starts_with("decide:")).count(),
            1,
            "answered once: {seen:?}"
        );
    }
}

#[tokio::test]
async fn muse_code_resumes_by_its_session_sets_the_model_back_and_a_held_lease_is_said() {
    let dir = tempfile::tempdir().unwrap();
    let stub = write_stub(
        dir.path(),
        "muse",
        &format!(
            r#"
seen="$0.seen"
echo "args: $*" > "$seen"
while read -r line; do
  case "$line" in
{handshake}
    *'"method":"session/resume"'*)
      echo "resume: $line" >> "$seen"
      echo '{{"jsonrpc":"2.0","id":3,"result":{{"session":{{"sessionId":"s-9","status":"idle","turnCount":2,"workspaceRoot":"/work","providerId":"muse","modelId":"small"}},"viewCursor":"v4:s-9:40","history":{{"mode":"none"}},"pendingRequests":[],"lastTurn":{{"terminal":"completed","turnId":"t-2"}}}}}}'
      ;;
    *'"method":"session/setModel"'*)
      echo "setModel: $line" >> "$seen"
      echo '{{"jsonrpc":"2.0","id":4,"result":{{"commandId":"c","status":"accepted"}}}}'
      ;;
    *'"method":"turn/start"'*)
      echo "turn: $line" >> "$seen"
      id=$(printf '%s' "$line" | sed 's/.*"id":\([0-9]*\).*/\1/')
      echo "{{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{{\"status\":\"accepted\",\"turnId\":\"t-3\",\"disposition\":\"started\",\"startedNewTurn\":true}}}}"
      echo '{{"jsonrpc":"2.0","method":"turn/completed","params":{{"sessionId":"s-9","turnId":"t-3","terminal":"completed"}}}}'
      ;;
  esac
done
"#,
            handshake = msp_handshake_lines(),
        ),
    );
    let adapter = muse_on(&stub);
    let mut token = token_for("muse-code", "s-9", dir.path(), None);
    token.model = Some("large".into());
    token.effort = Some(bisa_harness::Effort::Low);
    let session = adapter.attach(&token).await.unwrap();
    let mut stream = session.subscribe();
    // Revived, idle: a prompt runs a turn on the session as it was.
    seen_until(&stub, "setModel:").await;
    let idle = tokio::time::Instant::now() + STUB_DEADLINE;
    while session.phase() != bisa_harness::Phase::Idle {
        assert!(
            tokio::time::Instant::now() < idle,
            "never idle: {:?}",
            session.phase()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    session.prompt("go on".into()).await.unwrap();
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;
    assert!(ended_completed(&events), "{events:?}");
    let minted = session.resume_token().unwrap();
    assert_eq!(minted.native_id, "s-9");
    assert_eq!(minted.model.as_deref(), Some("large"));
    assert_eq!(minted.effort, Some(bisa_harness::Effort::Low));
    session.dispose().await.unwrap();
    let seen = seen_by(&stub);
    let resume = seen.iter().find(|l| l.starts_with("resume:")).unwrap();
    assert!(
        resume.contains(r#""sessionId":"s-9""#) && resume.contains(r#""excludeItems":true"#),
        "{resume}"
    );
    assert!(
        !resume.contains("workspaceRoot") && !resume.contains("approvalMode"),
        "a resume names the session, not a start: {resume}"
    );
    let set = seen.iter().find(|l| l.starts_with("setModel:")).unwrap();
    assert!(
        set.contains(r#""model":{"modelId":"large"}"#),
        "loaded on `small`, set back to the model the token names: {set}"
    );
    let turn = seen.iter().find(|l| l.starts_with("turn:")).unwrap();
    assert!(turn.contains(r#""reasoningEffort":"low""#), "{turn}");

    // A session another host holds is said as held.
    let held = write_stub(
        dir.path(),
        "muse-held",
        &format!(
            r#"
seen="$0.seen"
echo "args: $*" > "$seen"
while read -r line; do
  case "$line" in
{handshake}
    *'"method":"session/resume"'*)
      echo '{{"jsonrpc":"2.0","id":3,"error":{{"code":-32021,"message":"sessionInUse"}}}}'
      ;;
  esac
done
"#,
            handshake = msp_handshake_lines(),
        ),
    );
    let session = muse_on(&held).attach(&token).await.unwrap();
    let mut stream = session.subscribe();
    let events = collect_until_end(&mut stream, STUB_DEADLINE).await;
    match events.last() {
        Some(SessionEvent::Lifecycle(LifecycleEvent::Ended {
            outcome: Outcome::Failed { error },
            is_terminal: true,
        })) => {
            assert!(error.contains("held by another muse host"), "{error}");
        }
        other => panic!("{other:?}"),
    }
}

#[cfg(unix)]
#[tokio::test]
async fn muse_code_abort_cancels_the_turn_and_ends_the_host_and_its_group() {
    let dir = tempfile::tempdir().unwrap();
    let pids = dir.path().join("pids");
    let stub = msp_stub_host(
        dir.path(),
        "muse",
        &format!(
            "      echo \"$$\" > '{}'\n{MSP_HOLDING_TURN}",
            pids.display()
        ),
    );
    let adapter = muse_on(&stub);
    let session = adapter.launch(spec(dir.path(), "hold on")).await.unwrap();
    let mut stream = session.subscribe();
    let mut before = Vec::new();
    loop {
        let ev = tokio::time::timeout(STUB_DEADLINE, stream.next())
            .await
            .expect("words before the deadline")
            .expect("stream open");
        let said = has_text_delta(std::slice::from_ref(&ev), "muse hello");
        before.push(ev);
        if said {
            break;
        }
    }
    let child = pids_written(&pids).await[0];
    assert!(process_exists(child));
    session.abort().await.unwrap();
    let mut events = before;
    events.extend(collect_until_end(&mut stream, STUB_DEADLINE).await);
    assert!(
        events.iter().any(|e| matches!(
            e,
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: Outcome::Aborted,
                is_terminal: true
            })
        )),
        "a cancelled turn on a stop ends the session for good: {events:?}"
    );
    assert!(
        process_gone_within(child, Duration::from_secs(2)).await,
        "the host's process is gone — a cancel alone left it running"
    );
    let seen = seen_until(&stub, "cancel:").await;
    assert!(
        seen.iter()
            .any(|l| l.starts_with("cancel:") && l.contains(r#""sessionId":"s-1""#)),
        "told the way it understands first: {seen:?}"
    );
}

#[tokio::test]
async fn muse_codes_exit_codes_are_read_in_words() {
    let dir = tempfile::tempdir().unwrap();
    for (code, word) in [
        (0, "ended before its session began"),
        (2, "exit 2"),
        (3, "sign in"),
        (4, "another muse host"),
        (5, "MUSE_EXPERIMENTAL_SDK_ENABLED"),
    ] {
        let stub = write_stub(
            dir.path(),
            &format!("muse-{code}"),
            &format!("read -r _line\necho 'leaving with {code}' >&2\nexit {code}\n"),
        );
        let session = muse_on(&stub)
            .launch(spec(dir.path(), "hello"))
            .await
            .unwrap();
        let mut stream = session.subscribe();
        let events = collect_until_end(&mut stream, STUB_DEADLINE).await;
        match events.last() {
            Some(SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: Outcome::Failed { error },
                is_terminal: true,
            })) => {
                assert!(error.contains(word), "exit {code}: {error}");
                if code == 0 {
                    assert!(
                        error.contains("leaving with 0"),
                        "the stderr tail rides a silent exit: {error}"
                    );
                }
            }
            other => panic!("exit {code}: {other:?}"),
        }
    }
}

#[tokio::test]
async fn muse_code_lists_the_hosts_models_with_their_variants_and_the_levels_follow_and_a_fresh_adapter_asks_a_host_for_its_usage(
) {
    let dir = tempfile::tempdir().unwrap();
    let stub = msp_stub_host(dir.path(), "muse", MSP_WORD_TURN);
    let adapter = muse_on(&stub);
    assert_eq!(
        adapter.efforts(Some("large")),
        bisa_harness::Effort::ALL,
        "every level until the host has listed"
    );
    let models = adapter.models().await;
    assert_eq!(
        models
            .iter()
            .map(|m| (m.id.as_str(), m.label.as_deref()))
            .collect::<Vec<_>>(),
        [("small", Some("Small")), ("large", Some("Large"))],
        "the default first"
    );
    assert_eq!(
        models[1].efforts,
        [
            bisa_harness::Effort::Low,
            bisa_harness::Effort::High,
            bisa_harness::Effort::Max
        ]
    );
    assert_eq!(
        adapter.efforts(Some("large")),
        [
            bisa_harness::Effort::Low,
            bisa_harness::Effort::High,
            bisa_harness::Effort::Max
        ]
    );
    assert_eq!(
        adapter.efforts(None),
        [bisa_harness::Effort::Low, bisa_harness::Effort::High],
        "the default row's"
    );
    let seen = seen_by(&stub);
    assert_eq!(
        seen[0], "args: serve",
        "a host asked a question opens no session: no trust"
    );
    match adapter.usage().await {
        bisa_harness::UsageState::Report { report } => {
            assert_eq!(report.account.unwrap().plan.as_deref(), Some("everyday"));
            assert_eq!(
                report
                    .windows
                    .iter()
                    .map(|w| w.label.as_str())
                    .collect::<Vec<_>>(),
                ["5h", "Weekly"]
            );
        }
        other => panic!("{other:?}"),
    }
    assert!(
        seen_by(&stub).iter().any(|l| l == "usage/read"),
        "nothing heard: the host is asked"
    );
    let probe = adapter.probe().await;
    assert!(
        !probe.available,
        "a stub that answers --version with no version is not the CLI: {probe:?}"
    );
}
