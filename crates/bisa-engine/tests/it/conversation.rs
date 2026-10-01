//! Chatting with an agent, proven without an LLM.
//!
//! The default mock session answers a prompt with `echo: <prompt>` text
//! deltas and ends its turn — which is exactly the shape of a real reply, so
//! these tests exercise the whole path: message → dispatch → respond policy →
//! session launch → text capture → a reply posted under the agent's own key.

use crate::common;
use bisa_core::{
    AgentId, MemberRole, MessageBody, PrincipalId, RespondPolicy, RosterPolicy, SettingScope,
};
use bisa_engine::{Engine, EngineConfig};
use bisa_harness::mock::MockAdapter;
use bisa_harness::HarnessCatalog;
use bisa_store::{MemoryKeyStore, NewAgent, PostOrigin, Workspace};

/// The platform's own agent, as the wire spells it.
const CORE_AGENT_ID: &str = AgentId::GENERAL;
use std::sync::Arc;
use std::time::Duration;

fn workspace(dir: &tempfile::TempDir) -> Workspace {
    Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap()
}

fn catalog() -> HarnessCatalog {
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::new(MockAdapter {
        id: "chat-harness".into(),
        ..Default::default()
    }));
    catalog
}

fn config() -> EngineConfig {
    EngineConfig {
        // The guided driver must stays out of these tests: they are about chat.
        design_enabled: false,
        ..Default::default()
    }
}

fn new_agent(name: &str, respond: RespondPolicy) -> NewAgent {
    NewAgent {
        name: name.into(),
        photo: None,
        description: None,
        system_prompt: format!("You are {name}."),
        harness: "chat-harness".into(),
        models: Default::default(),
        skills: vec![],
        mcps: vec![],
        tags: Default::default(),
        respond,
        decision_making: false,
    }
}

/// Point the core agent at the harness these tests control.
///
/// The core agent is the one every workspace opens with and the one an
/// unaddressed message reaches by default, so a test cannot substitute a
/// definition of its own — what it can do is the one thing an owner can do,
/// which is edit that agent's harness. (Which agent triage wakes *is*
/// configurable, through `agents.default`; the test below sets it.)
fn core_on(ws: &Workspace, harness: &str) {
    let mut def = ws.get_agent(&AgentId::general()).unwrap();
    def.harness = harness.into();
    ws.update_agent(def).unwrap();
}

/// Every message in a scope authored by one agent.
fn said_by(engine: &Engine, scope: &str, pubkey: &str) -> Vec<String> {
    engine
        .workspace()
        .messages(scope, None, 200)
        .unwrap()
        .into_iter()
        .filter(|m| m.author == pubkey)
        .map(|m| m.content)
        .collect()
}

