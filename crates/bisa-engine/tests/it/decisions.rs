//! The Decision-Making Agent at every point it is wired to: off until somebody
//! switches it on, the point's own rule whenever it is not sure or does not
//! answer, a security verdict that fails closed, and a record of every
//! judgement.
//!
//! Every judgement here is answered by a scripted provider standing in for
//! the one the settings name: nothing is asked of a harness, a model or a
//! network, and no key exists.

use crate::common;
use bisa_core::workitem::{WorkItemSpec, WorkItemState};
use bisa_core::{
    Budget, DecisionAnswer, DecisionPoint, DecisionProviderKind, DecisionQuestion, DecisionRequest,
    Effort, EffortChoice, JudgeOption, JudgementOutcome, ModelChoice, ModelPlan, ModelStrategy,
    RespondPolicy, SettingScope, StepKind, ToolTier, WorkItemId,
};
use bisa_decision::{ProviderError, ScriptedProvider};
use bisa_engine::decider::{self, Judged, Standing};
use bisa_engine::{Engine, EnginePayload};
use bisa_harness::mock::{DeadModel, MockAdapter, ModelFailure};
use bisa_harness::SessionSpec;
use bisa_security::classify::{Subject, Verdict};
use bisa_store::NewAgent;
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

fn engine(dir: &tempfile::TempDir) -> Engine {
    common::engine_with(dir, vec![common::yielding("mock", json!({"ok": true}))])
}

/// Put a scripted provider in the settings' place, and keep a handle on it.
fn scripted(
    engine: &Engine,
    script: Vec<bisa_decision::scripted::Scripted>,
) -> Arc<ScriptedProvider> {
    let provider = Arc::new(ScriptedProvider::new(script));
    engine.stand_in_decision_provider(Some(provider.clone()));
    provider
}

fn set(engine: &Engine, key: &str, value: serde_json::Value) {
    engine
        .set_setting(SettingScope::Workspace, None, key, value)
        .unwrap();
}

fn choice(
    id: &str,
    option: &str,
    options: &[&str],
    confidence: f64,
) -> bisa_decision::scripted::Scripted {
    ScriptedProvider::answers(id, ScriptedProvider::choice(option, options, confidence))
}

fn agent(engine: &Engine, name: &str, what: &str, decision_making: bool) -> String {
    engine
        .workspace()
        .add_agent(NewAgent {
            name: name.into(),
            photo: None,
            description: Some(what.into()),
            system_prompt: format!("You are {name}."),
            harness: "mock".into(),
            models: Default::default(),
            skills: vec![],
            mcps: vec![],
            tags: Default::default(),
            respond: RespondPolicy::OwnerOnly,
            decision_making,
        })
        .unwrap()
        .id
        .to_string()
}

