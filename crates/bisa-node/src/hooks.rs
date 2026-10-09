//! `POST /hooks/{host}/{step}` — the door a caller outside this machine
//! begins a run through — and what both hook doors share.
//!
//! # Two doors, one listener
//!
//! A `hook` start is a listener like any other: a start step armed for a host
//! — a library workflow that is On (`workspace:<WorkflowId>`) or a goal that
//! listens (`goal:<GoalId>`). It is called through one of two doors:
//!
//! - **locally**, `POST /workflows/{wfid}/hooks/{step}` and
//!   `POST /goals/{id}/hooks/{step}` (`listening.rs`), under the
//!   control-plane token like every other route — a script on this machine;
//! - **publicly**, here, by whoever holds the listener's own secret — and
//!   only when the start says `public = true` **and** this machine allows
//!   public hooks (`events.public_hooks`, off until a person turns it on).
//!
//! # Reachability is the operator's decision, not this module's
//!
//! The node binds a unix socket (0600) plus, on request, one loopback TCP
//! address. This route changes none of that: it mounts on the listener that
//! already exists and opens nothing. A webhook from a code host, a CI runner
//! or a cron box on another machine reaches it only if **you** put a tunnel
//! or a reverse proxy in front of the node, and that is deliberately a
//! decision Bisa does not take for you.
//!
//! # What the route does, and the much longer list of what it does not
//!
//! Its only effect is to **enqueue a signal**
//! ([`bisa_engine::Engine::call_hook`]). It never starts a run, never blocks
//! on one, and never tells the caller what a run did — a caller learns that
//! its POST was accepted and a signal id, and nothing about the goal, the
//! agent or the outcome. That is what makes a hostile flood cost at most the
//! rate limit: the expensive half of the system is on the other side of a
//! durable queue, under the same gates, budgets and concurrency cap as a
//! person's start. The engine redacts a public body before it is stored and
//! holds it for the content screen.
//!
//! # Authentication
//!
//! Two forms, either sufficient, both compared in constant time against the
//! listener's secret in the keystore (never in a record):
//!
//! | Header | Value |
//! |---|---|
//! | `X-Bisa-Token` | the secret exactly as it was shown (64 hex chars) |
//! | `X-Hub-Signature-256` | `sha256=<hmac-sha256(body)>`, GitHub-compatible |
//!
//! The HMAC key is **the secret's printed 64-character hex form, as ASCII** —
//! paste what Bisa showed you into a code host's *Secret* field and its
//! signature verifies unchanged. Signing over the raw bytes that arrived (not
//! a re-serialization) is what makes that true.
//!
//! # Answers, and why the refusals look alike
//!
//! | Condition | Status |
//! |---|---|
//! | `events.public_hooks` or `events.enabled` is off on this machine | **404**, whoever asks |
//! | body over [`MAX_SIGNAL_PAYLOAD_BYTES`] | **413** |
//! | the rate limit is spent, or the backlog is full | **429** (`Retry-After` for the rate limit) |
//! | unknown listener, no secret minted, a missing or wrong token or HMAC | **401**, one body |
//! | authenticated, but the host is Off or paused | **403** |
//! | authenticated, but the start is not public, or is no hook start any more | **404** |
//! | accepted | **202** `{"signal": "<id>"}` |
//!
//! **An unknown listener and a bad secret are one response.** If a missing
//! listener answered 404 the route would be an oracle enumerating which
//! hosts and steps exist, which is exactly what an attacker holding no
//! secret wants. So a request for a listener that has no secret still runs
//! the full comparison — against a per-process decoy secret — and still
//! answers 401 with the same body, so neither the status, the body, nor the
//! constant-time comparison distinguishes the two. (Whether the keystore was
//! opened is a filesystem-level timing difference; equalising *that* is
//! beyond what a loopback control plane can reasonably promise, and is
//! stated rather than pretended.) The one 404 an unauthenticated caller can
//! see is the machine's switch, which says nothing about any listener.
//!
//! Rate limiting is keyed by the **requested** listener whether or not it is
//! real, for the same reason: a shared bucket for unknown ones would let an
//! attacker drain it and then read "429 vs 401" as "this listener exists".
//! The bucket table is bounded, and eviction prefers buckets that have
//! refilled to full — a full bucket is indistinguishable from no bucket, so
//! dropping it leaks nothing and a flood of invented names only churns its
//! own entries.
//!
//! What the engine refuses comes **after** authentication on purpose:
//! answering 403 to somebody already holding the listener's secret tells
//! them nothing they do not know, while answering it first would leak
//! existence to somebody holding nothing.
//!
//! # What the signal carries
//!
//! The payload is the body: a JSON object as it came, any other JSON value
//! under `value`, and a body that is not JSON at all under `text` — so a
//! start's input mapping always has a path to read. A delivery id — the
//! caller's `Idempotency-Key`, else `X-GitHub-Delivery` — makes a redelivery
//! the same signal: one occurrence, one run.

