//! Agent registry with the omp invariants: CAS registration, generation-
//! checked mutation, `Aborted` is terminal, tombstones on disk.

use bisa_core::{ConversationId, GoalId, SessionId, WorkItemId, WorkstreamId};
use dashmap::DashMap;
use std::fmt;
use std::str::FromStr;

/// The handle of one **live harness session** the engine is driving — a
/// worker's attempt at a step, a guided wake, a chat turn.
///
/// Deliberately not [`bisa_core::LiveRunId`]: that is a *workflow run*, the
/// durable execution of a workflow on a goal, which outlives every session
/// that works on it. This one is process-local and dies with the registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LiveRunId(ulid::Ulid);

impl LiveRunId {
    /// A fresh handle, minted from the clock.
    pub fn mint() -> Self {
        Self(ulid::Ulid::from_datetime(SystemTime::now()))
    }

    pub fn from_ulid(u: ulid::Ulid) -> Self {
        Self(u)
    }
}

impl fmt::Display for LiveRunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for LiveRunId {
    type Err = ulid::DecodeError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(ulid::Ulid::from_str(s)?))
    }
}

impl schemars::JsonSchema for LiveRunId {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("LiveRunId")
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "description": "A live harness session's handle: a ULID, minted per launch."
        })
    }
}
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// What a session is a run of — the store's vocabulary, so the roster, the
/// durable row and the wire say the same four words: `worker` (a step's work
/// item), `guided` (the Workflow Agent's design wake), `conversation` (an
/// agent's turn in a conversation, a channel or a goal's thread), `terminal`
/// (a harness a person opened in a desktop terminal, reporting through
/// [`crate::interactive`]; never scheduled, never registered here).
pub use bisa_store::SessionKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Running,
    Idle,
    Parked,
    Aborted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentRef {
    pub id: LiveRunId,
    pub kind: SessionKind,
    pub status: AgentStatus,
    /// Bumped on every accepted mutation; stale writers are rejected.
    pub generation: u64,
    pub session_id: Option<SessionId>,
    pub work_item: Option<WorkItemId>,
    /// The conversation this session is a turn of, for a `conversation` kind
    /// whose scope is a conversation — never a channel's or a goal thread's.
    pub conversation: Option<ConversationId>,
    /// The goal the session is about, when it is about one and no work item
    /// says so — a guided wake designing a goal's workflow, a turn of a
    /// conversation with a goal origin.
    pub goal: Option<GoalId>,
    /// The workstream the run is standing in — the checkout that is
    /// its cwd. `None` for a run in an agent's or a goal's scratch folder.
    pub workstream: Option<WorkstreamId>,
    pub transcript_path: Option<PathBuf>,
    /// Unix seconds of the last observed activity.
    pub last_activity: u64,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum RegistryError {
    #[error("agent already registered (or expectation mismatch)")]
    CasFailed,
    #[error("stale mutation: expected generation {expected}, current {current}")]
    Stale { expected: u64, current: u64 },
    #[error("agent {0} is aborted (terminal)")]
    Aborted(String),
    #[error("agent {0} not found")]
    NotFound(String),
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Tombstone path for a transcript: `<transcript>.tombstone`.
pub fn tombstone_path(transcript: &Path) -> PathBuf {
    let mut os = transcript.as_os_str().to_owned();
    os.push(".tombstone");
    PathBuf::from(os)
}

pub fn is_tombstoned(transcript: &Path) -> bool {
    tombstone_path(transcript).exists()
}

#[derive(Default)]
pub struct AgentRegistry {
    agents: DashMap<LiveRunId, AgentRef>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Compare-and-swap registration. `expected == None` means "only if
    /// absent"; `Some(prev)` means "only replacing exactly this ref" (used by
    /// revival: only the parked ref a revival was authorized for can be
    /// claimed).
    pub fn register_if(
        &self,
        agent: AgentRef,
        expected: Option<&AgentRef>,
    ) -> Result<(), RegistryError> {
        // A tombstoned transcript can never be (re)registered as live.
        if let Some(t) = &agent.transcript_path {
            if is_tombstoned(t) && agent.status != AgentStatus::Aborted {
                return Err(RegistryError::Aborted(agent.id.to_string()));
            }
        }
        match self.agents.entry(agent.id) {
            dashmap::mapref::entry::Entry::Vacant(v) => {
                if expected.is_some() {
                    return Err(RegistryError::CasFailed);
                }
                v.insert(agent);
                Ok(())
            }
            dashmap::mapref::entry::Entry::Occupied(mut o) => match expected {
                Some(prev) if o.get() == prev => {
                    o.insert(agent);
                    Ok(())
                }
                _ => Err(RegistryError::CasFailed),
            },
        }
    }

    pub fn get(&self, id: LiveRunId) -> Option<AgentRef> {
        self.agents.get(&id).map(|r| r.clone())
    }

    pub fn list(&self) -> Vec<AgentRef> {
        self.agents.iter().map(|r| r.clone()).collect()
    }

    /// Whether a run was stopped for good. `Aborted` is entered once and
    /// never left, so this is what a driver reads before it keeps a session
    /// it was about to: a stop that came while the session was being made
    /// is honoured by whoever holds it next.
    pub fn is_aborted(&self, id: LiveRunId) -> bool {
        self.agents
            .get(&id)
            .is_some_and(|run| run.status == AgentStatus::Aborted)
    }

    /// Generation-checked mutation. Rejects stale writers and any transition
    /// out of `Aborted`.
    pub fn mutate(
        &self,
        id: LiveRunId,
        expected_generation: u64,
        f: impl FnOnce(&mut AgentRef),
    ) -> Result<AgentRef, RegistryError> {
        let mut entry = self
            .agents
            .get_mut(&id)
            .ok_or_else(|| RegistryError::NotFound(id.to_string()))?;
        if entry.status == AgentStatus::Aborted {
            return Err(RegistryError::Aborted(id.to_string()));
        }
        if entry.generation != expected_generation {
            return Err(RegistryError::Stale {
                expected: expected_generation,
                current: entry.generation,
            });
        }
        f(&mut entry);
        // `Aborted` may be *entered* here, never left (checked above on the
        // next call).
        entry.generation += 1;
        entry.last_activity = now_secs();
        Ok(entry.clone())
    }

    /// Forget a run that is over. Refused while it could still be running:
    /// only an `Idle`, a `Parked` (disposed, revived by nothing here) or an
    /// `Aborted` ref leaves, and only presence's retention timer asks.
    pub fn remove(&self, id: LiveRunId) -> Result<(), RegistryError> {
        match self.agents.get(&id).map(|a| a.status) {
            None => Err(RegistryError::NotFound(id.to_string())),
            Some(AgentStatus::Idle) | Some(AgentStatus::Parked) | Some(AgentStatus::Aborted) => {
                self.agents.remove(&id);
                Ok(())
            }
            Some(_) => Err(RegistryError::CasFailed),
        }
    }

    /// Hard-kill: status -> Aborted (terminal) and a durable tombstone beside
    /// the transcript so the kill survives restarts.
    pub fn abort(&self, id: LiveRunId) -> Result<(), RegistryError> {
        let mut entry = self
            .agents
            .get_mut(&id)
            .ok_or_else(|| RegistryError::NotFound(id.to_string()))?;
        if entry.status == AgentStatus::Aborted {
            return Ok(()); // idempotent
        }
        entry.status = AgentStatus::Aborted;
        entry.generation += 1;
        if let Some(t) = entry.transcript_path.clone() {
            // create_new: first writer wins; racing with an existing tombstone
            // is success, any other error is logged but the abort stands.
            match std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(tombstone_path(&t))
            {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => tracing::warn!("failed to write tombstone for {}: {e}", t.display()),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aref(id: LiveRunId) -> AgentRef {
        AgentRef {
            id,
            kind: SessionKind::Worker,
            status: AgentStatus::Running,
            generation: 1,
            session_id: None,
            work_item: None,
            conversation: None,
            goal: None,
            workstream: None,
            transcript_path: None,
            last_activity: 0,
        }
    }

    fn new_id() -> LiveRunId {
        LiveRunId::from_ulid(ulid::Ulid::from_datetime(SystemTime::now()))
    }

    #[test]
    fn cas_registration() {
        let reg = AgentRegistry::new();
        let id = new_id();
        let a = aref(id);
        reg.register_if(a.clone(), None).unwrap();
        // Re-register without expectation fails.
        assert_eq!(
            reg.register_if(a.clone(), None),
            Err(RegistryError::CasFailed)
        );
        // Replace with the exact current ref succeeds.
        let mut b = a.clone();
        b.status = AgentStatus::Idle;
        reg.register_if(b, Some(&a)).unwrap();
        // Replace with a stale expectation fails.
        assert_eq!(
            reg.register_if(a.clone(), Some(&a)),
            Err(RegistryError::CasFailed)
        );
    }

    #[test]
    fn stale_mutation_rejected() {
        let reg = AgentRegistry::new();
        let id = new_id();
        reg.register_if(aref(id), None).unwrap();
        let updated = reg.mutate(id, 1, |a| a.status = AgentStatus::Idle).unwrap();
        assert_eq!(updated.generation, 2);
        // A writer that still holds generation 1 is stale.
        assert_eq!(
            reg.mutate(id, 1, |a| a.status = AgentStatus::Running),
            Err(RegistryError::Stale {
                expected: 1,
                current: 2
            })
        );
    }

    #[test]
    fn only_a_finished_run_can_be_forgotten() {
        let reg = AgentRegistry::new();
        let id = new_id();
        reg.register_if(aref(id), None).unwrap();
        assert_eq!(
            reg.remove(id),
            Err(RegistryError::CasFailed),
            "running: refused"
        );
        reg.mutate(id, 1, |a| a.status = AgentStatus::Idle).unwrap();
        reg.remove(id).unwrap();
        assert!(reg.get(id).is_none());
        assert!(matches!(reg.remove(id), Err(RegistryError::NotFound(_))));
    }

    #[test]
    fn aborted_is_terminal_and_tombstoned() {
        let dir = tempfile::tempdir().unwrap();
        let transcript = dir.path().join("t.jsonl");
        std::fs::write(&transcript, "x").unwrap();

        let reg = AgentRegistry::new();
        let id = new_id();
        let mut a = aref(id);
        a.transcript_path = Some(transcript.clone());
        reg.register_if(a, None).unwrap();

        reg.abort(id).unwrap();
        assert!(is_tombstoned(&transcript));
        // No transition out of Aborted.
        let gen = reg.get(id).unwrap().generation;
        assert!(matches!(
            reg.mutate(id, gen, |a| a.status = AgentStatus::Idle),
            Err(RegistryError::Aborted(_))
        ));
        // A tombstoned transcript can't re-register as live (fresh registry =
        // "after restart").
        let reg2 = AgentRegistry::new();
        let mut b = aref(new_id());
        b.transcript_path = Some(transcript);
        assert!(matches!(
            reg2.register_if(b, None),
            Err(RegistryError::Aborted(_))
        ));
    }
}