fn item(engine: &Engine, instructions: &str) -> WorkItemSpec {
    let goal = engine
        .submit_goal(common::guided("ship the release"))
        .unwrap();
    WorkItemSpec {
        id: WorkItemId::from_ulid(ulid::Ulid::from_datetime(SystemTime::now())),
        home: bisa_core::Home::Goal { goal: goal.id },
        run: None,
        step: None,
        instructions: instructions.into(),
        state: WorkItemState::Open,
        project: None,
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

fn judged_events(
    events: &mut tokio::sync::broadcast::Receiver<bisa_engine::EngineEvent>,
) -> Vec<bisa_core::Judgement> {
    let mut out = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let EnginePayload::Judged { judgement, .. } = event.payload {
            out.push(judgement);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Off by default, and what switches it on
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn it_is_off_until_somebody_switches_it_on_and_then_asks_nobody() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let provider = scripted(&engine, vec![]);
    let inner = engine.inner();

    let status = engine.decider_status().unwrap();
    assert!(!status.enabled);
    assert_eq!(status.agent.id, "decision-making-agent");
    assert_eq!(status.agent.name, "Decision-Making Agent");
    assert_eq!(status.provider, DecisionProviderKind::Harness);
    assert_eq!(status.answers_as, "claude-code/claude-sonnet-5-5[1m]");
    // `claude-code` is no harness of this fixture's: nothing says what the
    // model takes, so the status names no effort.
    assert_eq!(status.effort, None);
    assert_eq!(status.agent.default_effort, bisa_core::Effort::High);
    assert!(!status.calibrated);
    assert_eq!(status.key_stored, None, "a harness takes no key");
    for point in &status.points {
        assert_eq!(
            point.on, point.selected_explicitly,
            "{}: only a point selected by name is on",
            point.point
        );
    }

    // Every point the switch governs is off, and asks nobody.
    let request = DecisionRequest::one("s", "q", DecisionQuestion::noul("Is it so?"));
    for point in DecisionPoint::ALL {
        if point.is_selected_explicitly() {
            continue;
        }
        let judged = decider::judge(inner, point, &Standing::default(), request.clone()).await;
        assert_eq!(judged, Judged::Off, "{point}");
    }
    assert!(provider.asked().is_empty());

    // The pick of a pool falls to the lot, exactly as before.
    let reviewer = agent(&engine, "Reviewer", "reviews code", false);
    let writer = agent(&engine, "Writer", "writes docs", false);
    let pool = vec![reviewer, writer];
    let spec = item(&engine, "review the diff");
    assert_eq!(
        bisa_engine::assign::choose(inner, &pool, &spec).await,
        bisa_engine::assign::pick(&pool, spec.id)
    );
    assert!(provider.asked().is_empty(), "off asks nobody");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_workspace_an_agent_and_a_point_list_each_switch_exactly_their_own() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let inner = engine.inner();
    let point = DecisionPoint::AssignPick;

    assert!(!decider::is_on(inner, point, &Standing::default()));
    // An agent's — or a workflow's — own switch, with the workspace's off.
    let own = Standing {
        switched_on: true,
        ..Default::default()
    };
    assert!(decider::is_on(inner, point, &own));

    set(&engine, "decisions.enabled", json!(true));
    assert!(decider::is_on(inner, point, &Standing::default()));
    assert!(decider::is_on(
        inner,
        DecisionPoint::DispatchTriage,
        &Standing::default()
    ));

    // A point left to its rule stays there, whoever switched the rest on.
    set(&engine, "decisions.points_off", json!(["assign.pick"]));
    assert!(!decider::is_on(inner, point, &Standing::default()));
    assert!(!decider::is_on(inner, point, &own));
    assert!(decider::is_on(
        inner,
        DecisionPoint::DispatchTriage,
        &Standing::default()
    ));
    // … and a point selected by name is not the list's to switch off.
    set(
        &engine,
        "decisions.points_off",
        json!(["model.route", "workflow.judge"]),
    );
    assert!(decider::is_on(
        inner,
        DecisionPoint::ModelRoute,
        &Standing::default()
    ));
    assert!(decider::is_on(
        inner,
        DecisionPoint::WorkflowJudge,
        &Standing::default()
    ));
}

// ---------------------------------------------------------------------------
// The points
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn a_pick_is_the_decision_making_agents_when_it_is_sure_and_the_lot_when_it_is_not() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let inner = engine.inner();
    let reviewer = agent(&engine, "Reviewer", "reviews code for defects", true);
    let writer = agent(&engine, "Writer", "writes documentation", false);
    let pool = vec![reviewer.clone(), writer.clone()];
    let ids = [reviewer.as_str(), writer.as_str()];

    // One agent of the pool has its switch on: that is enough to ask.
    let provider = scripted(
        &engine,
        vec![
            choice("agent", &writer, &ids, 0.9),
            choice("agent", &writer, &ids, 0.2),
            Err(ProviderError::Unreachable("down".into())),
        ],
    );
    let spec = item(&engine, "write the release notes");
    assert_eq!(
        bisa_engine::assign::choose(inner, &pool, &spec).await,
        Some(writer.clone())
    );
    let asked = provider.asked();
    assert_eq!(asked[0].state, json!({ "work": "write the release notes" }));
    let DecisionQuestion::Choice { criteria, .. } = &asked[0].questions["agent"] else {
        panic!("a pick is a choice");
    };
    assert_eq!(criteria[&writer], "writes documentation");

    // Not sure, and no answer: the lot, both times.
    let by_lot = bisa_engine::assign::pick(&pool, spec.id);
    assert_eq!(
        bisa_engine::assign::choose(inner, &pool, &spec).await,
        by_lot
    );
    assert_eq!(
        bisa_engine::assign::choose(inner, &pool, &spec).await,
        by_lot
    );
    assert_eq!(provider.asked().len(), 3);

    // A pool of one is no question.
    let alone = vec![reviewer.clone()];
    assert_eq!(
        bisa_engine::assign::choose(inner, &alone, &spec).await,
        Some(reviewer)
    );
    assert_eq!(provider.asked().len(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn auto_route_leads_with_the_pick_and_never_asks_about_a_plan_that_is_not_routed() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let inner = engine.inner();
    let routed = ModelPlan {
        strategy: ModelStrategy::AutoRoute,
        effort: None,
        models: vec![
            ModelChoice::suited("haiku", "small, quick edits"),
            ModelChoice::suited("opus", "design and hard debugging"),
        ],
    };
    let provider = scripted(
        &engine,
        vec![
            choice("model", "opus", &["haiku", "opus"], 0.85),
            choice("model", "opus", &["haiku", "opus"], 0.1),
        ],
    );
    // Selected by name: on with the workspace's switch off.
    let lead = decider::route(
        inner,
        &routed,
        "mock",
        "design the sync protocol",
        &Standing::default(),
    )
    .await;
    assert_eq!(lead.as_deref(), Some("opus"));
    let asked = provider.asked();
    assert_eq!(
        asked[0].state,
        json!({ "task": "design the sync protocol" })
    );
    let DecisionQuestion::Choice { criteria, .. } = &asked[0].questions["model"] else {
        panic!("a route is a choice");
    };
    assert_eq!(criteria["opus"], "design and hard debugging");

    // Not sure: no lead, and the plan's own order stands.
    assert_eq!(
        decider::route(inner, &routed, "mock", "anything", &Standing::default()).await,
        None
    );

    // A plan that is not routed, or has one model, is never a question.
    let fallback = ModelPlan::fallback(["haiku", "opus"]);
    assert_eq!(
        decider::route(inner, &fallback, "mock", "anything", &Standing::default()).await,
        None
    );
    let mut single = routed.clone();
    single.models.truncate(1);
    assert_eq!(
        decider::route(inner, &single, "mock", "anything", &Standing::default()).await,
        None
    );
    assert_eq!(provider.asked().len(), 2);
}

// ---------------------------------------------------------------------------
// An effort of `auto`
// ---------------------------------------------------------------------------

/// What a model that takes `xhigh` takes, and what one that does not.
const FIVE: [Effort; 5] = [
    Effort::Low,
    Effort::Medium,
    Effort::High,
    Effort::Xhigh,
    Effort::Max,
];
const FOUR: [Effort; 4] = [Effort::Low, Effort::Medium, Effort::High, Effort::Max];

fn words(levels: &[Effort]) -> Vec<&'static str> {
    levels.iter().map(|level| level.as_str()).collect()
}

/// Every spec the engine's one harness was launched with, oldest first.
type Launches = Arc<Mutex<Vec<SessionSpec>>>;

/// An engine over one mock harness that yields a result, takes every level
/// for a model it keeps no list for and `lists` for the ones it does, and
/// refuses the `dead` models — with the launches it records.
fn engine_taking(
    dir: &tempfile::TempDir,
    dead: Vec<ModelFailure>,
    lists: Vec<(&str, Vec<Effort>)>,
) -> (Engine, Launches) {
    let mock = MockAdapter {
        dead_models: dead,
        efforts: Effort::ALL.to_vec(),
        model_efforts: lists
            .into_iter()
            .map(|(model, efforts)| (model.to_string(), efforts))
            .collect(),
        ..common::yielding("mock", json!({"ok": true}))
    };
    let launches = Arc::clone(&mock.launches);
    (common::engine_with(dir, vec![mock]), launches)
}

/// The model and the effort of every launch, oldest first.
fn sent(launches: &Launches) -> Vec<(Option<String>, Option<Effort>)> {
    launches
        .lock()
        .unwrap()
        .iter()
        .map(|spec| (spec.model.clone(), spec.effort))
        .collect()
}

fn on(model: &str, effort: Effort) -> (Option<String>, Option<Effort>) {
    (Some(model.to_string()), Some(effort))
}

fn agent_with(engine: &Engine, name: &str, what: &str, plan: ModelPlan) -> String {
    engine
        .workspace()
        .add_agent(NewAgent {
            name: name.into(),
            photo: None,
            description: Some(what.into()),
            system_prompt: format!("You are {name}."),
            harness: "mock".into(),
            models: plan,
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

/// One work item for `agent` on a manual goal of its own, run to settlement
/// as the single step of a workflow — the step carries the item's pins.
async fn run_for(engine: &Engine, agent: &str, instructions: &str, pin: Option<EffortChoice>) {
    let goal = engine
        .submit_goal(bisa_engine::SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..bisa_engine::SubmitRequest::captured("ship the release")
        })
        .unwrap();
    let spec = WorkItemSpec {
        home: bisa_core::Home::Goal { goal: goal.id },
        assignees: vec![bisa_core::Assignee::Agent(agent.to_string())],
        effort: pin,
        ..item(engine, instructions)
    };
    common::run_spec(engine, goal.id, spec).await;
}

fn effort_judgements(
    events: &mut tokio::sync::broadcast::Receiver<bisa_engine::EngineEvent>,
) -> Vec<bisa_core::Judgement> {
    judged_events(events)
        .into_iter()
        .filter(|j| j.point == DecisionPoint::ModelEffort)
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn an_effort_of_auto_is_judged_once_for_a_walk_and_kept_across_a_wall() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, launches) = engine_taking(
        &dir,
        vec![ModelFailure::new("fable-5", DeadModel::NoProgress)],
        vec![("fable-5", FIVE.to_vec()), ("opus-5", FOUR.to_vec())],
    );
    let mut events = engine.inner().subscribe();
    let provider = scripted(&engine, vec![choice("effort", "xhigh", &words(&FIVE), 0.9)]);
    let builder = agent_with(
        &engine,
        "Builder",
        "builds the product",
        ModelPlan::fallback(["fable-5", "opus-5"]).at(EffortChoice::Auto),
    );

    run_for(&engine, &builder, "refactor the billing module", None).await;

    // The judged level went to the first model as it is, and — after the
    // wall — to the second fitted to what that one takes.
    assert_eq!(
        sent(&launches),
        [on("fable-5", Effort::Xhigh), on("opus-5", Effort::High)]
    );

    // One walk, one question: about the task, the agent and the model the
    // plan leads with, among the levels that model takes.
    let asked = provider.asked();
    assert_eq!(asked.len(), 1, "a wall does not ask again");
    assert_eq!(
        asked[0].state,
        json!({
            "task": "refactor the billing module",
            "agent": { "name": "Builder", "description": "builds the product" },
            "model": "fable-5",
        })
    );
    let DecisionQuestion::Choice {
        instructions,
        criteria,
    } = &asked[0].questions["effort"]
    else {
        panic!("an effort is a choice");
    };
    assert!(
        instructions.contains("Prefer the lowest level that will do the job well"),
        "{instructions}"
    );
    let mut offered: Vec<&str> = criteria.keys().map(String::as_str).collect();
    offered.sort();
    let mut levels = words(&FIVE);
    levels.sort();
    assert_eq!(offered, levels, "the wire words of the five levels");
    assert_eq!(
        criteria["low"],
        "level 1 of 5, the lowest — simple, well-specified, quick tasks"
    );
    assert_eq!(
        criteria["xhigh"],
        "level 4 of 5 — long agentic work across many steps"
    );
    assert!(criteria["max"].starts_with("level 5 of 5, the highest"));

    // Recorded at its own point, on the goal the work is for.
    let judged = effort_judgements(&mut events);
    assert_eq!(judged.len(), 1);
    assert_eq!(judged[0].outcome, JudgementOutcome::Applied);
    assert!(engine
        .recent_judgements(10)
        .unwrap()
        .iter()
        .any(|r| r.judgement.point == DecisionPoint::ModelEffort
            && r.goal.is_some()
            && r.agent.as_deref() == Some(builder.as_str())));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_step_that_says_auto_is_judged_over_a_plan_that_names_a_level() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, launches) = engine_taking(&dir, vec![], vec![("opus-5", FOUR.to_vec())]);
    let provider = scripted(&engine, vec![choice("effort", "low", &words(&FOUR), 0.95)]);
    let builder = agent_with(
        &engine,
        "Builder",
        "builds the product",
        ModelPlan::fallback(["opus-5"]).at(EffortChoice::Max),
    );

    run_for(
        &engine,
        &builder,
        "rename a variable",
        Some(EffortChoice::Auto),
    )
    .await;

    assert_eq!(sent(&launches), [on("opus-5", Effort::Low)]);
    assert_eq!(provider.asked().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn nobody_is_asked_when_no_attempt_comes_to_auto_or_the_model_offers_no_choice() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, launches) = engine_taking(
        &dir,
        vec![],
        vec![
            ("opus-5", FIVE.to_vec()),
            ("solo-1", vec![Effort::Medium]),
            ("bare-1", vec![]),
        ],
    );
    let mut events = engine.inner().subscribe();
    let provider = scripted(&engine, vec![]);

    // Somebody named the level at every link an attempt reads.
    let named = agent_with(
        &engine,
        "Named",
        "builds",
        ModelPlan::fallback(["opus-5"]).at(EffortChoice::Max),
    );
    run_for(&engine, &named, "do the work", None).await;
    // A step's level is over a plan that says `auto`.
    let auto = agent_with(
        &engine,
        "Auto",
        "builds",
        ModelPlan::fallback(["opus-5"]).at(EffortChoice::Auto),
    );
    run_for(&engine, &auto, "do the work", Some(EffortChoice::Low)).await;
    // `auto`, on a model that takes one level: no choice to put.
    let solo = agent_with(
        &engine,
        "Solo",
        "builds",
        ModelPlan::fallback(["solo-1"]).at(EffortChoice::Auto),
    );
    run_for(&engine, &solo, "do the work", None).await;
    // `auto`, on a model that takes none: nothing to choose, nothing sent.
    let bare = agent_with(
        &engine,
        "Bare",
        "builds",
        ModelPlan::fallback(["bare-1"]).at(EffortChoice::Auto),
    );
    run_for(&engine, &bare, "do the work", None).await;

    assert_eq!(
        sent(&launches),
        [
            on("opus-5", Effort::Max),
            on("opus-5", Effort::Low),
            // The fallback, `high`, fitted to the one level there is.
            on("solo-1", Effort::Medium),
            (Some("bare-1".to_string()), None),
        ]
    );
    assert!(provider.asked().is_empty(), "{:?}", provider.asked());
    assert!(effort_judgements(&mut events).is_empty());
}

/// Naming `auto` is the switch: the workspace's switch and the list of
/// points left to their rule do not reach it. What is left is a judge that
/// is not sure, one that does not answer, and an answer that names no level
/// the question offered — each runs the attempt's own fallback, the next
/// level down the chain.
#[tokio::test(flavor = "multi_thread")]
async fn unsure_failed_and_a_word_never_offered_each_take_the_fallback() {
    let cases: Vec<(&str, bisa_decision::scripted::Scripted)> = vec![
        ("unsure", choice("effort", "max", &words(&FIVE), 0.2)),
        ("failed", Err(ProviderError::TimedOut)),
        (
            "a word never offered",
            choice("effort", "ultra", &["ultra", "low"], 0.99),
        ),
    ];
    for (case, script) in cases {
        let dir = tempfile::tempdir().unwrap();
        let (engine, launches) = engine_taking(&dir, vec![], vec![("opus-5", FIVE.to_vec())]);
        let mut events = engine.inner().subscribe();
        // The switch is off and the point is listed: neither is this point's.
        set(&engine, "decisions.enabled", json!(false));
        set(&engine, "decisions.points_off", json!(["model.effort"]));
        assert!(decider::is_on(
            engine.inner(),
            DecisionPoint::ModelEffort,
            &Standing::default()
        ));
        // The next level down the chain from the plan's `auto`.
        set(&engine, "agents.effort", json!("low"));
        let provider = scripted(&engine, vec![script]);
        let builder = agent_with(
            &engine,
            "Builder",
            "builds the product",
            ModelPlan::fallback(["opus-5"]).at(EffortChoice::Auto),
        );

        run_for(&engine, &builder, "do the work", None).await;

        assert_eq!(sent(&launches), [on("opus-5", Effort::Low)], "{case}");
        assert_eq!(provider.asked().len(), 1, "{case}: asked, once");
        let judged = effort_judgements(&mut events);
        assert_eq!(judged.len(), 1, "{case}");
        assert_ne!(judged[0].outcome, JudgementOutcome::Applied, "{case}");
    }
}

/// When a harness answers for the Decision-Making Agent, its session runs the
/// model and the effort the settings name — fitted to the model like any
/// other — and is one launch: it asks nobody how hard to work.
#[tokio::test(flavor = "multi_thread")]
async fn the_judges_own_session_runs_at_its_settings_effort_and_asks_nobody() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, launches) = engine_taking(
        &dir,
        vec![],
        vec![("judge-model", vec![Effort::Medium, Effort::High])],
    );
    set(&engine, "decisions.provider", json!("harness"));
    set(&engine, "decisions.harness.id", json!("mock"));
    set(&engine, "decisions.harness.model", json!("judge-model"));
    set(&engine, "decisions.harness.effort", json!("low"));
    set(&engine, "decisions.retries", json!(0));
    // Every agent's session would be judged, were this one an agent's.
    set(&engine, "agents.effort", json!("auto"));
    assert_eq!(decider::harness_effort(engine.inner()), Effort::Low);
    // The status says the level the session is sent: `low`, on a model
    // whose lowest is `medium`.
    assert_eq!(
        engine.decider_status().unwrap().effort,
        Some(Effort::Medium)
    );

    let request = DecisionRequest::one("s", "q", DecisionQuestion::noul("Is it so?"));
    let standing = Standing {
        switched_on: true,
        ..Default::default()
    };
    // The mock says nothing a contract holds to; what is read is the launch.
    let _judged = decider::judge(
        engine.inner(),
        DecisionPoint::AgentDecide,
        &standing,
        request,
    )
    .await;

    assert_eq!(sent(&launches), [on("judge-model", Effort::Medium)]);
}

/// A one-shot question put to an agent whose plan says `auto` takes the
/// fallback: the session a judgement or a verdict runs in never asks.
#[tokio::test(flavor = "multi_thread")]
async fn a_one_shot_question_never_asks_how_hard_to_work() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, launches) = engine_taking(&dir, vec![], vec![]);
    let provider = scripted(&engine, vec![]);
    set(&engine, "agents.effort", json!("medium"));
    let reader = agent_with(
        &engine,
        "Reader",
        "reads what it is shown",
        ModelPlan::fallback(["opus-5"]).at(EffortChoice::Auto),
    );

    // Whatever the mock says, or does not: what is read is the launch.
    let _answer = bisa_engine::ask::ask_agent_once(
        engine.inner(),
        &reader,
        bisa_engine::ask::Asking::of(bisa_core::AskPurpose::Decision { point: None }),
        "What is this?",
        Duration::from_secs(10),
    )
    .await;

    assert_eq!(sent(&launches), [on("opus-5", Effort::Medium)]);
    assert!(provider.asked().is_empty(), "{:?}", provider.asked());
}

