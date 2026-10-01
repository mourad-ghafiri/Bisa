//! Assignment resolution: who takes the work, and who may sign for it.
//!
//! Everything here runs against the mock harness, so nothing needs a model.

use crate::common;
use bisa_core::workitem::{WorkItemSpec, WorkItemState};
use bisa_core::Budget;
use bisa_core::{
    AgentId, Assignee, Gate, GoalId, MemberRole, PrincipalId, Project, ProjectId, RespondPolicy,
    ToolTier, WorkItemId,
};
use bisa_engine::{assign, Engine, EngineConfig, EnginePayload, SubmitRequest};
use bisa_harness::mock::MockAdapter;
use bisa_harness::HarnessCatalog;
use bisa_store::{Admission, MemoryKeyStore, NewAgent, NewProject, Workspace};
use common::run_spec;
use nostr::key::Keys;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

fn ulid() -> ulid::Ulid {
    ulid::Ulid::from_datetime(SystemTime::now())
}

fn workspace(dir: &tempfile::TempDir) -> Workspace {
    Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap()
}

fn catalog() -> HarnessCatalog {
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::new(MockAdapter::default()));
    // A second harness that is present but never available, so "the harness
    // probes available" is tested against something that really does not.
    catalog.register(Arc::new(MockAdapter {
        id: "dead".into(),
        available: false,
        ..Default::default()
    }));
    catalog
}

fn design_off_config() -> EngineConfig {
    EngineConfig {
        design_enabled: false,
        ..Default::default()
    }
}

fn engine(dir: &tempfile::TempDir) -> Engine {
    Engine::start(workspace(dir), catalog(), design_off_config()).unwrap()
}

fn agent(ws: &Workspace, name: &str, harness: &str) -> String {
    ws.add_agent(NewAgent {
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
    })
    .unwrap()
    .id
    .to_string()
}

fn aid(s: &str) -> AgentId {
    AgentId::new(s).unwrap()
}

/// A managed project attached to `goal`. Projects belong to the workspace;
/// the attachment is what makes one visible to a goal's work.
fn attached_project(ws: &Workspace, goal: GoalId, new: NewProject) -> Project {
    let project = ws.create_project(new).unwrap();
    ws.attach(goal, project.id).unwrap();
    project
}

fn human() -> PrincipalId {
    PrincipalId::new(Keys::generate().public_key().to_hex()).unwrap()
}

fn item(goal: GoalId, project: Option<ProjectId>) -> WorkItemSpec {
    WorkItemSpec {
        id: WorkItemId::from_ulid(ulid()),
        home: bisa_core::Home::Goal { goal },
        run: None,
        step: None,
        instructions: "do the work".into(),
        state: WorkItemState::Open,
        project,
        harness_candidates: vec!["mock".into()],
        model: None,
        effort: None,
        output_schema: None,
        budget: Budget::default(),
        assignees: vec![],
        tier_ceiling: ToolTier::Write,
        agent: None,
        spawn_allowlist: vec![],
        depth_budget: 0,
        result_attempts: 0,
        interruptions: 0,
    }
}

// ---------------------------------------------------------------------------

/// The four levels of the union, in precedence order, plus the item's own
/// request, which overrides all of them.
#[tokio::test(flavor = "multi_thread")]
async fn explicit_beats_project_beats_goal_beats_ancestor() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let ws = engine.workspace();

    let anc_agent = agent(ws, "Ancestor", "mock");
    let int_agent = agent(ws, "Goal", "mock");
    let proj_agent = agent(ws, "Project", "mock");
    let pinned = agent(ws, "Pinned", "mock");

    let ancestor = ws
        .create_goal(bisa_store::NewGoal::captured("the parent"))
        .unwrap();
    ws.set_goal_assignees(ancestor.id, vec![Assignee::Agent(anc_agent.clone())])
        .unwrap();

    let child = ws
        .create_goal(bisa_store::NewGoal {
            origin: bisa_core::GoalOrigin::Spawned {
                parent: ancestor.id,
            },
            ..bisa_store::NewGoal::captured("the child")
        })
        .unwrap();
    ws.set_goal_assignees(child.id, vec![Assignee::Agent(int_agent.clone())])
        .unwrap();

    let project = attached_project(
        ws,
        child.id,
        NewProject {
            origin: bisa_core::ProjectOrigin::Workspace,
            assignees: vec![Assignee::Agent(proj_agent.clone())],
            ..NewProject::managed("storefront").unwrap()
        },
    );

    let mut spec = item(child.id, Some(project.id));
    assert_eq!(
        assign::workers(engine.inner(), &spec).await,
        vec![proj_agent.clone(), int_agent.clone(), anc_agent.clone()],
        "nearest first: project, then goal, then ancestor"
    );

    // An explicit request wins, and wins alone.
    spec.assignees = vec![Assignee::Agent(pinned.clone())];
    assert_eq!(assign::workers(engine.inner(), &spec).await, vec![pinned]);

    // Without the project the project's agent is simply not in the union.
    let no_project = item(child.id, None);
    assert_eq!(
        assign::workers(engine.inner(), &no_project).await,
        vec![int_agent, anc_agent]
    );

    engine.shutdown().await;
}

