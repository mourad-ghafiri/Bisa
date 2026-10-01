//! A stub HTTP server on the loopback interface, for the GitHub tests: canned
//! answers matched by method and path, consumed in order, and a record of every
//! request — method, path, query, JSON body, and whether a bearer token came
//! with it. Nothing leaves the machine.

use axum::body::Bytes;
use axum::extract::{Request, State};
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use axum::Router;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// One answer the stub gives, once, to the first matching request.
#[derive(Clone, Debug)]
pub struct Canned {
    pub method: String,
    pub path: String,
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Option<serde_json::Value>,
}

impl Canned {
    pub fn json(method: &str, path: &str, status: u16, body: serde_json::Value) -> Self {
        Self {
            method: method.to_string(),
            path: path.to_string(),
            status,
            headers: Vec::new(),
            body: Some(body),
        }
    }

    /// A bodiless answer — GitHub's 204 on a deleted ref.
    pub fn empty(method: &str, path: &str, status: u16) -> Self {
        Self {
            method: method.to_string(),
            path: path.to_string(),
            status,
            headers: Vec::new(),
            body: None,
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
    pub headers: Vec<(String, String)>,
    /// The `Authorization` header, verbatim — a test compares it to the token
    /// it handed in, never to a real one.
    pub authorization: Option<String>,
    pub bearer: bool,
}

impl Call {
    pub fn header(&self, name: &str) -> Option<String> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
    }
}

#[derive(Clone)]
struct Shared {
    answers: Arc<Mutex<VecDeque<Canned>>>,
    calls: Arc<Mutex<Vec<Call>>>,
}

pub struct Stub {
    base_url: String,
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
            shared,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn calls(&self) -> Vec<Call> {
        self.shared.calls.lock().unwrap().clone()
    }
}

async fn handle(State(shared): State<Shared>, req: Request) -> Response {
    let (parts, body) = req.into_parts();
    let bytes: Bytes = axum::body::to_bytes(body, 1 << 20)
        .await
        .unwrap_or_default();
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
        headers: parts
            .headers
            .iter()
            .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_string())))
            .collect(),
        bearer: authorization
            .as_deref()
            .is_some_and(|a| a.starts_with("Bearer ") && a.len() > "Bearer ".len()),
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
            let mut resp = match c.body {
                Some(body) => (StatusCode::from_u16(c.status).unwrap(), axum::Json(body)).into_response(),
                None => StatusCode::from_u16(c.status).unwrap().into_response(),
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
