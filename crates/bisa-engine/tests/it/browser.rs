//! The browser bridge (ide/18): a `browser` op parks its request for the
//! desktop, which answers over the engine; nobody home is said at once when
//! no desktop has read the list, and a request nobody answers says so; the
//! workspace's word on who may ask is checked before anything is parked;
//! a tab is kept out of sight in a goal that runs unattended unless the
//! agent asks; a screenshot's upload comes back as a named copy's path; a
//! tab is at home where the engine says — the checkout, the goal, the
//! channel, the direct message, the conversation — for every kind of
//! session; a script is refused where the workspace says so.

use crate::common::{engine_with, intake_roundtrip, until};
use bisa_core::settings::Scope as SettingScope;
use bisa_core::{ConversationOrigin, GoalMode, SkillId};
use bisa_engine::browser::{
    BrowserAction, BrowserHome, BrowserHomeScope, BrowserRequest, BrowserResult, BrowserScope,
    BrowserTab, BROWSER_SKILL, DESKTOP_SILENT, LOCAL_ONLY, NOBODY_HOME, NOBODY_MAY, NOT_ASSIGNED,
    OFF, SCRIPTS_REFUSED,
};
use bisa_engine::SubmitRequest;
use bisa_store::{CatalogKind, NewAgent};
use bisa_store::{NewConversation, NewProject};
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn a_browser_op_waits_for_the_desktop_and_answers_what_it_said() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    // A desktop is home: it read the list, as an open one does every twenty seconds.
    engine.inner().browser.pending();
    let socket = engine.socket_path().to_path_buf();
    let call = tokio::spawn(async move {
        intake_roundtrip(
            &socket,
            json!({"op": "browser", "request": {"action": "tabs"}}),
        )
        .await
    });
    // The request is parked, on the bus and in the list a desktop reads.
    let pending = until("a parked browser request", || {
        let p = engine.inner().browser.pending();
        (!p.is_empty()).then_some(p)
    })
    .await;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].request.action, BrowserAction::Tabs);
    assert!(
        !pending[0].headless,
        "no goal: a conversation with a person in it"
    );
    assert_eq!(
        pending[0].scope.home, None,
        "no scope at all: the tab is the workspace's"
    );
    assert_eq!(
        pending[0].scope.agent.as_deref(),
        Some("general-agent"),
        "a session naming no agent asks as the General Agent"
    );
    let id = pending[0].id.clone();
    let answered = engine.inner().browser.answer(
        &id,
        BrowserResult {
            ok: true,
            tabs: vec![BrowserTab {
                key: "b1".into(),
                url: "http://127.0.0.1:4173/".into(),
                title: "Storefront".into(),
            }],
            ..BrowserResult::default()
        },
    );
    assert!(answered);
    assert!(
        !engine
            .inner()
            .browser
            .answer(&id, BrowserResult::refused("twice")),
        "answered once"
    );
    let reply = call.await.unwrap();
    assert_eq!(reply["ok"], json!(true));
    assert_eq!(reply["result"]["ok"], json!(true));
    assert_eq!(reply["result"]["tabs"][0]["key"], json!("b1"));
    assert!(
        engine.inner().browser.pending().is_empty(),
        "answered: no longer parked"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn nobody_home_is_said_at_once_when_no_desktop_has_read_the_list() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    let socket = engine.socket_path().to_path_buf();
    let started = std::time::Instant::now();
    let reply = intake_roundtrip(
        &socket,
        json!({"op": "browser", "request": {"action": "tabs"}}),
    )
    .await;
    assert_eq!(reply["ok"], json!(true));
    assert_eq!(reply["result"]["ok"], json!(false));
    assert_eq!(reply["result"]["error"], json!(NOBODY_HOME));
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "said at once, never after the answer timeout"
    );
    assert!(
        engine.inner().browser.pending().is_empty(),
        "nothing parked for nobody"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tab_is_out_of_sight_in_an_auto_goal_and_shown_in_a_guided_one_unless_the_agent_asks() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    engine.inner().browser.pending();
    let socket = engine.socket_path().to_path_buf();
    let auto = engine
        .submit_goal(SubmitRequest {
            mode: GoalMode::Auto,
            ..SubmitRequest::captured("Ship the storefront")
        })
        .unwrap();
    let guided = engine
        .submit_goal(SubmitRequest {
            mode: GoalMode::Guided,
            ..SubmitRequest::captured("Review the storefront")
        })
        .unwrap();
    let asked = |goal: bisa_core::GoalId, headless: Option<bool>| {
        let socket = socket.clone();
        let mut request = json!({"action": "open", "url": "http://localhost:5173/"});
        if let Some(h) = headless {
            request["headless"] = json!(h);
        }
        tokio::spawn(async move {
            intake_roundtrip(
                &socket,
                json!({"op": "browser", "goal": goal, "request": request}),
            )
            .await
        })
    };
    let parked = |what: &'static str| async {
        let p = until(what, || {
            let p = engine.inner().browser.pending();
            (!p.is_empty()).then_some(p)
        })
        .await;
        engine
            .inner()
            .browser
            .answer(&p[0].id, BrowserResult::refused("enough"));
        p[0].clone()
    };
    let call = asked(auto.id, None);
    let row = parked("the auto goal's request").await;
    assert!(
        row.headless,
        "an auto goal runs with nobody watching: out of sight"
    );
    assert_eq!(
        row.scope.home,
        Some(BrowserHome::new(BrowserHomeScope::Goal, auto.id)),
        "a goal-scoped session's tab is at home beside the goal"
    );
    call.await.unwrap();
    let call = asked(guided.id, None);
    assert!(
        !parked("the guided goal's request").await.headless,
        "a guided goal has a person"
    );
    call.await.unwrap();
    let call = asked(auto.id, Some(false));
    assert!(
        !parked("the auto goal's shown request").await.headless,
        "the agent's word: a person should watch"
    );
    call.await.unwrap();
    let call = asked(guided.id, Some(true));
    assert!(
        parked("the guided goal's headless request").await.headless,
        "… and out of sight when it asks so"
    );
    call.await.unwrap();
    // The policy's other two words.
    let ws = engine.workspace();
    ws.set_setting(
        SettingScope::Workspace,
        None,
        "browser.agents.headless",
        json!("never"),
    )
    .unwrap();
    let call = asked(auto.id, None);
    assert!(!parked("never: shown").await.headless);
    call.await.unwrap();
    ws.set_setting(
        SettingScope::Workspace,
        None,
        "browser.agents.headless",
        json!("always"),
    )
    .unwrap();
    let call = asked(guided.id, None);
    assert!(parked("always: out of sight").await.headless);
    call.await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_request_nobody_answers_says_so_and_an_unknown_id_answers_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    // A slot dropped under the waiter: a desktop was here, and went silent.
    let (tx, rx) = tokio::sync::watch::channel(None);
    drop(tx);
    let r = engine.inner().browser.wait("gone", rx).await;
    assert!(!r.ok);
    assert_eq!(
        r.error.as_deref(),
        Some(DESKTOP_SILENT),
        "a parked request had a desktop: its silence is not nobody home"
    );
    assert!(!engine
        .inner()
        .browser
        .answer("never-asked", BrowserResult::refused("x")));
}

