//! A stub HTTP server on the loopback interface — canned answers matched by
//! method and path, consumed in order, and a record of every request: method,
//! path, query, headers, the body as JSON and as text — plus the fakes a
//! `Client` is built from: a map of credentials, a clock that stands still,
//! entropy that counts, and a scripted transport for the tests that never
//! need a socket. Nothing leaves the machine.

#![allow(dead_code)]

use async_trait::async_trait;
use axum::body::Bytes;
use axum::extract::{Request as AxumRequest, State};
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response as AxumResponse};
use axum::routing::any;
use axum::Router;
use bisa_connectors::creds::{AccountRef, Clock, Credentials, Entropy, Field, Stored, TokenSet};
use bisa_connectors::files::{FileData, Files};
use bisa_connectors::http::{HttpTransport, Request, Response, TransportError};
use bisa_connectors::spec::{AuthSpec, CallSpec, Method, ParamKind, ParamSpec};
use bisa_connectors::{Client, ConnectorError, ReqwestTransport, Secret};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// One answer the stub gives, once, to the first matching request.
#[derive(Clone, Debug)]
pub struct Canned {
    pub method: String,
    pub path: String,
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Option<serde_json::Value>,
    pub text: Option<String>,
}

impl Canned {
    pub fn json(method: &str, path: &str, status: u16, body: serde_json::Value) -> Self {
        Self {
            method: method.to_string(),
            path: path.to_string(),
            status,
            headers: Vec::new(),
            body: Some(body),
            text: None,
        }
    }

    pub fn text(method: &str, path: &str, status: u16, text: &str) -> Self {
        Self {
            method: method.to_string(),
            path: path.to_string(),
            status,
            headers: Vec::new(),
            body: None,
            text: Some(text.to_string()),
        }
    }

    pub fn empty(method: &str, path: &str, status: u16) -> Self {
        Self {
            method: method.to_string(),
            path: path.to_string(),
            status,
            headers: Vec::new(),
            body: None,
            text: None,
        }
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }
}

/// What the stub saw.
#[derive(Clone, Debug)]
pub struct Call {
    pub method: String,
    pub path: String,
    pub query: Option<String>,
    pub body: serde_json::Value,
    pub text: String,
    pub headers: Vec<(String, String)>,
    pub authorization: Option<String>,
}

impl Call {
    pub fn header(&self, name: &str) -> Option<String> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
    }

    /// The form-encoded body as pairs.
    pub fn form(&self) -> BTreeMap<String, String> {
        url::form_urlencoded::parse(self.text.as_bytes())
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect()
    }
}

#[derive(Clone)]
struct Shared {
    answers: Arc<Mutex<VecDeque<Canned>>>,
    calls: Arc<Mutex<Vec<Call>>>,
}

pub struct Stub {
    base_url: String,
    host: String,
    shared: Shared,
}

impl Stub {
    pub async fn start(answers: Vec<Canned>) -> Self {
        let shared = Shared {
            answers: Arc::new(Mutex::new(answers.into_iter().collect())),
            calls: Arc::new(Mutex::new(Vec::new())),
        };
        let app = Router::new()
            .fallback(any(handle))
            .with_state(shared.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _served = axum::serve(listener, app).await;
        });
        Self {
            base_url: format!("http://{addr}"),
            host: addr.to_string(),
            shared,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// `127.0.0.1:<port>`, the entry a definition declares.
    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn calls(&self) -> Vec<Call> {
        self.shared.calls.lock().unwrap().clone()
    }
}

async fn handle(State(shared): State<Shared>, req: AxumRequest) -> AxumResponse {
    let (parts, body) = req.into_parts();
    let bytes: Bytes = axum::body::to_bytes(body, 1 << 20)
        .await
        .unwrap_or_default();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let body = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    let method = parts.method.to_string();
    let path = parts.uri.path().to_string();
    let authorization = parts
        .headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    shared.calls.lock().unwrap().push(Call {
        method: method.clone(),
        path: path.clone(),
        query: parts.uri.query().map(str::to_string),
        body,
        text,
        headers: parts
            .headers
            .iter()
            .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_string())))
            .collect(),
        authorization,
    });
    let canned = {
        let mut answers = shared.answers.lock().unwrap();
        let at = answers
            .iter()
            .position(|a| a.method.eq_ignore_ascii_case(&method) && a.path == path);
        at.and_then(|i| answers.remove(i))
    };
    match canned {
        Some(c) => {
            let status = StatusCode::from_u16(c.status).unwrap();
            let mut resp = match (c.body, c.text) {
                (Some(body), _) => (status, axum::Json(body)).into_response(),
                (None, Some(text)) => (status, text).into_response(),
                (None, None) => status.into_response(),
            };
            for (k, v) in c.headers {
                resp.headers_mut().insert(
                    HeaderName::from_bytes(k.as_bytes()).unwrap(),
                    HeaderValue::from_str(&v).unwrap(),
                );
            }
            resp
        }
        None => (
            StatusCode::NOT_IMPLEMENTED,
            axum::Json(serde_json::json!({"message": format!("the stub has no answer for {method} {path}")})),
        )
            .into_response(),
    }
}

