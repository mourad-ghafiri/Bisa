//! Connectors as the engine runs them: the one HTTP client every `connector`
//! step calls through, the credentials it reads from the workspace's
//! keystore, the OAuth flows waiting for a browser to come back, the doors a
//! person's routes go through to add a definition or an account, and the
//! roster an agent reads.
//!
//! **One door.** A step, a connector start's poll, an account check and an agent's
//! `call_connector` all go through [`invoke`]: the operation's own deadline
//! (`step_deadline`), a permit from the connector's concurrency cap
//! (`connectors.concurrency`), the workspace's host judge, the leaf crate's
//! call — pages followed, the host's circuit asked — and the selected answer
//! through the redactor before anyone reads it. A write's idempotency key is
//! the caller's: a step's is stable per run and step (`step_key`), so a
//! retry, a re-run and a restart send the same one; a poll and an agent's
//! read send none, since they never write.
//!
//! The definition and the account are the store's; the request, the auth, the
//! retry and the OAuth exchange are the leaf crate's
//! (`bisa_connectors`). This module is the seam between them: it maps a
//! [`Connector`] into the crate's [`CallSpec`], resolves which account a step
//! runs as, renders the step's parameters against the run, judges the host
//! against the workspace's own allow and deny lists, and hands the outcome
//! back to the run machine. A credential never passes through a log, an
//! event or a reason: the crate scrubs what it exposed, and every reason a
//! step fails with is redacted once more on the way out.

use crate::events::{ConnectorsChange, EngineEvent, EnginePayload};
use crate::{effects, EngineError, Inner};
use bisa_connectors as cx;
/// The leaf crate's error, re-exported so the node maps it without depending on the crate.
pub use bisa_connectors::ConnectorError;
use bisa_core::{
    AccountId, AuthScheme, Connector, ConnectorAccount, ConnectorId, HttpMethod, JwtAlg, KeyPlace,
    Operation, OperationBody, OperationId, ParamKind, PartSource, SecretField, Step, StepKind,
    ValueRef, WorkflowRun,
};
use bisa_store::{NewConnector, NewConnectorAccount, Workspace};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{broadcast, Semaphore};

/// How long a started OAuth flow waits for the browser to come back.
pub const PENDING_TTL_SECS: u64 = 600;

/// The machine setting that caps calls in flight per connector.
pub const CONCURRENCY_KEY: &str = "connectors.concurrency";
/// The cap when the setting cannot be read — the registry's default.
const DEFAULT_CONCURRENCY: usize = 4;
/// What the engine adds to an operation's deadline for its own work around
/// the call — the credential read, a file read, the render — so the outer
/// timeout is the operation's plus this, never the same number twice.
pub const OVERHEAD: Duration = Duration::from_secs(5);

/// A connection a person started and has not finished: what the redirect
/// must match, and what the exchange needs.
#[derive(Clone, Debug)]
pub struct PendingFlow {
    pub connector: ConnectorId,
    pub account: AccountId,
    pub verifier: cx::Secret,
    pub redirect_uri: String,
    pub expires_at: u64,
}

/// The engine's connector desk.
pub struct ConnectorDesk {
    pub client: cx::Client,
    /// Pending OAuth flows by `state`.
    pub pending: DashMap<String, PendingFlow>,
    /// One permit pool per connector: the cap on calls in flight.
    caps: Mutex<HashMap<ConnectorId, Arc<Semaphore>>>,
}

impl ConnectorDesk {
    pub fn new(
        ws: Arc<Workspace>,
        bus: broadcast::Sender<EngineEvent>,
        http: Arc<bisa_http::Clients>,
    ) -> Self {
        let client = cx::Client::new(
            Arc::new(cx::ReqwestTransport::with_http(http)),
            Arc::new(StoreCredentials { ws, bus }),
            Arc::new(cx::SystemClock),
            Arc::new(cx::OsEntropy),
        );
        Self {
            client,
            pending: DashMap::new(),
            caps: Mutex::new(HashMap::new()),
        }
    }

    /// The connector's permit pool, made with `limit` permits the first time
    /// it is asked for. A cap changed in Settings applies to a connector
    /// nobody has called yet; a pool already made keeps its size until the
    /// engine restarts — a permit count cannot shrink under calls in flight.
    fn cap(&self, connector: &ConnectorId, limit: usize) -> Arc<Semaphore> {
        let mut caps = self.caps.lock().unwrap_or_else(|p| p.into_inner());
        Arc::clone(
            caps.entry(connector.clone())
                .or_insert_with(|| Arc::new(Semaphore::new(limit.max(1)))),
        )
    }
}

/// The cap the machine's setting names, else the registry's default.
fn concurrency(inner: &Inner) -> usize {
    inner
        .ws
        .setting(CONCURRENCY_KEY, None)
        .ok()
        .and_then(|r| r.value.as_u64())
        .map_or(DEFAULT_CONCURRENCY, |n| n.clamp(1, 32) as usize)
}

/// The operation's own deadline, else the machine's `connector_timeout_secs`.
pub fn operation_timeout(inner: &Inner, op: &Operation) -> Duration {
    Duration::from_secs(
        op.timeout_secs
            .unwrap_or(inner.config.connector_timeout_secs)
            .max(1),
    )
}

/// What a `connector` step's task is given before it is given up on: the
/// operation's deadline plus the engine's overhead — or the machine's plus
/// the overhead when the step's connector cannot be read, so a task never
/// outlives its step for want of a definition.
pub fn step_deadline(inner: &Inner, step: &Step) -> Duration {
    let StepKind::Connector {
        connector: Some(connector),
        operation: Some(operation),
        ..
    } = &step.kind
    else {
        return Duration::from_secs(inner.config.connector_timeout_secs.max(1)) + OVERHEAD;
    };
    let op_timeout = inner
        .ws
        .get_connector(connector)
        .ok()
        .and_then(|def| {
            def.operation(operation)
                .map(|op| operation_timeout(inner, op))
        })
        .unwrap_or(Duration::from_secs(
            inner.config.connector_timeout_secs.max(1),
        ));
    op_timeout + OVERHEAD
}

