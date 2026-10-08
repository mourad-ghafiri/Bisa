//! Connectors: the declarative definition of one outside platform's API, and
//! the accounts a node holds for it.
//!
//! A **connector** (kind 33414) is a document — a base URL, the hosts it may
//! reach, an auth scheme, account-level parameters and a list of operations
//! — that reads the same on every node, so it travels like a skill. An
//! **account** is one login to that platform on this machine: a label, the
//! non-secret parameters the base URL needs (a Jira site, a page id), which
//! secret fields are set, and nothing else. The secrets themselves live in the
//! keystore under `connector:<connector>:<account>:<field>` and never enter a
//! record, a snapshot, a route or a prompt.
//!
//! A definition's strings are templates under [`crate::template::Grammar::Connector`]:
//! `{account.<param>}` reads the account's parameters, `{params.<name>}` an
//! operation's. Those are the only two roots — a definition cannot read a run,
//! and a workflow step cannot read a definition's parameters. The engine binds
//! a step's rendered parameters to an operation and renders the definition
//! once, account parameters first, then the operation's; a substituted value
//! is never scanned again.
//!
//! Zero I/O: [`Connector::validate`] is a pure walk over the document that
//! parses the base URL's scheme and authority by hand. The HTTP client, the
//! retries and OAuth live in `bisa-connectors`; the engine maps this
//! shape into that crate's once.

use crate::id::{AccountId, ConnectorId, OperationId};
use crate::origin::Origin;
use crate::tags::Tags;
use crate::template::{placeholders_in, Grammar, Placeholder};
use crate::workflow::InputName;
use crate::Localize as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// A serialised definition may not exceed this: a connector is a description
/// of an API, not a copy of it.
pub const MAX_CONNECTOR_BYTES: usize = 64 * 1024;

pub use bisa_netrules::{host_matches, is_host_pattern, is_loopback, RESERVED_HEADERS};

/// The longest life a signed token may be given: a token is minted per
/// request, so a long one buys nothing and widens what a leak is worth.
pub const MAX_JWT_TTL_SECS: u64 = 86_400;

/// The life a signed token has when the definition names none.
pub const DEFAULT_JWT_TTL_SECS: u64 = 300;

/// The longest deadline one operation may ask for, retries and pages
/// included: a bulk export is slow, a call that has not answered in ten
/// minutes is not going to.
pub const MAX_OPERATION_TIMEOUT_SECS: u64 = 600;

/// The most pages one paged operation reads in one call: enough for a
/// poll's backlog, not enough for a step to walk a platform's whole history.
pub const MAX_PAGES: u8 = 20;

/// A parameter's name — the input-name word, so a definition and the step
/// that calls it spell it the same way.
pub type ParamName = InputName;

/// One outside platform's API, declared.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Connector {
    pub id: ConnectorId,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub tags: Tags,
    #[serde(default)]
    pub origin: Origin,
    /// `https://…` (or `http://` on a loopback host). A template over
    /// `{account.<param>}` only: `https://{account.site}.atlassian.net`.
    pub base_url: String,
    /// The only hosts a call may reach, `host[:port]` or `*.suffix`. The base
    /// URL's host must be one of them. An OAuth2 scheme's consent page and
    /// token endpoint are declared by their own URLs ([`Self::oauth_hosts`])
    /// and need no row here.
    pub hosts: Vec<String>,
    /// Accept a certificate nobody signed — for a loopback host only, the way
    /// Obsidian's local API serves itself.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub insecure_tls: bool,
    pub auth: AuthScheme,
    /// Account-level parameters a person fills once per account and the base
    /// URL, paths and bodies read as `{account.<name>}`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<ParamDef>,
    pub operations: Vec<Operation>,
    /// The operation *Check* runs to prove an account works: one with no
    /// required parameter, and one that writes nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check: Option<OperationId>,
    /// The catalog's stamp on a built-in: the bundled definition's revision
    /// when it was installed or last refreshed. A person's own definition
    /// has none (0, not written). The engine refreshes an installed built-in
    /// at start when the bundle's revision is higher.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub revision: u32,
    pub created_at: u64,
}

fn is_zero_u32(n: &u32) -> bool {
    *n == 0
}

/// How a request proves who is calling.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, tag = "scheme", rename_all = "snake_case")]
pub enum AuthScheme {
    /// An open API, or one the account's parameters address by themselves.
    None,
    /// A key sent in a header or a query parameter, with an optional prefix
    /// before it (`Token `, `Bot `).
    ApiKey {
        place: KeyPlace,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prefix: Option<String>,
    },
    /// `Authorization: Bearer <token>` — a pasted long-lived token.
    Bearer,
    /// `Authorization: Basic base64(username:password)` — Atlassian's email
    /// and API token, say.
    Basic,
    /// Authorization code, with PKCE by default. The person registers their
    /// own client at the platform and enters its id and secret on the account.
    #[serde(rename = "oauth2")]
    OAuth2 {
        authorization_url: String,
        token_url: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        scopes: Vec<String>,
        #[serde(default = "default_true")]
        pkce: bool,
        /// Extra query pairs on the authorization URL (`access_type=offline`,
        /// `prompt=consent`) and the token request's `token_auth = "basic"`.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        extra: BTreeMap<String, String>,
        /// The name the client's id travels under, on the authorization URL
        /// and in the token form: `client_id` (RFC 6749) unless the platform
        /// spells it otherwise (TikTok's `client_key`).
        #[serde(
            default = "default_client_id_param",
            skip_serializing_if = "is_default_client_id_param"
        )]
        client_id_param: String,
        /// How the scopes are joined on the authorization URL: by a space
        /// (RFC 6749) or, where the platform says so, by a comma.
        #[serde(default, skip_serializing_if = "ScopeJoin::is_default")]
        scope_join: ScopeJoin,
        /// How the PKCE challenge is written: the base64url of the SHA-256
        /// (RFC 7636) or, where the platform says so, its hex.
        #[serde(default, skip_serializing_if = "ChallengeEncoding::is_default")]
        code_challenge: ChallengeEncoding,
    },
    /// A token the platform signs itself at each request and sends as
    /// `Authorization: Bearer` — the signed-assertion scheme behind service
    /// accounts and developer APIs. The account holds the private key (PEM,
    /// PKCS#8; PKCS#1 for RSA); `claims` and `header` are templates over
    /// `{account.<param>}` (`iss = "{account.issuer}"`, `kid =
    /// "{account.key_id}"`); `iat` and `exp` (`iat + ttl_secs`) are the
    /// clock's, `alg` and `typ` the signer's.
    Jwt {
        alg: JwtAlg,
        claims: BTreeMap<String, String>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        header: BTreeMap<String, String>,
        #[serde(default = "default_jwt_ttl")]
        ttl_secs: u64,
    },
}

fn default_true() -> bool {
    true
}

/// The RFC 6749 name of the client's id, which most platforms keep.
pub const DEFAULT_CLIENT_ID_PARAM: &str = "client_id";

fn default_client_id_param() -> String {
    DEFAULT_CLIENT_ID_PARAM.to_string()
}

fn is_default_client_id_param(s: &String) -> bool {
    s == DEFAULT_CLIENT_ID_PARAM
}

/// How an OAuth2 scheme joins its scopes on the authorization URL.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ScopeJoin {
    /// `scope=a b` — RFC 6749's space.
    #[default]
    Space,
    /// `scope=a,b` — the platforms that say so (TikTok, Slack, Linear).
    Comma,
}

impl ScopeJoin {
    pub fn is_default(&self) -> bool {
        *self == Self::Space
    }

    /// The character the scopes are joined with.
    pub fn separator(self) -> &'static str {
        match self {
            Self::Space => " ",
            Self::Comma => ",",
        }
    }
}

/// How an OAuth2 scheme writes the PKCE challenge of its verifier.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeEncoding {
    /// The base64url, no padding, of the SHA-256 — RFC 7636's `S256`.
    #[default]
    Base64url,
    /// The lowercase hex of the SHA-256 — what TikTok's desktop flow reads
    /// under the same `S256` word.
    Hex,
}

impl ChallengeEncoding {
    pub fn is_default(&self) -> bool {
        *self == Self::Base64url
    }
}

fn default_jwt_ttl() -> u64 {
    DEFAULT_JWT_TTL_SECS
}

/// The signing algorithms a `jwt` scheme may name, by their JOSE words.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum JwtAlg {
    /// ECDSA over P-256 with SHA-256 — the `.p8` keys developer APIs hand out.
    Es256,
    /// RSA PKCS#1 v1.5 with SHA-256 — the keys service accounts hand out.
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

/// Where an API key goes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, tag = "in", rename_all = "snake_case")]
pub enum KeyPlace {
    Header { name: String },
    Query { name: String },
}

/// The secret fields an account may hold; which ones a scheme needs is
/// [`AuthScheme::fields`]. The wire words are the keystore's field names.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SecretField {
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

impl SecretField {
    pub const ALL: [SecretField; 9] = [
        SecretField::ApiKey,
        SecretField::Token,
        SecretField::Username,
        SecretField::Password,
        SecretField::ClientId,
        SecretField::ClientSecret,
        SecretField::AccessToken,
        SecretField::RefreshToken,
        SecretField::PrivateKey,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            SecretField::ApiKey => "api_key",
            SecretField::Token => "token",
            SecretField::Username => "username",
            SecretField::Password => "password",
            SecretField::ClientId => "client_id",
            SecretField::ClientSecret => "client_secret",
            SecretField::AccessToken => "access_token",
            SecretField::RefreshToken => "refresh_token",
            SecretField::PrivateKey => "private_key",
        }
    }

    pub fn parse(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.as_str() == word)
    }
}

