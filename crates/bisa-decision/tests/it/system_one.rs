use crate::support::*;
use bisa_connectors::AllowAll;
use bisa_core::DecisionProviderKind;
use bisa_decision::{DecisionProvider, ProviderError, SystemOneProvider, JEV_ENDPOINT};
use std::sync::Arc;
use std::time::Duration;

const DEADLINE: Duration = Duration::from_secs(20);

fn jev(transport: Arc<ScriptedTransport>) -> SystemOneProvider {
    SystemOneProvider::new(
        DecisionProviderKind::Jev,
        JEV_ENDPOINT,
        "jev-latest",
        bisa_connectors::Secret::some(FIXTURE_KEY),
        transport,
        &AllowAll,
        Arc::new(FixedClock(1_000)),
    )
    .unwrap()
}

#[tokio::test]
async fn the_request_is_the_system_one_wire_and_the_answer_is_the_contract() {
    let transport = ScriptedTransport::with(vec![http(200, routed_answer("small"))]);
    let provider = jev(transport.clone());
    let response = provider.decide(&routed(), DEADLINE).await.unwrap();
    assert_eq!(response.model, "jev-1.13.0");
    assert_eq!(response.answer("model").unwrap().chosen(), Some("small"));
    assert_eq!(response.usage.input_tokens, 40);

    let sent = transport.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].method.as_str(), "POST");
    assert_eq!(sent[0].url.as_str(), "https://api.typesafe.ai/v1/systemone");
    assert_eq!(
        sent[0].header("authorization"),
        Some(format!("Bearer {FIXTURE_KEY}").as_str())
    );
    assert_eq!(sent[0].timeout, DEADLINE);
    let body: serde_json::Value = serde_json::from_slice(sent[0].body.as_ref().unwrap()).unwrap();
    assert_eq!(body["model"], "jev-latest");
    assert_eq!(body["state"]["task"], "rename a variable");
    assert_eq!(body["questions"]["model"]["type"], "choice");
    assert_eq!(
        body["questions"]["model"]["criteria"]["small"],
        "quick edits"
    );
    // A request is never printed with its key.
    assert!(!format!("{:?}", sent[0]).contains(FIXTURE_KEY));
}

#[tokio::test]
async fn an_endpoint_with_or_without_a_trailing_slash_is_asked_at_the_same_path() {
    for endpoint in ["http://127.0.0.1:8080", "http://127.0.0.1:8080/"] {
        let transport = ScriptedTransport::with(vec![http(200, routed_answer("large"))]);
        let provider = SystemOneProvider::new(
            DecisionProviderKind::Rlcd,
            endpoint,
            "local-rlcd",
            None,
            transport.clone(),
            &AllowAll,
            Arc::new(FixedClock(0)),
        )
        .unwrap();
        provider.decide(&routed(), DEADLINE).await.unwrap();
        let sent = transport.sent();
        assert_eq!(sent[0].url.as_str(), "http://127.0.0.1:8080/v1/systemone");
        assert_eq!(sent[0].header("authorization"), None);
        assert_eq!(provider.descriptor().model, "local-rlcd");
        assert!(provider.descriptor().calibrated());
    }
}

#[test]
fn a_provider_that_cannot_be_asked_is_refused_before_any_request() {
    let build = |endpoint: &str, model: &str, judge: &dyn bisa_connectors::HostJudge| {
        SystemOneProvider::new(
            DecisionProviderKind::Rlcd,
            endpoint,
            model,
            None,
            ScriptedTransport::with(vec![]),
            judge,
            Arc::new(FixedClock(0)),
        )
        .err()
    };
    for (endpoint, model) in [
        ("", "m"),
        ("not a url", "m"),
        ("https://models.example", " "),
        // Plain http leaves this machine only over loopback.
        ("http://models.example", "m"),
        ("ftp://models.example", "m"),
    ] {
        assert!(
            matches!(
                build(endpoint, model, &AllowAll),
                Some(ProviderError::Misconfigured(_))
            ),
            "{endpoint:?} {model:?}"
        );
    }
    let refused = build("https://models.example", "m", &Refusing("models.example"));
    assert!(
        matches!(&refused, Some(ProviderError::Misconfigured(why)) if why.contains("deny list")),
        "{refused:?}"
    );
    assert!(build("https://models.example", "m", &AllowAll).is_none());
}

#[tokio::test]
async fn each_status_reads_as_what_a_caller_can_do_about_it() {
    let ask = |answer| async move {
        jev(ScriptedTransport::with(vec![answer]))
            .decide(&routed(), DEADLINE)
            .await
            .unwrap_err()
    };
    for status in [401, 403, 404, 422] {
        let error = ask(http(status, serde_json::json!({ "error": "no" }))).await;
        assert!(
            matches!(error, ProviderError::Refused { status: s, .. } if s == status),
            "{status}: {error:?}"
        );
        assert!(!error.is_transient(), "{status} is never asked again");
    }
    for status in [408, 429, 500, 503, 529] {
        let error = ask(http(status, serde_json::json!({}))).await;
        assert!(
            matches!(error, ProviderError::Busy { status: s, .. } if s == status),
            "{status}: {error:?}"
        );
        assert!(error.is_transient());
    }
    let waited = ask(http_with(
        429,
        &[("Retry-After", "3")],
        serde_json::json!({}),
    ))
    .await;
    assert_eq!(waited.retry_after(), Some(Duration::from_secs(3)));

    let unreachable = ask(Err(bisa_connectors::TransportError::Timeout)).await;
    assert!(matches!(unreachable, ProviderError::Unreachable(_)));
    assert!(unreachable.is_transient());
}

#[tokio::test]
async fn an_answer_that_is_not_the_contracts_shape_is_not_read() {
    for body in [
        serde_json::json!("SAFE"),
        serde_json::json!({ "model": "m", "answers": {} }),
        serde_json::json!({ "model": "m", "answers": {}, "note": "x",
                            "usage": { "input_tokens": 0, "output_tokens": 0 } }),
    ] {
        let error = jev(ScriptedTransport::with(vec![http(200, body.clone())]))
            .decide(&routed(), DEADLINE)
            .await
            .unwrap_err();
        assert!(matches!(error, ProviderError::Unreadable(_)), "{body}");
    }
}

#[tokio::test]
async fn a_refusal_never_repeats_the_key() {
    let echo = serde_json::json!({ "error": format!("bad key {FIXTURE_KEY}") });
    let error = jev(ScriptedTransport::with(vec![http(401, echo)]))
        .decide(&routed(), DEADLINE)
        .await
        .unwrap_err();
    let said = error.to_string();
    assert!(said.contains("bad key"), "{said}");
    assert!(!said.contains(FIXTURE_KEY), "{said}");
}
