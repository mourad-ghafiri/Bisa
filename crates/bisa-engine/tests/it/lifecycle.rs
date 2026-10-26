//! The lifecycle events, end to end: a goal's, a workflow's, a run's and a
//! session's move read off the bus as the `goal`, `workflow`, `run` and
//! `agent` events, a checkout's as a `project` change — each a durable
//! signal for the listener that hears it, deduplicated, under the causal
//! chain. The ear is a call (`hear`) where a test needs an event heard now;
//! what the engine itself announces reaches the same ear and is written
//! once.

use crate::common;

use bisa_core::{
    AgentChange, AgentFilter, ClosureReason, Finish, Flow, GoalChange, GoalFilter, Guard,
    ListenerHost, ListenerKey, MessageBody, MessageFilter, MessageFrom, MessagePlace, Overlap,
    PlatformFilter, ProjectChange, ProjectFilter, RunChange, RunFilter, RunOutcome, SignalSource,
    StartOn, Step, StepKind, ValueRef, WaitFor, Workflow, WorkflowChange, WorkflowFilter,
    WorkstreamState,
};
use bisa_engine::{Begun, Engine, EngineEvent, EnginePayload};
use bisa_harness::mock::MockAdapter;
use bisa_store::{PostOrigin, QueuedSignal};
use common::*;
use serde_json::json;
use std::collections::BTreeMap;
use std::num::NonZeroU32;

fn engine_on(dir: &tempfile::TempDir) -> Engine {
    engine_with(dir, vec![MockAdapter::default()])
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// A `start` step on `on` that flows to `hold`, mapping nothing, taking
/// several at once so a burst of occurrences never waits on the guard.
fn start(id: &str, on: StartOn) -> Step {
    let mut s = step(
        id,
        StepKind::Start {
            on,
            inputs: BTreeMap::new(),
            guard: Guard {
                overlap: Overlap::Parallel(NonZeroU32::new(8).unwrap()),
                ..Guard::default()
            },
        },
    );
    s.then = vec![Flow::to(sid("hold"))];
    s
}

/// A step a run stays live on until somebody releases it.
fn hold() -> Step {
    step(
        "hold",
        StepKind::Wait {
            until: WaitFor::Release,
        },
    )
}

/// A library workflow with one start on `on`, turned on; its host and the
/// listener's key.
fn listening(engine: &Engine, name: &str, on: StartOn) -> (Workflow, ListenerHost, ListenerKey) {
    let wf = engine
        .create_workflow(new_workflow(name, vec![start("on", on), hold()]))
        .unwrap();
    let host = ListenerHost::Workspace { workflow: wf.id };
    engine.set_listening(host, BTreeMap::new(), None).unwrap();
    (
        wf,
        host,
        ListenerKey {
            host,
            step: sid("on"),
        },
    )
}

/// A host's signals, oldest first.
fn signals(engine: &Engine, host: &ListenerHost) -> Vec<QueuedSignal> {
    let mut all = engine.workspace().list_signals(Some(host), 200).unwrap();
    all.sort_by(|a, b| (a.signal.at, &a.signal.id).cmp(&(b.signal.at, &b.signal.id)));
    all
}

/// The host's signals once it has at least `n` — what the engine's own
/// announcements write arrives through the ear's own task.
async fn heard(engine: &Engine, host: &ListenerHost, n: usize) -> Vec<QueuedSignal> {
    until(&format!("{n} signal(s) for {host}"), || {
        let all = signals(engine, host);
        (all.len() >= n).then_some(all)
    })
    .await
}

fn on_goal(goal: Option<String>, changes: Vec<GoalChange>) -> StartOn {
    StartOn::Goal {
        filter: GoalFilter {
            goal: goal.map(Into::into),
            change: changes.into(),
        },
    }
}

/// A goal's lifecycle is the workspace's news about it: a library workflow
/// naming the goal hears its close with the goal's title and status, and so
/// does another goal that listens for it — the registry rebuilt under the
/// closing still arms both, since a goal is named by its id's syntax and
/// never by whether it is still open. A goal not named is nobody's event.
#[tokio::test(flavor = "multi_thread")]
async fn a_goals_close_is_heard_by_a_workflow_and_by_another_goal_that_name_it() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let ship = engine
        .submit_goal(bisa_engine::SubmitRequest {
            title: Some("Ship it".into()),
            ..bisa_engine::SubmitRequest::captured("ship the storefront")
        })
        .unwrap();
    let other = engine
        .submit_goal(bisa_engine::SubmitRequest::captured("something else"))
        .unwrap();
    let (library, library_host, _) = listening(
        &engine,
        "after shipping",
        on_goal(Some(ship.id.to_string()), vec![GoalChange::Closed]),
    );
    // A goal whose workflow begins when `ship` closes: it listens.
    let (follower, _) = goal_on(
        &engine,
        "tidy up after shipping",
        new_workflow(
            "tidy",
            vec![
                start(
                    "on",
                    on_goal(
                        Some(ship.id.to_string()),
                        vec![GoalChange::Closed, GoalChange::Deleted],
                    ),
                ),
                hold(),
            ],
        ),
    );
    let begun = engine.begin_goal(follower.id, BTreeMap::new()).unwrap();
    assert!(matches!(begun, Begun::Listening(_)), "{begun:?}");
    let follower_host = ListenerHost::Goal { goal: follower.id };

    // Another goal closing is nobody's event here.
    engine
        .close_goal(other.id, ClosureReason::Abandoned { rationale: None })
        .unwrap();
    engine.hear(&EngineEvent::scoped(
        other.id,
        None,
        EnginePayload::GoalClosed {
            reason: ClosureReason::Abandoned { rationale: None },
        },
    ));
    assert!(signals(&engine, &library_host).is_empty());
    assert!(signals(&engine, &follower_host).is_empty());

    // The named goal closes: the engine says so on the bus, and the test
    // says it again — written once.
    engine
        .close_goal(ship.id, ClosureReason::Abandoned { rationale: None })
        .unwrap();
    engine.hear(&EngineEvent::scoped(
        ship.id,
        None,
        EnginePayload::GoalClosed {
            reason: ClosureReason::Abandoned { rationale: None },
        },
    ));
    let closed = heard(&engine, &library_host, 1).await;
    assert_eq!(closed.len(), 1, "{closed:?}");
    let occurrence = &closed[0].signal;
    assert_eq!(occurrence.source, SignalSource::Goal);
    assert_eq!(occurrence.payload["goal"], json!(ship.id.to_string()));
    assert_eq!(occurrence.payload["change"], json!("closed"));
    assert_eq!(occurrence.payload["title"], json!("Ship it"));
    assert_eq!(occurrence.payload["status"], json!("closed"));
    assert_eq!(occurrence.payload["reason"], json!("abandoned"));
    assert_eq!(occurrence.payload["by"], json!(null));
    assert_eq!(
        occurrence.dedupe_key,
        Some(format!("goal:{}:closed", ship.id))
    );
    let followed = heard(&engine, &follower_host, 1).await;
    assert_eq!(followed.len(), 1, "a goal hears another goal's close");
    assert_eq!(
        followed[0].signal.scope,
        bisa_core::SignalScope::Goal { goal: follower.id },
        "the occurrence is the listening goal's own"
    );

    assert_eq!(engine.drain_signals().await, 2);
    assert_eq!(engine.workflow_runs(library.id).unwrap().len(), 1);
    assert!(engine
        .workspace()
        .get_current_run(follower.id)
        .unwrap()
        .is_some());
    engine.shutdown().await;
}

