//! What a crash costs the store: nothing acknowledged. The index is a cache
//! that heals itself — a damaged file is rebuilt at open, a projection a
//! crash left behind its snapshot is reconciled at open — the signal queue's
//! truth log is written before the working set and carries the count, and a
//! session has an end the next boot can give it. Every scenario here is a
//! file on a temporary directory; nothing is killed, and nothing of the
//! machine's is touched.

use bisa_core::settings::Scope;
use bisa_core::{
    Chain, Flow, Home, Join, ListenerHost, ListenerKey, OnFail, RunScope, Signal, SignalScope,
    SignalSource, Step, StepId, StepKind, Tags, ValueRef, WaitFor, WorkflowOrigin,
    DEFAULT_MAX_VISITS,
};
use bisa_core::{
    ChannelId, Gate, MemberRole, OwnerScope, PrincipalId, RosterPolicy, RunEntry, WorkstreamId,
};
use bisa_store::{
    Admission, BootObserver, BootPhase, FileKeyStore, GatePolicy, MemoryKeyStore, NewDrawing,
    NewGoal, NewNote, NewProject, NewWorkflow, OwnerFilter, Paths, ProblemKind, SessionRow,
    SessionStatus, SignalState, StoreError, Workspace,
};
use nostr::key::Keys;
use std::collections::BTreeMap;
use std::io::{Seek, SeekFrom, Write};
use std::sync::Mutex;

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

// ---------------------------------------------------------------------------
// The open: one bad file never stops it — the owner key alone does
// ---------------------------------------------------------------------------

fn poison(path: &std::path::Path) {
    std::fs::write(path, "{not a record this build can read").expect("overwrite the file");
}

/// A member file a crash tore is moved aside and the owner is a member
/// again; the people it named are kept, whole, in the moved file, and the
/// open says so once — and nothing the next open.
#[test]
fn a_torn_members_file_is_quarantined_and_the_owner_is_a_member_again() {
    let dir = tempfile::tempdir().unwrap();
    let guest = PrincipalId::new(Keys::generate().public_key().to_hex()).unwrap();
    {
        let ws = file_ws(&dir);
        ws.add_member(guest.clone(), MemberRole::HOSTED[0], Admission::default())
            .unwrap();
        assert_eq!(ws.members().unwrap().len(), 2);
    }
    let members = Paths::new(dir.path()).members_file();
    // Torn at the tail, the way a crash mid-write tears: the roster's words
    // are still in the file, and the file is no longer a member file.
    let mut torn = std::fs::read_to_string(&members).unwrap();
    torn.truncate(torn.len() - 4);
    std::fs::write(&members, &torn).unwrap();

    let ws = file_ws(&dir);
    let roster = ws.members().unwrap();
    assert_eq!(roster.len(), 1, "the owner alone: {roster:?}");
    assert_eq!(roster[0].role, MemberRole::Owner);
    let problems = ws.problems();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(problems[0].kind, ProblemKind::Recreated);
    assert_eq!(problems[0].path, members.display().to_string());
    let kept = std::path::PathBuf::from(problems[0].quarantined.as_deref().expect("moved aside"));
    assert!(
        kept.starts_with(Paths::new(dir.path()).quarantine_dir()),
        "{}",
        kept.display()
    );
    assert_eq!(
        std::fs::read_to_string(&kept).unwrap(),
        torn,
        "the bytes are moved, never changed"
    );
    assert!(
        torn.contains(guest.as_hex()),
        "the guest's key is in the moved file for the person to admit again"
    );
    drop(ws);
    assert!(
        file_ws(&dir).problems().is_empty(),
        "the file is whole now: the next open has nothing to say"
    );
}

