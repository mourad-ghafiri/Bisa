//! Listening: whether a host hears its workflow's start events, and each
//! listener's runtime memory.
//!
//! A **library workflow** listens once a person turns it On: its record,
//! `workflows/listening/<WorkflowId>.json`, is this machine's truth — kept
//! apart from the definition, so turning a workflow On or Off is never a
//! revision of it, and never a GEP kind: a workflow listens on the host it
//! was turned on at. A **goal** listens through its own snapshot
//! (`Goal::listening`): the goal is the aggregate that owns its standing, and
//! its status stays a pure fold of the goal and its run.
//!
//! Each listener — one start step of a listening host — keeps a runtime
//! memory under `events/listeners/`: when it next comes due, what a poll has
//! seen, a project's last heads, a check's last result. None of it is a fact
//! about the workspace, so none of it is snapshotted; turning the host On
//! starts every listener's memory afresh (no catch-up burst), and it goes
//! with the host.
//!
//! A public hook start's secret lives in the keystore under
//! `hook:<host kind>:<host id>:<step>` — minted when its host first listens,
//! kept across Off and On, rotated on request, removed with its host or its
//! start. It never enters a record, a snapshot or the index.

use crate::error::StoreError;
use crate::paths::Paths;
use crate::workspace::{now_secs, Workspace};
use bisa_core::{GoalId, ListenerHost, ListenerKey, Listening, WorkflowId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One listener's memory on this machine. Every field is absent until the
/// runtime has something to remember; a missing or unreadable file reads as
/// the default.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListenerRuntime {
    /// A digest of the resolved start this memory belongs to: a start that
    /// changed under it starts afresh.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub armed: Option<String>,
    /// When a schedule, a poll or a check next comes due.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_due: Option<u64>,
    /// When the listener last started a run — what its debounce counts from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_dispatch_at: Option<u64>,
    /// A connector poll's memory: the keys of the items seen so far, the
    /// oldest dropped past the runtime's bound. `None` until the first poll —
    /// which learns what is there and starts nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seen: Option<Vec<String>>,
    /// A project start's last view of its refs: branch (or remote branch) →
    /// commit. `None` until the first look, which learns them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heads: Option<BTreeMap<String, String>>,
    /// A files start's last view of its tree: path → what the status or the
    /// scan said of it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<BTreeMap<String, String>>,
    /// The pull requests' states last seen: number → state word.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr_states: Option<BTreeMap<String, String>>,
    /// When pull request states were last asked of the code host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr_polled_at: Option<u64>,
    /// A check start's last result: passed or not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_passed: Option<bool>,
    /// Why the listener could not be armed, when it could not — reported
    /// once, not every tick.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed: Option<String>,
}

impl Workspace {
    /// A host's standing, when it listens.
    pub fn listening(&self, host: &ListenerHost) -> Result<Option<Listening>, StoreError> {
        match host {
            ListenerHost::Workspace { workflow } => self.workflow_listening(*workflow),
            ListenerHost::Goal { goal } => Ok(self.get_goal(*goal)?.listening),
        }
    }

    fn workflow_listening(&self, wf: WorkflowId) -> Result<Option<Listening>, StoreError> {
        let path = self.paths.listening_file(wf);
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|e| StoreError::unreadable(&path, "listening record", e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(StoreError::io(path.display().to_string(), e)),
        }
    }

