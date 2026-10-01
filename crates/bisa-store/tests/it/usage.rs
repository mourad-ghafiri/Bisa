//! Nothing is deleted while something points at it, and the refusal names what.

use bisa_core::{
    AgentId, Assignee, Flow, Gate, GoalId, Home, Join, McpId, McpServerConfig, MessageFilter,
    OnFail, RosterPolicy, RunEnd, RunFilter, RunScope, SkillId, StartOn, Step, StepId, StepKind,
    Tags, TeamId, ToolTier, ValueRef, WorkItemId, WorkItemSpec, WorkItemState, WorkItemTransition,
    WorkflowId, DEFAULT_MAX_VISITS,
};
use bisa_store::{
    GatePolicy, MemoryKeyStore, NewAgent, NewGoal, NewMcp, NewProject, NewSkill, NewWorkflow,
    StoreError, UsageKind, Workspace,
};
use std::collections::BTreeMap;

fn ws() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    (dir, ws)
}

fn new_agent(name: &str) -> NewAgent {
    NewAgent {
        name: name.into(),
        system_prompt: "x".into(),
        harness: "mock".into(),
        ..Default::default()
    }
}

fn new_skill(id: &str) -> NewSkill {
    NewSkill {
        id: SkillId::new(id).unwrap(),
        name: id.into(),
        description: "when".into(),
        tags: Tags::default(),
        markdown: "# body".into(),
    }
}

fn new_mcp(id: &str) -> NewMcp {
    NewMcp {
        id: McpId::new(id).unwrap(),
        description: String::new(),
        tags: Tags::default(),
        transport: McpServerConfig::Http {
            name: id.into(),
            url: "http://127.0.0.1:1".into(),
            headers: Default::default(),
        },
    }
}

fn agent_files(ws: &Workspace) -> Vec<(String, Vec<u8>)> {
    let mut out: Vec<(String, Vec<u8>)> = std::fs::read_dir(ws.paths().agents_dir())
        .unwrap()
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| {
            (
                e.file_name().to_string_lossy().into_owned(),
                std::fs::read(e.path()).unwrap(),
            )
        })
        .collect();
    out.sort();
    out
}