/// A governance document this build cannot read does not stop the open:
/// every gate is the owner's alone until it is fixed — closed, never open —
/// the strict read still refuses by name, and Settings › Governance's write
/// repairs it by moving the torn file aside.
#[test]
fn an_unreadable_governance_document_reads_as_owner_only_and_names_itself() {
    let dir = tempfile::tempdir().unwrap();
    let gov = Paths::new(dir.path()).governance_file();
    {
        let ws = file_ws(&dir);
        ws.set_gate_policy(Gate::Approval, GatePolicy::Owner)
            .unwrap();
        assert!(gov.exists());
    }
    poison(&gov);

    let ws = file_ws(&dir);
    assert!(
        matches!(ws.governance(), Err(StoreError::Unreadable { .. })),
        "the strict read still refuses"
    );
    let problems = ws.problems();
    assert!(
        problems
            .iter()
            .any(|p| p.kind == ProblemKind::Unreadable && p.path == gov.display().to_string()),
        "{problems:?}"
    );
    ws.set_gate_policy(Gate::Approval, GatePolicy::Owner)
        .unwrap();
    assert!(ws.governance().is_ok(), "written again from the defaults");
    assert!(
        ws.problems()
            .iter()
            .any(|p| p.kind == ProblemKind::Quarantined && p.quarantined.is_some()),
        "{:?}",
        ws.problems()
    );
    drop(ws);
    assert!(file_ws(&dir).problems().is_empty());
}

/// The `general` channel a crash tore is moved aside and made again at the
/// open; the room every workspace has is never the reason one does not open.
#[test]
fn a_corrupt_general_channel_is_made_again_at_open() {
    let dir = tempfile::tempdir().unwrap();
    let general = ChannelId::general();
    let path = Paths::new(dir.path())
        .state_dir(Paths::NS_CHANNELS)
        .join(format!(
            "{}-{}.json",
            bisa_core::kind::KIND_CHANNEL,
            general.as_str()
        ));
    {
        let ws = file_ws(&dir);
        ws.get_channel(&general).unwrap();
        assert!(path.exists(), "{}", path.display());
    }
    poison(&path);

    let ws = file_ws(&dir);
    ws.get_channel(&general).expect("made again");
    let problems = ws.problems();
    assert!(
        problems
            .iter()
            .any(|p| p.kind == ProblemKind::Recreated && p.quarantined.is_some()),
        "{problems:?}"
    );
    assert_eq!(
        ws.list_channels()
            .unwrap()
            .iter()
            .filter(|c| c.id == general)
            .count(),
        1
    );
    drop(ws);
    assert!(file_ws(&dir).problems().is_empty());
}

/// A settings layer this build cannot read costs its values and not the
/// resolution — the engine, the pump and the logger all read `settings()`
/// at boot — and saving a setting there again repairs the scope by moving
/// the torn file aside.
#[test]
fn a_broken_settings_layer_costs_its_values_not_the_resolution() {
    const KEY: &str = "security.guard.terminal_hooks";
    let dir = tempfile::tempdir().unwrap();
    let machine = Paths::new(dir.path()).machine_settings();
    {
        let ws = file_ws(&dir);
        ws.set_setting(Scope::Machine, None, KEY, serde_json::json!(false))
            .unwrap();
    }
    poison(&machine);

    let ws = file_ws(&dir);
    let resolved = ws.settings(None).unwrap();
    let hooks = resolved.iter().find(|r| r.key == KEY).expect("the key");
    assert_eq!(
        hooks.value,
        serde_json::json!(true),
        "the broken layer's value is not applied; the default stands"
    );
    assert!(
        ws.settings_layer(Scope::Machine, None).is_err(),
        "the strict read still refuses"
    );
    let problems = ws.problems();
    assert!(
        problems
            .iter()
            .any(|p| p.kind == ProblemKind::SettingsLayerUnreadable
                && p.path == machine.display().to_string()),
        "{problems:?}"
    );
    assert_eq!(
        ws.settings(None).unwrap().len(),
        resolved.len(),
        "a second resolution reads the same"
    );
    assert_eq!(
        ws.problems().len(),
        problems.len(),
        "and says the same trouble once"
    );
    ws.set_setting(Scope::Machine, None, KEY, serde_json::json!(false))
        .unwrap();
    assert!(ws.settings_layer(Scope::Machine, None).is_ok(), "repaired");
    assert!(ws
        .problems()
        .iter()
        .any(|p| p.kind == ProblemKind::Quarantined));
    drop(ws);
    assert!(file_ws(&dir).problems().is_empty());
}

