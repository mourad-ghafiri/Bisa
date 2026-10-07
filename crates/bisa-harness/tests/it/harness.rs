//! Integration tests: mock session event flow, capability gating, catalog
//! loading, and subprocess plumbing.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use bisa_core::{HarnessCaps, ToolTier};
use bisa_harness::mock::MockAdapter;
use bisa_harness::proc::{Line, ProcHandle, ProcSpec};
use bisa_harness::{
    HarnessAdapter, HarnessCatalog, HarnessError, HarnessTier, LifecycleEvent, Outcome,
    SessionEvent, SessionSpec,
};
use futures::StreamExt;

fn spec(prompt: &str) -> SessionSpec {
    SessionSpec {
        work_item: None,
        cwd: std::env::temp_dir(),
        prompt: prompt.into(),
        model: None,
        effort: None,
        mcp_servers: vec![],
        env: BTreeMap::new(),
        env_remove: Vec::new(),
        tier_ceiling: ToolTier::Write,
        output_schema: None,
        skills: vec![],
    }
}

#[tokio::test]
async fn mock_session_streams_events_to_terminal_end() {
    let adapter = MockAdapter::default();
    let session = adapter.launch(spec("do the thing")).await.unwrap();
    let mut events = session.subscribe();
    session.prompt("do the thing".into()).await.unwrap();

    let mut saw_started = false;
    let mut saw_text = false;
    let mut terminal: Option<Outcome> = None;
    while let Ok(Some(ev)) = tokio::time::timeout(Duration::from_secs(5), events.next()).await {
        match ev {
            SessionEvent::Lifecycle(LifecycleEvent::Started) => saw_started = true,
            SessionEvent::Progress(bisa_harness::ProgressEvent::TextDelta { text }) => {
                assert!(text.contains("do the thing"));
                saw_text = true;
            }
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome,
                is_terminal,
            }) => {
                assert!(is_terminal);
                terminal = Some(outcome);
                break;
            }
            _ => {}
        }
    }
    assert!(saw_started && saw_text);
    assert!(matches!(terminal, Some(Outcome::Completed)));
    assert_eq!(session.phase(), bisa_harness::Phase::Ended);
    assert!(
        session.snapshot().revision > 1,
        "snapshot revision advanced"
    );
}

#[tokio::test]
async fn steer_without_capability_is_not_supported() {
    let adapter = MockAdapter {
        caps: HarnessCaps::empty(),
        available: true,
        ..Default::default()
    };
    let session = adapter.launch(spec("x")).await.unwrap();
    match session.steer("go left".into()).await {
        Err(HarnessError::NotSupported("steer")) => {}
        other => panic!("expected NotSupported(steer), got {other:?}"),
    }
    match session.resume_token() {
        None => {}
        Some(t) => panic!("no RESUME cap but token {t:?}"),
    }
}

#[tokio::test]
async fn unavailable_adapter_asks_for_fallback() {
    let adapter = MockAdapter {
        available: false,
        ..Default::default()
    };
    let err = adapter.launch(spec("x")).await.err().expect("must fail");
    assert!(err.is_unavailable());
}

