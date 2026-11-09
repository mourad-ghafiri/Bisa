//! The UI/UX design policy at the prompt (06 — Agents and teams § Skills):
//! with the skill installed and `design.agents` at *everyone*, a work item of
//! an agent that does not carry it reads one sentence pointing at
//! `skill_read`, a carrier gets the skill and no sentence, and a harness
//! without the platform's tools is handed the skill whole and told so; under
//! *assigned* nobody but a carrier hears of it; under *nobody* the skill
//! leaves every session. And the agent's role opens every work item.

use crate::common::{engine_with, guided, run_spec, yielding};
use bisa_core::design::{DESIGN_NOTE, DESIGN_NOTE_APPENDED};
use bisa_core::workitem::{WorkItemSpec, WorkItemState};
use bisa_core::{
    Agent, Assignee, Budget, GoalId, HarnessCaps, RespondPolicy, SettingScope, SkillId, ToolTier,
    WorkItemId,
};
use bisa_engine::{Engine, SubmitRequest};
use bisa_harness::mock::MockAdapter;
use bisa_harness::SessionSpec;
use bisa_store::{NewAgent, NewSkill, Workspace};
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

const DESIGN: &str =
    "# UI/UX Design\n\nLook first.\n\n## Reference: Craft floor\n\nContrast 4.5:1.\n";

/// What a mock registered with the engine still lets a test read: the
/// prompts its sessions were given and the specs they were launched with.
struct Mock {
    prompts: Arc<Mutex<Vec<String>>>,
    launches: Arc<Mutex<Vec<SessionSpec>>>,
}

impl Mock {
    fn hold(adapter: &MockAdapter) -> Self {
        Self {
            prompts: Arc::clone(&adapter.prompts),
            launches: Arc::clone(&adapter.launches),
        }
    }
    fn prompts(&self) -> Vec<String> {
        self.prompts.lock().unwrap().clone()
    }
    fn launches(&self) -> Vec<SessionSpec> {
        self.launches.lock().unwrap().clone()
    }
}

fn install_design(ws: &Workspace) {
    ws.create_skill(NewSkill {
        id: SkillId::new("ui-ux-design").unwrap(),
        name: "UI/UX Design".into(),
        description: "Use when work changes a screen.".into(),
        tags: Default::default(),
        markdown: DESIGN.into(),
    })
    .unwrap();
}

fn agent(ws: &Workspace, name: &str, harness: &str, skills: Vec<SkillId>) -> Agent {
    ws.add_agent(NewAgent {
        name: name.into(),
        photo: None,
        description: None,
        system_prompt: format!("You are {name}."),
        harness: harness.into(),
        models: Default::default(),
        skills,
        mcps: vec![],
        tags: Default::default(),
        respond: RespondPolicy::OwnerOnly,
        decision_making: false,
    })
    .unwrap()
}

fn goal(engine: &Engine) -> GoalId {
    engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            title: Some("Ship the shelf".into()),
            ..guided("ship the shelf")
        })
        .unwrap()
        .id
}

/// A one-step item for `run_spec`, assigned to `agent` when one is named:
/// the step's assignee is what the engine writes back as the item's agent.
fn item(goal: GoalId, agent: Option<&str>, harness: &str) -> WorkItemSpec {
    WorkItemSpec {
        id: WorkItemId::from_ulid(ulid::Ulid::from_datetime(SystemTime::now())),
        home: bisa_core::Home::Goal { goal },
        run: None,
        step: None,
        instructions: "do the work".into(),
        state: WorkItemState::Open,
        project: None,
        harness_candidates: vec![harness.into()],
        model: None,
        effort: None,
        output_schema: None,
        budget: Budget::default(),
        assignees: agent
            .map(|a| vec![Assignee::Agent(a.to_string())])
            .unwrap_or_default(),
        tier_ceiling: ToolTier::Write,
        agent: None,
        spawn_allowlist: vec![],
        depth_budget: 0,
        result_attempts: 0,
        interruptions: 0,
    }
}