/// Credentials in a map, with a count of every save.
#[derive(Default)]
pub struct MemoryCreds {
    pub stored: Mutex<BTreeMap<String, Stored>>,
    pub saved: Mutex<Vec<(String, TokenSet)>>,
}

impl MemoryCreds {
    pub fn with(account: &AccountRef, stored: Stored) -> Arc<Self> {
        let me = Self::default();
        me.stored.lock().unwrap().insert(account.key(), stored);
        Arc::new(me)
    }

    pub fn get(&self, account: &AccountRef) -> Stored {
        self.stored
            .lock()
            .unwrap()
            .get(&account.key())
            .cloned()
            .unwrap_or_default()
    }

    pub fn saves(&self) -> usize {
        self.saved.lock().unwrap().len()
    }
}

#[async_trait]
impl Credentials for MemoryCreds {
    async fn load(&self, account: &AccountRef) -> Result<Stored, ConnectorError> {
        Ok(self.get(account))
    }

    async fn save_tokens(
        &self,
        account: &AccountRef,
        tokens: &TokenSet,
    ) -> Result<(), ConnectorError> {
        let mut all = self.stored.lock().unwrap();
        let entry = all.entry(account.key()).or_default();
        entry
            .fields
            .insert(Field::AccessToken, tokens.access_token.clone());
        if let Some(r) = &tokens.refresh_token {
            entry.fields.insert(Field::RefreshToken, r.clone());
        }
        entry.expires_at = tokens.expires_at;
        self.saved
            .lock()
            .unwrap()
            .push((account.key(), tokens.clone()));
        Ok(())
    }
}

/// A clock that stands where it is put.
pub struct FixedClock(pub Mutex<u64>);

impl FixedClock {
    pub fn at(now: u64) -> Arc<Self> {
        Arc::new(Self(Mutex::new(now)))
    }
}

impl Clock for FixedClock {
    fn now(&self) -> u64 {
        *self.0.lock().unwrap()
    }
}

/// Bytes that count up from a seed, so a state and a verifier are known.
pub struct CountingEntropy(pub Mutex<u8>);

impl CountingEntropy {
    pub fn from(seed: u8) -> Arc<Self> {
        Arc::new(Self(Mutex::new(seed)))
    }
}

impl Entropy for CountingEntropy {
    fn fill(&self, out: &mut [u8]) {
        let mut n = self.0.lock().unwrap();
        for b in out.iter_mut() {
            *b = *n;
            *n = n.wrapping_add(1);
        }
    }
}

/// A transport that answers from a script and records what it was sent —
/// for the tests that never need a socket, and for timing under a paused clock.
#[derive(Default)]
pub struct ScriptedTransport {
    pub answers: Mutex<VecDeque<Result<Response, TransportError>>>,
    pub sent: Mutex<Vec<Request>>,
}