#[tokio::test]
async fn catalog_loads_custom_descriptors_and_strips_reserved_env() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("my-agent.json"),
        serde_json::json!({
            "id": "my-agent",
            "label": "My Agent",
            "command": "my-agent-bin",
            "args": ["acp"],
            "env": {"BISA_PRIVATE_KEY": "steal", "MY_MODE": "acp"},
            "install_hint": "https://example.invalid"
        })
        .to_string(),
    )
    .unwrap();
    // Reserved id: must be reported as an error, not loaded.
    std::fs::write(
        dir.path().join("evil.json"),
        serde_json::json!({"id": "claude-code", "label": "E", "command": "e"}).to_string(),
    )
    .unwrap();
    // Garbage file: must not abort discovery.
    std::fs::write(dir.path().join("broken.json"), "{not json").unwrap();

    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::new(MockAdapter::default()));
    let errors = catalog.load_custom_dir(dir.path());
    assert_eq!(errors.len(), 2, "reserved id + broken file: {errors:?}");

    let specs = catalog.custom_specs();
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].id, "my-agent");
    assert!(
        !specs[0].env.contains_key("BISA_PRIVATE_KEY"),
        "reserved env stripped"
    );
    assert_eq!(specs[0].env.get("MY_MODE"), Some(&"acp".to_string()));

    let listings = catalog.list().await;
    let builtin = listings
        .iter()
        .find(|l| l.id == "mock")
        .expect("builtin listed");
    assert_eq!(builtin.tier, HarnessTier::Builtin);
    assert!(builtin.probe.available);
    assert!(listings.iter().any(|l| l.id == "custom:my-agent"));
    assert!(listings.iter().any(|l| l.tier == HarnessTier::Preset));

    // A custom descriptor's command *is* its interactive form, args and all.
    let custom = listings
        .iter()
        .find(|l| l.id == "custom:my-agent")
        .expect("custom listed");
    let launch = custom.launch.as_ref().expect("a custom row can be run");
    assert_eq!(launch.program, "my-agent-bin");
    assert_eq!(launch.args, vec!["acp".to_string()]);

    // Missing custom dir is silently empty, not an error.
    let mut empty = HarnessCatalog::new();
    assert!(empty
        .load_custom_dir(&dir.path().join("does-not-exist"))
        .is_empty());
}

/// A harness with no interactive form says so, rather than offering the command
/// it uses to speak a protocol.
///
/// This is the default on the trait, and the default is the point: an adapter
/// author who does nothing gets `None`, which shows a person no menu entry.
/// Getting it wrong in the other direction — offering `goose acp` as something
/// to sit in front of — hands somebody a terminal full of JSON-RPC frames.
#[test]
fn a_harness_with_no_interactive_form_offers_none() {
    let mock = MockAdapter::default();
    assert!(
        mock.interactive().is_none(),
        "the trait default must be None, so silence is safe"
    );
}

/// `list` and `list_with_timeout` differ only in how they probe **adapters**.
///
/// They used to carry a verbatim copy of the preset and custom loops each,
/// which is how a listing comes to disagree with itself depending on which
/// caller asked — the CLI reads one, the node reads the other.
#[tokio::test]
async fn both_listings_agree_about_everything_that_is_not_an_adapter() {
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::new(MockAdapter::default()));

    let slow = catalog.list().await;
    let fast = catalog
        .list_with_timeout(std::time::Duration::from_secs(3))
        .await;

    let rows = |ls: &[bisa_harness::HarnessListing]| -> Vec<String> {
        ls.iter()
            .filter(|l| l.tier != HarnessTier::Builtin)
            .map(|l| {
                format!(
                    "{}|{}|{:?}|{:?}|{:?}",
                    l.id, l.label, l.tier, l.launch, l.install_hint
                )
            })
            .collect()
    };
    assert_eq!(rows(&slow), rows(&fast));
    assert!(!rows(&slow).is_empty(), "there is something to compare");
}

