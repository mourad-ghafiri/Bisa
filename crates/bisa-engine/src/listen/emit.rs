//! The one door a named signal is raised through.
//!
//! An `emit` step and an `emit` boundary act, a session's `emit_signal`
//! tool, a person through `POST /signals` or `bisa signal emit`, an A2A task:
//! every named signal comes through [`emit`], so the name rule, the payload
//! cap, the causal chain and the durable record are one code path.
//!
//! A named signal is recorded **once, with no listener** — settled at once,
//! kept so a `wait` re-armed after a restart can replay what it missed — and
//! then offered like anything the ear hears: to the waits and boundaries
//! holding for it, and to every `signal` start that hears it, each getting a
//! signal of its own. An emit never fails because a listener refused it.
//!
//! A named signal **raised from outside this machine** — an A2A task — comes
//! through [`emit_from_outside`]: redacted, and read by the content screen
//! before anything hears it.

use super::ear::{announce_received, enqueue_for, new_signal_id, offer};
use super::{armed, now_secs, report_once};
use crate::{EngineError, Inner};
use bisa_core::{valid_signal_name, Chain, Heard, Signal, SignalScope, SignalSource};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// What an emit did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Emitted {
    /// The record kept for replay.
    pub signal: String,
    /// The signals written for the listeners that heard it.
    pub listeners: Vec<String>,
}

/// Raise the named signal `name` with `payload`, in `scope`, carrying the
/// causal `chain` of whatever raised it. `dedupe` — an emit step's
/// `emit:<run>:<step>:<entered>` — makes a repeat after a restart the same
/// signal: its record and every listener's copy are found, not written
/// twice.
pub fn emit(
    inner: &Arc<Inner>,
    name: &str,
    payload: Value,
    scope: SignalScope,
    chain: Chain,
    dedupe: Option<&str>,
) -> Result<Emitted, EngineError> {
    let name = name.trim();
    if !valid_signal_name(name) {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-signal-name",
            name = name.to_string()
        )));
    }
    let payload = match payload {
        Value::Object(map) => Value::Object(map),
        Value::Null => json!({}),
        other => json!({ "value": other }),
    };
    // The record's id is the dedupe key's, when there is one, so a repeat
    // finds the record it already wrote.
    let id = match dedupe {
        Some(key) => format!(
            "emit-{}",
            &hex::encode(Sha256::digest(key.as_bytes()))[..32]
        ),
        None => new_signal_id(),
    };
    let record = Signal {
        id,
        listener: None,
        source: SignalSource::Signal,
        name: Some(name.to_string()),
        at: now_secs(),
        payload: payload.clone(),
        scope: scope.clone(),
        chain: chain.clone(),
        dedupe_key: dedupe.map(str::to_string),
    };
    let (kept, fresh) = inner.ws.enqueue_signal(&record)?;
    if fresh {
        announce_received(inner, &kept.signal);
    }
    let heard = Heard {
        source: SignalSource::Signal,
        name: Some(name.to_string()),
        scope,
        payload,
        chain,
    };
    let listeners = offer(inner, &heard, dedupe);
    Ok(Emitted {
        signal: kept.signal.id,
        listeners,
    })
}

/// Raise the named signal `name` for a payload that came from outside this
/// machine; `source` says from where, in the words the screen and a person
/// read (*an A2A task*). Every string of the payload is redacted first. With
/// the content screen off it is then raised like any other. With it on,
/// nothing hears it until the screen has read it, off the caller's path: a
/// sure `safe` raises it; anything else writes it **held** for every listener
/// that would have heard it — the reason on the signal and on its host's row,
/// for a person to let through or leave — and no wait or boundary ever hears
/// what the screen did not pass.
pub fn emit_from_outside(
    inner: &Arc<Inner>,
    name: &str,
    payload: Value,
    scope: SignalScope,
    source: String,
) -> Result<(), EngineError> {
    let name = name.trim().to_string();
    if !valid_signal_name(&name) {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-signal-name",
            name = name.clone()
        )));
    }
    let payload = super::hooks::redact_value(inner, payload);
    if !inner.security.policy().content.screen {
        emit(inner, &name, payload, scope, Chain::default(), None)?;
        return Ok(());
    }
    let inner = Arc::clone(inner);
    tokio::spawn(crate::survive("content screen of a signal", async move {
        let home = scope.goal().map(bisa_core::Home::from);
        let text = payload.to_string();
        match super::hooks::read_outside(&inner, source, &text, home).await {
            Ok(()) => {
                if let Err(e) = emit(&inner, &name, payload, scope, Chain::default(), None) {
                    // LCOV_EXCL_START: raising a screened signal writes the index, which fails only unwritable (disk-only)
                    tracing::warn!(target: "bisa_engine::listen", signal = %name, "a screened signal could not be raised: {e}");
                    // LCOV_EXCL_STOP
                }
            }
            Err(reason) => hold_for_listeners(&inner, &name, payload, scope, &reason),
        }
    }));
    Ok(())
}

/// What the screen did not pass, written held for every listener that would
/// have heard it, and said on each one's host's row.
fn hold_for_listeners(
    inner: &Arc<Inner>,
    name: &str,
    payload: Value,
    scope: SignalScope,
    reason: &str,
) {
    if !inner.listen.settings().enabled {
        return;
    }
    let heard = Heard {
        source: SignalSource::Signal,
        name: Some(name.to_string()),
        scope,
        payload: match payload {
            Value::Object(map) => Value::Object(map),
            Value::Null => json!({}),
            other => json!({ "value": other }),
        },
        chain: Chain::default(),
    };
    let note = super::hooks::held_note(reason);
    let registry = armed(inner);
    for listener in registry.armed.iter().filter(|a| a.hears(&heard)) {
        if let Some(signal) = enqueue_for(inner, listener, &heard, None, Some(note.as_str())) {
            report_once(inner, &listener.key, Some(signal.as_str()), note.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_names_rule_is_the_cores() {
        for good in ["report.ready", "deploy-finished", "a.b_c.d9"] {
            assert!(bisa_core::valid_signal_name(good), "{good}");
        }
        for bad in ["", "Report.Ready", "a..b", "trailing.", "sp ace"] {
            assert!(!bisa_core::valid_signal_name(bad), "{bad}");
        }
    }
}