/// The key a step's write travels with: the same for every attempt of the
/// same step of the same run, and for a restart's re-run — which is what
/// lets the platform tell a resend from a second request.
pub fn step_key(run: bisa_core::RunId, step: &bisa_core::StepId) -> String {
    format!("bisa-{run}-{step}")
}

/// Whether a `connector` step's operation writes, and whether its write is
/// keyed — what decides if a failure that may have reached the platform is
/// retried or stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WriteShape {
    pub writes: bool,
    pub keyed: bool,
}

impl WriteShape {
    /// A read, or a keyed write: every failure may be tried again.
    pub const SAFE: WriteShape = WriteShape {
        writes: false,
        keyed: false,
    };

    /// Whether `e` stops the step for good: an un-keyed write whose request
    /// may have reached the platform — a timeout, a failure after the
    /// connection was made, a 5xx. A connect failure never reached it; a
    /// paused host was never asked; a refusal is the caller's to fix.
    pub fn stops_on(&self, e: &EngineError) -> bool {
        self.writes
            && !self.keyed
            && matches!(
                e,
                EngineError::Connector(
                    ConnectorError::Timeout(_)
                        | ConnectorError::Transport(_)
                        | ConnectorError::Upstream { .. }
                )
            )
    }
}

/// The write shape of a `connector` step, read from its definition; a step
/// whose connector cannot be read is taken as safe — the call itself will
/// say what is wrong.
pub fn write_shape(inner: &Inner, step: &Step) -> WriteShape {
    let StepKind::Connector {
        connector: Some(connector),
        operation: Some(operation),
        ..
    } = &step.kind
    else {
        return WriteShape::SAFE;
    };
    inner
        .ws
        .get_connector(connector)
        .ok()
        .and_then(|def| {
            def.operation(operation).map(|op| WriteShape {
                writes: op.writes,
                keyed: op.idempotency.is_some(),
            })
        })
        .unwrap_or(WriteShape::SAFE)
}

/// Whether a restart may not re-run the step: a `connector` write with no
/// idempotency key, whose request the last process may have sent.
pub fn unsafe_to_resend(inner: &Inner, run: &WorkflowRun, step: &bisa_core::StepId) -> bool {
    run.workflow
        .step(step)
        .map(|def| {
            let shape = write_shape(inner, def);
            shape.writes && !shape.keyed
        })
        .unwrap_or(false)
}

/// Abort every call a run has in flight — the run stopped; nothing waits on
/// a platform for a step nobody reads any more.
pub fn abort_calls(inner: &Inner, run: bisa_core::RunId) {
    inner.connector_calls.retain(|(r, _), handle| {
        if *r == run {
            handle.abort();
            false
        } else {
            true
        }
    });
}

/// Who is calling, for the reason a refusal is recorded with and for the
/// redaction's note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Caller {
    Step,
    Poll,
    Check,
    Agent,
}

impl Caller {
    fn word(self) -> &'static str {
        match self {
            Caller::Step => "step",
            Caller::Poll => "poll",
            Caller::Check => "check",
            Caller::Agent => "agent",
        }
    }
}

/// One call, as every caller asks for it.
pub struct Invocation<'a> {
    pub def: &'a Connector,
    pub op: &'a Operation,
    /// The account resolved by the caller (`resolve_account`); none for a
    /// scheme that needs none.
    pub account: Option<&'a ConnectorAccount>,
    /// The operation's parameters, rendered — text the crate types by kind.
    pub params: &'a BTreeMap<String, String>,
    /// Where a `file` parameter is read from; `NoFiles` for a call with no checkout.
    pub files: &'a dyn cx::Files,
    /// The workspace's say over the host.
    pub judge: &'a dyn cx::HostJudge,
    /// The write's key, when the operation names a header for one.
    pub key: Option<String>,
    pub caller: Caller,
}

/// The one door: the cap's permit, the operation's deadline, the crate's
/// call, the answer through the redactor. What comes back is the crate's
/// outcome with `selected` already safe to hand to anyone.
pub async fn invoke(inner: &Inner, inv: Invocation<'_>) -> Result<cx::Outcome, EngineError> {
    let cap = inner.connectors.cap(&inv.def.id, concurrency(inner));
    let _permit = cap.acquire().await.map_err(|_| {
        EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-connector-s-permit-pool-closed"
        ))
    })?;
    let mut spec = call_spec(inv.def, inv.op, operation_timeout(inner, inv.op));
    spec.idempotency_key = inv.key;
    let account_ref = inv
        .account
        .map(|a| cx::AccountRef::new(inv.def.id.as_str(), a.id.to_string()));
    let empty = BTreeMap::new();
    let account_params = inv.account.map(|a| &a.params).unwrap_or(&empty);
    let mut outcome = inner
        .connectors
        .client
        .call_with(
            &spec,
            account_ref.as_ref(),
            account_params,
            inv.params,
            inv.files,
            inv.judge,
        )
        .await?;
    outcome.selected = redact_value(
        inner,
        &format!(
            "connector {}.{} ({})",
            inv.def.id,
            inv.op.id,
            inv.caller.word()
        ),
        outcome.selected,
    );
    Ok(outcome)
}

/// A JSON answer through the redactor: a platform that echoes a key it was
/// given, or mints one, must not hand it to a run's history, a signal or an
/// agent. The value is redacted as text and read back; a replacement that
/// broke the JSON leaves the redacted text itself.
fn redact_value(inner: &Inner, at: &str, value: Value) -> Value {
    let text = value.to_string();
    let redaction = inner.security.redact_at(at, &text);
    if redaction.count == 0 {
        return value;
    }
    serde_json::from_str(&redaction.text).unwrap_or(Value::String(redaction.text))
}

impl std::fmt::Debug for ConnectorDesk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectorDesk")
            .field("pending", &self.pending.len())
            .finish()
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn emit(bus: &broadcast::Sender<EngineEvent>, what: ConnectorsChange) {
    if bus
        .send(EngineEvent::global(EnginePayload::ConnectorsChanged {
            what,
        }))
        .is_err()
    {
        tracing::trace!("connectors event dropped: no subscribers");
    }
}