/// A listing in which a probe outstayed its budget is answered — the row
/// unavailable, saying so — but not kept: the next ask probes again, so a
/// cold `--version` on a first launch is not "not installed" for the TTL.
/// A listing whose probes all answered is kept as before.
#[tokio::test]
async fn a_listing_with_a_timed_out_probe_is_answered_but_not_cached() {
    let ttl = Duration::from_secs(60);
    let slow = Arc::new(MockAdapter {
        id: "slow".into(),
        probe_delay: Duration::from_millis(300),
        ..MockAdapter::default()
    });
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::clone(&slow) as Arc<dyn bisa_harness::HarnessAdapter>);
    let first = catalog.list_cached(ttl, Duration::from_millis(50)).await;
    let row = first
        .iter()
        .find(|l| l.id == "slow")
        .expect("the slow adapter is listed");
    assert!(
        !row.probe.available,
        "a probe that outstayed its budget is unavailable"
    );
    assert!(
        row.probe
            .reason
            .as_deref()
            .is_some_and(|r| r.starts_with("probe timed out after")),
        "and says so: {:?}",
        row.probe.reason
    );
    catalog.list_cached(ttl, Duration::from_millis(50)).await;
    assert_eq!(
        slow.probes.load(std::sync::atomic::Ordering::SeqCst),
        2,
        "the next ask probes again"
    );

    let quick = Arc::new(MockAdapter {
        id: "quick".into(),
        ..MockAdapter::default()
    });
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::clone(&quick) as Arc<dyn bisa_harness::HarnessAdapter>);
    catalog.list_cached(ttl, Duration::from_secs(3)).await;
    catalog.list_cached(ttl, Duration::from_secs(3)).await;
    assert_eq!(
        quick.probes.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "a listing whose probes answered is kept"
    );
}

/// **A preset may never shadow a compiled-in adapter.**
///
/// Tier 2 is defined as *a harness we can detect but ship no first-class
/// adapter for*. `preset:omp` and `preset:opencode` broke that: both had
/// adapters, so `harness list` showed omp three times and two of the three
/// could not run — a preset has no adapter object, so `get()` answers `None`
/// for it. The rule is cheap to state and was expensive to notice.
#[test]
fn a_preset_never_shadows_an_adapter() {
    use bisa_harness::catalog::{BUILTIN_IDS, PRESET_HARNESSES};
    for preset in PRESET_HARNESSES {
        assert!(
            !BUILTIN_IDS.contains(&preset.command),
            "{} duplicates the compiled-in adapter {:?}; tier 2 is for harnesses \
             with no adapter",
            preset.id,
            preset.command
        );
    }
}

#[tokio::test]
async fn proc_roundtrips_ndjson_lines() {
    // `cat` echoes stdin to stdout: whatever JSON we write comes back framed.
    let mut proc = ProcHandle::spawn(ProcSpec::new("cat")).expect("spawn cat");
    proc.send_json(&serde_json::json!({"type": "prompt", "n": 1}))
        .await
        .unwrap();
    match proc
        .recv_timeout(Duration::from_secs(5))
        .await
        .expect("no timeout")
    {
        Some(Line::Json(v)) => assert_eq!(v["type"], "prompt"),
        other => panic!("expected json line, got {other:?}"),
    }

    proc.send_line("plain text line").await.unwrap();
    match proc
        .recv_timeout(Duration::from_secs(5))
        .await
        .expect("no timeout")
    {
        Some(Line::Text(t)) => assert_eq!(t, "plain text line"),
        other => panic!("expected text line, got {other:?}"),
    }
    assert!(proc.idle_for() < Duration::from_secs(5));

    // EOF ends the process; stream closes.
    proc.close_stdin().await.unwrap();
    match proc
        .recv_timeout(Duration::from_secs(5))
        .await
        .expect("no timeout")
    {
        None => {}
        Some(l) => panic!("expected closed stream, got {l:?}"),
    }
    let status = proc.wait().await.unwrap();
    assert!(status.success());
}

#[tokio::test]
async fn proc_spawn_missing_binary_is_unavailable() {
    let err = ProcHandle::spawn(ProcSpec::new("definitely-not-a-real-binary-xyz"))
        .err()
        .expect("must fail");
    assert!(err.is_unavailable(), "{err:?}");
}

// ---------------------------------------------------------------------------
// A harness's process group: a stop reaches what the harness started.

/// A pid as the kernel's.
#[cfg(unix)]
fn kernel_pid(pid: u32) -> rustix::process::Pid {
    rustix::process::Pid::from_raw(i32::try_from(pid).expect("a pid fits"))
        .expect("a pid is not zero")
}

/// Whether a process with this pid exists — a zombie not yet reaped counts.
#[cfg(unix)]
fn exists(pid: u32) -> bool {
    rustix::process::test_kill_process(kernel_pid(pid)).is_ok()
}