/// The owner's key is the one file that stops an open — every record is
/// signed by it — and the refusal names the file; no key is minted over it,
/// and nothing else is touched. The key itself is written whole or not at
/// all, so no crash of the platform's own leaves it torn.
#[test]
fn a_torn_owner_key_stops_the_open_by_name_and_nothing_else_is_touched() {
    let dir = tempfile::tempdir().unwrap();
    let identity = Paths::new(dir.path()).identity_dir();
    {
        let _ws = file_ws(&dir);
    }
    let entries: Vec<_> = std::fs::read_dir(&identity)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    assert!(
        entries
            .iter()
            .all(|p| !p.to_string_lossy().contains(".tmp.")),
        "a key is written whole or not at all: {entries:?}"
    );
    let key = entries
        .iter()
        .find(|p| {
            p.extension().is_some_and(|e| e == "key")
                && !p
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("agent")
        })
        .cloned()
        .expect("the owner's key file");
    std::fs::write(&key, "").unwrap();
    let err =
        Workspace::open_with_keystore(dir.path(), Box::new(FileKeyStore::new(identity.clone())))
            .err()
            .expect("refused");
    assert!(
        matches!(err, StoreError::OwnerKeyUnreadable { .. }),
        "{err}"
    );
    assert!(
        err.to_string().contains(&key.display().to_string()),
        "{err}"
    );
    assert_eq!(
        std::fs::read_to_string(&key).unwrap(),
        "",
        "the key is not minted over"
    );
    assert!(
        !Paths::new(dir.path()).quarantine_dir().exists(),
        "nothing else is touched"
    );
}

// ---------------------------------------------------------------------------
// The rebuild: one bad record never stops it, and it is never trusted half-done
// ---------------------------------------------------------------------------

/// A workflow of one `end` step: a run of it is finished the moment it starts.
fn ending_workflow(name: &str) -> NewWorkflow {
    NewWorkflow {
        name: name.into(),
        description: String::new(),
        inputs: vec![],
        steps: vec![step(
            "end",
            StepKind::End {
                finish: bisa_core::Finish::Done,
            },
            &[],
        )],
        tags: Tags::default(),
        decision_making: false,
    }
}

/// The index deleted the way a person would — the file and the WAL
/// companions SQLite keeps beside it, or the old pages come back with the
/// next open instead of a rebuild.
fn delete_index(paths: &Paths) {
    let db = paths.index_db();
    for suffix in ["", "-wal", "-shm"] {
        let mut name = db.file_name().unwrap().to_os_string();
        name.push(suffix);
        match std::fs::remove_file(db.with_file_name(name)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => panic!("deleting the index: {e}"),
        }
    }
}

/// A run's entry as an event of its goal begins one: the signal carries the
/// listener it came from, which is what puts the signal on the run's row.
fn begun_by(signal: &str, goal: bisa_core::GoalId) -> RunEntry {
    RunEntry {
        step: None,
        event: Some(Signal {
            id: signal.into(),
            listener: Some(ListenerKey {
                host: ListenerHost::Goal { goal },
                step: sid("end"),
            }),
            source: SignalSource::Schedule,
            name: None,
            at: 10,
            payload: serde_json::json!({}),
            scope: SignalScope::Goal { goal },
            chain: Chain::default(),
            dedupe_key: None,
        }),
    }
}

/// Every `.md` under a folder, recursively.
fn markdown_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            markdown_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
}

