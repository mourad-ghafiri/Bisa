//! Boundary events: an event on a live step.
//!
//! A step whose work can be stopped — an agent's session, a person's answer or
//! approval, a wait, a spawned goal the step waits for — may carry boundary
//! events. Each is named, says what it listens for while the step is live
//! ([`BoundaryOn`]: a timeout, a reminder cadence, a message, a signal) and
//! what it does when heard ([`BoundaryAct`]):
//!
//! - **divert** — the step stops (`StepState::Diverted`), its work is
//!   cancelled, and the run takes only the flows labelled with the boundary's
//!   name: a timeout's escalation path, the first event of a race;
//! - **notify** / **emit** — an action *beside* the live step: a post in a
//!   conversation, a named signal. The step goes on; these have no flows.
//!
//! A reminder (`every`) never diverts — a divert on a cadence would stop the
//! step at its first tick (`ProblemKind::ReminderInterrupts`). The run machine
//! decides what a fired boundary does; the engine only notices the event and
//! records `RunEvent::BoundaryFired` against the step's entry, so a boundary
//! of an earlier visit can never move a later one.

use crate::assignee::Assignee;
use crate::listen::{MessageFilter, SignalFilter};
use crate::workflow::{refuse_unknown_keys, Branch, InputName, StepKind, ValueRef};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One boundary event of a step. The act sits flattened beside `name` and
/// `on`: `{ name = "late", on = { event = "after", secs = 172800 }, act =
/// "divert" }`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Boundary {
    /// Unique among the step's boundaries and branches; a divert's flows are
    /// labelled with it.
    pub name: Branch,
    pub on: BoundaryOn,
    #[serde(flatten)]
    pub act: BoundaryAct,
}

impl Boundary {
    /// The keys a boundary carries beside its act's own.
    pub const FIELDS: &'static [&'static str] = &["name", "on", "act"];

    pub fn diverts(&self) -> bool {
        matches!(self.act, BoundaryAct::Divert)
    }

    /// Every template the boundary renders against the run: its event's
    /// filter and its act's post or signal.
    pub fn templates(&self) -> Vec<&str> {
        let mut out = self.on.templates();
        out.extend(self.act.templates());
        out
    }

    /// Every input the boundary reads through a reference.
    pub fn input_refs(&self) -> Vec<(&InputName, &'static str)> {
        let mut out = self.on.input_refs();
        out.extend(self.act.input_refs());
        out
    }

    /// The assignees it names: who a message is from or mentions, whom a post
    /// mentions and speaks as.
    pub fn assignee_refs(&self) -> Vec<&ValueRef<Assignee>> {
        let mut out = match &self.on {
            BoundaryOn::Message { filter } => filter.assignee_refs(),
            _ => vec![],
        };
        if let BoundaryAct::Notify {
            mentions, author, ..
        } = &self.act
        {
            out.extend(mentions.iter().chain(author.iter()));
        }
        out
    }

    /// Refuse a key of a raw boundary table — its own, its act's, or its
    /// event's — that none of them carries.
    pub(crate) fn refuse_unknown<E: serde::de::Error>(raw: &serde_json::Value) -> Result<(), E> {
        let Some(map) = raw.as_object() else {
            return Ok(());
        };
        let tag = map
            .get("act")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| E::missing_field("act"))?;
        let own = BoundaryAct::fields_of(tag)
            .ok_or_else(|| E::unknown_variant(tag, &BoundaryAct::NAMES))?;
        refuse_unknown_keys::<E>(map, "boundary event", Self::FIELDS, own)?;
        match map.get("on") {
            Some(on) => BoundaryOn::refuse_unknown(on),
            None => Ok(()),
        }
    }
}

/// What a boundary listens for while its step is live.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "event")]
pub enum BoundaryOn {
    /// Once, this many seconds after the step was entered — fixed, or read
    /// from a `number` input.
    After { secs: ValueRef<u64> },
    /// Every this many seconds after the step was entered, at most `max`
    /// times: a reminder. Never a divert.
    Every {
        secs: ValueRef<u64>,
        #[serde(default = "default_reminders")]
        max: u32,
    },
    /// A message posted into a conversation, filtered as a message start is —
    /// its fields render against the run.
    Message {
        #[serde(flatten)]
        filter: MessageFilter,
    },
    /// A named signal with exact fields.
    Signal {
        #[serde(flatten)]
        filter: SignalFilter,
    },
}

/// How many times a reminder fires when its `max` is not written.
pub const DEFAULT_REMINDERS: u32 = 3;

fn default_reminders() -> u32 {
    DEFAULT_REMINDERS
}