/// The timeout, run in milliseconds through the mechanism `wait` uses: a
/// parked request the desktop never answers is forgotten and answers the
/// desktop's silence — a different sentence from nobody home, which is only
/// said before parking — and a late answer then finds nobody waiting.
#[tokio::test(flavor = "multi_thread")]
async fn a_desktop_that_never_answers_a_parked_request_is_silent_not_absent() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    engine.inner().browser.pending();
    let (id, rx) = engine.inner().browser.ask(
        engine.inner(),
        BrowserRequest {
            action: BrowserAction::Tabs,
            ..Default::default()
        },
        BrowserScope {
            home: None,
            agent: Some("general-agent".into()),
        },
        false,
    );
    assert_eq!(engine.inner().browser.pending().len(), 1);
    let r = engine
        .inner()
        .browser
        .wait_for(&id, rx, std::time::Duration::from_millis(50))
        .await;
    assert!(!r.ok);
    assert_eq!(r.error.as_deref(), Some(DESKTOP_SILENT));
    assert_ne!(DESKTOP_SILENT, NOBODY_HOME);
    assert!(
        DESKTOP_SILENT.contains("once more"),
        "the sentence says what to do: {DESKTOP_SILENT}"
    );
    assert!(
        engine.inner().browser.pending().is_empty(),
        "a request that timed out is forgotten"
    );
    assert!(
        !engine
            .inner()
            .browser
            .answer(&id, BrowserResult::refused("late")),
        "a late answer finds nobody waiting"
    );
}

