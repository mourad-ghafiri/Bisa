//! What a crash costs the store: nothing acknowledged. The index is a cache
//! that heals itself — a damaged file is rebuilt at open, a projection a
//! crash left behind its snapshot is reconciled at open — the signal queue's
//! truth log is written before the working set and carries the count, and a
//! session has an end the next boot can give it. Every scenario here is a
//! file on a temporary directory; nothing is killed, and nothing of the
//! machine's is touched.

use bisa_core::{
    Chain, Flow, Home, Join, ListenerHost, ListenerKey, OnFail, RunScope, Signal, SignalScope,
    SignalSource, Step, StepId, StepKind, Tags, ValueRef, WaitFor, WorkflowOrigin,
    DEFAULT_MAX_VISITS,
};
use bisa_store::{
    FileKeyStore, MemoryKeyStore, NewGoal, NewWorkflow, Paths, SessionRow, SessionStatus,
    SignalState, Workspace,
};
use std::collections::BTreeMap;
use std::io::{Seek, SeekFrom, Write};

fn ws() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    (dir, ws)
}

fn file_ws(dir: &tempfile::TempDir) -> Workspace {
    Workspace::open_with_keystore(
        dir.path(),
        Box::new(FileKeyStore::new(Paths::new(dir.path()).identity_dir())),
    )
    .unwrap()
}

fn sid(s: &str) -> StepId {
    StepId::new(s).unwrap()
}

fn step(id: &str, kind: StepKind, then: &[&str]) -> Step {
    Step {
        id: sid(id),
        name: id.into(),
        kind,
        then: then.iter().map(|t| Flow::to(sid(t))).collect(),
        boundaries: vec![],
        join: Join::All,
        on_fail: OnFail::Fail,
        retries: 0,
        max_visits: DEFAULT_MAX_VISITS,
        position: None,
    }
}

/// A workflow that waits a minute, then ends — a run of it holds one armed wait.
fn waiting_workflow(name: &str) -> NewWorkflow {
    NewWorkflow {
        name: name.into(),
        description: String::new(),
        inputs: vec![],
        steps: vec![
            step(
                "hold",
                StepKind::Wait {
                    until: WaitFor::Delay {
                        secs: ValueRef::Fixed(60),
                    },
                },
                &["end"],
            ),
            step(
                "end",
                StepKind::End {
                    finish: bisa_core::Finish::Done,
                },
                &[],
            ),
        ],
        tags: Tags::default(),
        decision_making: false,
    }
}

#[test]
fn a_damaged_index_at_the_right_version_is_rebuilt_at_open_and_every_fact_reads_back() {
    let dir = tempfile::tempdir().unwrap();
    let index = Paths::new(dir.path()).index_db();
    let goals: Vec<_> = {
        let ws = file_ws(&dir);
        // Enough goals to spill past the first page, so damage lands in data.
        (0..40)
            .map(|i| {
                ws.create_goal(NewGoal::captured(&format!(
                    "goal number {i} with a longer statement to fill pages"
                )))
                .unwrap()
                .id
            })
            .collect()
    };
    // The connection closed with the workspace, so the WAL is checkpointed
    // into the file; the header (and its version stamp) stays intact while a
    // page in the middle is overwritten — the shape of a torn write.
    assert!(
        std::fs::metadata(&index).unwrap().len() > 8192,
        "the index grew past two pages"
    );
    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .open(&index)
            .unwrap();
        f.seek(SeekFrom::Start(4096)).unwrap();
        f.write_all(&[0xFFu8; 4096]).unwrap();
        f.sync_all().unwrap();
    }
    let ws = file_ws(&dir);
    let listed = ws.list_goals(None).unwrap();
    assert_eq!(
        listed.len(),
        goals.len(),
        "every goal is back from its snapshot"
    );
    for id in &goals {
        assert!(listed.iter().any(|g| g.id == *id));
    }
}