impl std::fmt::Display for SecretField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl AuthScheme {
    /// Every secret field an account under this scheme may hold.
    pub fn fields(&self) -> &'static [SecretField] {
        match self {
            AuthScheme::None => &[],
            AuthScheme::ApiKey { .. } => &[SecretField::ApiKey],
            AuthScheme::Bearer => &[SecretField::Token],
            AuthScheme::Basic => &[SecretField::Username, SecretField::Password],
            AuthScheme::OAuth2 { .. } => &[
                SecretField::ClientId,
                SecretField::ClientSecret,
                SecretField::AccessToken,
                SecretField::RefreshToken,
            ],
            AuthScheme::Jwt { .. } => &[SecretField::PrivateKey],
        }
    }

    /// The fields without which no call can be made. OAuth2 needs only the
    /// client id to *start* a connection; the tokens arrive from the flow.
    pub fn required(&self) -> &'static [SecretField] {
        match self {
            AuthScheme::None => &[],
            AuthScheme::ApiKey { .. } => &[SecretField::ApiKey],
            AuthScheme::Bearer => &[SecretField::Token],
            AuthScheme::Basic => &[SecretField::Username, SecretField::Password],
            AuthScheme::OAuth2 { .. } => &[SecretField::ClientId],
            AuthScheme::Jwt { .. } => &[SecretField::PrivateKey],
        }
    }

    /// The scheme's one word, as the wire spells it.
    pub fn word(&self) -> &'static str {
        match self {
            AuthScheme::None => "none",
            AuthScheme::ApiKey { .. } => "api_key",
            AuthScheme::Bearer => "bearer",
            AuthScheme::Basic => "basic",
            AuthScheme::OAuth2 { .. } => "oauth2",
            AuthScheme::Jwt { .. } => "jwt",
        }
    }

    /// Whether a call under this scheme needs an account at all.
    pub fn needs_account(&self) -> bool {
        !matches!(self, AuthScheme::None)
    }
}

/// The HTTP method of an operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
}

impl HttpMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Patch => "PATCH",
            HttpMethod::Delete => "DELETE",
            HttpMethod::Head => "HEAD",
        }
    }
}

/// What a parameter's value is, once rendered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ParamKind {
    Text,
    Number,
    Bool,
    /// A JSON value, parsed after the template renders.
    Json,
    /// A path inside the run's checkout; the engine reads the bytes when the
    /// step runs and the model never sees them. Only a multipart part or a
    /// raw body may carry it — never a template.
    File,
    /// A slash-separated path on the platform — a note's place in a vault,
    /// a file's in a tree. In a URL path its slashes stay and each segment
    /// is percent-encoded on its own; anywhere else it is text.
    Path,
}

impl ParamKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ParamKind::Text => "text",
            ParamKind::Number => "number",
            ParamKind::Bool => "bool",
            ParamKind::Json => "json",
            ParamKind::File => "file",
            ParamKind::Path => "path",
        }
    }
}

/// One parameter: of an operation (`{params.<name>}`) or of a connector's
/// account (`{account.<name>}`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ParamDef {
    pub name: ParamName,
    pub label: String,
    #[serde(default = "default_param_kind")]
    pub kind: ParamKind,
    #[serde(default)]
    pub required: bool,
    /// What goes in it — the designer's hint and the Workflow Agent's.
    #[serde(default)]
    pub doc: String,
}

fn default_param_kind() -> ParamKind {
    ParamKind::Text
}

/// What of the response becomes the step's output.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputSpec {
    /// A dotted path into the response JSON; the whole body when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub select: Option<String>,
    /// A JSON Schema the selected value satisfies — the shape a later step
    /// may rely on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<Value>,
    /// What a 2xx answer must say for the call to have succeeded, for the
    /// platforms that answer a failure with a 200: read before `select`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expect: Option<Expect>,
}

/// What a 2xx answer must say for the call to count — Slack's `ok`, a
/// GraphQL answer's `errors`, a status page's `authenticated`. Exactly one
/// of `equals` and `absent`; a miss is a refusal with the platform's own
/// sentence read at `reason`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Expect {
    /// A dotted path into the answer.
    pub path: String,
    /// The value the path must hold (`true`, `"ok"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equals: Option<Value>,
    /// The path must resolve to nothing — an answer with no `errors`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub absent: bool,
    /// A dotted path to the platform's sentence when the expectation fails
    /// (`error`, `errors.0.message`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// One thing the platform can be asked to do.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    pub id: OperationId,
    pub name: String,
    pub description: String,
    pub method: HttpMethod,
    /// Appended to the base URL; a template whose substituted values are
    /// percent-encoded as path segments.
    pub path: String,
    /// Query pairs; a value that renders empty from an absent optional
    /// parameter is dropped.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub query: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    /// What the request carries, and how it is encoded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<OperationBody>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<ParamDef>,
    #[serde(default)]
    pub output: OutputSpec,
    /// Whether the call changes something on the platform — the Workflow
    /// Agent puts an approval before one that does.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub writes: bool,
    /// This operation's own deadline, retries and pages included, when the
    /// machine's `connector_timeout_secs` is not the right one for it — a
    /// bulk export is slow, a health check should not wait a minute.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
    /// The header a write's key travels in, when the platform honours one:
    /// a retry, a step run again and a restart then send the same key, and
    /// the write happens once. Only a writing operation has one; without it,
    /// an interrupted write is never re-sent on the platform's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency: Option<Idempotency>,
    /// How the operation pages, when it does: a read whose `select` is a
    /// list is followed across its pages under the one deadline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<Paging>,
}

/// The header a writing operation's key travels in (`Idempotency-Key` on
/// most platforms; the platform's documentation names it).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Idempotency {
    pub header: String,
}

/// How a read pages: the optional `text` parameter the cursor rides in, the
/// dotted path in the answer that names the next cursor, and the most pages
/// one call reads (1 to `MAX_PAGES`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Paging {
    pub cursor_param: ParamName,
    pub next_cursor: String,
    pub max_pages: u8,
}

/// What an operation sends, and how it is encoded on the wire. Each kind is
/// one encoder in `bisa-connectors`; a definition names the kind and the
/// client does the rest.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, tag = "kind", rename_all = "snake_case")]
pub enum OperationBody {
    /// `application/json`: an object or array whose string leaves are
    /// templates; a leaf that is exactly one `{params.<name>}` of a typed
    /// parameter becomes the typed value.
    Json { value: Value },
    /// `application/x-www-form-urlencoded`: fields whose values are
    /// templates; a field that names an absent optional parameter is dropped.
    Form { fields: BTreeMap<String, String> },
    /// `multipart/form-data`: text parts, whose values are templates, and
    /// file parts, which carry a `file` parameter's bytes.
    Multipart { parts: Vec<Part> },
    /// The bytes of one parameter under `content_type` — a `file`
    /// parameter's, or another kind's rendered text.
    Raw {
        content_type: String,
        from: ParamName,
    },
}

impl OperationBody {
    /// The kind's wire word.
    pub fn kind(&self) -> &'static str {
        match self {
            OperationBody::Json { .. } => "json",
            OperationBody::Form { .. } => "form",
            OperationBody::Multipart { .. } => "multipart",
            OperationBody::Raw { .. } => "raw",
        }
    }
}

/// One part of a multipart body.
///
/// Read by hand: its source is flattened beside its own fields, and a derived
/// `flatten` lets a key nobody knows drop in silence — so the keys are held
/// to the list first ([`crate::workflow::refuse_unknown_keys`]), then read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct Part {
    pub name: String,
    #[serde(flatten)]
    pub source: PartSource,
    /// The file name the part announces; a file part without one announces
    /// the file's own name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    /// The part's media type; a file part without one is
    /// `application/octet-stream`, a text part without one is plain text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
}

/// The keys a part carries: its own, and its source's — one of `file`, `text`.
const PART_FIELDS: &[&str] = &["name", "filename", "content_type", "file", "text"];

impl<'de> Deserialize<'de> for Part {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;
        #[derive(Deserialize)]
        struct Shape {
            name: String,
            #[serde(flatten)]
            source: PartSource,
            #[serde(default)]
            filename: Option<String>,
            #[serde(default)]
            content_type: Option<String>,
        }
        let raw = serde_json::Map::<String, serde_json::Value>::deserialize(d)?;
        crate::workflow::refuse_unknown_keys::<D::Error>(&raw, "multipart part", PART_FIELDS, &[])?;
        let shape: Shape =
            serde_json::from_value(serde_json::Value::Object(raw)).map_err(D::Error::custom)?;
        Ok(Part {
            name: shape.name,
            source: shape.source,
            filename: shape.filename,
            content_type: shape.content_type,
        })
    }
}

/// Where a part's bytes come from: a `file` parameter, or a template.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum PartSource {
    File { file: ParamName },
    Text { text: String },
}

/// Whether `s` reads as a media type: `type/subtype`, optional parameters
/// after `;`, printable ASCII throughout.
pub fn is_media_type(s: &str) -> bool {
    let printable = s.chars().all(|c| (' '..='~').contains(&c));
    let Some(essence) = s.split(';').next() else {
        return false;
    };
    let essence = essence.trim();
    printable
        && essence
            .split_once('/')
            .is_some_and(|(t, sub)| !t.is_empty() && !sub.is_empty() && !essence.contains(' '))
}

