//! The access token an OAuth2 flow minted, sent as a bearer. The flow itself
//! — authorize, exchange, refresh — is [`crate::oauth`]'s; this signer only
//! reads the token the account holds and writes it.

use super::{bearer::write_bearer, mismatch, need, Applied, AuthSigner, SignContext};
use crate::creds::{Credential, Field, Stored};
use crate::error::ConnectorError;
use crate::http::Request;

pub struct OAuth2Signer;

impl AuthSigner for OAuth2Signer {
    fn credential_for(&self, stored: &Stored) -> Result<Credential, ConnectorError> {
        Ok(Credential::OAuth2(need(stored, Field::AccessToken)?))
    }

    fn apply(
        &self,
        credential: &Credential,
        req: &mut Request,
        _cx: &SignContext<'_>,
    ) -> Result<Applied, ConnectorError> {
        let Credential::OAuth2(token) = credential else {
            return Err(mismatch("oauth2"));
        };
        Ok(write_bearer(req, token))
    }
}