#[test]
fn a_projection_behind_its_snapshot_is_reconciled_at_open_and_its_wait_is_listed() {
    let dir = tempfile::tempdir().unwrap();
    let index = Paths::new(dir.path()).index_db();
    let backup = dir.path().join("index-before-the-run.sqlite");
    let (goal, wf) = {
        let ws = file_ws(&dir);
        let goal = ws.create_goal(NewGoal::captured("hold on")).unwrap().id;
        let wf = ws
            .create_workflow(waiting_workflow("Hold"), WorkflowOrigin::Workspace)
            .unwrap();
        ws.set_goal_workflow(goal, Some(wf.id)).unwrap();
        (goal, wf.id)
    };
    // The index as it was before the run: what a crash between the run's
    // snapshot write and its index transaction leaves behind.
    std::fs::copy(&index, &backup).unwrap();
    let run = {
        let ws = file_ws(&dir);
        let (run, _) = ws
            .create_run(
                RunScope::Goal { goal },
                wf,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        assert_eq!(
            ws.list_armed_waits().unwrap().len(),
            1,
            "the run's wait is armed in the index"
        );
        run.id
    };
    for suffix in ["-wal", "-shm"] {
        let mut name = index.as_os_str().to_os_string();
        name.push(suffix);
        // A sidecar that is not there is nothing to remove.
        let _removed = std::fs::remove_file(name);
    }
    std::fs::copy(&backup, &index).unwrap();

    let ws = file_ws(&dir);
    let armed = ws.list_armed_waits().unwrap();
    assert_eq!(
        armed.len(),
        1,
        "the open reconciled the run from its snapshot: {armed:?}"
    );
    assert_eq!(armed[0].0, run);
    assert_eq!(armed[0].1, sid("hold"));
    assert_eq!(
        ws.get_goal(goal).unwrap().run,
        Some(run),
        "the snapshot was the truth all along"
    );
}

#[test]
fn a_finished_signal_is_truth_first_and_a_rebuild_keeps_its_count_and_its_error() {
    let (_dir, ws) = ws();
    let wf = ws
        .create_workflow(waiting_workflow("Hold"), WorkflowOrigin::Workspace)
        .unwrap();
    let listener = ListenerKey {
        host: ListenerHost::Workspace { workflow: wf.id },
        step: sid("ticket"),
    };
    let signal = Signal {
        id: "sig-1".into(),
        listener: Some(listener.clone()),
        source: SignalSource::Hook,
        name: None,
        at: 10,
        payload: serde_json::json!({"n": 1}),
        scope: SignalScope::Workspace,
        chain: Chain::default(),
        dedupe_key: Some("delivery-1".into()),
    };
    let (_, fresh) = ws.enqueue_signal(&signal).unwrap();
    assert!(fresh);
    let claimed = ws.claim_next_signal().unwrap().expect("the signal");
    assert_eq!(claimed.signal.id, "sig-1");
    ws.move_signal("sig-1", SignalState::Failed, Some("the start refused"))
        .unwrap();
    let settled = ws.signal("sig-1").unwrap().unwrap();
    assert_eq!(settled.state, SignalState::Failed);
    assert_eq!(settled.attempts, 1);

    // The queue log — truth — has the outcome as its last word on the signal,
    // and a rebuild reads the count and the error back from it, not from the
    // index it just threw away.
    let log = std::fs::read_to_string(Paths::new(_dir.path()).signal_queue()).unwrap();
    let last = log.lines().last().unwrap();
    assert!(
        last.contains("\"state\":\"failed\"") && last.contains("\"attempts\":1"),
        "{last}"
    );
    assert!(last.contains("the start refused"), "{last}");
    ws.rebuild_index().unwrap();
    let rebuilt = ws.signal("sig-1").unwrap().unwrap();
    assert_eq!(rebuilt.state, SignalState::Failed);
    assert_eq!(rebuilt.attempts, 1);
    assert_eq!(rebuilt.note.as_deref(), Some("the start refused"));
    assert_eq!(rebuilt.signal.listener, Some(listener));
    assert!(
        ws.claim_next_signal().unwrap().is_none(),
        "a finished signal is never claimed again"
    );
    // The same delivery again is the same signal, before and after the
    // rebuild: one occurrence, one row.
    let (again, fresh) = ws
        .enqueue_signal(&Signal {
            id: "sig-2".into(),
            ..signal
        })
        .unwrap();
    assert!(!fresh);
    assert_eq!(again.signal.id, "sig-1");
}

/// A signal claimed and never settled — the process ended mid-dispatch — is
/// queued again by the recovery sweep, the log first; the rebuild after it
/// reads it queued.
#[test]
fn a_signal_claimed_when_the_process_ended_is_queued_again() {
    let dir = tempfile::tempdir().unwrap();
    let ws = file_ws(&dir);
    let wf = ws
        .create_workflow(waiting_workflow("Hold"), WorkflowOrigin::Workspace)
        .unwrap();
    ws.enqueue_signal(&Signal {
        id: "sig-1".into(),
        listener: Some(ListenerKey {
            host: ListenerHost::Workspace { workflow: wf.id },
            step: sid("nightly"),
        }),
        source: SignalSource::Schedule,
        name: None,
        at: 10,
        payload: serde_json::json!({"at": 10}),
        scope: SignalScope::Workspace,
        chain: Chain::default(),
        dedupe_key: Some("schedule:10".into()),
    })
    .unwrap();
    ws.claim_next_signal().unwrap().expect("claimed");
    assert_eq!(
        ws.signal_state("sig-1").unwrap(),
        Some(SignalState::Running)
    );
    drop(ws);

    let ws = file_ws(&dir);
    assert_eq!(
        ws.requeue_stale_running(0).unwrap(),
        vec!["sig-1".to_string()]
    );
    ws.rebuild_index().unwrap();
    assert_eq!(ws.signal_state("sig-1").unwrap(), Some(SignalState::Queued));
    assert_eq!(
        ws.claim_next_signal().unwrap().map(|q| q.signal.id),
        Some("sig-1".to_string()),
        "at least once: the occurrence is not lost"
    );
}

#[test]
fn a_live_session_is_listed_ended_with_its_time_and_stays_ended_across_a_rebuild() {
    let (_dir, ws) = ws();
    ws.record_session(&SessionRow {
        id: "sess-1".into(),
        adapter: "mock".into(),
        status: SessionStatus::Live,
        pid: Some(4242),
        pid_seen_at: Some(7),
        ..Default::default()
    })
    .unwrap();
    ws.record_session(&SessionRow {
        id: "sess-2".into(),
        adapter: "mock".into(),
        status: SessionStatus::Parked,
        parked_at: Some(5),
        ..Default::default()
    })
    .unwrap();
    let live = ws.list_live_sessions().unwrap();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].id, "sess-1");
    assert_eq!(live[0].pid, Some(4242));
    assert_eq!(live[0].pid_seen_at, Some(7));

    ws.end_session("sess-1", 99).unwrap();
    let ended = ws.session_by_id("sess-1").unwrap().unwrap();
    assert_eq!(ended.status, SessionStatus::Ended);
    assert_eq!(ended.ended_at, Some(99));
    assert_eq!(ended.pid, None, "nothing is left to terminate");
    assert_eq!(ended.pid_seen_at, None);
    assert!(ws.list_live_sessions().unwrap().is_empty());
    assert!(ws.end_session("nobody", 1).is_err());

    ws.rebuild_index().unwrap();
    assert_eq!(
        ws.session_by_id("sess-1").unwrap().unwrap().status,
        SessionStatus::Ended
    );
    assert_eq!(
        ws.session_by_id("sess-2").unwrap().unwrap().status,
        SessionStatus::Parked
    );
    assert!(ws.list_live_sessions().unwrap().is_empty());
}