async fn until<T>(what: &str, mut probe: impl FnMut() -> Option<T>) -> T {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(v) = probe() {
            return v;
        }
        assert!(std::time::Instant::now() < deadline, "timed out: {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Give the engine a beat to *not* do something, for negative assertions.
async fn settle() {
    tokio::time::sleep(Duration::from_millis(600)).await;
}

/// DM an agent and it answers — signed by its own key, attested to the owner.
#[tokio::test]
async fn dm_to_an_agent_gets_a_reply_signed_by_that_agent() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    let agent = ws
        .add_agent(new_agent("Scout", RespondPolicy::OwnerOnly))
        .unwrap();
    let dm = ws.open_dm(std::slice::from_ref(&agent.pubkey)).unwrap();
    let engine = Engine::start(ws, catalog(), config()).unwrap();

    engine
        .workspace()
        .post_message(
            dm.id.as_str(),
            MessageBody::post("hello there"),
            None,
            &[],
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap();

    let reply = until("the agent's reply", || {
        engine
            .workspace()
            .messages(dm.id.as_str(), None, 20)
            .unwrap()
            .into_iter()
            .find(|m| m.author == agent.pubkey.as_hex())
    })
    .await;
    assert!(
        reply.content.contains("hello there"),
        "the reply should answer what was said, got {:?}",
        reply.content
    );

    // The stored event is signed by the agent and attested to the owner —
    // a collaborator's node can tell which agent spoke.
    let raw = std::fs::read_to_string(
        engine
            .workspace()
            .paths()
            .conversation_log(dm.id.as_str())
            .unwrap(),
    )
    .unwrap();
    let signed: Vec<nostr::event::Event> = raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let agent_event = signed
        .iter()
        .find(|e| e.pubkey.to_hex() == agent.pubkey.as_hex())
        .expect("an event authored by the agent");
    assert!(agent_event.verify().is_ok(), "agent reply must be signed");
    let owner = bisa_store::verify_attestation(agent_event).expect("NIP-OA attestation");
    assert_eq!(owner, engine.workspace().owner_principal().as_hex());

    engine.shutdown().await;
}

/// A wake that panics — a harness whose adapter breaks at launch — costs that
/// one turn and nothing after it: the next message wakes the agent again.
/// The mark that says *a wake is in flight* used to outlive the task that
/// held it, and every later message in that conversation waited behind a
/// wake that was gone.
#[tokio::test(flavor = "multi_thread")]
async fn a_wake_that_panics_costs_one_turn_and_the_next_message_is_answered() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    let mut agent = ws
        .add_agent(new_agent("Scout", RespondPolicy::OwnerOnly))
        .unwrap();
    agent.harness = "broken".into();
    let agent = ws.update_agent(agent).unwrap();
    let dm = ws.open_dm(std::slice::from_ref(&agent.pubkey)).unwrap();
    let mut catalog = catalog();
    let broken = MockAdapter {
        id: "broken".into(),
        panic_on_launch: true,
        ..Default::default()
    };
    let launches = Arc::clone(&broken.launches);
    catalog.register(Arc::new(broken));
    let engine = Engine::start(ws, catalog, config()).unwrap();
    let say = |words: &str| {
        engine
            .workspace()
            .post_message(
                dm.id.as_str(),
                MessageBody::post(words),
                None,
                &[],
                &[],
                None,
                PostOrigin::Asked,
            )
            .unwrap();
    };

    say("hello there");
    // The wake ran and unwound: give it the beat it needs to be gone.
    settle().await;
    assert!(launches.lock().unwrap().is_empty(), "it never launched");

    // The person puts the agent on a harness that works, and says it again.
    let mut mended = engine.workspace().get_agent(&agent.id).unwrap();
    mended.harness = "chat-harness".into();
    engine.workspace().update_agent(mended).unwrap();
    say("are you there now?");
    let reply = until("the agent's reply to the next message", || {
        engine
            .workspace()
            .messages(dm.id.as_str(), None, 20)
            .unwrap()
            .into_iter()
            .find(|m| m.author == agent.pubkey.as_hex())
    })
    .await;
    assert!(
        reply.content.contains("are you there now?"),
        "{}",
        reply.content
    );
    engine.shutdown().await;
}

/// The agent's own reply must not wake it again — otherwise two agents in a
/// conversation would talk to each other forever.
#[tokio::test]
async fn an_agents_own_message_never_wakes_it() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    let agent = ws
        .add_agent(new_agent("Echo", RespondPolicy::OwnerOnly))
        .unwrap();
    let dm = ws.open_dm(std::slice::from_ref(&agent.pubkey)).unwrap();
    let engine = Engine::start(ws, catalog(), config()).unwrap();

    engine
        .workspace()
        .post_message(
            dm.id.as_str(),
            MessageBody::post("ping"),
            None,
            &[],
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap();
    until("first reply", || {
        engine
            .workspace()
            .messages(dm.id.as_str(), None, 20)
            .unwrap()
            .into_iter()
            .find(|m| m.author == agent.pubkey.as_hex())
    })
    .await;

    // Let any runaway loop show itself.
    settle().await;
    settle().await;
    let from_agent = engine
        .workspace()
        .messages(dm.id.as_str(), None, 100)
        .unwrap()
        .into_iter()
        .filter(|m| m.author == agent.pubkey.as_hex())
        .count();
    assert_eq!(from_agent, 1, "exactly one reply — the loop guard holds");

    engine.shutdown().await;
}

/// `RespondPolicy::OwnerOnly` is enforced: a member who is not the owner
/// gets silence, not an answer.
#[tokio::test]
async fn owner_only_agents_ignore_other_members() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    let agent = ws
        .add_agent(new_agent("Quiet", RespondPolicy::OwnerOnly))
        .unwrap();
    let dm = ws.open_dm(std::slice::from_ref(&agent.pubkey)).unwrap();
    let engine = Engine::start(ws, catalog(), config()).unwrap();

    // A message that did not come from the owner: fake the author by
    // ingesting a stranger's event is out of scope here, so assert the
    // policy predicate directly through the dispatcher's only input — a
    // non-owner member.
    let other = PrincipalId::new("ab".repeat(32)).unwrap();
    engine
        .workspace()
        .add_member(
            other.clone(),
            MemberRole::Member,
            bisa_store::Admission::default(),
        )
        .unwrap();

    // The owner's message DOES get a reply (control).
    engine
        .workspace()
        .post_message(
            dm.id.as_str(),
            MessageBody::post("owner speaking"),
            None,
            &[],
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap();
    until("control reply", || {
        engine
            .workspace()
            .messages(dm.id.as_str(), None, 20)
            .unwrap()
            .into_iter()
            .find(|m| m.author == agent.pubkey.as_hex())
    })
    .await;

    // Flip the policy to Members and confirm the predicate distinguishes the
    // two: an OwnerOnly agent must refuse a collaborator.
    let owner_only = bisa_engine::conversation::may_respond(
        engine.inner(),
        &engine.workspace().get_agent(&agent.id).unwrap(),
        other.as_hex(),
    );
    assert!(!owner_only, "OwnerOnly must not act on a collaborator");

    engine.shutdown().await;
}

/// Mentioning an agent in a standing channel wakes **that** agent, and nobody
/// else — the mention is the whole subscription.
///
/// The first half of this test used to assert that an unaddressed channel
/// message summoned nobody at all, which was true and was the bug: a standing
/// channel has an empty audience, so `dispatch` matched no agent and returned
/// having done nothing, silently. It now asserts the rule that replaced it,
/// and it names the agent that woke rather than checking one agent's absence
/// — the old assertion looked only at Ada, so it would have gone on passing
/// with the behaviour reversed underneath it.
#[tokio::test]
async fn mentioning_an_agent_in_a_channel_wakes_it_and_only_it() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    core_on(&ws, "chat-harness");
    let core = ws.get_agent(&AgentId::general()).unwrap();
    let agent = ws
        .add_agent(new_agent("Ada", RespondPolicy::OwnerOnly))
        .unwrap();
    let bystander = ws
        .add_agent(new_agent("Grace", RespondPolicy::OwnerOnly))
        .unwrap();
    let channel = ws
        .create_channel(
            "design",
            Some("how it looks"),
            RosterPolicy::default(),
            Default::default(),
        )
        .unwrap();
    let engine = Engine::start(ws, catalog(), config()).unwrap();

    // Named nobody: the platform takes it, because an unaddressed message is
    // addressed to the platform.
    engine
        .workspace()
        .post_message(
            channel.id.as_str(),
            MessageBody::post("just thinking out loud"),
            None,
            &[],
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap();
    until("the core agent's triage reply", || {
        said_by(&engine, channel.id.as_str(), core.pubkey.as_hex())
            .into_iter()
            .next()
    })
    .await;
    settle().await;
    assert_eq!(
        said_by(&engine, channel.id.as_str(), agent.pubkey.as_hex()),
        Vec::<String>::new(),
        "triage must wake the core agent alone, not every agent in the workspace"
    );
    assert_eq!(
        said_by(&engine, channel.id.as_str(), bystander.pubkey.as_hex()),
        Vec::<String>::new(),
        "triage must wake the core agent alone, not every agent in the workspace"
    );

    // Mentioned: it answers in the channel, and the agent nobody named stays
    // quiet.
    engine
        .workspace()
        .post_message(
            channel.id.as_str(),
            MessageBody::post("@Ada what do you think?"),
            None,
            std::slice::from_ref(&agent.pubkey),
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap();
    until("the mentioned agent's reply", || {
        said_by(&engine, channel.id.as_str(), agent.pubkey.as_hex())
            .into_iter()
            .next()
    })
    .await;
    settle().await;
    assert_eq!(
        said_by(&engine, channel.id.as_str(), bystander.pubkey.as_hex()),
        Vec::<String>::new(),
        "a mention addresses exactly who it names"
    );

    engine.shutdown().await;
}

/// **An unaddressed message is addressed to the platform**, and to nothing
/// else: the core agent answers, and no other agent in the workspace stirs.
#[tokio::test]
async fn an_unaddressed_channel_message_wakes_the_core_agent_and_nobody_else() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    core_on(&ws, "chat-harness");
    let core = ws.get_agent(&AgentId::general()).unwrap();
    let others = ["Ada", "Grace", "Alan"].map(|n| {
        ws.add_agent(new_agent(n, RespondPolicy::OwnerOnly))
            .unwrap()
    });
    let channel = ws
        .create_channel("ops", None, RosterPolicy::default(), Default::default())
        .unwrap();
    let engine = Engine::start(ws, catalog(), config()).unwrap();

    engine
        .workspace()
        .post_message(
            channel.id.as_str(),
            MessageBody::post("who can look at the build?"),
            None,
            &[],
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap();

    let said = until("the core agent's reply", || {
        said_by(&engine, channel.id.as_str(), core.pubkey.as_hex())
            .into_iter()
            .next()
    })
    .await;
    assert!(
        said.contains("who can look at the build?"),
        "the triage session must be given the message it is triaging, got {said:?}"
    );

    settle().await;
    for other in &others {
        assert_eq!(
            said_by(&engine, channel.id.as_str(), other.pubkey.as_hex()),
            Vec::<String>::new(),
            "{} answered a message that did not name it",
            other.name
        );
    }
    // And exactly one triage session, not one per agent in the workspace.
    assert_eq!(
        said_by(&engine, channel.id.as_str(), core.pubkey.as_hex()).len(),
        1
    );

    engine.shutdown().await;
}

/// The hand-off terminates, and the shape is what terminates it.
///
/// Human → triage → worker → stop. The core agent's post naming an agent wakes
/// that agent; the agent's reply is agent-authored and is not the core agent,
/// so it wakes nothing. Two sessions per human message, maximum, provable
/// without a counter.
#[tokio::test]
async fn a_hand_off_from_the_core_agent_wakes_one_agent_and_then_stops() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    core_on(&ws, "chat-harness");
    let core = ws.get_agent(&AgentId::general()).unwrap();
    let developer = ws
        .add_agent(new_agent("Dev", RespondPolicy::OwnerOnly))
        .unwrap();
    let channel = ws
        .create_channel("build", None, RosterPolicy::default(), Default::default())
        .unwrap();
    let engine = Engine::start(ws, catalog(), config()).unwrap();

    // The core agent hands the question on, exactly as `post_message` with
    // `mentions` does it: signed by the core agent, `p`-tagging the developer.
    engine
        .workspace()
        .post_message(
            channel.id.as_str(),
            MessageBody::post("@Dev this is yours"),
            None,
            std::slice::from_ref(&developer.pubkey),
            &[],
            Some(&AgentId::general()),
            PostOrigin::Asked,
        )
        .unwrap();

    until("the handed-to agent's reply", || {
        said_by(&engine, channel.id.as_str(), developer.pubkey.as_hex())
            .into_iter()
            .next()
    })
    .await;

    // The developer's reply is agent-authored, so nothing may follow it — not
    // another turn of its own, and not a second core-agent session.
    settle().await;
    settle().await;
    assert_eq!(
        said_by(&engine, channel.id.as_str(), developer.pubkey.as_hex()).len(),
        1,
        "the handed-to agent answers once and the chain ends there"
    );
    assert_eq!(
        said_by(&engine, channel.id.as_str(), core.pubkey.as_hex()).len(),
        1,
        "the core agent's own post must not wake the core agent"
    );

    engine.shutdown().await;
}

/// An agent that the picker hides is still reachable by name: the core agent
/// resolves through the one resolver, exactly as any other agent id does.
///
/// This is the store-side half of the desktop's suggest/resolve split. A
/// mention token that stopped resolving because a surface stopped offering it
/// would post a message addressing nobody, and say nothing about it.
#[tokio::test]
async fn a_mention_of_the_core_agent_still_resolves() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    core_on(&ws, "chat-harness");
    let core = ws.get_agent(&AgentId::general()).unwrap();
    let channel = ws
        .create_channel("design", None, RosterPolicy::default(), Default::default())
        .unwrap();

    let resolved = ws
        .resolve_mentions(channel.id.as_str(), &[CORE_AGENT_ID.to_string()])
        .unwrap();
    assert_eq!(resolved, vec![core.pubkey.clone()]);

    let engine = Engine::start(ws, catalog(), config()).unwrap();
    engine
        .workspace()
        .post_message(
            channel.id.as_str(),
            MessageBody::post("@General Agent hello"),
            None,
            &resolved,
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap();
    until("the named core agent's reply", || {
        said_by(&engine, channel.id.as_str(), core.pubkey.as_hex())
            .into_iter()
            .next()
    })
    .await;

    engine.shutdown().await;
}

/// A burst of messages produces one reply and one follow-up — not one
/// session per message.
#[tokio::test]
async fn a_burst_produces_one_wake_then_one_follow_up() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    let agent = ws
        .add_agent(new_agent("Burst", RespondPolicy::OwnerOnly))
        .unwrap();
    let dm = ws.open_dm(std::slice::from_ref(&agent.pubkey)).unwrap();
    let engine = Engine::start(ws, catalog(), config()).unwrap();

    for i in 0..4 {
        engine
            .workspace()
            .post_message(
                dm.id.as_str(),
                MessageBody::post(format!("message {i}")),
                None,
                &[],
                &[],
                None,
                PostOrigin::Asked,
            )
            .unwrap();
    }
    settle().await;
    settle().await;

    let replies = engine
        .workspace()
        .messages(dm.id.as_str(), None, 100)
        .unwrap()
        .into_iter()
        .filter(|m| m.author == agent.pubkey.as_hex())
        .count();
    assert!(
        (1..=2).contains(&replies),
        "a burst should coalesce into one reply plus at most one follow-up, got {replies}"
    );

    engine.shutdown().await;
}

