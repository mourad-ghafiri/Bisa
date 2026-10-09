//! The listeners armed right now.
//!
//! A projection, never a second truth: built from the listening hosts
//! ([`bisa_store::Workspace::list_listening`]), each host's workflow as it
//! stands, and each event start resolved against the inputs the host listens
//! with. A paused host arms nothing. A start that cannot be armed — its
//! workflow has problems, an input it reads no longer binds — is reported
//! once ([`super::report_once`]) and skipped, never armed on a guess.
//!
//! What a start's memory belongs to is its **digest**: a hash of the resolved
//! event, its mapping and its guard. A listener whose start changed under its
//! memory starts that memory afresh ([`runtime_of`]).

use crate::Inner;
use bisa_core::{
    Guard, Heard, Hearer, ListenerHost, ListenerKey, Listening, SignalScope, SignalSource, StartOn,
    StepKind, Workflow,
};
use bisa_store::ListenerRuntime;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Arc;

/// One armed listener: a start step of a host, resolved.
#[derive(Clone, Debug)]
pub struct Armed {
    pub key: ListenerKey,
    /// The definition the host runs, as it stands.
    pub workflow: Arc<Workflow>,
    /// The start's event, every template rendered and every input read.
    pub on: StartOn,
    /// How an occurrence maps onto the run's inputs.
    pub mapping: BTreeMap<String, String>,
    pub guard: Guard,
    pub listening: Listening,
    pub digest: String,
}

impl Armed {
    /// Who this listener is, as a signal's scope admits it.
    pub fn hearer(&self) -> Hearer {
        match self.key.host {
            ListenerHost::Workspace { .. } => Hearer::WorkspaceListener,
            ListenerHost::Goal { goal } => Hearer::GoalListener(goal),
        }
    }

    /// Where this listener's own occurrences belong: its goal, or the
    /// workspace.
    pub fn scope(&self) -> SignalScope {
        match self.key.host {
            ListenerHost::Workspace { .. } => SignalScope::Workspace,
            ListenerHost::Goal { goal } => SignalScope::Goal { goal },
        }
    }

    /// Whether what the ear heard is this listener's event. The sources the
    /// ticker and the hook door produce for a listener themselves — a
    /// schedule, a call, a poll, a check — are never heard.
    pub fn hears(&self, heard: &Heard) -> bool {
        if !heard.scope.heard_by(self.hearer()) {
            return false;
        }
        match &self.on {
            StartOn::Message { filter } => filter.hears(heard),
            StartOn::Signal { filter } => filter.hears(heard),
            StartOn::Project { filter } => filter.hears(heard),
            StartOn::Run { filter } => filter.hears(heard),
            StartOn::Platform { filter } => filter.hears(heard),
            StartOn::Manual
            | StartOn::Schedule { .. }
            | StartOn::Hook { .. }
            | StartOn::Connector { .. }
            | StartOn::Check { .. } => false,
        }
    }

    /// What a signal of this listener records itself as.
    pub fn source(&self) -> SignalSource {
        self.on.source().unwrap_or(SignalSource::Test)
    }
}

/// Every armed listener.
#[derive(Debug, Default)]
pub struct Registry {
    pub armed: Vec<Armed>,
}

impl Registry {
    pub fn get(&self, key: &ListenerKey) -> Option<&Armed> {
        self.armed.iter().find(|a| &a.key == key)
    }

    pub fn of_host<'a>(&'a self, host: &'a ListenerHost) -> impl Iterator<Item = &'a Armed> {
        self.armed.iter().filter(move |a| &a.key.host == host)
    }

    /// Whether any armed listener could hear events of `source` — the ear's
    /// cheap question before it builds anything.
    pub fn wants(&self, source: SignalSource) -> bool {
        self.armed.iter().any(|a| a.on.source() == Some(source))
    }

    /// Whether any armed `platform` listener names `topic`.
    pub fn wants_topic(&self, topic: &str) -> bool {
        self.armed.iter().any(|a| {
            matches!(
                &a.on,
                StartOn::Platform { filter } if filter.topic == topic
            )
        })
    }
}