    /// Set or clear a host's standing. A workflow's is its record (and the
    /// index's column); a goal's is its snapshot, a revision of the goal.
    /// Turning a host On from Off forgets every listener's memory, so each
    /// starts afresh; clearing it forgets them too. The caller — the engine's
    /// one door, which also says so on the bus — has already judged whether
    /// the host may listen.
    pub fn set_listening(
        &self,
        host: &ListenerHost,
        listening: Option<Listening>,
    ) -> Result<(), StoreError> {
        let before = self.listening(host)?;
        match host {
            ListenerHost::Workspace { workflow } => {
                let wf = self.get_workflow(*workflow)?;
                if wf.origin.goal().is_some() {
                    return Err(StoreError::Invalid(bisa_core::text!(
                        "error-store-invalid-listening-goal-design",
                        workflow = workflow.to_string()
                    )));
                }
                let path = self.paths.listening_file(*workflow);
                match &listening {
                    Some(l) => crate::paths::write_atomic(&path, &serde_json::to_vec_pretty(l)?)?,
                    None => match std::fs::remove_file(&path) {
                        Ok(()) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(e) => return Err(StoreError::io(path.display().to_string(), e)),
                    },
                }
                self.idx().set_workflow_listening(
                    &workflow.to_string(),
                    listening.as_ref().map(|l| l.since),
                )?;
            }
            ListenerHost::Goal { goal } => self.set_goal_listening(*goal, listening.clone())?,
        }
        let turned_on = before.is_none() && listening.is_some();
        if turned_on || listening.is_none() {
            self.forget_listener_runtimes(host)?;
        }
        Ok(())
    }

