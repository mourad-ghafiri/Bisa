//! A key sent in a header or a query parameter, with an optional prefix
//! before it (`Token `, `Bot `) — and, where a platform asks for more than
//! one, each further key in a place of its own.

use super::{mismatch, need, Applied, AuthSigner, SignContext};
use crate::creds::{Credential, Field, Secret, Stored};
use crate::error::ConnectorError;
use crate::http::Request;
use crate::spec::{ExtraKey, KeyPlace};

pub struct ApiKeySigner<'a> {
    pub place: &'a KeyPlace,
    pub prefix: Option<&'a str>,
    pub extra: &'a [ExtraKey],
}

impl AuthSigner for ApiKeySigner<'_> {
    fn credential_for(&self, stored: &Stored) -> Result<Credential, ConnectorError> {
        let key = need(stored, Field::ApiKey)?;
        let extra = self
            .extra
            .iter()
            .map(|k| need(stored, k.field))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Credential::ApiKey { key, extra })
    }

    fn apply(
        &self,
        credential: &Credential,
        req: &mut Request,
        _cx: &SignContext<'_>,
    ) -> Result<Applied, ConnectorError> {
        let Credential::ApiKey { key, extra } = credential else {
            // LCOV_EXCL_START: the store hands each scheme its own kind; the arm keeps the match total
            return Err(mismatch("api_key"));
            // LCOV_EXCL_STOP
        };
        if extra.len() != self.extra.len() {
            // LCOV_EXCL_START: `credential_for` reads one secret per key of the same scheme; the arm keeps the pairing honest
            return Err(mismatch("api_key"));
            // LCOV_EXCL_STOP
        }
        let mut applied = Applied::default();
        write_key(req, self.place, self.prefix, key, &mut applied);
        for (spec, secret) in self.extra.iter().zip(extra) {
            write_key(
                req,
                &spec.place,
                spec.prefix.as_deref(),
                secret,
                &mut applied,
            );
        }
        Ok(applied)
    }
}

/// One key in its place, hidden from the request's `Debug` and named among
/// what the request exposed.
fn write_key(
    req: &mut Request,
    place: &KeyPlace,
    prefix: Option<&str>,
    key: &Secret,
    applied: &mut Applied,
) {
    let value = format!("{}{}", prefix.unwrap_or_default(), key.expose());
    match place {
        KeyPlace::Header { name } => {
            req.hidden_headers.push(name.to_ascii_lowercase());
            req.headers.push((name.clone(), value));
        }
        KeyPlace::Query { name } => {
            req.url.query_pairs_mut().append_pair(name, &value);
            req.secret_query.push(name.clone());
        }
    }
    applied.secrets.push(key.expose().to_string());
}