fn announce(inner: &Inner, what: ConnectorsChange) {
    inner.emit(EngineEvent::global(EnginePayload::ConnectorsChanged {
        what,
    }));
}

// ---------------------------------------------------------------------------
// The crate's ports, implemented over the workspace
// ---------------------------------------------------------------------------

/// The crate's credentials, read from and written to the workspace's
/// keystore. A read maps every field the account's record says is set; a
/// save is what keeps a refreshed token.
struct StoreCredentials {
    ws: Arc<Workspace>,
    bus: broadcast::Sender<EngineEvent>,
}

fn account_ref(account: &cx::AccountRef) -> Result<(ConnectorId, AccountId), cx::ConnectorError> {
    let connector = ConnectorId::new(&account.connector)
        .map_err(|e| cx::ConnectorError::Store(e.to_string()))?;
    let id = account
        .account
        .parse::<AccountId>()
        .map_err(|e| cx::ConnectorError::Store(e.to_string()))?;
    Ok((connector, id))
}

#[async_trait::async_trait]
impl cx::Credentials for StoreCredentials {
    async fn load(&self, account: &cx::AccountRef) -> Result<cx::Stored, cx::ConnectorError> {
        let (connector, id) = account_ref(account)?;
        let record = self
            .ws
            .get_connector_account(&connector, id)
            .map_err(|e| cx::ConnectorError::Store(e.to_string()))?;
        let mut stored = cx::Stored {
            fields: BTreeMap::new(),
            expires_at: record.auth.expires_at,
        };
        for field in &record.auth.fields_set {
            let Some(name) = cx::Field::parse(field.as_str()) else {
                continue;
            };
            if let Some(value) = self
                .ws
                .connector_secret(&connector, id, *field)
                .map_err(|e| cx::ConnectorError::Store(e.to_string()))?
            {
                stored.fields.insert(name, cx::Secret::new(value));
            }
        }
        Ok(stored)
    }

    async fn save_tokens(
        &self,
        account: &cx::AccountRef,
        tokens: &cx::TokenSet,
    ) -> Result<(), cx::ConnectorError> {
        let (connector, id) = account_ref(account)?;
        self.ws
            .set_connector_tokens(
                &connector,
                id,
                tokens.access_token.expose(),
                tokens.refresh_token.as_ref().map(|s| s.expose()),
                tokens.expires_at,
                tokens.scope.clone(),
            )
            .map_err(|e| cx::ConnectorError::Store(e.to_string()))?;
        emit(&self.bus, ConnectorsChange::Accounts);
        Ok(())
    }
}

/// The workspace's say over a host the definition declared: the
/// `security.net.*` lists, judged by the pure policy. A refusal is recorded
/// the way a guard decision is, so the journal and Settings › Security show
/// what the platform's own traffic was stopped from reaching.
pub(crate) struct PolicyHostJudge<'a> {
    pub(crate) inner: &'a Inner,
    /// The goal, or the run of the workspace, a refusal is recorded on.
    pub(crate) home: Option<bisa_core::Home>,
    pub(crate) subject: String,
    pub(crate) declared: &'a [String],
}