/// The classifier, read by a harness: its session runs the model and the
/// effort `security.classifier.*` name, and asks nobody.
#[tokio::test(flavor = "multi_thread")]
async fn the_classifiers_own_session_runs_at_its_settings_effort_and_asks_nobody() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, launches) = engine_taking(&dir, vec![], vec![]);
    let provider = scripted(&engine, vec![]);
    set(&engine, "security.classifier.provider", json!("harness"));
    set(&engine, "security.classifier.harness", json!("mock"));
    set(&engine, "security.classifier.model", json!("reader-model"));
    set(&engine, "security.classifier.effort", json!("minimal"));
    set(&engine, "agents.effort", json!("auto"));
    let classifier = engine.inner().security.policy().classifier.clone();
    assert_eq!(classifier.effort, Effort::Minimal);

    // No verdict is read from what the mock says; the launch is what counts.
    let _verdict =
        bisa_engine::classifier::classify(engine.inner(), &bash("ls -la"), &classifier, None).await;

    assert_eq!(sent(&launches), [on("reader-model", Effort::Minimal)]);
    assert!(provider.asked().is_empty(), "{:?}", provider.asked());
}

/// One `judge` step over the goal's statement — good news, bad news, or
/// nobody sure — each branch an end of its own.
fn judge_workflow(name: &str) -> bisa_store::NewWorkflow {
    let option = |branch: &str, meaning: &str| JudgeOption {
        branch: bisa_core::Branch::new(branch).unwrap(),
        meaning: meaning.into(),
    };
    let mut judge = common::step(
        "judge",
        StepKind::Judge {
            state: "{goal.statement}".into(),
            instructions: "Is this good news?".into(),
            options: vec![
                option("good", "nothing needs a person"),
                option("bad", "somebody should look"),
            ],
            otherwise: bisa_core::Branch::new("unsure").unwrap(),
            min_confidence: None,
        },
    );
    judge.then = ["good", "bad", "unsure"]
        .into_iter()
        .map(|b| bisa_core::Flow::branch(common::sid(b), bisa_core::Branch::new(b).unwrap()))
        .collect();
    let end = |id: &str| {
        common::step(
            id,
            StepKind::End {
                finish: bisa_core::Finish::Done,
            },
        )
    };
    common::new_workflow(name, vec![judge, end("good"), end("bad"), end("unsure")])
}