use crate::route_docs::RouteDoc;
use crate::{ApiError, Shared};
use axum::body::Body;
use axum::extract::{Path as AxPath, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use bisa_core::signal::MAX_SIGNAL_PAYLOAD_BYTES;
use bisa_core::ListenerKey;
use bisa_engine::{HookDoor, HookRefusal};
use hmac::{Hmac, Mac as _};
use serde_json::{json, Value};
use sha2::Sha256;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

/// Requests a single listener may absorb back-to-back.
const HOOK_BURST: f64 = 30.0;
/// Steady-state refill, tokens per second (30/minute).
const HOOK_REFILL_PER_SEC: f64 = 0.5;
/// Ceiling on tracked buckets. Bounded so a flood of invented listeners
/// cannot grow the table without limit.
const MAX_BUCKETS: usize = 4096;
/// The longest delivery id taken: a code host's is a UUID, a caller's own
/// key rarely longer. A longer one is no id, and the call is its own
/// occurrence.
const MAX_DELIVERY_ID_BYTES: usize = 128;
/// What the engine adds around a body that is no JSON object — the key it
/// files it under — so a body that fits the cap still fits once filed.
const WRAPPING_BYTES: usize = 16;

pub(crate) fn routes() -> Router<Shared> {
    Router::new().route("/hooks/{host}/{step}", post(hook))
}

// ---------------------------------------------------------------------------
// Per-listener token bucket
// ---------------------------------------------------------------------------

struct Bucket {
    tokens: f64,
    last: Instant,
}

/// The public hook route's process-lifetime state: the rate-limit table and
/// the decoy secret a listener with no secret is compared against.
pub(crate) struct Hooks {
    buckets: Mutex<HashMap<String, Bucket>>,
    /// 64 hex chars from the same CSPRNG that mints real hook secrets.
    /// It authenticates nothing — it exists so an unknown listener costs the
    /// same comparison a known one does.
    decoy: String,
}

impl Default for Hooks {
    fn default() -> Self {
        let bytes = nostr::key::Keys::generate().secret_key().to_secret_bytes();
        Self {
            buckets: Mutex::new(HashMap::new()),
            decoy: hex::encode(bytes),
        }
    }
}

impl Hooks {
    /// Spend one token for `key`. `Err(secs)` is the `Retry-After` to send.
    fn take(&self, key: &str) -> Result<(), u64> {
        let now = Instant::now();
        let mut map = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
        if !map.contains_key(key) && map.len() >= MAX_BUCKETS {
            evict(&mut map, now);
        }
        let bucket = map.entry(key.to_string()).or_insert(Bucket {
            tokens: HOOK_BURST,
            last: now,
        });
        let elapsed = now.duration_since(bucket.last).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * HOOK_REFILL_PER_SEC).min(HOOK_BURST);
        bucket.last = now;
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            Ok(())
        } else {
            let deficit = 1.0 - bucket.tokens;
            Err((deficit / HOOK_REFILL_PER_SEC).ceil().max(1.0) as u64)
        }
    }
}

