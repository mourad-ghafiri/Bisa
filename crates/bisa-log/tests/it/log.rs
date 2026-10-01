//! The file layer on a temporary folder: what lands in the file, at which
//! level, how a change of configuration moves it, what the first open does
//! — the sweep, the marker, the replay — and what a death leaves behind.
//! Under a thread-local subscriber, so the global one is never set and the
//! tests never touch a real workspace; the one test of the process-wide
//! hook installs the global subscriber, which a thread-local one overrides
//! everywhere else.
//!
//! Nothing here deletes anything: the tempdir is dropped, never removed by
//! hand.

use std::path::Path;

use bisa_log::{
    build, install, is_crash_name, is_log_name, list, read_crash, runs_dir, ChildExit, CrashKind,
    CrashReport, LogConfig, LogLevel, LogRotation, Marker, Process, ReportError,
};
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

/// Every JSON line of every family file under the root, as it was written.
fn lines(root: &Path) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for family in list(root).unwrap().families {
        for file in family.files {
            let text = std::fs::read_to_string(family.dir.join(&file.name)).unwrap();
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                out.push(serde_json::from_str(line).unwrap());
            }
        }
    }
    out
}

fn messages(root: &Path) -> Vec<String> {
    lines(root)
        .into_iter()
        .filter_map(|v| {
            v.get("message")
                .and_then(|m| m.as_str())
                .map(str::to_string)
        })
        .collect()
}

fn family_files(root: &Path, process: Process) -> Vec<String> {
    list(root)
        .unwrap()
        .families
        .into_iter()
        .find(|f| f.process == process)
        .map(|f| f.files.into_iter().map(|f| f.name).collect())
        .unwrap_or_default()
}

/// A pid nobody has: the largest a `kill` accepts, which no system hands
/// out.
const DEAD: u32 = i32::MAX as u32 - 1;

#[test]
fn a_line_lands_at_or_above_the_level_and_a_change_of_level_lets_more_through() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("logs");
    let (handle, layers) = build(Process::Node, "0.0.0-test");
    let _guard = tracing_subscriber::registry().with(layers).set_default();

    // Nothing is attached: nothing is written and no folder is made.
    tracing::error!(target: "it", "before attach");
    assert!(!root.exists());
    assert!(!handle.writing());

    handle
        .attach(
            &root,
            LogConfig {
                level: LogLevel::Warn,
                ..LogConfig::default()
            },
        )
        .unwrap();
    assert!(handle.writing());
    tracing::error!(target: "it", answer = 42, "an error");
    tracing::warn!(target: "it", "a warning");
    tracing::info!(target: "it", "an info line");
    tracing::debug!(target: "it", "a debug line");

    let got = messages(&root);
    assert!(got.contains(&"an error".to_string()));
    assert!(got.contains(&"a warning".to_string()));
    assert!(!got.contains(&"an info line".to_string()));
    assert!(!got.contains(&"a debug line".to_string()));
    // The *log started* line is at info, under the level here.
    assert!(!got.contains(&"log started".to_string()));

    // The event's fields are flattened beside the message, with the level
    // and the target the file layer adds.
    let error = lines(&root)
        .into_iter()
        .find(|v| v["message"] == "an error")
        .unwrap();
    assert_eq!(error["answer"], 42);
    assert_eq!(error["level"], "ERROR");
    assert_eq!(error["target"], "it");
    assert!(error["timestamp"].is_string());

    // The level moves in place: the same file, a debug line now lands, and
    // the *log started* line says the level it was attached at and where
    // the reports go.
    handle
        .apply(LogConfig {
            level: LogLevel::Debug,
            ..LogConfig::default()
        })
        .unwrap();
    tracing::debug!(target: "it", "a later debug line");
    let got = messages(&root);
    assert!(got.contains(&"a later debug line".to_string()));
    let started = lines(&root)
        .into_iter()
        .find(|v| v["message"] == "log started")
        .unwrap();
    assert_eq!(started["level"], "INFO");
    assert_eq!(started["process"], "node");
    assert_eq!(started["version"], "0.0.0-test");
    assert_eq!(started["min_level"], "debug");
    assert_eq!(started["rotation"], "daily");
    assert_eq!(started["keep_files"], 14);
    assert_eq!(
        started["crashes"],
        root.join("crashes").display().to_string()
    );

    // One file family under its own folder, named for the process and the
    // day; the other families are listed with nothing.
    let listing = list(&root).unwrap();
    assert_eq!(listing.families.len(), Process::ALL.len());
    let node = &listing.families[0];
    assert_eq!(node.process, Process::Node);
    assert_eq!(node.dir, root.join("node"));
    assert_eq!(node.files.len(), 1);
    assert!(node.files[0].name.starts_with("node."));
    assert!(node.files[0].name.ends_with(".jsonl"));
    assert!(node.files[0].bytes > 0);
    assert!(listing.families[1..].iter().all(|f| f.files.is_empty()));
    assert!(listing.crashes.is_empty());
    assert_eq!(listing.bytes(), node.files[0].bytes);
    assert_eq!(handle.root().as_deref(), Some(root.as_path()));
    assert_eq!(handle.config().level, LogLevel::Debug);
}