/// The workflow a host listens with, when it may listen at all: a library
/// workflow that is not archived, or an open goal's workflow.
fn host_workflow(inner: &Inner, host: &ListenerHost) -> Result<Option<Workflow>, String> {
    match host {
        ListenerHost::Workspace { workflow } => {
            let wf = inner
                .ws
                .get_workflow(*workflow)
                .map_err(|e| e.to_string())?;
            Ok((!wf.is_archived()).then_some(wf))
        }
        ListenerHost::Goal { goal } => {
            let g = inner.ws.get_goal(*goal).map_err(|e| e.to_string())?;
            if g.is_closed() {
                // LCOV_EXCL_START: a closed goal's listening is turned off by close_goal before the registry is rebuilt (stopping_or_closing_a_goal_stops_its_listening)
                return Ok(None);
                // LCOV_EXCL_STOP
            }
            match g.workflow {
                Some(wf) => inner
                    .ws
                    .get_workflow(wf)
                    .map(Some)
                    .map_err(|e| e.to_string()),
                // LCOV_EXCL_START: a goal pointed at no workflow has its listening turned off by set_workflow before the registry is rebuilt
                None => Ok(None),
                // LCOV_EXCL_STOP
            }
        }
    }
}

/// Build the registry from the listening hosts.
pub fn build(inner: &Arc<Inner>) -> Registry {
    let hosts = match inner.ws.list_listening() {
        Ok(hosts) => hosts,
        // LCOV_EXCL_START: the listening hosts are read from the index, which fails only unreadable (disk-only)
        Err(e) => {
            tracing::warn!(target: "bisa_engine::listen", "the listening hosts could not be read; nothing is armed until they can: {e}");
            return Registry::default();
            // LCOV_EXCL_STOP
        }
    };
    let mut armed = Vec::new();
    for (host, listening) in hosts {
        if listening.is_paused() {
            continue;
        }
        let wf = match host_workflow(inner, &host) {
            Ok(Some(wf)) => Arc::new(wf),
            // LCOV_EXCL_START: a host whose workflow is gone was turned off with it (delete_workflow, close_goal)
            Ok(None) => continue,
            // LCOV_EXCL_STOP
            // LCOV_EXCL_START: a host's workflow is read from the store, which fails only unreadable (disk-only)
            Err(e) => {
                tracing::warn!(target: "bisa_engine::listen", %host, "a listening host's workflow could not be read: {e}");
                continue;
                // LCOV_EXCL_STOP
            }
        };
        let keys: Vec<ListenerKey> = wf
            .event_starts()
            .iter()
            .map(|(step, _)| ListenerKey {
                host,
                step: step.id.clone(),
            })
            .collect();
        match inner.ws.validate_workflow(&wf) {
            Ok(problems) if !problems.is_empty() => {
                let first = problems
                    .first()
                    .map(|p| p.text.to_string())
                    .unwrap_or_default();
                for key in &keys {
                    super::report_once(
                        inner,
                        key,
                        None,
                        format!(
                            "not armed: the workflow has {} problem{} — {first}",
                            problems.len(),
                            if problems.len() == 1 { "" } else { "s" }
                        ),
                    );
                }
                continue;
            }
            Ok(_) => {}
            // LCOV_EXCL_START: validating a workflow reads the store, which fails only unreadable (disk-only)
            Err(e) => {
                tracing::warn!(target: "bisa_engine::listen", %host, "a listening workflow could not be validated: {e}");
                continue;
                // LCOV_EXCL_STOP
            }
        }
        for step in &wf.steps {
            let StepKind::Start {
                on,
                inputs: mapping,
                guard,
            } = &step.kind
            else {
                continue;
            };
            if on.is_manual() {
                continue;
            }
            let key = ListenerKey {
                host,
                step: step.id.clone(),
            };
            match on.resolve(&wf.with_defaults(&listening.inputs)) {
                Ok(resolved) => {
                    let digest = digest(&resolved, mapping, guard);
                    armed.push(Armed {
                        key,
                        workflow: Arc::clone(&wf),
                        on: resolved,
                        mapping: mapping.clone(),
                        guard: *guard,
                        listening: listening.clone(),
                        digest,
                    });
                }
                Err(e) => {
                    super::report_once(inner, &key, None, format!("not armed: {e}"));
                }
            }
        }
    }
    Registry { armed }
}

/// The digest of a resolved start: what its runtime memory belongs to.
fn digest(on: &StartOn, mapping: &BTreeMap<String, String>, guard: &Guard) -> String {
    let bytes = serde_json::to_vec(&(on, mapping, guard)).unwrap_or_default();
    hex::encode(Sha256::digest(&bytes))
}

/// A listener's memory, started afresh when its start changed under it.
pub fn runtime_of(inner: &Inner, armed: &Armed) -> ListenerRuntime {
    let rt = inner.ws.listener_runtime(&armed.key);
    if rt.armed.as_deref() == Some(armed.digest.as_str()) {
        return rt;
    }
    ListenerRuntime {
        armed: Some(armed.digest.clone()),
        // What a person's start already paced survives a change of the start.
        last_dispatch_at: rt.last_dispatch_at,
        ..ListenerRuntime::default()
    }
}

