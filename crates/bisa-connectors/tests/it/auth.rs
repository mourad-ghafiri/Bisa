//! Each scheme writes its credential where the platform reads it — asserted
//! on what the stub recorded, never on a real token.

use crate::support::*;
use aws_lc_rs::encoding::{AsDer, Pkcs8V1Der};
use aws_lc_rs::rand::SystemRandom;
use aws_lc_rs::rsa::{KeyPair as RsaKey, KeySize};
use aws_lc_rs::signature::{
    EcdsaKeyPair, KeyPair as _, UnparsedPublicKey, ECDSA_P256_SHA256_FIXED,
    ECDSA_P256_SHA256_FIXED_SIGNING, RSA_PKCS1_2048_8192_SHA256,
};
use base64::Engine as _;
use bisa_connectors::creds::{Field, Stored};
use bisa_connectors::hosts::AllowAll;
use bisa_connectors::spec::{AuthSpec, JwtAlg, KeyPlace};
use bisa_connectors::ConnectorError;
use serde_json::{json, Value};
use std::collections::BTreeMap;

async fn call_with(
    auth: AuthSpec,
    stored: Stored,
) -> (Stub, Result<bisa_connectors::Outcome, ConnectorError>) {
    let stub = Stub::start(vec![Canned::json(
        "GET",
        "/v1/items/1",
        200,
        json!({"ok": true}),
    )])
    .await;
    let client = client(MemoryCreds::with(&account(), stored));
    let spec = spec(&stub, auth);
    let out = client
        .call(
            &spec,
            Some(&account()),
            &no_account_params(),
            &params(&[("id", "1")]),
            &AllowAll,
        )
        .await;
    (stub, out)
}