/// The index deleted — the guide says it may be, any time — and every kind
/// of record a rebuild walks torn one at a time: the open still opens, the
/// rest of each list is there, and every torn file is named. Before this,
/// one torn note, session, workstream, project, channel, drawing or
/// listening record stopped the rebuild, the index stayed unstamped, and
/// every later open rebuilt and failed again.
#[test]
fn a_rebuild_skips_each_record_it_cannot_read_and_names_it() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path());
    let general = ChannelId::general();
    let (keep, torn_project_record, torn_workstream_record, room_file, note_file, drawing_file) = {
        let ws = file_ws(&dir);
        let keep = ws
            .create_project(NewProject::managed("keep").unwrap())
            .unwrap();
        let torn = ws
            .create_project(NewProject::managed("torn").unwrap())
            .unwrap();
        let torn_project_record = paths.project(&torn.slug).record();
        // A workstream record of the kept project written by another shape
        // of the code: a path and a backing, no kind.
        let wid = WorkstreamId::from_ulid(ulid::Ulid::from_parts(3, 1));
        let torn_workstream_record = paths
            .project(&keep.slug)
            .workstreams()
            .join(format!("{wid}.json"));
        std::fs::create_dir_all(torn_workstream_record.parent().unwrap()).unwrap();
        std::fs::write(
            &torn_workstream_record,
            format!(
                r#"{{"id":"{wid}","project":"{}","path":"/x","backing":{{"type":"copy"}}}}"#,
                keep.id
            ),
        )
        .unwrap();
        let room = ws
            .create_channel("room", None, RosterPolicy::default(), Tags::default())
            .unwrap();
        let room_file = paths.state_dir(Paths::NS_CHANNELS).join(format!(
            "{}-{}.json",
            bisa_core::kind::KIND_CHANNEL,
            room.id.as_str()
        ));
        ws.create_note(NewNote {
            scope: OwnerScope::Workspace,
            title: "torn".into(),
            body: "a note".into(),
        })
        .unwrap();
        let mut notes = Vec::new();
        markdown_files(&paths.notes_dir(), &mut notes);
        let [note_file] = notes.as_slice() else {
            panic!("one note file: {notes:?}");
        };
        let drawing = ws
            .create_drawing(NewDrawing {
                scope: OwnerScope::Workspace,
                title: "torn".into(),
                scene: None,
            })
            .unwrap();
        let drawing_file = paths.state_dir(Paths::NS_DRAWINGS).join(format!(
            "{}-{}.json",
            bisa_core::kind::KIND_DRAWING,
            drawing.id
        ));
        ws.record_session(&SessionRow {
            id: "sess-torn".into(),
            adapter: "mock".into(),
            status: SessionStatus::Parked,
            parked_at: Some(5),
            ..Default::default()
        })
        .unwrap();
        (
            keep,
            torn_project_record,
            torn_workstream_record,
            room_file,
            note_file.clone(),
            drawing_file,
        )
    };
    let session_file = paths.sessions_dir().join("mock").join("sess-torn.json");
    assert!(session_file.exists(), "{}", session_file.display());
    let listening_file = paths
        .listening_dir()
        .join(format!("{}.json", ulid::Ulid::from_parts(5, 1)));
    std::fs::create_dir_all(listening_file.parent().unwrap()).unwrap();
    poison(&torn_project_record);
    poison(&room_file);
    poison(&drawing_file);
    poison(&session_file);
    poison(&listening_file);
    std::fs::write(&note_file, "not a note: no front matter, no title").unwrap();
    // A torn multi-byte character — a crash mid-write — in each line log the
    // rebuild reads whole.
    let general_log = paths.conversation_log(general.as_str()).unwrap();
    std::fs::create_dir_all(general_log.parent().unwrap()).unwrap();
    for log in [&general_log, &paths.seen_file(), &paths.signal_queue()] {
        std::fs::create_dir_all(log.parent().unwrap()).unwrap();
        let mut bytes = std::fs::read(log).unwrap_or_default();
        bytes.extend_from_slice(b"{\"torn\":\"\xff\xfe\"}\n");
        std::fs::write(log, bytes).unwrap();
    }
    std::fs::create_dir_all(paths.activity_dir()).unwrap();
    std::fs::write(
        paths.activity_dir().join("2026-10.jsonl"),
        b"{\"torn\":\"\xff\xfe\"}\n",
    )
    .unwrap();
    // The guide's advice, followed: the index is deleted, so the next open
    // rebuilds it from every file above.
    delete_index(&paths);

    let ws = file_ws(&dir);
    let projects = ws.list_projects().unwrap();
    assert_eq!(
        projects.iter().map(|p| p.slug.as_str()).collect::<Vec<_>>(),
        vec!["keep"],
        "the torn project costs its row, not the list"
    );
    assert!(
        ws.primary_workstream(keep.id).is_ok(),
        "the kept project's own workstream is indexed"
    );
    let rooms = ws.list_channels().unwrap();
    assert!(
        rooms.iter().any(|c| c.id == general) && rooms.len() == 1,
        "{rooms:?}"
    );
    assert!(ws.list_notes(OwnerFilter::All).unwrap().is_empty());
    assert!(ws.list_drawings(OwnerFilter::All).unwrap().is_empty());
    assert!(ws.list_live_sessions().unwrap().is_empty());
    let problems = ws.problems();
    for file in [
        &torn_project_record,
        &torn_workstream_record,
        &room_file,
        &note_file,
        &drawing_file,
        &session_file,
    ] {
        assert!(
            problems
                .iter()
                .any(|p| p.kind == ProblemKind::RebuildSkipped
                    && p.path == file.display().to_string()),
            "{} is named: {problems:?}",
            file.display()
        );
    }
    for file in [&torn_project_record, &room_file, &note_file] {
        assert!(file.exists(), "a skipped record is left where it is");
    }
}