/// A chat survives a quota wall: the agent's plan is walked exactly the way a
/// work item's is, so the model dying mid-turn relaunches on the next entry
/// and the human still gets an answer.
///
/// The dead model is refused `NoProgress` — the launch-failure signature of
/// every subprocess harness — and only for its first launch, so the assertion
/// is about the *walk*, not about a permanently broken model.
#[tokio::test(flavor = "multi_thread")]
async fn a_chat_walks_the_plan_when_a_model_hits_a_wall() {
    use bisa_harness::mock::{DeadModel, ModelFailure};
    use bisa_harness::{ModelPlan, ModelStrategy};

    let dir = tempfile::tempdir().unwrap();
    let adapter = Arc::new(MockAdapter {
        id: "chat-harness".into(),
        dead_models: vec![ModelFailure::new("fable-5", DeadModel::NoProgress)
            .reason("rate limited")
            .retry_after(300)],
        ..Default::default()
    });
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::clone(&adapter) as Arc<dyn bisa_harness::HarnessAdapter>);

    let ws = workspace(&dir);
    let agent = ws
        .add_agent(NewAgent {
            models: ModelPlan {
                strategy: ModelStrategy::Fallback,
                effort: None,
                models: vec![
                    bisa_harness::ModelChoice::new("fable-5"),
                    bisa_harness::ModelChoice::new("opus-5"),
                ],
            },
            ..new_agent("Scout", RespondPolicy::OwnerOnly)
        })
        .unwrap();
    let dm = ws.open_dm(std::slice::from_ref(&agent.pubkey)).unwrap();
    let engine = Engine::start(ws, catalog, config()).unwrap();

    engine
        .workspace()
        .post_message(
            dm.id.as_str(),
            MessageBody::post("hello there"),
            None,
            &[],
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap();

    let reply = until("the agent's reply, after the failover", || {
        engine
            .workspace()
            .messages(dm.id.as_str(), None, 20)
            .unwrap()
            .into_iter()
            .find(|m| m.author == agent.pubkey.as_hex())
    })
    .await;
    assert!(reply.content.contains("hello there"), "{:?}", reply.content);

    // The wall was walked, in plan order, exactly once each.
    assert_eq!(
        adapter.launched_models(),
        vec![Some("fable-5".to_string()), Some("opus-5".to_string())]
    );
    // ...and the ledger remembers, so the next wake starts on opus-5.
    let health = engine.model_health();
    let row = health
        .iter()
        .find(|r| r.model == "fable-5")
        .unwrap_or_else(|| panic!("fable-5 in the ledger: {health:?}"));
    assert_eq!(row.harness, "chat-harness");
    assert!(row.is_cooling(), "{row:?}");

    engine.shutdown().await;
}

