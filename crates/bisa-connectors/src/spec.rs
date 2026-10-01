//! The call-shaped mirror of a connector definition.
//!
//! The domain's `Connector` and `Operation` live in `bisa-core`, which
//! this crate cannot see; the engine maps one operation of one connector into
//! a [`CallSpec`] once, and everything here reads that. The shapes are the
//! same words as the definition's so the mapping is a copy, not a translation.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Duration;

/// An HTTP method the definition may name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
            Method::Head => "HEAD",
        }
    }
}

/// Where an API key travels.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "in", rename_all = "snake_case")]
pub enum KeyPlace {
    Header { name: String },
    Query { name: String },
}

/// How an account proves itself to the platform.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "scheme", rename_all = "snake_case")]
pub enum AuthSpec {
    /// No credential at all — a public API, or a loopback service.
    None,
    /// A key in a header or a query parameter; `prefix` is written verbatim
    /// before the key (`"Token "`).
    ApiKey {
        place: KeyPlace,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prefix: Option<String>,
    },
    /// `Authorization: Bearer <token>` from a pasted long-lived token.
    Bearer,
    /// `Authorization: Basic base64(username:password)`.
    Basic,
    /// Authorization code with PKCE; the tokens are minted by the flow and
    /// refreshed here before they expire.
    OAuth2 {
        authorization_url: String,
        token_url: String,
        #[serde(default)]
        scopes: Vec<String>,
        #[serde(default = "yes")]
        pkce: bool,
        /// Extra pairs on the authorization URL (`access_type=offline`) and,
        /// for `token_auth = "basic"`, how the client authenticates at the
        /// token endpoint.
        #[serde(default)]
        extra: BTreeMap<String, String>,
    },
    /// A token this crate signs at each request with the account's private
    /// key and sends as `Authorization: Bearer`; `claims` and `header` are
    /// templates over `{account.<param>}`, `iat` and `exp` are the clock's.
    Jwt {
        alg: JwtAlg,
        claims: BTreeMap<String, String>,
        #[serde(default)]
        header: BTreeMap<String, String>,
        ttl_secs: u64,
    },
}

fn yes() -> bool {
    true
}

/// The signing algorithms a `jwt` scheme may name, by their JOSE words.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum JwtAlg {
    Es256,
    Rs256,
}

impl JwtAlg {
    pub fn as_str(self) -> &'static str {
        match self {
            JwtAlg::Es256 => "ES256",
            JwtAlg::Rs256 => "RS256",
        }
    }
}

/// The kind a parameter's rendered text is read as.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ParamKind {
    Text,
    Number,
    Bool,
    Json,
    /// A path the [`crate::files::Files`] port reads; the bytes travel in a
    /// multipart part or a raw body and never through a template.
    File,
}

impl ParamKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ParamKind::Text => "text",
            ParamKind::Number => "number",
            ParamKind::Bool => "bool",
            ParamKind::Json => "json",
            ParamKind::File => "file",
        }
    }
}

/// What an operation sends, by the definition's `kind` — one encoder each in
/// [`crate::body`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CallBody {
    /// `application/json`; a leaf that is exactly one typed placeholder
    /// becomes the typed value.
    Json { value: serde_json::Value },
    /// `application/x-www-form-urlencoded`; a field naming an absent
    /// optional parameter is dropped.
    Form { fields: BTreeMap<String, String> },
    /// `multipart/form-data`; text parts are templates, file parts carry a
    /// `file` parameter's bytes.
    Multipart { parts: Vec<Part> },
    /// One parameter's bytes under `content_type`.
    Raw { content_type: String, from: String },
}

/// One part of a multipart body.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Part {
    pub name: String,
    #[serde(flatten)]
    pub source: PartSource,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub content_type: Option<String>,
}

/// Where a part's bytes come from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum PartSource {
    File { file: String },
    Text { text: String },
}

/// One parameter an operation takes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ParamSpec {
    pub name: String,
    pub kind: ParamKind,
    #[serde(default)]
    pub required: bool,
}

/// A header the client fills with the caller's key, so the platform can tell
/// a resend from a second request: the same key on a retry, a step run again
/// and a restart means the write happens once. Only a writing operation has
/// one; the caller decides the key (`CallSpec::idempotency_key`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Idempotency {
    pub header: String,
}

/// How an operation pages: the optional `text` parameter the cursor rides in,
/// the dotted path in the answer that names the next cursor, and how many
/// pages at most. The client follows the cursor while the answer names one
/// and pages remain, under the one deadline, and joins the pages' selected
/// arrays into one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Paging {
    pub cursor_param: String,
    pub next_cursor: String,
    pub max_pages: u8,
}

/// One operation of one connector, ready to call.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CallSpec {
    /// The connector's id, for messages.
    pub connector: String,
    /// The operation's id, for messages.
    pub operation: String,
    /// A template over `{account.<param>}` only; its host must be in `hosts`.
    pub base_url: String,
    /// The hosts this connector may reach: `host[:port]`, or `*.suffix`.
    pub hosts: Vec<String>,
    /// Accept a self-signed certificate — allowed for loopback hosts only.
    #[serde(default)]
    pub insecure_tls: bool,
    pub auth: AuthSpec,
    pub method: Method,
    /// A template over `{account.…}` and `{params.…}`, joined under the base URL's path.
    pub path: String,
    /// Query pairs whose values are templates; a pair whose value renders
    /// empty from an absent optional parameter is dropped.
    #[serde(default)]
    pub query: BTreeMap<String, String>,
    /// Extra headers whose values are templates.
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// What the request carries, and how it is encoded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<CallBody>,
    #[serde(default)]
    pub params: Vec<ParamSpec>,
    /// A dotted path into the response JSON that becomes the step's output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub select: Option<String>,
    /// Whether the operation changes something on the platform — a write is
    /// retried only where the platform promises the retry is safe.
    #[serde(default)]
    pub writes: bool,
    /// The header a write's key travels in, when the platform honours one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency: Option<Idempotency>,
    /// The caller's key for this call — the same for every attempt of the
    /// same work. Sent only when `idempotency` names a header.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    /// How the operation pages, when it does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<Paging>,
    /// The whole call's deadline, including retries and every page.
    #[serde(with = "secs")]
    #[schemars(with = "u64")]
    pub timeout: Duration,
}

impl CallSpec {
    /// The parameter named `name`, if the operation takes one.
    pub fn param(&self, name: &str) -> Option<&ParamSpec> {
        self.params.iter().find(|p| p.name == name)
    }
}

mod secs {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S: Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
        d.as_secs().serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        u64::deserialize(d).map(Duration::from_secs)
    }
}