/// Run one item on a goal of its own — a run per goal, since a goal's next
/// workflow is set only once its last run finished.
async fn run(engine: &Engine, agent: Option<&str>, harness: &str) {
    let goal = goal(engine);
    run_spec(engine, goal, item(goal, agent, harness)).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn with_the_skill_installed_every_session_reads_one_sentence_naming_skill_read_and_a_carrier_gets_the_skill_instead(
) {
    let dir = tempfile::tempdir().unwrap();
    let adapter = yielding("mock", json!({"ok": true}));
    let mock = Mock::hold(&adapter);
    let engine = engine_with(&dir, vec![adapter]);
    let ws = engine.workspace();
    install_design(ws);
    let plain = agent(ws, "Scout", "mock", vec![]);
    let carrier = agent(
        ws,
        "Designer",
        "mock",
        vec![SkillId::new("ui-ux-design").unwrap()],
    );
    run(&engine, Some(plain.id.as_str()), "mock").await;
    let prompt = mock.prompts().remove(0);
    assert!(prompt.contains(DESIGN_NOTE), "the one sentence:\n{prompt}");
    assert!(
        !prompt.contains("## Skill: UI/UX Design"),
        "a non-carrier reads the skill with the tool, never appended:\n{prompt}"
    );
    run(&engine, Some(carrier.id.as_str()), "mock").await;
    let prompt = mock.prompts().remove(1);
    assert!(
        prompt.contains("## Skill: UI/UX Design") && prompt.contains("Look first."),
        "a carrier has the skill as any skill:\n{prompt}"
    );
    assert!(
        prompt.contains("Reference sections, read with skill_read: craft-floor"),
        "the body and an index, since the harness mounts the platform's servers:\n{prompt}"
    );
    assert!(
        !prompt.contains("Contrast 4.5:1.") && !prompt.contains(DESIGN_NOTE),
        "nothing said twice, no section appended:\n{prompt}"
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn assigned_tells_nobody_but_a_carrier_and_nobody_drops_the_skill_from_every_session() {
    let dir = tempfile::tempdir().unwrap();
    let adapter = yielding("mock", json!({"ok": true}));
    let mock = Mock::hold(&adapter);
    let engine = engine_with(&dir, vec![adapter]);
    let ws = engine.workspace();
    install_design(ws);
    let plain = agent(ws, "Scout", "mock", vec![]);
    let carrier = agent(
        ws,
        "Designer",
        "mock",
        vec![SkillId::new("ui-ux-design").unwrap()],
    );
    ws.set_setting(
        SettingScope::Workspace,
        None,
        "design.agents",
        json!("assigned"),
    )
    .unwrap();
    run(&engine, Some(plain.id.as_str()), "mock").await;
    run(&engine, Some(carrier.id.as_str()), "mock").await;
    let prompts = mock.prompts();
    assert!(
        !prompts[0].contains(DESIGN_NOTE) && !prompts[0].contains("## Skill: UI/UX Design"),
        "assigned: a non-carrier hears nothing:\n{}",
        prompts[0]
    );
    assert!(
        prompts[1].contains("## Skill: UI/UX Design"),
        "assigned: a carrier keeps its skill:\n{}",
        prompts[1]
    );
    ws.set_setting(
        SettingScope::Workspace,
        None,
        "design.agents",
        json!("nobody"),
    )
    .unwrap();
    run(&engine, Some(carrier.id.as_str()), "mock").await;
    let prompts = mock.prompts();
    assert!(
        !prompts[2].contains("## Skill: UI/UX Design") && !prompts[2].contains(DESIGN_NOTE),
        "nobody: the skill leaves a carrier's session and nothing is said:\n{}",
        prompts[2]
    );
    let launches = mock.launches();
    assert!(
        launches[2].skills.iter().all(|s| s.id != "ui-ux-design"),
        "… and its spec no longer carries it: {:?}",
        launches[2].skills.iter().map(|s| &s.id).collect::<Vec<_>>()
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_harness_without_platform_tools_is_handed_the_skill_whole_and_told_so() {
    let dir = tempfile::tempdir().unwrap();
    let mut toolless = yielding("toolless", json!({"ok": true}));
    toolless.caps = HarnessCaps::STEER | HarnessCaps::FOLLOW_UP;
    let mock = Mock::hold(&toolless);
    let engine = engine_with(&dir, vec![toolless]);
    let ws = engine.workspace();
    install_design(ws);
    let plain = agent(ws, "Scout", "toolless", vec![]);
    run(&engine, Some(plain.id.as_str()), "toolless").await;
    let prompt = mock.prompts().remove(0);
    assert!(
        prompt.contains(DESIGN_NOTE_APPENDED),
        "told it is appended:\n{prompt}"
    );
    assert!(
        prompt.contains("## Skill: UI/UX Design") && prompt.contains("## Reference: Craft floor"),
        "the whole skill, sections and all — there is no tool to read them with:\n{prompt}"
    );
    assert!(
        !prompt.contains("skill_read"),
        "no tool is named to a harness without one:\n{prompt}"
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_note_is_silent_while_the_skill_is_not_installed() {
    let dir = tempfile::tempdir().unwrap();
    let adapter = yielding("mock", json!({"ok": true}));
    let mock = Mock::hold(&adapter);
    let engine = engine_with(&dir, vec![adapter]);
    let ws = engine.workspace();
    let plain = agent(ws, "Scout", "mock", vec![]);
    run(&engine, Some(plain.id.as_str()), "mock").await;
    let prompt = mock.prompts().remove(0);
    assert!(
        !prompt.contains("UI/UX Design") && !prompt.contains("skill_read"),
        "nothing to point at yet:\n{prompt}"
    );
    engine.shutdown().await;
}

/// The agent's role opens its every work item — the prompt, a blank line,
/// then the step's instructions — as it opens its conversations; an item
/// with no agent starts with the instructions.
#[tokio::test(flavor = "multi_thread")]
async fn a_work_items_first_prompt_opens_with_its_agents_role_then_the_steps_instructions() {
    let dir = tempfile::tempdir().unwrap();
    let adapter = yielding("mock", json!({"ok": true}));
    let mock = Mock::hold(&adapter);
    let engine = engine_with(&dir, vec![adapter]);
    let ws = engine.workspace();
    let scout = agent(ws, "Scout", "mock", vec![]);
    run(&engine, Some(scout.id.as_str()), "mock").await;
    run(&engine, None, "mock").await;
    let prompts = mock.prompts();
    assert!(
        prompts[0].starts_with("You are Scout.\n\ndo the work"),
        "the role, then the step:\n{}",
        prompts[0]
    );
    assert!(
        prompts[1].starts_with("do the work"),
        "no agent, no role:\n{}",
        prompts[1]
    );
    engine.shutdown().await;
}