/// Where in a definition a template sits — for a problem to name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TemplateSite {
    BaseUrl,
    Path(OperationId),
    Query(OperationId, String),
    Header(OperationId, String),
    /// A leaf of a JSON body.
    Body(OperationId),
    /// A field of a form body.
    Form(OperationId, String),
    /// A text part of a multipart body.
    Part(OperationId, String),
    /// A claim of a `jwt` scheme.
    AuthClaim(String),
    /// A header field of a `jwt` scheme.
    AuthHeader(String),
}

impl TemplateSite {
    /// The operation the site belongs to; none for the connector's own.
    pub fn operation(&self) -> Option<&OperationId> {
        match self {
            TemplateSite::Path(op)
            | TemplateSite::Query(op, _)
            | TemplateSite::Header(op, _)
            | TemplateSite::Body(op)
            | TemplateSite::Form(op, _)
            | TemplateSite::Part(op, _) => Some(op),
            TemplateSite::BaseUrl | TemplateSite::AuthClaim(_) | TemplateSite::AuthHeader(_) => {
                None
            }
        }
    }
}

impl std::fmt::Display for TemplateSite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TemplateSite::BaseUrl => f.write_str("base_url"),
            TemplateSite::Path(op) => write!(f, "operations.{op}.path"),
            TemplateSite::Query(op, k) => write!(f, "operations.{op}.query.{k}"),
            TemplateSite::Header(op, k) => write!(f, "operations.{op}.headers.{k}"),
            TemplateSite::Body(op) => write!(f, "operations.{op}.body"),
            TemplateSite::Form(op, k) => write!(f, "operations.{op}.body.fields.{k}"),
            TemplateSite::Part(op, k) => write!(f, "operations.{op}.body.parts.{k}"),
            TemplateSite::AuthClaim(k) => write!(f, "auth.claims.{k}"),
            TemplateSite::AuthHeader(k) => write!(f, "auth.header.{k}"),
        }
    }
}

/// One thing wrong with a definition, and the field it is about.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConnectorProblem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// The rule broken, as a message (`problem-connector-…` in `locales/en/problems.ftl`).
    pub text: crate::Text,
}

impl ConnectorProblem {
    fn at(field: impl Into<String>, text: crate::Text) -> Self {
        Self {
            field: Some(field.into()),
            text,
        }
    }
}

/// A parsed `scheme://authority` — what validation needs of a URL, read by
/// hand so the crate stays free of an URL parser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UrlHead<'a> {
    pub scheme: &'a str,
    /// `host[:port]`, lowercase in the definition's own spelling.
    pub authority: &'a str,
}

/// Split `scheme://authority/rest` into its head. `None` when there is no
/// `://`, the authority is empty, or it carries userinfo (`user@host`) —
/// a credential in a URL is refused, not parsed.
pub fn url_head(url: &str) -> Option<UrlHead<'_>> {
    let (scheme, rest) = url.split_once("://")?;
    if scheme.is_empty()
        || !scheme
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'-' || b == b'.')
    {
        return None;
    }
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    Some(UrlHead { scheme, authority })
}

/// Every string leaf of a JSON value, with a dotted path to each.
fn string_leaves<'a>(value: &'a Value, path: &mut String, out: &mut Vec<(String, &'a str)>) {
    match value {
        Value::String(s) => out.push((path.clone(), s.as_str())),
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                let len = path.len();
                if !path.is_empty() {
                    path.push('.');
                }
                path.push_str(&i.to_string());
                string_leaves(item, path, out);
                path.truncate(len);
            }
        }
        Value::Object(map) => {
            for (k, v) in map {
                let len = path.len();
                if !path.is_empty() {
                    path.push('.');
                }
                path.push_str(k);
                string_leaves(v, path, out);
                path.truncate(len);
            }
        }
        _ => {}
    }
}

impl Connector {
    pub fn operation(&self, id: &OperationId) -> Option<&Operation> {
        self.operations.iter().find(|o| &o.id == id)
    }

    pub fn param(&self, name: &str) -> Option<&ParamDef> {
        self.params.iter().find(|p| p.name.as_str() == name)
    }

    /// The authorities an OAuth2 scheme's consent page and token endpoint
    /// live on — declared by the URLs themselves, so the flow reaches them
    /// as a call reaches the operations' hosts, the deny list still ahead.
    /// Nothing for another scheme.
    pub fn oauth_hosts(&self) -> Vec<String> {
        let AuthScheme::OAuth2 {
            authorization_url,
            token_url,
            ..
        } = &self.auth
        else {
            return Vec::new();
        };
        let mut out: Vec<String> = Vec::new();
        for url in [authorization_url, token_url] {
            if let Some(head) = url_head(url) {
                let host = head.authority.to_ascii_lowercase();
                if !out.contains(&host) {
                    out.push(host);
                }
            }
        }
        out
    }

    /// The hosts a call made for this connector may reach: the declared ones
    /// and, for an OAuth2 scheme, the consent page's and the token endpoint's.
    pub fn declared_hosts(&self) -> Vec<String> {
        let mut out = self.hosts.clone();
        for h in self.oauth_hosts() {
            if !out.iter().any(|d| d.eq_ignore_ascii_case(&h)) {
                out.push(h);
            }
        }
        out
    }

    /// Every template string the definition carries, with where it sits.
    pub fn templates(&self) -> Vec<(TemplateSite, &str)> {
        let mut out = vec![(TemplateSite::BaseUrl, self.base_url.as_str())];
        for op in &self.operations {
            out.push((TemplateSite::Path(op.id.clone()), op.path.as_str()));
            for (k, v) in &op.query {
                out.push((TemplateSite::Query(op.id.clone(), k.clone()), v.as_str()));
            }
            for (k, v) in &op.headers {
                out.push((TemplateSite::Header(op.id.clone(), k.clone()), v.as_str()));
            }
            match &op.body {
                None | Some(OperationBody::Raw { .. }) => {}
                Some(OperationBody::Json { value }) => {
                    let mut leaves = Vec::new();
                    string_leaves(value, &mut String::new(), &mut leaves);
                    for (_, leaf) in leaves {
                        out.push((TemplateSite::Body(op.id.clone()), leaf));
                    }
                }
                Some(OperationBody::Form { fields }) => {
                    for (k, v) in fields {
                        out.push((TemplateSite::Form(op.id.clone(), k.clone()), v.as_str()));
                    }
                }
                Some(OperationBody::Multipart { parts }) => {
                    for part in parts {
                        if let PartSource::Text { text } = &part.source {
                            out.push((
                                TemplateSite::Part(op.id.clone(), part.name.clone()),
                                text.as_str(),
                            ));
                        }
                    }
                }
            }
        }
        if let AuthScheme::Jwt { claims, header, .. } = &self.auth {
            for (k, v) in claims {
                out.push((TemplateSite::AuthClaim(k.clone()), v.as_str()));
            }
            for (k, v) in header {
                out.push((TemplateSite::AuthHeader(k.clone()), v.as_str()));
            }
        }
        out
    }