/// A list of changes hears any of them, and the goal payload names what
/// each change carries: the project attached, the gate decided.
#[tokio::test(flavor = "multi_thread")]
async fn a_goal_start_on_a_list_of_changes_hears_each_of_them_with_its_fields() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let goal = engine
        .submit_goal(bisa_engine::SubmitRequest::captured("a goal"))
        .unwrap();
    let project = engine
        .workspace()
        .create_project(bisa_store::NewProject::managed("api").unwrap())
        .unwrap();
    let (_, host, _) = listening(
        &engine,
        "on attach or decide",
        on_goal(None, vec![GoalChange::Attached, GoalChange::Decided]),
    );
    let scoped = |payload: EnginePayload| EngineEvent::scoped(goal.id, None, payload);
    engine.hear(&scoped(EnginePayload::GoalUpdated { goal: goal.id }));
    engine.hear(&scoped(EnginePayload::GateOpened {
        gate_id: "g1".into(),
        gate: bisa_core::Gate::Approval,
        question: "ok?".into(),
    }));
    assert!(
        signals(&engine, &host).is_empty(),
        "changes it does not name"
    );
    engine.hear(&scoped(EnginePayload::AttachmentChanged {
        project: project.id,
        attached: true,
    }));
    engine.hear(&scoped(EnginePayload::GateDecided {
        gate_id: "g1".into(),
        gate: bisa_core::Gate::Approval,
        approve: false,
    }));
    engine.hear(&scoped(EnginePayload::GateDecided {
        gate_id: "g1".into(),
        gate: bisa_core::Gate::Approval,
        approve: false,
    }));
    let queued = signals(&engine, &host);
    assert_eq!(queued.len(), 2, "{queued:?}");
    assert_eq!(queued[0].signal.payload["change"], json!("attached"));
    assert_eq!(
        queued[0].signal.payload["project"],
        json!(project.id.to_string())
    );
    assert_eq!(queued[0].signal.payload["status"], json!("draft"));
    assert_eq!(queued[1].signal.payload["change"], json!("decided"));
    assert_eq!(queued[1].signal.payload["gate"], json!("approval"));
    assert_eq!(queued[1].signal.payload["approve"], json!(false));
    assert_eq!(
        queued[1].signal.dedupe_key,
        Some(format!("goal:{}:decided:g1", goal.id)),
        "a decision heard twice is one occurrence"
    );
    engine.shutdown().await;
}

/// A workflow definition's life: a save is heard with the name and the
/// revision, a proposal is not a save, and a deletion is heard by a start
/// that names the deleted workflow — the library no longer has it, which is
/// no problem of the definition that listens.
#[tokio::test(flavor = "multi_thread")]
async fn a_workflow_start_hears_a_save_and_a_deletion_and_not_a_proposal() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let watched = engine
        .create_workflow(new_workflow("watched", vec![hold()]))
        .unwrap();
    let on_workflow = |change: WorkflowChange| StartOn::Workflow {
        filter: WorkflowFilter {
            workflow: Some(watched.id.into()),
            change: change.into(),
        },
    };
    let (_, saves, _) = listening(&engine, "on a save", on_workflow(WorkflowChange::Saved));
    let (_, deletions, _) = listening(
        &engine,
        "on a deletion",
        on_workflow(WorkflowChange::Deleted),
    );

    let (saved, problems) = engine
        .save_workflow(
            watched.id,
            new_workflow("watched, renamed", vec![hold()]),
            watched.revision,
        )
        .unwrap();
    assert!(problems.is_empty());
    engine.hear(&EngineEvent::global(EnginePayload::WorkflowChanged {
        workflow: watched.id,
        revision: saved.revision,
        designed: false,
    }));
    let heard_save = heard(&engine, &saves, 1).await;
    assert_eq!(heard_save.len(), 1, "{heard_save:?}");
    let occurrence = &heard_save[0].signal;
    assert_eq!(occurrence.source, SignalSource::Workflow);
    assert_eq!(occurrence.payload["change"], json!("saved"));
    assert_eq!(occurrence.payload["name"], json!("watched, renamed"));
    assert_eq!(occurrence.payload["revision"], json!(saved.revision));
    assert_eq!(occurrence.payload["designed"], json!(false));
    assert_eq!(
        occurrence.dedupe_key,
        Some(format!("workflow:{}:saved@{}", watched.id, saved.revision))
    );
    engine.hear(&EngineEvent::global(EnginePayload::WorkflowProposed {
        workflow: watched.id,
        revision: saved.revision + 1,
        gate_id: None,
    }));
    assert_eq!(
        signals(&engine, &saves).len(),
        1,
        "a proposal is not a save"
    );
    assert!(signals(&engine, &deletions).is_empty());

    engine.delete_workflow(watched.id).unwrap();
    engine.hear(&EngineEvent::global(EnginePayload::WorkflowDeleted {
        workflow: watched.id,
    }));
    let heard_deletion = heard(&engine, &deletions, 1).await;
    assert_eq!(heard_deletion.len(), 1, "{heard_deletion:?}");
    assert_eq!(heard_deletion[0].signal.payload["change"], json!("deleted"));
    assert_eq!(
        heard_deletion[0].signal.payload["name"],
        json!(null),
        "gone: no name to say"
    );
    assert_eq!(signals(&engine, &saves).len(), 1);
    assert_eq!(engine.drain_signals().await, 2);
    engine.shutdown().await;
}