/// A judgement runs in a task of its own, and the task owes its step a
/// settlement: a panic inside it fails the step in words. Left unsettled,
/// the step read *running* with nothing at work on it until the node was
/// started again.
#[tokio::test(flavor = "multi_thread")]
async fn a_judgement_that_panics_fails_its_step_and_leaves_nothing_running() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    engine.stand_in_decision_provider(Some(Arc::new(common::PanickingProvider)));
    let (goal, _) = common::run_on(&engine, "the build is red", judge_workflow("Judge"));
    let run = common::finished_run(&engine, goal.id).await;
    let step = &run.steps[&common::sid("judge")];
    assert_eq!(step.state, bisa_core::StepState::Failed, "{step:?}");
    let why = step.error.clone().unwrap_or_default();
    assert!(
        why.contains("panicked") && why.contains("the provider broke"),
        "the step says what happened: {why}"
    );
    assert_eq!(run.status(), bisa_core::RunStatus::Failed);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_judge_step_takes_the_branch_chosen_and_otherwise_when_nobody_is_sure() {
    for (script, branch, judged) in [
        (choice("branch", "bad", &["good", "bad"], 0.9), "bad", true),
        (
            choice("branch", "bad", &["good", "bad"], 0.3),
            "unsure",
            false,
        ),
        (Err(ProviderError::TimedOut), "unsure", false),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let engine = engine(&dir);
        let provider = scripted(&engine, vec![script]);
        let (goal, _) = common::run_on(&engine, "the build is red", judge_workflow("Judge"));
        let run = common::finished_run(&engine, goal.id).await;
        let step = &run.steps[&common::sid("judge")];
        assert_eq!(
            step.state,
            bisa_core::StepState::chose(bisa_core::Branch::new(branch).unwrap())
        );
        assert_eq!(step.output.as_ref().unwrap()["judged"], json!(judged));
        // The state is the template rendered against the run.
        assert_eq!(provider.asked()[0].state, json!("the build is red"));
        // A judgement on a goal is in the goal's journal.
        let journal = engine
            .workspace()
            .journal(&bisa_core::Home::from(goal.id))
            .unwrap();
        assert!(journal.iter().any(|e| matches!(
            &e.payload,
            bisa_core::JournalPayload::Judgement { judgement, step: Some(s), .. }
                if judgement.point == DecisionPoint::WorkflowJudge && s.as_str() == "judge"
        )));
    }
}

