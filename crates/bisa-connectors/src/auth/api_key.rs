//! A key sent in a header or a query parameter, with an optional prefix
//! before it (`Token `, `Bot `).

use super::{mismatch, need, Applied, AuthSigner, SignContext};
use crate::creds::{Credential, Field, Stored};
use crate::error::ConnectorError;
use crate::http::Request;
use crate::spec::KeyPlace;

pub struct ApiKeySigner<'a> {
    pub place: &'a KeyPlace,
    pub prefix: Option<&'a str>,
}

impl AuthSigner for ApiKeySigner<'_> {
    fn credential_for(&self, stored: &Stored) -> Result<Credential, ConnectorError> {
        Ok(Credential::ApiKey(need(stored, Field::ApiKey)?))
    }

    fn apply(
        &self,
        credential: &Credential,
        req: &mut Request,
        _cx: &SignContext<'_>,
    ) -> Result<Applied, ConnectorError> {
        let Credential::ApiKey(key) = credential else {
            return Err(mismatch("api_key"));
        };
        let value = format!("{}{}", self.prefix.unwrap_or_default(), key.expose());
        match self.place {
            KeyPlace::Header { name } => {
                req.hidden_headers.push(name.to_ascii_lowercase());
                req.headers.push((name.clone(), value));
            }
            KeyPlace::Query { name } => {
                req.url.query_pairs_mut().append_pair(name, &value);
                req.secret_query = Some(name.clone());
            }
        }
        Ok(Applied {
            secrets: vec![key.expose().to_string()],
        })
    }
}