#[test]
fn off_writes_nothing_and_on_again_writes_to_the_same_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("logs");
    let (handle, layers) = build(Process::Cli, "0.0.0-test");
    let _guard = tracing_subscriber::registry().with(layers).set_default();

    handle
        .attach(
            &root,
            LogConfig {
                enabled: false,
                ..LogConfig::default()
            },
        )
        .unwrap();
    tracing::error!(target: "it", "while off");
    assert!(
        family_files(&root, Process::Cli).is_empty(),
        "off opens no file"
    );
    assert!(!handle.writing());
    assert!(!runs_dir(&root).exists(), "off writes no marker either");
    let refused = handle
        .report_crash(CrashReport::new(CrashKind::ChildExit, "while off"))
        .unwrap_err();
    assert!(matches!(refused, ReportError::Off), "{refused}");

    handle
        .apply(LogConfig {
            enabled: true,
            level: LogLevel::Error,
            rotation: LogRotation::Hourly,
            keep_files: 3,
        })
        .unwrap();
    assert!(handle.writing());
    tracing::error!(target: "it", "while on");
    let got = messages(&root);
    assert!(got.contains(&"while on".to_string()));
    assert!(!got.contains(&"while off".to_string()));
    let files = family_files(&root, Process::Cli);
    assert_eq!(files.len(), 1);
    // Hourly names carry the hour: `cli.YYYY-MM-DD-HH.jsonl`.
    let period = files[0]
        .trim_start_matches("cli.")
        .trim_end_matches(".jsonl");
    assert_eq!(period.split('-').count(), 4, "{period} is an hourly period");
}

#[test]
fn a_folder_whose_parent_is_missing_is_remembered_and_nothing_is_made() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("not-a-workspace").join("logs");
    let (handle, layers) = build(Process::Mcp, "0.0.0-test");
    let _guard = tracing_subscriber::registry().with(layers).set_default();
    handle.attach(&root, LogConfig::default()).unwrap();
    tracing::error!(target: "it", "into nothing");
    assert!(!root.parent().unwrap().exists());
    assert!(!root.exists());
    assert!(!handle.writing());
    assert_eq!(handle.root().as_deref(), Some(root.as_path()));
    let refused = handle
        .report_crash(CrashReport::new(CrashKind::ChildExit, "into nothing"))
        .unwrap_err();
    // The root is remembered, so the report is refused by the write, not
    // for want of a folder — and the workspace stays unmade.
    assert!(matches!(refused, ReportError::Io(_)), "{refused}");
    assert!(!root.exists());
}

#[test]
fn the_listing_groups_the_families_newest_first_and_ignores_a_stranger() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    for (folder, name) in [
        ("node", "node.2026-09-07.jsonl"),
        ("node", "node.2026-09-08.jsonl"),
        ("node", "notes.txt"),
        ("node", "desktop.2026-09-08.jsonl"),
        ("desktop", "desktop.2026-09-08.jsonl"),
        ("crashes", "node.20260911T102233Z.12.json"),
        ("crashes", "cli.20260911T102234Z.13.json"),
        ("crashes", "stranger.20260911T102233Z.12.json"),
        ("runs", "node.12.json"),
    ] {
        std::fs::create_dir_all(root.join(folder)).unwrap();
        std::fs::write(root.join(folder).join(name), format!("{name}\n")).unwrap();
    }
    std::fs::write(root.join("stranger.2026-09-08.jsonl"), "x\n").unwrap();

    let listing = list(root).unwrap();
    let node = family_files(root, Process::Node);
    assert_eq!(node.len(), 2, "{node:?}");
    assert!(node
        .iter()
        .all(|n| n.starts_with("node.") && is_log_name(n)));
    assert_eq!(
        family_files(root, Process::Desktop),
        vec!["desktop.2026-09-08.jsonl".to_string()]
    );
    assert!(family_files(root, Process::Cli).is_empty());
    let crashes: Vec<&str> = listing.crashes.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(crashes.len(), 2);
    assert!(crashes.iter().all(|n| is_crash_name(n)));
    assert!(!crashes.contains(&"stranger.20260911T102233Z.12.json"));
    for family in &listing.families {
        for pair in family.files.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            assert!(
                a.modified_at > b.modified_at
                    || (a.modified_at == b.modified_at && a.name > b.name),
                "newest first, then by name"
            );
        }
    }
    for f in listing.files() {
        assert_eq!(f.bytes, (f.name.len() + 1) as u64);
    }
    let nowhere = list(&root.join("nowhere")).unwrap();
    assert!(nowhere.families.iter().all(|f| f.files.is_empty()));
    assert!(nowhere.crashes.is_empty());
    assert_eq!(nowhere.bytes(), 0);
}