impl BoundaryOn {
    pub fn as_str(&self) -> &'static str {
        match self {
            BoundaryOn::After { .. } => "after",
            BoundaryOn::Every { .. } => "every",
            BoundaryOn::Message { .. } => "message",
            BoundaryOn::Signal { .. } => "signal",
        }
    }

    pub const NAMES: [&'static str; 4] = ["after", "every", "message", "signal"];

    pub fn fields_of(tag: &str) -> Option<&'static [&'static str]> {
        Some(match tag {
            "after" => &["secs"],
            "every" => &["secs", "max"],
            "message" => MessageFilter::FIELDS,
            "signal" => SignalFilter::FIELDS,
            _ => return None,
        })
    }

    pub(crate) fn refuse_unknown<E: serde::de::Error>(raw: &serde_json::Value) -> Result<(), E> {
        let Some(map) = raw.as_object() else {
            return Ok(());
        };
        let tag = map
            .get("event")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| E::missing_field("event"))?;
        let own = BoundaryOn::fields_of(tag)
            .ok_or_else(|| E::unknown_variant(tag, &BoundaryOn::NAMES))?;
        refuse_unknown_keys::<E>(map, "boundary event's `on`", &["event"], own)
    }

    /// A timeout or a reminder: the clock drives it.
    pub fn is_timer(&self) -> bool {
        matches!(self, BoundaryOn::After { .. } | BoundaryOn::Every { .. })
    }

    /// The seconds a timer counts, as written.
    pub fn secs(&self) -> Option<&ValueRef<u64>> {
        match self {
            BoundaryOn::After { secs } | BoundaryOn::Every { secs, .. } => Some(secs),
            BoundaryOn::Message { .. } | BoundaryOn::Signal { .. } => None,
        }
    }

    pub fn templates(&self) -> Vec<&str> {
        match self {
            BoundaryOn::After { .. } | BoundaryOn::Every { .. } => vec![],
            BoundaryOn::Message { filter } => filter.templates(),
            BoundaryOn::Signal { filter } => filter.templates(),
        }
    }

    pub fn input_refs(&self) -> Vec<(&InputName, &'static str)> {
        match self {
            BoundaryOn::After {
                secs: ValueRef::Input { input },
            }
            | BoundaryOn::Every {
                secs: ValueRef::Input { input },
                ..
            } => vec![(input, "number")],
            BoundaryOn::Message { filter } => filter.input_refs(),
            _ => vec![],
        }
    }

    /// When a timer next comes due, from when its step was entered and how
    /// many times it has fired this visit: a timeout once, a reminder at every
    /// multiple of its seconds up to `max`. `None` for a timer that is spent
    /// and for an event the clock does not drive. `secs` is the resolved
    /// value.
    pub fn next_due(&self, secs: u64, entered_at: u64, fired: u32) -> Option<u64> {
        match self {
            BoundaryOn::After { .. } => (fired == 0).then(|| entered_at.saturating_add(secs)),
            BoundaryOn::Every { max, .. } => (fired < *max)
                .then(|| entered_at.saturating_add(secs.saturating_mul(u64::from(fired) + 1))),
            BoundaryOn::Message { .. } | BoundaryOn::Signal { .. } => None,
        }
    }

    /// Whether a boundary that has fired `fired` times this visit may fire
    /// again: a timeout once, a reminder up to `max`, a message or a signal
    /// act on every one heard, a divert once (it ends the visit).
    pub fn may_fire_again(&self, fired: u32) -> bool {
        match self {
            BoundaryOn::After { .. } => fired == 0,
            BoundaryOn::Every { max, .. } => fired < *max,
            BoundaryOn::Message { .. } | BoundaryOn::Signal { .. } => true,
        }
    }
}

/// What a boundary does when its event is heard.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "act")]
pub enum BoundaryAct {
    /// Stop the step and take the flows labelled with the boundary's name.
    Divert,
    /// Post into a conversation beside the live step, as a `notify` step
    /// does: into `scope` (the run's own conversation when absent), as
    /// `author` (the Workflow Agent when absent).
    Notify {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        template: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        mentions: Vec<ValueRef<Assignee>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        author: Option<ValueRef<Assignee>>,
    },
    /// Raise a named signal beside the live step, as an `emit` step does.
    Emit {
        signal: String,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        payload: BTreeMap<String, String>,
    },
}

