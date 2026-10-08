//! How a call holds up: a write's key on every attempt, a read followed
//! across its pages under one deadline, and a host's circuit that stops
//! asking after five failures — all against a scripted transport.

use crate::support::*;
use bisa_connectors::hosts::AllowAll;
use bisa_connectors::http::TransportError;
use bisa_connectors::spec::{AuthSpec, Idempotency, Paging, ParamKind, ParamSpec};
use bisa_connectors::{ConnectorError, COOLDOWN_SECS, OPEN_AFTER};
use serde_json::json;
use std::collections::BTreeMap;

fn header(req: &bisa_connectors::http::Request, name: &str) -> Option<String> {
    req.headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.clone())
}

#[tokio::test(start_paused = true)]
async fn a_keyed_write_carries_the_same_key_on_every_attempt_and_an_unkeyed_one_none() {
    let transport = ScriptedTransport::with(vec![
        ok(503, json!({"message": "busy"})),
        ok(200, json!({"id": "m1"})),
    ]);
    let client = scripted(
        transport.clone(),
        MemoryCreds::with(&account(), Default::default()),
        FixedClock::at(0),
    );
    let mut spec = offline_spec(AuthSpec::None);
    spec.writes = true;
    spec.idempotency = Some(Idempotency {
        header: "Idempotency-Key".into(),
    });
    spec.idempotency_key = Some("bisa-run-step".into());
    client
        .call(&spec, None, &no_account_params(), &params(&[]), &AllowAll)
        .await
        .unwrap();
    let sent = transport.sent();
    assert_eq!(sent.len(), 2, "a 503 is retried for a write");
    for req in &sent {
        assert_eq!(
            header(req, "Idempotency-Key").as_deref(),
            Some("bisa-run-step"),
            "{req:?}"
        );
    }

    // The header named, no key given — a poll, an agent's read: nothing is sent for it.
    let transport = ScriptedTransport::with(vec![ok(200, json!({"id": "m2"}))]);
    let client = scripted(
        transport.clone(),
        MemoryCreds::with(&account(), Default::default()),
        FixedClock::at(0),
    );
    spec.idempotency_key = None;
    client
        .call(&spec, None, &no_account_params(), &params(&[]), &AllowAll)
        .await
        .unwrap();
    assert!(header(&transport.sent()[0], "Idempotency-Key").is_none());
}

fn paged_spec() -> bisa_connectors::CallSpec {
    let mut spec = offline_spec(AuthSpec::None);
    spec.expect = None;
    spec.params = vec![ParamSpec {
        name: "cursor".into(),
        kind: ParamKind::Text,
        required: false,
    }];
    spec.query = BTreeMap::from([("cursor".to_string(), "{params.cursor}".to_string())]);
    spec.select = Some("items".into());
    spec.page = Some(Paging {
        cursor_param: "cursor".into(),
        next_cursor: "next".into(),
        max_pages: 3,
    });
    spec
}

#[tokio::test(start_paused = true)]
async fn a_paged_read_follows_the_cursor_and_joins_the_pages_up_to_the_cap() {
    let transport = ScriptedTransport::with(vec![
        ok(200, json!({"items": [1, 2], "next": "c2"})),
        ok(200, json!({"items": [3], "next": "c3"})),
        ok(200, json!({"items": [4], "next": "c4"})),
        ok(200, json!({"items": [5]})),
    ]);
    let client = scripted(
        transport.clone(),
        MemoryCreds::with(&account(), Default::default()),
        FixedClock::at(0),
    );
    let out = client
        .call(
            &paged_spec(),
            None,
            &no_account_params(),
            &params(&[]),
            &AllowAll,
        )
        .await
        .unwrap();
    assert_eq!(
        out.selected,
        json!([1, 2, 3, 4]),
        "three pages joined, the fourth never asked for"
    );
    assert_eq!(out.pages, 3);
    let sent = transport.sent();
    assert_eq!(sent.len(), 3);
    assert!(
        !sent[0].url.as_str().contains("cursor="),
        "the first page carries no cursor: {}",
        sent[0].url
    );
    assert!(
        sent[1].url.as_str().contains("cursor=c2"),
        "{}",
        sent[1].url
    );
    assert!(
        sent[2].url.as_str().contains("cursor=c3"),
        "{}",
        sent[2].url
    );
}