#[test]
fn the_first_open_sweeps_a_stale_marker_writes_this_runs_and_replays_what_was_said_before() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("logs");
    // A previous node run that never said goodbye, and one of another
    // family that is not this process's to judge.
    std::fs::create_dir_all(runs_dir(&root)).unwrap();
    let stale = Marker {
        process: Process::Node,
        version: "0.0.0-before".to_string(),
        pid: DEAD,
        started_at: "2026-09-11T09:00:00Z".to_string(),
    };
    std::fs::write(
        runs_dir(&root).join(stale.file_name()),
        serde_json::to_vec(&stale).unwrap(),
    )
    .unwrap();
    let other = Marker {
        process: Process::Cli,
        ..stale.clone()
    };
    std::fs::write(
        runs_dir(&root).join(other.file_name()),
        serde_json::to_vec(&other).unwrap(),
    )
    .unwrap();

    let (handle, layers) = build(Process::Node, "0.0.0-test");
    let _guard = tracing_subscriber::registry().with(layers).set_default();

    // Said before there was a file: the recorder has them.
    tracing::warn!(target: "early", port = 4477, "the node is not up yet");
    tracing::debug!(target: "early", "a debug step");
    assert_eq!(handle.recent().len(), 2);

    handle
        .attach(
            &root,
            LogConfig {
                level: LogLevel::Warn,
                ..LogConfig::default()
            },
        )
        .unwrap();

    let all = lines(&root);
    // The replay: the warning, marked, with its field as its words; the
    // debug line was under the level.
    let replayed = all
        .iter()
        .find(|v| v["message"] == "the node is not up yet")
        .expect("the early warning was replayed");
    assert_eq!(replayed["replayed"], true);
    assert_eq!(replayed["level"], "WARN");
    assert_eq!(replayed["target"], "early");
    assert_eq!(replayed["port"], "4477");
    assert!(all.iter().all(|v| v["message"] != "a debug step"));
    // It sits above what followed.
    let index = |message: &str| all.iter().position(|v| v["message"] == message).unwrap();
    let goodbye = all
        .iter()
        .find(|v| {
            v["message"]
                .as_str()
                .is_some_and(|m| m.contains("ended without a goodbye"))
        })
        .expect("the stale marker was reported");
    assert!(index("the node is not up yet") < all.iter().position(|v| v == goodbye).unwrap());

    // The sweep: an error line naming the report, the report on disk with
    // the marker's facts, the stale marker gone, the other family's left.
    assert_eq!(goodbye["level"], "ERROR");
    assert_eq!(goodbye["pid"], DEAD);
    assert_eq!(goodbye["version"], "0.0.0-before");
    let crash_name = goodbye["crash"].as_str().unwrap().to_string();
    assert!(is_crash_name(&crash_name), "{crash_name}");
    let report = read_crash(&root, &crash_name).unwrap();
    assert_eq!(report.kind, CrashKind::AbruptEnd);
    assert_eq!(report.process, Process::Node);
    assert_eq!(report.pid, DEAD);
    assert_eq!(report.version, "0.0.0-before");
    assert!(!runs_dir(&root).join(stale.file_name()).exists());
    assert!(runs_dir(&root).join(other.file_name()).exists());

    // This run's marker, for the goodbye.
    let own = Marker {
        process: Process::Node,
        version: "0.0.0-test".to_string(),
        pid: std::process::id(),
        started_at: String::new(),
    };
    let own_path = runs_dir(&root).join(own.file_name());
    let written: Marker =
        serde_json::from_str(&std::fs::read_to_string(&own_path).unwrap()).unwrap();
    assert_eq!(written.pid, std::process::id());
    assert_eq!(written.version, "0.0.0-test");
    assert!(written.started_at.ends_with('Z'));

    // A second attach — the settings moved — neither replays nor sweeps
    // again: one replayed line, one report, one marker.
    handle
        .apply(LogConfig {
            level: LogLevel::Debug,
            ..LogConfig::default()
        })
        .unwrap();
    let all = lines(&root);
    assert_eq!(all.iter().filter(|v| v["replayed"] == true).count(), 1);
    assert_eq!(list(&root).unwrap().crashes.len(), 1);
    assert_eq!(
        std::fs::read_dir(runs_dir(&root)).unwrap().count(),
        2,
        "this run's and the other family's"
    );

    // The goodbye: one line with the reason, the marker gone, and a second
    // goodbye says nothing more.
    handle.goodbye("finished");
    handle.goodbye("finished again");
    let all = lines(&root);
    let stopped: Vec<&serde_json::Value> = all
        .iter()
        .filter(|v| v["message"] == "log stopped")
        .collect();
    assert_eq!(stopped.len(), 1);
    assert_eq!(stopped[0]["reason"], "finished");
    assert!(stopped[0]["uptime_secs"].is_number());
    assert!(!own_path.exists(), "the goodbye took the marker");
    assert!(runs_dir(&root).join(other.file_name()).exists());
}