/// An effort of `auto` is judged on the message that opens the session, and
/// once: the relaunch a model wall asks for carries what the wake was told,
/// so the next model runs at the same judged level, fitted to its own list,
/// and nobody is asked a second time.
#[tokio::test(flavor = "multi_thread")]
async fn a_chat_is_judged_once_and_a_wall_keeps_the_judged_level() {
    use bisa_core::{DecisionPoint, DecisionQuestion, Effort, EffortChoice};
    use bisa_decision::ScriptedProvider;
    use bisa_harness::mock::{DeadModel, ModelFailure};
    use bisa_harness::ModelPlan;

    let five = [
        Effort::Low,
        Effort::Medium,
        Effort::High,
        Effort::Xhigh,
        Effort::Max,
    ];
    let four = [Effort::Low, Effort::Medium, Effort::High, Effort::Max];

    let dir = tempfile::tempdir().unwrap();
    let adapter = Arc::new(MockAdapter {
        id: "chat-harness".into(),
        dead_models: vec![ModelFailure::new("fable-5", DeadModel::NoProgress)
            .reason("rate limited")
            .retry_after(300)],
        efforts: Effort::ALL.to_vec(),
        model_efforts: vec![
            ("fable-5".into(), five.to_vec()),
            ("opus-5".into(), four.to_vec()),
        ],
        ..Default::default()
    });
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::clone(&adapter) as Arc<dyn bisa_harness::HarnessAdapter>);

    let ws = workspace(&dir);
    let agent = ws
        .add_agent(NewAgent {
            models: ModelPlan::fallback(["fable-5", "opus-5"]).at(EffortChoice::Auto),
            ..new_agent("Scout", RespondPolicy::OwnerOnly)
        })
        .unwrap();
    let dm = ws.open_dm(std::slice::from_ref(&agent.pubkey)).unwrap();
    let engine = Engine::start(ws, catalog, config()).unwrap();
    // A scripted provider stands in for the one the settings name: it says
    // `xhigh` once, and has nothing to say to a second question.
    let words: Vec<&str> = five.iter().map(|level| level.as_str()).collect();
    let provider = Arc::new(ScriptedProvider::new(vec![ScriptedProvider::answers(
        "effort",
        ScriptedProvider::choice("xhigh", &words, 0.9),
    )]));
    engine.stand_in_decision_provider(Some(provider.clone()));

    engine
        .workspace()
        .post_message(
            dm.id.as_str(),
            MessageBody::post("refactor the billing module"),
            None,
            &[],
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap();
    until("the agent's reply, after the failover", || {
        engine
            .workspace()
            .messages(dm.id.as_str(), None, 20)
            .unwrap()
            .into_iter()
            .find(|m| m.author == agent.pubkey.as_hex())
    })
    .await;

    assert_eq!(
        adapter.launched_models(),
        vec![Some("fable-5".to_string()), Some("opus-5".to_string())]
    );
    assert_eq!(
        adapter.launched_efforts(),
        vec![Some(Effort::Xhigh), Some(Effort::High)]
    );
    let asked = provider.asked();
    assert_eq!(asked.len(), 1, "the relaunch carried the judged level");
    assert_eq!(asked[0].state["task"], "refactor the billing module");
    assert_eq!(asked[0].state["agent"]["name"], "Scout");
    assert_eq!(asked[0].state["model"], "fable-5");
    assert!(matches!(
        &asked[0].questions["effort"],
        DecisionQuestion::Choice { criteria, .. } if criteria.len() == five.len()
    ));
    let judged = engine.recent_judgements(10).unwrap();
    assert_eq!(judged.len(), 1);
    assert_eq!(judged[0].judgement.point, DecisionPoint::ModelEffort);

    // The roster's row is the session that answered: the second model, at
    // the level it was sent.
    let row = engine
        .inner()
        .presence
        .snapshot()
        .into_iter()
        .find(|row| row.model.as_deref() == Some("opus-5"))
        .expect("the turn's session is on the roster");
    assert_eq!(row.effort, Some(Effort::High));

    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// Triage reaches a thread
// ---------------------------------------------------------------------------

/// **The complaint this change is about.**
///
/// A goal thread is not a channel, so `get_channel` failed, the triage
/// predicate was false, every agent hit the `continue`, and `dispatch` returned
/// having emitted nothing at all — no reply, no event, no log line. Asking a
/// question in your own goal and hearing nothing back is indistinguishable
/// from a workspace that is not running.
#[tokio::test]
async fn an_unaddressed_goal_message_wakes_the_core_agent_and_nobody_else() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    core_on(&ws, "chat-harness");
    let core = ws.get_agent(&AgentId::general()).unwrap();
    let others = ["Ada", "Grace"].map(|n| {
        ws.add_agent(new_agent(n, RespondPolicy::OwnerOnly))
            .unwrap()
    });
    let goal = ws
        .create_goal(bisa_store::NewGoal::captured("ship the demo"))
        .unwrap();
    let scope = goal.id.to_string();
    let engine = Engine::start(ws, catalog(), config()).unwrap();

    engine
        .workspace()
        .post_message(
            &scope,
            MessageBody::post("how should we handle the cart rounding?"),
            None,
            &[],
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap();

    let said = until("the core agent's reply", || {
        said_by(&engine, &scope, core.pubkey.as_hex())
            .into_iter()
            .next()
    })
    .await;
    assert!(
        said.contains("cart rounding"),
        "the triage session must be given the message it is triaging, got {said:?}"
    );

    settle().await;
    for other in &others {
        assert_eq!(
            said_by(&engine, &scope, other.pubkey.as_hex()),
            Vec::<String>::new(),
            "{} answered a message that did not name it",
            other.name
        );
    }
    // One session, not one per agent in the workspace.
    assert_eq!(said_by(&engine, &scope, core.pubkey.as_hex()).len(), 1);

    engine.shutdown().await;
}

/// **The hazard that kept triage out of threads.**
///
/// A `notify` step now signs as an agent; this test posts through the store,
/// owner-signed and with no mentions, to prove it is the *origin* alone that
/// keeps triage away — a line nobody asked, landing on the goal thread or in
/// a channel, is byte-for-byte what a person typing into an empty room
/// produces, and without `PostOrigin` every cron tick, probe and webhook
/// would start a harness session. The channel half closes a hole that was
/// once real and known.
#[tokio::test]
async fn a_notification_never_triages_in_a_thread_or_a_channel() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    core_on(&ws, "chat-harness");
    let core = ws.get_agent(&AgentId::general()).unwrap();
    let goal = ws
        .create_goal(bisa_store::NewGoal::captured("watched by a probe"))
        .unwrap();
    let thread = goal.id.to_string();
    let channel = ws
        .create_channel("alerts", None, RosterPolicy::default(), Default::default())
        .unwrap();
    let engine = Engine::start(ws, catalog(), config()).unwrap();

    for scope in [thread.as_str(), channel.id.as_str()] {
        engine
            .workspace()
            .post_message(
                scope,
                MessageBody::post("probe failed: exit 1"),
                None,
                &[],
                &[],
                None,
                PostOrigin::Announced,
            )
            .unwrap();
    }

    settle().await;
    for scope in [thread.as_str(), channel.id.as_str()] {
        assert_eq!(
            said_by(&engine, scope, core.pubkey.as_hex()),
            Vec::<String>::new(),
            "a notification in {scope} summoned an agent"
        );
    }

    // And the same room answers a person, so this is provably about the
    // origin rather than about the room being unreachable.
    engine
        .workspace()
        .post_message(
            &thread,
            MessageBody::post("what broke?"),
            None,
            &[],
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap();
    until("the core agent answering a person", || {
        said_by(&engine, &thread, core.pubkey.as_hex())
            .into_iter()
            .next()
    })
    .await;

    engine.shutdown().await;
}

/// An announcement carries the owner's authority whoever signs it: a
/// workflow's `notify`, signed by the agent it names, wakes exactly the agent
/// it mentions — under the default respond policy, which admits only the
/// owner and the General Agent for an *asked* message. The same post asked,
/// not announced, is a non-core agent's and reaches nobody: the loop guard
/// holds, and the woken agent's reply cannot chain.
#[tokio::test]
async fn an_announcement_wakes_whom_it_mentions_whoever_signs_it() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    core_on(&ws, "chat-harness");
    let announcer = ws
        .add_agent(new_agent("Announcer", RespondPolicy::OwnerOnly))
        .unwrap();
    let assayer = ws
        .add_agent(new_agent("Assayer", RespondPolicy::OwnerOnly))
        .unwrap();
    let goal = ws
        .create_goal(bisa_store::NewGoal::captured("announced"))
        .unwrap();
    let thread = goal.id.to_string();
    let engine = Engine::start(ws, catalog(), config()).unwrap();

    engine
        .workspace()
        .post_message(
            &thread,
            MessageBody::post("the release is out — check it"),
            None,
            std::slice::from_ref(&assayer.pubkey),
            &[],
            Some(&announcer.id),
            PostOrigin::Announced,
        )
        .unwrap();
    until("the assayer answering the announcement", || {
        said_by(&engine, &thread, assayer.pubkey.as_hex())
            .into_iter()
            .next()
    })
    .await;
    settle().await;
    assert_eq!(
        said_by(&engine, &thread, announcer.pubkey.as_hex()).len(),
        1,
        "the announcer said its line and was not woken by the reply"
    );

    engine
        .workspace()
        .post_message(
            &thread,
            MessageBody::post("and again?"),
            None,
            std::slice::from_ref(&assayer.pubkey),
            &[],
            Some(&announcer.id),
            PostOrigin::Asked,
        )
        .unwrap();
    settle().await;
    assert_eq!(
        said_by(&engine, &thread, assayer.pubkey.as_hex()).len(),
        1,
        "a non-core agent's ask reaches nobody"
    );
    engine.shutdown().await;
}