/// Wait for the process to be gone, reaped included, or give up.
#[cfg(unix)]
async fn gone_within(pid: u32, within: Duration) -> bool {
    let until = tokio::time::Instant::now() + within;
    while exists(pid) {
        if tokio::time::Instant::now() >= until {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    true
}

/// The next two lines a child prints, as pids.
#[cfg(unix)]
async fn two_pids(proc: &mut ProcHandle) -> (u32, u32) {
    let mut pids = Vec::new();
    while pids.len() < 2 {
        let line = tokio::time::timeout(Duration::from_secs(5), proc.recv())
            .await
            .expect("a pid line before the deadline")
            .expect("the child is still talking");
        if let Line::Text(text) = line {
            pids.push(text.trim().parse::<u32>().expect("a pid"));
        }
    }
    (pids[0], pids[1])
}

/// A shell that starts a grandchild, says both pids, then becomes a sleeper.
#[cfg(unix)]
fn a_child_with_a_grandchild() -> ProcSpec {
    ProcSpec::new("sh")
        .arg("-c")
        .arg("sleep 300 & echo $!; echo $$; exec sleep 300")
}

#[cfg(unix)]
#[tokio::test]
async fn a_spawned_harness_leads_a_group_of_its_own() {
    let mut proc = ProcHandle::spawn(ProcSpec::new("sleep").arg("300")).expect("spawn sleep");
    let pid = proc.pid().expect("a running child has a pid");
    assert_eq!(
        rustix::process::getpgid(Some(kernel_pid(pid))).expect("a group"),
        kernel_pid(pid),
        "the child is the leader of its own group"
    );
    assert!(proc.group().alive());
    proc.kill().await.expect("killed");
    assert!(
        !proc.group().alive(),
        "a killed and reaped group is nobody's"
    );
    assert!(gone_within(pid, Duration::from_secs(2)).await);
}

#[cfg(unix)]
#[tokio::test]
async fn terminate_ends_a_child_and_its_grandchild() {
    let mut proc = ProcHandle::spawn(a_child_with_a_grandchild()).expect("spawn sh");
    let (grandchild, child) = two_pids(&mut proc).await;
    assert_eq!(proc.pid(), Some(child));
    assert!(exists(grandchild) && exists(child));
    let status = tokio::time::timeout(
        Duration::from_secs(5),
        proc.terminate(Duration::from_millis(500)),
    )
    .await
    .expect("terminate answers before the deadline")
    .expect("the child is reaped");
    assert!(
        !status.success(),
        "a terminated child did not succeed: {status}"
    );
    assert!(
        gone_within(child, Duration::from_secs(2)).await,
        "the child is gone"
    );
    assert!(
        gone_within(grandchild, Duration::from_secs(2)).await,
        "the grandchild went with the group — the one pid alone would have orphaned it"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_child_that_ignores_eof_and_term_is_killed_at_the_grace() {
    // The ignored signal survives the `exec`: the sleeper ignores `SIGTERM`
    // and never reads stdin — a harness that will not leave when asked.
    let mut proc = ProcHandle::spawn(
        ProcSpec::new("sh")
            .arg("-c")
            .arg("trap '' TERM; echo $$; echo $$; exec sleep 300"),
    )
    .expect("spawn sh");
    let (pid, _) = two_pids(&mut proc).await;
    proc.close_stdin().await.expect("stdin closed");
    let started = std::time::Instant::now();
    let status = tokio::time::timeout(
        Duration::from_secs(5),
        proc.terminate(Duration::from_millis(300)),
    )
    .await
    .expect("terminate answers before the deadline")
    .expect("the child is reaped");
    assert!(
        started.elapsed() >= Duration::from_millis(300),
        "the grace was given before the kill"
    );
    assert!(!status.success());
    assert!(
        gone_within(pid, Duration::from_secs(2)).await,
        "killed past the grace"
    );
}

/// An abort gives the group the EOF first: a child that leaves on it is
/// gone before any signal, on its own terms; one that ignores EOF and
/// `SIGTERM` alike is killed within the same grace.
#[cfg(unix)]
#[tokio::test]
async fn an_abort_lets_a_child_leave_on_eof_and_kills_one_that_ignores_it_within_the_grace() {
    // `cat` leaves the moment its input ends.
    let mut leaver = ProcHandle::spawn(
        ProcSpec::new("sh")
            .arg("-c")
            .arg("echo $$; echo $$; exec cat"),
    )
    .expect("spawn sh");
    let (pid, _) = two_pids(&mut leaver).await;
    leaver.close_stdin().await.expect("stdin closed");
    let group = leaver.group();
    let started = std::time::Instant::now();
    assert!(
        group.leave_or_terminate(Duration::from_secs(4)).await,
        "left on EOF"
    );
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "and well within the first half of the grace"
    );
    // Reaped by whoever holds the child — here the test — then gone for good.
    let status = tokio::time::timeout(Duration::from_secs(2), leaver.wait())
        .await
        .expect("reaped in time")
        .expect("a status");
    assert!(status.success(), "cat left cleanly on EOF: {status}");
    assert!(gone_within(pid, Duration::from_secs(1)).await);

    // The sleeper ignores `SIGTERM` and never reads stdin.
    let mut stayer = ProcHandle::spawn(
        ProcSpec::new("sh")
            .arg("-c")
            .arg("trap '' TERM; echo $$; echo $$; exec sleep 300"),
    )
    .expect("spawn sh");
    let (pid, _) = two_pids(&mut stayer).await;
    stayer.close_stdin().await.expect("stdin closed");
    let group = stayer.group();
    let started = std::time::Instant::now();
    let left = tokio::time::timeout(
        Duration::from_secs(5),
        group.leave_or_terminate(Duration::from_millis(600)),
    )
    .await
    .expect("answers before the deadline");
    assert!(!left, "it did not leave on its own");
    assert!(
        started.elapsed() >= Duration::from_millis(600),
        "both halves of the grace were given before the kill"
    );
    let status = tokio::time::timeout(Duration::from_secs(2), stayer.wait())
        .await
        .expect("reaped in time")
        .expect("a status");
    assert!(!status.success(), "killed, not left: {status}");
    assert!(
        gone_within(pid, Duration::from_secs(2)).await,
        "killed past the grace"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_dropped_handle_takes_its_running_childs_group_with_it() {
    let mut proc = ProcHandle::spawn(a_child_with_a_grandchild()).expect("spawn sh");
    let (grandchild, child) = two_pids(&mut proc).await;
    drop(proc);
    assert!(
        gone_within(child, Duration::from_secs(3)).await,
        "tokio kills the leader on drop"
    );
    assert!(
        gone_within(grandchild, Duration::from_secs(3)).await,
        "the handle's drop takes the rest of the group"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_timed_out_command_takes_what_it_started_with_it() {
    let dir = tempfile::tempdir().expect("a tempdir");
    let file = dir.path().join("grandchild.pid");
    let mut cmd = tokio::process::Command::new("sh");
    cmd.arg("-c")
        .arg(format!("sleep 300 & echo $! > '{}'; wait", file.display()));
    let run = tokio::time::timeout(
        Duration::from_millis(500),
        bisa_harness::proc::group_output(cmd),
    )
    .await;
    assert!(
        run.is_err(),
        "the command was still waiting at the deadline"
    );
    let mut grandchild = None;
    for _ in 0..40 {
        if let Ok(text) = std::fs::read_to_string(&file) {
            if let Ok(pid) = text.trim().parse::<u32>() {
                grandchild = Some(pid);
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let grandchild = grandchild.expect("the shell wrote its child's pid");
    assert!(
        gone_within(grandchild, Duration::from_secs(2)).await,
        "the timed-out command's child went with its group"
    );
}
