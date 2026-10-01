//! A hook call: the door a caller outside the engine — a script on this
//! machine under the control-plane token, or, when the start and the machine
//! both allow it, somebody on the internet with the start's secret — begins a
//! run through.
//!
//! The node verifies the caller (the token, or the secret's HMAC over the raw
//! body) and bounds the body; this module decides whether the listener takes
//! the call and writes it down. A public body is redacted before it is
//! stored and, with the content screen on, **held** until the classifier is
//! sure it is safe ([`screen_held`]): anything else keeps it held and tells
//! the host, and a person lets it through or leaves it. The same screen
//! reads every payload from outside — a poll's item, and a signal raised
//! from outside ([`super::emit::emit_from_outside`]: an A2A task).

use super::ear::{armed_for, new_signal_id, occur};
use super::report_once;
use crate::Inner;
use bisa_core::{ListenerKey, StartOn};
use bisa_security::classify::PageSubject;
use bisa_security::ClassifierVerdict;
use serde_json::{json, Value};
use std::sync::Arc;

/// How the call reached the node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookDoor {
    /// On this machine, under the control-plane token.
    Local,
    /// From outside, with the start's secret.
    Public,
}

/// Why a hook call was not taken. The node answers a local caller in words.
/// A public caller learns as little as the node can let it: nothing of a
/// listener before its secret verified, and only then that the host is off
/// or the start takes no public calls.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum HookRefusal {
    /// `events.enabled` is off on this machine.
    #[error("listening is off on this machine (events.enabled)")]
    Disabled,
    /// The host does not listen: the workflow is Off, the goal is not
    /// listening or is paused.
    #[error("{0} is not listening; turn it on first")]
    NotListening(String),
    /// The step is not a hook start of that host.
    #[error("{0} is not a hook start")]
    NoSuchStart(String),
    /// A public call to a start that is not public, or while this machine
    /// allows no public hooks (`events.public_hooks`).
    #[error("{0} takes no public calls")]
    NotPublic(String),
    /// Its backlog is full, or it could not be written down.
    #[error("{0} could not take the call now: its backlog is full")]
    Busy(String),
}

/// The note a signal held for the screen carries.
pub const HELD_FOR_SCREEN: &str = "held for the content screen";

/// Take one hook call. `delivery` — the caller's `Idempotency-Key` or
/// `X-GitHub-Delivery` — makes a redelivery the same signal. Returns the
/// signal.
pub fn call(
    inner: &Arc<Inner>,
    key: &ListenerKey,
    body: Value,
    delivery: Option<&str>,
    door: HookDoor,
) -> Result<String, HookRefusal> {
    let settings = inner.listen.settings();
    if !settings.enabled {
        return Err(HookRefusal::Disabled);
    }
    if door == HookDoor::Public && !settings.public_hooks {
        return Err(HookRefusal::NotPublic(key.to_string()));
    }
    let Some(armed) = armed_for(inner, key) else {
        let listening = inner
            .ws
            .listening(&key.host)
            .ok()
            .flatten()
            .is_some_and(|l| !l.is_paused());
        return Err(if listening {
            HookRefusal::NoSuchStart(key.to_string())
        } else {
            HookRefusal::NotListening(key.host.to_string())
        });
    };
    let StartOn::Hook { public } = armed.on else {
        return Err(HookRefusal::NoSuchStart(key.to_string()));
    };
    if door == HookDoor::Public && !public {
        return Err(HookRefusal::NotPublic(key.to_string()));
    }
    let payload = shape(body);
    let (payload, held) = match door {
        HookDoor::Local => (payload, None),
        HookDoor::Public => {
            let redacted = redact_value(inner, payload);
            let held = inner
                .security
                .policy()
                .content
                .screen
                .then_some(HELD_FOR_SCREEN);
            (redacted, held)
        }
    };
    let dedupe = match delivery.map(str::trim).filter(|d| !d.is_empty()) {
        Some(d) => format!("hook:{d}"),
        None => format!("hook:{}", new_signal_id()),
    };
    let text = held.map(|_| payload.to_string());
    let signal = occur(inner, &armed, payload, &dedupe, held)
        .ok_or_else(|| HookRefusal::Busy(key.to_string()))?;
    if let Some(text) = text {
        screen_held(
            inner,
            key.clone(),
            signal.clone(),
            format!("a hook call to {key}"),
            text,
        );
    }
    Ok(signal)
}