/// The rebuild says how far it is: at the first goal, every few goals, and
/// at the last — what a process waiting on the open relays to a person.
#[test]
fn a_rebuild_says_how_far_it_is_from_the_first_goal_to_the_last() {
    struct Counting(Mutex<Vec<BootPhase>>);
    impl BootObserver for Counting {
        fn phase(&self, phase: BootPhase) {
            self.0.lock().unwrap().push(phase);
        }
    }
    let (_dir, ws) = ws();
    for i in 0..30 {
        ws.create_goal(NewGoal::captured(&format!("goal {i}")))
            .unwrap();
    }
    let counting = Counting(Mutex::new(Vec::new()));
    ws.rebuild_index_observed(&counting).unwrap();
    let phases = counting.0.into_inner().unwrap();
    assert_eq!(
        phases.first(),
        Some(&BootPhase::RebuildingIndex { done: 0, of: 30 })
    );
    assert!(phases.contains(&BootPhase::RebuildingIndex { done: 25, of: 30 }));
    assert_eq!(
        phases.last(),
        Some(&BootPhase::RebuildingIndex { done: 30, of: 30 })
    );
    assert_eq!(ws.list_goals(None).unwrap().len(), 30);
}

/// A rebuild cut short — a crash, a kill — leaves the index unstamped, so the
/// next open rebuilds it from the files rather than trusting an index that
/// was wiped and half filled. Before this, `bisa workspace reindex` on a
/// stamped index that failed half-way left an empty index the next open
/// believed: the workspace read as wiped with every file intact.
#[test]
fn a_rebuild_cut_short_leaves_the_index_unstamped_and_the_next_open_rebuilds() {
    struct Cut;
    impl BootObserver for Cut {
        fn phase(&self, phase: BootPhase) {
            if matches!(phase, BootPhase::RebuildingIndex { done: 0, .. }) {
                std::panic::resume_unwind(Box::new("cut short"));
            }
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path());
    let ws = file_ws(&dir);
    for i in 0..3 {
        ws.create_goal(NewGoal::captured(&format!("goal {i}")))
            .unwrap();
    }
    let cut = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ws.rebuild_index_observed(&Cut)
    }));
    assert!(cut.is_err(), "the rebuild was cut short");
    drop(ws);
    let version: i64 = rusqlite::Connection::open(paths.index_db())
        .unwrap()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        version, 0,
        "the stamp came off before the tables were wiped"
    );
    let ws = file_ws(&dir);
    assert_eq!(
        ws.list_goals(None).unwrap().len(),
        3,
        "the next open rebuilt the index from the files"
    );
    assert!(ws.problems().is_empty(), "{:?}", ws.problems());
}

