//! What the platform needs before it can work (16 — The setup gate), read
//! against fakes: the harness is a `MockAdapter` that says whether it is
//! installed, git is the engine's own handle — pointed at a program that
//! does not exist to stand for a machine without git — and nothing is ever
//! installed or run.

use crate::common::{self, engine_with};
use bisa_core::{AgentId, EffortChoice, ModelPlan, SettingScope};
use bisa_engine::readiness::{CheckId, CheckState, Door, Fix};
use bisa_engine::{Engine, EngineConfig};
use bisa_harness::mock::MockAdapter;
use bisa_harness::ModelInfo;
use serde_json::json;

fn set(engine: &Engine, key: &str, value: serde_json::Value) {
    engine
        .set_setting(SettingScope::Workspace, None, key, value)
        .unwrap();
}

fn state(r: &bisa_engine::readiness::Readiness, id: CheckId) -> &bisa_engine::readiness::Check {
    r.checks.iter().find(|c| c.id == id).expect("the check")
}

/// The two core agents that hold a record, on the mock harness with a plan.
fn core_agents_on(engine: &Engine, harness: &str) {
    for id in [AgentId::general(), AgentId::workflow()] {
        let ws = engine.workspace();
        let mut a = ws.get_agent(&id).unwrap();
        a.harness = harness.into();
        a.models = ModelPlan::pinned("mock-model");
        ws.update_agent(a).unwrap();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn everything_ready_on_a_mock_harness_reads_ready() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    core_agents_on(&engine, "mock");
    set(&engine, "decisions.enabled", json!(true));
    set(&engine, "decisions.provider", json!("harness"));
    set(&engine, "decisions.harness.id", json!("mock"));
    let r = engine.readiness().await;
    let ids: Vec<CheckId> = r.checks.iter().map(|c| c.id).collect();
    assert_eq!(
        ids,
        [
            CheckId::Git,
            CheckId::Harness,
            CheckId::DecisionMakingAgent,
            CheckId::GeneralAgent,
            CheckId::WorkflowAgent
        ]
    );
    for c in &r.checks {
        assert_eq!(c.state, CheckState::Ready, "{:?}: {}", c.id, c.detail);
    }
    assert!(r.ready);
    assert!(
        state(&r, CheckId::Harness).detail.contains("Mock"),
        "{}",
        state(&r, CheckId::Harness).detail
    );
    assert!(state(&r, CheckId::DecisionMakingAgent)
        .detail
        .starts_with("On — answers as mock"));
    assert!(r.checked_at > 0);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_five_checks_go_out_in_order_under_their_wire_ids() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let r = engine.readiness().await;
    let wire = serde_json::to_value(&r).unwrap();
    let ids: Vec<&str> = wire["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            "git",
            "harness",
            "decision_making_agent",
            "general_agent",
            "workflow_agent"
        ]
    );
    engine.shutdown().await;
}