/// A `run` start that names a step's move hears the failing step with its
/// state and kind, and a `run` start that says nothing of `change` hears the
/// run's end alone, as it always did — its payload now saying so.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_start_hears_a_steps_move_and_an_unchanged_run_start_hears_ends_alone() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let failing = engine
        .create_workflow(new_workflow(
            "fails",
            vec![
                {
                    let mut s = step(
                        "begin",
                        StepKind::Start {
                            on: StartOn::Manual,
                            inputs: BTreeMap::new(),
                            guard: Guard::default(),
                        },
                    );
                    s.then = vec![Flow::to(sid("bad"))];
                    s
                },
                step(
                    "bad",
                    StepKind::End {
                        finish: Finish::Failed,
                    },
                ),
            ],
        ))
        .unwrap();
    let (_, on_step, _) = listening(
        &engine,
        "on a failed step",
        StartOn::Run {
            filter: RunFilter {
                workflow: Some(failing.id.into()),
                change: RunChange::Step.into(),
                state: Some("failed".into()),
                ..RunFilter::default()
            },
        },
    );
    let (_, on_end, _) = listening(
        &engine,
        "on an end",
        StartOn::Run {
            filter: RunFilter {
                workflow: Some(failing.id.into()),
                ..RunFilter::default()
            },
        },
    );
    let run = engine
        .start_workspace_run(failing.id, BTreeMap::new())
        .unwrap();
    let failed = run_finished(&engine, run.id).await;
    assert_eq!(failed.outcome, Some(RunOutcome::Failed));
    engine.hear(&EngineEvent::of_run(
        &failed,
        None,
        EnginePayload::StepChanged {
            run: failed.id,
            workflow: failing.id,
            step: sid("bad"),
            state: "failed".into(),
            kind: "end".into(),
        },
    ));
    engine.hear(&EngineEvent::of_run(
        &failed,
        None,
        EnginePayload::RunFinished {
            run: failed.id,
            workflow: failing.id,
            outcome: RunOutcome::Failed,
        },
    ));
    let steps = heard(&engine, &on_step, 1).await;
    assert_eq!(steps.len(), 1, "{steps:?}");
    let moved = &steps[0].signal;
    assert_eq!(moved.source, SignalSource::Run);
    assert_eq!(moved.payload["change"], json!("step"));
    assert_eq!(moved.payload["step"], json!("bad"));
    assert_eq!(moved.payload["state"], json!("failed"));
    assert_eq!(moved.payload["kind"], json!("end"));
    assert_eq!(moved.payload["outcome"], json!(null));
    let entered = failed.steps[&sid("bad")].started_at.unwrap();
    assert_eq!(
        moved.dedupe_key,
        Some(format!("run:{}:step:bad:failed@{entered}", failed.id))
    );
    let ends = heard(&engine, &on_end, 1).await;
    assert_eq!(ends.len(), 1, "{ends:?}");
    assert_eq!(ends[0].signal.payload["change"], json!("ended"));
    assert_eq!(ends[0].signal.payload["outcome"], json!("failed"));
    assert_eq!(ends[0].signal.payload["step"], json!(null));
    assert_eq!(
        ends[0].signal.dedupe_key,
        Some(format!("run:{}", failed.id))
    );
    assert_eq!(signals(&engine, &on_step).len(), 1, "an end is no step");
    engine.shutdown().await;
}

