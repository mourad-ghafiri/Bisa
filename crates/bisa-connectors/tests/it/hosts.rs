//! A host the definition does not name is refused before a request exists.

use crate::support::*;
use bisa_connectors::hosts::{AllowAll, HostJudge};
use bisa_connectors::spec::AuthSpec;
use bisa_connectors::ConnectorError;
use serde_json::json;
use std::sync::Arc;

#[tokio::test(flavor = "multi_thread")]
async fn a_host_the_definition_does_not_name_is_refused_before_any_request() {
    let stub = Stub::start(vec![Canned::json("GET", "/v1/items/1", 200, json!({}))]).await;
    let client = client(MemoryCreds::with(&account(), Default::default()));
    let mut spec = spec(&stub, AuthSpec::None);
    spec.hosts = vec!["api.example.com".into()];
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
        matches!(err, ConnectorError::HostRefused { ref host, .. } if host == stub.host()),
        "{err}"
    );
    assert!(err.to_string().contains("api.example.com"));
    assert!(stub.calls().is_empty());
}

#[test]
fn http_is_loopback_only() {
    use bisa_connectors::hosts::check;
    let declared = vec!["api.example.com".to_string(), "localhost:8080".to_string()];
    assert!(check(
        &url::Url::parse("http://localhost:8080/x").unwrap(),
        &declared,
        false
    )
    .is_ok());
    let err = check(
        &url::Url::parse("http://api.example.com/x").unwrap(),
        &declared,
        false,
    )
    .unwrap_err();
    assert!(matches!(err, ConnectorError::HostRefused { .. }));
}

#[test]
fn insecure_tls_is_loopback_only() {
    use bisa_connectors::hosts::check;
    let declared = vec!["api.example.com".to_string(), "127.0.0.1:27124".to_string()];
    assert!(check(
        &url::Url::parse("https://127.0.0.1:27124/x").unwrap(),
        &declared,
        true
    )
    .is_ok());
    let err = check(
        &url::Url::parse("https://api.example.com/x").unwrap(),
        &declared,
        true,
    )
    .unwrap_err();
    assert!(matches!(err, ConnectorError::BadDefinition(_)));
}

struct DenyStub(String);

impl HostJudge for DenyStub {
    fn judge(&self, host: &str) -> Result<(), String> {
        if host == self.0 {
            Err(format!("security.net.deny_hosts forbids {host}"))
        } else {
            Ok(())
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_judge_can_deny_a_declared_host() {
    let stub = Stub::start(vec![Canned::json("GET", "/v1/items/1", 200, json!({}))]).await;
    let client = client(MemoryCreds::with(&account(), Default::default()));
    let spec = spec(&stub, AuthSpec::None);
    let judge = Arc::new(DenyStub(stub.host().to_string()));
    let err = client
        .call(
            &spec,
            None,
            &no_account_params(),
            &params(&[("id", "1")]),
            judge.as_ref(),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, ConnectorError::HostRefused { ref allowed, .. } if allowed.contains("deny_hosts")),
        "{err}"
    );
    assert!(stub.calls().is_empty());
}
