//! The retry policy, under a paused clock: what is retried, how long the
//! wait is, and when the last attempt is the last.

use crate::support::*;
use bisa_connectors::hosts::AllowAll;
use bisa_connectors::http::TransportError;
use bisa_connectors::spec::AuthSpec;
use bisa_connectors::ConnectorError;
use serde_json::json;
use tokio::time::Instant;

#[tokio::test(start_paused = true)]
async fn a_429_with_retry_after_is_retried_after_that_many_seconds_then_answers() {
    let transport = ScriptedTransport::with(vec![
        with_header(ok(429, json!({"message": "slow down"})), "Retry-After", "3"),
        ok(200, json!({"ok": true})),
    ]);
    let client = scripted(
        transport.clone(),
        MemoryCreds::with(&account(), Default::default()),
        FixedClock::at(0),
    );
    let spec = offline_spec(AuthSpec::None);
    let before = Instant::now();
    let out = client
        .call(&spec, None, &no_account_params(), &params(&[]), &AllowAll)
        .await
        .unwrap();
    assert_eq!(out.body, json!({"ok": true}));
    assert_eq!(transport.sent().len(), 2);
    assert!(
        before.elapsed() >= std::time::Duration::from_secs(3),
        "waited {:?}",
        before.elapsed()
    );
    assert!(
        before.elapsed() < std::time::Duration::from_secs(4),
        "waited {:?}",
        before.elapsed()
    );
}

#[tokio::test(start_paused = true)]
async fn a_500_is_retried_for_a_read_and_not_for_a_write() {
    let transport =
        ScriptedTransport::with(vec![ok(500, json!({"error": "boom"})), ok(200, json!(1))]);
    let client = scripted(
        transport.clone(),
        MemoryCreds::with(&account(), Default::default()),
        FixedClock::at(0),
    );
    let spec = offline_spec(AuthSpec::None);
    client
        .call(&spec, None, &no_account_params(), &params(&[]), &AllowAll)
        .await
        .unwrap();
    assert_eq!(transport.sent().len(), 2, "a read tries again");

    let transport =
        ScriptedTransport::with(vec![ok(500, json!({"error": "boom"})), ok(200, json!(1))]);
    let client = scripted(
        transport.clone(),
        MemoryCreds::with(&account(), Default::default()),
        FixedClock::at(0),
    );
    let mut spec = offline_spec(AuthSpec::None);
    spec.writes = true;
    let err = client
        .call(&spec, None, &no_account_params(), &params(&[]), &AllowAll)
        .await
        .unwrap_err();
    assert!(
        matches!(err, ConnectorError::Upstream { status: 500, ref reason } if reason == "boom"),
        "{err}"
    );
    assert_eq!(
        transport.sent().len(),
        1,
        "a write is not repeated on a 500"
    );

    // A transport failure is a read's to retry, never a write's.
    let transport = ScriptedTransport::with(vec![
        Err(TransportError::Connect("refused".into())),
        ok(200, json!(1)),
    ]);
    let client = scripted(
        transport.clone(),
        MemoryCreds::with(&account(), Default::default()),
        FixedClock::at(0),
    );
    let mut spec = offline_spec(AuthSpec::None);
    spec.writes = true;
    let err = client
        .call(&spec, None, &no_account_params(), &params(&[]), &AllowAll)
        .await
        .unwrap_err();
    assert!(matches!(err, ConnectorError::Unreachable(_)), "{err}");
    assert_eq!(transport.sent().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn retry_after_is_capped() {
    let transport = ScriptedTransport::with(vec![
        with_header(ok(429, json!({})), "Retry-After", "3600"),
        ok(200, json!(1)),
    ]);
    let client = scripted(
        transport.clone(),
        MemoryCreds::with(&account(), Default::default()),
        FixedClock::at(0),
    );
    let spec = offline_spec(AuthSpec::None);
    let before = Instant::now();
    client
        .call(&spec, None, &no_account_params(), &params(&[]), &AllowAll)
        .await
        .unwrap();
    assert!(before.elapsed() >= std::time::Duration::from_secs(30));
    assert!(
        before.elapsed() < std::time::Duration::from_secs(31),
        "capped at the policy's thirty seconds"
    );
}

#[tokio::test(start_paused = true)]
async fn three_attempts_then_upstream() {
    let transport = ScriptedTransport::with(vec![
        ok(503, json!({"error": "down"})),
        ok(503, json!({"error": "down"})),
        ok(503, json!({"error": "still down"})),
        ok(200, json!(1)),
    ]);
    let client = scripted(
        transport.clone(),
        MemoryCreds::with(&account(), Default::default()),
        FixedClock::at(0),
    );
    let spec = offline_spec(AuthSpec::None);
    let err = client
        .call(&spec, None, &no_account_params(), &params(&[]), &AllowAll)
        .await
        .unwrap_err();
    assert!(
        matches!(err, ConnectorError::Upstream { status: 503, ref reason } if reason == "still down"),
        "{err}"
    );
    assert_eq!(transport.sent().len(), 3, "three attempts, no more");
}

#[tokio::test(start_paused = true)]
async fn a_wait_longer_than_the_deadline_ends_the_call_with_what_it_has() {
    let transport = ScriptedTransport::with(vec![
        with_header(ok(429, json!({})), "Retry-After", "20"),
        ok(200, json!(1)),
    ]);
    let client = scripted(
        transport.clone(),
        MemoryCreds::with(&account(), Default::default()),
        FixedClock::at(0),
    );
    let mut spec = offline_spec(AuthSpec::None);
    spec.timeout = std::time::Duration::from_secs(10);
    let err = client
        .call(&spec, None, &no_account_params(), &params(&[]), &AllowAll)
        .await
        .unwrap_err();
    assert!(
        matches!(err, ConnectorError::RateLimited { retry_after: 20 }),
        "{err}"
    );
    assert_eq!(transport.sent().len(), 1);
}