// ---------------------------------------------------------------------------
// Security fails closed
// ---------------------------------------------------------------------------

fn bash(command: &str) -> Subject {
    Subject {
        tool: "Bash".into(),
        summary: command.into(),
        paths: vec![],
        cwd: Some("~/work/app".into()),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_classifier_reads_only_a_sure_no_harm_as_safe() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    set(
        &engine,
        "security.classifier.provider",
        json!("decision_making_agent"),
    );
    let classifier = engine.inner().security.policy().classifier.clone();
    let harms: Vec<&str> = bisa_security::classify::TOOL_HARMS
        .iter()
        .map(|(id, _)| *id)
        .collect();

    // Each call is a different command: a verdict is cached by its subject.
    let cases: Vec<(bisa_decision::scripted::Scripted, Option<bool>)> = vec![
        (choice("harm", "none", &harms, 0.97), Some(true)),
        (choice("harm", "exfiltrates", &harms, 0.95), Some(false)),
        // A harm the question never offered breaks the contract: no verdict.
        (choice("harm", "bogus", &["bogus", "none"], 0.99), None),
        // Sure to less than `decisions.confidence.security`: no verdict.
        (choice("harm", "none", &harms, 0.8), None),
        // A provider that fails, times out or is refused gives no verdict.
        (Err(ProviderError::TimedOut), None),
        (Err(ProviderError::Unreachable("down".into())), None),
        (
            Err(ProviderError::Refused {
                status: 401,
                message: "no".into(),
            }),
            None,
        ),
    ];
    let provider = scripted(&engine, cases.iter().map(|(s, _)| s.clone()).collect());
    for (i, (_, expected)) in cases.iter().enumerate() {
        let subject = bash(&format!("command-{i}"));
        let verdict =
            bisa_engine::classifier::classify(engine.inner(), &subject, &classifier, None).await;
        match expected {
            Some(true) => assert_eq!(verdict.unwrap(), Verdict::Safe, "case {i}"),
            Some(false) => assert!(
                matches!(verdict.unwrap(), Verdict::Harmful { ref reason } if reason.contains("secrets")),
                "case {i}"
            ),
            // No verdict is an error here, which `decide_tool` turns into a
            // question for the person. It is never `Safe`.
            None => assert!(verdict.is_err(), "case {i}"),
        }
    }
    assert_eq!(provider.asked().len(), cases.len());
    // What the model read is the redacted call, and the harms it chose among.
    let asked = &provider.asked()[0];
    assert!(asked.state.as_str().unwrap().contains("command-0"));
    let DecisionQuestion::Choice { criteria, .. } = &asked.questions["harm"] else {
        panic!("a reading is a choice");
    };
    assert!(criteria.contains_key("none") && criteria.contains_key("destroys_data"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_secret_never_reaches_a_provider() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    set(&engine, "decisions.enabled", json!(true));
    let provider = scripted(
        &engine,
        vec![ScriptedProvider::answers(
            "q",
            DecisionAnswer::Noul { noul: 0.99 },
        )],
    );
    // A GitHub token's shape, which the built-in redaction rules know. A
    // fixture: it opens nothing.
    let token = format!("ghp_{}", "a1B2".repeat(9));
    let request = DecisionRequest::one(
        format!("deploy with {token}"),
        "q",
        DecisionQuestion::noul(format!("Is {token} safe to print?")),
    );
    let judged = decider::judge(
        engine.inner(),
        DecisionPoint::AgentDecide,
        &Standing::default(),
        request,
    )
    .await;
    assert!(matches!(judged, Judged::Answered(_)));
    let asked = serde_json::to_string(&provider.asked()[0]).unwrap();
    assert!(!asked.contains(&token), "{asked}");
    assert!(
        asked.contains("secret:"),
        "a placeholder stands where it was: {asked}"
    );
}

// ---------------------------------------------------------------------------
// Records
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn every_judgement_is_recorded_with_what_became_of_it() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    set(&engine, "decisions.enabled", json!(true));
    let mut events = engine.inner().subscribe();
    scripted(
        &engine,
        vec![
            ScriptedProvider::answers("q", DecisionAnswer::Noul { noul: 0.98 }),
            ScriptedProvider::answers("q", DecisionAnswer::Noul { noul: 0.6 }),
            Err(ProviderError::Unreachable("down".into())),
        ],
    );
    let request = DecisionRequest::one("s", "q", DecisionQuestion::noul("Is it so?"));
    let point = DecisionPoint::AgentDecide;
    let standing = Standing {
        agent: Some("developer".into()),
        ..Default::default()
    };
    for _ in 0..3 {
        decider::judge(engine.inner(), point, &standing, request.clone()).await;
    }
    let outcomes: Vec<_> = judged_events(&mut events)
        .into_iter()
        .map(|j| (j.outcome, j.reason.is_some(), j.answers.len()))
        .collect();
    assert_eq!(
        outcomes,
        [
            (JudgementOutcome::Applied, false, 1),
            (JudgementOutcome::Unsure, true, 1),
            (JudgementOutcome::Failed, true, 0),
        ]
    );

    // The feed keeps every one, goal or no goal, newest first.
    let recent = engine.recent_judgements(10).unwrap();
    assert_eq!(recent.len(), 3);
    assert_eq!(recent[0].judgement.outcome, JudgementOutcome::Failed);
    assert_eq!(recent[2].judgement.outcome, JudgementOutcome::Applied);
    assert!(recent.iter().all(|r| r.goal.is_none()));
    assert!(recent
        .iter()
        .all(|r| r.agent.as_deref() == Some("developer")));
    assert_eq!(recent[0].judgement.point, point);
    assert!(
        recent[0].judgement.calibrated,
        "the scripted provider stands for an RLCD model"
    );
    assert_eq!(engine.recent_judgements(1).unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_key_is_kept_and_never_read_back() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    set(&engine, "decisions.provider", json!("jev"));
    let status = engine.decider_status().unwrap();
    assert_eq!(status.provider, DecisionProviderKind::Jev);
    assert_eq!(status.answers_as, "jev-latest");
    assert!(status.calibrated);
    assert_eq!(status.key_stored, Some(false));
    assert!(!status.ready);
    assert!(status.problem.as_deref().unwrap().contains("no API key"));

    // A fixture value: it opens nothing.
    let key = "fixture-key-opens-nothing";
    engine
        .set_decision_key(DecisionProviderKind::Jev, Some(key))
        .unwrap();
    let status = engine.decider_status().unwrap();
    assert_eq!(status.key_stored, Some(true));
    assert!(status.ready);
    assert!(!serde_json::to_string(&status).unwrap().contains(key));

    // A provider that takes no key refuses one; an empty key is no key.
    assert!(engine
        .set_decision_key(DecisionProviderKind::Harness, Some(key))
        .is_err());
    assert!(engine
        .set_decision_key(DecisionProviderKind::Jev, Some("  "))
        .is_err());

    engine
        .set_decision_key(DecisionProviderKind::Jev, None)
        .unwrap();
    assert_eq!(engine.decider_status().unwrap().key_stored, Some(false));
    engine
        .set_decision_key(DecisionProviderKind::Jev, None)
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn try_it_holds_the_request_and_the_answer_to_the_contract() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    scripted(&engine, vec![choice("q", "a", &["a", "b"], 0.4)]);
    let request = DecisionRequest::one(
        "s",
        "q",
        DecisionQuestion::choice("Which?", [("a", "first"), ("b", "second")]),
    );
    // An unsure answer is still the answer: nothing is decided here.
    let response = engine.try_decision(&request).await.unwrap();
    assert_eq!(response.answer("q").unwrap().chosen(), Some("a"));
    assert!(
        engine.recent_judgements(10).unwrap().is_empty(),
        "and nothing is recorded"
    );

    let mut none = request.clone();
    none.questions.clear();
    assert!(engine.try_decision(&none).await.is_err());
}
