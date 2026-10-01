//! How an account proves itself: one signer per scheme behind one trait. A
//! signer reads the credential a scheme needs from what the account holds,
//! and writes it onto a request — a header, a query pair, a token it signs
//! on the spot. What was exposed on the way is returned so the caller can
//! scrub it from anything it says afterwards.

mod api_key;
mod basic;
mod bearer;
mod jwt;
mod oauth2;

use crate::creds::{Credential, Stored};
use crate::error::ConnectorError;
use crate::http::Request;
use crate::spec::AuthSpec;
use serde_json::Value;
use std::collections::BTreeMap;

pub use jwt::{parse_pem, Pem};

/// The strings a request now carries that must never appear in a message.
#[derive(Clone, Debug, Default)]
pub struct Applied {
    pub secrets: Vec<String>,
}

/// What a signer may read besides the credential: the clock, for a token's
/// life, and the account's non-secret parameters, for its claims.
#[derive(Clone, Copy, Debug)]
pub struct SignContext<'a> {
    pub now: u64,
    pub account: &'a BTreeMap<String, Value>,
}

/// One scheme's way of proving who is calling.
pub trait AuthSigner {
    /// The credential the scheme needs, from what the account holds; a
    /// missing field is named so the person knows what to set.
    fn credential_for(&self, stored: &Stored) -> Result<Credential, ConnectorError>;

    /// Write the credential onto the request as the scheme says.
    fn apply(
        &self,
        credential: &Credential,
        req: &mut Request,
        cx: &SignContext<'_>,
    ) -> Result<Applied, ConnectorError>;
}

/// The signer a scheme names — the one place the schemes meet the signers.
pub fn signer_for(auth: &AuthSpec) -> Box<dyn AuthSigner + '_> {
    match auth {
        AuthSpec::None => Box::new(NoAuth),
        AuthSpec::ApiKey { place, prefix } => Box::new(api_key::ApiKeySigner {
            place,
            prefix: prefix.as_deref(),
        }),
        AuthSpec::Bearer => Box::new(bearer::BearerSigner),
        AuthSpec::Basic => Box::new(basic::BasicSigner),
        AuthSpec::OAuth2 { .. } => Box::new(oauth2::OAuth2Signer),
        AuthSpec::Jwt {
            alg,
            claims,
            header,
            ttl_secs,
        } => Box::new(jwt::JwtSigner {
            alg: *alg,
            claims,
            header,
            ttl_secs: *ttl_secs,
        }),
    }
}

/// The credential a scheme needs, from what the account holds.
pub fn credential_for(auth: &AuthSpec, stored: &Stored) -> Result<Credential, ConnectorError> {
    signer_for(auth).credential_for(stored)
}

/// Write the credential onto the request as the scheme says, and the
/// platform's user agent when the definition set none.
pub fn apply(
    auth: &AuthSpec,
    credential: &Credential,
    req: &mut Request,
    cx: &SignContext<'_>,
) -> Result<Applied, ConnectorError> {
    let applied = signer_for(auth).apply(credential, req, cx)?;
    if req.header("user-agent").is_none() {
        req.headers.push(("user-agent".into(), "bisa".into()));
    }
    Ok(applied)
}

/// The field a scheme needs, or the sentence for its absence.
fn need(
    stored: &Stored,
    field: crate::creds::Field,
) -> Result<crate::creds::Secret, ConnectorError> {
    stored.field(field).cloned().ok_or_else(|| {
        ConnectorError::NotAuthenticated(format!("{field} is not set for this account"))
    })
}

/// The scheme's word for a credential of another shape — a programming
/// error, never a person's.
fn mismatch(scheme: &str) -> ConnectorError {
    ConnectorError::BadDefinition(format!("the credential does not fit the {scheme} scheme"))
}

/// An open API, or one the account's parameters address by themselves.
struct NoAuth;

impl AuthSigner for NoAuth {
    fn credential_for(&self, _stored: &Stored) -> Result<Credential, ConnectorError> {
        Ok(Credential::None)
    }

    fn apply(
        &self,
        _credential: &Credential,
        _req: &mut Request,
        _cx: &SignContext<'_>,
    ) -> Result<Applied, ConnectorError> {
        Ok(Applied::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::creds::Field;
    use crate::spec::{KeyPlace, Method};
    use std::time::Duration;

    fn request() -> Request {
        Request::new(
            Method::Get,
            url::Url::parse("https://api.example.com/v1/x").unwrap(),
            Duration::from_secs(5),
        )
    }

    fn cx<'a>(account: &'a BTreeMap<String, Value>) -> SignContext<'a> {
        SignContext {
            now: 1_000,
            account,
        }
    }

    #[test]
    fn a_missing_field_is_named_and_nothing_is_applied() {
        let err = credential_for(&AuthSpec::Bearer, &Stored::default()).unwrap_err();
        assert!(
            matches!(err, ConnectorError::NotAuthenticated(m) if m == "token is not set for this account")
        );
        let err = credential_for(
            &AuthSpec::Basic,
            &Stored::default().with(Field::Username, "u"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("password"));
        assert!(matches!(
            credential_for(&AuthSpec::None, &Stored::default()).unwrap(),
            Credential::None
        ));
        let jwt = AuthSpec::Jwt {
            alg: crate::spec::JwtAlg::Es256,
            claims: BTreeMap::new(),
            header: BTreeMap::new(),
            ttl_secs: 60,
        };
        let err = credential_for(&jwt, &Stored::default()).unwrap_err();
        assert!(err.to_string().contains("private_key is not set"));
    }

    #[test]
    fn each_scheme_writes_its_own_place_and_reports_what_it_exposed() {
        let account = BTreeMap::new();
        let mut req = request();
        let applied = apply(
            &AuthSpec::ApiKey {
                place: KeyPlace::Query { name: "key".into() },
                prefix: None,
            },
            &Credential::ApiKey(crate::Secret::new("k1")),
            &mut req,
            &cx(&account),
        )
        .unwrap();
        assert_eq!(req.url.query(), Some("key=k1"));
        assert_eq!(applied.secrets, vec!["k1"]);
        assert!(!format!("{req:?}").contains("k1"), "{req:?}");

        let mut req = request();
        apply(
            &AuthSpec::ApiKey {
                place: KeyPlace::Header {
                    name: "X-Api-Key".into(),
                },
                prefix: Some("Token ".into()),
            },
            &Credential::ApiKey(crate::Secret::new("k2")),
            &mut req,
            &cx(&account),
        )
        .unwrap();
        assert_eq!(req.header("x-api-key"), Some("Token k2"));
        assert!(!format!("{req:?}").contains("k2"));

        let mut req = request();
        let applied = apply(
            &AuthSpec::Basic,
            &Credential::Basic {
                username: "u".into(),
                password: crate::Secret::new("p"),
            },
            &mut req,
            &cx(&account),
        )
        .unwrap();
        assert_eq!(req.header("authorization"), Some("Basic dTpw"));
        assert!(applied.secrets.contains(&"u:p".to_string()));
        assert_eq!(req.header("user-agent"), Some("bisa"));

        let mut req = request();
        let err = apply(
            &AuthSpec::Bearer,
            &Credential::Basic {
                username: "u".into(),
                password: crate::Secret::new("p"),
            },
            &mut req,
            &cx(&account),
        )
        .unwrap_err();
        assert!(matches!(err, ConnectorError::BadDefinition(_)));
    }
}