impl BoundaryAct {
    pub fn as_str(&self) -> &'static str {
        match self {
            BoundaryAct::Divert => "divert",
            BoundaryAct::Notify { .. } => "notify",
            BoundaryAct::Emit { .. } => "emit",
        }
    }

    pub const NAMES: [&'static str; 3] = ["divert", "notify", "emit"];

    pub fn fields_of(tag: &str) -> Option<&'static [&'static str]> {
        Some(match tag {
            "divert" => &[],
            "notify" => &["scope", "template", "mentions", "author"],
            "emit" => &["signal", "payload"],
            _ => return None,
        })
    }

    pub fn templates(&self) -> Vec<&str> {
        match self {
            BoundaryAct::Divert => vec![],
            BoundaryAct::Notify {
                scope, template, ..
            } => std::iter::once(template.as_str())
                .chain(scope.iter().map(String::as_str))
                .collect(),
            BoundaryAct::Emit { signal, payload } => std::iter::once(signal.as_str())
                .chain(payload.values().map(String::as_str))
                .collect(),
        }
    }

    pub fn input_refs(&self) -> Vec<(&InputName, &'static str)> {
        match self {
            BoundaryAct::Notify {
                mentions, author, ..
            } => mentions
                .iter()
                .chain(author.iter())
                .filter_map(|r| match r {
                    ValueRef::Input { input } => Some((input, "assignee")),
                    ValueRef::Fixed(_) => None,
                })
                .collect(),
            BoundaryAct::Divert | BoundaryAct::Emit { .. } => vec![],
        }
    }
}