impl cx::HostJudge for PolicyHostJudge<'_> {
    fn judge(&self, host: &str) -> Result<(), String> {
        let policy = self.inner.security.policy();
        match bisa_security::net::decide_host(&policy.hosts, self.declared, host) {
            bisa_security::net::HostVerdict::Allow { .. } => Ok(()),
            bisa_security::net::HostVerdict::Deny { reason } => {
                crate::security::record_host_refusal(self.inner, self.home, &self.subject, &reason);
                Err(reason)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// From the domain's definition to the crate's call
// ---------------------------------------------------------------------------

fn method_of(m: HttpMethod) -> cx::Method {
    match m {
        HttpMethod::Get => cx::Method::Get,
        HttpMethod::Post => cx::Method::Post,
        HttpMethod::Put => cx::Method::Put,
        HttpMethod::Patch => cx::Method::Patch,
        HttpMethod::Delete => cx::Method::Delete,
        HttpMethod::Head => cx::Method::Head,
    }
}

fn kind_of(k: ParamKind) -> cx::ParamKind {
    match k {
        ParamKind::Text => cx::ParamKind::Text,
        ParamKind::Number => cx::ParamKind::Number,
        ParamKind::Bool => cx::ParamKind::Bool,
        ParamKind::Json => cx::ParamKind::Json,
        ParamKind::File => cx::ParamKind::File,
    }
}

/// The crate's body shape for a definition's — the same words, once.
fn body_of(body: &OperationBody) -> cx::CallBody {
    match body {
        OperationBody::Json { value } => cx::CallBody::Json {
            value: value.clone(),
        },
        OperationBody::Form { fields } => cx::CallBody::Form {
            fields: fields.clone(),
        },
        OperationBody::Multipart { parts } => cx::CallBody::Multipart {
            parts: parts
                .iter()
                .map(|p| cx::Part {
                    name: p.name.clone(),
                    source: match &p.source {
                        PartSource::File { file } => cx::PartSource::File {
                            file: file.to_string(),
                        },
                        PartSource::Text { text } => cx::PartSource::Text { text: text.clone() },
                    },
                    filename: p.filename.clone(),
                    content_type: p.content_type.clone(),
                })
                .collect(),
        },
        OperationBody::Raw { content_type, from } => cx::CallBody::Raw {
            content_type: content_type.clone(),
            from: from.to_string(),
        },
    }
}

/// The crate's auth shape for a definition's scheme — the same words, once.
pub fn auth_spec(auth: &AuthScheme) -> cx::AuthSpec {
    match auth {
        AuthScheme::None => cx::AuthSpec::None,
        AuthScheme::ApiKey { place, prefix } => cx::AuthSpec::ApiKey {
            place: match place {
                KeyPlace::Header { name } => cx::KeyPlace::Header { name: name.clone() },
                KeyPlace::Query { name } => cx::KeyPlace::Query { name: name.clone() },
            },
            prefix: prefix.clone(),
        },
        AuthScheme::Bearer => cx::AuthSpec::Bearer,
        AuthScheme::Basic => cx::AuthSpec::Basic,
        AuthScheme::OAuth2 {
            authorization_url,
            token_url,
            scopes,
            pkce,
            extra,
        } => cx::AuthSpec::OAuth2 {
            authorization_url: authorization_url.clone(),
            token_url: token_url.clone(),
            scopes: scopes.clone(),
            pkce: *pkce,
            extra: extra.clone(),
        },
        AuthScheme::Jwt {
            alg,
            claims,
            header,
            ttl_secs,
        } => cx::AuthSpec::Jwt {
            alg: match alg {
                JwtAlg::Es256 => cx::JwtAlg::Es256,
                JwtAlg::Rs256 => cx::JwtAlg::Rs256,
            },
            claims: claims.clone(),
            header: header.clone(),
            ttl_secs: *ttl_secs,
        },
    }
}

/// The files a `connector` step's `file` parameters name: paths inside the
/// run's checkout — where the run's work landed (`projects::check_cwd`) —
/// read when the step runs. A path that climbs out, an absolute one, or one
/// that resolves outside the root through a link is refused by name; the
/// bytes never pass through a prompt or a log.
pub struct RunFiles {
    pub root: PathBuf,
}

#[async_trait::async_trait]
impl cx::Files for RunFiles {
    async fn read(&self, path: &str) -> Result<cx::FileData, String> {
        let rel = Path::new(path);
        if rel.as_os_str().is_empty()
            || rel.is_absolute()
            || rel.components().any(|c| {
                matches!(
                    c,
                    Component::ParentDir | Component::Prefix(_) | Component::RootDir
                )
            })
        {
            return Err("a file is a relative path inside the run's checkout, with no `..`".into());
        }
        let root = tokio::fs::canonicalize(&self.root)
            .await
            .map_err(|e| format!("the run's checkout is not readable: {e}"))?;
        let full = tokio::fs::canonicalize(root.join(rel))
            .await
            .map_err(|e| format!("{path}: {e}"))?;
        if !full.starts_with(&root) {
            return Err(format!("{path} points outside the run's checkout"));
        }
        let meta = tokio::fs::metadata(&full)
            .await
            .map_err(|e| format!("{path}: {e}"))?;
        if !meta.is_file() {
            return Err(format!("{path} is not a file"));
        }
        if meta.len() > cx::MAX_FILE_BYTES as u64 {
            return Err(format!(
                "{path} is {} bytes; a file is read up to {} MiB",
                meta.len(),
                cx::MAX_FILE_BYTES / (1024 * 1024)
            ));
        }
        let bytes = tokio::fs::read(&full)
            .await
            .map_err(|e| format!("{path}: {e}"))?;
        Ok(cx::FileData {
            filename: rel
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            content_type: None,
            bytes,
        })
    }
}

/// One operation of one connector as the crate calls it. A 1:1 mapping —
/// the definition is the domain's, the call shape is the crate's, and this is
/// the only place the two meet.
pub fn call_spec(connector: &Connector, op: &Operation, timeout: Duration) -> cx::CallSpec {
    cx::CallSpec {
        connector: connector.id.to_string(),
        operation: op.id.to_string(),
        base_url: connector.base_url.clone(),
        hosts: connector.hosts.clone(),
        insecure_tls: connector.insecure_tls,
        auth: auth_spec(&connector.auth),
        method: method_of(op.method),
        path: op.path.clone(),
        query: op.query.clone(),
        headers: op.headers.clone(),
        body: op.body.as_ref().map(body_of),
        params: op
            .params
            .iter()
            .map(|p| cx::ParamSpec {
                name: p.name.to_string(),
                kind: kind_of(p.kind),
                required: p.required,
            })
            .collect(),
        select: op.output.select.clone(),
        writes: op.writes,
        idempotency: op.idempotency.as_ref().map(|i| cx::Idempotency {
            header: i.header.clone(),
        }),
        // The caller's, set by `invoke`; the mapping carries none.
        idempotency_key: None,
        page: op.page.as_ref().map(|p| cx::Paging {
            cursor_param: p.cursor_param.to_string(),
            next_cursor: p.next_cursor.clone(),
            max_pages: p.max_pages,
        }),
        timeout,
    }
}

/// The account a call runs as: the one named, else the connector's default,
/// else its only one. A connector whose scheme needs no account runs as
/// nobody unless one is named.
pub fn resolve_account(
    ws: &Workspace,
    connector: &Connector,
    wanted: Option<AccountId>,
) -> Result<Option<ConnectorAccount>, EngineError> {
    match wanted {
        Some(id) => Ok(Some(ws.get_connector_account(&connector.id, id)?)),
        None if !connector.auth.needs_account() => Ok(None),
        None => match ws.default_connector_account(&connector.id)? {
            Some(a) => Ok(Some(a)),
            None => Err(EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-connector-has-no-account-here-add-one",
                a0 = (connector.id).to_string(),
                a1 = (connector.name).to_string()
            ))),
        },
    }
}

fn account_of(
    run: &WorkflowRun,
    step: &Step,
    account: Option<&ValueRef<AccountId>>,
) -> Result<Option<AccountId>, EngineError> {
    Ok(match account {
        None => None,
        Some(ValueRef::Fixed(id)) => Some(*id),
        Some(ValueRef::Input { input }) => {
            let value = run.inputs.get(input.as_str()).and_then(Value::as_str);
            match value.map(str::parse::<AccountId>) {
                Some(Ok(id)) => Some(id),
                _ => {
                    return Err(EngineError::Invalid(bisa_core::text!(
                        "error-engine-invalid-step-input-holds-no-account-id",
                        a0 = (step.id).to_string(),
                        input = input.to_string()
                    )))
                }
            }
        }
    })
}

/// Call the operation a `connector` step names, as the run's account, with
/// the step's parameters rendered against the run. The answer is what the
/// operation selects, checked against the step's schema when it has one.
pub async fn call(
    inner: &Arc<Inner>,
    run: &WorkflowRun,
    step: &Step,
) -> Result<Value, EngineError> {
    let StepKind::Connector {
        connector,
        operation,
        account,
        params,
        output_schema,
        ..
    } = &step.kind
    else {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-step-not-connector-step",
            a0 = (step.id).to_string(),
            a1 = (step.kind.as_str()).to_string()
        )));
    };
    // A choice not made is a problem the start gate refuses; a run that
    // reaches here with one has no connector to call.
    let Some(connector) = connector else {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-step-names-no-connector",
            a0 = (step.id).to_string()
        )));
    };
    let Some(operation) = operation else {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-step-names-no-operation",
            a0 = (step.id).to_string(),
            connector = connector.to_string()
        )));
    };
    let def = inner.ws.get_connector(connector)?;
    let op = def.operation(operation).ok_or_else(|| {
        EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-connector-has-no-operation",
            connector = connector.to_string(),
            operation = operation.to_string()
        ))
    })?;
    let wanted = account_of(run, step, account.as_ref())?;
    let acct = resolve_account(&inner.ws, &def, wanted)?;
    let mut rendered = BTreeMap::new();
    for (name, tmpl) in params {
        rendered.insert(name.clone(), effects::render(inner, run, tmpl)?);
    }
    let subject = format!("{} {}{}", op.method.as_str(), def.hosts.join("|"), op.path);
    // A rendered parameter is never restored — a placeholder in it would go
    // to the host as the literal text, which is neither the secret nor
    // anything the host can use. Refused before the request exists.
    let placeholders: Vec<String> = rendered
        .iter()
        .filter(|(_, v)| bisa_security::has_placeholder(v))
        .map(|(name, _)| name.clone())
        .collect();
    if !placeholders.is_empty() {
        let reason = crate::security::record_unresolved(
            inner,
            Some(run.home()),
            "connector",
            &subject,
            &placeholders,
        );
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-step-was-not-sent",
            a0 = (step.id).to_string(),
            reason = reason.to_string()
        )));
    }
    let judge = PolicyHostJudge {
        inner,
        home: Some(run.home()),
        subject,
        declared: &def.hosts,
    };
    let files = RunFiles {
        root: crate::projects::check_cwd(inner, run),
    };
    let outcome = invoke(
        inner,
        Invocation {
            def: &def,
            op,
            account: acct.as_ref(),
            params: &rendered,
            files: &files,
            judge: &judge,
            key: op.idempotency.as_ref().map(|_| step_key(run.id, &step.id)),
            caller: Caller::Step,
        },
    )
    .await?;
    if let Some(schema) = output_schema {
        let errors = effects::schema_errors(schema, &outcome.selected);
        if !errors.is_empty() {
            return Err(EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-answer-does-not-fit-step-s-output",
                connector = connector.to_string(),
                operation = operation.to_string(),
                a0 = (errors.join("; ")).to_string()
            )));
        }
    }
    Ok(outcome.selected)
}