    /// A goal's standing, on its own snapshot: a revision of the goal.
    fn set_goal_listening(
        &self,
        goal: GoalId,
        listening: Option<Listening>,
    ) -> Result<(), StoreError> {
        let mut g = self.get_goal(goal)?;
        if g.listening == listening {
            return Ok(());
        }
        if listening.is_some() && g.is_closed() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-listening-goal-closed",
                goal = goal.to_string()
            )));
        }
        g.listening = listening;
        g.revision += 1;
        self.write_goal_snapshot(&g, now_secs())?;
        self.index_goal(&g, None)
    }

    /// Every host that listens right now, paused ones included: the library
    /// workflows that are On, then the goals, each oldest first. A record the
    /// index names that is gone, or that this build cannot read, costs its
    /// own host and never the list: it is said, and the others listen on.
    pub fn list_listening(&self) -> Result<Vec<(ListenerHost, Listening)>, StoreError> {
        let (workflows, goals) = {
            let idx = self.idx();
            (idx.listening_workflow_ids()?, idx.listening_goal_ids()?)
        };
        let mut out = Vec::with_capacity(workflows.len() + goals.len());
        for id in workflows {
            let Ok(workflow) = id.parse::<WorkflowId>() else {
                continue;
            };
            let read = crate::workspace::tolerated(
                "listening record",
                &id,
                self.workflow_listening(workflow),
            )?;
            if let Some(l) = read.flatten() {
                out.push((ListenerHost::Workspace { workflow }, l));
            }
        }
        for id in goals {
            let Ok(goal) = id.parse::<GoalId>() else {
                continue;
            };
            let read = crate::workspace::tolerated("listening goal", &id, self.get_goal(goal))?;
            if let Some(l) = read.and_then(|g| g.listening) {
                out.push((ListenerHost::Goal { goal }, l));
            }
        }
        Ok(out)
    }

    /// How many runs one listener began are unfinished — queued, running or
    /// waiting: what its guard counts, never its dispatches.
    pub fn live_runs_of_listener(&self, key: &ListenerKey) -> Result<u32, StoreError> {
        self.idx().live_runs_of_listener(&key.to_string())
    }

    /// The runs one listener began, newest first.
    pub fn runs_of_listener(
        &self,
        key: &ListenerKey,
        limit: usize,
    ) -> Result<Vec<bisa_core::RunId>, StoreError> {
        Ok(self
            .idx()
            .runs_of_listener(&key.to_string(), limit)?
            .iter()
            .filter_map(|row| row.id.parse().ok())
            .collect())
    }

    /// One listener's memory; the default when there is none.
    pub fn listener_runtime(&self, key: &ListenerKey) -> ListenerRuntime {
        std::fs::read(self.paths.listener_runtime_file(key))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn put_listener_runtime(
        &self,
        key: &ListenerKey,
        runtime: &ListenerRuntime,
    ) -> Result<(), StoreError> {
        let path = self.paths.listener_runtime_file(key);
        crate::paths::write_atomic(&path, &serde_json::to_vec_pretty(runtime)?)
    }

    /// Forget every listener memory of one host — it was turned On anew, or
    /// it is gone.
    pub(crate) fn forget_listener_runtimes(&self, host: &ListenerHost) -> Result<(), StoreError> {
        let dir = self.paths.listeners_dir();
        let prefix = Paths::host_stem_prefix(host);
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(StoreError::io(dir.display().to_string(), e)),
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(&prefix) && name.ends_with(".json") {
                let path = entry.path();
                match std::fs::remove_file(&path) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(StoreError::io(path.display().to_string(), e)),
                }
            }
        }
        Ok(())
    }

    /// A public hook start's secret, minted when it has none — what turning
    /// its host On and adopting a design do. `Some(secret)` only when this
    /// call minted it: a secret is shown once.
    pub fn ensure_hook_secret(&self, key: &ListenerKey) -> Result<Option<Vec<u8>>, StoreError> {
        if self.identity.has_hook_secret(key)? {
            return Ok(None);
        }
        self.identity.mint_hook_secret(key).map(Some)
    }

    /// Replace a hook start's secret; the old one stops verifying at once.
    pub fn rotate_hook_secret(&self, key: &ListenerKey) -> Result<Vec<u8>, StoreError> {
        self.identity.rotate_hook_secret(key)
    }

    /// The secret a public hook call is verified against.
    pub fn hook_secret(&self, key: &ListenerKey) -> Result<Vec<u8>, StoreError> {
        self.identity.hook_secret(key)
    }

    pub fn has_hook_secret(&self, key: &ListenerKey) -> Result<bool, StoreError> {
        self.identity.has_hook_secret(key)
    }

    /// Remove a hook start's secret — its start is gone, or its host.
    pub fn delete_hook_secret(&self, key: &ListenerKey) -> Result<(), StoreError> {
        self.identity.delete_hook_secret(key)
    }

    /// Everything a host that is gone leaves on this machine: its record, its
    /// listeners' memories, its hook secrets, its signals in the index.
    pub(crate) fn forget_host(
        &self,
        host: &ListenerHost,
        hook_steps: &[bisa_core::StepId],
    ) -> Result<(), StoreError> {
        if let ListenerHost::Workspace { workflow } = host {
            let path = self.paths.listening_file(*workflow);
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(StoreError::io(path.display().to_string(), e)),
            }
        }
        for step in hook_steps {
            let key = ListenerKey {
                host: *host,
                step: step.clone(),
            };
            if let Err(e) = self.identity.delete_hook_secret(&key) {
                tracing::warn!("{key}: hook secret not removed: {e}");
            }
        }
        self.forget_listener_runtimes(host)?;
        self.forget_signals_of(host)
    }

    /// Rebuild support: restore the library's listening column from the
    /// records. A record whose workflow is gone restores nothing.
    pub(crate) fn reindex_listening(&self) -> Result<(), StoreError> {
        let dir = self.paths.listening_dir();
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(StoreError::io(dir.display().to_string(), e)),
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(workflow) = name
                .strip_suffix(".json")
                .and_then(|stem| stem.parse::<WorkflowId>().ok())
            else {
                continue;
            };
            let Some(listening) = self
                .tolerated_record(
                    "listening record",
                    &workflow.to_string(),
                    self.workflow_listening(workflow),
                )?
                .flatten()
            else {
                continue;
            };
            let idx = self.idx();
            if idx.get_workflow(&workflow.to_string())?.is_some() {
                idx.set_workflow_listening(&workflow.to_string(), Some(listening.since))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::MemoryKeyStore;
    use crate::workspace::NewGoal;
    use bisa_core::{PauseReason, Paused, RunId, StepId};

    fn ws() -> (tempfile::TempDir, Workspace, WorkflowId) {
        let dir = tempfile::tempdir().unwrap();
        let w =
            Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
        let wf = w
            .create_workflow(
                crate::workflows::tests::notify_workflow("Post"),
                bisa_core::WorkflowOrigin::Workspace,
            )
            .unwrap();
        (dir, w, wf.id)
    }

    fn on(since: u64) -> Listening {
        Listening {
            inputs: BTreeMap::new(),
            budget: None,
            since,
            paused: None,
        }
    }

    fn key(host: ListenerHost, step: &str) -> ListenerKey {
        ListenerKey {
            host,
            step: StepId::new(step).unwrap(),
        }
    }

    #[test]
    fn a_workflow_turned_on_writes_its_record_and_the_index_and_off_clears_both() {
        let (_dir, ws, wf) = ws();
        let host = ListenerHost::Workspace { workflow: wf };
        assert_eq!(ws.listening(&host).unwrap(), None);
        let revision = ws.get_workflow(wf).unwrap().revision;
        ws.set_listening(&host, Some(on(7))).unwrap();
        assert_eq!(ws.listening(&host).unwrap(), Some(on(7)));
        assert!(ws.paths().listening_file(wf).exists());
        assert_eq!(
            ws.idx().workflow_listening_since(&wf.to_string()).unwrap(),
            Some(7)
        );
        assert_eq!(
            ws.get_workflow(wf).unwrap().revision,
            revision,
            "turning a workflow On is no revision of it"
        );
        assert_eq!(ws.list_listening().unwrap(), vec![(host, on(7))]);
        ws.set_listening(&host, None).unwrap();
        assert_eq!(ws.listening(&host).unwrap(), None);
        assert!(!ws.paths().listening_file(wf).exists());
        assert!(ws.list_listening().unwrap().is_empty());
    }

    #[test]
    fn a_listening_record_nobody_can_read_costs_its_own_host_and_never_the_list() {
        let (_dir, ws, first) = ws();
        let second = ws
            .create_workflow(
                crate::workflows::tests::notify_workflow("Digest"),
                bisa_core::WorkflowOrigin::Workspace,
            )
            .unwrap()
            .id;
        let torn = ListenerHost::Workspace { workflow: first };
        let sound = ListenerHost::Workspace { workflow: second };
        ws.set_listening(&torn, Some(on(3))).unwrap();
        ws.set_listening(&sound, Some(on(4))).unwrap();
        // A record of another shape, in the test's own temporary workspace.
        std::fs::write(ws.paths().listening_file(first), b"{ \"since\": ").unwrap();
        assert!(
            ws.listening(&torn).is_err(),
            "the one read still refuses it by name"
        );
        assert_eq!(
            ws.list_listening().unwrap(),
            vec![(sound, on(4))],
            "the list goes on without it"
        );
    }

    #[test]
    fn a_goal_listens_on_its_own_snapshot_and_a_closed_one_cannot() {
        let (_dir, ws, _wf) = ws();
        let goal = ws
            .create_goal(NewGoal::captured("post every Monday"))
            .unwrap();
        let host = ListenerHost::Goal { goal: goal.id };
        ws.set_listening(&host, Some(on(9))).unwrap();
        let g = ws.get_goal(goal.id).unwrap();
        assert_eq!(g.listening, Some(on(9)));
        assert_eq!(g.revision, goal.revision + 1);
        assert_eq!(
            ws.idx().listening_goal_ids().unwrap(),
            vec![goal.id.to_string()]
        );
        let paused = Listening {
            paused: Some(Paused {
                reason: PauseReason::RunFailed {
                    run: RunId::from_ulid(ulid::Ulid::from_parts(3, 3)),
                },
                at: 11,
            }),
            ..on(9)
        };
        ws.set_listening(&host, Some(paused.clone())).unwrap();
        assert_eq!(ws.list_listening().unwrap(), vec![(host, paused)]);
        ws.set_listening(&host, None).unwrap();
        assert!(ws.idx().listening_goal_ids().unwrap().is_empty());
        ws.set_goal_closed(
            goal.id,
            bisa_core::ClosureReason::Abandoned { rationale: None },
        )
        .unwrap();
        assert!(ws.set_listening(&host, Some(on(12))).is_err());
    }

    #[test]
    fn a_goals_own_design_is_never_turned_on_by_itself() {
        let (_dir, ws, _wf) = ws();
        let goal = ws.create_goal(NewGoal::captured("design me")).unwrap();
        let design = ws
            .create_workflow(
                crate::workflows::tests::notify_workflow("For the goal"),
                bisa_core::WorkflowOrigin::Goal { goal: goal.id },
            )
            .unwrap();
        assert!(ws
            .set_listening(
                &ListenerHost::Workspace {
                    workflow: design.id
                },
                Some(on(1))
            )
            .is_err());
    }

    #[test]
    fn a_listeners_memory_starts_afresh_when_its_host_turns_on_and_goes_with_it() {
        let (_dir, ws, wf) = ws();
        let host = ListenerHost::Workspace { workflow: wf };
        let k = key(host, "nightly");
        let memory = ListenerRuntime {
            next_due: Some(100),
            seen: Some(vec!["a".into()]),
            ..ListenerRuntime::default()
        };
        ws.put_listener_runtime(&k, &memory).unwrap();
        assert_eq!(ws.listener_runtime(&k), memory);
        ws.set_listening(&host, Some(on(1))).unwrap();
        assert_eq!(
            ws.listener_runtime(&k),
            ListenerRuntime::default(),
            "no catch-up: On starts afresh"
        );
        ws.put_listener_runtime(&k, &memory).unwrap();
        ws.set_listening(&host, Some(on(1))).unwrap();
        assert_eq!(ws.listener_runtime(&k), memory, "On again changes nothing");
        ws.rebuild_index().unwrap();
        assert_eq!(
            ws.listener_runtime(&k),
            memory,
            "a rebuild keeps the memory"
        );
        assert_eq!(
            ws.idx().workflow_listening_since(&wf.to_string()).unwrap(),
            Some(1),
            "a rebuild restores the column from the record"
        );
        ws.set_listening(&host, None).unwrap();
        assert_eq!(ws.listener_runtime(&k), ListenerRuntime::default());
    }

    #[test]
    fn a_hook_secret_is_minted_once_kept_across_off_and_on_and_rotated_on_request() {
        let (_dir, ws, wf) = ws();
        let host = ListenerHost::Workspace { workflow: wf };
        let k = key(host, "ticket");
        let first = ws.ensure_hook_secret(&k).unwrap().expect("minted");
        assert_eq!(first.len(), 32);
        assert_eq!(ws.ensure_hook_secret(&k).unwrap(), None, "shown once");
        ws.set_listening(&host, Some(on(1))).unwrap();
        ws.set_listening(&host, None).unwrap();
        assert_eq!(ws.hook_secret(&k).unwrap(), first, "kept across Off and On");
        let rotated = ws.rotate_hook_secret(&k).unwrap();
        assert_ne!(rotated, first);
        assert_eq!(ws.hook_secret(&k).unwrap(), rotated);
        ws.forget_host(&host, std::slice::from_ref(&k.step))
            .unwrap();
        assert!(!ws.has_hook_secret(&k).unwrap(), "gone with its host");
    }

    #[test]
    fn a_secret_never_reaches_a_record_or_the_index() {
        let (_dir, ws, wf) = ws();
        let host = ListenerHost::Workspace { workflow: wf };
        let k = key(host, "ticket");
        let secret = ws.ensure_hook_secret(&k).unwrap().unwrap();
        ws.set_listening(&host, Some(on(1))).unwrap();
        let hex_secret = hex::encode(&secret);
        let record = std::fs::read_to_string(ws.paths().listening_file(wf)).unwrap();
        assert!(!record.contains(&hex_secret));
        assert!(!record.to_lowercase().contains("secret"));
    }
}