#[test]
fn a_crash_report_carries_the_recorders_entries_and_the_childs_last_words() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("logs");
    let (handle, layers) = build(Process::Desktop, "0.0.0-test");
    let _guard = tracing_subscriber::registry().with(layers).set_default();

    // No folder yet: nothing to write into.
    let refused = handle
        .report_crash(CrashReport::new(CrashKind::ChildExit, "too early"))
        .unwrap_err();
    assert!(matches!(refused, ReportError::NoFolder), "{refused}");

    handle.attach(&root, LogConfig::default()).unwrap();
    tracing::debug!(target: "it", attempt = 2, "restarting the node");
    tracing::trace!(target: "it", "never recorded");
    let path = handle
        .report_crash(
            CrashReport::new(CrashKind::ChildExit, "the node exited on its own").with_child(
                ChildExit {
                    process: Process::Node,
                    pid: 4242,
                    code: None,
                    signal: Some(11),
                    stderr: vec!["thread 'main' has overflowed its stack".to_string()],
                },
            ),
        )
        .unwrap();
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    assert!(is_crash_name(&name), "{name}");
    assert!(name.starts_with("desktop."));
    let report = read_crash(&root, &name).unwrap();
    assert_eq!(report.kind, CrashKind::ChildExit);
    assert_eq!(report.process, Process::Desktop);
    assert_eq!(report.version, "0.0.0-test");
    assert_eq!(report.pid, std::process::id());
    assert!(report.at.ends_with('Z'));
    let child = report.child.unwrap();
    assert_eq!(child.pid, 4242);
    assert_eq!(child.signal, Some(11));
    assert_eq!(child.stderr[0], "thread 'main' has overflowed its stack");
    // The recorder's entries, the debug one included, the trace one not.
    assert!(report
        .recent
        .iter()
        .any(|r| r.message == "restarting the node" && r.level == "DEBUG"));
    assert!(report.recent.iter().all(|r| r.message != "never recorded"));
    assert!(report
        .recent
        .iter()
        .any(|r| r.message == "log started" && r.level == "INFO"));
    assert_eq!(list(&root).unwrap().crashes[0].name, name);
    assert_eq!(bisa_log::latest_crash(&root).unwrap().0, name);
}

/// The one test of the process-wide subscriber and its hook. A thread-local
/// subscriber overrides the global one everywhere else in this binary, and
/// the folder is a tempdir, so nothing reaches a real workspace.
#[test]
fn the_panic_hook_writes_a_report_with_a_backtrace_and_a_line_naming_it() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("logs");
    let handle = install(Process::Cli, "0.0.0-test");
    handle.attach(&root, LogConfig::default()).unwrap();
    tracing::debug!(target: "it", "just before");

    let died = std::thread::Builder::new()
        .name("the-doomed-thread".to_string())
        .spawn(|| panic!("one bad row"))
        .unwrap()
        .join();
    assert!(died.is_err());

    // The hook is the process's, so another test's panic may land here too:
    // the report is found by its thread, not by being the only one.
    let listing = list(&root).unwrap();
    let (name, report) = listing
        .crashes
        .iter()
        .filter_map(|f| read_crash(&root, &f.name).map(|r| (f.name.clone(), r)))
        .find(|(_, r)| r.thread.as_deref() == Some("the-doomed-thread"))
        .expect("a report for the doomed thread");
    assert_eq!(report.kind, CrashKind::Panic);
    assert_eq!(report.message, "one bad row");
    assert_eq!(report.process, Process::Cli);
    assert_eq!(report.thread.as_deref(), Some("the-doomed-thread"));
    assert!(report
        .location
        .as_deref()
        .unwrap_or_default()
        .contains("log.rs"));
    assert!(!report.backtrace.as_deref().unwrap_or_default().is_empty());
    assert!(report.recent.iter().any(|r| r.message == "just before"));
    assert!(report
        .recent
        .iter()
        .any(|r| r.message == "one bad row" && r.target == "panic"));

    let line = lines(&root)
        .into_iter()
        .find(|v| v["target"] == "panic")
        .expect("the panic line");
    assert_eq!(line["level"], "ERROR");
    assert_eq!(line["message"], "one bad row");
    assert_eq!(line["thread"], "the-doomed-thread");
    assert_eq!(line["crash"], name);
    assert!(line["location"].as_str().unwrap().contains("log.rs"));
}