/// The reason a step fails with: the error's own words through the redactor,
/// capped so a platform's error page does not become the journal.
pub fn redact_reason(inner: &Inner, e: &EngineError) -> String {
    let text = inner.security.redact(&e.to_string()).text;
    text.chars().take(2000).collect()
}

// ---------------------------------------------------------------------------
// OAuth
// ---------------------------------------------------------------------------

/// What `oauth_start` hands the person: where to send the browser, where it
/// comes back, and until when the flow is remembered.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OAuthStart {
    pub url: String,
    pub redirect_uri: String,
    pub expires_at: u64,
}

fn oauth_scheme(connector: &Connector) -> Result<cx::AuthSpec, EngineError> {
    match &connector.auth {
        AuthScheme::OAuth2 { .. } => Ok(auth_spec(&connector.auth)),
        other => Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-connector-authenticates-with-not-oauth-set-secrets",
            a0 = (connector.id).to_string(),
            a1 = (other.word()).to_string()
        ))),
    }
}

/// Begin connecting an account: the authorization URL for the person's own
/// OAuth client, with the state the redirect must bring back.
pub fn oauth_start(
    inner: &Inner,
    connector: &ConnectorId,
    account: AccountId,
    port: u16,
) -> Result<OAuthStart, EngineError> {
    let def = inner.ws.get_connector(connector)?;
    let auth = oauth_scheme(&def)?;
    inner.ws.get_connector_account(connector, account)?;
    let client_id = inner
        .ws
        .connector_secret(connector, account, SecretField::ClientId)?
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-account-has-no-oauth-client-id-yet",
                account = account.to_string(),
                connector = connector.to_string()
            ))
        })?;
    let now = now_secs();
    inner.connectors.pending.retain(|_, f| f.expires_at > now);
    let redirect_uri = format!("http://127.0.0.1:{port}/connectors/oauth/callback");
    // The consent page is a host too: the deny list holds for it.
    let judge = PolicyHostJudge {
        inner,
        home: None,
        subject: format!("oauth authorize {}", def.id),
        declared: &def.hosts,
    };
    let authorize = cx::authorize_url(
        &auth,
        &client_id,
        &redirect_uri,
        inner.connectors.client.entropy(),
        &judge,
    )?;
    let expires_at = now + PENDING_TTL_SECS;
    inner.connectors.pending.insert(
        authorize.state.clone(),
        PendingFlow {
            connector: connector.clone(),
            account,
            verifier: authorize.verifier,
            redirect_uri: redirect_uri.clone(),
            expires_at,
        },
    );
    Ok(OAuthStart {
        url: authorize.url,
        redirect_uri,
        expires_at,
    })
}

async fn finish_flow(
    inner: &Inner,
    flow: PendingFlow,
    code: &str,
) -> Result<(ConnectorId, AccountId), EngineError> {
    let def = inner.ws.get_connector(&flow.connector)?;
    let auth = oauth_scheme(&def)?;
    let account = cx::AccountRef::new(flow.connector.as_str(), flow.account.to_string());
    let judge = PolicyHostJudge {
        inner,
        home: None,
        subject: format!("oauth token {}", def.id),
        declared: &def.hosts,
    };
    cx::exchange(
        &inner.connectors.client,
        &auth,
        &account,
        code,
        &flow.verifier,
        &flow.redirect_uri,
        &judge,
    )
    .await?;
    Ok((flow.connector, flow.account))
}

