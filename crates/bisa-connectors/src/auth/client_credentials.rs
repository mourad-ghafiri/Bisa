//! The access token a client-credentials grant answered, sent as a bearer.
//! Asking for it — and asking again when it lapses — is [`crate::oauth`]'s;
//! this signer says what the account must hold and writes the token.

use super::{bearer::write_bearer, mismatch, need, Applied, AuthSigner, SignContext};
use crate::creds::{Credential, Field, Secret, Stored};
use crate::error::ConnectorError;
use crate::http::Request;

pub struct ClientCredentialsSigner;

impl AuthSigner for ClientCredentialsSigner {
    /// The client's id and secret are what the account must hold; the token
    /// is the platform's answer to them. The one handed back here is the
    /// stored one, or none yet: the call asks for a usable one before it
    /// sends anything ([`crate::oauth::ensure_fresh`]).
    fn credential_for(&self, stored: &Stored) -> Result<Credential, ConnectorError> {
        need(stored, Field::ClientId)?;
        need(stored, Field::ClientSecret)?;
        Ok(Credential::OAuth2(
            stored
                .field(Field::AccessToken)
                .cloned()
                .unwrap_or_else(|| Secret::new("")),
        ))
    }

    fn apply(
        &self,
        credential: &Credential,
        req: &mut Request,
        _cx: &SignContext<'_>,
    ) -> Result<Applied, ConnectorError> {
        let Credential::OAuth2(token) = credential else {
            // LCOV_EXCL_START: the store hands each scheme its own kind; the arm keeps the match total
            return Err(mismatch("client_credentials"));
            // LCOV_EXCL_STOP
        };
        Ok(write_bearer(req, token))
    }
}
