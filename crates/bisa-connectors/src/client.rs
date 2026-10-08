//! One call, start to finish: bind, render, refuse a host, ask the host's
//! circuit, resolve the credential, apply it, send with the retry policy,
//! follow the pages, read the answer, and scrub every word that leaves.

use crate::auth::{self, SignContext};
use crate::breaker::Breaker;
use crate::creds::{AccountRef, Clock, Credential, Credentials, Entropy, Secret, Stored};
use crate::error::ConnectorError;
use crate::files::{FileData, Files, NoFiles, MAX_FILE_BYTES};
use crate::hosts::{self, HostJudge};
use crate::http::{HttpTransport, Request, Response, TransportError};
use crate::outcome::{self, Outcome};
use crate::request;
use crate::retry::{self, Next, RetryPolicy};
use crate::spec::{AuthSpec, CallSpec, ParamKind};
use crate::template::Values;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A token endpoint's deadline, which is not the operation's.
const OAUTH_TIMEOUT: Duration = Duration::from_secs(30);

pub struct Client {
    transport: Arc<dyn HttpTransport>,
    creds: Arc<dyn Credentials>,
    clock: Arc<dyn Clock>,
    entropy: Arc<dyn Entropy>,
    retry: RetryPolicy,
    refresh_locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    /// One circuit per host, across every call this client makes.
    breaker: Breaker,
}

impl Client {
    pub fn new(
        transport: Arc<dyn HttpTransport>,
        creds: Arc<dyn Credentials>,
        clock: Arc<dyn Clock>,
        entropy: Arc<dyn Entropy>,
    ) -> Self {
        Self {
            transport,
            creds,
            clock,
            entropy,
            retry: RetryPolicy::default(),
            refresh_locks: Mutex::new(HashMap::new()),
            breaker: Breaker::default(),
        }
    }

    /// The hosts' circuits, for a caller that wants to say a host is paused.
    pub fn breaker(&self) -> &Breaker {
        &self.breaker
    }

    pub fn creds(&self) -> &dyn Credentials {
        self.creds.as_ref()
    }

    pub fn entropy(&self) -> &dyn Entropy {
        self.entropy.as_ref()
    }

    pub fn now(&self) -> u64 {
        self.clock.now()
    }

    pub(crate) fn oauth_timeout(&self) -> Duration {
        OAUTH_TIMEOUT
    }

