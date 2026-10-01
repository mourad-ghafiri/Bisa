//! `Authorization: Basic base64(username:password)` — Atlassian's email and
//! API token, say.

use super::{mismatch, need, Applied, AuthSigner, SignContext};
use crate::creds::{Credential, Field, Stored};
use crate::error::ConnectorError;
use crate::http::Request;
use base64::Engine as _;

pub struct BasicSigner;

impl AuthSigner for BasicSigner {
    fn credential_for(&self, stored: &Stored) -> Result<Credential, ConnectorError> {
        Ok(Credential::Basic {
            username: need(stored, Field::Username)?.expose().to_string(),
            password: need(stored, Field::Password)?,
        })
    }

    fn apply(
        &self,
        credential: &Credential,
        req: &mut Request,
        _cx: &SignContext<'_>,
    ) -> Result<Applied, ConnectorError> {
        let Credential::Basic { username, password } = credential else {
            return Err(mismatch("basic"));
        };
        let pair = format!("{username}:{}", password.expose());
        let encoded = base64::engine::general_purpose::STANDARD.encode(pair.as_bytes());
        req.headers
            .push(("authorization".into(), format!("Basic {encoded}")));
        Ok(Applied {
            secrets: vec![password.expose().to_string(), pair],
        })
    }
}