/// The MCP server's argv for the session, as the harness was handed it.
fn bisa_args(spec: &bisa_harness::SessionSpec) -> Vec<String> {
    spec.mcp_servers
        .iter()
        .find_map(|m| match &m.config {
            bisa_harness::McpServerConfig::Stdio { name, args, .. } if name == "bisa" => {
                Some(args.clone())
            }
            _ => None,
        })
        .expect("the bisa MCP server")
}

/// The Workflow Agent's turn in a goal's thread is launched knowing the goal
/// — `--goal` beside `--conversation` — so the MCP server hands it the
/// shaping tools, and *Request changes* in the thread proposes: the goal
/// points at the proposal, which is the goal's own design.
#[tokio::test(flavor = "multi_thread")]
async fn a_goal_thread_turn_of_the_workflow_agent_is_launched_with_the_goal_and_can_propose() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    common::drive_on(&ws, &AgentId::workflow(), "chat-harness");
    let proposal = serde_json::json!({
        "name": "Proposed from the thread",
        "description": "one step, asked for in the conversation",
        "steps": [{"id": "do", "name": "Do it", "kind": "agent",
                   "instructions": "do {goal.statement}", "harness": ["chat-harness"]}]
    });
    let script = bisa_harness::mock::IntakeScript::new(vec![serde_json::json!({
        "op": "propose_workflow", "agent": AgentId::WORKFLOW, "goal": "{{goal}}",
        "workflow": proposal
    })]);
    let replies = script.replies.clone();
    let adapter = Arc::new(MockAdapter {
        id: "chat-harness".into(),
        intake_script: Some(script),
        ..Default::default()
    });
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::clone(&adapter) as Arc<dyn bisa_harness::HarnessAdapter>);
    let engine = Engine::start(ws, catalog, config()).unwrap();
    let goal = engine
        .submit_goal(bisa_engine::SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..common::guided("shape me from the thread")
        })
        .unwrap();
    let thread = goal.id.to_string();
    let workflow_agent = engine.workspace().get_agent(&AgentId::workflow()).unwrap();

    engine
        .workspace()
        .post_message(
            &thread,
            MessageBody::post("please propose a workflow for this"),
            None,
            std::slice::from_ref(&workflow_agent.pubkey),
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap();
    // The tool's reply is the last thing the turn writes: waiting for it
    // is waiting for the proposal and everything the engine did with it.
    let reply = until("the proposal's reply", || {
        replies.lock().unwrap().first().cloned()
    })
    .await;
    assert_eq!(reply["ok"], serde_json::json!(true), "{reply}");
    let wf = engine
        .workspace()
        .get_goal(goal.id)
        .unwrap()
        .workflow
        .expect("the goal points at the proposal");
    let args = bisa_args(&adapter.launches()[0]);
    let after = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    assert_eq!(after("--conversation").as_deref(), Some(thread.as_str()));
    assert_eq!(after("--agent").as_deref(), Some(AgentId::WORKFLOW));
    assert_eq!(
        after("--goal").as_deref(),
        Some(thread.as_str()),
        "the turn is launched knowing the goal"
    );
    let workflow = engine.workspace().get_workflow(wf).unwrap();
    assert_eq!(
        workflow.origin,
        bisa_core::WorkflowOrigin::Goal { goal: goal.id },
        "the goal's own design"
    );
    assert_eq!(workflow.name, "Proposed from the thread");
    engine.shutdown().await;
}