/// An agent event hears a session's start and its end, in the project it
/// works in, and nothing of what the session did in between; a reply in a
/// conversation is heard when it was posted and never when it was not.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_start_hears_a_sessions_start_and_end_in_its_project_and_a_reply() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    let ws = engine.workspace();
    let project = ws
        .create_project(bisa_store::NewProject::managed("api").unwrap())
        .unwrap();
    let on_agent =
        |project: Option<ProjectFilterProject>, changes: Vec<AgentChange>| StartOn::Agent {
            filter: AgentFilter {
                agent: None,
                project: project.map(|p| ValueRef::Fixed(p).into()),
                harness: None,
                change: changes.into(),
            },
        };
    // A worker whose result landed is ended by the engine: the roster says
    // `aborted`, and so does the event — `done` is a session whose turn
    // ended on its own. A listener on either hears the end.
    let (_, done_here, _) = listening(
        &engine,
        "when a session in api ends",
        on_agent(
            Some(project.id),
            vec![AgentChange::Done, AgentChange::Aborted],
        ),
    );
    let (_, started, _) = listening(
        &engine,
        "when any session starts",
        on_agent(None, vec![AgentChange::Started]),
    );
    let (_, waiting, _) = listening(
        &engine,
        "when a session waits",
        on_agent(None, vec![AgentChange::Waiting, AgentChange::Failed]),
    );
    let (_, replied, _) = listening(
        &engine,
        "when an agent replies",
        on_agent(None, vec![AgentChange::Replied]),
    );

    let mut go = agent_step("go", "mock");
    if let StepKind::Agent { project: at, .. } = &mut go.kind {
        *at = Some(ValueRef::Fixed(project.id));
    }
    let (goal, _) = goal_on(&engine, "work in api", new_workflow("works", vec![go]));
    engine.attach_project(goal.id, project.id).unwrap();
    let run = engine.start_run(goal.id, BTreeMap::new()).unwrap();
    let finished = run_finished(&engine, run.id).await;
    assert_eq!(finished.outcome, Some(RunOutcome::Done));

    let ended = heard(&engine, &done_here, 1).await;
    assert_eq!(ended.len(), 1, "{ended:?}");
    let occurrence = &ended[0].signal;
    assert_eq!(occurrence.source, SignalSource::Agent);
    assert_eq!(occurrence.payload["change"], json!("aborted"));
    assert_eq!(occurrence.payload["state"], json!("aborted"));
    assert_eq!(occurrence.payload["harness"], json!("mock"));
    assert_eq!(occurrence.payload["kind"], json!("worker"));
    assert_eq!(occurrence.payload["project"], json!(project.id.to_string()));
    assert_eq!(occurrence.payload["goal"], json!(goal.id.to_string()));
    assert_eq!(occurrence.payload["run"], json!(run.id.to_string()));
    assert_eq!(
        occurrence.scope,
        bisa_core::SignalScope::Workspace,
        "the occurrence is the listening workflow's own, whatever goal the session was on"
    );
    let live_run = occurrence.payload["live_run"].as_str().unwrap().to_string();
    assert_eq!(
        occurrence.dedupe_key,
        Some(format!("session:{live_run}:aborted"))
    );
    let begun = heard(&engine, &started, 1).await;
    assert_eq!(begun.len(), 1, "{begun:?}");
    assert_eq!(begun[0].signal.payload["change"], json!("started"));
    assert_eq!(
        begun[0].signal.dedupe_key,
        Some(format!("session:{live_run}:started"))
    );
    assert!(
        signals(&engine, &waiting).is_empty(),
        "the churn between a start and an end — idle, thinking — is nobody's event"
    );

    // A reply in a conversation: heard when it was posted, never when the
    // turn ended with nothing said.
    let general = bisa_core::ChannelId::general().to_string();
    engine.hear(&EngineEvent::global(EnginePayload::AgentReplied {
        scope: general.clone(),
        agent: "herald".into(),
        posted: false,
        message: None,
    }));
    assert!(signals(&engine, &replied).is_empty());
    engine.hear(&EngineEvent::global(EnginePayload::AgentReplied {
        scope: general.clone(),
        agent: "herald".into(),
        posted: true,
        message: Some("m1".into()),
    }));
    engine.hear(&EngineEvent::global(EnginePayload::AgentReplied {
        scope: general.clone(),
        agent: "herald".into(),
        posted: true,
        message: Some("m1".into()),
    }));
    let replies = signals(&engine, &replied);
    assert_eq!(replies.len(), 1, "{replies:?}");
    assert_eq!(replies[0].signal.payload["change"], json!("replied"));
    assert_eq!(replies[0].signal.payload["agent"], json!("herald"));
    assert_eq!(replies[0].signal.payload["scope"], json!(general));
    assert_eq!(replies[0].signal.payload["message"], json!("m1"));
    assert_eq!(replies[0].signal.payload["project"], json!(null));
    assert_eq!(replies[0].signal.dedupe_key, Some("reply:m1".to_string()));
    engine.shutdown().await;
}

type ProjectFilterProject = bisa_core::ProjectId;