/// An agent definition with the skills given — shared with `intake_scope`.
pub(crate) fn scout(skills: Vec<SkillId>) -> NewAgent {
    NewAgent {
        name: "Scout".into(),
        photo: None,
        description: None,
        system_prompt: "You are Scout.".into(),
        harness: "chat-harness".into(),
        models: Default::default(),
        skills,
        mcps: vec![],
        tags: Default::default(),
        respond: Default::default(),
        decision_making: false,
    }
}

fn errors_of(reply: &serde_json::Value) -> Vec<String> {
    reply["errors"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_op_refuses_by_the_workspace_policy_and_the_skill_is_the_assignment() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    engine.inner().browser.pending();
    let ws = engine.workspace();
    let socket = engine.socket_path().to_path_buf();
    let read = |agent: &str| json!({"op": "browser", "agent": agent, "request": {"action": "read", "tab": "b1"}});

    // Everyone, by default: an agent with no skill at all is parked like anyone's.
    let plain = ws.add_agent(scout(vec![])).unwrap();
    let plain_id = plain.id.to_string();
    let socket_default = socket.clone();
    let call =
        tokio::spawn(async move { intake_roundtrip(&socket_default, read(&plain_id)).await });
    let pending = until("the plain agent's request parked under the default", || {
        let p = engine.inner().browser.pending();
        (!p.is_empty()).then_some(p)
    })
    .await;
    assert_eq!(pending[0].scope.agent.as_deref(), Some(plain.id.as_str()));
    engine
        .inner()
        .browser
        .answer(&pending[0].id, BrowserResult::refused("no tab b1"));
    assert_eq!(call.await.unwrap()["result"]["error"], json!("no tab b1"));

    // Nobody: the core agents too.
    ws.set_setting(
        SettingScope::Workspace,
        None,
        "browser.agents",
        json!("nobody"),
    )
    .unwrap();
    let reply = intake_roundtrip(&socket, read("general-agent")).await;
    assert_eq!(reply["ok"], json!(false));
    assert_eq!(errors_of(&reply), vec![NOBODY_MAY.to_string()]);
    assert!(
        engine.inner().browser.pending().is_empty(),
        "refused before parking"
    );

    // Assigned: an agent without the skill is told what to attach; with it,
    // the request is parked like anyone's.
    ws.set_setting(
        SettingScope::Workspace,
        None,
        "browser.agents",
        json!("assigned"),
    )
    .unwrap();
    let bare = ws.add_agent(scout(vec![])).unwrap();
    let reply = intake_roundtrip(&socket, read(bare.id.as_str())).await;
    assert_eq!(errors_of(&reply), vec![NOT_ASSIGNED.to_string()]);
    ws.install(CatalogKind::Skill, BROWSER_SKILL).unwrap();
    let skilled = ws
        .add_agent(scout(vec![SkillId::new(BROWSER_SKILL).unwrap()]))
        .unwrap();
    let skilled_id = skilled.id.to_string();
    let socket2 = socket.clone();
    let call = tokio::spawn(async move { intake_roundtrip(&socket2, read(&skilled_id)).await });
    let pending = until("the skilled agent's request parked", || {
        let p = engine.inner().browser.pending();
        (!p.is_empty()).then_some(p)
    })
    .await;
    assert_eq!(pending[0].scope.agent.as_deref(), Some(skilled.id.as_str()));
    engine
        .inner()
        .browser
        .answer(&pending[0].id, BrowserResult::refused("no tab b1"));
    let reply = call.await.unwrap();
    assert_eq!(reply["ok"], json!(true));
    assert_eq!(reply["result"]["error"], json!("no tab b1"));

    // Everyone: the bare agent too — but only as far as the reach allows.
    ws.set_setting(
        SettingScope::Workspace,
        None,
        "browser.agents",
        json!("everyone"),
    )
    .unwrap();
    ws.set_setting(
        SettingScope::Workspace,
        None,
        "browser.agents.reach",
        json!("local_only"),
    )
    .unwrap();
    let open = |url: &str| json!({"op": "browser", "agent": bare.id.as_str(), "request": {"action": "open", "url": url}});
    let reply = intake_roundtrip(&socket, open("https://example.com/")).await;
    assert_eq!(errors_of(&reply), vec![LOCAL_ONLY.to_string()]);
    assert!(engine.inner().browser.pending().is_empty());

    // The machine's switch beats every policy.
    ws.set_setting(SettingScope::Machine, None, "browser.enabled", json!(false))
        .unwrap();
    let reply = intake_roundtrip(&socket, open("http://localhost:5173/")).await;
    assert_eq!(errors_of(&reply), vec![OFF.to_string()]);
}

/// One parked request for a scope, answered at once so the op returns; what
/// the engine said of the tab's home comes back.
async fn home_for(engine: &bisa_engine::Engine, op: serde_json::Value) -> Option<BrowserHome> {
    let socket = engine.socket_path().to_path_buf();
    let call = tokio::spawn(async move { intake_roundtrip(&socket, op).await });
    let pending = until("a parked request", || {
        let p = engine.inner().browser.pending();
        (!p.is_empty()).then_some(p)
    })
    .await;
    let home = pending[0].scope.home.clone();
    engine
        .inner()
        .browser
        .answer(&pending[0].id, BrowserResult::refused("enough"));
    call.await.unwrap();
    home
}

/// Where a tab is at home follows the session, for every kind: a channel's
/// turn beside the channel, a direct message's beside the message, a
/// conversation about a checkout in that checkout, about a workflow beside
/// the workflow, about the workspace beside the conversation itself — never
/// at a goal that does not exist, whatever the MCP scope defaulted `goal`
/// to — and a work item's in the checkout it was placed in.
#[tokio::test(flavor = "multi_thread")]
async fn a_tab_is_at_home_where_the_session_speaks_for_every_kind_of_scope() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    engine.inner().browser.pending();
    let ws = engine.workspace();
    // The envelope as the real client builds it: the scope id rides as the
    // candidate `goal` on every request (`Scope::apply`), which for a channel
    // or a DM is no ULID at all.
    let open = |scope: &str| {
        let mut op = json!({"op": "browser", "request": {"action": "open", "url": "http://localhost:5173/"}});
        bisa_mcp::Scope::Conversation {
            scope: scope.to_string(),
            agent: "general-agent".to_string(),
            goal: None,
        }
        .apply_for_test(&mut op);
        op
    };

    // A channel, by its slug.
    assert_eq!(
        home_for(&engine, open("general")).await,
        Some(BrowserHome::new(BrowserHomeScope::Channel, "general"))
    );

    // A direct message, by its id.
    let scout = ws.add_agent(scout(vec![])).unwrap();
    let dm = ws.open_dm(std::slice::from_ref(&scout.pubkey)).unwrap();
    assert_eq!(
        home_for(&engine, open(dm.id.as_str())).await,
        Some(BrowserHome::new(BrowserHomeScope::Dm, dm.id.clone()))
    );

    // A conversation about a checkout: in the checkout.
    let project = ws
        .create_project(NewProject::managed("web-app").unwrap())
        .unwrap();
    let primary = ws.primary_workstream(project.id).unwrap();
    let about_checkout = ws
        .create_conversation(NewConversation {
            origin: ConversationOrigin::Workstream {
                id: primary.id,
                project: project.id,
            },
            title: None,
            mode: bisa_core::ConversationMode::Auto,
        })
        .unwrap();
    assert_eq!(
        home_for(&engine, open(&about_checkout.id.to_string())).await,
        Some(BrowserHome::new(BrowserHomeScope::Workstream, primary.id))
    );

    // A conversation about the workspace: beside the conversation — and the
    // MCP scope's default `goal` (the scope id itself) names no goal.
    let about_workspace = ws
        .create_conversation(NewConversation {
            origin: ConversationOrigin::Workspace,
            title: None,
            mode: bisa_core::ConversationMode::Auto,
        })
        .unwrap();
    let scope = about_workspace.id.to_string();
    let with_bogus_goal = json!({"op": "browser", "scope": scope, "goal": scope, "request": {"action": "open", "url": "http://localhost:5173/"}});
    assert_eq!(
        home_for(&engine, with_bogus_goal).await,
        Some(BrowserHome::new(
            BrowserHomeScope::Conversation,
            about_workspace.id
        )),
        "a conversation's id is not a goal"
    );

    // A conversation about a goal: beside the goal.
    let goal = engine
        .submit_goal(SubmitRequest::captured("Ship it"))
        .unwrap();
    let about_goal = ws
        .create_conversation(NewConversation {
            origin: ConversationOrigin::Goal { id: goal.id },
            title: None,
            mode: bisa_core::ConversationMode::Auto,
        })
        .unwrap();
    assert_eq!(
        home_for(&engine, open(&about_goal.id.to_string())).await,
        Some(BrowserHome::new(BrowserHomeScope::Goal, goal.id))
    );

    // A goal's thread: the scope id is the goal.
    assert_eq!(
        home_for(&engine, open(&goal.id.to_string())).await,
        Some(BrowserHome::new(BrowserHomeScope::Goal, goal.id))
    );

    // A goal-scoped session that names a goal that exists.
    let named = json!({"op": "browser", "goal": goal.id, "request": {"action": "open", "url": "http://localhost:5173/"}});
    assert_eq!(
        home_for(&engine, named).await,
        Some(BrowserHome::new(BrowserHomeScope::Goal, goal.id))
    );
}