/// A body as a signal carries it: an object as it came; text as `{ text }`;
/// anything else as `{ value }`.
fn shape(body: Value) -> Value {
    match body {
        Value::Object(map) => Value::Object(map),
        Value::String(text) => json!({ "text": text }),
        Value::Null => json!({}),
        other => json!({ "value": other }),
    }
}

/// Every string in a value through the redactor: a secret a caller sent is
/// never stored.
pub(crate) fn redact_value(inner: &Inner, value: Value) -> Value {
    match value {
        Value::String(s) => Value::String(inner.security.redact(&s).text),
        Value::Array(items) => {
            Value::Array(items.into_iter().map(|v| redact_value(inner, v)).collect())
        }
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| (k, redact_value(inner, v)))
                .collect(),
        ),
        other => other,
    }
}

/// Read a held outside payload, off the caller's path: a sure `safe` lets it
/// through to the worker; a harmful reading or no verdict keeps it held and
/// says so on its host's row, where a person lets it through or leaves it.
pub(crate) fn screen_held(
    inner: &Arc<Inner>,
    key: ListenerKey,
    signal: String,
    source: String,
    text: String,
) {
    let inner = Arc::clone(inner);
    tokio::spawn(crate::survive("content screen of a signal", async move {
        let home = key.host.goal().map(bisa_core::Home::from);
        match read_outside(&inner, source, &text, home).await {
            Ok(()) => match inner.ws.release_signal(&signal) {
                Ok(()) => inner.listen.wake(),
                Err(e) => {
                    tracing::warn!(target: "bisa_engine::listen", %signal, "a screened signal could not be let through: {e}");
                }
            },
            Err(reason) => keep_held(&inner, &key, &signal, held_note(&reason)),
        }
    }));
}

/// What the content screen says of a payload from outside: `Ok` for a sure
/// `safe` and nothing else — a harmful reading, a classifier that is off and
/// a verdict that never came are each the reason it stays held.
pub(crate) async fn read_outside(
    inner: &Arc<Inner>,
    source: String,
    text: &str,
    home: Option<bisa_core::Home>,
) -> Result<(), String> {
    let policy = inner.security.policy();
    let bounded: String = text
        .chars()
        .take(crate::content::MAX_SCREENED_CHARS)
        .collect();
    let subject = PageSubject {
        source,
        url: None,
        title: None,
        text: inner.security.redact(&bounded).text,
    };
    let verdict = if policy.classifier.enabled {
        crate::classifier::classify_content(inner, &subject, &policy.classifier, home).await
    } else {
        Err(crate::EngineError::Security("the classifier is off".into()))
    };
    match verdict {
        Ok(ClassifierVerdict::Safe) => Ok(()),
        Ok(ClassifierVerdict::Harmful { reason }) => {
            Err(format!("the classifier says harmful: {reason}"))
        }
        Err(e) => Err(format!("the classifier gave no verdict: {e}")),
    }
}

/// The note a signal the screen did not pass is held under.
pub(crate) fn held_note(reason: &str) -> String {
    format!("held by the content screen — {reason}; let it through or leave it")
}

/// A held signal stays held, its reason on it and on its host's row.
fn keep_held(inner: &Arc<Inner>, key: &ListenerKey, signal: &str, note: String) {
    if let Err(e) = inner
        .ws
        .move_signal(signal, bisa_store::SignalState::Held, Some(&note))
    {
        tracing::warn!(target: "bisa_engine::listen", %signal, "a held signal's reason could not be written: {e}");
    }
    report_once(inner, key, Some(signal), note);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_body_is_an_object_whatever_was_sent() {
        assert_eq!(shape(json!({"a": 1})), json!({"a": 1}));
        assert_eq!(shape(json!("hello")), json!({"text": "hello"}));
        assert_eq!(shape(json!(7)), json!({"value": 7}));
        assert_eq!(shape(Value::Null), json!({}));
    }

    #[test]
    fn a_refusal_names_the_listener_or_its_host() {
        let msg = HookRefusal::NotListening("workspace:01X".into()).to_string();
        assert!(msg.contains("workspace:01X"), "{msg}");
        assert!(HookRefusal::Disabled.to_string().contains("events.enabled"));
    }
}