#[tokio::test(flavor = "multi_thread")]
async fn api_key_in_header_with_prefix() {
    let (stub, out) = call_with(
        AuthSpec::ApiKey {
            place: KeyPlace::Header {
                name: "X-Api-Key".into(),
            },
            prefix: Some("Token ".into()),
        },
        Stored::default().with(Field::ApiKey, "k-123"),
    )
    .await;
    out.unwrap();
    assert_eq!(
        stub.calls()[0].header("x-api-key").as_deref(),
        Some("Token k-123")
    );
    assert!(stub.calls()[0].authorization.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn api_key_in_query() {
    let (stub, out) = call_with(
        AuthSpec::ApiKey {
            place: KeyPlace::Query { name: "key".into() },
            prefix: None,
        },
        Stored::default().with(Field::ApiKey, "k-456"),
    )
    .await;
    out.unwrap();
    assert_eq!(stub.calls()[0].query.as_deref(), Some("key=k-456"));
}

#[tokio::test(flavor = "multi_thread")]
async fn bearer() {
    let (stub, out) = call_with(
        AuthSpec::Bearer,
        Stored::default().with(Field::Token, "xoxb-1"),
    )
    .await;
    out.unwrap();
    assert_eq!(
        stub.calls()[0].authorization.as_deref(),
        Some("Bearer xoxb-1")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn basic_is_base64_user_colon_pass() {
    let (stub, out) = call_with(
        AuthSpec::Basic,
        Stored::default()
            .with(Field::Username, "me@x.io")
            .with(Field::Password, "tok"),
    )
    .await;
    out.unwrap();
    assert_eq!(
        stub.calls()[0].authorization.as_deref(),
        Some("Basic bWVAeC5pbzp0b2s=")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn oauth2_bearer() {
    let auth = AuthSpec::OAuth2 {
        authorization_url: "https://auth.example.com/authorize".into(),
        token_url: "https://auth.example.com/token".into(),
        scopes: vec![],
        pkce: true,
        extra: BTreeMap::new(),
    };
    let (stub, out) = call_with(auth, Stored::default().with(Field::AccessToken, "at-1")).await;
    out.unwrap();
    assert_eq!(
        stub.calls()[0].authorization.as_deref(),
        Some("Bearer at-1")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn none_sends_no_authorization() {
    let (stub, out) = call_with(
        AuthSpec::None,
        Stored::default().with(Field::Token, "unused"),
    )
    .await;
    out.unwrap();
    assert!(stub.calls()[0].authorization.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_required_field_is_not_authenticated_without_a_request() {
    let (stub, out) = call_with(AuthSpec::Bearer, Stored::default()).await;
    let err = out.unwrap_err();
    assert!(
        matches!(err, ConnectorError::NotAuthenticated(m) if m == "token is not set for this account")
    );
    assert!(stub.calls().is_empty(), "nothing was sent");

    // No account at all, on a scheme that needs one.
    let stub2 = Stub::start(vec![]).await;
    let client = client(MemoryCreds::with(&account(), Stored::default()));
    let err = client
        .call(
            &spec(&stub2, AuthSpec::Bearer),
            None,
            &no_account_params(),
            &params(&[("id", "1")]),
            &AllowAll,
        )
        .await
        .unwrap_err();
    assert!(matches!(err, ConnectorError::NotAuthenticated(_)));
    assert!(stub2.calls().is_empty());
}

/// DER bytes as a PEM block, the way a platform hands a key out.
fn pem(label: &str, der: &[u8]) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(der);
    let lines: Vec<&str> = b64
        .as_bytes()
        .chunks(64)
        .map(|c| std::str::from_utf8(c).unwrap())
        .collect();
    format!(
        "-----BEGIN {label}-----\n{}\n-----END {label}-----\n",
        lines.join("\n")
    )
}

fn jwt(alg: JwtAlg) -> AuthSpec {
    AuthSpec::Jwt {
        alg,
        claims: BTreeMap::from([
            ("iss".to_string(), "{account.issuer}".to_string()),
            ("aud".to_string(), "platform-v1".to_string()),
        ]),
        header: BTreeMap::from([("kid".to_string(), "{account.key_id}".to_string())]),
        ttl_secs: 1200,
    }
}

fn account_params() -> BTreeMap<String, Value> {
    BTreeMap::from([
        ("issuer".to_string(), json!("ISS-1")),
        ("key_id".to_string(), json!("K1")),
    ])
}

/// The bearer token the stub saw, split into its three parts and decoded.
fn token_parts(stub: &Stub) -> (Value, Value, Vec<u8>, String) {
    let authz = stub.calls()[0].authorization.clone().unwrap();
    let token = authz.strip_prefix("Bearer ").unwrap();
    let parts: Vec<&str> = token.split('.').collect();
    assert_eq!(parts.len(), 3, "{token}");
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let header: Value = serde_json::from_slice(&b64.decode(parts[0]).unwrap()).unwrap();
    let claims: Value = serde_json::from_slice(&b64.decode(parts[1]).unwrap()).unwrap();
    (
        header,
        claims,
        b64.decode(parts[2]).unwrap(),
        format!("{}.{}", parts[0], parts[1]),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn a_jwt_scheme_signs_es256_with_the_accounts_key_and_the_clock() {
    let rng = SystemRandom::new();
    let doc = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng).unwrap();
    let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, doc.as_ref()).unwrap();
    let stub = Stub::start(vec![Canned::json(
        "GET",
        "/v1/items/1",
        200,
        json!({"ok": true}),
    )])
    .await;
    let client = client(MemoryCreds::with(
        &account(),
        Stored::default().with(Field::PrivateKey, pem("PRIVATE KEY", doc.as_ref())),
    ));
    let spec = spec(&stub, jwt(JwtAlg::Es256));
    client
        .call(
            &spec,
            Some(&account()),
            &account_params(),
            &params(&[("id", "1")]),
            &AllowAll,
        )
        .await
        .unwrap();
    let (header, claims, signature, signed) = token_parts(&stub);
    assert_eq!(header, json!({"alg": "ES256", "typ": "JWT", "kid": "K1"}));
    assert_eq!(
        claims,
        json!({"iss": "ISS-1", "aud": "platform-v1", "iat": 999_970, "exp": 1_001_200}),
        "the clock stands at 1_000_000: `iat` sits IAT_LEEWAY_SECS behind it for a platform whose clock runs ahead, `exp` is the clock plus the 1200 s life"
    );
    UnparsedPublicKey::new(&ECDSA_P256_SHA256_FIXED, pair.public_key().as_ref())
        .verify(signed.as_bytes(), &signature)
        .expect("the platform's public key verifies the signature");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_jwt_scheme_signs_rs256_with_a_pkcs8_rsa_key() {
    let key = RsaKey::generate(KeySize::Rsa2048).unwrap();
    let der = AsDer::<Pkcs8V1Der>::as_der(&key).unwrap();
    let stub = Stub::start(vec![Canned::json(
        "GET",
        "/v1/items/1",
        200,
        json!({"ok": true}),
    )])
    .await;
    let client = client(MemoryCreds::with(
        &account(),
        Stored::default().with(Field::PrivateKey, pem("PRIVATE KEY", der.as_ref())),
    ));
    let spec = spec(&stub, jwt(JwtAlg::Rs256));
    client
        .call(
            &spec,
            Some(&account()),
            &account_params(),
            &params(&[("id", "1")]),
            &AllowAll,
        )
        .await
        .unwrap();
    let (header, _claims, signature, signed) = token_parts(&stub);
    assert_eq!(header["alg"], "RS256");
    UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, key.public_key().as_ref())
        .verify(signed.as_bytes(), &signature)
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_key_the_scheme_cannot_sign_with_is_refused_by_name_and_nothing_is_sent() {
    let stub = Stub::start(vec![Canned::json(
        "GET",
        "/v1/items/1",
        200,
        json!({"ok": true}),
    )])
    .await;
    // A SEC1 key: refused with the conversion to make, the key never quoted.
    let sec1 = "-----BEGIN EC PRIVATE KEY-----\nAQIDBAU=\n-----END EC PRIVATE KEY-----\n";
    let c = client(MemoryCreds::with(
        &account(),
        Stored::default().with(Field::PrivateKey, sec1),
    ));
    let spec = spec(&stub, jwt(JwtAlg::Es256));
    let err = c
        .call(
            &spec,
            Some(&account()),
            &account_params(),
            &params(&[("id", "1")]),
            &AllowAll,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&err, ConnectorError::NotAuthenticated(m) if m.contains("SEC1") && !m.contains("AQIDBAU")),
        "{err}"
    );
    // An RSA key under ES256: not the curve the algorithm signs with.
    let key = RsaKey::generate(KeySize::Rsa2048).unwrap();
    let der = AsDer::<Pkcs8V1Der>::as_der(&key).unwrap();
    let c = client(MemoryCreds::with(
        &account(),
        Stored::default().with(Field::PrivateKey, pem("PRIVATE KEY", der.as_ref())),
    ));
    let err = c
        .call(
            &spec,
            Some(&account()),
            &account_params(),
            &params(&[("id", "1")]),
            &AllowAll,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&err, ConnectorError::NotAuthenticated(m) if m.contains("ES256")),
        "{err}"
    );
    // No key at all: the field is named.
    let c = client(MemoryCreds::with(&account(), Stored::default()));
    let err = c
        .call(
            &spec,
            Some(&account()),
            &account_params(),
            &params(&[("id", "1")]),
            &AllowAll,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&err, ConnectorError::NotAuthenticated(m) if m == "private_key is not set for this account"),
        "{err}"
    );
    assert!(stub.calls().is_empty(), "nothing reached the platform");
}
