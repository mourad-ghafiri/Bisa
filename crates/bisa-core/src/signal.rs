//! `Signal`: the durable, idempotent record of one occurrence.
//!
//! An event never acts. Whatever the engine hears that a listener waits for —
//! a schedule coming due, a hook call, a message, a named signal, a project's
//! change, a run's end, a platform topic, a poll's new item, a check's result
//! — is written down as a signal first, and only then does the queue's worker
//! start a run from it, under the same gates, budgets, pause switch and caps
//! as a person's start. The write is idempotent: a signal carries the
//! listener it is for and a `dedupe_key` its source derives
//! (`schedule:<due>`, `poll:<key>`, `commit:<branch>@<sha>`, a hook's
//! delivery id, `emit:<run>:<step>:<entered>`), and one listener never holds
//! two signals with the same key — so a crash between hearing and starting,
//! or a redelivered hook, starts one run.
//!
//! A named signal an `emit` raises is recorded once with no listener too, so
//! a `wait` re-armed after a restart can replay what it missed. The run a
//! signal starts keeps the whole signal (`WorkflowRun::event`), and its start
//! step's input mapping reads it as `{event.…}` — the one place the event is
//! read.

use crate::listen::{Chain, Heard, ListenerKey, SignalScope, SignalSource};
use serde::{Deserialize, Serialize};

/// Hard cap on a signal's payload. A hook body is attacker-controlled;
/// without a ceiling one POST could pin the queue's memory and the index.
pub const MAX_SIGNAL_PAYLOAD_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Signal {
    /// ULID — the signal's own identity.
    pub id: String,
    /// The listener it was raised for; `None` for a named signal recorded
    /// only so a re-armed `wait` can replay it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub listener: Option<ListenerKey>,
    pub source: SignalSource,
    /// A named signal's name, or a platform topic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub at: u64,
    #[serde(default)]
    pub payload: serde_json::Value,
    pub scope: SignalScope,
    /// The listeners the causal line behind this occurrence passed through.
    #[serde(default, skip_serializing_if = "Chain::is_empty")]
    pub chain: Chain,
    /// What makes a second write of the same occurrence one signal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dedupe_key: Option<String>,
}

impl Signal {
    /// The occurrence as the filters read it.
    pub fn heard(&self) -> Heard {
        Heard {
            source: self.source,
            name: self.name.clone(),
            scope: self.scope.clone(),
            payload: self.payload.clone(),
            chain: self.chain.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{GoalId, WorkflowId};
    use crate::listen::ListenerHost;
    use crate::workflow::StepId;

    fn signal() -> Signal {
        let listener = ListenerKey {
            host: ListenerHost::Workspace {
                workflow: WorkflowId::from_ulid(ulid::Ulid::from_parts(9, 1)),
            },
            step: StepId::new("ticket").unwrap(),
        };
        Signal {
            id: "01SIGNAL".into(),
            listener: Some(listener.clone()),
            source: SignalSource::Hook,
            name: None,
            at: 1_700_000_000,
            payload: serde_json::json!({"x": 1}),
            scope: SignalScope::Goal {
                goal: GoalId::from_ulid(ulid::Ulid::from_parts(1, 1)),
            },
            chain: Chain::default().extend(&listener),
            dedupe_key: Some("delivery:abc".into()),
        }
    }

    #[test]
    fn signal_json_roundtrip() {
        let s = signal();
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Signal>(&json).unwrap(), s);
    }

    #[test]
    fn a_signal_shaped_by_the_trigger_era_is_refused() {
        let old = serde_json::json!({
            "id": "01S", "trigger": "01T", "topic": "webhook", "at": 0,
            "payload": {}, "scope": {"scope": "workspace"}
        });
        assert!(serde_json::from_value::<Signal>(old).is_err());
    }

    #[test]
    fn a_signal_is_heard_as_it_was_recorded() {
        let s = signal();
        let heard = s.heard();
        assert_eq!(heard.source, SignalSource::Hook);
        assert_eq!(heard.payload, s.payload);
        assert_eq!(heard.chain.depth, 1);
        assert_eq!(heard.scope, s.scope);
    }
}