fn work_item(
    goal: GoalId,
    assignees: Vec<Assignee>,
    agent: Option<&str>,
    spawn: &[&str],
) -> WorkItemSpec {
    WorkItemSpec {
        id: WorkItemId::from_ulid(ulid::Ulid::from_datetime(std::time::SystemTime::now())),
        home: Home::Goal { goal },
        run: None,
        step: None,
        instructions: "build the thing".into(),
        state: WorkItemState::Open,
        project: None,
        harness_candidates: vec!["mock".into()],
        model: None,
        effort: None,
        output_schema: None,
        budget: Default::default(),
        assignees,
        tier_ceiling: ToolTier::Write,
        agent: agent.map(str::to_string),
        spawn_allowlist: spawn.iter().map(|s| s.to_string()).collect(),
        depth_budget: 0,
        result_attempts: 0,
        interruptions: 0,
    }
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

fn end() -> Step {
    step(
        "end",
        StepKind::End {
            finish: bisa_core::Finish::Done,
        },
        &[],
    )
}

/// A one-step workflow whose first step names `who` (an agent step for an
/// agent, a notify with a mention for a team).
fn workflow_naming(name: &str, who: &Assignee) -> NewWorkflow {
    let first = match who {
        Assignee::Agent(_) => step(
            "work",
            StepKind::Agent {
                instructions: "do it".into(),
                assignee: Some(ValueRef::Fixed(who.clone())),
                project: None,
                harness: vec![],
                model: None,
                effort: None,
                output_schema: None,
                tier_ceiling: ToolTier::Write,
            },
            &["end"],
        ),
        _ => step(
            "work",
            StepKind::Notify {
                scope: None,
                template: "hi".into(),
                mentions: vec![ValueRef::Fixed(who.clone())],
                author: None,
            },
            &["end"],
        ),
    };
    NewWorkflow {
        name: name.into(),
        description: String::new(),
        inputs: vec![],
        steps: vec![first, end()],
        tags: Tags::default(),
        decision_making: false,
    }
}

fn notify_into(name: &str, scope: &str) -> NewWorkflow {
    NewWorkflow {
        name: name.into(),
        description: String::new(),
        inputs: vec![],
        steps: vec![
            step(
                "post",
                StepKind::Notify {
                    scope: Some(scope.into()),
                    template: "{goal.statement}".into(),
                    mentions: vec![],
                    author: None,
                },
                &["end"],
            ),
            end(),
        ],
        tags: Tags::default(),
        decision_making: false,
    }
}

fn spawning(name: &str, target: WorkflowId) -> NewWorkflow {
    NewWorkflow {
        name: name.into(),
        description: String::new(),
        inputs: vec![],
        steps: vec![
            step(
                "child",
                StepKind::Spawn {
                    statement_template: "child of {goal.statement}".into(),
                    workflow: Some(target),
                    assignees: vec![],
                    inputs: Default::default(),
                    wait: false,
                },
                &["end"],
            ),
            end(),
        ],
        tags: Tags::default(),
        decision_making: false,
    }
}

/// A workflow that begins on `on` and posts: what a start holds is what it
/// names.
fn starting_on(name: &str, on: StartOn) -> NewWorkflow {
    NewWorkflow {
        name: name.into(),
        description: String::new(),
        inputs: vec![],
        steps: vec![
            step(
                "start",
                StepKind::Start {
                    on,
                    inputs: BTreeMap::new(),
                    guard: Default::default(),
                },
                &["post"],
            ),
            step(
                "post",
                StepKind::Notify {
                    scope: None,
                    template: "heard".into(),
                    mentions: vec![],
                    author: None,
                },
                &["end"],
            ),
            end(),
        ],
        tags: Tags::default(),
        decision_making: false,
    }
}

fn remove(ws: &Workspace, kind: UsageKind, id: &str) -> Result<(), StoreError> {
    match kind {
        UsageKind::Agent => ws.remove_agent(&AgentId::new(id).unwrap()),
        UsageKind::Team => ws.remove_team(&TeamId::new(id).unwrap()),
        UsageKind::Skill => ws.remove_skill(&SkillId::new(id).unwrap()),
        UsageKind::Mcp => ws.remove_mcp(&McpId::new(id).unwrap()),
        UsageKind::Channel => {
            let c = ws.get_channel(&bisa_core::ChannelId::new(id).unwrap())?;
            ws.delete_channel(bisa_core::DeletableChannel::new(c)?)
        }
        UsageKind::Workflow => ws.delete_workflow(id.parse().unwrap()),
        UsageKind::Connector => ws.remove_connector(&bisa_core::ConnectorId::new(id).unwrap()),
    }
}

/// The delete is refused while `holder` names the object, and the refusal
/// says so; after `release`, it succeeds.
fn refuses_then_allows(
    ws: &Workspace,
    kind: UsageKind,
    id: &str,
    holder_word: &str,
    release: impl FnOnce(),
) {
    let err = remove(ws, kind, id)
        .expect_err("should be refused")
        .to_string();
    assert!(err.contains(holder_word), "{err}");
    assert!(err.contains("first"), "a refusal gives the remedy: {err}");
    assert!(!ws.usage_of(kind, id).unwrap().is_empty());
    release();
    assert!(ws.usage_of(kind, id).unwrap().is_empty());
    remove(ws, kind, id).unwrap();
}

fn an_agent(ws: &Workspace) -> AgentId {
    ws.add_agent(new_agent("Scribe")).unwrap().id
}

#[test]
fn a_skill_an_agent_carries_cannot_be_removed_and_no_definition_changes() {
    let (_d, ws) = ws();
    ws.create_skill(new_skill("checklist")).unwrap();
    let a = an_agent(&ws);
    ws.attach_skill(&a, &SkillId::new("checklist").unwrap())
        .unwrap();
    let before = agent_files(&ws);
    assert!(ws
        .remove_skill(&SkillId::new("checklist").unwrap())
        .is_err());
    assert_eq!(
        agent_files(&ws),
        before,
        "a refused delete rewrites nothing"
    );
    refuses_then_allows(&ws, UsageKind::Skill, "checklist", "agent", || {
        ws.detach_skill(&a, &SkillId::new("checklist").unwrap())
            .unwrap();
    });
}

#[test]
fn an_mcp_server_an_agent_carries_cannot_be_removed() {
    let (_d, ws) = ws();
    ws.create_mcp(new_mcp("postgres")).unwrap();
    let a = an_agent(&ws);
    ws.attach_mcp(&a, &McpId::new("postgres").unwrap()).unwrap();
    let before = agent_files(&ws);
    assert!(ws.remove_mcp(&McpId::new("postgres").unwrap()).is_err());
    assert_eq!(agent_files(&ws), before);
    refuses_then_allows(&ws, UsageKind::Mcp, "postgres", "agent", || {
        ws.detach_mcp(&a, &McpId::new("postgres").unwrap()).unwrap();
    });
}

#[test]
fn an_agent_a_team_or_a_channel_rosters_cannot_be_removed() {
    let (_d, ws) = ws();
    let a = an_agent(&ws);
    let team = ws
        .create_team(
            "Eng",
            None,
            vec![Assignee::Agent(a.to_string())],
            Tags::default(),
        )
        .unwrap();
    let channel = ws
        .create_channel(
            "room",
            None,
            RosterPolicy::Listed {
                agents: vec![a.clone()],
                teams: vec![],
                humans: vec![],
            },
            Tags::default(),
        )
        .unwrap();
    let usage = ws.usage_of(UsageKind::Agent, a.as_str()).unwrap();
    assert_eq!(usage.len(), 2, "{usage:?}");
    refuses_then_allows(&ws, UsageKind::Agent, a.as_str(), "team", || {
        let mut t = ws.get_team(&team.id).unwrap();
        t.members.clear();
        ws.update_team(t).unwrap();
        ws.update_channel(&channel.id, None, RosterPolicy::default(), Tags::default())
            .unwrap();
    });
}

#[test]
fn an_agent_a_goal_or_a_project_assigns_cannot_be_removed() {
    let (_d, ws) = ws();
    let a = an_agent(&ws);
    let goal = ws
        .create_goal(NewGoal::captured("Ship the landing page").title("Landing"))
        .unwrap();
    ws.set_goal_assignees(goal.id, vec![Assignee::Agent(a.to_string())])
        .unwrap();
    let mut new = NewProject::managed("web").unwrap();
    new.assignees = vec![Assignee::Agent(a.to_string())];
    let p = ws.create_project(new).unwrap();
    refuses_then_allows(&ws, UsageKind::Agent, a.as_str(), "goal", || {
        ws.set_goal_assignees(goal.id, vec![]).unwrap();
        let mut p = ws.get_project(p.id).unwrap();
        p.assignees.clear();
        ws.update_project(p).unwrap();
    });
}

#[test]
fn an_agent_mid_work_item_cannot_be_removed_and_can_be_once_it_settles() {
    let (_d, ws) = ws();
    let a = an_agent(&ws);
    let goal = ws.create_goal(NewGoal::captured("g")).unwrap();
    let running = work_item(goal.id, vec![], Some(a.as_str()), &[]);
    ws.put_work_item(&running).unwrap();
    let spawner = work_item(goal.id, vec![], None, &[a.as_str(), "*"]);
    ws.put_work_item(&spawner).unwrap();
    let assigned = work_item(goal.id, vec![Assignee::Agent(a.to_string())], None, &[]);
    ws.put_work_item(&assigned).unwrap();
    assert_eq!(ws.usage_of(UsageKind::Agent, a.as_str()).unwrap().len(), 3);
    refuses_then_allows(&ws, UsageKind::Agent, a.as_str(), "work item", || {
        for w in [&running, &spawner, &assigned] {
            ws.transition_work_item(
                &Home::Goal { goal: goal.id },
                w.id,
                &WorkItemTransition::Cancel,
            )
            .unwrap();
        }
    });
}

#[test]
fn an_agent_or_team_a_workflow_step_names_cannot_be_removed() {
    let (_d, ws) = ws();
    let a = an_agent(&ws);
    let team = ws
        .create_team("Ops", None, vec![], Tags::default())
        .unwrap();
    let by_agent = ws
        .create_workflow(
            workflow_naming("Uses the agent", &Assignee::Agent(a.to_string())),
            bisa_core::WorkflowOrigin::Workspace,
        )
        .unwrap();
    let by_team = ws
        .create_workflow(
            workflow_naming("Mentions the team", &Assignee::Team(team.id.to_string())),
            bisa_core::WorkflowOrigin::Workspace,
        )
        .unwrap();
    refuses_then_allows(&ws, UsageKind::Agent, a.as_str(), "workflow", || {
        ws.delete_workflow(by_agent.id).unwrap();
    });
    refuses_then_allows(&ws, UsageKind::Team, team.id.as_str(), "workflow", || {
        ws.delete_workflow(by_team.id).unwrap();
    });
}

#[test]
fn an_agent_or_team_a_gate_policy_lists_cannot_be_removed() {
    let (_d, ws) = ws();
    let a = ws.add_agent(new_agent("Signer")).unwrap();
    let team = ws
        .create_team("Board", None, vec![], Tags::default())
        .unwrap();
    ws.set_gate_policy(
        Gate::Publish,
        GatePolicy::Listed(vec![
            a.pubkey.as_hex().to_string(),
            format!("team:{}", team.id),
        ]),
    )
    .unwrap();
    refuses_then_allows(&ws, UsageKind::Agent, a.id.as_str(), "gate policy", || {
        ws.set_gate_policy(
            Gate::Publish,
            GatePolicy::Listed(vec![format!("team:{}", team.id)]),
        )
        .unwrap();
    });
    refuses_then_allows(
        &ws,
        UsageKind::Team,
        team.id.as_str(),
        "gate policy",
        || {
            ws.set_gate_policy(Gate::Publish, GatePolicy::Owner)
                .unwrap();
        },
    );
}

#[test]
fn a_channel_a_workflow_notifies_into_cannot_be_removed() {
    let (_d, ws) = ws();
    let c = ws
        .create_channel("alerts", None, RosterPolicy::default(), Tags::default())
        .unwrap();
    let wf = ws
        .create_workflow(
            notify_into("Alerting", c.id.as_str()),
            bisa_core::WorkflowOrigin::Workspace,
        )
        .unwrap();
    refuses_then_allows(&ws, UsageKind::Channel, c.id.as_str(), "workflow", || {
        ws.delete_workflow(wf.id).unwrap();
    });
}

#[test]
fn a_channel_a_message_start_listens_in_cannot_be_removed() {
    let (_d, ws) = ws();
    let c = ws
        .create_channel("support", None, RosterPolicy::default(), Tags::default())
        .unwrap();
    let wf = ws
        .create_workflow(
            starting_on(
                "Answers support",
                StartOn::Message {
                    filter: MessageFilter {
                        r#in: Some(c.id.as_str().to_string()),
                        ..MessageFilter::default()
                    },
                },
            ),
            bisa_core::WorkflowOrigin::Workspace,
        )
        .unwrap();
    refuses_then_allows(&ws, UsageKind::Channel, c.id.as_str(), "workflow", || {
        ws.delete_workflow(wf.id).unwrap();
    });
}

#[test]
fn a_workflow_a_goal_a_run_start_or_a_spawn_step_uses_cannot_be_removed() {
    let (_d, ws) = ws();
    let wf = ws
        .create_workflow(
            notify_into("Target", "general"),
            bisa_core::WorkflowOrigin::Workspace,
        )
        .unwrap();
    let id = wf.id.to_string();
    let goal = ws.create_goal(NewGoal::captured("uses it")).unwrap();
    ws.set_goal_workflow(goal.id, Some(wf.id)).unwrap();
    let hears = ws
        .create_workflow(
            starting_on(
                "Hears it fail",
                StartOn::Run {
                    filter: RunFilter {
                        workflow: Some(wf.id),
                        outcome: Some(RunEnd::Failed),
                    },
                },
            ),
            bisa_core::WorkflowOrigin::Workspace,
        )
        .unwrap();
    let parent = ws
        .create_workflow(
            spawning("Spawns it", wf.id),
            bisa_core::WorkflowOrigin::Workspace,
        )
        .unwrap();
    let usage = ws.usage_of(UsageKind::Workflow, &id).unwrap();
    assert_eq!(usage.len(), 3, "{usage:?}");
    // The goal's reference is not live: it points at the workflow but runs
    // nothing yet. A run started on it is live until it finishes — the
    // designer goes read-only on that word.
    let by_goal = |ws: &Workspace| {
        ws.usage_of(UsageKind::Workflow, &id)
            .unwrap()
            .iter()
            .find(|r| r.kind == bisa_store::ReferenceKind::Goal)
            .cloned()
            .expect("the goal is a holder")
    };
    assert!(!by_goal(&ws).live, "no run, not live");
    let (run, _) = ws
        .create_run(
            RunScope::Goal { goal: goal.id },
            wf.id,
            BTreeMap::new(),
            bisa_core::RunEntry::by_hand(),
            None,
        )
        .unwrap();
    assert!(by_goal(&ws).live, "an unfinished run is live");
    ws.record_run_event(
        run.id,
        bisa_core::RunEvent::Cancel {
            cause: bisa_core::CancelCause::Stopped { rationale: None },
        },
    )
    .unwrap();
    assert!(!by_goal(&ws).live, "a cancelled run is finished");
    refuses_then_allows(&ws, UsageKind::Workflow, &id, "goal", || {
        ws.set_goal_workflow(goal.id, None).unwrap();
        ws.delete_workflow(hears.id).unwrap();
        ws.delete_workflow(parent.id).unwrap();
    });
}

#[test]
fn usage_of_nothing_is_empty_an_unknown_id_is_an_error_and_core_agents_are_refused_on_their_own_grounds(
) {
    let (_d, ws) = ws();
    let a = an_agent(&ws);
    assert!(ws
        .usage_of(UsageKind::Agent, a.as_str())
        .unwrap()
        .is_empty());
    assert!(ws.usage_of(UsageKind::Agent, "nobody").is_err());
    assert!(ws.usage_of(UsageKind::Skill, "nothing").is_err());
    assert!(ws
        .usage_of(UsageKind::Workflow, "01ARZ3NDEKTSV4RRFFQ69G5FAV")
        .is_err());
    for core in AgentId::CORE {
        let id = AgentId::new(core).unwrap();
        assert!(ws
            .usage_of(UsageKind::Agent, id.as_str())
            .unwrap()
            .is_empty());
        assert!(matches!(
            ws.remove_agent(&id),
            Err(StoreError::Agent(
                bisa_core::AgentError::CoreCannotBeRemoved
            ))
        ));
    }
}

#[test]
fn the_refusal_names_holders_counts_the_rest_and_gives_the_remedy() {
    let (_d, ws) = ws();
    let a = an_agent(&ws);
    for i in 0..4 {
        let g = ws
            .create_goal(NewGoal::captured(&format!("goal {i}")))
            .unwrap();
        ws.set_goal_assignees(g.id, vec![Assignee::Agent(a.to_string())])
            .unwrap();
    }
    let err = ws.remove_agent(&a).unwrap_err().to_string();
    assert!(err.contains("and 2 others"), "{err}");
    assert!(err.contains("Unassign it from the goal"), "{err}");
    assert_eq!(ws.list_agents().unwrap().len(), 3, "nothing was removed");
}

/// An edit that sends a secret back masked keeps the stored value, one
/// that sends a new value writes it, and a masked key the store never had
/// is dropped — the registry's write-once rule, applied where the file is.
#[test]
fn an_edit_that_keeps_a_masked_value_keeps_the_stored_one() {
    let (_dir, ws) = ws();
    let id = McpId::new("docs").unwrap();
    ws.create_mcp(NewMcp {
        id: id.clone(),
        description: String::new(),
        tags: Tags::default(),
        transport: McpServerConfig::Http {
            name: "docs".into(),
            url: "https://mcp.example.test/mcp".into(),
            headers: BTreeMap::from([("Authorization".to_string(), "Bearer real".to_string())]),
        },
    })
    .unwrap();
    let mut def = ws.get_mcp(&id).unwrap();
    def.transport = McpServerConfig::Http {
        name: "docs".into(),
        url: "https://mcp.example.test/v2/mcp".into(),
        headers: BTreeMap::from([
            ("Authorization".to_string(), bisa_core::MCP_MASK.to_string()),
            ("X-Tenant".to_string(), "acme".to_string()),
            ("X-Ghost".to_string(), bisa_core::MCP_MASK.to_string()),
        ]),
    };
    let saved = ws.update_mcp(def).unwrap();
    let headers = saved.transport.headers().unwrap();
    assert_eq!(
        headers["Authorization"], "Bearer real",
        "the mask kept the stored value"
    );
    assert_eq!(headers["X-Tenant"], "acme");
    assert!(
        !headers.contains_key("X-Ghost"),
        "a mask for a key never stored means nothing"
    );
    assert_eq!(
        saved.transport.url(),
        Some("https://mcp.example.test/v2/mcp")
    );
    let again = ws.get_mcp(&id).unwrap();
    assert_eq!(
        again.transport, saved.transport,
        "the truth file holds the real value"
    );
}