/// The browser came back: exchange the code for tokens and keep them.
pub async fn oauth_complete(
    inner: &Inner,
    state: &str,
    code: &str,
) -> Result<(ConnectorId, AccountId), EngineError> {
    let flow = inner
        .connectors
        .pending
        .remove(state)
        .map(|(_, f)| f)
        .filter(|f| f.expires_at > now_secs())
        .ok_or_else(|| {
            EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-unknown-expired-oauth-state-start-connection-again"
            ))
        })?;
    // The flow is consumed by the exchange: what it was about is said here,
    // or a failure could name nobody.
    let (connector, account) = (flow.connector.clone(), flow.account);
    finish_flow(inner, flow, code).await.inspect_err(|e| {
        tracing::warn!(target: "bisa_engine::connectors", %connector, %account, "the OAuth exchange did not finish: {e}");
    })
}

/// The person pasted the code by hand — for a platform that cannot send the
/// browser back to loopback. The newest flow started for the account is the
/// one it completes.
pub async fn oauth_paste(
    inner: &Inner,
    connector: &ConnectorId,
    account: AccountId,
    code: &str,
) -> Result<(), EngineError> {
    let now = now_secs();
    let newest = inner
        .connectors
        .pending
        .iter()
        .filter(|e| &e.connector == connector && e.account == account && e.expires_at > now)
        .max_by_key(|e| e.expires_at)
        .map(|e| e.key().clone());
    let Some(state) = newest else {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-no-connection-was-started-account-start-first",
            account = account.to_string(),
            connector = connector.to_string()
        )));
    };
    let flow = inner
        .connectors
        .pending
        .remove(&state)
        .map(|(_, f)| f)
        .ok_or_else(|| {
            EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-connection-was-already-completed"
            ))
        })?;
    finish_flow(inner, flow, code).await?;
    Ok(())
}

/// How many flows are waiting for a browser right now.
pub fn oauth_pending(inner: &Inner) -> usize {
    let now = now_secs();
    inner.connectors.pending.retain(|_, f| f.expires_at > now);
    inner.connectors.pending.len()
}

// ---------------------------------------------------------------------------
// Checking an account
// ---------------------------------------------------------------------------

/// What one live request as the account found.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AccountCheckState {
    /// The check operation answered.
    Connected,
    /// The platform refused the credential, the host or the request.
    Refused,
    /// The platform could not be reached, or failed.
    Unreachable,
    /// The definition names no check operation.
    NoCheck,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AccountCheck {
    pub state: AccountCheckState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Run the connector's `check` operation as the account — the one request
/// Settings makes to prove a way in works.
pub async fn check_account(
    inner: &Inner,
    connector: &ConnectorId,
    account: AccountId,
) -> Result<AccountCheck, EngineError> {
    let def = inner.ws.get_connector(connector)?;
    let acct = inner.ws.get_connector_account(connector, account)?;
    let Some(op) = def.check.as_ref().and_then(|id| def.operation(id)) else {
        return Ok(AccountCheck {
            state: AccountCheckState::NoCheck,
            status: None,
            reason: Some("the connector names no check operation".into()),
        });
    };
    let judge = PolicyHostJudge {
        inner,
        home: None,
        subject: format!("{} {}{}", op.method.as_str(), def.hosts.join("|"), op.path),
        declared: &def.hosts,
    };
    let outcome = invoke(
        inner,
        Invocation {
            def: &def,
            op,
            account: Some(&acct),
            params: &BTreeMap::new(),
            files: &cx::NoFiles,
            judge: &judge,
            key: None,
            caller: Caller::Check,
        },
    )
    .await
    .map_err(|e| match e {
        EngineError::Connector(c) => c,
        other => cx::ConnectorError::Store(other.to_string()),
    });
    Ok(match outcome {
        Ok(o) => AccountCheck {
            state: AccountCheckState::Connected,
            status: Some(o.status),
            reason: None,
        },
        Err(e) => {
            let status = match &e {
                cx::ConnectorError::Refused { status, .. }
                | cx::ConnectorError::Upstream { status, .. } => Some(*status),
                cx::ConnectorError::NotAuthenticated(_) => Some(401),
                cx::ConnectorError::NotFound(_) => Some(404),
                cx::ConnectorError::RateLimited { .. } => Some(429),
                _ => None,
            };
            let state = if e.is_refusal() {
                AccountCheckState::Refused
            } else {
                AccountCheckState::Unreachable
            };
            let reason = inner.security.redact(&e.to_string()).text;
            AccountCheck {
                state,
                status,
                reason: Some(reason.chars().take(600).collect()),
            }
        }
    })
}

// ---------------------------------------------------------------------------
// The doors: definitions and accounts
// ---------------------------------------------------------------------------

/// Record a person's own connector definition.
pub fn create_connector(inner: &Inner, new: NewConnector) -> Result<Connector, EngineError> {
    let created = inner.ws.create_connector(new)?;
    announce(inner, ConnectorsChange::Definitions);
    Ok(created)
}

/// Replace a person's own definition. A catalog entry is the catalog's: it
/// is refused here, so an install can never be quietly overwritten.
pub fn update_connector(inner: &Inner, def: Connector) -> Result<Connector, EngineError> {
    let existing = inner.ws.get_connector(&def.id)?;
    if matches!(existing.origin, bisa_core::Origin::Catalog { .. }) {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-connector-catalog-s-not-edited-write-your",
            a0 = (def.id).to_string()
        )));
    }
    let updated = inner.ws.update_connector(def)?;
    announce(inner, ConnectorsChange::Definitions);
    Ok(updated)
}

/// Remove a definition — refused while an account or a step names it.
pub fn remove_connector(inner: &Inner, id: &ConnectorId) -> Result<(), EngineError> {
    inner.ws.remove_connector(id)?;
    announce(inner, ConnectorsChange::Definitions);
    Ok(())
}

