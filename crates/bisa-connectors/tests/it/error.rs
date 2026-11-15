//! No error text ever carries the secret the call exposed — whether the
//! service echoed it, the URL carried it, or the definition refused it.

use crate::support::*;
use bisa_connectors::creds::{Field, Stored};
use bisa_connectors::hosts::AllowAll;
use bisa_connectors::spec::{AuthSpec, KeyPlace};
use bisa_connectors::ConnectorError;
use serde_json::json;

const KEY: &str = "sk-live-VERY-SECRET-9";

#[tokio::test(flavor = "multi_thread")]
async fn error_text_never_contains_the_secret() {
    // 1. The service echoes the key in its error body.
    let stub = Stub::start(vec![Canned::json(
        "GET",
        "/v1/items/1",
        401,
        json!({"message": format!("bad key {KEY}")}),
    )])
    .await;
    let creds = MemoryCreds::with(&account(), Stored::default().with(Field::ApiKey, KEY));
    let client = client(creds.clone());
    let spec = spec(
        &stub,
        AuthSpec::ApiKey {
            place: KeyPlace::Query { name: "key".into() },
            prefix: None,
        },
    );
    let err = client
        .call(
            &spec,
            Some(&account()),
            &no_account_params(),
            &params(&[("id", "1")]),
            &AllowAll,
        )
        .await
        .unwrap_err();
    let text = format!("{err} / {err:?}");
    assert!(!text.contains(KEY), "{text}");
    assert!(text.contains("«redacted»"), "{text}");

    // 2. A transport error on a URL that carried the key in its query.
    let mut dead = spec.clone();
    dead.base_url = "http://127.0.0.1:1".into();
    dead.hosts = vec!["127.0.0.1:1".into()];
    dead.timeout = std::time::Duration::from_secs(2);
    let err = client
        .call(
            &dead,
            Some(&account()),
            &no_account_params(),
            &params(&[("id", "1")]),
            &AllowAll,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            ConnectorError::Unreachable(_) | ConnectorError::Timeout(_)
        ),
        "{err}"
    );
    assert!(!format!("{err} / {err:?}").contains(KEY));

    // 3. A Basic credential echoed in its base64 form.
    let stub = Stub::start(vec![Canned::json(
        "GET",
        "/v1/items/1",
        403,
        json!({"message": "refused for dXNlcjpwYXNz"}),
    )])
    .await;
    let creds = MemoryCreds::with(
        &account(),
        Stored::default()
            .with(Field::Username, "user")
            .with(Field::Password, "pass"),
    );
    let client = crate::support::client(creds);
    let spec = crate::support::spec(&stub, AuthSpec::Basic);
    let err = client
        .call(
            &spec,
            Some(&account()),
            &no_account_params(),
            &params(&[("id", "1")]),
            &AllowAll,
        )
        .await
        .unwrap_err();
    let text = err.to_string();
    assert!(!text.contains("dXNlcjpwYXNz"), "{text}");
    assert!(!text.contains("pass"), "{text}");
}

#[test]
fn a_request_and_a_secret_never_print_their_credential() {
    use bisa_connectors::http::Request;
    use bisa_connectors::spec::Method;
    let mut req = Request::new(
        Method::Get,
        url::Url::parse(&format!("https://api.example.com/x?key={KEY}&q=1")).unwrap(),
        std::time::Duration::from_secs(1),
    );
    req.secret_query = Some("key".into());
    req.headers
        .push(("authorization".into(), format!("Bearer {KEY}")));
    req.headers.push(("x-api-key".into(), KEY.into()));
    req.hidden_headers.push("x-api-key".into());
    let shown = format!("{req:?}");
    assert!(!shown.contains(KEY), "{shown}");
    assert!(shown.contains("q=1"));
    assert!(!format!("{:?}", secret(KEY)).contains(KEY));
}

#[test]
fn is_refusal_tells_the_callers_fault_from_the_networks() {
    let mine = [
        ConnectorError::BadDefinition("x".into()),
        ConnectorError::BadParam {
            name: "n".into(),
            why: "w".into(),
        },
        ConnectorError::Unresolved("params.x".into()),
        ConnectorError::HostRefused {
            host: "h".into(),
            allowed: "a".into(),
        },
        ConnectorError::NotAuthenticated("n".into()),
        ConnectorError::NotFound("n".into()),
        ConnectorError::Refused {
            status: 400,
            reason: "r".into(),
        },
        ConnectorError::SelectMissing { path: "p".into() },
        ConnectorError::TooLarge(2),
        ConnectorError::OAuth("o".into()),
    ];
    for e in mine {
        assert!(e.is_refusal(), "{e}");
    }
    let theirs = [
        ConnectorError::RateLimited { retry_after: 1 },
        ConnectorError::Upstream {
            status: 502,
            reason: "r".into(),
        },
        ConnectorError::Transport("t".into()),
        ConnectorError::Timeout(std::time::Duration::from_secs(1)),
        ConnectorError::Store("s".into()),
    ];
    for e in theirs {
        assert!(!e.is_refusal(), "{e}");
    }
}

/// What a call **answers** never carries the secret it was made with
/// either: a platform that echoes the key in a 200 — raw, percent-encoded
/// as a query carries it, or as the Basic header carried it — hands the
/// step a placeholder. This is the crate's own scrub of the exact secrets
/// it sent, whatever shape they have and whatever the caller redacts later.
#[tokio::test(flavor = "multi_thread")]
async fn an_answer_never_carries_the_secret_the_call_was_made_with() {
    const ODD: &str = "a+b/c=d e&f";
    let encoded = "a%2Bb%2Fc%3Dd%20e%26f";
    let formed = "a%2Bb%2Fc%3Dd+e%26f";
    let stub = Stub::start(vec![Canned::json(
        "GET",
        "/v1/items/1",
        200,
        json!({
            "item": {"echo": format!("you sent {ODD}"), "nested": [format!("url?key={encoded}"), format!("form key={formed}")]},
            "debug": {"key": ODD},
        }),
    )])
    .await;
    let creds = MemoryCreds::with(&account(), Stored::default().with(Field::ApiKey, ODD));
    let client = client(creds.clone());
    let mut spec = spec(
        &stub,
        AuthSpec::ApiKey {
            place: KeyPlace::Query { name: "key".into() },
            prefix: None,
        },
    );
    spec.select = Some("item".into());
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
    let text = format!("{} / {}", out.selected, out.body);
    for form in [ODD, encoded, formed] {
        assert!(!text.contains(form), "{form} in {text}");
    }
    assert_eq!(text.matches("«redacted»").count(), 7, "{text}");

    // And an error whose text carries an encoded form.
    let stub = Stub::start(vec![Canned::json(
        "GET",
        "/v1/items/1",
        400,
        json!({"message": format!("bad request for /v1/items/1?key={encoded}")}),
    )])
    .await;
    let mut spec = spec_for_error(&stub);
    spec.select = None;
    let err = client
        .call(
            &spec,
            Some(&account()),
            &no_account_params(),
            &params(&[("id", "1")]),
            &AllowAll,
        )
        .await
        .unwrap_err();
    let text = format!("{err} / {err:?}");
    assert!(
        !text.contains(encoded) && text.contains("«redacted»"),
        "{text}"
    );
}

fn spec_for_error(stub: &Stub) -> bisa_connectors::spec::CallSpec {
    spec(
        stub,
        AuthSpec::ApiKey {
            place: KeyPlace::Query { name: "key".into() },
            prefix: None,
        },
    )
}
