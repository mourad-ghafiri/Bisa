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
        default: None,
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

// added by the coverage pass: hardening.rs

/// The transport's own edges: a header is found whatever its case, every
/// transport error has its words, the default transport is the strict one,
/// a port nobody answers on is unreachable in words, and a loopback
/// platform that asks for the lenient client is still reached.
#[tokio::test(flavor = "multi_thread")]
async fn the_transports_edges() {
    use bisa_connectors::http::{ReqwestTransport, Response, TransportError};
    let answer = Response {
        status: 200,
        headers: vec![("Content-Type".into(), "text/plain".into())],
        body: vec![],
    };
    assert_eq!(answer.header("content-type"), Some("text/plain"));
    assert_eq!(answer.header("x-missing"), None);
    assert_eq!(
        TransportError::Connect("refused".into()).to_string(),
        "cannot connect: refused"
    );
    assert_eq!(
        TransportError::Body("cut".into()).to_string(),
        "cannot read the answer: cut"
    );
    assert_eq!(TransportError::Timeout.to_string(), "timed out");
    let _strict = ReqwestTransport::default();

    let nobody = bisa_connectors::spec::CallSpec {
        base_url: "http://127.0.0.1:1".into(),
        hosts: vec!["127.0.0.1:1".into()],
        ..offline_spec(AuthSpec::None)
    };
    let client = client(MemoryCreds::with(
        &account(),
        bisa_connectors::creds::Stored::default(),
    ));
    let err = client
        .call(&nobody, None, &no_account_params(), &params(&[]), &AllowAll)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, ConnectorError::Unreachable(why) if why.to_lowercase().contains("connection refused")),
        "{err}"
    );

    let stub = Stub::start(vec![Canned::json(
        "GET",
        "/v1/items/1",
        200,
        json!({"ok": 1}),
    )])
    .await;
    let lenient = bisa_connectors::spec::CallSpec {
        insecure_tls: true,
        ..spec(&stub, AuthSpec::None)
    };
    let out = client
        .call(
            &lenient,
            None,
            &no_account_params(),
            &params(&[("id", "1")]),
            &AllowAll,
        )
        .await
        .unwrap();
    assert_eq!(out.body, json!({"ok": 1}));
}

// added by the coverage pass: http.rs

/// A platform that closes the connection mid-body — the head promised a
/// hundred bytes, five came — fails the call as a transport failure, not a
/// refusal and not something to send again.
#[tokio::test(flavor = "multi_thread")]
async fn a_connection_cut_mid_body_is_a_transport_failure_asked_once() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let served = std::thread::spawn(move || {
        let mut count = 0;
        listener
            .set_nonblocking(false)
            .expect("a blocking listener");
        for stream in listener.incoming().take(1) {
            let mut stream = stream.expect("a connection");
            let mut request = [0u8; 1024];
            let _bytes_read = stream.read(&mut request);
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 100\r\n\r\n{\"ok\"",
                )
                .expect("the head and five bytes");
            drop(stream);
            count += 1;
        }
        count
    });
    let cut = bisa_connectors::spec::CallSpec {
        base_url: format!("http://{addr}"),
        hosts: vec![addr.to_string()],
        timeout: std::time::Duration::from_secs(5),
        ..offline_spec(AuthSpec::None)
    };
    let client = client(MemoryCreds::with(
        &account(),
        bisa_connectors::creds::Stored::default(),
    ));
    let err = client
        .call(&cut, None, &no_account_params(), &params(&[]), &AllowAll)
        .await
        .unwrap_err();
    assert!(matches!(err, ConnectorError::Transport(_)), "{err}");
    assert_eq!(
        served.join().unwrap(),
        1,
        "the one connection, never a second"
    );
}

// added by the connectors quality programme: one deadline, a lost answer

/// A platform that takes the request and closes the connection without a
/// word — the request went out, no answer came — is a transport failure,
/// not a connection never made: a write is never sent again on it, by this
/// crate or by a step's own retries, since the platform may have done the
/// thing.
#[tokio::test(flavor = "multi_thread")]
async fn a_request_lost_after_it_was_sent_is_a_transport_failure_never_sent_again() {
    use std::io::Read;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let served = std::thread::spawn(move || {
        let mut count = 0;
        for stream in listener.incoming().take(1) {
            let mut stream = stream.expect("a connection");
            let mut request = [0u8; 4096];
            let _bytes_read = stream.read(&mut request);
            drop(stream);
            count += 1;
        }
        count
    });
    let mut lost = bisa_connectors::spec::CallSpec {
        base_url: format!("http://{addr}"),
        hosts: vec![addr.to_string()],
        timeout: std::time::Duration::from_secs(5),
        ..offline_spec(AuthSpec::None)
    };
    lost.method = bisa_connectors::spec::Method::Post;
    lost.writes = true;
    let client = client(MemoryCreds::with(
        &account(),
        bisa_connectors::creds::Stored::default(),
    ));
    let err = client
        .call(&lost, None, &no_account_params(), &params(&[]), &AllowAll)
        .await
        .unwrap_err();
    assert!(
        matches!(err, ConnectorError::Transport(_)),
        "the request went out: {err}"
    );
    assert_eq!(
        served.join().unwrap(),
        1,
        "the one connection, never a second"
    );
}

/// A transport that takes two hundred milliseconds a page, within the
/// request's own deadline, and answers every page with a cursor to the next.
struct SlowPages {
    sent: std::sync::Mutex<Vec<bisa_connectors::http::Request>>,
}

#[async_trait::async_trait]
impl bisa_connectors::http::HttpTransport for SlowPages {
    async fn send(
        &self,
        req: &bisa_connectors::http::Request,
        _insecure_loopback: bool,
    ) -> Result<bisa_connectors::http::Response, TransportError> {
        let n = {
            let mut sent = self.sent.lock().unwrap();
            sent.push(req.clone());
            sent.len()
        };
        let page = tokio::time::sleep(std::time::Duration::from_millis(200));
        if tokio::time::timeout(req.timeout, page).await.is_err() {
            return Err(TransportError::Timeout);
        }
        ok(200, json!({"items": [n], "next": format!("c{}", n + 1)}))
    }
}

/// The operation's deadline is one for the whole read, its pages included:
/// a second page that would start past it is not asked for, and the call
/// times out naming the operation's own deadline.
#[tokio::test(flavor = "multi_thread")]
async fn a_paged_read_shares_one_deadline_across_its_pages() {
    let transport = std::sync::Arc::new(SlowPages {
        sent: std::sync::Mutex::new(Vec::new()),
    });
    let client = bisa_connectors::Client::new(
        transport.clone(),
        MemoryCreds::with(&account(), Default::default()),
        FixedClock::at(0),
        CountingEntropy::from(1),
    );
    let mut spec = paged_spec();
    spec.timeout = std::time::Duration::from_millis(300);
    let err = client
        .call(&spec, None, &no_account_params(), &params(&[]), &AllowAll)
        .await
        .unwrap_err();
    assert!(
        matches!(err, ConnectorError::Timeout(d) if d == std::time::Duration::from_millis(300)),
        "{err}"
    );
    assert_eq!(
        transport.sent.lock().unwrap().len(),
        2,
        "the second page met the deadline; a third was never asked for"
    );
}