/// It holds no record, so there is no agent to open: what a person fixes is
/// the `decisions.*` settings, behind the tab that carries its name.
#[tokio::test(flavor = "multi_thread")]
async fn the_decision_making_agents_door_is_its_tab_in_settings() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let r = engine.readiness().await;
    let dm = state(&r, CheckId::DecisionMakingAgent);
    assert_eq!(dm.title, "The Decision-Making Agent");
    assert_eq!(
        dm.door,
        Door::Settings {
            tab: "decision-making".into()
        }
    );
    assert_eq!(
        serde_json::to_value(&dm.door).unwrap(),
        json!({"door": "settings", "tab": "decision-making"})
    );
    assert!(
        dm.detail
            .ends_with("(Settings › Decision Settings › Decision Making)."),
        "{}",
        dm.detail
    );
    for id in [CheckId::GeneralAgent, CheckId::WorkflowAgent] {
        assert_eq!(state(&r, id).door, Door::Agents, "{id:?}");
    }
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_harness_nobody_installed_is_missing_with_the_install_hints_and_leaves_the_agents_no_fix()
{
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(
        &dir,
        vec![MockAdapter {
            available: false,
            ..Default::default()
        }],
    );
    core_agents_on(&engine, "mock");
    let r = engine.readiness().await;
    assert!(!r.ready);
    let harness = state(&r, CheckId::Harness);
    assert_eq!(harness.state, CheckState::Missing);
    let hint = harness.hint.expect("Claude Code's official hint");
    assert_eq!(hint.url, "https://code.claude.com/docs/en/setup");
    assert!(hint
        .commands
        .iter()
        .any(|c| c.command.contains("claude.ai/install.sh")));
    for id in [CheckId::GeneralAgent, CheckId::WorkflowAgent] {
        let c = state(&r, id);
        assert_eq!(c.state, CheckState::Unready, "{:?}", id);
        assert!(c.detail.contains("mock, is not installed"), "{}", c.detail);
        assert!(
            c.fixes.is_empty(),
            "nothing installed: nothing to move onto"
        );
    }
    let dm = state(&r, CheckId::DecisionMakingAgent);
    assert_eq!(dm.state, CheckState::Unready);
    assert!(dm.fixes.is_empty());
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_decision_making_agent_is_unready_until_its_fix_is_applied() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    core_agents_on(&engine, "mock");
    let r = engine.readiness().await;
    let dm = state(&r, CheckId::DecisionMakingAgent);
    assert_eq!(dm.state, CheckState::Unready);
    assert!(
        dm.detail.starts_with("It is switched off."),
        "{}",
        dm.detail
    );
    let Some(Fix::Settings { label, set: values }) = dm.fixes.first() else {
        panic!("a settings fix per installed harness: {:?}", dm.fixes);
    };
    assert!(label.starts_with("Use Mock"), "{label}");
    assert!(label.ends_with(" as the Decision-Making Agent"), "{label}");
    assert_eq!(values["decisions.enabled"], json!(true));
    assert_eq!(values["decisions.provider"], json!("harness"));
    assert_eq!(values["decisions.harness.id"], json!("mock"));
    for (key, value) in values {
        set(&engine, key, value.clone());
    }
    let r = engine.readiness().await;
    assert_eq!(
        state(&r, CheckId::DecisionMakingAgent).state,
        CheckState::Ready
    );
    assert!(r.ready);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_decision_making_agent_on_a_harness_that_is_not_installed_is_not_ready() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(
        &dir,
        vec![MockAdapter {
            available: false,
            ..Default::default()
        }],
    );
    set(&engine, "decisions.enabled", json!(true));
    set(&engine, "decisions.provider", json!("harness"));
    set(&engine, "decisions.harness.id", json!("mock"));
    // The listing is read first: the readiness rule sees a probe, not a guess.
    let r = engine.readiness().await;
    let status = engine.decider_status().unwrap();
    assert!(!status.ready, "registered is not installed");
    assert_eq!(
        status.problem.as_deref(),
        Some("`mock` is not installed on this machine")
    );
    assert_eq!(
        state(&r, CheckId::DecisionMakingAgent).state,
        CheckState::Unready
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_core_agent_on_a_harness_not_here_is_unready_and_the_fix_moves_it() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    // The Workflow Agent is on the mock; the General Agent is still on claude-code, absent here.
    let ws = engine.workspace();
    let mut w = ws.get_agent(&AgentId::workflow()).unwrap();
    w.harness = "mock".into();
    w.models = ModelPlan::pinned("mock-model");
    ws.update_agent(w).unwrap();
    let r = engine.readiness().await;
    let general = state(&r, CheckId::GeneralAgent);
    assert_eq!(general.state, CheckState::Unready);
    assert!(
        general.detail.contains("claude-code, is not installed"),
        "{}",
        general.detail
    );
    let Some(Fix::AgentHarness {
        label,
        agent,
        harness,
        models,
    }) = general.fixes.first()
    else {
        panic!("a move per installed harness: {:?}", general.fixes);
    };
    assert_eq!(
        (label.as_str(), agent.as_str(), harness.as_str()),
        ("Run on Mock Harness", "general-agent", "mock")
    );
    let mut a = ws.get_agent(&AgentId::general()).unwrap();
    a.harness = harness.clone();
    a.models = models.clone();
    ws.update_agent(a).unwrap();
    let r = engine.readiness().await;
    assert_eq!(state(&r, CheckId::GeneralAgent).state, CheckState::Ready);
    assert_eq!(state(&r, CheckId::WorkflowAgent).state, CheckState::Ready);
    engine.shutdown().await;
}

/// The first fix a check offers for one agent, as the plan it would write.
fn plan_fix(r: &bisa_engine::readiness::Readiness, id: CheckId) -> ModelPlan {
    match state(r, id).fixes.first() {
        Some(Fix::AgentHarness { models, .. }) => models.clone(),
        other => panic!("a move per installed harness: {other:?}"),
    }
}

/// The first fix the Decision-Making Agent's check offers: its label, and
/// the model it would write.
fn judge_fix(r: &bisa_engine::readiness::Readiness) -> (String, serde_json::Value) {
    match state(r, CheckId::DecisionMakingAgent).fixes.first() {
        Some(Fix::Settings { label, set }) => {
            (label.clone(), set["decisions.harness.model"].clone())
        }
        other => panic!("a settings fix per installed harness: {other:?}"),
    }
}

/// Keep the plan the General Agent has and name an effort on it — the one
/// word of a plan that is the person's whichever models a fix writes.
fn general_agent_at(engine: &Engine, effort: EffortChoice) {
    let ws = engine.workspace();
    let mut general = ws.get_agent(&AgentId::general()).unwrap();
    general.models.effort = Some(effort);
    ws.update_agent(general).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn the_fixes_write_what_the_harness_recommends_and_keep_the_agents_effort() {
    let dir = tempfile::tempdir().unwrap();
    let recommended = ModelPlan::fallback(["best-model", "next-model"]);
    let engine = engine_with(
        &dir,
        vec![MockAdapter {
            // Listed in an order no agent would want: the recommendation is
            // the harness's word, not the head of its list.
            listed_models: vec![
                ModelInfo::new("small-model", None, vec![]),
                ModelInfo::new("next-model", None, vec![]),
                ModelInfo::new("best-model", None, vec![]),
            ],
            recommended_plan: Some(recommended.clone()),
            recommended_judge: Some("next-model".into()),
            ..Default::default()
        }],
    );
    // Both core agents are on claude-code, which is not here.
    general_agent_at(&engine, EffortChoice::Max);
    let r = engine.readiness().await;

    // The General Agent named an effort: the recommended models, its effort.
    let general = plan_fix(&r, CheckId::GeneralAgent);
    assert_eq!(general.models, recommended.models);
    assert_eq!(general.strategy, recommended.strategy);
    assert_eq!(general.effort, Some(EffortChoice::Max));
    // The Workflow Agent named none: the recommended plan as it is.
    assert_eq!(plan_fix(&r, CheckId::WorkflowAgent), recommended);

    // The judge gets the model recommended for judging, and the label says it.
    let (label, model) = judge_fix(&r);
    assert_eq!(model, json!("next-model"));
    assert_eq!(
        label,
        "Use Mock Harness · next-model as the Decision-Making Agent"
    );

    // Applied, the fix leaves the agent ready and its effort where it was.
    let ws = engine.workspace();
    let mut a = ws.get_agent(&AgentId::general()).unwrap();
    a.harness = "mock".into();
    a.models = general;
    ws.update_agent(a).unwrap();
    let r = engine.readiness().await;
    assert_eq!(state(&r, CheckId::GeneralAgent).state, CheckState::Ready);
    assert_eq!(
        ws.get_agent(&AgentId::general()).unwrap().models.effort,
        Some(EffortChoice::Max)
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_harness_with_no_opinion_gets_its_first_listed_models() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(
        &dir,
        vec![MockAdapter {
            listed_models: vec![
                ModelInfo::new("first-model", None, vec![]),
                ModelInfo::new("second-model", None, vec![]),
                ModelInfo::new("third-model", None, vec![]),
            ],
            ..Default::default()
        }],
    );
    general_agent_at(&engine, EffortChoice::Auto);
    let r = engine.readiness().await;

    let general = plan_fix(&r, CheckId::GeneralAgent);
    assert_eq!(
        general,
        ModelPlan::fallback(["first-model", "second-model"]).at(EffortChoice::Auto)
    );
    assert_eq!(
        plan_fix(&r, CheckId::WorkflowAgent),
        ModelPlan::fallback(["first-model", "second-model"])
    );
    let (label, model) = judge_fix(&r);
    assert_eq!(model, json!("first-model"));
    assert!(label.contains("first-model"), "{label}");
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_harness_that_lists_and_recommends_nothing_keeps_the_agents_plan() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    general_agent_at(&engine, EffortChoice::Low);
    let kept = engine
        .workspace()
        .get_agent(&AgentId::general())
        .unwrap()
        .models;
    assert!(!kept.models.is_empty(), "the General Agent ships a plan");
    let r = engine.readiness().await;

    assert_eq!(plan_fix(&r, CheckId::GeneralAgent), kept);
    // The judge is pinned to the harness's own default: no model word.
    let (label, model) = judge_fix(&r);
    assert_eq!(model, json!(""));
    assert_eq!(label, "Use Mock Harness as the Decision-Making Agent");
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn no_git_on_this_machine_is_missing_with_the_git_hint() {
    let dir = tempfile::tempdir().unwrap();
    let config = EngineConfig {
        git: Some(bisa_vcs::Git::new().with_program("bisa-no-such-git-binary")),
        ..common::design_off_config()
    };
    let engine = Engine::start(
        common::workspace(&dir),
        common::catalog_with(vec![MockAdapter::default()]),
        config,
    )
    .unwrap();
    let r = engine.readiness().await;
    let git = state(&r, CheckId::Git);
    assert_eq!(git.state, CheckState::Missing);
    assert!(
        git.detail.starts_with("git was not found on your PATH"),
        "{}",
        git.detail
    );
    let hint = git.hint.expect("git's official hint");
    assert_eq!(hint.url, "https://git-scm.com/install");
    assert!(hint
        .commands
        .iter()
        .any(|c| c.command == "brew install git"));
    assert!(!r.ready);
    engine.shutdown().await;
}