/// Make room: drop buckets that have refilled to full first (a full bucket
/// says nothing a missing one does not), then the least recently used.
fn evict(map: &mut HashMap<String, Bucket>, now: Instant) {
    map.retain(|_, b| {
        let refilled = b.tokens + now.duration_since(b.last).as_secs_f64() * HOOK_REFILL_PER_SEC;
        refilled < HOOK_BURST
    });
    while map.len() >= MAX_BUCKETS {
        let Some(oldest) = map
            .iter()
            .min_by_key(|(_, b)| b.last)
            .map(|(k, _)| k.clone())
        else {
            break;
        };
        map.remove(&oldest);
    }
}

// ---------------------------------------------------------------------------
// Constant-time comparison
// ---------------------------------------------------------------------------

/// Byte equality in time independent of *where* the inputs differ.
///
/// The loop runs over the longer input, which is the caller-supplied one, so
/// its length leaks nothing about the secret; only the position of a mismatch
/// would matter, and that is exactly what is hidden.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    let mut diff = (a.len() ^ b.len()) as u32;
    let n = a.len().max(b.len());
    for i in 0..n {
        let x = *a.get(i).unwrap_or(&0);
        let y = *b.get(i).unwrap_or(&0);
        diff |= (x ^ y) as u32;
    }
    diff == 0
}

/// Verify a GitHub-shaped `sha256=<hex>` signature over the raw body.
///
/// `key` is the secret's printed hex form as ASCII — see the module docs.
/// The comparison is `Mac::verify_slice`, which is constant time.
fn verify_hmac(header: &str, body: &[u8], key: &[u8]) -> bool {
    let Some(hexsig) = header.trim().strip_prefix("sha256=") else {
        return false;
    };
    let Ok(provided) = hex::decode(hexsig.trim()) else {
        return false;
    };
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts a key of any length");
    mac.update(body);
    mac.verify_slice(&provided).is_ok()
}

/// Either accepted form authenticates. Both are attempted so a caller sending
/// both a wrong token and a right signature still gets in.
fn authenticate(headers: &HeaderMap, body: &[u8], secret_hex: &str) -> bool {
    let mut ok = false;
    if let Some(token) = headers.get("x-bisa-token").and_then(|v| v.to_str().ok()) {
        ok |= ct_eq(token.trim().as_bytes(), secret_hex.as_bytes());
    }
    if let Some(sig) = headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok())
    {
        ok |= verify_hmac(sig, body, secret_hex.as_bytes());
    }
    ok
}

// ---------------------------------------------------------------------------
// What both doors read off a call
// ---------------------------------------------------------------------------

/// A body over the signal payload cap.
#[derive(Debug)]
pub(crate) struct TooLarge;

