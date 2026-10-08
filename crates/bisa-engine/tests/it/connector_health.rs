//! What this machine's connector accounts answered when last checked (03 §
//! Connectors), against the stub platform: a check is kept and told to the
//! bus, a second ask joins the first, a budget ends a check the platform
//! never answers and the next ask dials again, the doors forget the answer,
//! a connector with no check keeps nothing, and a check dropped mid-flight
//! leaves no slot behind for the next ask.

use crate::common::stub::*;
use crate::common::*;
use bisa_core::{ConnectorId, SecretField};
use bisa_engine::connector_health::{AccountHealthState, AccountHealthView, DROPPED};
use bisa_engine::connectors::{self, AccountCheckState};
use bisa_engine::EnginePayload;
use bisa_harness::mock::MockAdapter;
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

fn cid() -> ConnectorId {
    ConnectorId::new("chat").unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_check_is_kept_as_the_accounts_health_and_told_to_the_bus() {
    let dir = tempfile::tempdir().unwrap();
    let stub = Stub::start(vec![
        canned("GET", "/whoami", 200, json!({"user": "me"})),
        canned("GET", "/whoami", 401, json!({"error": "invalid_auth"})),
    ])
    .await;
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let account = install_chat(&engine, &stub, Some(TOKEN)).unwrap();
    let health = &engine.inner().connector_health;
    assert_eq!(
        health.view_of(&cid(), account),
        AccountHealthView::unknown(),
        "nothing until asked"
    );
    let mut bus = engine.events();

    let check = health
        .check(engine.inner(), &cid(), account, None)
        .await
        .unwrap();
    assert_eq!(check.state, AccountCheckState::Connected);
    let view = health.view_of(&cid(), account);
    assert_eq!(view.state, AccountHealthState::Ok);
    assert_eq!(view.check, Some(AccountCheckState::Connected));
    assert_eq!(view.status, Some(200));
    assert!(view.checked_at.is_some());
    assert_eq!(view.reason, None);
    let frame = until("the checked frame", || {
        while let Ok(e) = bus.try_recv() {
            if let EnginePayload::ConnectorChecked {
                connector,
                account: who,
                ok,
            } = e.payload.clone()
            {
                return Some((connector, who, ok));
            }
        }
        None
    })
    .await;
    assert_eq!(frame, (cid(), account, true));

    // A failure is a health too, in the platform's words, the token nowhere.
    let refused = health
        .check(engine.inner(), &cid(), account, None)
        .await
        .unwrap();
    assert_eq!(refused.state, AccountCheckState::Refused);
    let view = health.view_of(&cid(), account);
    assert_eq!(view.state, AccountHealthState::Failing);
    assert_eq!(view.check, Some(AccountCheckState::Refused));
    assert_eq!(view.status, Some(401));
    let reason = view.reason.unwrap_or_default();
    assert!(reason.contains("invalid_auth"), "{reason}");
    assert!(!reason.contains(TOKEN));
    let frame = until("the failed frame", || {
        while let Ok(e) = bus.try_recv() {
            if let EnginePayload::ConnectorChecked { ok, .. } = e.payload.clone() {
                return Some(ok);
            }
        }
        None
    })
    .await;
    assert!(!frame);
    assert_eq!(stub.calls().len(), 2);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_ask_joins_the_check_already_running_and_the_platform_is_asked_once() {
    let dir = tempfile::tempdir().unwrap();
    let stub = Stub::start(vec![slow(
        "GET",
        "/whoami",
        200,
        json!({"user": "me"}),
        800,
    )])
    .await;
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let account = install_chat(&engine, &stub, Some(TOKEN)).unwrap();
    let health = &engine.inner().connector_health;
    let id = cid();
    let (first, second) = tokio::join!(health.check(engine.inner(), &id, account, None), async {
        // Behind the first by a moment, so it finds the slot running.
        tokio::time::sleep(Duration::from_millis(100)).await;
        health.check(engine.inner(), &id, account, None).await
    });
    assert_eq!(first.unwrap().state, AccountCheckState::Connected);
    assert_eq!(second.unwrap().state, AccountCheckState::Connected);
    assert_eq!(stub.calls().len(), 1, "one request for two asks");
    assert_eq!(
        health.view_of(&cid(), account).state,
        AccountHealthState::Ok
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_budget_ends_a_check_the_platform_never_answers_and_the_next_one_dials_again() {
    let dir = tempfile::tempdir().unwrap();
    let stub = Stub::start(vec![
        slow("GET", "/whoami", 200, json!({"user": "me"}), 5_000),
        canned("GET", "/whoami", 200, json!({"user": "me"})),
    ])
    .await;
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let account = install_chat(&engine, &stub, Some(TOKEN)).unwrap();
    let health = &engine.inner().connector_health;
    let started = std::time::Instant::now();
    let check = health
        .check(engine.inner(), &cid(), account, Some(1))
        .await
        .unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "the budget, not the platform, ended the wait"
    );
    assert_eq!(check.state, AccountCheckState::Unreachable);
    assert_eq!(check.status, None);
    let view = health.view_of(&cid(), account);
    assert_eq!(view.state, AccountHealthState::Failing);
    assert_eq!(view.check, Some(AccountCheckState::Unreachable));
    assert!(view.reason.is_some());
    // The permit the first check held is back in the pool: the next check
    // dials, and the platform answers.
    let again = health
        .check(engine.inner(), &cid(), account, None)
        .await
        .unwrap();
    assert_eq!(again.state, AccountCheckState::Connected);
    assert_eq!(
        health.view_of(&cid(), account).state,
        AccountHealthState::Ok
    );
    // The budget is clamped, never a refusal.
    assert_eq!(connectors::check_budget(None), Duration::from_secs(20));
    assert_eq!(connectors::check_budget(Some(0)), Duration::from_secs(1));
    assert_eq!(connectors::check_budget(Some(600)), Duration::from_secs(60));
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_doors_that_change_a_way_in_forget_its_health() {
    let dir = tempfile::tempdir().unwrap();
    let stub = Stub::start(vec![
        canned("GET", "/whoami", 200, json!({"user": "me"})),
        canned("GET", "/whoami", 200, json!({"user": "me"})),
        canned("GET", "/whoami", 200, json!({"user": "me"})),
    ])
    .await;
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let account = install_chat(&engine, &stub, Some(TOKEN)).unwrap();
    let inner = engine.inner();
    let health = &inner.connector_health;
    let ok = |h: &bisa_engine::connector_health::ConnectorHealth| {
        h.view_of(&cid(), account).state == AccountHealthState::Ok
    };
    health.check(inner, &cid(), account, None).await.unwrap();
    assert!(ok(health));

    // The secrets set again: what was checked is not what is held.
    connectors::set_secrets(
        inner,
        &cid(),
        account,
        BTreeMap::from([(SecretField::Token, "xoxb-another".to_string())]),
    )
    .unwrap();
    assert_eq!(
        health.view_of(&cid(), account),
        AccountHealthView::unknown(),
        "new secrets, no answer standing"
    );
    health.check(inner, &cid(), account, None).await.unwrap();
    assert!(ok(health));

    // A parameter edit feeds the URL and the bodies: another way in.
    connectors::update_account(inner, &cid(), account, "work (eu)".into(), BTreeMap::new())
        .unwrap();
    assert_eq!(
        health.view_of(&cid(), account).state,
        AccountHealthState::Unknown
    );
    health.check(inner, &cid(), account, None).await.unwrap();
    assert!(ok(health));

    // The definition moved: every account of it starts over. The default
    // mark moving is not a change to the way in.
    connectors::set_default_account(inner, &cid(), account).unwrap();
    assert!(ok(health), "the default mark is not a way in");
    let mut def = inner.ws.get_connector(&cid()).unwrap();
    def.description = "moved".into();
    connectors::update_connector(inner, def).unwrap();
    assert_eq!(
        health.view_of(&cid(), account).state,
        AccountHealthState::Unknown
    );

    // Forgotten: nothing stands, and nothing is dialed for an account that is gone.
    connectors::delete_account(inner, &cid(), account).unwrap();
    assert_eq!(
        health.view_of(&cid(), account).state,
        AccountHealthState::Unknown
    );
    assert!(
        health.check(inner, &cid(), account, None).await.is_err(),
        "an unknown account is the store's refusal"
    );
    assert_eq!(stub.calls().len(), 3);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_connector_with_no_check_is_answered_in_words_and_nothing_is_kept_or_told() {
    let dir = tempfile::tempdir().unwrap();
    let stub = Stub::start(vec![]).await;
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let ws = engine.workspace();
    let mut def = chat(&stub, bisa_core::AuthScheme::Bearer);
    def.check = None;
    ws.create_connector(def).unwrap();
    let account = ws
        .create_connector_account(bisa_store::NewConnectorAccount {
            connector: cid(),
            label: "work".into(),
            params: BTreeMap::new(),
            default: true,
        })
        .unwrap();
    ws.set_connector_secrets(
        &cid(),
        account.id,
        &BTreeMap::from([(SecretField::Token, TOKEN.to_string())]),
    )
    .unwrap();
    let mut bus = engine.events();
    let health = &engine.inner().connector_health;
    let check = health
        .check(engine.inner(), &cid(), account.id, None)
        .await
        .unwrap();
    assert_eq!(check.state, AccountCheckState::NoCheck);
    assert_eq!(
        health.view_of(&cid(), account.id),
        AccountHealthView::unknown(),
        "a refusal in words is not a health"
    );
    tokio::time::sleep(Duration::from_millis(150)).await;
    while let Ok(e) = bus.try_recv() {
        assert!(
            !matches!(e.payload, EnginePayload::ConnectorChecked { .. }),
            "nothing was checked, so nothing is told"
        );
    }
    assert!(stub.calls().is_empty());
    // An unknown connector is the store's refusal, and leaves no slot.
    assert!(health
        .check(
            engine.inner(),
            &ConnectorId::new("nobody").unwrap(),
            account.id,
            None
        )
        .await
        .is_err());
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_check_dropped_mid_flight_leaves_no_slot_behind() {
    let dir = tempfile::tempdir().unwrap();
    let stub = Stub::start(vec![
        slow("GET", "/whoami", 200, json!({"user": "me"}), 2_000),
        canned("GET", "/whoami", 200, json!({"user": "me"})),
    ])
    .await;
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let account = install_chat(&engine, &stub, Some(TOKEN)).unwrap();
    let inner = Arc::clone(engine.inner());
    // The runner's caller goes away mid-check — a closed window.
    let runner = tokio::spawn({
        let inner = Arc::clone(&inner);
        async move {
            inner
                .connector_health
                .check(&inner, &cid(), account, None)
                .await
        }
    });
    tokio::time::sleep(Duration::from_millis(150)).await;
    // A joiner behind it is told the runner went, not left waiting.
    let joiner = tokio::spawn({
        let inner = Arc::clone(&inner);
        async move {
            inner
                .connector_health
                .check(&inner, &cid(), account, None)
                .await
        }
    });
    tokio::time::sleep(Duration::from_millis(150)).await;
    runner.abort();
    let joined = joiner.await.unwrap().unwrap();
    assert_eq!(joined.state, AccountCheckState::Unreachable);
    assert_eq!(joined.reason.as_deref(), Some(DROPPED));
    assert_eq!(
        inner.connector_health.view_of(&cid(), account),
        AccountHealthView::unknown(),
        "a dropped check is no health"
    );
    // The next ask is the runner, not a joiner of nobody: the platform is dialed again.
    let again = inner
        .connector_health
        .check(&inner, &cid(), account, None)
        .await
        .unwrap();
    assert_eq!(again.state, AccountCheckState::Connected);
    assert_eq!(stub.calls().len(), 2, "the dropped request and the new one");
    engine.shutdown().await;
}