/// The scripts policy: `browser_eval` is refused before parking when the
/// workspace says so, and a project may say so for its own checkouts alone.
#[tokio::test(flavor = "multi_thread")]
async fn a_script_is_refused_where_the_workspace_or_the_project_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    engine.inner().browser.pending();
    let ws = engine.workspace();
    let socket = engine.socket_path().to_path_buf();
    // The screen is off here: what a page answers is content from outside,
    // and with nobody to ask it would be withheld — the screen's own rules
    // are `content.rs`'s to test; this test is about the scripts switch.
    ws.set_setting(
        SettingScope::Workspace,
        None,
        "security.content.screen",
        json!(false),
    )
    .unwrap();
    let eval = |scope: Option<String>| {
        let mut op = json!({"op": "browser", "request": {"action": "eval", "tab": "b1", "expression": "1 + 1"}});
        if let Some(s) = scope {
            op["scope"] = json!(s);
        }
        op
    };

    // Allowed by default: parked like any request.
    let socket1 = socket.clone();
    let op = eval(None);
    let call = tokio::spawn(async move { intake_roundtrip(&socket1, op).await });
    let pending = until("the eval parked", || {
        let p = engine.inner().browser.pending();
        (!p.is_empty()).then_some(p)
    })
    .await;
    assert_eq!(pending[0].request.action, BrowserAction::Eval);
    assert_eq!(pending[0].request.expression.as_deref(), Some("1 + 1"));
    engine.inner().browser.answer(
        &pending[0].id,
        BrowserResult {
            ok: true,
            value: Some(json!(2)),
            ..BrowserResult::default()
        },
    );
    assert_eq!(call.await.unwrap()["result"]["value"], json!(2));

    // Refused at the workspace.
    ws.set_setting(
        SettingScope::Workspace,
        None,
        "browser.agents.scripts",
        json!("refuse"),
    )
    .unwrap();
    let reply = intake_roundtrip(&socket, eval(None)).await;
    assert_eq!(errors_of(&reply), vec![SCRIPTS_REFUSED.to_string()]);
    assert!(
        engine.inner().browser.pending().is_empty(),
        "refused before parking"
    );
    let read = json!({"op": "browser", "request": {"action": "read", "tab": "b1"}});
    let socket2 = socket.clone();
    let call = tokio::spawn(async move { intake_roundtrip(&socket2, read).await });
    let pending = until("a read still parks", || {
        let p = engine.inner().browser.pending();
        (!p.is_empty()).then_some(p)
    })
    .await;
    engine
        .inner()
        .browser
        .answer(&pending[0].id, BrowserResult::refused("no tab b1"));
    call.await.unwrap();

    // A project allows what the workspace refuses, for a conversation about
    // its checkout — the project's word binds there and nowhere else.
    let project = ws
        .create_project(NewProject::managed("web-app").unwrap())
        .unwrap();
    ws.set_setting(
        SettingScope::Project,
        Some(project.id),
        "browser.agents.scripts",
        json!("allow"),
    )
    .unwrap();
    let primary = ws.primary_workstream(project.id).unwrap();
    let conversation = ws
        .create_conversation(NewConversation {
            origin: ConversationOrigin::Workstream {
                id: primary.id,
                project: project.id,
            },
            title: None,
            mode: bisa_core::ConversationMode::Auto,
        })
        .unwrap();
    let socket3 = socket.clone();
    let op = eval(Some(conversation.id.to_string()));
    let call = tokio::spawn(async move { intake_roundtrip(&socket3, op).await });
    let pending = until("the project's eval parked", || {
        let p = engine.inner().browser.pending();
        (!p.is_empty()).then_some(p)
    })
    .await;
    assert_eq!(
        pending[0].scope.home,
        Some(BrowserHome::new(BrowserHomeScope::Workstream, primary.id))
    );
    engine
        .inner()
        .browser
        .answer(&pending[0].id, BrowserResult::refused("enough"));
    call.await.unwrap();
    let reply = intake_roundtrip(&socket, eval(None)).await;
    assert_eq!(
        errors_of(&reply),
        vec![SCRIPTS_REFUSED.to_string()],
        "off the project the workspace's word stands"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_screenshot_answer_is_kept_as_a_named_copy_the_agent_reads_by_path() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    engine.inner().browser.pending();
    let ws = engine.workspace();
    // What the desktop uploads before it answers: a PNG's bytes, under a name.
    let png = ws
        .put_attachment(
            b"\x89PNG\r\n\x1a\nnot really",
            "browser-b1-01SHOT.png",
            "image/png",
        )
        .unwrap();
    let socket = engine.socket_path().to_path_buf();
    let call = tokio::spawn(async move {
        intake_roundtrip(
            &socket,
            json!({"op": "browser", "request": {"action": "screenshot", "tab": "b1"}}),
        )
        .await
    });
    let pending = until("a parked screenshot request", || {
        let p = engine.inner().browser.pending();
        (!p.is_empty()).then_some(p)
    })
    .await;
    assert_eq!(pending[0].request.action, BrowserAction::Screenshot);
    engine.inner().browser.answer(
        &pending[0].id,
        BrowserResult {
            ok: true,
            tab: Some("b1".into()),
            url: Some("http://localhost:5173/".into()),
            title: Some("Storefront".into()),
            screenshot: Some(png.clone()),
            width: Some(1280),
            height: Some(800),
            ..BrowserResult::default()
        },
    );
    let reply = call.await.unwrap();
    assert_eq!(reply["ok"], json!(true));
    let result = &reply["result"];
    assert_eq!(result["ok"], json!(true));
    assert_eq!(result["width"], json!(1280));
    let path = std::path::PathBuf::from(result["path"].as_str().expect("a path"));
    assert!(path.is_file(), "{}", path.display());
    assert_eq!(path.file_name().unwrap(), "browser-b1-01SHOT.png");
    assert!(
        path.starts_with(ws.paths().attachments_named_dir()),
        "the named copy lives beside the store, never where the desktop said"
    );
    assert_eq!(
        std::fs::read(&path).unwrap(),
        b"\x89PNG\r\n\x1a\nnot really"
    );
    assert_eq!(result["screenshot"]["sha256"], json!(png.sha256));

    // A screenshot this machine does not hold is not answered as a path.
    let socket = engine.socket_path().to_path_buf();
    let call = tokio::spawn(async move {
        intake_roundtrip(
            &socket,
            json!({"op": "browser", "request": {"action": "screenshot", "tab": "b1"}}),
        )
        .await
    });
    let pending = until("a second parked request", || {
        let p = engine.inner().browser.pending();
        (!p.is_empty()).then_some(p)
    })
    .await;
    let mut missing = png.clone();
    missing.sha256 = "f".repeat(64);
    engine.inner().browser.answer(
        &pending[0].id,
        BrowserResult {
            ok: true,
            screenshot: Some(missing),
            ..BrowserResult::default()
        },
    );
    let reply = call.await.unwrap();
    assert_eq!(reply["result"]["ok"], json!(false));
    assert!(reply["result"]["error"]
        .as_str()
        .unwrap()
        .contains("could not be kept"));
    assert!(reply["result"]["path"].is_null());
}