/// The same crash, for a run of the workspace: its folder is the truth, and
/// the open reconciles its projection — the run listed as going, its wait
/// armed — from the snapshot there, with no goal to read it through.
#[test]
fn a_workspace_run_behind_its_snapshot_is_reconciled_at_open() {
    let dir = tempfile::tempdir().unwrap();
    let index = Paths::new(dir.path()).index_db();
    let backup = dir.path().join("index-before-the-run.sqlite");
    let wf = {
        let ws = file_ws(&dir);
        ws.create_workflow(waiting_workflow("Hold"), WorkflowOrigin::Workspace)
            .unwrap()
            .id
    };
    std::fs::copy(&index, &backup).unwrap();
    let run = {
        let ws = file_ws(&dir);
        let (run, _) = ws
            .create_run(
                RunScope::Workspace {
                    budget: Default::default(),
                },
                wf,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        assert_eq!(ws.list_armed_waits().unwrap().len(), 1);
        run.id
    };
    // The workspace closed with its connection, so the WAL was checkpointed
    // into the file this copy replaces.
    std::fs::copy(&backup, &index).unwrap();

    let ws = file_ws(&dir);
    let armed = ws.list_armed_waits().unwrap();
    assert_eq!(armed.len(), 1, "reconciled from its folder: {armed:?}");
    assert_eq!(armed[0].0, run);
    assert_eq!(
        ws.live_workspace_runs(Some(wf))
            .unwrap()
            .iter()
            .map(|r| r.id)
            .collect::<Vec<_>>(),
        vec![run],
        "the run is listed as going again"
    );
    assert!(
        ws.list_goals(None).unwrap().is_empty(),
        "no goal is involved"
    );
}

/// A work-item snapshot this build cannot read, under an open goal with a
/// live run — what a crash between two builds leaves behind — is one error
/// line and a skipped item at the next open, never a workspace nobody can
/// open again. The run is still reconciled and the readable items are
/// listed.
#[test]
fn an_unreadable_work_item_under_a_live_run_does_not_stop_the_open() {
    let dir = tempfile::tempdir().unwrap();
    let goal = {
        let ws = file_ws(&dir);
        let goal = ws
            .create_goal(NewGoal::captured("keep opening"))
            .unwrap()
            .id;
        let wf = ws
            .create_workflow(waiting_workflow("Hold"), WorkflowOrigin::Workspace)
            .unwrap();
        ws.set_goal_workflow(goal, Some(wf.id)).unwrap();
        ws.create_run(
            RunScope::Goal { goal },
            wf.id,
            BTreeMap::new(),
            bisa_core::RunEntry::by_hand(),
            None,
        )
        .unwrap();
        goal
    };
    // A file named as a work-item snapshot, holding what another build wrote.
    let planted = Paths::new(dir.path())
        .state_dir(&Paths::ns_goal(goal))
        .join(format!(
            "{}-01ARZ3NDEKTSV4RRFFQ69G5FAV.json",
            bisa_core::kind::KIND_WORK_ITEM
        ));
    std::fs::write(&planted, "{not an event this build can read").unwrap();

    let ws = file_ws(&dir);
    let (items, unreadable) = ws.list_work_items_readable(&Home::Goal { goal }).unwrap();
    assert!(items.is_empty(), "{items:?}");
    assert_eq!(unreadable, vec!["01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string()]);
    assert!(
        ws.list_work_items(&Home::Goal { goal }).is_err(),
        "the strict list still refuses it by name"
    );
    assert_eq!(
        ws.list_armed_waits().unwrap().len(),
        1,
        "the live run was reconciled around it"
    );
}

/// A workflow snapshot this build cannot read survives a rebuild of the
/// index: the rebuild skips it with a line, the other workflows come back,
/// and the one read of it still refuses by name.
#[test]
fn an_unreadable_workflow_snapshot_survives_a_rebuild() {
    let dir = tempfile::tempdir().unwrap();
    let index = Paths::new(dir.path()).index_db();
    let (good, bad) = {
        let ws = file_ws(&dir);
        let good = ws
            .create_workflow(waiting_workflow("Readable"), WorkflowOrigin::Workspace)
            .unwrap()
            .id;
        let bad = ws
            .create_workflow(waiting_workflow("Elsewhere"), WorkflowOrigin::Workspace)
            .unwrap()
            .id;
        (good, bad)
    };
    let snapshot = Paths::new(dir.path())
        .state_dir(Paths::NS_WORKFLOWS)
        .join(format!("{}-{bad}.json", bisa_core::kind::KIND_WORKFLOW));
    std::fs::write(&snapshot, "{not an event this build can read").unwrap();
    for suffix in ["", "-wal", "-shm"] {
        let mut name = index.as_os_str().to_os_string();
        name.push(suffix);
        // A sidecar that is not there is nothing to remove.
        let _removed = std::fs::remove_file(name);
    }

    let ws = file_ws(&dir);
    let ids: Vec<_> = ws
        .list_workflows()
        .unwrap()
        .into_iter()
        .map(|w| w.id)
        .collect();
    assert_eq!(ids, vec![good], "the rebuild kept what it could read");
    let read = ws.get_workflow(bad);
    assert!(
        matches!(read, Err(bisa_store::StoreError::Unreadable { .. })),
        "the one read refuses it by name: {read:?}"
    );
}

/// A record of the roster or of its library that cannot be read costs that
/// record and never the list: every other agent, team, skill and server is
/// still listed, the broken one is said — its file, and what it should have
/// been — when it is asked for by itself, and the workspace opens.
#[test]
fn an_unreadable_record_of_the_roster_costs_that_record_and_not_the_list() {
    use bisa_core::{McpId, McpServerConfig, SkillId};
    use bisa_store::{NewAgent, NewMcp, NewSkill, StoreError};

    let dir = tempfile::tempdir().unwrap();
    let ws = file_ws(&dir);
    let agent = |name: &str| {
        ws.add_agent(NewAgent {
            name: name.into(),
            system_prompt: "x".into(),
            harness: "mock".into(),
            ..Default::default()
        })
        .unwrap()
    };
    let skill = |id: &str| {
        ws.create_skill(NewSkill {
            id: SkillId::new(id).unwrap(),
            name: id.into(),
            description: "when".into(),
            tags: Tags::default(),
            markdown: "# body".into(),
        })
        .unwrap()
    };
    let server = |id: &str| {
        ws.create_mcp(NewMcp {
            id: McpId::new(id).unwrap(),
            description: String::new(),
            tags: Tags::default(),
            transport: McpServerConfig::Http {
                name: id.into(),
                url: "http://127.0.0.1:1".into(),
                headers: Default::default(),
            },
        })
        .unwrap()
    };
    let (cut_agent, kept_agent) = (agent("Cut"), agent("Kept"));
    let cut_team = ws
        .create_team("cut", None, vec![], Tags::default())
        .unwrap();
    let kept_team = ws
        .create_team("kept", None, vec![], Tags::default())
        .unwrap();
    let (cut_skill, kept_skill) = (skill("cut-skill"), skill("kept-skill"));
    let (cut_server, kept_server) = (server("cut-server"), server("kept-server"));

    // Written over by something that is no record — another shape of the
    // code, a disk that lost the end of the file.
    let paths = ws.paths();
    let cut = [
        (paths.agent_file(&cut_agent.id), "agent"),
        (paths.team_file(&cut_team.id), "team"),
        (paths.skill_file(&cut_skill.id), "skill"),
        (paths.mcp_file(&cut_server.id), "mcp server"),
    ];
    for (file, _) in &cut {
        assert!(file.is_file(), "{}", file.display());
        std::fs::write(file, b"{ \"id\": ").unwrap();
    }

    let listed = |ws: &Workspace| {
        let agents: Vec<String> = ws
            .list_agents()
            .expect("the roster, whatever one file says")
            .into_iter()
            .filter(|a| !a.is_core())
            .map(|a| a.id.to_string())
            .collect();
        let teams: Vec<String> = ws
            .list_teams()
            .expect("the teams")
            .into_iter()
            .map(|t| t.id.to_string())
            .collect();
        let skills: Vec<String> = ws
            .list_skills()
            .expect("the library")
            .into_iter()
            .map(|s| s.id.to_string())
            .collect();
        let servers: Vec<String> = ws
            .list_mcps()
            .expect("the registry")
            .into_iter()
            .map(|m| m.id.to_string())
            .collect();
        (agents, teams, skills, servers)
    };
    let kept = (
        vec![kept_agent.id.to_string()],
        vec![kept_team.id.to_string()],
        vec![kept_skill.id.to_string()],
        vec![kept_server.id.to_string()],
    );
    assert_eq!(listed(&ws), kept);

    let asked: [Result<(), StoreError>; 4] = [
        ws.get_agent(&cut_agent.id).map(drop),
        ws.get_team(&cut_team.id).map(drop),
        ws.get_skill(&cut_skill.id).map(drop),
        ws.get_mcp(&cut_server.id).map(drop),
    ];
    for (answer, (file, expected)) in asked.into_iter().zip(&cut) {
        match answer {
            Err(StoreError::Unreadable { path, what, .. }) => {
                assert_eq!(what, *expected);
                assert_eq!(path, file.display().to_string());
            }
            other => panic!("{}: {other:?}", file.display()),
        }
    }

    // And the next start opens, with what can be read.
    drop(ws);
    let ws = file_ws(&dir);
    assert_eq!(listed(&ws), kept);
}

/// An addon's record and a pet's manifest are read like every record of the
/// roster: one that is not here is *not found* — what a screen standing on
/// it reads as gone, never a request that was wrong — and one cut short is
/// said by its file and what it should have been, never as a parser's words.
#[test]
fn an_addon_or_a_pet_that_is_not_here_is_not_found_and_one_cut_short_is_said_by_its_file() {
    use bisa_core::AddonId;
    use bisa_store::StoreError;

    let dir = tempfile::tempdir().unwrap();
    let ws = file_ws(&dir);
    let paths = ws.paths();

    let nobody = AddonId::new("acme.nobody").unwrap();
    assert!(
        matches!(
            ws.get_addon(&nobody),
            Err(StoreError::DefinitionNotFound { kind: "addon", .. })
        ),
        "{:?}",
        ws.get_addon(&nobody).map(|_| ())
    );
    let missing = ws.get_pet("nobody").map(|_| ());
    assert!(
        matches!(
            missing,
            Err(StoreError::DefinitionNotFound { kind: "pet", .. })
        ),
        "{missing:?}"
    );

    // Written over by something that is no record.
    let cut = AddonId::new("acme.cut").unwrap();
    let record = paths.addon_record(&cut);
    std::fs::create_dir_all(record.parent().unwrap()).unwrap();
    std::fs::write(&record, "{ \"manifest\": ").unwrap();
    let manifest = paths.pet_manifest("cut").unwrap();
    std::fs::create_dir_all(manifest.parent().unwrap()).unwrap();
    std::fs::write(&manifest, "{ \"id\": ").unwrap();

    match ws.get_addon(&cut).map(|_| ()) {
        Err(StoreError::Unreadable { path, what, .. }) => {
            assert_eq!(path, record.display().to_string());
            assert_eq!(what, "addon");
        }
        other => panic!("an addon cut short: {other:?}"),
    }
    match ws.get_pet("cut").map(|_| ()) {
        Err(StoreError::Unreadable { path, what, .. }) => {
            assert_eq!(path, manifest.display().to_string());
            assert_eq!(what, "pet");
        }
        other => panic!("a pet cut short: {other:?}"),
    }
    // And neither costs the list: the built-ins are all there.
    assert!(ws.list_addons().unwrap().is_empty());
    assert!(ws.list_pets().unwrap().iter().all(|p| p.id != "cut"));
}