/// Every agent's turn in a goal's thread is launched knowing the goal; the
/// router, not the argv, is what withholds the shaping tools from anyone
/// but the Workflow Agent.
#[tokio::test(flavor = "multi_thread")]
async fn a_goal_thread_turn_of_another_agent_is_launched_with_the_goal_too() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    let dev = ws
        .add_agent(new_agent("Developer", RespondPolicy::OwnerOnly))
        .unwrap();
    let goal = ws
        .create_goal(bisa_store::NewGoal::captured("with a developer"))
        .unwrap();
    let thread = goal.id.to_string();
    let adapter = Arc::new(MockAdapter {
        id: "chat-harness".into(),
        ..Default::default()
    });
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::clone(&adapter) as Arc<dyn bisa_harness::HarnessAdapter>);
    let engine = Engine::start(ws, catalog, config()).unwrap();
    engine
        .workspace()
        .post_message(
            &thread,
            MessageBody::post("a word?"),
            None,
            std::slice::from_ref(&dev.pubkey),
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap();
    until("the developer answering", || {
        said_by(&engine, &thread, dev.pubkey.as_hex())
            .into_iter()
            .next()
    })
    .await;
    let args = bisa_args(&adapter.launches()[0]);
    let goal_arg = args
        .iter()
        .position(|a| a == "--goal")
        .and_then(|i| args.get(i + 1))
        .cloned();
    assert_eq!(goal_arg.as_deref(), Some(thread.as_str()));
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// The mark
// ---------------------------------------------------------------------------

/// The message an agent takes carries that agent's own 👀.
///
/// Signed by the agent rather than the owner, which is the whole point: a mark
/// authored by the owner would appear as *your* reaction on *your* message and
/// say nothing about who picked it up.
#[tokio::test]
async fn a_message_an_agent_takes_is_marked_by_that_agent() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    let agent = ws
        .add_agent(new_agent("Scout", RespondPolicy::OwnerOnly))
        .unwrap();
    let dm = ws.open_dm(std::slice::from_ref(&agent.pubkey)).unwrap();
    let engine = Engine::start(ws, catalog(), config()).unwrap();

    let asked = engine
        .workspace()
        .post_message(
            dm.id.as_str(),
            MessageBody::post("can you look at this?"),
            None,
            &[],
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap();

    let mark = until("the agent's mark", || {
        engine
            .workspace()
            .list_reactions(dm.id.as_str())
            .unwrap()
            .into_iter()
            .find(|r| r.target_id == asked)
    })
    .await;
    assert_eq!(mark.emoji, "👀");
    assert_eq!(
        mark.author,
        agent.pubkey.as_hex(),
        "the mark must be the agent's, not the owner's"
    );

    // A second message coalesces into the same wake; the mark still dedupes
    // per (target, author, emoji), so nothing doubles up.
    engine
        .workspace()
        .post_message(
            dm.id.as_str(),
            MessageBody::post("and this one too"),
            None,
            &[],
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap();
    settle().await;
    let on_the_first = engine
        .workspace()
        .list_reactions(dm.id.as_str())
        .unwrap()
        .into_iter()
        .filter(|r| r.target_id == asked)
        .count();
    assert_eq!(on_the_first, 1, "one mark per agent per message");

    engine.shutdown().await;
}

/// A mark wakes nothing — not the agent that drew it, not anybody else.
///
/// `spawn_listener` filters to `KIND_MESSAGE`, so a reaction never reaches
/// `dispatch` at all. It is the loop-guard question a reader will ask, and it
/// is answered before the guard rather than by it.
#[tokio::test]
async fn an_agents_mark_wakes_nobody() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    core_on(&ws, "chat-harness");
    let core = ws.get_agent(&AgentId::general()).unwrap();
    let channel = ws
        .create_channel("quiet", None, RosterPolicy::default(), Default::default())
        .unwrap();
    let engine = Engine::start(ws, catalog(), config()).unwrap();

    // A message nobody answers: announced, so triage stays out of it.
    let posted = engine
        .workspace()
        .post_message(
            channel.id.as_str(),
            MessageBody::post("nightly build finished"),
            None,
            &[],
            &[],
            None,
            PostOrigin::Announced,
        )
        .unwrap();
    settle().await;

    engine
        .workspace()
        .react(&posted, "👀", Some(&AgentId::general()))
        .unwrap();
    settle().await;

    assert_eq!(
        said_by(&engine, channel.id.as_str(), core.pubkey.as_hex()),
        Vec::<String>::new(),
        "a reaction woke an agent"
    );

    engine.shutdown().await;
}

/// A project can name the agent its conversations reach, and a setting that
/// names nobody usable falls back to the core agent rather than to silence.
#[tokio::test]
async fn an_unaddressed_message_in_a_checkouts_conversation_wakes_the_projects_default_agent() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    core_on(&ws, "chat-harness");
    let reviewer = ws
        .add_agent(new_agent("Reviewer", RespondPolicy::Members))
        .unwrap();
    let project = ws
        .create_project(bisa_store::NewProject::managed("web-app").unwrap())
        .unwrap();
    let workstream = ws.primary_workstream(project.id).unwrap();
    std::fs::create_dir_all(ws.checkout_in(&project, &workstream)).unwrap();
    let conversation = ws
        .create_conversation(bisa_store::NewConversation {
            origin: bisa_core::ConversationOrigin::Workstream {
                id: workstream.id,
                project: project.id,
            },
            title: None,
            mode: bisa_core::ConversationMode::Auto,
        })
        .unwrap();
    let scope = conversation.id.to_string();
    let name_default = |ws: &Workspace, id: &str| {
        ws.set_setting(
            SettingScope::Project,
            Some(project.id),
            "agents.default",
            serde_json::json!(id),
        )
        .unwrap();
    };
    name_default(&ws, reviewer.id.as_ref());
    let engine = Engine::start(ws, catalog(), config()).unwrap();

    let say = |what: &str| {
        engine
            .workspace()
            .post_message(
                &scope,
                MessageBody::post(what),
                None,
                &[],
                &[],
                None,
                PostOrigin::Asked,
            )
            .unwrap()
    };
    let core_agent = engine.workspace().get_agent(&AgentId::general()).unwrap();
    let core = core_agent.pubkey.as_hex();

    // Nobody named: the project's default agent takes it, and the core agent
    // — which would have taken it before — stays out.
    say("what do you make of this branch?");
    let answered = until("the project's default agent answers", || {
        said_by(&engine, &scope, reviewer.pubkey.as_hex())
            .into_iter()
            .next()
    })
    .await;
    assert!(answered.contains("what do you make of this branch?"));
    assert!(
        said_by(&engine, &scope, core).is_empty(),
        "the core agent does not answer a conversation whose project named somebody else"
    );

    // A default nobody can wake — disabled here, and the same for an id that
    // names no agent at all — hands back to the core agent. Silence would be
    // the wrong answer to a message somebody typed.
    engine
        .workspace()
        .set_agent_enabled(&reviewer.id, false)
        .unwrap();
    say("and now?");
    let fell_back = until("the core agent stands in for a disabled default", || {
        said_by(&engine, &scope, core).into_iter().next()
    })
    .await;
    assert!(fell_back.contains("and now?"));
    assert_eq!(
        said_by(&engine, &scope, reviewer.pubkey.as_hex()).len(),
        1,
        "a disabled agent answers nothing further"
    );

    name_default(engine.workspace(), "no-such-agent");
    say("still there?");
    let again = until("the core agent stands in for an unknown default", || {
        said_by(&engine, &scope, core)
            .into_iter()
            .find(|m| m.contains("still there?"))
    })
    .await;
    assert!(again.contains("still there?"));

    engine.shutdown().await;
}