/// A checkout's move is a project change heard from the bus — no look at
/// the repository — with the checkout's state and its pull request.
#[tokio::test(flavor = "multi_thread")]
async fn a_checkouts_move_is_a_project_change_heard_from_the_bus() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let ws = engine.workspace();
    let project = ws
        .create_project(bisa_store::NewProject::managed("api").unwrap())
        .unwrap();
    let primary = ws.primary_workstream(project.id).unwrap();
    let (_, host, _) = listening(
        &engine,
        "when the checkout moves",
        StartOn::Project {
            filter: ProjectFilter {
                project: Some(ValueRef::Fixed(project.id).into()),
                change: vec![ProjectChange::Workstream, ProjectChange::Commit].into(),
                ..ProjectFilter::default()
            },
        },
    );
    let moved = |state: WorkstreamState| {
        EngineEvent::global(EnginePayload::WorkstreamChanged {
            workstream: primary.id,
            state,
        })
    };
    engine.hear(&moved(WorkstreamState::Committed));
    engine.hear(&moved(WorkstreamState::Committed));
    engine.hear(&moved(WorkstreamState::PrOpen {
        number: 7,
        url: "https://example.invalid/pr/7".into(),
    }));
    let queued = signals(&engine, &host);
    assert_eq!(queued.len(), 2, "{queued:?}");
    let committed = &queued[0].signal;
    assert_eq!(committed.source, SignalSource::Project);
    assert_eq!(committed.payload["change"], json!("workstream"));
    assert_eq!(committed.payload["state"], json!("committed"));
    assert_eq!(committed.payload["project"], json!(project.id.to_string()));
    assert_eq!(committed.payload["slug"], json!("api"));
    assert_eq!(
        committed.payload["workstream"],
        json!(primary.id.to_string())
    );
    assert_eq!(
        committed.dedupe_key,
        Some(format!("workstream:{}:committed", primary.id))
    );
    assert_eq!(queued[1].signal.payload["state"], json!("pr_open"));
    assert_eq!(queued[1].signal.payload["pull_request"]["number"], json!(7));
    // A checkout opening is the change in state `open`, on its branch.
    engine.hear(&EngineEvent::global(EnginePayload::WorkstreamOpened {
        workstream: primary.id,
        project: project.id,
        path: "api".into(),
        branch: Some("main".into()),
        source: "test".into(),
    }));
    let queued = signals(&engine, &host);
    assert_eq!(queued.len(), 3, "{queued:?}");
    let opened = queued
        .iter()
        .find(|q| q.signal.payload["state"] == json!("open"))
        .unwrap();
    assert_eq!(opened.signal.payload["branch"], json!("main"));
    assert_eq!(
        opened.signal.dedupe_key,
        Some(format!("workstream:{}:open", primary.id))
    );
    // A checkout of another project is not this project's.
    let elsewhere = ws
        .create_project(bisa_store::NewProject::managed("web").unwrap())
        .unwrap();
    let theirs = ws.primary_workstream(elsewhere.id).unwrap();
    engine.hear(&EngineEvent::global(EnginePayload::WorkstreamChanged {
        workstream: theirs.id,
        state: WorkstreamState::Pushed,
    }));
    assert_eq!(signals(&engine, &host).len(), 3);
    assert_eq!(engine.drain_signals().await, 3);

    // A project whose changes come from the bus alone is never looked at:
    // a tick writes its listener no memory, and the bus still reaches it.
    let (_, bus_fed, bus_key) = listening(
        &engine,
        "on the other checkout alone",
        StartOn::Project {
            filter: ProjectFilter {
                project: Some(ValueRef::Fixed(elsewhere.id).into()),
                change: ProjectChange::Workstream.into(),
                ..ProjectFilter::default()
            },
        },
    );
    engine.tick_listeners_at(now()).await;
    assert!(
        ws.listener_runtime(&bus_key).armed.is_none(),
        "never looked at"
    );
    engine.hear(&EngineEvent::global(EnginePayload::WorkstreamChanged {
        workstream: theirs.id,
        state: WorkstreamState::Committed,
    }));
    let fed = signals(&engine, &bus_fed);
    assert_eq!(fed.len(), 1, "{fed:?}");
    assert_eq!(fed[0].signal.payload["state"], json!("committed"));
    assert_eq!(
        fed[0].signal.payload["project"],
        json!(elsewhere.id.to_string())
    );
    engine.shutdown().await;
}

/// A platform start hears any topic of its list.
#[tokio::test(flavor = "multi_thread")]
async fn a_platform_start_hears_any_topic_of_its_list() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let goal = engine
        .submit_goal(bisa_engine::SubmitRequest::captured("a goal"))
        .unwrap();
    let (_, host, _) = listening(
        &engine,
        "on two topics",
        StartOn::Platform {
            filter: PlatformFilter {
                topic: vec!["goal.closed".to_string(), "goal.updated".to_string()].into(),
                fields: BTreeMap::new(),
            },
        },
    );
    engine.hear(&EngineEvent::scoped(
        goal.id,
        None,
        EnginePayload::GoalUpdated { goal: goal.id },
    ));
    engine.hear(&EngineEvent::scoped(
        goal.id,
        None,
        EnginePayload::GoalClosed {
            reason: ClosureReason::Abandoned { rationale: None },
        },
    ));
    engine.hear(&EngineEvent::scoped(
        goal.id,
        None,
        EnginePayload::GoalArchived {
            goal: goal.id,
            archived: true,
        },
    ));
    let queued = signals(&engine, &host);
    assert_eq!(queued.len(), 2, "{queued:?}");
    assert_eq!(queued[0].signal.name.as_deref(), Some("goal.updated"));
    assert_eq!(queued[1].signal.name.as_deref(), Some("goal.closed"));
    engine.shutdown().await;
}

