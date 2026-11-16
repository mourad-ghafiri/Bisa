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
    assert_eq!(
        TransportError::Unsendable("a header's value".into()).to_string(),
        "the request could not be built: a header's value"
    );
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

// --- the hardening pass (2026-11) ---

/// A platform that hands back the cursor it was given — its last page, on
/// some — or a page with nothing on it, has nothing more: the read stops
/// there instead of asking for the same page up to the cap and handing the
/// step every item several times over.
#[tokio::test(start_paused = true)]
async fn a_paged_read_stops_on_a_cursor_it_already_followed_and_on_an_empty_page() {
    let transport = ScriptedTransport::with(vec![
        ok(200, json!({"items": [1], "next": "c2"})),
        ok(200, json!({"items": [2], "next": "c2"})),
        ok(200, json!({"items": ["never"], "next": "c2"})),
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
        json!([1, 2]),
        "the repeated cursor is the end"
    );
    assert_eq!(transport.sent().len(), 2);

    let transport = ScriptedTransport::with(vec![
        ok(200, json!({"items": [1], "next": "c2"})),
        ok(200, json!({"items": [], "next": "c3"})),
        ok(200, json!({"items": ["never"], "next": "c4"})),
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
    assert_eq!(out.selected, json!([1]), "an empty page is the end");
    assert_eq!(transport.sent().len(), 2);
}

/// A list the platform leaves out of an empty answer is an empty list, where
/// the operation says so (`or_empty`): a poll on a quiet inbox finds nothing
/// new instead of failing on every tick. Without the word a missing path is
/// still the definition's fault, said by name.
#[tokio::test(start_paused = true)]
async fn a_list_left_out_of_an_empty_answer_is_empty_where_the_operation_says_so() {
    let answer = || ScriptedTransport::with(vec![ok(200, json!({"resultSizeEstimate": 0}))]);
    let mut spec = offline_spec(AuthSpec::None);
    spec.select = Some("messages".into());
    let client = scripted(
        answer(),
        MemoryCreds::with(&account(), Default::default()),
        FixedClock::at(0),
    );
    let strict = client
        .call(&spec, None, &no_account_params(), &params(&[]), &AllowAll)
        .await;
    assert!(
        matches!(&strict, Err(bisa_connectors::ConnectorError::SelectMissing { path }) if path == "messages"),
        "{strict:?}"
    );
    spec.or_empty = true;
    let client = scripted(
        answer(),
        MemoryCreds::with(&account(), Default::default()),
        FixedClock::at(0),
    );
    let out = client
        .call(&spec, None, &no_account_params(), &params(&[]), &AllowAll)
        .await
        .unwrap();
    assert_eq!(out.selected, json!([]));
    assert_eq!(out.body, json!({"resultSizeEstimate": 0}));
}

/// A token pasted with a line break at its end cannot be a header: the
/// request is never built, and that is said as what it is — never as *could
/// not connect*, never tried again, and never counted against the host. Five
/// of them used to open the host's circuit for every account that calls it.
#[tokio::test(flavor = "multi_thread")]
async fn a_credential_a_header_cannot_carry_is_said_and_never_counted_against_the_host() {
    use bisa_connectors::creds::{Field, Stored};
    let stub = Stub::start(
        (0..2)
            .map(|_| Canned::json("GET", "/v1/items/1", 200, json!({"ok": true})))
            .collect(),
    )
    .await;
    let bad = bisa_connectors::creds::AccountRef::new("stubby", "pasted");
    let creds = MemoryCreds::with(&bad, Stored::default().with(Field::Token, "tok\nen"));
    creds.stored.lock().unwrap().insert(
        account().key(),
        Stored::default().with(Field::Token, "a-good-token"),
    );
    let client = client(creds.clone());
    let spec = spec(&stub, AuthSpec::Bearer);
    for round in 0..bisa_connectors::breaker::OPEN_AFTER + 1 {
        let err = client
            .call(
                &spec,
                Some(&bad),
                &no_account_params(),
                &params(&[("id", "1")]),
                &AllowAll,
            )
            .await
            .unwrap_err();
        assert!(
            matches!(&err, bisa_connectors::ConnectorError::BadDefinition(why) if why.contains("could not be built")),
            "round {round}: {err:?}"
        );
        assert!(
            !format!("{err}").contains("tok"),
            "the secret is not said: {err}"
        );
    }
    assert!(stub.calls().is_empty(), "nothing was sent");
    // The host's circuit is closed: the account beside it is answered.
    let out = client
        .call(
            &spec,
            Some(&account()),
            &no_account_params(),
            &params(&[("id", "1")]),
            &AllowAll,
        )
        .await
        .unwrap();
    assert_eq!(out.body, json!({"ok": true}));
}

/// Refuses its first calls, never answers the next, then answers.
struct RefusesThenHangs {
    sent: std::sync::atomic::AtomicU32,
}

#[async_trait::async_trait]
impl bisa_connectors::http::HttpTransport for RefusesThenHangs {
    async fn send(
        &self,
        _req: &bisa_connectors::http::Request,
        _insecure_loopback: bool,
    ) -> Result<bisa_connectors::http::Response, TransportError> {
        let n = self.sent.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if n < OPEN_AFTER {
            return Err(TransportError::Connect("connection refused".into()));
        }
        if n == OPEN_AFTER {
            std::future::pending::<()>().await;
        }
        ok(200, json!({"ok": true}))
    }
}

/// The probe let through after a pause may be dropped before it answers — a
/// stopped run, a deadline above it. It gives the probe back: the next call
/// goes out instead of waiting on a probe nobody is making.
#[tokio::test(start_paused = true)]
async fn a_probe_dropped_before_it_answers_lets_the_next_call_probe() {
    let transport = std::sync::Arc::new(RefusesThenHangs {
        sent: std::sync::atomic::AtomicU32::new(0),
    });
    let clock = FixedClock::at(100);
    let client = bisa_connectors::Client::new(
        transport.clone(),
        MemoryCreds::with(&account(), Default::default()),
        clock.clone(),
        CountingEntropy::from(1),
    );
    let mut spec = offline_spec(AuthSpec::None);
    spec.writes = true;
    for _ in 0..OPEN_AFTER {
        client
            .call(&spec, None, &no_account_params(), &params(&[]), &AllowAll)
            .await
            .unwrap_err();
    }
    *clock.0.lock().unwrap() = 100 + COOLDOWN_SECS;
    let dropped = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        client.call(&spec, None, &no_account_params(), &params(&[]), &AllowAll),
    )
    .await;
    assert!(dropped.is_err(), "the probe was let go mid-flight");
    let out = client
        .call(&spec, None, &no_account_params(), &params(&[]), &AllowAll)
        .await
        .unwrap();
    assert_eq!(out.body, json!({"ok": true}));
}

/// A call to this machine never goes to a proxy, however the address is
/// spelled: the person's proxy leaves `localhost` and `127.0.0.1` alone by
/// name, and `0.0.0.0` — the same machine — was handed to it, credential
/// and all. The proxy here answers on no port: only a direct call arrives.
#[tokio::test(flavor = "multi_thread")]
async fn a_call_to_this_machine_never_goes_through_a_proxy() {
    use bisa_connectors::http::{HttpTransport, Request, ReqwestTransport};
    use bisa_connectors::spec::Method;
    let stub = Stub::start(vec![Canned::json(
        "GET",
        "/v1/items/1",
        200,
        json!({"ok": 1}),
    )])
    .await;
    let nobody = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dead = nobody.local_addr().unwrap().port();
    drop(nobody);
    let policy = bisa_http::HttpPolicy {
        proxy: bisa_http::ProxyPolicy::Manual {
            http: Some(format!("http://127.0.0.1:{dead}").parse().unwrap()),
            https: None,
            no_proxy: vec![],
        },
        http1_only: false,
    };
    let transport = ReqwestTransport::with_http(std::sync::Arc::new(
        bisa_http::Clients::new(&policy).unwrap(),
    ));
    let port = stub.host().rsplit(':').next().unwrap().to_string();
    let answer = transport
        .send(
            &Request {
                method: Method::Get,
                url: format!("http://0.0.0.0:{port}/v1/items/1").parse().unwrap(),
                headers: vec![("Authorization".into(), "Bearer t".into())],
                body: None,
                timeout: std::time::Duration::from_secs(5),
                hidden_headers: vec![],
                secret_query: vec![],
            },
            false,
        )
        .await
        .expect("answered directly");
    assert_eq!(answer.status, 200);
    assert_eq!(stub.calls().len(), 1);
}

/// A platform out in the world is asked through the shared client — the
/// person's proxy, their HTTP version — and one nobody can find is said to
/// be unreachable, in words.
#[tokio::test(flavor = "multi_thread")]
async fn a_platform_nobody_can_find_is_unreachable() {
    use bisa_connectors::http::{HttpTransport, Request, ReqwestTransport};
    let answer = ReqwestTransport::new()
        .send(
            &Request {
                method: bisa_connectors::spec::Method::Get,
                url: "http://nobody.invalid/".parse().unwrap(),
                headers: vec![],
                body: None,
                timeout: std::time::Duration::from_secs(5),
                hidden_headers: vec![],
                secret_query: vec![],
            },
            false,
        )
        .await;
    assert!(answer.is_err(), "{:?}", answer.map(|r| r.status));
}