    /// The one lock per account a refresh is taken under. A lock nobody
    /// holds any more is let go on the way, so the table is the accounts
    /// refreshing right now, never every account ever refreshed.
    pub(crate) fn refresh_lock(&self, account: &AccountRef) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.refresh_locks.lock().unwrap_or_else(|p| p.into_inner());
        locks.retain(|_, lock| Arc::strong_count(lock) > 1);
        Arc::clone(locks.entry(account.key()).or_default())
    }

    /// Send through the host's circuit: refused while it is open, counted
    /// after — a transport error, a timeout or a 5xx is the host's failure;
    /// anything else closes the circuit. A call dropped before it answers —
    /// a stopped run, a deadline above this one — settles nothing and lets
    /// go of the probe it may have been.
    pub(crate) async fn send_judged(
        &self,
        req: &Request,
        writes: bool,
        insecure_loopback: bool,
        host: &str,
    ) -> Result<Response, ConnectorError> {
        self.breaker.admit(host, self.clock.now())?;
        let admission = Admission {
            breaker: &self.breaker,
            host,
            settled: false,
        };
        let result = self.send_with_retry(req, writes, insecure_loopback).await;
        let ok = matches!(&result, Ok(resp) if resp.status < 500);
        admission.settle(ok, self.clock.now());
        result
    }

    /// Send under the retry policy; the deadline is the request's own and
    /// covers every attempt and every wait.
    pub(crate) async fn send_with_retry(
        &self,
        req: &Request,
        writes: bool,
        insecure_loopback: bool,
    ) -> Result<Response, ConnectorError> {
        let started = Instant::now();
        let mut attempt: u8 = 0;
        loop {
            let left = req
                .timeout
                .checked_sub(started.elapsed())
                .unwrap_or(Duration::ZERO);
            if left.is_zero() {
                return Err(ConnectorError::Timeout(req.timeout));
            }
            let mut this = req.clone();
            this.timeout = left;
            let result = self.transport.send(&this, insecure_loopback).await;
            let retry_after = result
                .as_ref()
                .ok()
                .and_then(|r| retry::retry_after(&r.headers, self.clock.now()));
            let mut jitter = [0u8; 1];
            self.entropy.fill(&mut jitter);
            let next = retry::decide(
                &self.retry,
                attempt,
                &result,
                writes,
                retry_after,
                f64::from(jitter[0]) / 255.0,
            );
            match next {
                Next::RetryAfter(wait) => {
                    let left = req
                        .timeout
                        .checked_sub(started.elapsed())
                        .unwrap_or(Duration::ZERO);
                    if wait >= left {
                        return match result {
                            Ok(resp) => Ok(resp),
                            Err(e) => Err(transport_error(e, req.timeout)),
                        };
                    }
                    tracing::debug!(
                        attempt,
                        wait_ms = wait.as_millis() as u64,
                        "retrying a connector call"
                    );
                    tokio::time::sleep(wait).await;
                    attempt += 1;
                }
                Next::Done => {
                    return match result {
                        Ok(resp) => Ok(resp),
                        Err(e) => Err(transport_error(e, req.timeout)),
                    }
                }
            }
        }
    }

    /// Run one operation as `account` with the step's rendered parameters,
    /// for a call that has no checkout — a poll, an account check. A `file`
    /// parameter given here is refused by name.
    pub async fn call(
        &self,
        spec: &CallSpec,
        account: Option<&AccountRef>,
        account_params: &BTreeMap<String, Value>,
        params: &BTreeMap<String, String>,
        judge: &dyn HostJudge,
    ) -> Result<Outcome, ConnectorError> {
        self.call_with(spec, account, account_params, params, &NoFiles, judge)
            .await
    }

    /// Run one operation as `account` with the step's rendered parameters;
    /// a `file` parameter's path is read through `files` before the request
    /// is built.
    pub async fn call_with(
        &self,
        spec: &CallSpec,
        account: Option<&AccountRef>,
        account_params: &BTreeMap<String, Value>,
        params: &BTreeMap<String, String>,
        files: &dyn Files,
        judge: &dyn HostJudge,
    ) -> Result<Outcome, ConnectorError> {
        let mut exposed: Vec<String> = Vec::new();
        let result = self
            .call_inner(
                spec,
                account,
                account_params,
                params,
                files,
                judge,
                &mut exposed,
            )
            .await;
        let refs: Vec<&str> = exposed.iter().map(String::as_str).collect();
        result.map_err(|e| e.scrubbed(&refs))
    }

    /// The bytes of every `file` parameter the step set, by name; a path the
    /// port refuses, or a file past the cap, is the parameter's fault.
    async fn read_files(
        spec: &CallSpec,
        bound: &BTreeMap<String, Value>,
        files: &dyn Files,
    ) -> Result<BTreeMap<String, FileData>, ConnectorError> {
        let mut out = BTreeMap::new();
        for p in spec.params.iter().filter(|p| p.kind == ParamKind::File) {
            let Some(Value::String(path)) = bound.get(&p.name) else {
                continue;
            };
            let data = files
                .read(path)
                .await
                .map_err(|why| ConnectorError::BadParam {
                    name: p.name.clone(),
                    why,
                })?;
            if data.bytes.len() > MAX_FILE_BYTES {
                return Err(ConnectorError::BadParam {
                    name: p.name.clone(),
                    why: format!(
                        "{} bytes; a file is read up to {} MiB",
                        data.bytes.len(),
                        MAX_FILE_BYTES / (1024 * 1024)
                    ),
                });
            }
            out.insert(p.name.clone(), data);
        }
        Ok(out)
    }

    #[allow(clippy::too_many_arguments)]
    async fn call_inner(
        &self,
        spec: &CallSpec,
        account: Option<&AccountRef>,
        account_params: &BTreeMap<String, Value>,
        params: &BTreeMap<String, String>,
        files: &dyn Files,
        judge: &dyn HostJudge,
        exposed: &mut Vec<String>,
    ) -> Result<Outcome, ConnectorError> {
        let bound = request::bind_params(&spec.params, params)?;
        let file_data = Self::read_files(spec, &bound, files).await?;

        // The credential is resolved once for every page.
        let (stored, account) = match (&spec.auth, account) {
            (AuthSpec::None, _) => (Stored::default(), None),
            (_, None) => {
                return Err(ConnectorError::NotAuthenticated(
                    "no account is set for this connector".into(),
                ))
            }
            (_, Some(a)) => (self.creds.load(a).await?, Some(a)),
        };
        let mut credential = auth::credential_for(&spec.auth, &stored)?;
        if let (AuthSpec::OAuth2 { .. }, Some(a)) = (&spec.auth, account) {
            credential = Credential::OAuth2(
                crate::oauth::ensure_fresh(self, &spec.auth, a, &stored, judge).await?,
            );
        }
        let sign = SignContext {
            now: self.clock.now(),
            account: account_params,
        };

        // One request per page; the first carries no cursor, the rest carry
        // the one the page before named. An operation that does not page is
        // one page.
        let mut pages: Vec<Value> = Vec::new();
        let mut cursor: Option<String> = None;
        let mut read: u8 = 0;
        loop {
            let mut bound_page = bound.clone();
            if let (Some(paging), Some(c)) = (&spec.page, &cursor) {
                bound_page.insert(paging.cursor_param.clone(), Value::String(c.clone()));
            }
            let values = Values {
                account: account_params,
                params: &bound_page,
            };
            let mut req = request::build_request(spec, &values, &file_data, self.entropy.as_ref())?;
            hosts::check(&req.url, &spec.hosts, spec.insecure_tls)?;
            let host = hosts::host_of(&req.url).unwrap_or_default();
            judge
                .judge(&host)
                .map_err(|reason| ConnectorError::HostRefused {
                    host: host.clone(),
                    allowed: reason,
                })?;
            let insecure =
                spec.insecure_tls && hosts::is_loopback(req.url.host_str().unwrap_or_default());
            let applied = auth::apply(&spec.auth, &credential, &mut req, &sign)?;
            exposed.extend(applied.secrets);

            let mut resp = self.send_judged(&req, spec.writes, insecure, &host).await?;
            // A 401 under OAuth2 with a refresh token: the token died before its
            // expiry said so. One refresh, one resend, then the answer stands.
            if resp.status == 401 {
                if let (AuthSpec::OAuth2 { .. }, Some(a)) = (&spec.auth, account) {
                    if stored.field(crate::creds::Field::RefreshToken).is_some() {
                        let token: Secret =
                            crate::oauth::force_refresh(self, &spec.auth, a, judge).await?;
                        credential = Credential::OAuth2(token);
                        let mut again = request::build_request(
                            spec,
                            &values,
                            &file_data,
                            self.entropy.as_ref(),
                        )?;
                        let applied = auth::apply(&spec.auth, &credential, &mut again, &sign)?;
                        exposed.extend(applied.secrets);
                        resp = self
                            .send_judged(&again, spec.writes, insecure, &host)
                            .await?;
                    }
                }
            }
            let outcome = outcome::parse(
                &resp,
                spec.select.as_deref(),
                spec.expect.as_ref(),
                self.clock.now(),
            )?;
            let Some(paging) = &spec.page else {
                return Ok(outcome);
            };
            read += 1;
            let Value::Array(items) = outcome.selected else {
                return Err(ConnectorError::BadDefinition(format!(
                    "operation {} pages, but what it selects is not a list",
                    spec.operation
                )));
            };
            pages.extend(items);
            let next = outcome::next_cursor(&outcome.body, &paging.next_cursor);
            match next {
                Some(c) if read < paging.max_pages => cursor = Some(c),
                _ => {
                    return Ok(Outcome {
                        status: outcome.status,
                        body: outcome.body,
                        selected: Value::Array(pages),
                        pages: read,
                    })
                }
            }
        }
    }
}

/// A call admitted through a host's circuit, until its answer is recorded.
/// Dropped unsettled — the future cancelled mid-send — it abandons the probe
/// the call may have been, so the circuit never waits on nobody.
struct Admission<'a> {
    breaker: &'a Breaker,
    host: &'a str,
    settled: bool,
}

impl Admission<'_> {
    fn settle(mut self, ok: bool, now: u64) {
        self.breaker.record(self.host, ok, now);
        self.settled = true;
    }
}

impl Drop for Admission<'_> {
    fn drop(&mut self) {
        if !self.settled {
            self.breaker.abandon(self.host);
        }
    }
}

/// A connect failure never reached the platform — safe to send again; a
/// timeout or a failure mid-body may have: the caller tells them apart.
fn transport_error(e: TransportError, timeout: Duration) -> ConnectorError {
    match e {
        TransportError::Timeout => ConnectorError::Timeout(timeout),
        TransportError::Connect(why) => ConnectorError::Unreachable(why),
        other => ConnectorError::Transport(other.to_string()),
    }
}