/// A message start hears `anyone` and `people`, a direct channel by its
/// kind, and a list of places — each message saying the kind of place it
/// landed in.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_start_hears_people_anyone_a_direct_channel_and_a_list_of_places() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let ws = engine.workspace();
    let general = bisa_core::ChannelId::general().to_string();
    let herald = ws
        .add_agent(bisa_store::NewAgent {
            name: "Herald".into(),
            harness: "mock".into(),
            ..Default::default()
        })
        .unwrap();
    let dm = ws.open_dm(std::slice::from_ref(&herald.pubkey)).unwrap();
    let on_message = |filter: MessageFilter| StartOn::Message { filter };
    let (_, anyone_direct, _) = listening(
        &engine,
        "anyone, in a direct channel",
        on_message(MessageFilter {
            from: MessageFrom::Anyone.into(),
            kind: Some(MessagePlace::Direct.into()),
            ..MessageFilter::default()
        }),
    );
    let (_, people_anywhere, _) = listening(
        &engine,
        "any person, anywhere",
        on_message(MessageFilter {
            from: MessageFrom::People.into(),
            ..MessageFilter::default()
        }),
    );
    let (_, two_places, _) = listening(
        &engine,
        "anyone, in two places",
        on_message(MessageFilter {
            r#in: Some(vec![general.clone(), dm.id.to_string()].into()),
            from: MessageFrom::Anyone.into(),
            ..MessageFilter::default()
        }),
    );
    let say = |scope: &str, text: &str, as_agent: Option<&bisa_core::AgentId>| {
        ws.post_message(
            scope,
            MessageBody::post(text.to_string()),
            None,
            &[],
            &[],
            as_agent,
            PostOrigin::Asked,
        )
        .unwrap()
    };
    let mine_in_general = say(&general, "hello all", None);
    let theirs_in_dm = say(dm.id.as_ref(), "hello you", Some(&herald.id));

    let direct = heard(&engine, &anyone_direct, 1).await;
    assert_eq!(direct.len(), 1, "{direct:?}");
    assert_eq!(direct[0].signal.payload["message"], json!(theirs_in_dm));
    assert_eq!(direct[0].signal.payload["scope_kind"], json!("direct"));
    assert_eq!(direct[0].signal.payload["author_kind"], json!("agent"));
    let people = heard(&engine, &people_anywhere, 1).await;
    assert_eq!(people.len(), 1, "an agent is nobody's person: {people:?}");
    assert_eq!(people[0].signal.payload["message"], json!(mine_in_general));
    assert_eq!(people[0].signal.payload["scope_kind"], json!("channel"));
    let both = heard(&engine, &two_places, 2).await;
    assert_eq!(both.len(), 2, "{both:?}");

    // A standing channel is no direct one; a message elsewhere is not in
    // the list.
    let other = ws
        .create_channel("ops", None, Default::default(), Default::default())
        .unwrap();
    say(other.id.as_ref(), "psst", None);
    let after = heard(&engine, &people_anywhere, 2).await;
    assert_eq!(after.len(), 2);
    assert_eq!(signals(&engine, &anyone_direct).len(), 1);
    assert_eq!(signals(&engine, &two_places).len(), 2);
    engine.shutdown().await;
}

/// The loop guard reaches a goal's move through the goal's origin: a run that
/// spawned a goal is the one its closing is causally behind, so the listener
/// that began that run never hears the goal it made close.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_that_spawns_a_goal_never_feeds_its_own_goal_closed_start() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let child = engine
        .create_workflow(new_workflow(
            "child",
            vec![
                {
                    let mut s = step(
                        "begin",
                        StepKind::Start {
                            on: StartOn::Manual,
                            inputs: BTreeMap::new(),
                            guard: Guard::default(),
                        },
                    );
                    s.then = vec![Flow::to(sid("hold"))];
                    s
                },
                hold(),
            ],
        ))
        .unwrap();
    let spawn: Step = serde_json::from_value(json!({
        "id": "spawn",
        "name": "spawn a child",
        "kind": "spawn",
        "statement_template": "a child goal",
        "workflow": child.id.to_string(),
        "wait": false,
        "then": [{ "to": "end" }]
    }))
    .unwrap();
    let mut on = start("on", on_goal(None, vec![GoalChange::Closed]));
    on.then = vec![Flow::to(sid("spawn"))];
    let spawner = engine
        .create_workflow(new_workflow(
            "spawns on a close",
            vec![
                on,
                spawn,
                step(
                    "end",
                    StepKind::End {
                        finish: Finish::Path,
                    },
                ),
            ],
        ))
        .unwrap();
    let host = ListenerHost::Workspace {
        workflow: spawner.id,
    };
    engine.set_listening(host, BTreeMap::new(), None).unwrap();

    // Somebody else's goal closes: the listener begins a run, which spawns
    // a child goal.
    let unrelated = engine
        .submit_goal(bisa_engine::SubmitRequest::captured("unrelated"))
        .unwrap();
    engine.hear(&EngineEvent::scoped(
        unrelated.id,
        None,
        EnginePayload::GoalClosed {
            reason: ClosureReason::Abandoned { rationale: None },
        },
    ));
    assert_eq!(signals(&engine, &host).len(), 1);
    assert_eq!(engine.drain_signals().await, 1);
    let run = engine.workflow_runs(spawner.id).unwrap().remove(0);
    let run = run_finished(&engine, run.id).await;
    assert_eq!(run.outcome, Some(RunOutcome::Done));
    assert!(!run.chain.is_empty(), "the run keeps its listener's chain");
    let spawned =
        until("the child goal", || {
            engine.workspace().list_goals(None).ok()?.into_iter().find(
                |g| matches!(g.origin, bisa_core::GoalOrigin::Run { run: r, .. } if r == run.id),
            )
        })
        .await;

    // The child closes: the occurrence carries the spawning run's chain,
    // and the listener in it refuses.
    engine
        .close_goal(spawned.id, ClosureReason::Abandoned { rationale: None })
        .unwrap();
    engine.hear(&EngineEvent::scoped(
        spawned.id,
        None,
        EnginePayload::GoalClosed {
            reason: ClosureReason::Abandoned { rationale: None },
        },
    ));
    assert_eq!(
        signals(&engine, &host).len(),
        1,
        "its own child's close is never its event"
    );
    // Somebody else's is.
    let another = engine
        .submit_goal(bisa_engine::SubmitRequest::captured("another"))
        .unwrap();
    engine.hear(&EngineEvent::scoped(
        another.id,
        None,
        EnginePayload::GoalClosed {
            reason: ClosureReason::Abandoned { rationale: None },
        },
    ));
    assert_eq!(signals(&engine, &host).len(), 2);
    engine.shutdown().await;
}