/// A folder server the node would lend: records what it was asked and
/// answers a URL of its own, so the op's plumbing is what is under test.
struct FakeServer {
    asked: std::sync::Mutex<Vec<(bisa_core::WorkstreamId, std::path::PathBuf, String)>>,
}

#[async_trait::async_trait]
impl bisa_engine::browser::FolderServer for FakeServer {
    async fn serve(
        &self,
        workstream: bisa_core::WorkstreamId,
        checkout: &std::path::Path,
        folder: &str,
    ) -> Result<bisa_engine::browser::ServedPage, String> {
        if folder == "nowhere" {
            return Err(format!("{folder:?} is not a folder of the checkout"));
        }
        self.asked
            .lock()
            .unwrap()
            .push((workstream, checkout.to_path_buf(), folder.to_string()));
        Ok(bisa_engine::browser::ServedPage {
            id: "s1".into(),
            url: "http://127.0.0.1:4173/".into(),
            page: "http://127.0.0.1:4173/".into(),
            folder: folder.to_string(),
        })
    }
}

/// `browser_serve` (ide/18): a session in a checkout gets a URL for a folder
/// of it through the server the node lent the engine; a session in no
/// checkout, and an engine with no server, are told so; the same word on
/// who may use the browser applies; the served folders' listeners hear.
#[tokio::test(flavor = "multi_thread")]
async fn browser_serve_serves_the_sessions_checkout_through_the_nodes_server_or_says_why_not() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    let ws = engine.workspace();
    let socket = engine.socket_path().to_path_buf();
    let project = ws
        .create_project(NewProject::managed("web-app").unwrap())
        .unwrap();
    let primary = ws.primary_workstream(project.id).unwrap();
    let about_checkout = ws
        .create_conversation(NewConversation {
            origin: ConversationOrigin::Workstream {
                id: primary.id,
                project: project.id,
            },
            title: None,
            mode: bisa_core::ConversationMode::Auto,
        })
        .unwrap();
    let scope = about_checkout.id.to_string();

    // No node lent a server: said in a sentence, nothing parked.
    let reply = intake_roundtrip(&socket, json!({"op": "browser_serve", "scope": scope})).await;
    assert_eq!(
        errors_of(&reply),
        vec![bisa_engine::browser::NO_SERVER.to_string()]
    );

    let server = std::sync::Arc::new(FakeServer {
        asked: std::sync::Mutex::new(Vec::new()),
    });
    engine.set_folder_server(std::sync::Arc::clone(&server) as _);
    let mut events = engine.events();

    // A conversation about a checkout: that checkout, the folder as named.
    let reply = intake_roundtrip(
        &socket,
        json!({"op": "browser_serve", "scope": scope, "folder": "site"}),
    )
    .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    assert_eq!(reply["result"]["url"], json!("http://127.0.0.1:4173/"));
    assert_eq!(reply["result"]["folder"], json!("site"));
    {
        let asked = server.asked.lock().unwrap();
        assert_eq!(asked.len(), 1);
        assert_eq!(asked[0].0, primary.id);
        assert_eq!(asked[0].1, ws.checkout_in(&project, &primary));
        assert_eq!(asked[0].2, "site");
    }
    let heard = until("the served folders moved", || loop {
        match events.try_recv() {
            Ok(ev) => {
                if let bisa_engine::EnginePayload::ServerChanged { workstream } = ev.payload {
                    return Some(workstream);
                }
            }
            Err(_) => return None,
        }
    })
    .await;
    assert_eq!(heard, Some(primary.id));

    // No folder named: the checkout itself.
    let reply = intake_roundtrip(&socket, json!({"op": "browser_serve", "scope": scope})).await;
    assert_eq!(reply["result"]["folder"], json!(""), "{reply}");

    // The server's refusal is the op's error, in its words.
    let reply = intake_roundtrip(
        &socket,
        json!({"op": "browser_serve", "scope": scope, "folder": "nowhere"}),
    )
    .await;
    assert!(
        errors_of(&reply)[0].contains("not a folder of the checkout"),
        "{reply}"
    );

    // A conversation about the workspace stands in no checkout.
    let about_workspace = ws
        .create_conversation(NewConversation {
            origin: ConversationOrigin::Workspace,
            title: None,
            mode: bisa_core::ConversationMode::Auto,
        })
        .unwrap();
    let reply = intake_roundtrip(
        &socket,
        json!({"op": "browser_serve", "scope": about_workspace.id.to_string()}),
    )
    .await;
    assert_eq!(
        errors_of(&reply),
        vec![bisa_engine::browser::NO_CHECKOUT.to_string()]
    );

    // The workspace's word on who may use the browser applies here too.
    ws.set_setting(
        SettingScope::Workspace,
        None,
        "browser.agents",
        json!("nobody"),
    )
    .unwrap();
    let reply = intake_roundtrip(&socket, json!({"op": "browser_serve", "scope": scope})).await;
    assert_eq!(errors_of(&reply), vec![NOBODY_MAY.to_string()]);
    engine.shutdown().await;
}