/// Two runs that began from one signal — the first finished in a write whose
/// index update was lost, so the signal was dispatched again — survive the
/// next rebuild: the first keeps the signal, the second is indexed without
/// it, the clash is named, and the signal is not dispatched a third time.
/// Before this the rebuild hit the index's unique signal column and every
/// later open failed.
#[test]
fn two_runs_that_began_from_one_signal_survive_a_rebuild_and_the_first_keeps_it() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path());
    let (a, b, wf, first) = {
        let ws = file_ws(&dir);
        let wf = ws
            .create_workflow(ending_workflow("Done"), WorkflowOrigin::Workspace)
            .unwrap()
            .id;
        let a = ws.create_goal(NewGoal::captured("a")).unwrap().id;
        let b = ws.create_goal(NewGoal::captured("b")).unwrap().id;
        ws.set_goal_workflow(a, Some(wf)).unwrap();
        ws.set_goal_workflow(b, Some(wf)).unwrap();
        let (first, _) = ws
            .create_run(
                RunScope::Goal { goal: a },
                wf,
                BTreeMap::new(),
                begun_by("signal-1", a),
                Some("signal-1".into()),
            )
            .unwrap();
        assert!(first.is_finished(), "a run of one end step is over at once");
        (a, b, wf, first.id)
    };
    // The index write that recorded the dispatch is lost: what a crash
    // between the snapshot and the index transaction leaves behind.
    rusqlite::Connection::open(paths.index_db())
        .unwrap()
        .execute(
            "UPDATE workflow_runs SET dispatched = NULL WHERE id = ?1",
            [first.to_string()],
        )
        .unwrap();
    let second = {
        let ws = file_ws(&dir);
        let (second, _) = ws
            .create_run(
                RunScope::Goal { goal: b },
                wf,
                BTreeMap::new(),
                begun_by("signal-1", b),
                Some("signal-1".into()),
            )
            .expect("the index knows no run of the signal, so it is dispatched again");
        second.id
    };
    delete_index(&paths);

    let ws = file_ws(&dir);
    assert_eq!(ws.list_runs(a).unwrap().len(), 1);
    assert_eq!(ws.list_runs(b).unwrap().len(), 1);
    let problems = ws.problems();
    let clash = problems
        .iter()
        .find(|p| p.kind == ProblemKind::DuplicateDispatch)
        .unwrap_or_else(|| panic!("the clash is named: {problems:?}"));
    assert_eq!(
        clash.path,
        second.to_string(),
        "the later run lost the signal"
    );
    let text = clash.text.to_string();
    assert!(
        text.contains("signal-1") && text.contains(&first.to_string()),
        "{text}"
    );
    let again = ws.create_run(
        RunScope::Goal { goal: b },
        wf,
        BTreeMap::new(),
        begun_by("signal-1", b),
        Some("signal-1".into()),
    );
    assert!(
        matches!(again, Err(StoreError::AlreadyDispatched { .. })),
        "the signal is not dispatched a third time: {again:?}"
    );
}

// ---------------------------------------------------------------------------
// The run path: a crash at any write
// ---------------------------------------------------------------------------