    /// Every rule a definition must satisfy before it is stored, installed or
    /// called. Pure; never stops at the first finding.
    pub fn validate(&self) -> Vec<ConnectorProblem> {
        let mut out = Vec::new();
        let mut push = |field: &str, text: crate::Text| out.push(ConnectorProblem::at(field, text));

        if self.name.trim().is_empty() {
            push(
                "name",
                crate::text!("problem-connector-connector-needs-name"),
            );
        }
        if self.description.trim().is_empty() {
            push(
                "description",
                crate::text!("problem-connector-connector-needs-description"),
            );
        }

        // Hosts.
        if self.hosts.is_empty() {
            push(
                "hosts",
                crate::text!("problem-connector-declare-least-one-host-connector-may-reach"),
            );
        }
        for h in &self.hosts {
            if !is_host_pattern(h) {
                push(
                    "hosts",
                    crate::text!(
                        "problem-connector-not-host-write-host-port-suffix",
                        h = format!("{h:?}")
                    ),
                );
            }
        }
        if self.insecure_tls {
            for h in &self.hosts {
                if !is_loopback(h.trim_start_matches("*.")) {
                    push(
                        "insecure_tls",
                        crate::text!(
                            "problem-connector-insecure-tls-loopback-only",
                            h = h.to_string()
                        ),
                    );
                }
            }
        }

        // The base URL.
        let base_is_template = self.base_url.contains('{');
        match url_head(&self.base_url) {
            None => push(
                "base_url",
                crate::text!("problem-connector-base-url-must-be-scheme-host-port"),
            ),
            Some(head) => {
                let loopback = !base_is_template && is_loopback(head.authority);
                match head.scheme {
                    "https" => {}
                    "http" if loopback => {}
                    "http" => push(
                        "base_url",
                        crate::text!("problem-connector-http-allowed-loopback-host-only-use-https"),
                    ),
                    other => push(
                        "base_url",
                        crate::text!(
                            "problem-connector-scheme-not-http",
                            other = format!("{other:?}")
                        ),
                    ),
                }
                // A malformed pattern is refused above and vouches for nothing.
                if !base_is_template
                    && !self
                        .hosts
                        .iter()
                        .filter(|h| is_host_pattern(h))
                        .any(|h| host_matches(h, head.authority))
                {
                    push(
                        "base_url",
                        crate::text!(
                            "problem-connector-host-not-hosts",
                            a0 = (head.authority).to_string()
                        ),
                    );
                }
            }
        }

        // Connector params.
        let mut seen = BTreeSet::new();
        for p in &self.params {
            if !seen.insert(p.name.as_str()) {
                push(
                    "params",
                    crate::text!(
                        "problem-connector-parameter-declared-twice",
                        a0 = (p.name).to_string()
                    ),
                );
            }
            if p.label.trim().is_empty() {
                push(
                    "params",
                    crate::text!(
                        "problem-connector-parameter-needs-label",
                        name = p.name.to_string()
                    ),
                );
            }
            if p.kind == ParamKind::File {
                push(
                    "params",
                    crate::text!(
                        "problem-connector-parameter-account-parameter-cannot-be-file",
                        a0 = (p.name).to_string()
                    ),
                );
            }
        }
        let account_names: BTreeSet<&str> = self.params.iter().map(|p| p.name.as_str()).collect();

        // Operations.
        let mut op_ids = BTreeSet::new();
        for op in &self.operations {
            let at = |what: &str| format!("operations.{}.{what}", op.id);
            if !op_ids.insert(op.id.as_str()) {
                push(
                    "operations",
                    crate::text!(
                        "problem-connector-operation-declared-twice",
                        a0 = (op.id).to_string()
                    ),
                );
            }
            if op.name.trim().is_empty() {
                push(
                    &at("name"),
                    crate::text!("problem-connector-operation-needs-name"),
                );
            }
            if op.description.trim().is_empty() {
                push(
                    &at("description"),
                    crate::text!("problem-connector-operation-needs-description"),
                );
            }
            let mut names = BTreeSet::new();
            for p in &op.params {
                if !names.insert(p.name.as_str()) {
                    push(
                        &at("params"),
                        crate::text!(
                            "problem-connector-parameter-declared-twice",
                            a0 = (p.name).to_string()
                        ),
                    );
                }
                if p.label.trim().is_empty() {
                    push(
                        &at("params"),
                        crate::text!(
                            "problem-connector-parameter-needs-label",
                            name = p.name.to_string()
                        ),
                    );
                }
            }
            for name in op.headers.keys() {
                if bisa_netrules::is_reserved_header(name) {
                    push(
                        &at("headers"),
                        crate::text!(
                            "problem-connector-client-owns-header-definition-cannot-set",
                            name = name.to_string()
                        ),
                    );
                }
            }
            if let Some(select) = &op.output.select {
                if select.is_empty() || select.split('.').any(str::is_empty) {
                    push(
                        &at("output.select"),
                        crate::text!("problem-connector-select-dotted-path-with-no-empty-segment"),
                    );
                }
            }
            if let Some(schema) = &op.output.schema {
                if !schema.is_object() {
                    push(
                        &at("output.schema"),
                        crate::text!("problem-connector-output-schema-must-be-json-object"),
                    );
                }
            }
            if let Some(expect) = &op.output.expect {
                let dotted = |p: &str| !p.is_empty() && p.split('.').all(|s| !s.is_empty());
                if !dotted(&expect.path) {
                    push(
                        &at("output.expect.path"),
                        crate::text!("problem-connector-expect-path-dotted"),
                    );
                }
                if expect.equals.is_some() == expect.absent {
                    push(
                        &at("output.expect"),
                        crate::text!("problem-connector-expect-equals-or-absent"),
                    );
                }
                if expect.reason.as_deref().is_some_and(|r| !dotted(r)) {
                    push(
                        &at("output.expect.reason"),
                        crate::text!("problem-connector-expect-reason-dotted"),
                    );
                }
            }
            // How the call holds up: its own deadline, its write key, its pages.
            if let Some(secs) = op.timeout_secs {
                if !(1..=MAX_OPERATION_TIMEOUT_SECS).contains(&secs) {
                    push(
                        &at("timeout_secs"),
                        crate::text!(
                            "problem-connector-deadline-1-seconds",
                            max_operation_timeout_secs = (MAX_OPERATION_TIMEOUT_SECS).to_string()
                        ),
                    );
                }
            }
            if let Some(idem) = &op.idempotency {
                if !op.writes {
                    push(
                        &at("idempotency"),
                        crate::text!(
                            "problem-connector-only-writing-operation-carries-key-read-safe"
                        ),
                    );
                }
                let header = idem.header.trim();
                if header.is_empty()
                    || !header
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                {
                    push(
                        &at("idempotency.header"),
                        crate::text!("problem-connector-header-name-letters-digits-hyphens"),
                    );
                } else if bisa_netrules::is_reserved_header(header) {
                    push(
                        &at("idempotency.header"),
                        crate::text!(
                            "problem-connector-client-owns-header-key-travels-platform-s",
                            header = header.to_string()
                        ),
                    );
                } else if op.headers.keys().any(|k| k.eq_ignore_ascii_case(header)) {
                    push(
                        &at("idempotency.header"),
                        crate::text!(
                            "problem-connector-already-header-operation",
                            header = header.to_string()
                        ),
                    );
                }
            }
            if let Some(paging) = &op.page {
                if op.writes {
                    push(
                        &at("page"),
                        crate::text!("problem-connector-write-does-not-page-only-read-followed"),
                    );
                }
                if op.output.select.is_none() {
                    push(
                        &at("page"),
                        crate::text!(
                            "problem-connector-paged-operation-selects-list-pages-output-select"
                        ),
                    );
                }
                match op.params.iter().find(|p| p.name == paging.cursor_param) {
                    None => push(
                        &at("page.cursor_param"),
                        crate::text!(
                            "problem-connector-not-parameter-operation",
                            a0 = (paging.cursor_param).to_string()
                        ),
                    ),
                    Some(p) if p.kind != ParamKind::Text => push(
                        &at("page.cursor_param"),
                        crate::text!(
                            "problem-connector-must-be-text-parameter-cursor-text",
                            a0 = (paging.cursor_param).to_string()
                        ),
                    ),
                    Some(p) if p.required => push(
                        &at("page.cursor_param"),
                        crate::text!(
                            "problem-connector-must-be-optional-first-page-has-no",
                            a0 = (paging.cursor_param).to_string()
                        ),
                    ),
                    Some(_) => {}
                }
                if paging.next_cursor.is_empty() || paging.next_cursor.split('.').any(str::is_empty)
                {
                    push(
                        &at("page.next_cursor"),
                        crate::text!("problem-connector-next-cursor-dotted-path-with-no-empty"),
                    );
                }
                if !(1..=MAX_PAGES).contains(&paging.max_pages) {
                    push(
                        &at("page.max_pages"),
                        crate::text!(
                            "problem-connector-call-reads-1-pages",
                            max_pages = (MAX_PAGES).to_string()
                        ),
                    );
                }
            }
            // The body, and the file parameters only a body may carry.
            let mut carried: BTreeSet<&str> = BTreeSet::new();
            match &op.body {
                None => {}
                Some(OperationBody::Json { value }) if !(value.is_object() || value.is_array()) => {
                    push(
                        &at("body"),
                        crate::text!("problem-connector-json-body-object-or-array"),
                    );
                }
                Some(OperationBody::Json { .. }) => {}
                Some(OperationBody::Form { fields }) => {
                    if fields.is_empty() {
                        push(
                            &at("body"),
                            crate::text!("problem-connector-form-body-needs-field"),
                        );
                    }
                    if fields.keys().any(|k| k.trim().is_empty()) {
                        push(
                            &at("body.fields"),
                            crate::text!("problem-connector-form-field-needs-name"),
                        );
                    }
                }
                Some(OperationBody::Multipart { parts }) => {
                    if parts.is_empty() {
                        push(
                            &at("body"),
                            crate::text!("problem-connector-multipart-body-needs-least-one-part"),
                        );
                    }
                    let mut part_names = BTreeSet::new();
                    for part in parts {
                        let site = at(&format!("body.parts.{}", part.name));
                        if part.name.trim().is_empty() {
                            push(
                                &at("body.parts"),
                                crate::text!("problem-connector-part-needs-name"),
                            );
                        } else if !part_names.insert(part.name.as_str()) {
                            push(
                                &at("body.parts"),
                                crate::text!(
                                    "problem-connector-part-declared-twice",
                                    a0 = (part.name).to_string()
                                ),
                            );
                        }
                        if let Some(ct) = &part.content_type {
                            if !is_media_type(ct) {
                                push(
                                    &site,
                                    crate::text!(
                                        "problem-connector-not-media-type",
                                        content_type = format!("{ct:?}")
                                    ),
                                );
                            }
                        }
                        if let PartSource::File { file } = &part.source {
                            match op.params.iter().find(|p| &p.name == file) {
                                None => push(&site, crate::text!("problem-connector-not-parameter-operation-2", file = file.to_string())),
                                Some(p) if p.kind != ParamKind::File => push(&site, crate::text!("problem-connector-not-file-parameter-text-part-carries-rest", file = file.to_string())),
                                Some(p) => {
                                    carried.insert(p.name.as_str());
                                }
                            }
                        }
                    }
                }
                Some(OperationBody::Raw { content_type, from }) => {
                    if !is_media_type(content_type) {
                        push(
                            &at("body.content_type"),
                            crate::text!(
                                "problem-connector-not-media-type",
                                content_type = format!("{content_type:?}")
                            ),
                        );
                    }
                    match op.params.iter().find(|p| &p.name == from) {
                        None => push(
                            &at("body.from"),
                            crate::text!(
                                "problem-connector-not-parameter-operation-3",
                                from = from.to_string()
                            ),
                        ),
                        Some(p) => {
                            carried.insert(p.name.as_str());
                        }
                    }
                }
            }
            for p in op.params.iter().filter(|p| p.kind == ParamKind::File) {
                if !carried.contains(p.name.as_str()) {
                    push(
                        &at("params"),
                        crate::text!(
                            "problem-connector-file-parameter-carried-no-part-no-raw",
                            a0 = (p.name).to_string()
                        ),
                    );
                }
            }
        }
        if self.operations.is_empty() {
            push(
                "operations",
                crate::text!("problem-connector-declare-one-operation"),
            );
        }

        // Templates: every one parses under the connector grammar and names
        // only what is declared.
        for (site, tmpl) in self.templates() {
            let field = site.to_string();
            let op_params: Option<BTreeMap<&str, ParamKind>> = site.operation().and_then(|op| {
                self.operation(op)
                    .map(|o| o.params.iter().map(|p| (p.name.as_str(), p.kind)).collect())
            });
            match placeholders_in(tmpl, Grammar::Connector) {
                Err(e) => push(&field, e.text()),
                Ok(found) => {
                    for p in found {
                        match p {
                            Placeholder::Account(name) => {
                                if !account_names.contains(name.as_str()) {
                                    push(
                                        &field,
                                        crate::text!(
                                            "problem-connector-account-names-no-parameter",
                                            name = name.to_string()
                                        ),
                                    );
                                }
                            }
                            Placeholder::Param(name) => match &op_params {
                                None => push(
                                    &field,
                                    crate::text!(
                                        "problem-connector-params-account-only",
                                        name = name.to_string(),
                                        field = field.clone()
                                    ),
                                ),
                                Some(kinds) => match kinds.get(name.as_str()) {
                                    None => push(
                                        &field,
                                        crate::text!(
                                            "problem-connector-params-names-no-parameter",
                                            name = name.to_string()
                                        ),
                                    ),
                                    Some(ParamKind::File) => push(
                                        &field,
                                        crate::text!(
                                            "problem-connector-params-file-in-template",
                                            name = name.to_string()
                                        ),
                                    ),
                                    Some(_) => {}
                                },
                            },
                            _ => push(
                                &field,
                                crate::text!("problem-connector-reads-params-and-account-only"),
                            ),
                        }
                    }
                }
            }
        }

        // Auth.
        if let AuthScheme::OAuth2 {
            authorization_url,
            token_url,
            client_id_param,
            ..
        } = &self.auth
        {
            for (field, url) in [
                ("auth.authorization_url", authorization_url),
                ("auth.token_url", token_url),
            ] {
                match url_head(url) {
                    Some(head)
                        if head.scheme == "https"
                            || (head.scheme == "http" && is_loopback(head.authority)) => {}
                    _ => push(
                        field,
                        crate::text!("problem-connector-oauth-url-https-http-loopback-host"),
                    ),
                }
            }
            if client_id_param.is_empty()
                || !client_id_param
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.')
            {
                push(
                    "auth.client_id_param",
                    crate::text!("problem-connector-client-id-param-name"),
                );
            }
        }
        if let AuthScheme::Jwt {
            claims,
            header,
            ttl_secs,
            ..
        } = &self.auth
        {
            if claims.is_empty() {
                push(
                    "auth.claims",
                    crate::text!("problem-connector-signed-token-needs-least-one-claim-iss"),
                );
            }
            for k in ["iat", "exp"] {
                if claims.contains_key(k) {
                    push(
                        "auth.claims",
                        crate::text!(
                            "problem-connector-clock-s-platform-writes-each-request",
                            k = k.to_string()
                        ),
                    );
                }
            }
            for k in ["alg", "typ"] {
                if header.contains_key(k) {
                    push(
                        "auth.header",
                        crate::text!(
                            "problem-connector-signer-s-platform-writes",
                            k = k.to_string()
                        ),
                    );
                }
            }
            if !(1..=MAX_JWT_TTL_SECS).contains(ttl_secs) {
                push(
                    "auth.ttl_secs",
                    crate::text!(
                        "problem-connector-token-lives-1-seconds-default",
                        max_jwt_ttl_secs = (MAX_JWT_TTL_SECS).to_string(),
                        default_jwt_ttl_secs = (DEFAULT_JWT_TTL_SECS).to_string()
                    ),
                );
            }
        }
        if let AuthScheme::ApiKey { place, .. } = &self.auth {
            let name = match place {
                KeyPlace::Header { name } | KeyPlace::Query { name } => name,
            };
            if name.trim().is_empty() {
                push(
                    "auth.place",
                    crate::text!("problem-connector-key-s-header-query-name-empty"),
                );
            }
        }

        // The check operation.
        if let Some(check) = &self.check {
            match self.operation(check) {
                None => push(
                    "check",
                    crate::text!(
                        "problem-connector-not-operation-connector",
                        check = check.to_string()
                    ),
                ),
                Some(op) => {
                    if op.params.iter().any(|p| p.required) {
                        push(
                            "check",
                            crate::text!(
                                "problem-connector-needs-parameters-check-runs-with-none",
                                check = check.to_string()
                            ),
                        );
                    }
                    if op.writes {
                        push(
                            "check",
                            crate::text!(
                                "problem-connector-check-writes",
                                check = check.to_string()
                            ),
                        );
                    }
                }
            }
        }

        // Size.
        if let Ok(bytes) = serde_json::to_vec(self) {
            if bytes.len() > MAX_CONNECTOR_BYTES {
                push(
                    "",
                    crate::text!(
                        "problem-connector-definition-bytes-cap",
                        a0 = (bytes.len()).to_string(),
                        max_connector_bytes = (MAX_CONNECTOR_BYTES).to_string()
                    ),
                );
            }
        }

        out
    }
}