/// Keep a listener's memory, said when it cannot be. What is wrong with the
/// listener is the runtime's to say, never the caller's copy: a memory read
/// before a failure was reported would otherwise write the failure away.
pub fn put_runtime(inner: &Inner, key: &ListenerKey, rt: &ListenerRuntime) {
    let rt = ListenerRuntime {
        failed: inner.listen.trouble_of(key),
        ..rt.clone()
    };
    if let Err(e) = inner.ws.put_listener_runtime(key, &rt) {
        // LCOV_EXCL_START: a listener's memory is written to the index, which fails only unwritable (disk-only)
        tracing::warn!(target: "bisa_engine::listen", listener = %key, "the listener's memory could not be kept: {e}");
        // LCOV_EXCL_STOP
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_core::{Chain, GoalId, SignalFilter, WorkflowId};

    fn armed(host: ListenerHost, on: StartOn) -> Armed {
        Armed {
            key: ListenerKey {
                host,
                step: bisa_core::StepId::new("s").unwrap(),
            },
            workflow: Arc::new(Workflow {
                id: WorkflowId::from_ulid(ulid::Ulid::from_parts(1, 1)),
                name: "w".into(),
                description: String::new(),
                inputs: vec![],
                steps: vec![],
                origin: bisa_core::WorkflowOrigin::Workspace,
                author: bisa_core::PrincipalId::new("a".repeat(64)).unwrap(),
                tags: Default::default(),
                revision: 1,
                archived: None,
                decision_making: false,
                created_at: 0,
            }),
            on,
            mapping: BTreeMap::new(),
            guard: Guard::default(),
            listening: Listening {
                inputs: BTreeMap::new(),
                budget: None,
                since: 0,
                paused: None,
            },
            digest: String::new(),
        }
    }

    fn signal_heard(name: &str, scope: SignalScope) -> Heard {
        Heard {
            source: SignalSource::Signal,
            name: Some(name.into()),
            scope,
            payload: serde_json::json!({}),
            chain: Chain::default(),
        }
    }

    #[test]
    fn a_listener_hears_its_own_event_within_its_hosts_scope() {
        let a = GoalId::from_ulid(ulid::Ulid::from_parts(1, 2));
        let b = GoalId::from_ulid(ulid::Ulid::from_parts(2, 2));
        let on = StartOn::Signal {
            filter: SignalFilter {
                name: "report.ready".into(),
                fields: BTreeMap::new(),
            },
        };
        let goal_a = armed(ListenerHost::Goal { goal: a }, on.clone());
        let library = armed(
            ListenerHost::Workspace {
                workflow: WorkflowId::from_ulid(ulid::Ulid::from_parts(9, 9)),
            },
            on,
        );
        let of_b = signal_heard("report.ready", SignalScope::Goal { goal: b });
        assert!(!goal_a.hears(&of_b), "another goal's signal");
        assert!(library.hears(&of_b), "the workspace hears every goal's");
        assert!(goal_a.hears(&signal_heard("report.ready", SignalScope::Workspace)));
        assert!(!goal_a.hears(&signal_heard("other", SignalScope::Workspace)));
        assert_eq!(goal_a.scope(), SignalScope::Goal { goal: a });
    }

    #[test]
    fn what_a_source_produces_itself_is_never_heard() {
        let host = ListenerHost::Workspace {
            workflow: WorkflowId::from_ulid(ulid::Ulid::from_parts(3, 3)),
        };
        let schedule = armed(
            host,
            StartOn::Schedule {
                schedule: bisa_core::Schedule::every(60),
            },
        );
        let heard = Heard {
            source: SignalSource::Schedule,
            name: None,
            scope: SignalScope::Workspace,
            payload: serde_json::json!({}),
            chain: Chain::default(),
        };
        assert!(!schedule.hears(&heard));
        let registry = Registry {
            armed: vec![schedule],
        };
        assert!(registry.wants(SignalSource::Schedule));
        assert!(!registry.wants(SignalSource::Message));
        assert!(!registry.wants_topic("goal.closed"));
    }

    #[test]
    fn a_digest_follows_the_resolved_start() {
        let a = StartOn::Schedule {
            schedule: bisa_core::Schedule::every(60),
        };
        let b = StartOn::Schedule {
            schedule: bisa_core::Schedule::every(61),
        };
        let none = BTreeMap::new();
        assert_eq!(
            digest(&a, &none, &Guard::default()),
            digest(&a, &none, &Guard::default())
        );
        assert_ne!(
            digest(&a, &none, &Guard::default()),
            digest(&b, &none, &Guard::default())
        );
        let skip = Guard {
            overlap: bisa_core::Overlap::Skip,
            ..Guard::default()
        };
        assert_ne!(
            digest(&a, &none, &Guard::default()),
            digest(&a, &none, &skip)
        );
    }
}
