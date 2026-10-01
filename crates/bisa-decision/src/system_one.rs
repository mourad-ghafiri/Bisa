//! A System One model over its HTTP wire: `POST <endpoint>/v1/systemone` with
//! `{ state, model, questions }`, answered with the contract's own shape.
//!
//! Jev is this provider at TypeSafe AI's endpoint with a bearer key; any other
//! model trained for calibrated decisions is this provider at an endpoint a
//! person named. The endpoint's host passes the same rules a connector's does:
//! `https`, or `http` to this machine alone, and the node's own host lists.

use crate::provider::{DecisionProvider, ProviderDescriptor, ProviderError};
use async_trait::async_trait;
use bisa_connectors::{
    hosts, retry, Clock, HostJudge, HttpTransport, Method, Request, Response, Secret,
};
use bisa_core::{DecisionProviderKind, DecisionRequest, DecisionResponse};
use std::sync::Arc;
use std::time::Duration;
use url::Url;

/// TypeSafe AI's API, where Jev answers.
pub const JEV_ENDPOINT: &str = "https://api.typesafe.ai";
/// The one path of the System One wire, under an endpoint.
pub const SYSTEM_ONE_PATH: &str = "v1/systemone";

/// The most of a refusal's body kept in an error.
const MAX_REFUSAL_CHARS: usize = 300;

pub struct SystemOneProvider {
    kind: DecisionProviderKind,
    url: Url,
    model: String,
    key: Option<Secret>,
    transport: Arc<dyn HttpTransport>,
    clock: Arc<dyn Clock>,
}

impl SystemOneProvider {
    /// A provider for `endpoint`, refused here — before any request — when the
    /// endpoint is no URL, names no model, or is a host the node will not reach.
    pub fn new(
        kind: DecisionProviderKind,
        endpoint: &str,
        model: &str,
        key: Option<Secret>,
        transport: Arc<dyn HttpTransport>,
        judge: &dyn HostJudge,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, ProviderError> {
        let model = model.trim();
        if model.is_empty() {
            return Err(ProviderError::Misconfigured("no model is named".into()));
        }
        let url = system_one_url(endpoint)?;
        let host = hosts::host_of(&url)
            .ok_or_else(|| ProviderError::Misconfigured(format!("`{endpoint}` names no host")))?;
        hosts::check(&url, std::slice::from_ref(&host), false)
            .map_err(|e| ProviderError::Misconfigured(e.to_string()))?;
        judge.judge(&host).map_err(ProviderError::Misconfigured)?;
        Ok(Self {
            kind,
            url,
            model: model.to_string(),
            key,
            transport,
            clock,
        })
    }

    fn read(&self, response: Response) -> Result<DecisionResponse, ProviderError> {
        let status = response.status;
        if (200..300).contains(&status) {
            return serde_json::from_slice(&response.body)
                .map_err(|e| ProviderError::Unreadable(e.to_string()));
        }
        if status == 408 || status == 429 || status >= 500 {
            return Err(ProviderError::Busy {
                status,
                retry_after: retry::retry_after(&response.headers, self.clock.now()),
            });
        }
        Err(ProviderError::Refused {
            status,
            message: self.refusal(&response.body),
        })
    }

    /// What the service said, short, and never with the key in it.
    fn refusal(&self, body: &[u8]) -> String {
        let text = String::from_utf8_lossy(body);
        let text = match &self.key {
            Some(key) => text.replace(key.expose(), "…"),
            None => text.into_owned(),
        };
        text.trim().chars().take(MAX_REFUSAL_CHARS).collect()
    }
}

/// `<endpoint>/v1/systemone`, whatever slash the endpoint ends with.
fn system_one_url(endpoint: &str) -> Result<Url, ProviderError> {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        return Err(ProviderError::Misconfigured("no endpoint is named".into()));
    }
    let base = format!("{}/", endpoint.trim_end_matches('/'));
    Url::parse(&base)
        .and_then(|b| b.join(SYSTEM_ONE_PATH))
        .map_err(|e| ProviderError::Misconfigured(format!("`{endpoint}` is not a URL: {e}")))
}

#[async_trait]
impl DecisionProvider for SystemOneProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            kind: self.kind,
            model: self.model.clone(),
        }
    }

    async fn decide(
        &self,
        request: &DecisionRequest,
        deadline: Duration,
    ) -> Result<DecisionResponse, ProviderError> {
        let body = serde_json::json!({
            "state": request.state,
            "model": self.model,
            "questions": request.questions,
        });
        let mut http = Request::new(Method::Post, self.url.clone(), deadline);
        http.headers
            .push(("content-type".into(), "application/json".into()));
        http.headers
            .push(("accept".into(), "application/json".into()));
        if let Some(key) = &self.key {
            http.headers
                .push(("authorization".into(), format!("Bearer {}", key.expose())));
        }
        http.body =
            Some(serde_json::to_vec(&body).map_err(|e| ProviderError::Unreadable(e.to_string()))?);
        let insecure = false;
        let response = self
            .transport
            .send(&http, insecure)
            .await
            .map_err(|e| ProviderError::Unreachable(e.to_string()))?;
        self.read(response)
    }
}