/// A line log's tail a crash tore is mended by the next append: the torn
/// line stays one unparsable line a reader skips, and the new line is whole
/// on a line of its own — never glued to the torn one and lost with it.
#[test]
fn append_line_mends_a_torn_tail() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("log.jsonl");
    std::fs::write(&log, "{\"a\":1}\n{\"b\":").unwrap();
    bisa_store::append_line(&log, "{\"c\":3}").unwrap();
    assert_eq!(
        std::fs::read_to_string(&log).unwrap(),
        "{\"a\":1}\n{\"b\":\n{\"c\":3}\n"
    );
    // A whole tail, and an empty or absent file, are appended to as ever.
    bisa_store::append_line(&log, "{\"d\":4}").unwrap();
    assert!(std::fs::read_to_string(&log)
        .unwrap()
        .ends_with("{\"c\":3}\n{\"d\":4}\n"));
    let fresh = dir.path().join("fresh.jsonl");
    bisa_store::append_line(&fresh, "{\"e\":5}").unwrap();
    assert_eq!(std::fs::read_to_string(&fresh).unwrap(), "{\"e\":5}\n");
}

/// A run finished in a write whose index update a crash lost keeps a row
/// that says *running*; the next open reads the record back and brings the
/// row in step, so a listener's guard never counts the run forever.
#[test]
fn a_finished_run_whose_index_row_a_crash_left_live_is_repaired_at_open() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path());
    let run = {
        let ws = file_ws(&dir);
        let wf = ws
            .create_workflow(ending_workflow("Done"), WorkflowOrigin::Workspace)
            .unwrap()
            .id;
        let goal = ws.create_goal(NewGoal::captured("a")).unwrap().id;
        ws.set_goal_workflow(goal, Some(wf)).unwrap();
        let (run, _) = ws
            .create_run(
                RunScope::Goal { goal },
                wf,
                BTreeMap::new(),
                RunEntry::by_hand(),
                None,
            )
            .unwrap();
        assert!(run.is_finished());
        run.id
    };
    let index = rusqlite::Connection::open(paths.index_db()).unwrap();
    index
        .execute(
            "UPDATE workflow_runs SET status = 'running', finished_at = NULL WHERE id = ?1",
            [run.to_string()],
        )
        .unwrap();
    drop(index);

    let ws = file_ws(&dir);
    let problems = ws.problems();
    assert!(
        problems
            .iter()
            .any(|p| p.kind == ProblemKind::StaleRow && p.path == run.to_string()),
        "{problems:?}"
    );
    drop(ws);
    let status: String = rusqlite::Connection::open(paths.index_db())
        .unwrap()
        .query_row(
            "SELECT status FROM workflow_runs WHERE id = ?1",
            [run.to_string()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "done", "the row reads what the record says");
}

/// An index row for a run whose record is gone — a snapshot a person
/// removed by hand, a folder restored from an older copy — is dropped at
/// the open rather than listed as live for good.
#[test]
fn an_index_row_for_a_run_with_no_record_is_dropped_at_open() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path());
    let (goal, run) = {
        let ws = file_ws(&dir);
        let wf = ws
            .create_workflow(waiting_workflow("Hold"), WorkflowOrigin::Workspace)
            .unwrap()
            .id;
        let goal = ws.create_goal(NewGoal::captured("a")).unwrap().id;
        ws.set_goal_workflow(goal, Some(wf)).unwrap();
        let (run, _) = ws
            .create_run(
                RunScope::Goal { goal },
                wf,
                BTreeMap::new(),
                RunEntry::by_hand(),
                None,
            )
            .unwrap();
        (goal, run.id)
    };
    let snapshot = paths
        .state_dir(&Paths::ns_goal(goal))
        .join(format!("{}-{run}.json", bisa_core::kind::KIND_WORKFLOW_RUN));
    assert!(snapshot.exists(), "{}", snapshot.display());
    std::fs::remove_file(&snapshot).unwrap();

    let ws = file_ws(&dir);
    assert!(
        ws.problems()
            .iter()
            .any(|p| p.kind == ProblemKind::StaleRow && p.path == run.to_string()),
        "{:?}",
        ws.problems()
    );
    drop(ws);
    let rows: i64 = rusqlite::Connection::open(paths.index_db())
        .unwrap()
        .query_row(
            "SELECT count(*) FROM workflow_runs WHERE id = ?1",
            [run.to_string()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rows, 0, "the row is gone with its record");
}