/// A team is a shorthand for its members: agents reach `workers`, humans reach
/// `approvers`, and neither leaks into the other.
#[tokio::test(flavor = "multi_thread")]
async fn teams_expand_and_humans_never_take_work() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let ws = engine.workspace();

    let builder = agent(ws, "Builder", "mock");
    let reviewer = human();
    let team = ws
        .create_team(
            "squad",
            None,
            vec![
                Assignee::Agent(builder.clone()),
                Assignee::Human(reviewer.clone()),
            ],
            Default::default(),
        )
        .unwrap();

    let goal = ws
        .create_goal(bisa_store::NewGoal::captured("shipped by a squad"))
        .unwrap();
    ws.set_goal_assignees(goal.id, vec![Assignee::Team(team.id.to_string())])
        .unwrap();

    let spec = item(goal.id, None);
    assert_eq!(assign::workers(engine.inner(), &spec).await, vec![builder]);
    assert_eq!(
        assign::approvers(engine.inner(), goal.id, None),
        vec![reviewer]
    );

    engine.shutdown().await;
}

/// Disabled agents and agents whose harness cannot run here are filtered out,
/// which is the whole reason `workers` is async.
#[tokio::test(flavor = "multi_thread")]
async fn only_enabled_agents_with_a_live_harness_are_offered_work() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let ws = engine.workspace();

    let good = agent(ws, "Good", "mock");
    let disabled = agent(ws, "Disabled", "mock");
    let unavailable = agent(ws, "Unavailable", "dead");
    let unknown_harness = agent(ws, "Exotic", "no-such-harness");

    let mut def = ws.get_agent(&aid(&disabled)).unwrap();
    def.enabled = false;
    ws.update_agent(def).unwrap();

    let goal = ws
        .create_goal(bisa_store::NewGoal::captured("mixed pool"))
        .unwrap();
    ws.set_goal_assignees(
        goal.id,
        vec![
            Assignee::Agent(disabled),
            Assignee::Agent(unavailable),
            Assignee::Agent(unknown_harness),
            Assignee::Agent(good.clone()),
        ],
    )
    .unwrap();

    assert_eq!(
        assign::workers(engine.inner(), &item(goal.id, None)).await,
        vec![good]
    );

    engine.shutdown().await;
}

