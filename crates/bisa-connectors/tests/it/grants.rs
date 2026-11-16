//! The two auth abilities a definition may add: more than one key on a key
//! scheme, and a client-credentials login — each against the loopback stub,
//! asserted on what the stub recorded.

use crate::support::*;
use bisa_connectors::creds::{Field, Stored};
use bisa_connectors::hosts::AllowAll;
use bisa_connectors::spec::{AuthSpec, ExtraKey, KeyPlace, ScopeJoin, TokenAuth};
use bisa_connectors::{Client, ConnectorError, ReqwestTransport};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

const API_KEY: &str = "api-key-7f3a9c";
const APP_KEY: &str = "app-key-51be02";

/// A platform that asks for two keys: an API key in one header, an
/// application key in another.
fn two_keys() -> AuthSpec {
    AuthSpec::ApiKey {
        place: KeyPlace::Header {
            name: "DD-API-KEY".into(),
        },
        prefix: None,
        extra_keys: vec![ExtraKey {
            field: Field::AppKey,
            place: KeyPlace::Header {
                name: "DD-APPLICATION-KEY".into(),
            },
            prefix: None,
        }],
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn two_keys_land_in_two_headers() {
    let stub = Stub::start(vec![Canned::json(
        "GET",
        "/v1/items/1",
        200,
        json!({"ok": true}),
    )])
    .await;
    let creds = MemoryCreds::with(
        &account(),
        Stored::default()
            .with(Field::ApiKey, API_KEY)
            .with(Field::AppKey, APP_KEY),
    );
    let out = client(creds)
        .call(
            &spec(&stub, two_keys()),
            Some(&account()),
            &no_account_params(),
            &params(&[("id", "1")]),
            &AllowAll,
        )
        .await
        .unwrap();
    assert_eq!(out.selected, json!({"ok": true}));
    let call = &stub.calls()[0];
    assert_eq!(call.header("dd-api-key").as_deref(), Some(API_KEY));
    assert_eq!(call.header("dd-application-key").as_deref(), Some(APP_KEY));
    assert_eq!(call.authorization, None, "no key is a bearer");
}

/// A second key in the query with its own prefix, beside a first in a
/// header: each where the definition says, and neither in the request's
/// own `Debug`.
#[tokio::test(flavor = "multi_thread")]
async fn a_second_key_travels_in_its_own_place_with_its_own_prefix() {
    let stub = Stub::start(vec![Canned::json(
        "GET",
        "/v1/items/1",
        200,
        json!({"ok": true}),
    )])
    .await;
    let auth = AuthSpec::ApiKey {
        place: KeyPlace::Header {
            name: "X-Key".into(),
        },
        prefix: Some("Key ".into()),
        extra_keys: vec![
            ExtraKey {
                field: Field::AppKey,
                place: KeyPlace::Query { name: "app".into() },
                prefix: Some("a:".into()),
            },
            ExtraKey {
                field: Field::Token,
                place: KeyPlace::Query { name: "org".into() },
                prefix: None,
            },
        ],
    };
    let stored = Stored::default()
        .with(Field::ApiKey, API_KEY)
        .with(Field::AppKey, APP_KEY)
        .with(Field::Token, "org-token-99");
    client(MemoryCreds::with(&account(), stored.clone()))
        .call(
            &spec(&stub, auth.clone()),
            Some(&account()),
            &no_account_params(),
            &params(&[("id", "1")]),
            &AllowAll,
        )
        .await
        .unwrap();
    let call = &stub.calls()[0];
    assert_eq!(
        call.header("x-key").as_deref(),
        Some(format!("Key {API_KEY}").as_str())
    );
    let query: BTreeMap<String, String> =
        url::form_urlencoded::parse(call.query.as_deref().unwrap_or_default().as_bytes())
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
    assert_eq!(query["app"], format!("a:{APP_KEY}"));
    assert_eq!(query["org"], "org-token-99");

    // The request as a log would show it: every key hidden, both query pairs.
    let mut req = bisa_connectors::Request::new(
        bisa_connectors::Method::Get,
        url::Url::parse("https://api.example.com/v1/x?page=2").unwrap(),
        std::time::Duration::from_secs(1),
    );
    let none = BTreeMap::new();
    let credential = bisa_connectors::auth::credential_for(&auth, &stored).unwrap();
    let applied = bisa_connectors::auth::apply(
        &auth,
        &credential,
        &mut req,
        &bisa_connectors::auth::SignContext {
            now: 0,
            account: &none,
        },
    )
    .unwrap();
    assert_eq!(applied.secrets, vec![API_KEY, APP_KEY, "org-token-99"]);
    let shown = format!("{req:?}");
    for secret in [API_KEY, APP_KEY, "org-token-99"] {
        assert!(!shown.contains(secret), "{shown}");
    }
    assert!(shown.contains("page=2"), "{shown}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_key_that_is_not_set_is_named_and_nothing_is_sent() {
    let stub = Stub::start(vec![]).await;
    let creds = MemoryCreds::with(&account(), Stored::default().with(Field::ApiKey, API_KEY));
    let err = client(creds)
        .call(
            &spec(&stub, two_keys()),
            Some(&account()),
            &no_account_params(),
            &params(&[("id", "1")]),
            &AllowAll,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&err, ConnectorError::NotAuthenticated(m) if m == "app_key is not set for this account"),
        "{err:?}"
    );
    assert!(stub.calls().is_empty(), "half a credential is never sent");
}

/// A platform that says back what it was sent hands the step a placeholder
/// for the second key as for the first — in an answer and in a refusal.
#[tokio::test(flavor = "multi_thread")]
async fn every_key_is_scrubbed_from_what_the_platform_says_back() {
    let stub = Stub::start(vec![
        Canned::json(
            "GET",
            "/v1/items/1",
            200,
            json!({"you_sent": {"api": API_KEY, "app": format!("app={APP_KEY}")}}),
        ),
        Canned::json(
            "GET",
            "/v1/items/1",
            403,
            json!({"message": format!("application key {APP_KEY} may not read this (api key {API_KEY})")}),
        ),
    ])
    .await;
    let creds = MemoryCreds::with(
        &account(),
        Stored::default()
            .with(Field::ApiKey, API_KEY)
            .with(Field::AppKey, APP_KEY),
    );
    let client = client(creds);
    let spec = spec(&stub, two_keys());
    let ps = params(&[("id", "1")]);
    let out = client
        .call(
            &spec,
            Some(&account()),
            &no_account_params(),
            &ps,
            &AllowAll,
        )
        .await
        .unwrap();
    let said = out.selected.to_string();
    assert!(!said.contains(API_KEY) && !said.contains(APP_KEY), "{said}");
    let err = client
        .call(
            &spec,
            Some(&account()),
            &no_account_params(),
            &ps,
            &AllowAll,
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(!err.contains(API_KEY) && !err.contains(APP_KEY), "{err}");
    assert!(err.contains("may not read this"), "{err}");
}

// --- client credentials ----------------------------------------------------

const CLIENT_SECRET: &str = "cs-9d41e7aa";

fn grant(stub: &Stub) -> AuthSpec {
    AuthSpec::ClientCredentials {
        token_url: format!("{}/oauth/token", stub.base_url()),
        scopes: vec![],
        scope_join: ScopeJoin::Space,
        token_auth: TokenAuth::Form,
        grant_type: "client_credentials".into(),
        params: BTreeMap::new(),
    }
}

fn client_of() -> Stored {
    Stored::default()
        .with(Field::ClientId, "cid-1")
        .with(Field::ClientSecret, CLIENT_SECRET)
}

fn token(access: &str, expires_in: u64) -> Canned {
    Canned::json(
        "POST",
        "/oauth/token",
        200,
        json!({"access_token": access, "expires_in": expires_in, "token_type": "bearer"}),
    )
}

fn item(n: u64) -> Canned {
    Canned::json("GET", "/v1/items/1", 200, json!({"ok": n}))
}

fn client_at(creds: Arc<MemoryCreds>, clock: Arc<FixedClock>) -> Client {
    Client::new(
        Arc::new(ReqwestTransport::new()),
        creds,
        clock,
        CountingEntropy::from(1),
    )
}

fn token_calls(stub: &Stub) -> Vec<Call> {
    stub.calls()
        .into_iter()
        .filter(|c| c.path == "/oauth/token")
        .collect()
}

fn bearers(stub: &Stub) -> Vec<Option<String>> {
    stub.calls()
        .into_iter()
        .filter(|c| c.path == "/v1/items/1")
        .map(|c| c.authorization)
        .collect()
}

/// No browser: the first call asks for a token with the client's id and
/// secret, the next calls reuse it, and one within a minute of its expiry
/// asks again.
#[tokio::test(flavor = "multi_thread")]
async fn a_token_is_asked_for_once_reused_and_asked_for_again_when_it_lapses() {
    let stub = Stub::start(vec![
        token("at-1", 3600),
        item(1),
        item(2),
        token("at-2", 3600),
        item(3),
    ])
    .await;
    let creds = MemoryCreds::with(&account(), client_of());
    let clock = FixedClock::at(1_000_000);
    let client = client_at(creds.clone(), clock.clone());
    let spec = spec(&stub, grant(&stub));
    let ps = params(&[("id", "1")]);
    let none = no_account_params();

    client
        .call(&spec, Some(&account()), &none, &ps, &AllowAll)
        .await
        .unwrap();
    let asked = token_calls(&stub);
    assert_eq!(asked.len(), 1);
    let form = asked[0].form();
    assert_eq!(form["grant_type"], "client_credentials");
    assert_eq!(form["client_id"], "cid-1");
    assert_eq!(form["client_secret"], CLIENT_SECRET);
    assert!(
        !form.contains_key("scope"),
        "no scope was declared: {form:?}"
    );
    assert_eq!(asked[0].authorization, None);
    assert_eq!(
        asked[0].header("content-type").as_deref(),
        Some("application/x-www-form-urlencoded")
    );
    let kept = creds.get(&account());
    assert_eq!(
        kept.field(Field::AccessToken).map(|s| s.expose()),
        Some("at-1")
    );
    assert_eq!(kept.expires_at, Some(1_000_000 + 3600));
    assert!(kept.field(Field::RefreshToken).is_none());

    // Well inside its life: the same token, no second request.
    *clock.0.lock().unwrap() = 1_000_000 + 1800;
    client
        .call(&spec, Some(&account()), &none, &ps, &AllowAll)
        .await
        .unwrap();
    assert_eq!(token_calls(&stub).len(), 1);

    // Thirty seconds left: asked again before the call is sent.
    *clock.0.lock().unwrap() = 1_000_000 + 3570;
    client
        .call(&spec, Some(&account()), &none, &ps, &AllowAll)
        .await
        .unwrap();
    assert_eq!(token_calls(&stub).len(), 2);
    assert_eq!(creds.saves(), 2);
    assert_eq!(
        bearers(&stub),
        vec![
            Some("Bearer at-1".to_string()),
            Some("Bearer at-1".to_string()),
            Some("Bearer at-2".to_string())
        ]
    );
}

/// The dialect a platform may ask for: the client in a Basic header, its own
/// grant word, scopes joined its way, and pairs read from the account.
#[tokio::test(flavor = "multi_thread")]
async fn a_grant_is_spelled_as_the_definition_says() {
    let stub = Stub::start(vec![token("at-1", 3600), item(1)]).await;
    let auth = AuthSpec::ClientCredentials {
        token_url: format!("{}/oauth/token", stub.base_url()),
        scopes: vec!["meeting:read".into(), "user:read".into()],
        scope_join: ScopeJoin::Comma,
        token_auth: TokenAuth::Basic,
        grant_type: "account_credentials".into(),
        params: BTreeMap::from([
            ("account_id".to_string(), "{account.account_id}".to_string()),
            (
                "audience".to_string(),
                "https://api.example.com/".to_string(),
            ),
        ]),
    };
    let creds = MemoryCreds::with(&account(), client_of());
    let account_params: BTreeMap<String, Value> =
        BTreeMap::from([("account_id".to_string(), json!("acct & co"))]);
    client(creds)
        .call(
            &spec(&stub, auth),
            Some(&account()),
            &account_params,
            &params(&[("id", "1")]),
            &AllowAll,
        )
        .await
        .unwrap();
    let asked = token_calls(&stub);
    let form = asked[0].form();
    assert_eq!(form["grant_type"], "account_credentials");
    assert_eq!(form["scope"], "meeting:read,user:read");
    assert_eq!(
        form["account_id"], "acct & co",
        "encoded once, read back whole"
    );
    assert_eq!(form["audience"], "https://api.example.com/");
    assert!(
        !form.contains_key("client_id") && !form.contains_key("client_secret"),
        "the client travels in the header alone: {form:?}"
    );
    use base64::Engine as _;
    let basic = base64::engine::general_purpose::STANDARD.encode(format!("cid-1:{CLIENT_SECRET}"));
    assert_eq!(
        asked[0].authorization.as_deref(),
        Some(format!("Basic {basic}").as_str())
    );
}

/// A token the platform refuses before its expiry said so: asked for again
/// once, the call sent again once, and the answer stands.
#[tokio::test(flavor = "multi_thread")]
async fn a_401_asks_for_a_new_token_once_and_sends_again() {
    let stub = Stub::start(vec![
        Canned::json("GET", "/v1/items/1", 401, json!({"message": "expired"})),
        token("at-new", 3600),
        item(2),
    ])
    .await;
    let creds = MemoryCreds::with(
        &account(),
        Stored {
            expires_at: Some(1_000_000 + 3600),
            ..client_of().with(Field::AccessToken, "at-old")
        },
    );
    let out = client(creds.clone())
        .call(
            &spec(&stub, grant(&stub)),
            Some(&account()),
            &no_account_params(),
            &params(&[("id", "1")]),
            &AllowAll,
        )
        .await
        .unwrap();
    assert_eq!(out.selected, json!({"ok": 2}));
    assert_eq!(token_calls(&stub).len(), 1);
    assert_eq!(
        bearers(&stub),
        vec![
            Some("Bearer at-old".to_string()),
            Some("Bearer at-new".to_string())
        ]
    );

    // A second 401, on the new token, is the platform's answer.
    let stub = Stub::start(vec![
        Canned::json("GET", "/v1/items/1", 401, json!({"message": "no"})),
        token("at-new", 3600),
        Canned::json("GET", "/v1/items/1", 401, json!({"message": "still no"})),
    ])
    .await;
    let creds = MemoryCreds::with(&account(), client_of().with(Field::AccessToken, "at-old"));
    let err = client(creds)
        .call(
            &spec(&stub, grant(&stub)),
            Some(&account()),
            &no_account_params(),
            &params(&[("id", "1")]),
            &AllowAll,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&err, ConnectorError::NotAuthenticated(m) if m == "still no"),
        "{err:?}"
    );
    assert_eq!(token_calls(&stub).len(), 1, "one renewal, never a loop");
}

/// Several calls with no token yet ask for one between them.
#[tokio::test(flavor = "multi_thread")]
async fn calls_at_once_ask_for_one_token_between_them() {
    let stub = Stub::start(vec![token("at-1", 3600), item(1), item(2), item(3)]).await;
    let creds = MemoryCreds::with(&account(), client_of());
    let client = client(creds.clone());
    let spec = spec(&stub, grant(&stub));
    let (acct, none, ps) = (account(), no_account_params(), params(&[("id", "1")]));
    let (a, b, c) = tokio::join!(
        client.call(&spec, Some(&acct), &none, &ps, &AllowAll),
        client.call(&spec, Some(&acct), &none, &ps, &AllowAll),
        client.call(&spec, Some(&acct), &none, &ps, &AllowAll),
    );
    a.unwrap();
    b.unwrap();
    c.unwrap();
    assert_eq!(token_calls(&stub).len(), 1, "one grant for three flights");
    assert_eq!(creds.saves(), 1);
    assert!(bearers(&stub)
        .iter()
        .all(|b| b.as_deref() == Some("Bearer at-1")));
}

/// What the account is missing is named before anything is sent, and a
/// platform that refuses the client says so in words a person can act on —
/// never with the secret it was shown.
#[tokio::test(flavor = "multi_thread")]
async fn a_client_the_platform_refuses_is_the_accounts_to_mend() {
    let stub = Stub::start(vec![]).await;
    for (stored, missing) in [
        (Stored::default(), "client_id"),
        (
            Stored::default().with(Field::ClientId, "cid-1"),
            "client_secret",
        ),
    ] {
        let err = client(MemoryCreds::with(&account(), stored))
            .call(
                &spec(&stub, grant(&stub)),
                Some(&account()),
                &no_account_params(),
                &params(&[("id", "1")]),
                &AllowAll,
            )
            .await
            .unwrap_err();
        assert!(
            matches!(&err, ConnectorError::NotAuthenticated(m) if m == &format!("{missing} is not set for this account")),
            "{err:?}"
        );
    }
    assert!(stub.calls().is_empty());

    for refusal in [
        Canned::json(
            "POST",
            "/oauth/token",
            400,
            json!({"error": "invalid_client", "error_description": format!("secret {CLIENT_SECRET} is wrong")}),
        ),
        Canned::json(
            "POST",
            "/oauth/token",
            401,
            json!({"error": "invalid_client"}),
        ),
    ] {
        let stub = Stub::start(vec![refusal]).await;
        let err = client(MemoryCreds::with(&account(), client_of()))
            .call(
                &spec(&stub, grant(&stub)),
                Some(&account()),
                &no_account_params(),
                &params(&[("id", "1")]),
                &AllowAll,
            )
            .await
            .unwrap_err();
        assert!(
            matches!(&err, ConnectorError::NotAuthenticated(m) if m == "the platform refused the client's id and secret; set them again"),
            "{err:?}"
        );
        assert_eq!(bearers(&stub).len(), 0, "the operation is never sent");
    }

    // Any other refusal is said as the platform said it, scrubbed.
    let stub = Stub::start(vec![Canned::json(
        "POST",
        "/oauth/token",
        400,
        json!({"error": "invalid_scope", "error_description": format!("not for {CLIENT_SECRET}")}),
    )])
    .await;
    let err = client(MemoryCreds::with(&account(), client_of()))
        .call(
            &spec(&stub, grant(&stub)),
            Some(&account()),
            &no_account_params(),
            &params(&[("id", "1")]),
            &AllowAll,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&err, ConnectorError::Refused { status: 400, reason } if reason.starts_with("invalid_scope")),
        "{err:?}"
    );
    assert!(!err.to_string().contains(CLIENT_SECRET), "{err}");
}

/// A platform that adds a refresh token to the grant's answer: the account
/// has no field for one, and none is kept. A token with no expiry is used
/// until the platform refuses it.
#[tokio::test(flavor = "multi_thread")]
async fn a_grants_answer_keeps_the_access_token_alone() {
    let stub = Stub::start(vec![
        Canned::json(
            "POST",
            "/oauth/token",
            200,
            json!({"access_token": "at-1", "refresh_token": "rt-never", "token_type": "Bearer"}),
        ),
        item(1),
        item(2),
    ])
    .await;
    let creds = MemoryCreds::with(&account(), client_of());
    let clock = FixedClock::at(1_000_000);
    let client = client_at(creds.clone(), clock.clone());
    let spec = spec(&stub, grant(&stub));
    let ps = params(&[("id", "1")]);
    client
        .call(
            &spec,
            Some(&account()),
            &no_account_params(),
            &ps,
            &AllowAll,
        )
        .await
        .unwrap();
    let kept = creds.get(&account());
    assert!(kept.field(Field::RefreshToken).is_none());
    assert_eq!(kept.expires_at, None);
    *clock.0.lock().unwrap() = 9_000_000;
    client
        .call(
            &spec,
            Some(&account()),
            &no_account_params(),
            &ps,
            &AllowAll,
        )
        .await
        .unwrap();
    assert_eq!(token_calls(&stub).len(), 1);
}

/// The token endpoint is a host like any other: one the workspace denies is
/// refused before the client's secret is posted anywhere.
#[tokio::test(flavor = "multi_thread")]
async fn a_denied_token_host_is_refused_before_the_grant_is_posted() {
    struct DenyAll;
    impl bisa_connectors::HostJudge for DenyAll {
        fn judge(&self, host: &str) -> Result<(), String> {
            Err(format!("{host} is denied"))
        }
    }
    let stub = Stub::start(vec![]).await;
    let err = client(MemoryCreds::with(&account(), client_of()))
        .call(
            &spec(&stub, grant(&stub)),
            Some(&account()),
            &no_account_params(),
            &params(&[("id", "1")]),
            &DenyAll,
        )
        .await
        .unwrap_err();
    assert!(matches!(err, ConnectorError::HostRefused { .. }), "{err:?}");
    assert!(stub.calls().is_empty());
}