// ---------------------------------------------------------------------------
// The check: every file read as the next open would read it, without opening
// ---------------------------------------------------------------------------

/// A sound workspace checks clean; one a crash tore is read file by file —
/// the torn snapshot, the torn journal tail, the member file the open then
/// moved aside — and the owner key alone is the finding that stops an open.
/// Nothing is written: the same findings come back twice.
#[test]
fn the_check_names_every_torn_file_and_the_quarantine_and_is_quiet_on_a_sound_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path());
    let goal = {
        let ws = file_ws(&dir);
        ws.create_goal(NewGoal::captured("checked")).unwrap().id
    };
    assert_eq!(bisa_store::check_files(&paths).unwrap(), vec![], "sound");

    // The crash: a snapshot torn, a journal tail torn, the member file torn.
    let snapshot = paths
        .state_dir(&Paths::ns_goal(goal))
        .join(format!("{}-{goal}.json", bisa_core::kind::KIND_GOAL));
    poison(&snapshot);
    let journal = walk_for(dir.path(), "jsonl")
        .into_iter()
        .find(|p| p.starts_with(paths.goals_dir()))
        .expect("the goal's journal");
    {
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&journal)
            .unwrap();
        f.write_all(b"{\"torn\":").unwrap();
    }
    poison(&paths.members_file());

    let before = bisa_store::check_files(&paths).unwrap();
    let kinds: Vec<(bisa_store::FindingKind, &str)> =
        before.iter().map(|f| (f.kind, f.path.as_str())).collect();
    assert!(
        kinds.contains(&(
            bisa_store::FindingKind::Unparseable,
            snapshot.to_str().unwrap()
        )),
        "{kinds:?}"
    );
    assert!(
        kinds.contains(&(bisa_store::FindingKind::TornTail, journal.to_str().unwrap())),
        "{kinds:?}"
    );
    assert!(
        kinds.contains(&(
            bisa_store::FindingKind::Unparseable,
            paths.members_file().to_str().unwrap()
        )),
        "{kinds:?}"
    );
    assert_eq!(
        bisa_store::check_files(&paths).unwrap(),
        before,
        "the check writes nothing: the same findings twice"
    );

    // The open: the member file is moved aside, and the check now lists it
    // under the quarantine; the goal snapshot stays where it is, skipped.
    drop(file_ws(&dir));
    let after = bisa_store::check_files(&paths).unwrap();
    assert!(
        after
            .iter()
            .any(|f| f.kind == bisa_store::FindingKind::Quarantined
                && f.path.contains("quarantine")
                && f.path.ends_with("members.json")),
        "{after:?}"
    );
    assert!(
        !after
            .iter()
            .any(|f| f.path == paths.members_file().to_str().unwrap()),
        "the member file was made again and reads: {after:?}"
    );

    // The one finding that stops an open.
    let key = paths.identity_dir().join("owner.key");
    assert!(key.is_file(), "{}", key.display());
    std::fs::write(&key, "not a key").unwrap();
    let stopped = bisa_store::check_files(&paths).unwrap();
    assert!(
        stopped
            .iter()
            .any(|f| f.kind == bisa_store::FindingKind::OwnerKeyUnreadable
                && f.path == key.to_str().unwrap()),
        "{stopped:?}"
    );
    assert!(
        matches!(
            Workspace::open_with_keystore(
                dir.path(),
                Box::new(bisa_store::identity::FileKeyStore::new(
                    paths.identity_dir()
                ))
            ),
            Err(StoreError::OwnerKeyUnreadable { .. })
        ),
        "and the open says the same"
    );
    assert_eq!(
        bisa_store::check_files(&Paths::new(dir.path().join("nowhere"))).unwrap(),
        vec![],
        "a workspace that is not there has nothing to check"
    );
}

/// Every file under `dir` with the extension, depth first.
fn walk_for(dir: &std::path::Path, ext: &str) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk_for(&path, ext));
        } else if path.extension().is_some_and(|e| e == ext) {
            out.push(path);
        }
    }
    out.sort();
    out
}