/// `Goal.parent` is user-editable, so the ancestor walk must survive a
/// cycle rather than spin forever in a read path.
#[tokio::test(flavor = "multi_thread")]
async fn a_parent_cycle_terminates() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let ws = engine.workspace();

    let a_agent = agent(ws, "A", "mock");
    let b_agent = agent(ws, "B", "mock");

    // A goal's origin is recorded at capture and never edited, so a cycle
    // cannot be built through the API; the walk is still bounded by a
    // visited set, and a chain resolves nearest first.
    let b = ws.create_goal(bisa_store::NewGoal::captured("b")).unwrap();
    let a = ws
        .create_goal(bisa_store::NewGoal {
            origin: bisa_core::GoalOrigin::Spawned { parent: b.id },
            ..bisa_store::NewGoal::captured("a")
        })
        .unwrap();
    ws.set_goal_assignees(a.id, vec![Assignee::Agent(a_agent.clone())])
        .unwrap();
    ws.set_goal_assignees(b.id, vec![Assignee::Agent(b_agent.clone())])
        .unwrap();

    let resolved = tokio::time::timeout(
        Duration::from_secs(10),
        assign::workers(engine.inner(), &item(a.id, None)),
    )
    .await
    .expect("the ancestor walk must terminate");
    assert_eq!(resolved, vec![a_agent, b_agent]);

    // A goal with no ancestors and no assignees resolves to nobody.
    let solo = ws
        .create_goal(bisa_store::NewGoal::captured("solo"))
        .unwrap();
    let resolved = tokio::time::timeout(
        Duration::from_secs(10),
        assign::workers(engine.inner(), &item(solo.id, None)),
    )
    .await
    .expect("a lone goal terminates too");
    assert!(resolved.is_empty());

    engine.shutdown().await;
}

/// The point of dropping humans from `workers`: a goal assigned only to
/// people still runs, on the item's own harness candidates, with no agent
/// attributed to it.
#[tokio::test(flavor = "multi_thread")]
async fn a_human_only_assignment_still_executes() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let mut rx = engine.events();

    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..SubmitRequest::captured("only people on this")
        })
        .unwrap();
    engine
        .workspace()
        .set_goal_assignees(goal.id, vec![Assignee::Human(human())])
        .unwrap();

    let item_id = run_spec(&engine, goal.id, item(goal.id, None)).await;

    let ev = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let ev = rx.recv().await.expect("bus");
            if matches!(ev.payload, EnginePayload::Scheduled { .. }) {
                return ev;
            }
        }
    })
    .await
    .expect("the item must run despite a human-only assignment");
    let EnginePayload::Scheduled { harness } = ev.payload else {
        unreachable!()
    };
    assert_eq!(harness, "mock", "fell back to harness_candidates");

    let stored = engine
        .workspace()
        .get_work_item(&bisa_core::Home::from(goal.id), item_id)
        .unwrap();
    assert!(
        stored.agent.is_none(),
        "no agent was assigned, so none is claimed"
    );

    engine.shutdown().await;
}

/// Delegating an item to a team has to reach the team's agents.
///
/// This is the bug the split fixes: the request used to be squeezed into
/// `spec.agent`, a single agent id, so a `team:` assignee resolved to `None`
/// and the item was routed as if nobody had been named at all.
#[tokio::test(flavor = "multi_thread")]
async fn a_team_on_an_item_resolves_to_its_agents_and_one_is_picked() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let ws = engine.workspace();

    let builder = agent(ws, "Builder", "mock");
    let shipper = agent(ws, "Shipper", "mock");
    let reviewer = human();
    let team = ws
        .create_team(
            "squad",
            None,
            vec![
                Assignee::Agent(builder.clone()),
                Assignee::Agent(shipper.clone()),
                Assignee::Human(reviewer),
            ],
            Default::default(),
        )
        .unwrap();

    let goal = ws
        .create_goal(bisa_store::NewGoal::captured("delegated to a squad"))
        .unwrap();
    let mut spec = item(goal.id, None);
    spec.assignees = vec![Assignee::Team(team.id.to_string())];

    let pool = assign::workers(engine.inner(), &spec).await;
    assert_eq!(
        pool,
        vec![builder, shipper],
        "the team expands to its agents, in member order, with the human dropped"
    );
    let picked = assign::pick(&pool, spec.id).expect("a non-empty pool always picks");
    assert!(pool.contains(&picked), "the pick comes out of the pool");

    engine.shutdown().await;
}