/// Add an account and, in the same breath, the secrets it arrived with.
pub fn put_account(
    inner: &Inner,
    new: NewConnectorAccount,
    secrets: BTreeMap<SecretField, String>,
) -> Result<ConnectorAccount, EngineError> {
    let connector = new.connector.clone();
    let mut account = inner.ws.create_connector_account(new)?;
    if !secrets.is_empty() {
        account = inner
            .ws
            .set_connector_secrets(&connector, account.id, &secrets)?;
    }
    announce(inner, ConnectorsChange::Accounts);
    Ok(account)
}

/// Change an account's label and parameters.
pub fn update_account(
    inner: &Inner,
    connector: &ConnectorId,
    account: AccountId,
    label: String,
    params: BTreeMap<String, Value>,
) -> Result<ConnectorAccount, EngineError> {
    let updated = inner
        .ws
        .update_connector_account(connector, account, label, params)?;
    announce(inner, ConnectorsChange::Accounts);
    Ok(updated)
}

/// Set secret fields, each replaced; never read back.
pub fn set_secrets(
    inner: &Inner,
    connector: &ConnectorId,
    account: AccountId,
    secrets: BTreeMap<SecretField, String>,
) -> Result<ConnectorAccount, EngineError> {
    let updated = inner
        .ws
        .set_connector_secrets(connector, account, &secrets)?;
    announce(inner, ConnectorsChange::Accounts);
    Ok(updated)
}

/// Forget an account and every secret it held.
pub fn delete_account(
    inner: &Inner,
    connector: &ConnectorId,
    account: AccountId,
) -> Result<(), EngineError> {
    inner.ws.delete_connector_account(connector, account)?;
    inner
        .connectors
        .pending
        .retain(|_, f| !(&f.connector == connector && f.account == account));
    announce(inner, ConnectorsChange::Accounts);
    Ok(())
}

/// Make one account the connector's default.
pub fn set_default_account(
    inner: &Inner,
    connector: &ConnectorId,
    account: AccountId,
) -> Result<ConnectorAccount, EngineError> {
    let updated = inner.ws.set_default_connector_account(connector, account)?;
    announce(inner, ConnectorsChange::Accounts);
    Ok(updated)
}

// ---------------------------------------------------------------------------
// The roster the Workflow Agent reads
// ---------------------------------------------------------------------------

/// One operation as the roster lists it.
#[derive(Debug, Clone, Serialize)]
pub struct RosterOp {
    pub id: OperationId,
    pub name: String,
    pub description: String,
    pub method: String,
    pub writes: bool,
    /// `(name, kind, required)`.
    pub params: Vec<(String, String, bool)>,
    pub select: Option<String>,
}

/// One connector as the roster lists it: its accounts by label, never a
/// value.
#[derive(Debug, Clone, Serialize)]
pub struct RosterConnector {
    pub id: ConnectorId,
    pub name: String,
    pub description: String,
    pub auth: &'static str,
    /// `(id, label, default)`.
    pub accounts: Vec<(AccountId, String, bool)>,
    pub operations: Vec<RosterOp>,
}

/// The connectors a designer may name on a step: every definition installed
/// here, with what each operation takes and whether an account is connected.
/// One type, built once, rendered for a prompt and served as an op — so the
/// list the Workflow Agent reads in its directive and the one
/// `list_connectors` answers cannot disagree.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ConnectorRoster {
    pub connectors: Vec<RosterConnector>,
}

impl ConnectorRoster {
    /// Pure. Sorted by id, so the rendered roster reads the same on every
    /// wake.
    pub fn from_parts(connectors: &[Connector], accounts: &[ConnectorAccount]) -> Self {
        let mut out: Vec<RosterConnector> = connectors
            .iter()
            .map(|c| RosterConnector {
                id: c.id.clone(),
                name: c.name.clone(),
                description: c.description.clone(),
                auth: c.auth.word(),
                accounts: accounts
                    .iter()
                    .filter(|a| a.connector == c.id)
                    .map(|a| (a.id, a.label.clone(), a.default))
                    .collect(),
                operations: c
                    .operations
                    .iter()
                    .map(|o| RosterOp {
                        id: o.id.clone(),
                        name: o.name.clone(),
                        description: o.description.clone(),
                        method: o.method.as_str().to_string(),
                        writes: o.writes,
                        params: o
                            .params
                            .iter()
                            .map(|p| {
                                (
                                    p.name.to_string(),
                                    kind_of(p.kind).as_str().to_string(),
                                    p.required,
                                )
                            })
                            .collect(),
                        select: o.output.select.clone(),
                    })
                    .collect(),
            })
            .collect();
        out.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        Self { connectors: out }
    }

    /// The roster of this workspace as it stands.
    pub fn of(ws: &Workspace) -> Result<Self, bisa_store::StoreError> {
        Ok(Self::from_parts(
            &ws.list_connectors()?,
            &ws.list_all_connector_accounts()?,
        ))
    }

    pub fn is_empty(&self) -> bool {
        self.connectors.is_empty()
    }

    /// Does the roster hold this operation of this connector?
    pub fn holds(&self, connector: &ConnectorId, operation: &OperationId) -> bool {
        self.connectors
            .iter()
            .any(|c| &c.id == connector && c.operations.iter().any(|o| &o.id == operation))
    }