/// One login to a connector's platform, on this machine. Never synced; the
/// secrets are not here.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConnectorAccount {
    pub id: AccountId,
    pub connector: ConnectorId,
    pub label: String,
    /// Values for the connector's `params`, non-secret.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, Value>,
    /// The account a step uses when it names none.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub default: bool,
    #[serde(default)]
    pub auth: AccountAuth,
    pub created_at: u64,
}

/// What is known about an account's credentials without holding one.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AccountAuth {
    /// The secret fields that are set in the keystore.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields_set: Vec<SecretField>,
    /// When an OAuth access token expires, unix seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
    /// The scope the platform granted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

impl ConnectorAccount {
    /// The rules an account record must satisfy against its connector.
    pub fn validate(&self, connector: &Connector) -> Vec<ConnectorProblem> {
        let mut out = Vec::new();
        if self.connector != connector.id {
            out.push(ConnectorProblem::at(
                "connector",
                crate::text!(
                    "problem-connector-account-not",
                    a0 = (self.connector).to_string(),
                    a1 = (connector.id).to_string()
                ),
            ));
        }
        if self.label.trim().is_empty() {
            out.push(ConnectorProblem::at(
                "label",
                crate::text!("problem-connector-account-needs-label"),
            ));
        }
        for key in self.params.keys() {
            if connector.param(key).is_none() {
                out.push(ConnectorProblem::at(
                    "params",
                    crate::text!(
                        "problem-connector-not-parameter",
                        key = key.to_string(),
                        a0 = (connector.id).to_string()
                    ),
                ));
            }
        }
        for p in &connector.params {
            if p.required && !self.params.contains_key(p.name.as_str()) {
                out.push(ConnectorProblem::at(
                    "params",
                    crate::text!("problem-connector-required", a0 = (p.name).to_string()),
                ));
            }
        }
        for f in &self.auth.fields_set {
            if !connector.auth.fields().contains(f) {
                out.push(ConnectorProblem::at(
                    "auth",
                    crate::text!(
                        "problem-connector-not-field-scheme",
                        f = f.to_string(),
                        a0 = (connector.auth.word()).to_string()
                    ),
                ));
            }
        }
        out
    }