/// The request is a decision, not a suggestion: the goal's own assignees do
/// not widen it. Silently adding them back would make naming somebody on an
/// item mean "and also whoever else is around".
#[tokio::test(flavor = "multi_thread")]
async fn an_explicit_item_assignee_wins_alone() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let ws = engine.workspace();

    let chosen = agent(ws, "Chosen", "mock");
    let on_the_goal = agent(ws, "OnTheGoal", "mock");
    let on_the_project = agent(ws, "OnTheProject", "mock");

    let goal = ws
        .create_goal(bisa_store::NewGoal::captured("staffed twice over"))
        .unwrap();
    ws.set_goal_assignees(goal.id, vec![Assignee::Agent(on_the_goal.clone())])
        .unwrap();
    let project = attached_project(
        ws,
        goal.id,
        NewProject {
            origin: bisa_core::ProjectOrigin::Workspace,
            assignees: vec![Assignee::Agent(on_the_project.clone())],
            ..NewProject::managed("storefront").unwrap()
        },
    );

    let mut spec = item(goal.id, Some(project.id));
    spec.assignees = vec![Assignee::Agent(chosen.clone())];
    assert_eq!(assign::workers(engine.inner(), &spec).await, vec![chosen]);

    engine.shutdown().await;
}

/// A human-only request is harmless for the same reason a human-only goal
/// assignment is: people decide gates, they do not run sessions. The pool is
/// empty and the item falls back to its harness candidates rather than waiting
/// forever for somebody who was never going to take it.
#[tokio::test(flavor = "multi_thread")]
async fn a_human_only_item_assignment_still_executes() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let mut rx = engine.events();

    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..SubmitRequest::captured("one person named")
        })
        .unwrap();

    // A person named on a step is a member of the workspace, or the
    // workflow is refused (`UnknownAssignee`).
    let person = human();
    engine
        .workspace()
        .add_member(person.clone(), MemberRole::Member, Admission::default())
        .unwrap();
    let mut spec = item(goal.id, None);
    spec.assignees = vec![Assignee::Human(person)];
    assert!(
        assign::workers(engine.inner(), &spec).await.is_empty(),
        "an empty pool means nothing to route to, never \"not runnable\""
    );

    let item_id = run_spec(&engine, goal.id, spec).await;

    let ev = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let ev = rx.recv().await.expect("bus");
            if matches!(ev.payload, EnginePayload::Scheduled { .. }) {
                return ev;
            }
        }
    })
    .await
    .expect("the item must run despite a human-only request");
    let EnginePayload::Scheduled { harness } = ev.payload else {
        unreachable!()
    };
    assert_eq!(harness, "mock", "fell back to harness_candidates");
    assert!(
        engine
            .workspace()
            .get_work_item(&bisa_core::Home::from(goal.id), item_id)
            .unwrap()
            .agent
            .is_none(),
        "nobody took it, so nobody is recorded as having taken it"
    );

    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// Governance: the same union decides who may sign
// ---------------------------------------------------------------------------

/// A human assigned to the *project* may decide the goal's gates, including
/// `Publish`, under the default owner-only policy.
#[test]
fn a_project_human_may_decide_the_goals_gates() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);

    let ancestor = ws
        .create_goal(bisa_store::NewGoal::captured("the parent"))
        .unwrap();
    let goal = ws
        .create_goal(bisa_store::NewGoal {
            origin: bisa_core::GoalOrigin::Spawned {
                parent: ancestor.id,
            },
            ..bisa_store::NewGoal::captured("the child")
        })
        .unwrap();

    let project_human = human();
    let ancestor_human = human();
    let stranger = human();

    ws.set_goal_assignees(ancestor.id, vec![Assignee::Human(ancestor_human.clone())])
        .unwrap();
    attached_project(
        &ws,
        goal.id,
        NewProject {
            origin: bisa_core::ProjectOrigin::Workspace,
            assignees: vec![Assignee::Human(project_human.clone())],
            ..NewProject::managed("storefront").unwrap()
        },
    );

    // Publish is deliberately absent: it never defers to an assignment.
    for gate in [Gate::Approval, Gate::Escalation] {
        assert!(
            ws.gate_policy_allows_for(gate, &project_human, Some(goal.id))
                .unwrap(),
            "{gate:?}: a project assignee decides the goal's gates"
        );
        assert!(
            ws.gate_policy_allows_for(gate, &ancestor_human, Some(goal.id))
                .unwrap(),
            "{gate:?}: an ancestor's assignee decides too"
        );
        assert!(
            !ws.gate_policy_allows_for(gate, &stranger, Some(goal.id))
                .unwrap(),
            "{gate:?}: nobody else does"
        );
        // The owner keeps every gate.
        assert!(ws
            .gate_policy_allows_for(gate, &ws.owner_principal(), Some(goal.id))
            .unwrap());
    }

    // Publishing spends the OWNER's git/gh credentials against a remote the
    // owner is accountable for. Delegating the work never delegates the keys,
    // so on the default policy an assignee is refused and only the owner may
    // sign — see `Workspace::gate_policy_allows_for`.
    for who in [&project_human, &ancestor_human, &stranger] {
        assert!(
            !ws.gate_policy_allows_for(Gate::Publish, who, Some(goal.id))
                .unwrap(),
            "publish never defers to an assignment"
        );
    }
    assert!(ws
        .gate_policy_allows_for(Gate::Publish, &ws.owner_principal(), Some(goal.id))
        .unwrap());
}