/// A run's queueing is heard with its place in the line, once; a step's
/// move of a run the engine cannot place — a snapshot it cannot read — is
/// heard each time it is said, since nothing tells one saying from the next.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_start_hears_a_queued_run_and_a_steps_move_it_cannot_place() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let (_, host, _) = listening(
        &engine,
        "when a run queues or steps",
        StartOn::Run {
            filter: RunFilter {
                change: vec![RunChange::Queued, RunChange::Step].into(),
                ..RunFilter::default()
            },
        },
    );
    let run = bisa_core::RunId::from_ulid(ulid::Ulid::from_parts(5, 5));
    let workflow = bisa_core::WorkflowId::from_ulid(ulid::Ulid::from_parts(6, 6));
    let queued = EngineEvent::global(EnginePayload::RunQueued {
        run,
        workflow,
        position: 2,
    });
    engine.hear(&queued);
    engine.hear(&queued);
    let heard_now = signals(&engine, &host);
    assert_eq!(heard_now.len(), 1, "{heard_now:?}");
    let occurrence = &heard_now[0].signal;
    assert_eq!(occurrence.source, SignalSource::Run);
    assert_eq!(occurrence.payload["change"], json!("queued"));
    assert_eq!(occurrence.payload["position"], json!(2));
    assert_eq!(occurrence.payload["run"], json!(run.to_string()));
    assert_eq!(occurrence.payload["workflow"], json!(workflow.to_string()));
    assert_eq!(occurrence.payload["outcome"], json!(null));
    assert_eq!(occurrence.payload["goal"], json!(null));
    assert_eq!(occurrence.dedupe_key, Some(format!("run:{run}:queued")));

    let stepped = EngineEvent::global(EnginePayload::StepChanged {
        run,
        workflow,
        step: sid("go"),
        state: "failed".into(),
        kind: "agent".into(),
    });
    engine.hear(&stepped);
    engine.hear(&stepped);
    let all = signals(&engine, &host);
    let steps: Vec<_> = all
        .iter()
        .filter(|q| q.signal.payload["change"] == json!("step"))
        .collect();
    assert_eq!(
        steps.len(),
        2,
        "a move the engine cannot place is heard as often as it is said: {all:?}"
    );
    assert_eq!(steps[0].signal.payload["step"], json!("go"));
    assert_eq!(steps[0].signal.payload["state"], json!("failed"));
    assert_eq!(steps[0].signal.payload["kind"], json!("agent"));
    assert_eq!(steps[0].signal.payload["position"], json!(null));
    assert_eq!(steps[0].signal.dedupe_key, None);
    engine.shutdown().await;
}

/// A turn of a conversation is a session with no goal and no work item: its
/// start is the workspace's event, saying the scope it answers in; a reply
/// in a goal's thread belongs to that goal.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_start_hears_a_turn_in_a_channel_and_a_reply_in_a_goals_thread() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let ws = engine.workspace();
    let herald = ws
        .add_agent(bisa_store::NewAgent {
            name: "Herald".into(),
            harness: "mock".into(),
            ..Default::default()
        })
        .unwrap();
    let goal = engine
        .submit_goal(bisa_engine::SubmitRequest::captured("a goal"))
        .unwrap();
    let (_, host, _) = listening(
        &engine,
        "when herald starts or replies",
        StartOn::Agent {
            filter: AgentFilter {
                agent: Some(
                    ValueRef::Fixed(bisa_core::Assignee::Agent(herald.id.to_string())).into(),
                ),
                project: None,
                harness: None,
                change: vec![AgentChange::Started, AgentChange::Replied].into(),
            },
        },
    );
    let general = bisa_core::ChannelId::general().to_string();
    let live_run = bisa_engine::LiveRunId::mint();
    let frame = |state: bisa_engine::SessionState| {
        EngineEvent::global(EnginePayload::SessionState {
            live_run,
            presence: Box::new(bisa_engine::SessionPresence {
                id: live_run,
                kind: bisa_engine::registry::SessionKind::Conversation,
                origin: bisa_core::SessionOrigin::Turn {
                    scope: general.clone(),
                    on_behalf_of: None,
                },
                state,
                since: 1,
                harness: "mock".into(),
                model: None,
                effort: None,
                agent: Some(herald.id.clone()),
                session_id: None,
                work_item: None,
                conversation: None,
                goal: None,
                run: None,
                workstream: None,
                project: None,
                cwd: None,
                transcript_path: None,
                started: 1,
                pid: None,
                cost: Default::default(),
                children: vec![],
                last_activity: 1,
                revision: 1,
            }),
        })
    };
    engine.hear(&frame(bisa_engine::SessionState::Starting));
    engine.hear(&frame(bisa_engine::SessionState::Thinking));
    let begun = signals(&engine, &host);
    assert_eq!(begun.len(), 1, "{begun:?}");
    let occurrence = &begun[0].signal;
    assert_eq!(occurrence.payload["change"], json!("started"));
    assert_eq!(occurrence.payload["kind"], json!("conversation"));
    assert_eq!(occurrence.payload["scope"], json!(general));
    assert_eq!(occurrence.payload["goal"], json!(null));
    assert_eq!(occurrence.payload["work_item"], json!(null));
    assert_eq!(occurrence.payload["agent"], json!(herald.id.to_string()));
    assert_eq!(
        occurrence.dedupe_key,
        Some(format!("session:{live_run}:started"))
    );

    // A reply in the goal's thread is the goal's.
    engine.hear(&EngineEvent::global(EnginePayload::AgentReplied {
        scope: goal.id.to_string(),
        agent: herald.id.to_string(),
        posted: true,
        message: Some("m2".into()),
    }));
    let all = signals(&engine, &host);
    assert_eq!(all.len(), 2, "{all:?}");
    let reply = all
        .iter()
        .find(|q| q.signal.payload["change"] == json!("replied"))
        .unwrap();
    assert_eq!(reply.signal.payload["goal"], json!(goal.id.to_string()));
    assert_eq!(reply.signal.payload["scope"], json!(goal.id.to_string()));
    assert_eq!(reply.signal.payload["kind"], json!("conversation"));

    // Somebody else's reply is not herald's.
    engine.hear(&EngineEvent::global(EnginePayload::AgentReplied {
        scope: goal.id.to_string(),
        agent: "scribe".into(),
        posted: true,
        message: Some("m3".into()),
    }));
    assert_eq!(signals(&engine, &host).len(), 2);
    engine.shutdown().await;
}