#[tokio::test(start_paused = true)]
async fn a_paged_read_stops_where_the_answer_names_no_next_cursor() {
    let transport = ScriptedTransport::with(vec![
        ok(200, json!({"items": ["a"], "next": "c2"})),
        ok(200, json!({"items": ["b"], "next": ""})),
        ok(200, json!({"items": ["never"]})),
    ]);
    let client = scripted(
        transport.clone(),
        MemoryCreds::with(&account(), Default::default()),
        FixedClock::at(0),
    );
    let out = client
        .call(
            &paged_spec(),
            None,
            &no_account_params(),
            &params(&[]),
            &AllowAll,
        )
        .await
        .unwrap();
    assert_eq!(out.selected, json!(["a", "b"]));
    assert_eq!(out.pages, 2);
    assert_eq!(
        transport.sent().len(),
        2,
        "an empty cursor is the last page"
    );
}

#[tokio::test(start_paused = true)]
async fn a_page_that_selects_no_list_is_a_bad_definition() {
    let transport = ScriptedTransport::with(vec![ok(200, json!({"items": {"not": "a list"}}))]);
    let client = scripted(
        transport,
        MemoryCreds::with(&account(), Default::default()),
        FixedClock::at(0),
    );
    let err = client
        .call(
            &paged_spec(),
            None,
            &no_account_params(),
            &params(&[]),
            &AllowAll,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, ConnectorError::BadDefinition(ref why) if why.contains("not a list")),
        "{err}"
    );
}

#[tokio::test(start_paused = true)]
async fn five_failures_open_the_hosts_circuit_and_a_probe_closes_it() {
    let mut answers: Vec<Result<_, TransportError>> = Vec::new();
    for _ in 0..OPEN_AFTER {
        answers.push(Err(TransportError::Connect("connection refused".into())));
    }
    answers.push(ok(200, json!({"ok": true})));
    let transport = ScriptedTransport::with(answers);
    let clock = FixedClock::at(100);
    let client = scripted(
        transport.clone(),
        MemoryCreds::with(&account(), Default::default()),
        clock.clone(),
    );
    // A write: no retry inside a call, so one call is one attempt.
    let mut spec = offline_spec(AuthSpec::None);
    spec.writes = true;
    for _ in 0..OPEN_AFTER {
        let err = client
            .call(&spec, None, &no_account_params(), &params(&[]), &AllowAll)
            .await
            .unwrap_err();
        assert!(matches!(err, ConnectorError::Unreachable(_)), "{err}");
    }
    let err = client
        .call(&spec, None, &no_account_params(), &params(&[]), &AllowAll)
        .await
        .unwrap_err();
    assert!(
        matches!(err, ConnectorError::Open { until_secs, .. } if until_secs == COOLDOWN_SECS),
        "{err}"
    );
    assert!(
        !err.is_refusal(),
        "a paused host is the platform's, not the caller's"
    );
    assert_eq!(
        transport.sent().len() as u32,
        OPEN_AFTER,
        "the sixth call never went out"
    );
    // The pause ends: one probe goes and its success closes the circuit.
    *clock.0.lock().unwrap() = 100 + COOLDOWN_SECS;
    let out = client
        .call(&spec, None, &no_account_params(), &params(&[]), &AllowAll)
        .await
        .unwrap();
    assert_eq!(out.body, json!({"ok": true}));
    assert!(!client
        .breaker()
        .is_open(&spec.hosts[0], 100 + COOLDOWN_SECS));
}