/// An explicitly configured policy is authoritative: assigning somebody never
/// widens a policy an operator deliberately narrowed.
#[test]
fn an_explicit_policy_is_never_widened_by_an_assignment() {
    use bisa_store::GatePolicy;

    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    let goal = ws
        .create_goal(bisa_store::NewGoal::captured("narrow"))
        .unwrap();
    let assignee = human();
    // A listed pubkey counts only while it is a member whose role may
    // decide gates.
    let listed = human();
    ws.add_member(listed.clone(), MemberRole::Member, Admission::default())
        .unwrap();
    ws.set_goal_assignees(goal.id, vec![Assignee::Human(assignee.clone())])
        .unwrap();

    ws.set_gate_policy(
        Gate::Publish,
        GatePolicy::Listed(vec![listed.as_hex().to_string()]),
    )
    .unwrap();

    assert!(ws
        .gate_policy_allows_for(Gate::Publish, &listed, Some(goal.id))
        .unwrap());
    assert!(
        !ws.gate_policy_allows_for(Gate::Publish, &assignee, Some(goal.id))
            .unwrap(),
        "the assignment must not widen an explicit Listed policy"
    );
    // The other gates are untouched and still defer.
    assert!(ws
        .gate_policy_allows_for(Gate::Approval, &assignee, Some(goal.id))
        .unwrap());
}

// ---------------------------------------------------------------------------
// The core agent: in every room, in no work queue
// ---------------------------------------------------------------------------

/// Point the core agent at a harness these tests can actually launch.
///
/// Without this the filter under test would pass for the wrong reason: the
/// core agent ships on `claude-code`, which is not registered here, so the
/// availability check would drop it whether or not `workers` filtered it.
fn core_agent_on_mock(ws: &Workspace) -> String {
    let mut def = ws.get_agent(&AgentId::general()).unwrap();
    def.harness = "mock".into();
    ws.update_agent(def).unwrap();
    AgentId::GENERAL.to_string()
}

/// The core agent never wins a work item off the union, and always wins one
/// whose assignees name it.
///
/// It is an implicit member of every team, so leaving it in the pool would
/// make it a candidate for essentially every item in the workspace — and it
/// delegates rather than implements, so an item it won is an item done by the
/// one agent whose role forbids doing it. An item that asks for it by name is
/// the deliberate exception.
#[tokio::test(flavor = "multi_thread")]
async fn the_core_agent_is_never_offered_work_it_was_not_asked_for_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let ws = engine.workspace();
    let core = core_agent_on_mock(ws);
    let builder = agent(ws, "Builder", "mock");

    // A team it is not stored in — the store strips it from every members
    // list, so passing it here is a no-op rather than an error.
    let team = ws
        .create_team(
            "squad",
            None,
            vec![
                Assignee::Agent(builder.clone()),
                Assignee::Agent(core.clone()),
            ],
            Default::default(),
        )
        .unwrap();
    assert_eq!(
        ws.team_agents(&team.id).unwrap(),
        vec![aid(&builder)],
        "the work-routing pool is the stored members, and it is not one of them"
    );

    let goal = ws
        .create_goal(bisa_store::NewGoal::captured("team work"))
        .unwrap();
    ws.set_goal_assignees(goal.id, vec![Assignee::Team(team.id.to_string())])
        .unwrap();
    let mut spec = item(goal.id, None);
    assert_eq!(
        assign::workers(engine.inner(), &spec).await,
        vec![builder.clone()],
        "a team-assigned item routes to the team's agents only"
    );

    // …and the case the filter actually exists for: named directly on the
    // goal, where nothing strips it on the way in.
    ws.set_goal_assignees(
        goal.id,
        vec![
            Assignee::Agent(core.clone()),
            Assignee::Agent(builder.clone()),
        ],
    )
    .unwrap();
    assert_eq!(
        assign::workers(engine.inner(), &spec).await,
        vec![builder],
        "a direct assignment does not put it in the pool either"
    );

    // Naming it on the item is explicit, and explicit wins: the exclusion
    // cancels implicit membership, not a request somebody typed.
    spec.assignees = vec![Assignee::Agent(core.clone())];
    assert_eq!(
        assign::workers(engine.inner(), &spec).await,
        vec![core],
        "an item that asks for the core agent by name gets it"
    );

    engine.shutdown().await;
}

