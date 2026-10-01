//! The wire: a request as this crate builds it, a response as the transport
//! read it, and the one transport over `reqwest`. A test substitutes a
//! transport and nothing leaves the machine.

use crate::spec::Method;
use async_trait::async_trait;
use bisa_http::Clients;
use std::fmt;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use url::Url;

/// A request ready to send. Its `Debug` hides every credential-carrying
/// header and query pair, so a request can be logged without leaking.
#[derive(Clone)]
pub struct Request {
    pub method: Method,
    pub url: Url,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub timeout: Duration,
    /// Header names whose values are credentials — hidden in `Debug`.
    pub hidden_headers: Vec<String>,
    /// A query parameter carrying a credential — hidden in `Debug`.
    pub secret_query: Option<String>,
}

impl Request {
    pub fn new(method: Method, url: Url, timeout: Duration) -> Self {
        Self {
            method,
            url,
            headers: Vec::new(),
            body: None,
            timeout,
            hidden_headers: vec!["authorization".into()],
            secret_query: None,
        }
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// The URL with a credential-carrying query value replaced by `…`.
    pub fn shown_url(&self) -> String {
        let Some(secret) = self.secret_query.as_deref() else {
            return self.url.to_string();
        };
        let mut shown = self.url.clone();
        let pairs: Vec<(String, String)> = self
            .url
            .query_pairs()
            .map(|(k, v)| {
                let v = if k == secret {
                    "…".to_string()
                } else {
                    v.into_owned()
                };
                (k.into_owned(), v)
            })
            .collect();
        shown.query_pairs_mut().clear().extend_pairs(pairs);
        shown.to_string()
    }
}

impl fmt::Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let headers: Vec<(&str, &str)> = self
            .headers
            .iter()
            .map(|(k, v)| {
                let hidden = self
                    .hidden_headers
                    .iter()
                    .any(|h| h.eq_ignore_ascii_case(k));
                (k.as_str(), if hidden { "…" } else { v.as_str() })
            })
            .collect();
        f.debug_struct("Request")
            .field("method", &self.method.as_str())
            .field("url", &self.shown_url())
            .field("headers", &headers)
            .field("body_bytes", &self.body.as_ref().map_or(0, Vec::len))
            .field("timeout", &self.timeout)
            .finish()
    }
}

/// What came back.
#[derive(Clone, Debug)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// Why a request produced no response at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransportError {
    Connect(String),
    Timeout,
    Body(String),
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportError::Connect(why) => write!(f, "cannot connect: {why}"),
            TransportError::Timeout => f.write_str("timed out"),
            TransportError::Body(why) => write!(f, "cannot read the answer: {why}"),
        }
    }
}

/// One request, one response. `insecure_loopback` asks for the client that
/// accepts a self-signed certificate; a transport may only honour it for a
/// loopback host, which [`crate::hosts::check`] has already established.
#[async_trait]
pub trait HttpTransport: Send + Sync {
    async fn send(
        &self,
        req: &Request,
        insecure_loopback: bool,
    ) -> Result<Response, TransportError>;
}

/// The real transport over the engine's clients (`bisa-http`): the
/// `strict` one — no redirects, since a redirect could leave the declared
/// host — for every outside platform, under the proxy and the HTTP version
/// the person chose, swapped under every call when they change; and a
/// second client, built once, for a loopback service with its own
/// certificate — never a proxy on the way to this machine.
pub struct ReqwestTransport {
    http: Arc<Clients>,
    insecure: OnceLock<Arc<reqwest::Client>>,
}

impl Default for ReqwestTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl ReqwestTransport {
    /// Over the process's shared clients — a test's, or a caller with no
    /// engine behind it.
    pub fn new() -> Self {
        Self::with_http(Clients::shared())
    }

    /// Over the engine's clients.
    pub fn with_http(http: Arc<Clients>) -> Self {
        Self {
            http,
            insecure: OnceLock::new(),
        }
    }

    fn client(&self, insecure_loopback: bool) -> Arc<reqwest::Client> {
        if insecure_loopback {
            Arc::clone(self.insecure.get_or_init(|| {
                Arc::new(
                    Clients::loopback_builder()
                        .redirect(reqwest::redirect::Policy::none())
                        .danger_accept_invalid_certs(true)
                        .build()
                        .unwrap_or_default(),
                )
            }))
        } else {
            self.http.strict()
        }
    }
}

#[async_trait]
impl HttpTransport for ReqwestTransport {
    async fn send(
        &self,
        req: &Request,
        insecure_loopback: bool,
    ) -> Result<Response, TransportError> {
        let method = reqwest::Method::from_bytes(req.method.as_str().as_bytes())
            .map_err(|e| TransportError::Connect(e.to_string()))?;
        let mut builder = self
            .client(insecure_loopback)
            .request(method, req.url.clone())
            .timeout(req.timeout);
        for (k, v) in &req.headers {
            builder = builder.header(k, v);
        }
        if let Some(body) = &req.body {
            builder = builder.body(body.clone());
        }
        let mut resp = builder.send().await.map_err(|e| {
            if e.is_timeout() {
                TransportError::Timeout
            } else {
                // reqwest's message can carry the URL, and the URL can carry
                // a credential in its query; the error names the cause only.
                TransportError::Connect(cause_of(&e))
            }
        })?;
        let status = resp.status().as_u16();
        let headers = resp
            .headers()
            .iter()
            .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_string())))
            .collect();
        // Frame by frame, and no further than the cap plus one byte: the
        // reader (`outcome::parse`) judges `TooLarge` by length, and a
        // service that streams gigabytes is stopped after the first byte
        // past the cap rather than buffered whole and then measured.
        let mut body = Vec::new();
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| TransportError::Body(cause_of(&e)))?
        {
            let room = (crate::outcome::MAX_BODY_BYTES + 1).saturating_sub(body.len());
            body.extend_from_slice(&chunk[..chunk.len().min(room)]);
            if body.len() > crate::outcome::MAX_BODY_BYTES {
                break;
            }
        }
        Ok(Response {
            status,
            headers,
            body,
        })
    }
}

/// The innermost cause, without the URL reqwest prints beside it.
fn cause_of(e: &reqwest::Error) -> String {
    let mut source: &dyn std::error::Error = e;
    while let Some(next) = source.source() {
        source = next;
    }
    let text = source.to_string();
    if std::ptr::eq(source, e as &dyn std::error::Error) || text.is_empty() {
        if e.is_connect() {
            "connection refused".to_string()
        } else if e.is_request() {
            "the request could not be sent".to_string()
        } else {
            "request failed".to_string()
        }
    } else {
        text
    }
}
