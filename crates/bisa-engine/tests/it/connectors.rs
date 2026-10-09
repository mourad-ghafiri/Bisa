//! A `connector` step end to end: a definition the workspace holds, an
//! account whose secret lives in the memory keystore, and a loopback stub
//! standing in for the platform. Nothing leaves the machine, and no test
//! reads a secret from anywhere but the map it put it in.

use crate::common;

use bisa_core::{
    AccountId, AuthScheme, ConnectorId, Flow, HttpMethod, InputDef, InputKind, InputName,
    Operation, OperationBody, OperationId, OutputSpec, ParamKind, Part, PartSource, ProblemKind,
    RunOutcome, SecretField, SettingScope, StepKind, StepState, ValueRef,
};
use bisa_engine::connectors::{self, ConnectorRoster};
use bisa_engine::{Engine, EngineConfig, EnginePayload};
use bisa_harness::mock::MockAdapter;
use bisa_store::{NewConnector, NewConnectorAccount};
use common::*;
use serde_json::json;
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use common::stub::*;

#[tokio::test(flavor = "multi_thread")]
async fn a_connector_step_completes_with_the_selected_output() {
    let dir = tempfile::tempdir().unwrap();
    let stub = Stub::start(vec![canned(
        "POST",
        "/post",
        200,
        json!({"ok": true, "ts": "1.2"}),
    )])
    .await;
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    install_chat(&engine, &stub, Some(TOKEN));
    let (goal, _) = run_on(
        &engine,
        "the launch",
        new_workflow("posts", vec![post_step("say", None, None)]),
    );
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    assert_eq!(
        done.steps[&sid("say")].output,
        Some(json!("1.2")),
        "the selected field is the output"
    );
    let calls = stub.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].method, "POST");
    assert_eq!(calls[0].path, "/post");
    assert_eq!(
        calls[0].body["text"],
        json!("hello the launch"),
        "the param rendered against the run"
    );
    assert_eq!(
        calls[0].authorization.as_deref(),
        Some(&format!("Bearer {TOKEN}")[..])
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_500_fails_the_step_with_a_redacted_reason() {
    let dir = tempfile::tempdir().unwrap();
    let stub = Stub::start(vec![canned(
        "POST",
        "/post",
        500,
        json!({"message": format!("boom, and by the way your token is {TOKEN}")}),
    )])
    .await;
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    install_chat(&engine, &stub, Some(TOKEN));
    let (goal, _) = run_on(
        &engine,
        "the launch",
        new_workflow("posts", vec![post_step("say", None, None)]),
    );
    let failed = finished_run(&engine, goal.id).await;
    assert_eq!(failed.outcome, Some(RunOutcome::Failed));
    assert_eq!(failed.steps[&sid("say")].state, StepState::Failed);
    let why = failed.steps[&sid("say")].error.clone().unwrap_or_default();
    assert!(why.contains("500"), "{why}");
    assert!(!why.contains(TOKEN), "the reason carries no secret: {why}");
    for note in notes(&engine, goal.id) {
        assert!(
            !note.contains(TOKEN),
            "the journal carries no secret: {note}"
        );
    }
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_account_is_refused_before_a_run() {
    let dir = tempfile::tempdir().unwrap();
    let stub = Stub::start(vec![]).await;
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    // A bearer connector with no account at all.
    engine
        .workspace()
        .create_connector(chat(&stub, AuthScheme::Bearer))
        .unwrap();
    let err = engine
        .create_workflow(new_workflow("posts", vec![post_step("say", None, None)]))
        .unwrap_err();
    let bisa_engine::EngineError::Store(bisa_store::StoreError::WorkflowInvalid(problems)) = err
    else {
        panic!("expected the problems, got {err}");
    };
    assert!(
        problems
            .iter()
            .any(|p| p.kind == ProblemKind::UnknownAccount),
        "{problems:?}"
    );
    assert!(stub.calls().is_empty(), "nothing was called");
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_account_read_from_an_input_runs_the_call() {
    let dir = tempfile::tempdir().unwrap();
    let stub = Stub::start(vec![canned("POST", "/post", 200, json!({"ts": "9"}))]).await;
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let account = install_chat(&engine, &stub, Some(TOKEN)).unwrap();
    let mut draft = new_workflow(
        "posts as",
        vec![post_step(
            "say",
            Some(ValueRef::Input {
                input: InputName::new("who").unwrap(),
            }),
            None,
        )],
    );
    draft.inputs = vec![InputDef {
        name: InputName::new("who").unwrap(),
        label: "Who".into(),
        kind: InputKind::Account {
            connector: Some(ConnectorId::new("chat").unwrap()),
        },
        default: None,
        required: true,
    }];
    let (goal, _) = goal_on(&engine, "the launch", draft);
    // The wrong account is refused at start, with the validator's words.
    let stranger = AccountId::from_ulid(ulid::Ulid::from_parts(5, 5));
    let err = engine
        .start_run(
            goal.id,
            BTreeMap::from([("who".to_string(), json!(stranger.to_string()))]),
        )
        .unwrap_err();
    assert!(err.is_refusal(), "{err}");
    assert!(err.to_string().contains("not an account of"), "{err}");
    engine
        .start_run(
            goal.id,
            BTreeMap::from([("who".to_string(), json!(account.to_string()))]),
        )
        .unwrap();
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    assert_eq!(
        stub.calls()[0].authorization.as_deref(),
        Some(&format!("Bearer {TOKEN}")[..])
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_json_param_lands_as_a_typed_body_leaf() {
    let dir = tempfile::tempdir().unwrap();
    let stub = Stub::start(vec![canned("POST", "/post", 200, json!({"ts": "1"}))]).await;
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    install_chat(&engine, &stub, Some(TOKEN));
    let (goal, _) = run_on(
        &engine,
        "the launch",
        new_workflow(
            "posts",
            // Braces are doubled in a template; the rendered param is JSON.
            vec![post_step("say", None, Some(r#"{{"n": [1, 2]}}"#))],
        ),
    );
    finished_run(&engine, goal.id).await;
    assert_eq!(
        stub.calls()[0].body["extra"],
        json!({"n": [1, 2]}),
        "a json param is a value, not text"
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn oauth_start_complete_persists_tokens_and_a_bad_state_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let stub = Stub::start(vec![canned(
        "POST",
        "/token",
        200,
        json!({"access_token": "acc-1", "refresh_token": "ref-1", "expires_in": 3600, "token_type": "bearer", "scope": "chat"}),
    )])
    .await;
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let ws = engine.workspace();
    let mut def = chat(
        &stub,
        AuthScheme::OAuth2 {
            client_id_param: "client_id".into(),
            code_challenge: Default::default(),
            scope_join: Default::default(),
            authorization_url: format!("{}/auth", stub.base_url),
            token_url: format!("{}/token", stub.base_url),
            scopes: vec!["chat".into()],
            pkce: true,
            extra: BTreeMap::new(),
        },
    );
    def.check = None;
    ws.create_connector(def).unwrap();
    let cid = ConnectorId::new("chat").unwrap();
    let account = ws
        .create_connector_account(NewConnectorAccount {
            connector: cid.clone(),
            label: "me".into(),
            params: BTreeMap::new(),
            default: true,
        })
        .unwrap();
    // No client id yet: nothing to start with.
    let err = connectors::oauth_start(engine.inner(), &cid, account.id, 4478).unwrap_err();
    assert!(err.to_string().contains("client id"), "{err}");
    ws.set_connector_secrets(
        &cid,
        account.id,
        &BTreeMap::from([(SecretField::ClientId, "my-app".to_string())]),
    )
    .unwrap();
    let start = connectors::oauth_start(engine.inner(), &cid, account.id, 4478).unwrap();
    assert!(
        start.url.starts_with(&format!("{}/auth?", stub.base_url)),
        "{}",
        start.url
    );
    assert!(start.url.contains("client_id=my-app"), "{}", start.url);
    assert!(
        start.url.contains("code_challenge_method=S256"),
        "{}",
        start.url
    );
    assert_eq!(
        start.redirect_uri,
        "http://127.0.0.1:4478/connectors/oauth/callback"
    );
    assert_eq!(connectors::oauth_pending(engine.inner()), 1);
    let state = url_query(&start.url, "state").expect("a state");
    let err = connectors::oauth_complete(engine.inner(), "not-a-state", "code")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("unknown or expired"), "{err}");
    let (c, a) = connectors::oauth_complete(engine.inner(), &state, "the-code")
        .await
        .unwrap();
    assert_eq!((c, a), (cid.clone(), account.id));
    assert_eq!(connectors::oauth_pending(engine.inner()), 0);
    assert_eq!(
        ws.connector_secret(&cid, account.id, SecretField::AccessToken)
            .unwrap()
            .as_deref(),
        Some("acc-1")
    );
    assert_eq!(
        ws.connector_secret(&cid, account.id, SecretField::RefreshToken)
            .unwrap()
            .as_deref(),
        Some("ref-1")
    );
    let stored = ws.get_connector_account(&cid, account.id).unwrap();
    assert!(stored.auth.expires_at.is_some_and(|t| t > 0));
    assert_eq!(stored.auth.scope.as_deref(), Some("chat"));
    let exchange = &stub.calls()[0];
    assert_eq!(exchange.path, "/token");
    // A second completion of the same state is refused: the flow is spent.
    assert!(connectors::oauth_complete(engine.inner(), &state, "again")
        .await
        .is_err());
    engine.shutdown().await;
}

fn url_query(url: &str, name: &str) -> Option<String> {
    let (_, query) = url.split_once('?')?;
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == name).then(|| v.to_string())
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn the_roster_names_every_connector_operation_and_account_label() {
    let dir = tempfile::tempdir().unwrap();
    let stub = Stub::start(vec![]).await;
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    assert!(ConnectorRoster::of(engine.workspace()).unwrap().is_empty());
    install_chat(&engine, &stub, Some(TOKEN));
    let roster = ConnectorRoster::of(engine.workspace()).unwrap();
    assert!(roster.holds(
        &ConnectorId::new("chat").unwrap(),
        &OperationId::new("post").unwrap()
    ));
    let text = roster.render();
    assert!(text.contains("- chat \"Chat\" — A chat platform on the stub. [auth: bearer; accounts: work (default)]"), "{text}");
    assert!(text.contains("· post \"Post\" — Posts a message. [POST; params: text (text, required), extra (json); selects ts; writes]"), "{text}");
    assert!(text.contains("· whoami \"Who am I\""), "{text}");
    assert!(!text.contains(TOKEN));
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn list_connectors_answers_the_roster() {
    let dir = tempfile::tempdir().unwrap();
    let stub = Stub::start(vec![]).await;
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    install_chat(&engine, &stub, None);
    let reply = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "list_connectors", "agent": "workflow-agent"}),
    )
    .await;
    assert_eq!(reply["ok"], true, "{reply}");
    assert!(
        reply["text"]
            .as_str()
            .unwrap()
            .starts_with("CONNECTORS — installed here"),
        "{reply}"
    );
    assert_eq!(reply["connectors"][0]["id"], json!("chat"));
    assert_eq!(
        reply["connectors"][0]["operations"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    // The General Agent and the Workflow Agent read the roster: the General Agent reads it for
    // `call_connector`, the Workflow Agent for a `connector` step.
    let general = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "list_connectors", "agent": "general-agent"}),
    )
    .await;
    assert_eq!(general["ok"], true, "{general}");
    assert_eq!(general["connectors"][0]["id"], json!("chat"));
    // A worker's session names its item, not an agent — the engine resolves
    // who works it — and a cycle a person drives by hand names nobody: the
    // roster is every session's to read.
    for asked_by in [
        json!({"op": "list_connectors", "work_item": "01ARZ3NDEKTSV4RRFFQ69G5FAV"}),
        json!({"op": "list_connectors"}),
    ] {
        let reply = intake_roundtrip(engine.socket_path(), asked_by.clone()).await;
        assert_eq!(reply["ok"], true, "{asked_by}: {reply}");
        assert_eq!(reply["connectors"][0]["id"], json!("chat"), "{asked_by}");
    }
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_host_denied_by_policy_is_refused_and_journaled() {
    let dir = tempfile::tempdir().unwrap();
    let stub = Stub::start(vec![canned("POST", "/post", 200, json!({"ts": "1"}))]).await;
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    install_chat(&engine, &stub, Some(TOKEN));
    engine
        .workspace()
        .set_setting(
            SettingScope::Workspace,
            None,
            "security.net.deny_hosts",
            json!([stub.host.clone()]),
        )
        .unwrap();
    engine.inner().security.invalidate();
    let mut rx = engine.events();
    let (goal, _) = run_on(
        &engine,
        "the launch",
        new_workflow("posts", vec![post_step("say", None, None)]),
    );
    let failed = finished_run(&engine, goal.id).await;
    assert_eq!(failed.outcome, Some(RunOutcome::Failed));
    let why = failed.steps[&sid("say")].error.clone().unwrap_or_default();
    assert!(why.contains("deny_hosts"), "{why}");
    assert!(stub.calls().is_empty(), "the request never left");
    wait_for(
        &mut rx,
        "the guard decision",
        |e| matches!(&e.payload, EnginePayload::GuardDecided { tool, .. } if tool == "connector"),
    )
    .await;
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_timed_out_call_fails_the_step() {
    let dir = tempfile::tempdir().unwrap();
    // A listener that accepts and never answers.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let mut held = Vec::new();
        loop {
            if let Ok((socket, _)) = listener.accept().await {
                held.push(socket);
            }
        }
    });
    let engine = Engine::start(
        workspace(&dir),
        catalog_with(vec![MockAdapter::default()]),
        EngineConfig {
            connector_timeout_secs: 1,
            ..design_off_config()
        },
    )
    .unwrap();
    let stub = Stub {
        base_url: format!("http://{addr}"),
        host: addr.to_string(),
        shared: Shared {
            answers: Arc::new(Mutex::new(VecDeque::new())),
            calls: Arc::new(Mutex::new(Vec::new())),
        },
    };
    install_chat(&engine, &stub, None);
    let (goal, _) = run_on(
        &engine,
        "the launch",
        new_workflow("posts", vec![post_step("say", None, None)]),
    );
    let failed = finished_run(&engine, goal.id).await;
    assert_eq!(failed.outcome, Some(RunOutcome::Failed));
    let why = failed.steps[&sid("say")].error.clone().unwrap_or_default();
    assert!(why.contains("timed out"), "{why}");
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn check_account_runs_the_check_operation() {
    let dir = tempfile::tempdir().unwrap();
    let stub = Stub::start(vec![
        canned("GET", "/whoami", 200, json!({"user": "me"})),
        canned("GET", "/whoami", 401, json!({"error": "invalid_auth"})),
    ])
    .await;
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let account = install_chat(&engine, &stub, Some(TOKEN)).unwrap();
    let cid = ConnectorId::new("chat").unwrap();
    let ok = connectors::check_account(
        engine.inner(),
        &cid,
        account,
        connectors::check_budget(None),
    )
    .await
    .unwrap();
    assert_eq!(ok.state, connectors::AccountCheckState::Connected);
    assert_eq!(ok.status, Some(200));
    let refused = connectors::check_account(
        engine.inner(),
        &cid,
        account,
        connectors::check_budget(None),
    )
    .await
    .unwrap();
    assert_eq!(refused.state, connectors::AccountCheckState::Refused);
    assert_eq!(refused.status, Some(401));
    assert!(!refused.reason.unwrap_or_default().contains(TOKEN));
    // The doors announce, and a forgotten account takes its secrets with it.
    let mut rx = engine.events();
    connectors::delete_account(engine.inner(), &cid, account).unwrap();
    wait_for(&mut rx, "the accounts change", |e| {
        matches!(&e.payload, EnginePayload::ConnectorsChanged { what } if what.as_str() == "accounts")
    })
    .await;
    assert!(engine
        .workspace()
        .connector_secret(&cid, account, SecretField::Token)
        .unwrap()
        .is_none());
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_catalog_connector_is_not_edited_through_the_door() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    engine
        .workspace()
        .install(bisa_store::CatalogKind::Connector, "slack")
        .unwrap();
    let def = engine
        .workspace()
        .get_connector(&ConnectorId::new("slack").unwrap())
        .unwrap();
    let err = connectors::update_connector(engine.inner(), def).unwrap_err();
    assert!(err.to_string().contains("catalog"), "{err}");
    let _ = Flow::to(sid("x"));
    engine.shutdown().await;
}

/// A rendered parameter is never restored: a placeholder still in it — a
/// secret redacted before a restart, or one an agent wrote into the statement
/// — would reach the host as literal text. The step is refused before the
/// request exists, by the same rule a command gets.
#[tokio::test(flavor = "multi_thread")]
async fn a_parameter_still_carrying_a_placeholder_never_reaches_the_host() {
    let dir = tempfile::tempdir().unwrap();
    let stub = Stub::start(vec![canned("POST", "/post", 200, json!({"ts": "1"}))]).await;
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    install_chat(&engine, &stub, Some(TOKEN));
    let mut rx = engine.events();
    let (goal, _) = run_on(
        &engine,
        "post «secret:github_token:0000deadbeef» to the channel",
        new_workflow("posts", vec![post_step("say", None, None)]),
    );
    let failed = finished_run(&engine, goal.id).await;
    assert_eq!(failed.outcome, Some(RunOutcome::Failed));
    let why = failed.steps[&sid("say")].error.clone().unwrap_or_default();
    assert!(
        why.contains("placeholder") && why.contains("not sent"),
        "{why}"
    );
    assert!(stub.calls().is_empty(), "the request never left");
    let decided = wait_for(
        &mut rx,
        "the guard decision",
        |e| matches!(&e.payload, EnginePayload::GuardDecided { tool, .. } if tool == "connector"),
    )
    .await;
    assert!(
        matches!(&decided.payload, EnginePayload::GuardDecided { rule: Some(rule), .. } if rule == "unresolved_placeholder"),
        "{:?}",
        decided.payload
    );
    engine.shutdown().await;
}

/// A platform with one upload: metadata as a text part, the video as a file
/// part — the shape every media API takes.
pub(crate) fn media(stub: &Stub) -> NewConnector {
    NewConnector {
        id: ConnectorId::new("media").unwrap(),
        name: "Media".into(),
        description: "A media platform on the stub.".into(),
        tags: Default::default(),
        base_url: stub.base_url.clone(),
        hosts: vec![stub.host.clone()],
        insecure_tls: false,
        auth: AuthScheme::None,
        params: vec![],
        operations: vec![Operation {
            id: OperationId::new("upload").unwrap(),
            name: "Upload".into(),
            description: "Uploads a video with its metadata.".into(),
            method: HttpMethod::Post,
            path: "/upload".into(),
            query: BTreeMap::new(),
            headers: BTreeMap::new(),
            body: Some(OperationBody::Multipart {
                parts: vec![
                    Part {
                        name: "metadata".into(),
                        source: PartSource::Text {
                            text: "{params.meta}".into(),
                        },
                        filename: None,
                        content_type: Some("application/json".into()),
                    },
                    Part {
                        name: "media".into(),
                        source: PartSource::File {
                            file: InputName::new("video").unwrap(),
                        },
                        filename: None,
                        content_type: Some("video/mp4".into()),
                    },
                ],
            }),
            params: vec![
                param("meta", ParamKind::Json, true),
                param("video", ParamKind::File, true),
            ],
            output: OutputSpec {
                expect: None,
                select: Some("id".into()),
                schema: None,
            },
            writes: true,
            timeout_secs: None,
            idempotency: None,
            page: None,
        }],
        check: None,
    }
}

pub(crate) fn upload_step(id: &str, video: &str) -> bisa_core::Step {
    step(
        id,
        StepKind::Connector {
            connector: Some(ConnectorId::new("media").unwrap()),
            operation: Some(OperationId::new("upload").unwrap()),
            account: None,
            params: BTreeMap::from([
                (
                    "meta".to_string(),
                    // A literal brace is doubled; the placeholder inside stays one.
                    r#"{{"title":"{goal.statement}"}}"#.to_string(),
                ),
                ("video".to_string(), video.to_string()),
            ]),
            output_schema: None,
            // The fixture's write is the person's word; the gate rule has its
            // own tests in the core.
            unattended: true,
        },
    )
}

/// A `file` parameter is a path inside where the run's work landed — the
/// goal's scratch folder when no project is attached — read when the step
/// runs and sent as a multipart part; the model never saw the bytes.
#[tokio::test(flavor = "multi_thread")]
async fn a_file_parameter_is_read_from_the_runs_checkout_and_uploaded_as_a_part() {
    let dir = tempfile::tempdir().unwrap();
    let stub = Stub::start(vec![canned("POST", "/upload", 200, json!({"id": "v1"}))]).await;
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let ws = engine.workspace();
    ws.create_connector(media(&stub)).unwrap();
    let (goal, _) = goal_on(
        &engine,
        "the launch",
        new_workflow("uploads", vec![upload_step("send", "renders/clip.mp4")]),
    );
    let scratch = ws.paths().goal(goal.id).scratch();
    std::fs::create_dir_all(scratch.join("renders")).unwrap();
    std::fs::write(scratch.join("renders/clip.mp4"), b"MOVIE").unwrap();
    engine.start_run(goal.id, BTreeMap::new()).unwrap();
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(
        done.outcome,
        Some(RunOutcome::Done),
        "{:?}",
        done.steps[&sid("send")].error
    );
    assert_eq!(done.steps[&sid("send")].output, Some(json!("v1")));
    let calls = stub.calls();
    assert_eq!(calls.len(), 1);
    let content_type = calls[0].content_type.clone().unwrap_or_default();
    assert!(
        content_type.starts_with("multipart/form-data; boundary=bisa-"),
        "{content_type}"
    );
    assert!(
        calls[0].text.contains("name=\"metadata\"\r\nContent-Type: application/json\r\n\r\n{\"title\":\"the launch\"}\r\n"),
        "the text part rendered against the run: {}",
        calls[0].text
    );
    assert!(
        calls[0].text.contains(
            "name=\"media\"; filename=\"clip.mp4\"\r\nContent-Type: video/mp4\r\n\r\nMOVIE\r\n"
        ),
        "the file's bytes and its own name: {}",
        calls[0].text
    );
    engine.shutdown().await;
}

/// A path that climbs out of the checkout, or names nothing there, fails the
/// step by name before any request exists.
#[tokio::test(flavor = "multi_thread")]
async fn a_file_outside_the_runs_checkout_is_refused_and_nothing_is_sent() {
    let dir = tempfile::tempdir().unwrap();
    let stub = Stub::start(vec![]).await;
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    engine.workspace().create_connector(media(&stub)).unwrap();
    let (goal, _) = run_on(
        &engine,
        "the launch",
        new_workflow(
            "uploads",
            vec![upload_step("send", "../elsewhere/clip.mp4")],
        ),
    );
    let failed = finished_run(&engine, goal.id).await;
    assert_eq!(failed.outcome, Some(RunOutcome::Failed));
    let why = failed.steps[&sid("send")].error.clone().unwrap_or_default();
    assert!(why.contains("`video`") && why.contains("no `..`"), "{why}");
    let (goal, _) = run_on(
        &engine,
        "the launch",
        new_workflow("uploads2", vec![upload_step("send", "renders/missing.mp4")]),
    );
    let failed = finished_run(&engine, goal.id).await;
    let why = failed.steps[&sid("send")].error.clone().unwrap_or_default();
    assert!(why.contains("renders/missing.mp4"), "{why}");
    assert!(stub.calls().is_empty(), "nothing reached the platform");
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// How a call holds up: an ambiguous write stops, a keyed one is sent again
// with the same key, an echoed secret never reaches the output, and an
// agent reads through a connector but never writes.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn an_unkeyed_write_that_times_out_is_stopped_whatever_its_retries() {
    let dir = tempfile::tempdir().unwrap();
    // The platform answers after three seconds; the operation gives it one.
    let stub = Stub::start(vec![slow(
        "POST",
        "/post",
        200,
        json!({"ts": "late"}),
        3_000,
    )])
    .await;
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    install_chat_shaped(&engine, &stub, Some(TOKEN), |post| {
        post.timeout_secs = Some(1)
    });
    let mut say = post_step("say", None, None);
    say.retries = 2;
    let (goal, _) = run_on(&engine, "the launch", new_workflow("posts", vec![say]));
    let failed = finished_run(&engine, goal.id).await;
    assert_eq!(failed.outcome, Some(RunOutcome::Failed));
    let record = &failed.steps[&sid("say")];
    assert_eq!(record.state, StepState::Failed);
    assert_eq!(
        record.attempts, 1,
        "no retry was spent on a write the platform may hold"
    );
    let why = record.error.clone().unwrap_or_default();
    assert!(
        why.contains("check there before running this step again"),
        "{why}"
    );
    assert!(
        why.contains("idempotency header"),
        "the way out is named: {why}"
    );
    tokio::time::sleep(std::time::Duration::from_millis(3_500)).await;
    assert_eq!(stub.calls().len(), 1, "sent once, never again");
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_keyed_write_is_sent_again_with_the_same_key() {
    let dir = tempfile::tempdir().unwrap();
    let stub = Stub::start(vec![
        canned("POST", "/post", 503, json!({"message": "busy"})),
        canned("POST", "/post", 200, json!({"ok": true, "ts": "9.9"})),
    ])
    .await;
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    install_chat_shaped(&engine, &stub, Some(TOKEN), |post| {
        post.idempotency = Some(bisa_core::Idempotency {
            header: "Idempotency-Key".into(),
        })
    });
    let (goal, run) = run_on(
        &engine,
        "the launch",
        new_workflow("posts", vec![post_step("say", None, None)]),
    );
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    assert_eq!(done.steps[&sid("say")].output, Some(json!("9.9")));
    let calls = stub.calls();
    assert_eq!(calls.len(), 2, "the 503 was retried");
    let expected = connectors::step_key(run.id, &sid("say"));
    for call in &calls {
        assert_eq!(
            call.idempotency_key.as_deref(),
            Some(expected.as_str()),
            "the same key on every attempt"
        );
    }
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_secret_the_platform_echoes_never_reaches_the_steps_output() {
    let dir = tempfile::tempdir().unwrap();
    let stub = Stub::start(vec![canned(
        "POST",
        "/post",
        200,
        json!({"ok": true, "ts": format!("posted with {TOKEN}")}),
    )])
    .await;
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    install_chat(&engine, &stub, Some(TOKEN));
    let (goal, _) = run_on(
        &engine,
        "the launch",
        new_workflow("posts", vec![post_step("say", None, None)]),
    );
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    let output = done.steps[&sid("say")]
        .output
        .clone()
        .unwrap_or_default()
        .to_string();
    assert!(
        !output.contains(TOKEN),
        "the answer went through the redactor: {output}"
    );
    assert!(
        output.contains("posted with"),
        "the rest of the answer stands: {output}"
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_reads_through_a_connector_and_is_refused_a_write() {
    let dir = tempfile::tempdir().unwrap();
    let stub = Stub::start(vec![canned(
        "GET",
        "/whoami",
        200,
        json!({"user": "stub", "team": "T1"}),
    )])
    .await;
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    install_chat(&engine, &stub, Some(TOKEN));
    // Any agent lists the connectors; the roster carries no secret.
    let listed = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "list_connectors", "agent": "researcher"}),
    )
    .await;
    assert_eq!(listed["ok"], json!(true), "{listed}");
    assert!(
        listed["text"].as_str().unwrap_or("").contains("chat"),
        "{listed}"
    );
    assert!(!listed.to_string().contains(TOKEN));
    // A read goes out as the default account and comes back screened.
    let read = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "call_connector", "connector": "chat", "operation": "whoami", "agent": "researcher"}),
    )
    .await;
    assert_eq!(read["ok"], json!(true), "{read}");
    assert!(
        read.get("output").is_some() || read["withheld"] == json!(true),
        "the answer, or the screen's sentence: {read}"
    );
    assert!(!read.to_string().contains(TOKEN), "{read}");
    assert_eq!(stub.calls().len(), 1);
    assert_eq!(stub.calls()[0].path, "/whoami");
    // A write is a workflow step behind a gate, never a call.
    let refused = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "call_connector", "connector": "chat", "operation": "post", "params": {"text": "hi"}, "agent": "researcher"}),
    )
    .await;
    assert_eq!(refused["ok"], json!(false), "{refused}");
    assert!(
        refused["errors"][0]
            .as_str()
            .unwrap_or("")
            .contains("writes"),
        "{refused}"
    );
    assert_eq!(stub.calls().len(), 1, "nothing was posted");
    engine.shutdown().await;
}

/// The engine's start brings an installed catalog connector to the bundle's
/// revision — the definition rewritten, the credential moved to the field
/// the new scheme reads — and says so in the activity feed, since the bus
/// has nobody yet. A plain open never did it: this is a truth write, under
/// the engine's lock.
#[tokio::test(flavor = "multi_thread")]
async fn the_start_refreshes_an_installed_catalog_connector_and_moves_its_secret() {
    let dir = tempfile::tempdir().unwrap();
    let cid = ConnectorId::new("linear").unwrap();
    let account = {
        let ws = workspace(&dir);
        ws.install(bisa_store::CatalogKind::Connector, "linear")
            .unwrap();
        // The copy an earlier bundle installed: the bearer scheme, no revision.
        let mut old = ws.get_connector(&cid).unwrap();
        old.revision = 0;
        old.auth = AuthScheme::Bearer;
        ws.update_connector(old).unwrap();
        let account = ws
            .create_connector_account(NewConnectorAccount {
                connector: cid.clone(),
                label: "work".into(),
                params: BTreeMap::new(),
                default: true,
            })
            .unwrap();
        ws.set_connector_secrets(
            &cid,
            account.id,
            &BTreeMap::from([(SecretField::Token, "lin_api_1".to_string())]),
        )
        .unwrap();
        account.id
    };
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let ws = engine.workspace();
    let fresh = ws.get_connector(&cid).unwrap();
    assert!(
        matches!(fresh.auth, AuthScheme::ApiKey { .. }),
        "the bundle's scheme: {:?}",
        fresh.auth
    );
    assert!(fresh.revision >= 1);
    assert_eq!(
        ws.connector_secret(&cid, account, SecretField::ApiKey)
            .unwrap()
            .as_deref(),
        Some("lin_api_1")
    );
    assert_eq!(
        ws.connector_secret(&cid, account, SecretField::Token)
            .unwrap(),
        None
    );
    let whats: Vec<String> = ws
        .activity_page(None, None, 200)
        .unwrap()
        .into_iter()
        .filter(|r| r.kind == "connectors_changed")
        .filter_map(|r| serde_json::from_str::<serde_json::Value>(&r.event).ok())
        .filter_map(|e| e["what"].as_str().map(str::to_string))
        .collect();
    assert!(
        whats.contains(&"definitions".to_string()) && whats.contains(&"accounts".to_string()),
        "the feed holds both facts: {whats:?}"
    );
    engine.shutdown().await;
}

/// An OAuth2 scheme's consent page and token endpoint are hosts the scheme
/// declares by its own URLs: a connection starts without a hand-written
/// allow list, the deny list still wins, and connecting again forgets what
/// *Check* said about the tokens held before.
#[tokio::test(flavor = "multi_thread")]
async fn an_oauth_consent_page_needs_no_allow_list_and_connecting_again_forgets_the_health() {
    let dir = tempfile::tempdir().unwrap();
    let stub = Stub::start(vec![
        canned("GET", "/whoami", 200, json!({"user": "me"})),
        canned(
            "POST",
            "/token",
            200,
            json!({"access_token": "acc-2", "refresh_token": "ref-2", "expires_in": 3600, "token_type": "bearer"}),
        ),
    ])
    .await;
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let ws = engine.workspace();
    let def = chat(
        &stub,
        AuthScheme::OAuth2 {
            // Not one of the connector's hosts — the scheme's own.
            authorization_url: "https://consent.example.test/auth".into(),
            token_url: format!("{}/token", stub.base_url),
            scopes: vec!["chat".into()],
            pkce: true,
            extra: BTreeMap::new(),
            client_id_param: "client_id".into(),
            scope_join: Default::default(),
            code_challenge: Default::default(),
        },
    );
    let created = ws.create_connector(def).unwrap();
    assert_eq!(
        created.oauth_hosts(),
        vec!["consent.example.test".to_string(), stub.host.clone()]
    );
    let cid = ConnectorId::new("chat").unwrap();
    let account = ws
        .create_connector_account(NewConnectorAccount {
            connector: cid.clone(),
            label: "me".into(),
            params: BTreeMap::new(),
            default: true,
        })
        .unwrap();
    ws.set_connector_secrets(
        &cid,
        account.id,
        &BTreeMap::from([
            (SecretField::ClientId, "my-app".to_string()),
            (SecretField::AccessToken, "acc-1".to_string()),
        ]),
    )
    .unwrap();
    let health = &engine.inner().connector_health;
    health
        .check(engine.inner(), &cid, account.id, None)
        .await
        .unwrap();
    assert_eq!(
        health.view_of(&cid, account.id).state,
        bisa_engine::connector_health::AccountHealthState::Ok
    );

    let start = connectors::oauth_start(engine.inner(), &cid, account.id, 4478)
        .expect("the consent page is the scheme's own host");
    assert!(
        start.url.starts_with("https://consent.example.test/auth?"),
        "{}",
        start.url
    );
    let state = url_query(&start.url, "state").expect("a state");
    connectors::oauth_complete(engine.inner(), &state, "the-code")
        .await
        .unwrap();
    assert_eq!(
        health.view_of(&cid, account.id).state,
        bisa_engine::connector_health::AccountHealthState::Unknown,
        "connected again: what was checked is not what is held"
    );

    // The deny list is read ahead of the scheme's own hosts.
    ws.set_setting(
        SettingScope::Workspace,
        None,
        "security.net.deny_hosts",
        json!(["consent.example.test"]),
    )
    .unwrap();
    engine.inner().security.invalidate();
    let err = connectors::oauth_start(engine.inner(), &cid, account.id, 4478).unwrap_err();
    assert!(err.to_string().contains("deny_hosts"), "{err}");
    engine.shutdown().await;
}

/// The node's own deadline on a connector call (`connector_timeout_secs`),
/// when the operation sets none: a platform that answers too late fails the
/// step with the deadline it had.
#[tokio::test(flavor = "multi_thread")]
async fn the_nodes_deadline_ends_a_call_the_operation_set_none_for() {
    let dir = tempfile::tempdir().unwrap();
    let stub = Stub::start(vec![slow(
        "POST",
        "/post",
        200,
        json!({"ts": "late"}),
        3_000,
    )])
    .await;
    let engine = Engine::start(
        workspace(&dir),
        catalog_with(vec![MockAdapter::default()]),
        EngineConfig {
            connector_timeout_secs: 1,
            ..design_off_config()
        },
    )
    .unwrap();
    install_chat(&engine, &stub, Some(TOKEN));
    let (goal, _) = run_on(
        &engine,
        "a late platform",
        new_workflow("posts", vec![post_step("say", None, None)]),
    );
    let failed = finished_run(&engine, goal.id).await;
    assert_eq!(failed.outcome, Some(RunOutcome::Failed), "{failed:?}");
    let why = failed.steps[&sid("say")].error.clone().unwrap_or_default();
    assert!(why.contains("1s") || why.contains("timed out"), "{why}");
    engine.shutdown().await;
}