impl ScriptedTransport {
    pub fn with(answers: Vec<Result<Response, TransportError>>) -> Arc<Self> {
        Arc::new(Self {
            answers: Mutex::new(answers.into_iter().collect()),
            sent: Mutex::new(Vec::new()),
        })
    }

    pub fn sent(&self) -> Vec<Request> {
        self.sent.lock().unwrap().clone()
    }
}

#[async_trait]
impl HttpTransport for ScriptedTransport {
    async fn send(
        &self,
        req: &Request,
        _insecure_loopback: bool,
    ) -> Result<Response, TransportError> {
        self.sent.lock().unwrap().push(req.clone());
        self.answers
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Err(TransportError::Connect("the script ran out".into())))
    }
}

pub fn ok(status: u16, body: serde_json::Value) -> Result<Response, TransportError> {
    Ok(Response {
        status,
        headers: vec![],
        body: serde_json::to_vec(&body).unwrap(),
    })
}

pub fn with_header(
    mut r: Result<Response, TransportError>,
    name: &str,
    value: &str,
) -> Result<Response, TransportError> {
    if let Ok(resp) = &mut r {
        resp.headers.push((name.to_string(), value.to_string()));
    }
    r
}

/// The account every test runs as.
pub fn account() -> AccountRef {
    AccountRef::new("stubby", "work")
}

/// A `GET /v1/items/{params.id}` spec over the stub with a given auth.
pub fn spec(stub: &Stub, auth: AuthSpec) -> CallSpec {
    CallSpec {
        connector: "stubby".into(),
        operation: "get".into(),
        base_url: stub.base_url().to_string(),
        hosts: vec![stub.host().to_string()],
        insecure_tls: false,
        auth,
        method: Method::Get,
        path: "/v1/items/{params.id}".into(),
        query: BTreeMap::new(),
        headers: BTreeMap::new(),
        body: None,
        params: vec![ParamSpec {
            name: "id".into(),
            kind: ParamKind::Text,
            required: true,
        }],
        select: None,
        expect: None,
        writes: false,
        idempotency: None,
        idempotency_key: None,
        page: None,
        timeout: Duration::from_secs(5),
    }
}

/// A spec that never needs a socket — a transport answers it.
pub fn offline_spec(auth: AuthSpec) -> CallSpec {
    CallSpec {
        connector: "stubby".into(),
        operation: "get".into(),
        base_url: "https://api.example.com".into(),
        hosts: vec!["api.example.com".into()],
        insecure_tls: false,
        auth,
        method: Method::Get,
        path: "/v1/items".into(),
        query: BTreeMap::new(),
        headers: BTreeMap::new(),
        body: None,
        params: vec![],
        select: None,
        expect: None,
        writes: false,
        idempotency: None,
        idempotency_key: None,
        page: None,
        timeout: Duration::from_secs(60),
    }
}

/// A client over the real transport to the loopback stub.
pub fn client(creds: Arc<MemoryCreds>) -> Client {
    Client::new(
        Arc::new(ReqwestTransport::new()),
        creds,
        FixedClock::at(1_000_000),
        CountingEntropy::from(1),
    )
}

/// A client over a scripted transport.
pub fn scripted(
    transport: Arc<ScriptedTransport>,
    creds: Arc<MemoryCreds>,
    clock: Arc<FixedClock>,
) -> Client {
    Client::new(transport, creds, clock, CountingEntropy::from(1))
}

pub fn params(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

pub fn no_account_params() -> BTreeMap<String, serde_json::Value> {
    BTreeMap::new()
}

pub fn secret(s: &str) -> Secret {
    Secret::new(s)
}

/// Files in a map: the checkout a test pretends to have.
#[derive(Default)]
pub struct MemoryFiles(pub BTreeMap<String, FileData>);

impl MemoryFiles {
    pub fn with(path: &str, data: FileData) -> Self {
        Self(BTreeMap::from([(path.to_string(), data)]))
    }
}

#[async_trait]
impl Files for MemoryFiles {
    async fn read(&self, path: &str) -> Result<FileData, String> {
        self.0
            .get(path)
            .cloned()
            .ok_or_else(|| format!("no such file in the checkout: {path}"))
    }
}
