//! The authorization URL, the exchange, and the refresh that keeps a token
//! alive — against the stub's token endpoint, with credentials in a map.

use crate::support::*;
use bisa_connectors::creds::{Field, Stored};
use bisa_connectors::hosts::{AllowAll, HostJudge};
use bisa_connectors::oauth::{authorize_url, exchange};
use bisa_connectors::spec::{AuthSpec, ChallengeEncoding, ScopeJoin};
use bisa_connectors::ConnectorError;
use serde_json::json;
use sha2::Digest;
use std::collections::BTreeMap;
use std::sync::Arc;

fn oauth(stub_base: &str, extra: BTreeMap<String, String>) -> AuthSpec {
    AuthSpec::OAuth2 {
        authorization_url: "https://auth.example.com/authorize".into(),
        token_url: format!("{stub_base}/oauth/token"),
        scopes: vec!["read".into(), "write".into()],
        pkce: true,
        extra,
        client_id_param: bisa_connectors::DEFAULT_CLIENT_ID_PARAM.into(),
        scope_join: ScopeJoin::Space,
        code_challenge: ChallengeEncoding::Base64url,
    }
}

/// A scheme spelled the way TikTok reads it: `client_key`, scopes joined by
/// a comma, the challenge as hex.
fn tiktok_shaped(stub_base: &str) -> AuthSpec {
    AuthSpec::OAuth2 {
        authorization_url: "https://www.tiktok.example/v2/auth/authorize/".into(),
        token_url: format!("{stub_base}/v2/oauth/token/"),
        scopes: vec!["user.info.basic".into(), "video.list".into()],
        pkce: true,
        extra: BTreeMap::new(),
        client_id_param: "client_key".into(),
        scope_join: ScopeJoin::Comma,
        code_challenge: ChallengeEncoding::Hex,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dialect_names_the_client_id_joins_scopes_by_comma_and_writes_the_challenge_as_hex() {
    let stub = Stub::start(vec![Canned::json(
        "POST",
        "/v2/oauth/token/",
        200,
        json!({"access_token": "act.1", "refresh_token": "rft.1", "expires_in": 86400, "open_id": "o", "scope": "user.info.basic,video.list", "token_type": "Bearer"}),
    )])
    .await;
    let auth = tiktok_shaped(stub.base_url());
    let a = authorize_url(
        &auth,
        "key-1",
        "http://127.0.0.1:4478/connectors/oauth/callback",
        CountingEntropy::from(3).as_ref(),
        &AllowAll,
    )
    .unwrap();
    let url = url::Url::parse(&a.url).unwrap();
    let q: BTreeMap<String, String> = url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert_eq!(q["client_key"], "key-1");
    assert!(
        !q.contains_key("client_id"),
        "the id travels under one name"
    );
    assert_eq!(q["scope"], "user.info.basic,video.list");
    assert_eq!(q["code_challenge_method"], "S256");
    let expected = hex::encode(sha2::Sha256::digest(a.verifier.expose().as_bytes()));
    assert_eq!(
        q["code_challenge"], expected,
        "hex of the SHA-256, 64 characters"
    );
    assert_eq!(q["code_challenge"].len(), 64);

    let creds = MemoryCreds::with(
        &account(),
        Stored::default()
            .with(Field::ClientId, "key-1")
            .with(Field::ClientSecret, "shh"),
    );
    let client = client(creds.clone());
    let tokens = exchange(
        &client,
        &auth,
        &account(),
        "code-1",
        &a.verifier,
        "http://127.0.0.1:4478/connectors/oauth/callback",
        &AllowAll,
    )
    .await
    .unwrap();
    assert_eq!(tokens.access_token.expose(), "act.1");
    let form = stub.calls()[0].form();
    assert_eq!(
        form["client_key"], "key-1",
        "the token form spells it the same way"
    );
    assert!(!form.contains_key("client_id"));
    assert_eq!(form["client_secret"], "shh");
    assert_eq!(form["grant_type"], "authorization_code");
}

#[test]
fn the_authorize_url_carries_state_pkce_s256_scopes_and_extra() {
    let extra = BTreeMap::from([
        ("access_type".to_string(), "offline".to_string()),
        ("token_auth".to_string(), "basic".to_string()),
    ]);
    let auth = oauth("https://token.example.com", extra);
    let entropy = CountingEntropy::from(7);
    let a = authorize_url(
        &auth,
        "client-1",
        "http://127.0.0.1:4478/connectors/oauth/callback",
        entropy.as_ref(),
        &AllowAll,
    )
    .unwrap();
    let url = url::Url::parse(&a.url).unwrap();
    let q: BTreeMap<String, String> = url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert_eq!(url.host_str(), Some("auth.example.com"));
    assert_eq!(q["response_type"], "code");
    assert_eq!(q["client_id"], "client-1");
    assert_eq!(
        q["redirect_uri"],
        "http://127.0.0.1:4478/connectors/oauth/callback"
    );
    assert_eq!(q["scope"], "read write");
    assert_eq!(q["state"], a.state);
    assert_eq!(a.state.len(), 64, "32 random bytes as hex");
    assert_eq!(q["code_challenge_method"], "S256");
    assert_eq!(
        a.verifier.expose().len(),
        43,
        "32 random bytes, base64url without padding"
    );
    let expected = base64::Engine::encode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        sha2::Sha256::digest(a.verifier.expose().as_bytes()),
    );
    assert_eq!(
        q["code_challenge"], expected,
        "the challenge is the verifier's S256"
    );
    assert_eq!(q["access_type"], "offline");
    assert!(
        !q.contains_key("token_auth"),
        "token_auth is the token endpoint's, not the browser's"
    );
    // Deterministic entropy: the same seed gives the same state.
    let again = authorize_url(
        &auth,
        "client-1",
        "http://127.0.0.1:4478/x",
        CountingEntropy::from(7).as_ref(),
        &AllowAll,
    )
    .unwrap();
    assert_eq!(again.state, a.state);
    // Not OAuth2, and a non-https authorization URL, are bad definitions.
    assert!(matches!(
        authorize_url(
            &AuthSpec::Bearer,
            "c",
            "http://127.0.0.1/x",
            entropy.as_ref(),
            &AllowAll,
        ),
        Err(ConnectorError::BadDefinition(_))
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn exchange_posts_the_form_and_saves_the_tokens() {
    let stub = Stub::start(vec![Canned::json(
        "POST",
        "/oauth/token",
        200,
        json!({"access_token": "at-1", "refresh_token": "rt-1", "expires_in": 3600, "token_type": "Bearer", "scope": "read"}),
    )])
    .await;
    let creds = MemoryCreds::with(
        &account(),
        Stored::default()
            .with(Field::ClientId, "client-1")
            .with(Field::ClientSecret, "shh"),
    );
    let client = client(creds.clone());
    let auth = oauth(stub.base_url(), BTreeMap::new());
    let tokens = exchange(
        &client,
        &auth,
        &account(),
        "code-9",
        &secret("verifier-1"),
        "http://127.0.0.1:4478/cb",
        &AllowAll,
    )
    .await
    .unwrap();
    assert_eq!(tokens.access_token.expose(), "at-1");
    assert_eq!(
        tokens.refresh_token.as_ref().map(|s| s.expose()),
        Some("rt-1")
    );
    assert_eq!(tokens.expires_at, Some(1_000_000 + 3600));
    let call = &stub.calls()[0];
    assert_eq!(
        call.header("content-type").as_deref(),
        Some("application/x-www-form-urlencoded")
    );
    let form = call.form();
    assert_eq!(form["grant_type"], "authorization_code");
    assert_eq!(form["code"], "code-9");
    assert_eq!(form["code_verifier"], "verifier-1");
    assert_eq!(form["redirect_uri"], "http://127.0.0.1:4478/cb");
    assert_eq!(form["client_id"], "client-1");
    assert_eq!(form["client_secret"], "shh");
    assert!(call.authorization.is_none());
    let kept = creds.get(&account());
    assert_eq!(
        kept.field(Field::AccessToken).map(|s| s.expose()),
        Some("at-1")
    );
    assert_eq!(
        kept.field(Field::RefreshToken).map(|s| s.expose()),
        Some("rt-1")
    );
    assert_eq!(creds.saves(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn token_auth_basic_moves_the_client_credentials_into_the_header() {
    let stub = Stub::start(vec![Canned::json(
        "POST",
        "/oauth/token",
        200,
        json!({"access_token": "at-2", "token_type": "bearer"}),
    )])
    .await;
    let creds = MemoryCreds::with(
        &account(),
        Stored::default()
            .with(Field::ClientId, "cid")
            .with(Field::ClientSecret, "csec"),
    );
    let client = client(creds);
    let auth = oauth(
        stub.base_url(),
        BTreeMap::from([("token_auth".to_string(), "basic".to_string())]),
    );
    exchange(
        &client,
        &auth,
        &account(),
        "c",
        &secret("v"),
        "http://127.0.0.1/cb",
        &AllowAll,
    )
    .await
    .unwrap();
    let call = &stub.calls()[0];
    assert_eq!(call.authorization.as_deref(), Some("Basic Y2lkOmNzZWM="));
    assert!(!call.form().contains_key("client_secret"));
}

#[tokio::test(flavor = "multi_thread")]
async fn refresh_happens_only_within_sixty_seconds_of_expiry_and_once_under_contention() {
    let stub = Stub::start(vec![
        Canned::json("GET", "/v1/items/1", 200, json!({"ok": 1})),
        Canned::json("GET", "/v1/items/1", 200, json!({"ok": 2})),
        Canned::json("GET", "/v1/items/1", 200, json!({"ok": 3})),
        Canned::json(
            "POST",
            "/oauth/token",
            200,
            json!({"access_token": "at-new", "expires_in": 3600, "token_type": "Bearer"}),
        ),
    ])
    .await;
    let fresh = Stored::default()
        .with(Field::ClientId, "cid")
        .with(Field::AccessToken, "at-old")
        .with(Field::RefreshToken, "rt-old");
    // Plenty of time left: no refresh.
    let creds = MemoryCreds::with(
        &account(),
        Stored {
            expires_at: Some(1_000_000 + 3600),
            ..fresh.clone()
        },
    );
    let client = Arc::new(client(creds.clone()));
    let mut spec = spec(&stub, oauth(stub.base_url(), BTreeMap::new()));
    spec.hosts = vec![stub.host().to_string()];
    client
        .call(
            &spec,
            Some(&account()),
            &no_account_params(),
            &params(&[("id", "1")]),
            &AllowAll,
        )
        .await
        .unwrap();
    assert_eq!(creds.saves(), 0);
    assert_eq!(
        stub.calls().last().unwrap().authorization.as_deref(),
        Some("Bearer at-old")
    );

    // Thirty seconds left: two concurrent calls, one refresh.
    let creds = MemoryCreds::with(
        &account(),
        Stored {
            expires_at: Some(1_000_000 + 30),
            ..fresh
        },
    );
    let client = Arc::new(crate::support::client(creds.clone()));
    let acct = account();
    let none = no_account_params();
    let ps = params(&[("id", "1")]);
    let (a, b) = tokio::join!(
        client.call(&spec, Some(&acct), &none, &ps, &AllowAll),
        client.call(&spec, Some(&acct), &none, &ps, &AllowAll),
    );
    a.unwrap();
    b.unwrap();
    assert_eq!(creds.saves(), 1, "one refresh for two flights");
    let refreshes = stub
        .calls()
        .iter()
        .filter(|c| c.path == "/oauth/token")
        .count();
    assert_eq!(refreshes, 1);
    let form = stub
        .calls()
        .iter()
        .find(|c| c.path == "/oauth/token")
        .unwrap()
        .form();
    assert_eq!(form["grant_type"], "refresh_token");
    assert_eq!(form["refresh_token"], "rt-old");
    let bearers: Vec<Option<String>> = stub
        .calls()
        .iter()
        .filter(|c| c.path == "/v1/items/1")
        .skip(1)
        .map(|c| c.authorization.clone())
        .collect();
    assert!(
        bearers
            .iter()
            .all(|b| b.as_deref() == Some("Bearer at-new")),
        "{bearers:?}"
    );
    let kept = creds.get(&account());
    assert_eq!(
        kept.field(Field::RefreshToken).map(|s| s.expose()),
        Some("rt-old"),
        "an answer without a refresh token keeps the old one"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_401_forces_one_refresh_and_a_resend() {
    let stub = Stub::start(vec![
        Canned::json("GET", "/v1/items/1", 401, json!({"message": "expired"})),
        Canned::json(
            "POST",
            "/oauth/token",
            200,
            json!({"access_token": "at-2", "token_type": "Bearer"}),
        ),
        Canned::json("GET", "/v1/items/1", 200, json!({"ok": true})),
    ])
    .await;
    let creds = MemoryCreds::with(
        &account(),
        Stored::default()
            .with(Field::ClientId, "cid")
            .with(Field::AccessToken, "at-1")
            .with(Field::RefreshToken, "rt-1"),
    );
    let client = client(creds.clone());
    let spec = spec(&stub, oauth(stub.base_url(), BTreeMap::new()));
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
    let paths: Vec<String> = stub.calls().iter().map(|c| c.path.clone()).collect();
    assert_eq!(paths, vec!["/v1/items/1", "/oauth/token", "/v1/items/1"]);
    assert_eq!(
        stub.calls()[2].authorization.as_deref(),
        Some("Bearer at-2")
    );
    assert_eq!(creds.saves(), 1);

    // Without a refresh token the 401 stands.
    let stub2 = Stub::start(vec![Canned::json(
        "GET",
        "/v1/items/1",
        401,
        json!({"message": "expired"}),
    )])
    .await;
    let creds2 = MemoryCreds::with(
        &account(),
        Stored::default()
            .with(Field::ClientId, "cid")
            .with(Field::AccessToken, "at-1"),
    );
    let client2 = crate::support::client(creds2);
    let spec2 = crate::support::spec(&stub2, oauth(stub2.base_url(), BTreeMap::new()));
    let err = client2
        .call(
            &spec2,
            Some(&account()),
            &no_account_params(),
            &params(&[("id", "1")]),
            &AllowAll,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, ConnectorError::NotAuthenticated(ref m) if m == "expired"),
        "{err}"
    );
    assert_eq!(stub2.calls().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_grant_is_not_authenticated() {
    let stub = Stub::start(vec![
        Canned::json("GET", "/v1/items/1", 401, json!({})),
        Canned::json(
            "POST",
            "/oauth/token",
            400,
            json!({"error": "invalid_grant", "error_description": "revoked"}),
        ),
    ])
    .await;
    let creds = MemoryCreds::with(
        &account(),
        Stored::default()
            .with(Field::ClientId, "cid")
            .with(Field::AccessToken, "at-1")
            .with(Field::RefreshToken, "rt-dead"),
    );
    let client = client(creds);
    let spec = spec(&stub, oauth(stub.base_url(), BTreeMap::new()));
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
    assert!(
        matches!(err, ConnectorError::NotAuthenticated(ref m) if m.contains("revoked") && m.contains("connect the account again")),
        "{err}"
    );
}

/// A judge that refuses one host — the workspace's deny list, in a test.
struct Deny(&'static str);

impl HostJudge for Deny {
    fn judge(&self, host: &str) -> Result<(), String> {
        if host.starts_with(self.0) {
            Err(format!("security.net.deny_hosts forbids {host}"))
        } else {
            Ok(())
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_denied_token_host_is_refused_before_the_form_is_posted() {
    let stub = Stub::start(vec![Canned::json(
        "POST",
        "/oauth/token",
        200,
        json!({"access_token": "never", "token_type": "Bearer"}),
    )])
    .await;
    let creds = MemoryCreds::with(
        &account(),
        Stored::default()
            .with(Field::ClientId, "client-1")
            .with(Field::ClientSecret, "shh"),
    );
    let client = client(creds.clone());
    let auth = oauth(stub.base_url(), BTreeMap::new());
    let err = exchange(
        &client,
        &auth,
        &account(),
        "code-9",
        &secret("verifier-1"),
        "http://127.0.0.1:4478/cb",
        &Deny("127.0.0.1"),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, ConnectorError::HostRefused { .. }), "{err}");
    assert!(stub.calls().is_empty(), "nothing was posted");
    assert_eq!(creds.saves(), 0, "nothing was kept");
    // The consent page is judged the same way.
    assert!(matches!(
        authorize_url(
            &auth,
            "client-1",
            "http://127.0.0.1/x",
            CountingEntropy::from(1).as_ref(),
            &Deny("auth.example.com")
        ),
        Err(ConnectorError::HostRefused { .. })
    ));
}
