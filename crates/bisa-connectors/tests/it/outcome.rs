//! What a call records: the selected part of the body, a text body, and
//! the cap on how much is read — through the client, against the stub.

use crate::support::*;
use bisa_connectors::hosts::AllowAll;
use bisa_connectors::spec::AuthSpec;
use bisa_connectors::ConnectorError;
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn select_walks_a_dotted_path_and_a_missing_path_is_named() {
    let stub = Stub::start(vec![
        Canned::json(
            "GET",
            "/v1/items/1",
            200,
            json!({"data": {"items": [{"id": 7}]}}),
        ),
        Canned::json("GET", "/v1/items/2", 200, json!({"data": {}})),
    ])
    .await;
    let client = client(MemoryCreds::with(&account(), Default::default()));
    let mut spec = spec(&stub, AuthSpec::None);
    spec.select = Some("data.items.0.id".into());
    let out = client
        .call(
            &spec,
            None,
            &no_account_params(),
            &params(&[("id", "1")]),
            &AllowAll,
        )
        .await
        .unwrap();
    assert_eq!(out.selected, json!(7));
    let err = client
        .call(
            &spec,
            None,
            &no_account_params(),
            &params(&[("id", "2")]),
            &AllowAll,
        )
        .await
        .unwrap_err();
    assert!(matches!(err, ConnectorError::SelectMissing { path } if path == "data.items.0.id"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_non_json_body_is_text() {
    let stub = Stub::start(vec![Canned::text("GET", "/v1/items/1", 200, "plain words")]).await;
    let client = client(MemoryCreds::with(&account(), Default::default()));
    let spec = spec(&stub, AuthSpec::None);
    let out = client
        .call(
            &spec,
            None,
            &no_account_params(),
            &params(&[("id", "1")]),
            &AllowAll,
        )
        .await
        .unwrap();
    assert_eq!(out.body, json!("plain words"));
    assert_eq!(out.selected, json!("plain words"));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_size_cap_holds() {
    // Twice the cap: the transport stops reading one byte past it, so the
    // refusal names the cap plus one, never what the service kept sending.
    let big = "x".repeat(2 * bisa_connectors::outcome::MAX_BODY_BYTES);
    let stub = Stub::start(vec![Canned::text("GET", "/v1/items/1", 200, &big)]).await;
    let client = client(MemoryCreds::with(&account(), Default::default()));
    let spec = spec(&stub, AuthSpec::None);
    let err = client
        .call(
            &spec,
            None,
            &no_account_params(),
            &params(&[("id", "1")]),
            &AllowAll,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, ConnectorError::TooLarge(n) if n == bisa_connectors::outcome::MAX_BODY_BYTES + 1),
        "{err}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_not_found_and_a_refusal_carry_the_services_words() {
    let stub = Stub::start(vec![
        Canned::json(
            "GET",
            "/v1/items/1",
            404,
            json!({"message": "no such item"}),
        ),
        Canned::json(
            "GET",
            "/v1/items/2",
            422,
            json!({"errors": [{"message": "title is required"}]}),
        ),
    ])
    .await;
    let client = client(MemoryCreds::with(&account(), Default::default()));
    let spec = spec(&stub, AuthSpec::None);
    let err = client
        .call(
            &spec,
            None,
            &no_account_params(),
            &params(&[("id", "1")]),
            &AllowAll,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, ConnectorError::NotFound(ref m) if m == "no such item"),
        "{err}"
    );
    let err = client
        .call(
            &spec,
            None,
            &no_account_params(),
            &params(&[("id", "2")]),
            &AllowAll,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, ConnectorError::Refused { status: 422, ref reason } if reason == "title is required"),
        "{err}"
    );
}
