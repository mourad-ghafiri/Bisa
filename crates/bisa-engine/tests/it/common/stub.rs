//! A stub platform on the loopback interface, and the connector fixtures the
//! suites share: canned answers matched by method and path (one may sit on
//! its answer, the way a slow platform does), a record of every request, a
//! chat connector whose `post` writes and whose `whoami` is the check, and
//! the account that holds a token in the memory keystore. Nothing leaves the
//! machine.

#![allow(dead_code)]

use crate::common::step;
use axum::body::Bytes;
use axum::extract::{Request as AxumRequest, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use axum::routing::any;
use axum::Router;
use bisa_core::{
    AccountId, AuthScheme, ConnectorId, HttpMethod, InputName, Operation, OperationBody,
    OperationId, OutputSpec, ParamDef, ParamKind, SecretField, StepKind, ValueRef,
};
use bisa_engine::Engine;
use bisa_store::{NewConnector, NewConnectorAccount};
use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// A stub platform on the loopback interface
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Canned {
    pub method: String,
    pub path: String,
    pub status: u16,
    pub body: Value,
    /// How long the stub sits on the answer — a platform that is slow.
    pub delay_ms: u64,
}

#[derive(Clone, Debug)]
pub struct Call {
    pub method: String,
    pub path: String,
    pub body: Value,
    pub text: String,
    pub content_type: Option<String>,
    pub authorization: Option<String>,
    /// The write's key, when the request carried one.
    pub idempotency_key: Option<String>,
}

#[derive(Clone)]
pub struct Shared {
    pub answers: Arc<Mutex<VecDeque<Canned>>>,
    pub calls: Arc<Mutex<Vec<Call>>>,
}

pub struct Stub {
    pub base_url: String,
    pub host: String,
    pub shared: Shared,
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
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
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

    pub fn calls(&self) -> Vec<Call> {
        self.shared.calls.lock().unwrap().clone()
    }
}

async fn handle(State(shared): State<Shared>, req: AxumRequest) -> AxumResponse {
    let (parts, body) = req.into_parts();
    let bytes: Bytes = axum::body::to_bytes(body, 1 << 20)
        .await
        .unwrap_or_default();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let method = parts.method.to_string();
    let path = parts.uri.path().to_string();
    shared.calls.lock().unwrap().push(Call {
        method: method.clone(),
        path: path.clone(),
        body,
        text,
        content_type: parts
            .headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string),
        authorization: parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string),
        idempotency_key: parts
            .headers
            .get("idempotency-key")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string),
    });
    let canned = {
        let mut answers = shared.answers.lock().unwrap();
        let at = answers
            .iter()
            .position(|a| a.method.eq_ignore_ascii_case(&method) && a.path == path);
        at.and_then(|i| answers.remove(i))
    };
    if let Some(c) = &canned {
        if c.delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(c.delay_ms)).await;
        }
    }
    match canned {
        Some(c) => (StatusCode::from_u16(c.status).unwrap(), axum::Json(c.body)).into_response(),
        None => (
            StatusCode::NOT_IMPLEMENTED,
            axum::Json(json!({"message": format!("the stub has no answer for {method} {path}")})),
        )
            .into_response(),
    }
}

pub fn canned(method: &str, path: &str, status: u16, body: Value) -> Canned {
    Canned {
        method: method.into(),
        path: path.into(),
        status,
        body,
        delay_ms: 0,
    }
}

/// An answer the stub sits on for `delay_ms` first.
pub fn slow(method: &str, path: &str, status: u16, body: Value, delay_ms: u64) -> Canned {
    Canned {
        delay_ms,
        ..canned(method, path, status, body)
    }
}

// ---------------------------------------------------------------------------
// Definitions and accounts
// ---------------------------------------------------------------------------

pub fn param(name: &str, kind: ParamKind, required: bool) -> ParamDef {
    ParamDef {
        name: InputName::new(name).unwrap(),
        label: name.to_uppercase(),
        kind,
        required,
        doc: format!("The {name}."),
    }
}