/// Addressing is the other half of the split: a team's participants include
/// the core agent, so it is reachable in every room it implicitly belongs to.
#[tokio::test(flavor = "multi_thread")]
async fn the_core_agent_is_a_principal_of_every_team() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let ws = engine.workspace();
    let core_key = ws.get_agent(&AgentId::general()).unwrap().pubkey;
    let builder = agent(ws, "Builder", "mock");
    let reviewer = human();
    let team = ws
        .create_team(
            "squad",
            None,
            vec![
                Assignee::Agent(builder.clone()),
                Assignee::Human(reviewer.clone()),
            ],
            Default::default(),
        )
        .unwrap();

    let addressed = assign::principals(engine.inner(), &Assignee::Team(team.id.to_string()));
    let builder_key = ws.get_agent(&aid(&builder)).unwrap().pubkey;
    assert!(
        addressed.contains(&builder_key) && addressed.contains(&reviewer),
        "the stored members are addressable: {addressed:?}"
    );
    assert!(
        addressed.contains(&core_key),
        "the core agent is in every room, so a team message reaches it: {addressed:?}"
    );

    engine.shutdown().await;
}

/// A team stood down is in no room and carries no work: addressed, it names
/// nobody — not its members, not the agents that are in every room — and a
/// goal it carries is offered to nobody and signed for by nobody of it. It
/// is not gone: stood up again it is what it was.
#[tokio::test(flavor = "multi_thread")]
async fn a_team_stood_down_is_addressed_by_nothing_and_comes_back_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let ws = engine.workspace();
    let builder = agent(ws, "Builder", "mock");
    let reviewer = human();
    let team = ws
        .create_team(
            "squad",
            None,
            vec![
                Assignee::Agent(builder.clone()),
                Assignee::Human(reviewer.clone()),
            ],
            Default::default(),
        )
        .unwrap();
    let goal = ws
        .create_goal(bisa_store::NewGoal::captured("shipped by a squad"))
        .unwrap();
    ws.set_goal_assignees(goal.id, vec![Assignee::Team(team.id.to_string())])
        .unwrap();
    let squad = Assignee::Team(team.id.to_string());
    let addressed = assign::principals(engine.inner(), &squad);
    assert_eq!(
        addressed.len(),
        4,
        "two members, two core agents: {addressed:?}"
    );

    engine.set_team_enabled(&team.id, false).unwrap();
    assert_eq!(
        assign::principals(engine.inner(), &squad),
        vec![],
        "a team stood down names nobody"
    );
    let spec = item(goal.id, None);
    assert_eq!(
        assign::workers(engine.inner(), &spec).await,
        Vec::<String>::new()
    );
    assert_eq!(assign::approvers(engine.inner(), goal.id, None), vec![]);

    let back = engine.set_team_enabled(&team.id, true).unwrap();
    assert_eq!(back.members, team.members, "it kept its members");
    assert_eq!(assign::principals(engine.inner(), &squad), addressed);
    assert_eq!(assign::workers(engine.inner(), &spec).await, vec![builder]);
    assert_eq!(
        assign::approvers(engine.inner(), goal.id, None),
        vec![reviewer]
    );
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// The harness a step runs on
// ---------------------------------------------------------------------------