/// The Workflow Agent designs workflows and shapes goals; a conversation
/// about a checkout has neither. Named there — by a mention, or by a
/// project's `agents.default` — it stays silent and the General Agent takes
/// the message, so no surface can route around the rule. A goal thread still
/// reaches it.
#[tokio::test]
async fn the_workflow_agent_is_never_woken_in_a_conversation_about_a_checkout() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    core_on(&ws, "chat-harness");
    let mut workflow = ws.get_agent(&AgentId::workflow()).unwrap();
    workflow.harness = "chat-harness".into();
    ws.update_agent(workflow.clone()).unwrap();
    let project = ws
        .create_project(bisa_store::NewProject::managed("web-app").unwrap())
        .unwrap();
    let workstream = ws.primary_workstream(project.id).unwrap();
    std::fs::create_dir_all(ws.checkout_in(&project, &workstream)).unwrap();
    let conversation = ws
        .create_conversation(bisa_store::NewConversation {
            origin: bisa_core::ConversationOrigin::Workstream {
                id: workstream.id,
                project: project.id,
            },
            title: None,
            mode: bisa_core::ConversationMode::Auto,
        })
        .unwrap();
    let scope = conversation.id.to_string();
    let engine = Engine::start(ws, catalog(), config()).unwrap();

    let say = |scope: &str, what: &str, mentions: &[PrincipalId]| {
        engine
            .workspace()
            .post_message(
                scope,
                MessageBody::post(what),
                None,
                mentions,
                &[],
                None,
                PostOrigin::Asked,
            )
            .unwrap()
    };
    let general = engine.workspace().get_agent(&AgentId::general()).unwrap();
    let general_pk = general.pubkey.as_hex();
    let workflow_pk = workflow.pubkey.as_hex();
    let workflow_principal = PrincipalId::new(workflow_pk.to_string()).unwrap();

    // Mentioned by name in a conversation about a checkout: nobody addressed
    // is a triage, and triage names the General Agent.
    say(
        &scope,
        "@Workflow Agent, design something",
        std::slice::from_ref(&workflow_principal),
    );
    let took = until(
        "the General Agent takes a checkout's message that names the Workflow Agent",
        || said_by(&engine, &scope, general_pk).into_iter().next(),
    )
    .await;
    assert!(took.contains("design something"));
    assert!(
        said_by(&engine, &scope, workflow_pk).is_empty(),
        "the Workflow Agent never speaks in a conversation about a checkout"
    );

    // Named as the project's default: the setting falls through to the
    // General Agent, exactly as a disabled or unknown default does.
    engine
        .workspace()
        .set_setting(
            SettingScope::Project,
            Some(project.id),
            "agents.default",
            serde_json::json!(AgentId::WORKFLOW),
        )
        .unwrap();
    say(&scope, "and unaddressed?", &[]);
    until(
        "the General Agent stands in for a default that names the Workflow Agent",
        || {
            said_by(&engine, &scope, general_pk)
                .into_iter()
                .find(|m| m.contains("and unaddressed?"))
        },
    )
    .await;
    assert!(said_by(&engine, &scope, workflow_pk).is_empty());

    // A goal thread is the Workflow Agent's: the same mention wakes it there.
    let goal = engine
        .workspace()
        .create_goal(bisa_store::NewGoal::captured("a goal to shape"))
        .unwrap();
    let goal_scope = goal.id.to_string();
    say(
        &goal_scope,
        "@Workflow Agent, propose a workflow",
        &[workflow_principal],
    );
    let proposed = until("the Workflow Agent answers on a goal", || {
        said_by(&engine, &goal_scope, workflow_pk)
            .into_iter()
            .next()
    })
    .await;
    assert!(proposed.contains("propose a workflow"));

    engine.shutdown().await;
}