/// Read at most `cap` bytes, refusing **while reading**.
///
/// The frame loop is the point: a 1 GB body is abandoned a few kilobytes in
/// rather than buffered and then measured.
async fn read_capped(body: Body, cap: usize) -> Result<Vec<u8>, TooLarge> {
    use futures::StreamExt as _;
    let mut buf: Vec<u8> = Vec::new();
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        let Ok(chunk) = chunk else {
            return Err(TooLarge);
        };
        if buf.len() + chunk.len() > cap {
            return Err(TooLarge);
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

/// A hook call's body, raw — the bytes a signature is over — and bounded by
/// the signal payload cap: by its declared length before a frame is read,
/// then frame by frame.
pub(crate) async fn read_body(headers: &HeaderMap, body: Body) -> Result<Vec<u8>, TooLarge> {
    let declared = headers
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok());
    if declared.is_some_and(|len| len > MAX_SIGNAL_PAYLOAD_BYTES) {
        return Err(TooLarge);
    }
    read_capped(body, MAX_SIGNAL_PAYLOAD_BYTES).await
}

/// What a body carries, as the engine takes it: JSON as it came, a body that
/// is not JSON as its text, an empty one as nothing. Refused when what the
/// engine would file is over the cap — a body of quotes doubles once it is
/// escaped.
pub(crate) fn payload_of(raw: &[u8]) -> Result<Value, TooLarge> {
    if raw.iter().all(u8::is_ascii_whitespace) {
        return Ok(Value::Null);
    }
    let payload = serde_json::from_slice::<Value>(raw)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(raw).into_owned()));
    let filed = serde_json::to_vec(&payload).map_or(usize::MAX, |bytes| bytes.len());
    if filed.saturating_add(WRAPPING_BYTES) > MAX_SIGNAL_PAYLOAD_BYTES {
        return Err(TooLarge);
    }
    Ok(payload)
}

/// What makes a redelivery the same occurrence: the caller's
/// `Idempotency-Key`, else a code host's `X-GitHub-Delivery`. An empty one,
/// or one longer than an id is, is none.
pub(crate) fn delivery_of(headers: &HeaderMap) -> Option<String> {
    ["idempotency-key", "x-github-delivery"]
        .into_iter()
        .find_map(|name| {
            let id = headers.get(name)?.to_str().ok()?.trim();
            (!id.is_empty() && id.len() <= MAX_DELIVERY_ID_BYTES).then(|| id.to_string())
        })
}

/// A refusal as a caller on this machine reads it: in words, with the status
/// that says whose move it is — the host is Off (409), the step is no hook
/// start of it (404), its backlog is full (429).
pub(crate) fn refused_locally(refusal: HookRefusal) -> ApiError {
    match refusal {
        HookRefusal::Disabled => ApiError::text(
            StatusCode::CONFLICT,
            bisa_core::text!("error-node-hooks-listening-off-on-this-machine"),
        ),
        HookRefusal::NotListening(host) => ApiError::text(
            StatusCode::CONFLICT,
            bisa_core::text!("error-node-hooks-host-not-listening", host = host),
        ),
        HookRefusal::NoSuchStart(listener) => ApiError::text(
            StatusCode::NOT_FOUND,
            bisa_core::text!("error-node-hooks-not-hook-start", listener = listener),
        ),
        // LCOV_EXCL_START: the public route verifies a hook's secret before the engine's own refusal; a start that takes no public calls has none, so the call is unauthorized first (a_goal_with_no_workflow_lists_no_listeners_and_a_local_hook_takes_no_public_call)
        HookRefusal::NotPublic(listener) => ApiError::text(
            StatusCode::NOT_FOUND,
            bisa_core::text!(
                "error-node-hooks-takes-no-public-calls",
                listener = listener // LCOV_EXCL_STOP
            ),
        ),
        HookRefusal::Busy(listener) => ApiError::text(
            StatusCode::TOO_MANY_REQUESTS,
            bisa_core::text!("error-node-hooks-backlog-full", listener = listener),
        ),
    }
}

/// A body over the cap, as a caller on this machine reads it.
pub(crate) fn too_large_locally() -> ApiError {
    ApiError::text(
        StatusCode::PAYLOAD_TOO_LARGE,
        bisa_core::text!(
            "error-node-hooks-body-over-cap",
            max = MAX_SIGNAL_PAYLOAD_BYTES.to_string()
        ),
    )
}

/// The call was taken: the signal written for it, and nothing about what it
/// will start.
pub(crate) fn accepted(signal: String) -> Response {
    (StatusCode::ACCEPTED, Json(json!({ "signal": signal }))).into_response()
}

// ---------------------------------------------------------------------------
// The public door
// ---------------------------------------------------------------------------

// The public door answers a system, never a person: its bodies are English,
// as the caller's own logs read them.