    /// Whether every field the scheme requires is set.
    pub fn is_connected(&self, auth: &AuthScheme) -> bool {
        match auth {
            AuthScheme::OAuth2 { .. } => self.auth.fields_set.contains(&SecretField::AccessToken),
            other => other
                .required()
                .iter()
                .all(|f| self.auth.fields_set.contains(f)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn param(name: &str, kind: ParamKind, required: bool) -> ParamDef {
        ParamDef {
            name: InputName::new(name).unwrap(),
            label: name.to_uppercase(),
            kind,
            required,
            doc: String::new(),
        }
    }

    fn op(id: &str) -> Operation {
        Operation {
            id: OperationId::new(id).unwrap(),
            name: id.to_uppercase(),
            description: format!("does {id}"),
            method: HttpMethod::Get,
            path: "/things".into(),
            query: BTreeMap::new(),
            headers: BTreeMap::new(),
            body: None,
            params: vec![],
            output: OutputSpec::default(),
            writes: false,
            timeout_secs: None,
            idempotency: None,
            page: None,
        }
    }

    fn connector() -> Connector {
        Connector {
            id: ConnectorId::new("acme").unwrap(),
            name: "Acme".into(),
            description: "Acme's API".into(),
            tags: Tags::default(),
            origin: Origin::Local,
            base_url: "https://api.acme.example".into(),
            hosts: vec!["api.acme.example".into()],
            insecure_tls: false,
            auth: AuthScheme::Bearer,
            params: vec![],
            operations: vec![op("list")],
            check: None,
            revision: 0,
            created_at: 0,
        }
    }

    fn fields(problems: &[ConnectorProblem]) -> Vec<&str> {
        problems
            .iter()
            .map(|p| p.field.as_deref().unwrap_or(""))
            .collect()
    }

    #[test]
    fn a_well_formed_definition_has_no_problems_and_round_trips() {
        let c = connector();
        assert_eq!(c.validate(), vec![]);
        let json = serde_json::to_string(&c).unwrap();
        assert_eq!(serde_json::from_str::<Connector>(&json).unwrap(), c);
        let toml_text = toml::to_string(&c).unwrap();
        assert_eq!(toml::from_str::<Connector>(&toml_text).unwrap(), c);
        assert!(json.contains("\"scheme\":\"bearer\""), "{json}");
        assert!(
            !json.contains("insecure_tls"),
            "a false flag is not written"
        );
    }

    #[test]
    fn an_operation_may_say_its_deadline_its_key_and_its_pages() {
        let mut c = connector();
        let mut read = op("list");
        read.timeout_secs = Some(20);
        read.output.select = Some("items".into());
        read.params = vec![ParamDef {
            name: ParamName::new("cursor").unwrap(),
            label: "Cursor".into(),
            kind: ParamKind::Text,
            required: false,
            doc: "Where the page before ended.".into(),
        }];
        read.query = BTreeMap::from([("cursor".to_string(), "{params.cursor}".to_string())]);
        read.page = Some(Paging {
            cursor_param: ParamName::new("cursor").unwrap(),
            next_cursor: "meta.next".into(),
            max_pages: MAX_PAGES,
        });
        let mut write = op("create");
        write.method = HttpMethod::Post;
        write.writes = true;
        write.idempotency = Some(Idempotency {
            header: "Idempotency-Key".into(),
        });
        c.operations = vec![read.clone(), write.clone()];
        assert_eq!(
            c.validate(),
            vec![],
            "a deadline, a key on a write, pages on a read"
        );
        let wire = serde_json::to_value(&c).unwrap();
        assert_eq!(wire["operations"][0]["page"]["max_pages"], MAX_PAGES);
        assert_eq!(
            wire["operations"][1]["idempotency"]["header"],
            "Idempotency-Key"
        );
        assert!(
            wire["operations"][1].get("page").is_none(),
            "an absent field is absent on the wire"
        );
        let back: Connector = serde_json::from_value(wire).unwrap();
        assert_eq!(back.operations[0].timeout_secs, Some(20));

        // The deadline has bounds.
        let mut c2 = c.clone();
        c2.operations[0].timeout_secs = Some(0);
        assert!(fields(&c2.validate()).contains(&"operations.list.timeout_secs"));
        c2.operations[0].timeout_secs = Some(MAX_OPERATION_TIMEOUT_SECS + 1);
        assert!(fields(&c2.validate()).contains(&"operations.list.timeout_secs"));

        // A key belongs to a write, in a header of the platform's own.
        let mut c3 = c.clone();
        c3.operations[0].idempotency = Some(Idempotency {
            header: "Idempotency-Key".into(),
        });
        assert!(
            fields(&c3.validate()).contains(&"operations.list.idempotency"),
            "a read carries no key"
        );
        let mut c4 = c.clone();
        c4.operations[1].idempotency = Some(Idempotency {
            header: "Authorization".into(),
        });
        assert!(
            fields(&c4.validate()).contains(&"operations.create.idempotency.header"),
            "never a reserved header"
        );
        c4.operations[1].idempotency = Some(Idempotency {
            header: "X Key".into(),
        });
        assert!(
            fields(&c4.validate()).contains(&"operations.create.idempotency.header"),
            "a header name is one word"
        );
        c4.operations[1].headers =
            BTreeMap::from([("idempotency-key".to_string(), "fixed".to_string())]);
        c4.operations[1].idempotency = Some(Idempotency {
            header: "Idempotency-Key".into(),
        });
        assert!(
            fields(&c4.validate()).contains(&"operations.create.idempotency.header"),
            "not a header the definition already sets"
        );

        // Pages: a read that selects a list, with an optional text cursor.
        let mut c5 = c.clone();
        c5.operations[1].page = c5.operations[0].page.clone();
        assert!(
            fields(&c5.validate()).contains(&"operations.create.page"),
            "a write does not page"
        );
        let mut c6 = c.clone();
        c6.operations[0].output.select = None;
        assert!(
            fields(&c6.validate()).contains(&"operations.list.page"),
            "a paged read selects its list"
        );
        let mut c7 = c.clone();
        c7.operations[0].params[0].required = true;
        assert!(
            fields(&c7.validate()).contains(&"operations.list.page.cursor_param"),
            "the first page has no cursor"
        );
        c7.operations[0].params[0].required = false;
        c7.operations[0].params[0].kind = ParamKind::Number;
        assert!(
            fields(&c7.validate()).contains(&"operations.list.page.cursor_param"),
            "a cursor is text"
        );
        let mut c8 = c.clone();
        c8.operations[0].page.as_mut().unwrap().cursor_param = ParamName::new("nope").unwrap();
        assert!(fields(&c8.validate()).contains(&"operations.list.page.cursor_param"));
        c8.operations[0].page.as_mut().unwrap().cursor_param = ParamName::new("cursor").unwrap();
        c8.operations[0].page.as_mut().unwrap().max_pages = 0;
        assert!(fields(&c8.validate()).contains(&"operations.list.page.max_pages"));
        c8.operations[0].page.as_mut().unwrap().max_pages = MAX_PAGES + 1;
        assert!(fields(&c8.validate()).contains(&"operations.list.page.max_pages"));
        c8.operations[0].page.as_mut().unwrap().max_pages = 2;
        c8.operations[0].page.as_mut().unwrap().next_cursor = "a..b".into();
        assert!(fields(&c8.validate()).contains(&"operations.list.page.next_cursor"));
    }

    #[test]
    fn a_misspelled_key_is_refused() {
        let mut v = serde_json::to_value(connector()).unwrap();
        v["hots"] = json!(["x"]);
        let err = serde_json::from_value::<Connector>(v)
            .unwrap_err()
            .to_string();
        assert!(err.contains("hots"), "{err}");
    }

    #[test]
    fn names_hosts_and_operations_are_required() {
        let mut c = connector();
        c.name = " ".into();
        c.description = String::new();
        c.hosts = vec![];
        c.operations = vec![];
        let f = c.validate();
        assert_eq!(
            fields(&f),
            vec!["name", "description", "hosts", "base_url", "operations"]
        );
    }

    #[test]
    fn hosts_follow_the_pattern_grammar_and_the_base_host_is_one_of_them() {
        let mut c = connector();
        c.hosts = vec!["api.other.example".into(), "*.".into(), "a b".into()];
        let f = c.validate();
        assert_eq!(
            f.iter()
                .filter(|p| p.field.as_deref() == Some("hosts"))
                .count(),
            2,
            "{f:?}"
        );
        assert!(f.iter().any(|p| p.field.as_deref() == Some("base_url")
            && p.text.to_string().contains("not in hosts")));
        // Case is not the grammar's business (`bisa-netrules`): a host written
        // in capitals is the same host, and it covers the base host.
        let mut c = connector();
        c.hosts = vec!["API.acme.example".into()];
        assert_eq!(c.validate(), vec![], "{:?}", c.validate());
        let mut c = connector();
        c.hosts = vec!["*.acme.example".into()];
        assert_eq!(c.validate(), vec![], "a wildcard covers the base host");
        let mut c = connector();
        c.hosts = vec!["acme.example".into()];
        assert!(
            !c.validate().is_empty(),
            "a wildcard's suffix is not itself a match"
        );
    }

    #[test]
    fn the_base_url_is_https_or_loopback_http_with_no_user_info() {
        let mut c = connector();
        c.base_url = "http://api.acme.example".into();
        assert!(c
            .validate()
            .iter()
            .any(|p| p.text.to_string().contains("loopback host only")));
        c.base_url = "http://127.0.0.1:27124".into();
        c.hosts = vec!["127.0.0.1:27124".into()];
        assert_eq!(c.validate(), vec![]);
        c.base_url = "https://me:pw@api.acme.example".into();
        c.hosts = vec!["api.acme.example".into()];
        assert!(c
            .validate()
            .iter()
            .any(|p| p.text.to_string().contains("no user info")));
        c.base_url = "ftp://api.acme.example".into();
        assert!(c
            .validate()
            .iter()
            .any(|p| p.text.to_string().contains("not http or https")));
        c.base_url = "api.acme.example".into();
        assert!(c
            .validate()
            .iter()
            .any(|p| p.field.as_deref() == Some("base_url")));
    }

    #[test]
    fn an_unsigned_certificate_is_accepted_on_loopback_only() {
        let mut c = connector();
        c.insecure_tls = true;
        assert!(c
            .validate()
            .iter()
            .any(|p| p.field.as_deref() == Some("insecure_tls")));
        c.base_url = "https://localhost:27124".into();
        c.hosts = vec!["localhost:27124".into()];
        assert_eq!(c.validate(), vec![]);
    }

    #[test]
    fn a_templated_base_url_reads_account_parameters_only() {
        let mut c = connector();
        c.base_url = "https://{account.site}.atlassian.net".into();
        c.hosts = vec!["*.atlassian.net".into()];
        c.params = vec![param("site", ParamKind::Text, true)];
        assert_eq!(c.validate(), vec![]);
        c.params = vec![];
        assert!(c.validate().iter().any(|p| p
            .text
            .to_string()
            .contains("{account.site} names no connector parameter")));
        c.base_url = "https://{params.site}.atlassian.net".into();
        assert!(c
            .validate()
            .iter()
            .any(|p| p.text.to_string().contains("account parameters only")));
        c.base_url = "https://{inputs.site}.atlassian.net".into();
        assert!(c.validate().iter().any(|p| p
            .text
            .to_string()
            .contains("params.<name> and account.<name> only")));
    }

    #[test]
    fn operation_templates_name_declared_parameters_only() {
        let mut c = connector();
        let mut o = op("search");
        o.path = "/search/{params.q}".into();
        o.query.insert("max".into(), "{params.max}".into());
        o.headers.insert("X-Site".into(), "{account.site}".into());
        o.body = Some(OperationBody::Json {
            value: json!({"jql": "{params.q}", "nested": [{"n": "{params.n}"}]}),
        });
        o.params = vec![
            param("q", ParamKind::Text, true),
            param("max", ParamKind::Number, false),
        ];
        c.params = vec![param("site", ParamKind::Text, true)];
        c.operations = vec![o];
        let f = c.validate();
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].field.as_deref(), Some("operations.search.body"));
        assert!(f[0]
            .text
            .to_string()
            .contains("{params.n} names no parameter"));
        assert_eq!(
            c.templates().len(),
            6,
            "base, path, query, header, two body leaves ({{params.q}} and the nested {{params.n}})"
        );
    }

    #[test]
    fn ids_and_parameter_names_are_unique_and_reserved_headers_are_refused() {
        let mut c = connector();
        let mut o = op("list");
        o.params = vec![
            param("q", ParamKind::Text, false),
            param("q", ParamKind::Json, true),
        ];
        o.headers.insert("Authorization".into(), "x".into());
        c.operations = vec![o, op("list")];
        c.params = vec![
            param("site", ParamKind::Text, false),
            param("site", ParamKind::Text, false),
        ];
        let f = c.validate();
        assert!(f.iter().any(|p| p
            .text
            .to_string()
            .contains("operation `list` is declared twice")));
        assert!(f
            .iter()
            .any(|p| p.field.as_deref() == Some("operations.list.params")
                && p.text.to_string().contains("`q` is declared twice")));
        assert!(f.iter().any(|p| p.field.as_deref() == Some("params")
            && p.text.to_string().contains("`site` is declared twice")));
        assert!(f
            .iter()
            .any(|p| p.text.to_string().contains("Authorization header")));
    }

    #[test]
    fn select_schema_body_and_check_are_shaped() {
        let mut c = connector();
        let mut o = op("list");
        o.output.select = Some("items..0".into());
        o.output.schema = Some(json!([]));
        o.body = Some(OperationBody::Json {
            value: json!("text"),
        });
        o.params = vec![param("q", ParamKind::Text, true)];
        c.operations = vec![o];
        c.check = Some(OperationId::new("list").unwrap());
        let f = c.validate();
        assert!(f
            .iter()
            .any(|p| p.field.as_deref() == Some("operations.list.output.select")));
        assert!(f
            .iter()
            .any(|p| p.field.as_deref() == Some("operations.list.output.schema")));
        assert!(f
            .iter()
            .any(|p| p.field.as_deref() == Some("operations.list.body")));
        assert!(f.iter().any(|p| p.field.as_deref() == Some("check")
            && p.text.to_string().contains("needs parameters")));
        c.check = Some(OperationId::new("nope").unwrap());
        assert!(c
            .validate()
            .iter()
            .any(|p| p.text.to_string().contains("`nope` is not an operation")));
        let mut writes = op("post");
        writes.writes = true;
        c.operations = vec![writes];
        c.check = Some(OperationId::new("post").unwrap());
        assert!(c
            .validate()
            .iter()
            .any(|p| p.text.to_string().contains("`post` writes")));
    }

    #[test]
    fn oauth_urls_are_https_and_the_scheme_knows_its_fields() {
        let mut c = connector();
        c.auth = AuthScheme::OAuth2 {
            authorization_url: "http://auth.acme.example/o".into(),
            token_url: "https://auth.acme.example/t".into(),
            scopes: vec!["read".into()],
            pkce: true,
            extra: BTreeMap::new(),
            client_id_param: DEFAULT_CLIENT_ID_PARAM.into(),
            scope_join: ScopeJoin::Space,
            code_challenge: ChallengeEncoding::Base64url,
        };
        let f = c.validate();
        assert_eq!(fields(&f), vec!["auth.authorization_url"]);
        assert_eq!(c.auth.required(), &[SecretField::ClientId]);
        assert_eq!(c.auth.fields().len(), 4);
        assert_eq!(c.auth.word(), "oauth2");
        assert_eq!(
            AuthScheme::Basic.required(),
            &[SecretField::Username, SecretField::Password]
        );
        assert_eq!(AuthScheme::None.fields(), &[] as &[SecretField]);
        assert!(!AuthScheme::None.needs_account());
        let json = serde_json::to_value(&c.auth).unwrap();
        assert_eq!(json["scheme"], "oauth2");
        assert_eq!(json["pkce"], true);
        assert!(
            json.get("client_id_param").is_none()
                && json.get("scope_join").is_none()
                && json.get("code_challenge").is_none(),
            "the RFC's own words are not written: {json}"
        );
        let key = AuthScheme::ApiKey {
            place: KeyPlace::Query {
                name: String::new(),
            },
            prefix: None,
        };
        c.auth = key;
        assert!(c
            .validate()
            .iter()
            .any(|p| p.field.as_deref() == Some("auth.place")));
        for f in SecretField::ALL {
            assert_eq!(SecretField::parse(f.as_str()), Some(f));
        }
    }

    #[test]
    fn a_body_is_one_of_four_kinds_and_a_file_travels_only_in_a_part_or_a_raw_body() {
        let mut c = connector();
        let mut o = op("upload");
        o.method = HttpMethod::Post;
        o.params = vec![
            param("meta", ParamKind::Json, true),
            param("video", ParamKind::File, true),
        ];
        o.body = Some(OperationBody::Multipart {
            parts: vec![
                Part {
                    name: "metadata".into(),
                    source: PartSource::Text {
                        text: "{params.meta}".into(),
                    },
                    filename: None,
                    content_type: Some("application/json".into()),
                },
                Part {
                    name: "media".into(),
                    source: PartSource::File {
                        file: InputName::new("video").unwrap(),
                    },
                    filename: Some("clip.mp4".into()),
                    content_type: Some("video/mp4".into()),
                },
            ],
        });
        c.operations = vec![o.clone()];
        assert_eq!(c.validate(), vec![], "a well-formed multipart body");
        let json = serde_json::to_value(&c.operations[0].body).unwrap();
        assert_eq!(json["kind"], "multipart");
        assert_eq!(json["parts"][1]["file"], "video");
        let back: Connector = serde_json::from_value(serde_json::to_value(&c).unwrap()).unwrap();
        assert_eq!(back, c, "the tagged body round-trips");

        // A file parameter nobody carries, a text parameter in a file part,
        // a file in a template, a part named twice, a bad media type.
        let mut bad = o.clone();
        bad.params.push(param("thumb", ParamKind::File, false));
        bad.path = "/upload/{params.video}".into();
        bad.body = Some(OperationBody::Multipart {
            parts: vec![
                Part {
                    name: "media".into(),
                    source: PartSource::File {
                        file: InputName::new("meta").unwrap(),
                    },
                    filename: None,
                    content_type: Some("not a type".into()),
                },
                Part {
                    name: "media".into(),
                    source: PartSource::Text { text: "x".into() },
                    filename: None,
                    content_type: None,
                },
            ],
        });
        c.operations = vec![bad];
        let f = c.validate();
        let messages: Vec<String> = f.iter().map(|p| p.text.to_string()).collect();
        assert!(
            messages
                .iter()
                .any(|m| m.contains("`meta` is not a file parameter")),
            "{messages:?}"
        );
        assert!(messages
            .iter()
            .any(|m| m.contains("part `media` is declared twice")));
        assert!(messages.iter().any(|m| m.contains("is not a media type")));
        assert!(messages
            .iter()
            .any(|m| m.contains("file parameter `video` is carried by no part")));
        assert!(messages
            .iter()
            .any(|m| m.contains("file parameter `thumb` is carried by no part")));
        assert!(
            messages
                .iter()
                .any(|m| m.contains("{params.video} is a file; its bytes travel")),
            "{messages:?}"
        );

        // A raw body from a file parameter; a form body with fields.
        let mut raw = op("put");
        raw.method = HttpMethod::Put;
        raw.params = vec![param("blob", ParamKind::File, true)];
        raw.body = Some(OperationBody::Raw {
            content_type: "application/octet-stream".into(),
            from: InputName::new("blob").unwrap(),
        });
        let mut form = op("token");
        form.method = HttpMethod::Post;
        form.params = vec![param("code", ParamKind::Text, true)];
        form.body = Some(OperationBody::Form {
            fields: BTreeMap::from([
                ("grant_type".to_string(), "authorization_code".to_string()),
                ("code".to_string(), "{params.code}".to_string()),
            ]),
        });
        c.operations = vec![raw.clone(), form.clone()];
        assert_eq!(c.validate(), vec![]);
        raw.body = Some(OperationBody::Raw {
            content_type: "octet".into(),
            from: InputName::new("nope").unwrap(),
        });
        form.body = Some(OperationBody::Form {
            fields: BTreeMap::new(),
        });
        c.operations = vec![raw, form];
        let f = c.validate();
        assert_eq!(
            fields(&f),
            vec![
                "operations.put.body.content_type",
                "operations.put.body.from",
                "operations.put.params",
                "operations.token.body",
            ]
        );

        // An account parameter is never a file.
        c.operations = vec![op("list")];
        c.params = vec![param("key", ParamKind::File, false)];
        assert!(c.validate().iter().any(|p| p
            .text
            .to_string()
            .contains("an account parameter cannot be a file")));
        assert_eq!(ParamKind::File.as_str(), "file");
    }

    #[test]
    fn a_jwt_scheme_signs_with_a_private_key_and_its_claims_read_the_account() {
        let mut c = connector();
        c.params = vec![
            param("issuer", ParamKind::Text, true),
            param("kid", ParamKind::Text, true),
        ];
        c.auth = AuthScheme::Jwt {
            alg: JwtAlg::Es256,
            claims: BTreeMap::from([
                ("iss".to_string(), "{account.issuer}".to_string()),
                ("aud".to_string(), "api".to_string()),
            ]),
            header: BTreeMap::from([("kid".to_string(), "{account.kid}".to_string())]),
            ttl_secs: 1200,
        };
        assert_eq!(c.validate(), vec![]);
        assert_eq!(c.auth.fields(), &[SecretField::PrivateKey]);
        assert_eq!(c.auth.required(), &[SecretField::PrivateKey]);
        assert_eq!(c.auth.word(), "jwt");
        assert!(c.auth.needs_account());
        let json = serde_json::to_value(&c.auth).unwrap();
        assert_eq!(json["scheme"], "jwt");
        assert_eq!(json["alg"], "ES256");
        assert_eq!(
            SecretField::parse("private_key"),
            Some(SecretField::PrivateKey)
        );
        assert_eq!(SecretField::ALL.len(), 9);
        let default: AuthScheme = serde_json::from_value(
            json!({"scheme": "jwt", "alg": "RS256", "claims": {"iss": "x"}}),
        )
        .unwrap();
        assert!(
            matches!(default, AuthScheme::Jwt { ttl_secs, alg: JwtAlg::Rs256, .. } if ttl_secs == DEFAULT_JWT_TTL_SECS)
        );

        c.auth = AuthScheme::Jwt {
            alg: JwtAlg::Rs256,
            claims: BTreeMap::from([
                ("exp".to_string(), "1".to_string()),
                ("iss".to_string(), "{account.nope}".to_string()),
                ("sub".to_string(), "{params.who}".to_string()),
            ]),
            header: BTreeMap::from([("alg".to_string(), "none".to_string())]),
            ttl_secs: 0,
        };
        let f = c.validate();
        let messages: Vec<String> = f.iter().map(|p| p.text.to_string()).collect();
        assert!(
            messages.iter().any(|m| m.contains("`exp` is the clock's")),
            "{messages:?}"
        );
        assert!(messages.iter().any(|m| m.contains("`alg` is the signer's")));
        assert!(messages
            .iter()
            .any(|m| m.contains("a token lives 1 to 86400 seconds")));
        assert!(messages
            .iter()
            .any(|m| m.contains("{account.nope} names no connector parameter")));
        assert!(messages
            .iter()
            .any(|m| m.contains("auth.claims.sub may read account parameters only")));
        assert!(f
            .iter()
            .any(|p| p.field.as_deref() == Some("auth.claims.sub")));
        c.auth = AuthScheme::Jwt {
            alg: JwtAlg::Es256,
            claims: BTreeMap::new(),
            header: BTreeMap::new(),
            ttl_secs: 60,
        };
        assert!(c
            .validate()
            .iter()
            .any(|p| p.text.to_string().contains("needs at least one claim")));
    }

    #[test]
    fn a_definition_past_the_cap_is_refused() {
        let mut c = connector();
        c.description = "x".repeat(MAX_CONNECTOR_BYTES + 1);
        assert!(c
            .validate()
            .iter()
            .any(|p| p.text.to_string().contains("the cap is")));
    }

    #[test]
    fn an_account_is_judged_against_its_connector() {
        let mut c = connector();
        c.params = vec![param("site", ParamKind::Text, true)];
        let account = ConnectorAccount {
            id: AccountId::from_ulid(ulid::Ulid::from_parts(1, 1)),
            connector: ConnectorId::new("acme").unwrap(),
            label: "work".into(),
            params: BTreeMap::from([("site".to_string(), json!("acme"))]),
            default: true,
            auth: AccountAuth {
                fields_set: vec![SecretField::Token],
                expires_at: None,
                scope: None,
            },
            created_at: 0,
        };
        assert_eq!(account.validate(&c), vec![]);
        assert!(account.is_connected(&c.auth));
        let mut bad = account.clone();
        bad.label = String::new();
        bad.params = BTreeMap::from([("zone".to_string(), json!("eu"))]);
        bad.auth.fields_set = vec![SecretField::ApiKey];
        let f = bad.validate(&c);
        assert_eq!(fields(&f), vec!["label", "params", "params", "auth"]);
        assert!(!bad.is_connected(&c.auth));
        let json = serde_json::to_string(&account).unwrap();
        assert!(
            !json.contains("token\":"),
            "a record names fields, never values: {json}"
        );
        assert_eq!(
            serde_json::from_str::<ConnectorAccount>(&json).unwrap(),
            account
        );
    }

    #[test]
    fn an_oauth_scheme_may_spell_its_dialect_and_declares_its_own_hosts() {
        let mut c = connector();
        assert!(c.oauth_hosts().is_empty(), "a bearer scheme has none");
        assert_eq!(c.declared_hosts(), c.hosts);
        let tiktok: AuthScheme = serde_json::from_value(json!({
            "scheme": "oauth2",
            "authorization_url": "https://www.tiktok.com/v2/auth/authorize/",
            "token_url": "https://open.tiktokapis.com/v2/oauth/token/",
            "scopes": ["user.info.basic", "video.list"],
            "client_id_param": "client_key",
            "scope_join": "comma",
            "code_challenge": "hex"
        }))
        .unwrap();
        c.auth = tiktok.clone();
        assert_eq!(c.validate(), vec![]);
        assert_eq!(
            c.oauth_hosts(),
            vec![
                "www.tiktok.com".to_string(),
                "open.tiktokapis.com".to_string()
            ]
        );
        assert_eq!(
            c.declared_hosts(),
            vec![
                "api.acme.example".to_string(),
                "www.tiktok.com".to_string(),
                "open.tiktokapis.com".to_string()
            ]
        );
        let json = serde_json::to_value(&tiktok).unwrap();
        assert_eq!(json["client_id_param"], "client_key");
        assert_eq!(json["scope_join"], "comma");
        assert_eq!(json["code_challenge"], "hex");
        assert_eq!(ScopeJoin::Comma.separator(), ",");
        if let AuthScheme::OAuth2 {
            client_id_param, ..
        } = &mut c.auth
        {
            *client_id_param = "client id".into();
        }
        assert_eq!(fields(&c.validate()), vec!["auth.client_id_param"]);
        // Two URLs on one host declare it once; a token host already in
        // `hosts` is not declared twice.
        let mut same = connector();
        same.auth = serde_json::from_value(json!({
            "scheme": "oauth2",
            "authorization_url": "https://api.acme.example/o",
            "token_url": "https://api.acme.example/t"
        }))
        .unwrap();
        assert_eq!(same.oauth_hosts(), vec!["api.acme.example".to_string()]);
        assert_eq!(same.declared_hosts(), vec!["api.acme.example".to_string()]);
    }

    #[test]
    fn an_expectation_is_one_dotted_path_with_one_rule() {
        let mut c = connector();
        let mut o = op("list");
        o.output.expect = Some(Expect {
            path: "ok".into(),
            equals: Some(json!(true)),
            absent: false,
            reason: Some("error".into()),
        });
        c.operations = vec![o.clone()];
        assert_eq!(c.validate(), vec![]);
        let json = serde_json::to_value(&c.operations[0].output).unwrap();
        assert_eq!(
            json,
            json!({"expect": {"path": "ok", "equals": true, "reason": "error"}})
        );
        let absent: Expect =
            serde_json::from_value(json!({"path": "errors", "absent": true})).unwrap();
        assert!(absent.absent && absent.equals.is_none());
        // Both rules, or neither, is no expectation; an empty segment is no path.
        o.output.expect = Some(Expect {
            path: "a..b".into(),
            equals: Some(json!(1)),
            absent: true,
            reason: Some(String::new()),
        });
        c.operations = vec![o];
        let f = c.validate();
        assert_eq!(
            fields(&f),
            vec![
                "operations.list.output.expect.path",
                "operations.list.output.expect",
                "operations.list.output.expect.reason"
            ]
        );
        assert!(serde_json::from_value::<Expect>(json!({"path": "ok", "nope": 1})).is_err());
    }

    #[test]
    fn a_path_parameter_is_a_kind_and_the_revision_is_the_catalogs_stamp() {
        let mut c = connector();
        let mut o = op("read");
        o.path = "/vault/{params.path}".into();
        o.params = vec![param("path", ParamKind::Path, true)];
        c.operations = vec![o];
        assert_eq!(c.validate(), vec![]);
        assert_eq!(ParamKind::Path.as_str(), "path");
        assert_eq!(
            serde_json::to_value(ParamKind::Path).unwrap(),
            json!("path")
        );
        let json = serde_json::to_string(&c).unwrap();
        assert!(!json.contains("revision"), "a person's own has no revision");
        c.revision = 3;
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"revision\":3"), "{json}");
        assert_eq!(serde_json::from_str::<Connector>(&json).unwrap(), c);
    }

    #[test]
    fn url_heads_and_loopbacks_are_read_by_hand() {
        assert_eq!(
            url_head("https://a.b:8443/x?y").unwrap(),
            UrlHead {
                scheme: "https",
                authority: "a.b:8443"
            }
        );
        assert_eq!(url_head("http://[::1]:9/").unwrap().authority, "[::1]:9");
        assert!(url_head("nope").is_none());
        assert!(url_head("https://u@h").is_none());
        assert!(url_head("://h").is_none());
    }
}