    /// The roster as a prompt reads it: one line per connector, one per
    /// operation, under a `CONNECTORS` heading that says how to name them —
    /// or the sentence for none installed.
    pub fn render(&self) -> String {
        if self.is_empty() {
            return "CONNECTORS — none is installed here. Design no connector step; a person \
                    installs one under Settings › Capabilities › Connectors."
                .to_string();
        }
        let mut out = String::from(
            "CONNECTORS — installed here; a step reaches one as {\"kind\":\"connector\",\
             \"connector\":\"<id>\",\"operation\":\"<op>\",\"params\":{<name>: \"<template>\"}}. \
             Only these exist; an account is optional when one is marked default.\n",
        );
        for c in &self.connectors {
            let accounts = if c.accounts.is_empty() {
                "none".to_string()
            } else {
                c.accounts
                    .iter()
                    .map(|(_, label, default)| {
                        if *default {
                            format!("{label} (default)")
                        } else {
                            label.clone()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            out.push_str(&format!(
                "- {} \"{}\"{} [auth: {}; accounts: {}]\n",
                c.id,
                c.name,
                first_sentence(&c.description),
                c.auth,
                accounts
            ));
            for o in &c.operations {
                let mut facts = vec![o.method.clone()];
                if !o.params.is_empty() {
                    facts.push(format!(
                        "params: {}",
                        o.params
                            .iter()
                            .map(|(n, k, r)| format!(
                                "{n} ({k}{})",
                                if *r { ", required" } else { "" }
                            ))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                if let Some(s) = &o.select {
                    facts.push(format!("selects {s}"));
                }
                if o.writes {
                    facts.push("writes".to_string());
                }
                out.push_str(&format!(
                    "    · {} \"{}\"{} [{}]\n",
                    o.id,
                    o.name,
                    first_sentence(&o.description),
                    facts.join("; ")
                ));
            }
        }
        out
    }
}

/// ` — <first sentence>`, cut at 120 characters; nothing for no description.
fn first_sentence(text: &str) -> String {
    let text = text.trim();
    if text.is_empty() {
        return String::new();
    }
    let sentence = text
        .split_inclusive(['.', '!', '?'])
        .next()
        .unwrap_or(text)
        .trim();
    let cut: String = if sentence.chars().count() > 120 {
        let mut s: String = sentence.chars().take(117).collect();
        s.push('…');
        s
    } else {
        sentence.to_string()
    };
    format!(" — {cut}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_core::{OutputSpec, ParamDef, Tags};

    fn slack() -> Connector {
        Connector {
            id: ConnectorId::new("slack").unwrap(),
            name: "Slack".into(),
            description: "Slack, the chat. Everything else.".into(),
            tags: Tags::default(),
            origin: bisa_core::Origin::Local,
            base_url: "https://slack.com".into(),
            hosts: vec!["slack.com".into()],
            insecure_tls: false,
            auth: AuthScheme::Bearer,
            params: vec![],
            operations: vec![Operation {
                id: OperationId::new("post_message").unwrap(),
                name: "Post a message".into(),
                description: "Posts into a channel.".into(),
                method: HttpMethod::Post,
                path: "/api/chat.postMessage".into(),
                query: BTreeMap::new(),
                headers: BTreeMap::new(),
                body: Some(OperationBody::Json {
                    value: serde_json::json!({"channel": "{params.channel}", "text": "{params.text}"}),
                }),
                params: vec![ParamDef {
                    name: bisa_core::InputName::new("text").unwrap(),
                    label: "Text".into(),
                    kind: ParamKind::Text,
                    required: true,
                    doc: "The message.".into(),
                }],
                output: OutputSpec {
                    select: Some("ts".into()),
                    schema: None,
                },
                writes: true,
                timeout_secs: None,
                idempotency: None,
                page: None,
            }],
            check: None,
            created_at: 0,
        }
    }

    /// The crate's call shape is the definition, field for field.
    #[test]
    fn call_spec_carries_the_operation_whole() {
        let c = slack();
        let spec = call_spec(&c, &c.operations[0], Duration::from_secs(9));
        assert_eq!(spec.connector, "slack");
        assert_eq!(spec.operation, "post_message");
        assert_eq!(spec.base_url, "https://slack.com");
        assert_eq!(spec.hosts, vec!["slack.com".to_string()]);
        assert_eq!(spec.auth, cx::AuthSpec::Bearer);
        assert_eq!(spec.method, cx::Method::Post);
        assert_eq!(spec.path, "/api/chat.postMessage");
        assert!(
            matches!(&spec.body, Some(cx::CallBody::Json { value }) if value["channel"] == "{params.channel}"),
            "the body maps kind for kind: {:?}",
            spec.body
        );
        assert_eq!(spec.params.len(), 1);
        assert_eq!(spec.params[0].name, "text");
        assert_eq!(spec.params[0].kind, cx::ParamKind::Text);
        assert!(spec.params[0].required);
        assert_eq!(spec.select.as_deref(), Some("ts"));
        assert!(spec.writes);
        assert_eq!(spec.timeout, Duration::from_secs(9));
        let oauth = AuthScheme::OAuth2 {
            authorization_url: "https://a/auth".into(),
            token_url: "https://a/token".into(),
            scopes: vec!["read".into()],
            pkce: true,
            extra: BTreeMap::from([("access_type".to_string(), "offline".to_string())]),
        };
        assert!(matches!(
            auth_spec(&oauth),
            cx::AuthSpec::OAuth2 { pkce: true, .. }
        ));
    }

    /// The roster names every connector, its accounts by label and every
    /// operation with its parameters — and never a value.
    #[test]
    fn the_roster_renders_accounts_by_label_and_operations_with_params() {
        let account = ConnectorAccount {
            id: AccountId::from_ulid(ulid::Ulid::from_parts(1, 1)),
            connector: ConnectorId::new("slack").unwrap(),
            label: "work".into(),
            params: BTreeMap::new(),
            default: true,
            auth: Default::default(),
            created_at: 0,
        };
        let roster = ConnectorRoster::from_parts(&[slack()], &[account]);
        assert!(roster.holds(
            &ConnectorId::new("slack").unwrap(),
            &OperationId::new("post_message").unwrap()
        ));
        assert!(!roster.holds(
            &ConnectorId::new("slack").unwrap(),
            &OperationId::new("nope").unwrap()
        ));
        let text = roster.render();
        assert!(text.starts_with("CONNECTORS — installed here"), "{text}");
        assert!(
            text.contains(
                "- slack \"Slack\" — Slack, the chat. [auth: bearer; accounts: work (default)]"
            ),
            "{text}"
        );
        assert!(text.contains("· post_message \"Post a message\" — Posts into a channel. [POST; params: text (text, required); selects ts; writes]"), "{text}");
        assert!(ConnectorRoster::default()
            .render()
            .starts_with("CONNECTORS — none is installed here"));
    }
}