/// A goal spawned by a goal's run closes under that run's chain: the
/// parent's own `goal closed` listener, which began the run, never hears the
/// child it made close — everyone else does.
#[tokio::test(flavor = "multi_thread")]
async fn a_goal_spawned_by_a_goals_run_closes_under_that_runs_chain() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let (parent, _) = goal_on(
        &engine,
        "a parent that listens",
        new_workflow(
            "listens",
            vec![start("on", on_goal(None, vec![GoalChange::Closed])), hold()],
        ),
    );
    let begun = engine.begin_goal(parent.id, BTreeMap::new()).unwrap();
    assert!(matches!(begun, Begun::Listening(_)), "{begun:?}");
    let parent_host = ListenerHost::Goal { goal: parent.id };
    let (_, everyone, _) = listening(
        &engine,
        "when any goal closes",
        on_goal(None, vec![GoalChange::Closed]),
    );
    let closed = |id: bisa_core::GoalId| {
        EngineEvent::scoped(
            id,
            None,
            EnginePayload::GoalClosed {
                reason: ClosureReason::Abandoned { rationale: None },
            },
        )
    };

    // Somebody's goal closes: the parent's listener begins a run, which
    // holds.
    let unrelated = engine
        .submit_goal(bisa_engine::SubmitRequest::captured("unrelated"))
        .unwrap();
    engine.hear(&closed(unrelated.id));
    assert_eq!(signals(&engine, &parent_host).len(), 1);
    assert_eq!(signals(&engine, &everyone).len(), 1);
    assert_eq!(engine.drain_signals().await, 2);
    let run = until("the parent's live run", || {
        engine.workspace().get_current_run(parent.id).ok().flatten()
    })
    .await;
    assert!(!run.chain.is_empty(), "the run keeps its listener's chain");

    // A child the run spawned — its origin names the parent — closes: the
    // occurrence is causally behind the parent's live run, and the listener
    // in that run's chain refuses.
    let child = engine
        .submit_goal(bisa_engine::SubmitRequest {
            origin: bisa_core::GoalOrigin::Spawned {
                parent: parent.id,
                step: Some(sid("hold")),
            },
            ..bisa_engine::SubmitRequest::captured("the child")
        })
        .unwrap();
    engine
        .close_goal(child.id, ClosureReason::Abandoned { rationale: None })
        .unwrap();
    engine.hear(&closed(child.id));
    assert_eq!(
        signals(&engine, &parent_host).len(),
        1,
        "its own child's close is never its event"
    );
    let heard_by_all = heard(&engine, &everyone, 2).await;
    assert_eq!(heard_by_all.len(), 2, "everyone else hears it");
    assert_eq!(
        heard_by_all[1].signal.payload["origin"],
        json!(null),
        "a close carries no origin field"
    );
    engine.shutdown().await;
}

/// A message says the kind of place it landed in: a goal's thread, a
/// conversation in a checkout — `workstream` — and any other conversation;
/// a start on a list of kinds hears those alone.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_says_the_kind_of_place_it_landed_in() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let ws = engine.workspace();
    let goal = engine
        .submit_goal(bisa_engine::SubmitRequest::captured("a goal"))
        .unwrap();
    let project = ws
        .create_project(bisa_store::NewProject::managed("api").unwrap())
        .unwrap();
    let primary = ws.primary_workstream(project.id).unwrap();
    let in_checkout = ws
        .create_conversation(bisa_store::NewConversation {
            origin: bisa_core::ConversationOrigin::Workstream {
                id: primary.id,
                project: project.id,
            },
            title: Some("in the checkout".into()),
            mode: Default::default(),
        })
        .unwrap();
    let elsewhere = ws
        .create_conversation(bisa_store::NewConversation {
            origin: bisa_core::ConversationOrigin::Workspace,
            title: Some("elsewhere".into()),
            mode: Default::default(),
        })
        .unwrap();
    let on_message = |filter: MessageFilter| StartOn::Message { filter };
    let (_, threads, _) = listening(
        &engine,
        "in a goal's thread or a checkout's",
        on_message(MessageFilter {
            from: MessageFrom::Anyone.into(),
            kind: Some(vec![MessagePlace::Goal, MessagePlace::Workstream].into()),
            ..MessageFilter::default()
        }),
    );
    let (_, anywhere, _) = listening(
        &engine,
        "anywhere",
        on_message(MessageFilter {
            from: MessageFrom::Anyone.into(),
            ..MessageFilter::default()
        }),
    );
    let say = |scope: &str, text: &str| {
        ws.post_message(
            scope,
            MessageBody::post(text.to_string()),
            None,
            &[],
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap()
    };
    let in_goal = say(&goal.id.to_string(), "on the goal");
    let in_ws = say(&in_checkout.id.to_string(), "in the checkout");
    let in_other = say(&elsewhere.id.to_string(), "elsewhere");
    let all = heard(&engine, &anywhere, 3).await;
    let kind_of = |message: &str| {
        all.iter()
            .find(|q| q.signal.payload["message"] == json!(message))
            .map(|q| q.signal.payload["scope_kind"].clone())
            .unwrap()
    };
    assert_eq!(kind_of(&in_goal), json!("goal"));
    assert_eq!(kind_of(&in_ws), json!("workstream"));
    assert_eq!(kind_of(&in_other), json!("conversation"));
    let two = heard(&engine, &threads, 2).await;
    assert_eq!(two.len(), 2, "{two:?}");
    assert!(two
        .iter()
        .all(|q| q.signal.payload["message"] != json!(in_other)));
    engine.shutdown().await;
}