/// A chat platform on the stub: `post` writes `{ text }` and selects `ts`;
/// `whoami` is the check.
pub fn chat(stub: &Stub, auth: AuthScheme) -> NewConnector {
    NewConnector {
        id: ConnectorId::new("chat").unwrap(),
        name: "Chat".into(),
        description: "A chat platform on the stub.".into(),
        tags: Default::default(),
        base_url: stub.base_url.clone(),
        hosts: vec![stub.host.clone()],
        insecure_tls: false,
        auth,
        params: vec![],
        operations: vec![
            Operation {
                id: OperationId::new("post").unwrap(),
                name: "Post".into(),
                description: "Posts a message.".into(),
                method: HttpMethod::Post,
                path: "/post".into(),
                query: BTreeMap::new(),
                headers: BTreeMap::new(),
                body: Some(OperationBody::Json {
                    value: json!({"text": "{params.text}", "extra": "{params.extra}"}),
                }),
                params: vec![
                    param("text", ParamKind::Text, true),
                    param("extra", ParamKind::Json, false),
                ],
                output: OutputSpec {
                    expect: None,
                    select: Some("ts".into()),
                    schema: None,
                },
                writes: true,
                timeout_secs: None,
                idempotency: None,
                page: None,
            },
            Operation {
                id: OperationId::new("whoami").unwrap(),
                name: "Who am I".into(),
                description: "Answers who the token is.".into(),
                method: HttpMethod::Get,
                path: "/whoami".into(),
                query: BTreeMap::new(),
                headers: BTreeMap::new(),
                body: None,
                params: vec![],
                output: OutputSpec::default(),
                writes: false,
                timeout_secs: None,
                idempotency: None,
                page: None,
            },
        ],
        check: Some(OperationId::new("whoami").unwrap()),
    }
}

pub fn install_chat(engine: &Engine, stub: &Stub, token: Option<&str>) -> Option<AccountId> {
    install_chat_shaped(engine, stub, token, |_| {})
}

/// `install_chat` with the `post` operation reshaped first — a deadline of
/// its own, a key header.
pub fn install_chat_shaped(
    engine: &Engine,
    stub: &Stub,
    token: Option<&str>,
    shape: impl FnOnce(&mut Operation),
) -> Option<AccountId> {
    let ws = engine.workspace();
    let auth = if token.is_some() {
        AuthScheme::Bearer
    } else {
        AuthScheme::None
    };
    let mut def = chat(stub, auth);
    shape(&mut def.operations[0]);
    ws.create_connector(def).unwrap();
    token.map(|t| {
        let account = ws
            .create_connector_account(NewConnectorAccount {
                connector: ConnectorId::new("chat").unwrap(),
                label: "work".into(),
                params: BTreeMap::new(),
                default: true,
            })
            .unwrap();
        ws.set_connector_secrets(
            &ConnectorId::new("chat").unwrap(),
            account.id,
            &BTreeMap::from([(SecretField::Token, t.to_string())]),
        )
        .unwrap();
        account.id
    })
}

pub fn post_step(
    id: &str,
    account: Option<ValueRef<AccountId>>,
    extra: Option<&str>,
) -> bisa_core::Step {
    let mut params = BTreeMap::from([("text".to_string(), "hello {goal.statement}".to_string())]);
    if let Some(extra) = extra {
        params.insert("extra".to_string(), extra.to_string());
    }
    step(
        id,
        StepKind::Connector {
            connector: Some(ConnectorId::new("chat").unwrap()),
            operation: Some(OperationId::new("post").unwrap()),
            account,
            params,
            output_schema: None,
            // Every test here runs the write on its own word; the gate rule
            // is the validator's, tested in the core.
            unattended: true,
        },
    )
}

pub const TOKEN: &str = "xoxb-not-a-real-token-at-all";
