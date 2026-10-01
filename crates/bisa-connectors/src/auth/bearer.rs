//! `Authorization: Bearer <token>` — a pasted long-lived token.

use super::{mismatch, need, Applied, AuthSigner, SignContext};
use crate::creds::{Credential, Field, Secret, Stored};
use crate::error::ConnectorError;
use crate::http::Request;

pub struct BearerSigner;

impl AuthSigner for BearerSigner {
    fn credential_for(&self, stored: &Stored) -> Result<Credential, ConnectorError> {
        Ok(Credential::Bearer(need(stored, Field::Token)?))
    }

    fn apply(
        &self,
        credential: &Credential,
        req: &mut Request,
        _cx: &SignContext<'_>,
    ) -> Result<Applied, ConnectorError> {
        let Credential::Bearer(token) = credential else {
            return Err(mismatch("bearer"));
        };
        Ok(write_bearer(req, token))
    }
}

/// The one way a bearer token is written — shared by every scheme that ends
/// as one.
pub(super) fn write_bearer(req: &mut Request, token: &Secret) -> Applied {
    req.headers
        .push(("authorization".into(), format!("Bearer {}", token.expose())));
    Applied {
        secrets: vec![token.expose().to_string()],
    }
}
