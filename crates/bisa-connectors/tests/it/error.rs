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