/// Which kinds may carry boundary events: the ones whose work can be stopped
/// while it is live. A `check`, a connector call or a `judge` runs to its end
/// once begun, and the kinds that finish on entry are never live at all.
/// Exhaustive, so a new kind says which side it is on.
pub fn may_carry_boundaries(kind: &StepKind) -> bool {
    match kind {
        StepKind::Agent { .. }
        | StepKind::Human { .. }
        | StepKind::Approval { .. }
        | StepKind::Wait { .. } => true,
        StepKind::Spawn { wait, .. } => *wait,
        StepKind::Start { .. }
        | StepKind::Check { .. }
        | StepKind::Decide { .. }
        | StepKind::If { .. }
        | StepKind::Switch { .. }
        | StepKind::Judge { .. }
        | StepKind::ForEach { .. }
        | StepKind::While { .. }
        | StepKind::Parallel
        | StepKind::Connector { .. }
        | StepKind::Notify { .. }
        | StepKind::Emit { .. }
        | StepKind::End { .. } => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn every_boundary() -> Vec<Boundary> {
        vec![
            Boundary {
                name: Branch::new("late").unwrap(),
                on: BoundaryOn::After {
                    secs: ValueRef::Fixed(172_800),
                },
                act: BoundaryAct::Divert,
            },
            Boundary {
                name: Branch::new("nudge").unwrap(),
                on: BoundaryOn::Every {
                    secs: ValueRef::Input {
                        input: InputName::new("remind").unwrap(),
                    },
                    max: 5,
                },
                act: BoundaryAct::Notify {
                    scope: None,
                    template: "Still waiting on your decision.".into(),
                    mentions: vec![ValueRef::Fixed(Assignee::Team("ops".into()))],
                    author: None,
                },
            },
            Boundary {
                name: Branch::new("cancelled").unwrap(),
                on: BoundaryOn::Message {
                    filter: MessageFilter {
                        contains: Some("cancel".into()),
                        ..MessageFilter::default()
                    },
                },
                act: BoundaryAct::Divert,
            },
            Boundary {
                name: Branch::new("heads-up").unwrap(),
                on: BoundaryOn::Signal {
                    filter: SignalFilter {
                        name: "deploy.started".into(),
                        fields: BTreeMap::new(),
                    },
                },
                act: BoundaryAct::Emit {
                    signal: "review.waiting".into(),
                    payload: BTreeMap::from([("step".into(), "review".into())]),
                },
            },
        ]
    }

    #[test]
    fn every_boundary_round_trips_and_writes_only_its_keys() {
        for b in every_boundary() {
            let json = serde_json::to_value(&b).unwrap();
            assert_eq!(serde_json::from_value::<Boundary>(json.clone()).unwrap(), b);
            Boundary::refuse_unknown::<serde_json::Error>(&json).unwrap();
            let own = BoundaryAct::fields_of(b.act.as_str()).unwrap();
            for key in json.as_object().unwrap().keys() {
                assert!(
                    Boundary::FIELDS.contains(&key.as_str()) || own.contains(&key.as_str()),
                    "`{key}`"
                );
            }
            let on = BoundaryOn::fields_of(b.on.as_str()).unwrap();
            for key in json["on"].as_object().unwrap().keys() {
                assert!(key == "event" || on.contains(&key.as_str()), "`{key}`");
            }
        }
        assert_eq!(
            serde_json::to_value(&every_boundary()[0]).unwrap(),
            json!({"name": "late", "on": {"event": "after", "secs": 172800}, "act": "divert"})
        );
    }

    #[test]
    fn a_misspelled_boundary_key_is_refused_at_either_level() {
        let top = json!({"name": "late", "on": {"event": "after", "secs": 5}, "act": "divert", "then": []});
        assert!(Boundary::refuse_unknown::<serde_json::Error>(&top).is_err());
        let inner = json!({"name": "late", "on": {"event": "after", "sec": 5}, "act": "divert"});
        assert!(Boundary::refuse_unknown::<serde_json::Error>(&inner).is_err());
        let act = json!({"name": "late", "on": {"event": "after", "secs": 5}, "act": "escalate"});
        assert!(Boundary::refuse_unknown::<serde_json::Error>(&act).is_err());
        let notify = json!({"name": "n", "on": {"event": "after", "secs": 5}, "act": "divert", "template": "x"});
        assert!(
            Boundary::refuse_unknown::<serde_json::Error>(&notify).is_err(),
            "a divert has no post to write"
        );
    }

    #[test]
    fn a_timeout_fires_once_and_a_reminder_up_to_its_max() {
        let after = BoundaryOn::After {
            secs: ValueRef::Fixed(60),
        };
        assert_eq!(after.next_due(60, 1_000, 0), Some(1_060));
        assert_eq!(after.next_due(60, 1_000, 1), None);
        assert!(after.may_fire_again(0) && !after.may_fire_again(1));
        let every = BoundaryOn::Every {
            secs: ValueRef::Fixed(10),
            max: 2,
        };
        assert_eq!(every.next_due(10, 100, 0), Some(110));
        assert_eq!(every.next_due(10, 100, 1), Some(120));
        assert_eq!(every.next_due(10, 100, 2), None);
        let reminder: BoundaryOn =
            serde_json::from_value(json!({"event": "every", "secs": 5})).unwrap();
        assert_eq!(
            reminder,
            BoundaryOn::Every {
                secs: ValueRef::Fixed(5),
                max: DEFAULT_REMINDERS
            }
        );
        let message = BoundaryOn::Message {
            filter: MessageFilter::default(),
        };
        assert_eq!(message.next_due(1, 1, 0), None);
        assert!(message.may_fire_again(99));
        assert!(after.is_timer() && !message.is_timer());
    }

    #[test]
    fn a_boundary_names_what_it_reads() {
        let [_, nudge, cancelled, heads_up] = <[Boundary; 4]>::try_from(every_boundary()).unwrap();
        assert!(!nudge.diverts() && cancelled.diverts());
        assert_eq!(nudge.input_refs().len(), 1);
        assert_eq!(nudge.assignee_refs().len(), 1);
        assert_eq!(nudge.templates(), vec!["Still waiting on your decision."]);
        assert_eq!(
            heads_up.templates(),
            vec!["deploy.started", "review.waiting", "review"]
        );
        assert_eq!(cancelled.templates(), vec!["cancel"]);
    }

    // added by the coverage pass: boundary.rs

    #[test]
    fn a_raw_boundary_that_is_no_table_is_left_to_the_derive_and_a_notify_names_the_inputs_it_mentions(
    ) {
        assert!(Boundary::refuse_unknown::<serde_json::Error>(&serde_json::json!("late")).is_ok());
        assert!(BoundaryOn::refuse_unknown::<serde_json::Error>(&serde_json::json!(5)).is_ok());
        assert!(
            Boundary::refuse_unknown::<serde_json::Error>(
                &serde_json::json!({"name": "n", "act": "divert"})
            )
            .is_ok(),
            "a boundary with no `on` is the derive's to refuse"
        );
        assert_eq!(BoundaryOn::fields_of("never"), None);
        let who = InputName::new("who").unwrap();
        let act = BoundaryAct::Notify {
            scope: None,
            template: "hi".into(),
            mentions: vec![
                ValueRef::Input { input: who.clone() },
                ValueRef::Fixed(Assignee::Agent("a".into())),
            ],
            author: None,
        };
        assert_eq!(act.input_refs(), vec![(&who, "assignee")]);
    }

    // added by the coverage pass: b5-boundary.rs
    #[test]
    fn a_message_boundary_names_who_it_hears_and_a_clock_read_from_an_input_names_it() {
        let who = ValueRef::Fixed(Assignee::Agent("a".into()));
        let heard = Boundary {
            name: Branch::new("ping").unwrap(),
            on: BoundaryOn::Message {
                filter: MessageFilter {
                    mentions: Some(who.clone()),
                    ..MessageFilter::default()
                },
            },
            act: BoundaryAct::Divert,
        };
        assert_eq!(heard.assignee_refs(), vec![&who]);
        let n = InputName::new("n").unwrap();
        assert_eq!(
            BoundaryOn::After {
                secs: ValueRef::Input { input: n.clone() }
            }
            .input_refs(),
            vec![(&n, "number")]
        );
        let m = InputName::new("m").unwrap();
        assert_eq!(
            BoundaryOn::Message {
                filter: MessageFilter {
                    mentions: Some(ValueRef::Input { input: m.clone() }),
                    ..MessageFilter::default()
                },
            }
            .input_refs(),
            vec![(&m, "assignee")]
        );
    }
}