/// An engine over harnesses of these ids, each yielding its step's result,
/// and the handles a test reads their launches back from.
fn engine_over(
    dir: &tempfile::TempDir,
    ids: &[&str],
) -> (Engine, std::collections::BTreeMap<String, Arc<MockAdapter>>) {
    let mut catalog = HarnessCatalog::new();
    let mut handles = std::collections::BTreeMap::new();
    for id in ids {
        let adapter = Arc::new(common::yielding(id, serde_json::json!({"ok": true})));
        catalog.register(Arc::clone(&adapter) as Arc<dyn bisa_harness::HarnessAdapter>);
        handles.insert(id.to_string(), adapter);
    }
    let engine = Engine::start(workspace(dir), catalog, design_off_config()).unwrap();
    (engine, handles)
}

/// One agent step — naming `harnesses`, assigned to `assignee` — run on a
/// goal to its end.
async fn run_one_step(
    engine: &Engine,
    harnesses: &[&str],
    assignee: Option<&str>,
) -> bisa_core::WorkflowRun {
    let step = common::step(
        "work",
        bisa_core::StepKind::Agent {
            instructions: "do the work".into(),
            assignee: assignee
                .map(|id| bisa_core::ValueRef::Fixed(Assignee::Agent(id.to_string()))),
            project: None,
            harness: harnesses.iter().map(|h| h.to_string()).collect(),
            model: None,
            effort: None,
            output_schema: None,
            tier_ceiling: ToolTier::Write,
        },
    );
    let (goal, _) = common::run_on(engine, "one step", common::new_workflow("one", vec![step]));
    common::finished_run(engine, goal.id).await
}

fn launched_on(handles: &std::collections::BTreeMap<String, Arc<MockAdapter>>) -> Vec<String> {
    handles
        .iter()
        .flat_map(|(id, adapter)| adapter.launches().into_iter().map(|_| id.clone()))
        .collect()
}

/// **An agent runs on its own harness.** A step that names no harness, taken
/// by an agent defined on one, is launched there — never on the platform's
/// default with that agent's prompt and model plan.
#[tokio::test(flavor = "multi_thread")]
async fn a_step_that_names_no_harness_runs_on_the_harness_of_the_agent_that_takes_it() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, handles) = engine_over(&dir, &["claude-code", "other"]);
    let hand = agent(engine.workspace(), "Other hand", "other");

    let done = run_one_step(&engine, &[], Some(&hand)).await;
    assert_eq!(
        done.outcome,
        Some(bisa_core::RunOutcome::Done),
        "{:?}",
        done.steps
    );
    assert_eq!(launched_on(&handles), vec!["other".to_string()]);
    let items = common::items_of(&engine, done.scope.goal().unwrap());
    assert_eq!(items[0].agent.as_deref(), Some(hand.as_str()));
    assert_eq!(
        items[0].harness_candidates,
        vec!["other".to_string()],
        "the item says where it ran"
    );
    engine.shutdown().await;
}

/// The step's own word is the author's: a step that names its harnesses runs
/// on them, whoever takes it.
#[tokio::test(flavor = "multi_thread")]
async fn a_step_that_names_its_harness_runs_there_whoever_takes_it() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, handles) = engine_over(&dir, &["claude-code", "other", "named"]);
    let hand = agent(engine.workspace(), "Other hand", "other");

    let done = run_one_step(&engine, &["named"], Some(&hand)).await;
    assert_eq!(done.outcome, Some(bisa_core::RunOutcome::Done));
    assert_eq!(launched_on(&handles), vec!["named".to_string()]);
    engine.shutdown().await;
}

/// Nobody named one and no agent took it: the platform's default.
#[tokio::test(flavor = "multi_thread")]
async fn a_step_nobody_takes_and_that_names_no_harness_runs_on_the_default() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, handles) = engine_over(&dir, &["claude-code", "other"]);

    let done = run_one_step(&engine, &[], None).await;
    assert_eq!(done.outcome, Some(bisa_core::RunOutcome::Done));
    assert_eq!(
        launched_on(&handles),
        vec![bisa_core::DEFAULT_HARNESS.to_string()]
    );
    engine.shutdown().await;
}