fn unauthorized() -> Response {
    // One body for every authentication outcome — see the module docs.
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({"error": "unauthorized"})),
    )
        .into_response()
}

fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({"error": "not found"})), // for the machine
    )
        .into_response()
}

fn too_large() -> Response {
    (
        StatusCode::PAYLOAD_TOO_LARGE,
        Json(json!({
            "error": format!("body exceeds the {MAX_SIGNAL_PAYLOAD_BYTES}-byte signal payload cap") // for the machine
        })),
    )
        .into_response()
}

fn rate_limited(retry: u64) -> Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        [(axum::http::header::RETRY_AFTER, retry.to_string())],
        Json(json!({"error": "rate limit exceeded", "retry_after": retry})), // for the machine
    )
        .into_response()
}

/// What the engine refused, said to a caller who holds the listener's
/// secret — and so learns nothing it could not already know.
fn refused_publicly(refusal: &HookRefusal) -> Response {
    match refusal {
        HookRefusal::NotListening(_) => (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "not listening"})), // for the machine
        )
            .into_response(),
        HookRefusal::Busy(_) => (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error": "backlog full"})), // for the machine
        )
            .into_response(),
        HookRefusal::Disabled | HookRefusal::NoSuchStart(_) | HookRefusal::NotPublic(_) => {
            not_found()
        }
    }
}

