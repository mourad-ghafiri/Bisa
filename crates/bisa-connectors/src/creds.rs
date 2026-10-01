//! Credentials, in memory only, and the ports the engine fills.
//!
//! A [`Secret`] cannot be printed: its `Debug` and `Display` hide it, and
//! [`Secret::expose`] is the one door, at the request that carries it. What
//! an account holds arrives as a [`Stored`] through the [`Credentials`] port;
//! a token this crate mints or refreshes goes back through the same port,
//! which is the only way one is ever kept.

use crate::error::ConnectorError;
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::fmt;

/// A credential in memory. Never shown; `expose` is the one door.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The value, for the one request that needs it.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Trimmed and non-empty, else nothing.
    pub fn some(value: impl Into<String>) -> Option<Self> {
        let value: String = value.into();
        let value = value.trim();
        (!value.is_empty()).then(|| Self(value.to_string()))
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(…)")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("…")
    }
}

/// The secret fields an account may hold, named on the wire by `as_str`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Field {
    ApiKey,
    Token,
    Username,
    Password,
    ClientId,
    ClientSecret,
    AccessToken,
    RefreshToken,
    /// A signing key in PEM, for the `jwt` scheme.
    PrivateKey,
}

impl Field {
    pub const ALL: [Field; 9] = [
        Field::ApiKey,
        Field::Token,
        Field::Username,
        Field::Password,
        Field::ClientId,
        Field::ClientSecret,
        Field::AccessToken,
        Field::RefreshToken,
        Field::PrivateKey,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Field::ApiKey => "api_key",
            Field::Token => "token",
            Field::Username => "username",
            Field::Password => "password",
            Field::ClientId => "client_id",
            Field::ClientSecret => "client_secret",
            Field::AccessToken => "access_token",
            Field::RefreshToken => "refresh_token",
            Field::PrivateKey => "private_key",
        }
    }

    pub fn parse(word: &str) -> Option<Field> {
        Field::ALL.into_iter().find(|f| f.as_str() == word)
    }
}

impl fmt::Display for Field {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which account of which connector a call runs as.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AccountRef {
    pub connector: String,
    pub account: String,
}

impl AccountRef {
    pub fn new(connector: impl Into<String>, account: impl Into<String>) -> Self {
        Self {
            connector: connector.into(),
            account: account.into(),
        }
    }

    /// The one word a lock or a message names the account by.
    pub fn key(&self) -> String {
        format!("{}:{}", self.connector, self.account)
    }
}

/// Everything an account holds: its secret fields, and when its OAuth access
/// token expires. A field never set is simply absent.
#[derive(Clone, Debug, Default)]
pub struct Stored {
    pub fields: BTreeMap<Field, Secret>,
    pub expires_at: Option<u64>,
}

impl Stored {
    pub fn field(&self, field: Field) -> Option<&Secret> {
        self.fields.get(&field)
    }

    pub fn with(mut self, field: Field, value: impl Into<String>) -> Self {
        self.fields.insert(field, Secret::new(value));
        self
    }
}

/// What an OAuth2 exchange or refresh answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenSet {
    pub access_token: Secret,
    pub refresh_token: Option<Secret>,
    pub expires_at: Option<u64>,
    pub scope: Option<String>,
}

/// The credential a request is sent with, resolved from the scheme and the
/// stored fields.
#[derive(Clone, Debug)]
pub enum Credential {
    None,
    ApiKey(Secret),
    Bearer(Secret),
    Basic {
        username: String,
        password: Secret,
    },
    OAuth2(Secret),
    /// The private key a token is signed with at each request.
    Jwt(Secret),
}

/// Where secrets live — the engine's keystore, a test's map.
#[async_trait]
pub trait Credentials: Send + Sync {
    /// Every secret field held for the account plus the OAuth expiry.
    async fn load(&self, account: &AccountRef) -> Result<Stored, ConnectorError>;
    /// Replace the access and refresh tokens and the expiry after an exchange
    /// or a refresh — the one way a token this crate obtained is kept.
    async fn save_tokens(
        &self,
        account: &AccountRef,
        tokens: &TokenSet,
    ) -> Result<(), ConnectorError>;
}

/// Unix seconds, so a test can stand still.
pub trait Clock: Send + Sync {
    fn now(&self) -> u64;
}

/// Random bytes, so a test can predict a state and a verifier.
pub trait Entropy: Send + Sync {
    fn fill(&self, out: &mut [u8]);
}

/// The wall clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

/// The operating system's random source.
#[derive(Clone, Copy, Debug, Default)]
pub struct OsEntropy;

impl Entropy for OsEntropy {
    fn fill(&self, out: &mut [u8]) {
        // The OS refusing random bytes is not a state a request can proceed
        // from; an all-zero buffer would mint a guessable state, so this
        // panics rather than continue.
        getrandom::fill(out).expect("the operating system's random source");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_never_prints() {
        let s = Secret::new("hunter2");
        assert_eq!(format!("{s:?}"), "Secret(…)");
        assert_eq!(s.to_string(), "…");
        assert_eq!(s.expose(), "hunter2");
        assert!(Secret::some("  ").is_none());
        assert_eq!(Secret::some(" x ").unwrap().expose(), "x");
    }

    #[test]
    fn fields_round_trip_their_wire_words() {
        for f in Field::ALL {
            assert_eq!(Field::parse(f.as_str()), Some(f));
        }
        assert_eq!(Field::parse("nope"), None);
    }
}