/// Deleting a channel stops the turn an agent is running in it, as deleting
/// a conversation does: no session goes on posting into a scope that no
/// longer resolves. A channel something still points at is refused before
/// anything is stopped.
#[tokio::test]
async fn deleting_a_channel_stops_the_turn_running_in_it() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    core_on(&ws, "endless");
    let core = ws.get_agent(&AgentId::general()).unwrap();
    let channel = ws
        .create_channel(
            "fleeting",
            None,
            RosterPolicy::default(),
            Default::default(),
        )
        .unwrap();
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::new(MockAdapter {
        id: "endless".into(),
        script: Some(vec![]),
        ..Default::default()
    }));
    let engine = Engine::start(ws, catalog, config()).unwrap();
    let inner = engine.inner();
    engine
        .workspace()
        .post_message(
            channel.id.as_str(),
            MessageBody::post("anyone there?"),
            None,
            &[],
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap();
    let live = |inner: &bisa_engine::Inner| {
        inner
            .presence
            .snapshot()
            .into_iter()
            .filter(|row| row.agent.as_ref() == Some(&core.id) && row.state.is_live())
            .count()
    };
    until("the core agent's turn is live in the channel", || {
        (live(inner) == 1).then_some(())
    })
    .await;

    let deletable = bisa_core::DeletableChannel::new(channel.clone()).unwrap();
    bisa_engine::messaging::delete_channel(inner, deletable)
        .await
        .unwrap();
    assert!(engine.workspace().get_channel(&channel.id).is_err());
    until("the turn is stopped with the channel", || {
        (live(inner) == 0).then_some(())
    })
    .await;
    engine.shutdown().await;
}