async fn hook(
    State(state): State<Shared>,
    AxPath((host, step)): AxPath<(String, String)>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    // 0. The door itself. Closed on this machine, it answers nobody
    //    anything — before a byte is read or a bucket is spent.
    let settings = state.engine.inner().listen.settings();
    if !(settings.enabled && settings.public_hooks) {
        return not_found();
    }

    // 1. Size. Declared length first, then enforced frame by frame.
    let Ok(raw) = read_body(&headers, body).await else {
        return too_large();
    };

    // 2. Rate limit, on the requested listener whether or not it is real.
    let asked = format!("{host}/{step}");
    if let Err(retry) = state.hooks.take(&asked) {
        return rate_limited(retry);
    }

    // 3. Authenticate. A listener with no secret — unknown, or not public —
    //    is compared against the decoy, so this costs the same either way
    //    and answers the same either way. Only a real secret lets a call in.
    let ws = state.engine.workspace();
    let known = asked
        .parse::<ListenerKey>()
        .ok()
        .and_then(|key| ws.hook_secret(&key).ok().map(|s| (key, hex::encode(s))));
    let secret_hex = known
        .as_ref()
        .map_or(state.hooks.decoy.as_str(), |(_, secret)| secret.as_str());
    let authed = authenticate(&headers, &raw, secret_hex);
    let Some((key, _)) = known.filter(|_| authed) else {
        return unauthorized();
    };

    // 4. Enqueue, and nothing else: the engine decides whether the listener
    //    takes the call, redacts the body and holds it for the content
    //    screen. No run starts on this task, no run's outcome comes back.
    let Ok(payload) = payload_of(&raw) else {
        return too_large();
    };
    let delivery = delivery_of(&headers);
    match state
        .engine
        .call_hook(&key, payload, delivery.as_deref(), HookDoor::Public)
    {
        Ok(signal) => accepted(signal),
        Err(refusal) => refused_publicly(&refusal),
    }
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test. Kept beside `routes()` so a route added here is added here.
pub const ROUTES: &[RouteDoc] = &[RouteDoc {
    method: "POST",
    path: "/hooks/{host}/{step}",
    summary: "A public `hook` start's inbound endpoint, outside the control plane — `{host}` is `workspace:<WorkflowId>` or `goal:<GoalId>`: authenticated by the listener's own secret (`X-Bisa-Token`, or `X-Hub-Signature-256` over the raw body), at most 64 KiB, rate-limited per listener → `202 {signal}`; `Idempotency-Key` or `X-GitHub-Delivery` makes a redelivery the same signal. `404` while `events.public_hooks` or `events.enabled` is off, `401` (one body) for every authentication failure, `403` for a host that is Off or paused, `404` for a start that is not public, `413`, `429`.",
}];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_equality_still_decides_correctly() {
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"ab"));
        assert!(!ct_eq(b"", b"a"));
        assert!(ct_eq(b"", b""));
    }

    /// RFC 4231 test case 2 — proof the HMAC is the standard one GitHub
    /// computes, not a lookalike.
    #[test]
    fn hmac_matches_the_rfc_vector() {
        let sig = {
            let mut mac = Hmac::<Sha256>::new_from_slice(b"Jefe").unwrap();
            mac.update(b"what do ya want for nothing?");
            hex::encode(mac.finalize().into_bytes())
        };
        assert_eq!(
            sig,
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        assert!(verify_hmac(
            &format!("sha256={sig}"),
            b"what do ya want for nothing?",
            b"Jefe"
        ));
        assert!(!verify_hmac(
            &format!("sha256={sig}"),
            b"tampered body",
            b"Jefe"
        ));
        assert!(!verify_hmac(&sig, b"what do ya want for nothing?", b"Jefe"));
        assert!(!verify_hmac("sha256=zzzz", b"x", b"Jefe"));
    }

    #[test]
    fn either_header_authenticates_and_neither_is_required_to_be_present() {
        let secret = "a".repeat(64);
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(b"{\"a\":1}");
        let sig = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));

        let mut token_only = HeaderMap::new();
        token_only.insert("x-bisa-token", secret.parse().unwrap());
        assert!(authenticate(&token_only, b"{\"a\":1}", &secret));

        let mut hmac_only = HeaderMap::new();
        hmac_only.insert("x-hub-signature-256", sig.parse().unwrap());
        assert!(authenticate(&hmac_only, b"{\"a\":1}", &secret));

        // A right signature survives a wrong token sent alongside it.
        let mut both = HeaderMap::new();
        both.insert("x-bisa-token", "nope".parse().unwrap());
        both.insert("x-hub-signature-256", sig.parse().unwrap());
        assert!(authenticate(&both, b"{\"a\":1}", &secret));

        assert!(!authenticate(&HeaderMap::new(), b"{\"a\":1}", &secret));
        let mut wrong = HeaderMap::new();
        wrong.insert("x-bisa-token", "b".repeat(64).parse().unwrap());
        assert!(!authenticate(&wrong, b"{\"a\":1}", &secret));
    }

    #[tokio::test]
    async fn the_cap_is_enforced_while_reading() {
        let under = read_capped(Body::from(vec![7u8; 100]), 128).await;
        assert_eq!(under.unwrap().len(), 100);
        assert!(read_capped(Body::from(vec![7u8; 200]), 128).await.is_err());
    }

    #[tokio::test]
    async fn a_declared_length_over_the_cap_is_refused_before_a_frame_is_read() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_LENGTH,
            (MAX_SIGNAL_PAYLOAD_BYTES + 1).to_string().parse().unwrap(),
        );
        assert!(read_body(&headers, Body::from("{}")).await.is_err());
        let fits = read_body(&HeaderMap::new(), Body::from("{}")).await;
        assert_eq!(fits.unwrap(), b"{}");
    }

    #[test]
    fn a_body_is_json_as_it_came_text_otherwise_and_nothing_when_empty() {
        assert_eq!(payload_of(b"{\"a\":1}").unwrap(), json!({"a": 1}));
        assert_eq!(payload_of(b"[1,2]").unwrap(), json!([1, 2]));
        assert_eq!(
            payload_of(b"not json at all").unwrap(),
            json!("not json at all")
        );
        assert_eq!(payload_of(b"").unwrap(), Value::Null);
        assert_eq!(payload_of(b" \n").unwrap(), Value::Null);
        // Within the cap as it arrived, over it once every quote is escaped.
        let quotes = vec![b'"'; MAX_SIGNAL_PAYLOAD_BYTES - 1];
        assert!(payload_of(&quotes).is_err());
    }

    #[test]
    fn a_delivery_id_is_the_callers_key_then_the_code_hosts_and_bounded() {
        let mut headers = HeaderMap::new();
        assert_eq!(delivery_of(&headers), None);
        headers.insert("x-github-delivery", "72d3162e-cc78".parse().unwrap());
        assert_eq!(delivery_of(&headers).as_deref(), Some("72d3162e-cc78"));
        headers.insert("idempotency-key", " order-17 ".parse().unwrap());
        assert_eq!(delivery_of(&headers).as_deref(), Some("order-17"));
        // An id longer than an id is falls back to the next header.
        headers.insert(
            "idempotency-key",
            "k".repeat(MAX_DELIVERY_ID_BYTES + 1).parse().unwrap(),
        );
        assert_eq!(delivery_of(&headers).as_deref(), Some("72d3162e-cc78"));
        let mut blank = HeaderMap::new();
        blank.insert("idempotency-key", "  ".parse().unwrap());
        assert_eq!(delivery_of(&blank), None);
    }

    #[test]
    fn a_local_refusal_says_whose_move_it_is() {
        let status = |refusal: HookRefusal| refused_locally(refusal).status;
        assert_eq!(status(HookRefusal::Disabled), StatusCode::CONFLICT);
        assert_eq!(
            status(HookRefusal::NotListening("workspace:01X".into())),
            StatusCode::CONFLICT
        );
        assert_eq!(
            status(HookRefusal::NoSuchStart("workspace:01X/ticket".into())),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            status(HookRefusal::Busy("workspace:01X/ticket".into())),
            StatusCode::TOO_MANY_REQUESTS
        );
        let said = refused_locally(HookRefusal::NotListening("workspace:01X".into()));
        assert!(
            said.text.to_string().contains("workspace:01X"),
            "{}",
            said.text
        );
    }

    #[test]
    fn a_public_refusal_says_as_little_as_it_can() {
        let status = |refusal: HookRefusal| refused_publicly(&refusal).status();
        assert_eq!(
            status(HookRefusal::NotListening("goal:01G".into())),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            status(HookRefusal::Busy("goal:01G/ticket".into())),
            StatusCode::TOO_MANY_REQUESTS
        );
        for quiet in [
            HookRefusal::Disabled,
            HookRefusal::NoSuchStart("goal:01G/ticket".into()),
            HookRefusal::NotPublic("goal:01G/ticket".into()),
        ] {
            assert_eq!(status(quiet), StatusCode::NOT_FOUND);
        }
    }

    #[test]
    fn the_bucket_drains_then_refuses_with_a_retry_after() {
        let hooks = Hooks::default();
        for i in 0..HOOK_BURST as u32 {
            assert!(hooks.take("t").is_ok(), "token {i} should be available");
        }
        let retry = hooks.take("t").expect_err("bucket is empty");
        assert!(retry >= 1, "Retry-After must be a usable number of seconds");
        // A different listener has its own budget — one noisy hook cannot
        // mute another.
        assert!(hooks.take("other").is_ok());
    }

    #[test]
    fn the_bucket_table_stays_bounded() {
        let hooks = Hooks::default();
        for i in 0..(MAX_BUCKETS + 50) {
            let _verdict = hooks.take(&format!("id-{i}"));
        }
        let n = hooks.buckets.lock().unwrap().len();
        assert!(n <= MAX_BUCKETS, "bucket table grew to {n}");
    }

    #[test]
    fn the_decoy_is_secret_shaped_and_per_process() {
        let a = Hooks::default();
        let b = Hooks::default();
        assert_eq!(a.decoy.len(), 64);
        assert_ne!(a.decoy, b.decoy);
    }
}
