//! Addressable state snapshots: the current Goal / Plan / WorkItemSpec /
//! Channel / Agent / … stored as signed `kind:334xx` events in
//! `<namespace>/state/<kind>-<d>.json`.
//!
//! One store, many namespaces. A namespace is a relative directory under the
//! workspace root — `goals/<GoalId>`, `channels`, `agents/<id>/recall` — and
//! every spelling of one comes from [`Paths`], never from a literal here.
//!
//! **Revision first.** The `revision` tag is the snapshot authority; the
//! clock only breaks a tie between two authors at the same revision
//! ([`SnapshotStore::apply_remote`]). A local writer that edited revision N
//! writes through [`SnapshotStore::put_expecting`], which refuses when the
//! stored revision is no longer N — two writers a second apart cannot both
//! win, and the loser learns it by name ([`StoreError::RevisionConflict`]).
//! [`SnapshotStore::put`] is the plain latest-wins write for objects with one
//! writer. Snapshots are caches with respect to the journal for goals, and
//! truth for the objects that have no journal (channels, projects, workflows).

use crate::error::StoreError;
use crate::paths::Paths;
use bisa_core::kind;
use nostr::event::{Event, EventBuilder, FinalizeEvent, Kind, Tag};
use nostr::key::{Keys, PublicKey};
use nostr::nips::nip44;
use nostr::types::Timestamp;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::path::{Path, PathBuf};

pub struct SnapshotStore {
    paths: Paths,
    /// Where a file moved aside on a write is named; a store with none
    /// says it in the log alone.
    problems: Option<crate::problems::ProblemSink>,
}

impl SnapshotStore {
    pub fn new(paths: Paths) -> Self {
        Self {
            paths,
            problems: None,
        }
    }

    /// A store that names what it moves aside in the workspace's problems.
    pub(crate) fn with_problems(paths: Paths, problems: crate::problems::ProblemSink) -> Self {
        Self {
            paths,
            problems: Some(problems),
        }
    }

    /// The stored event at `path` for a write that replaces it, or `None`
    /// for a file this build cannot read — moved under `quarantine/` and
    /// named, so the object can be written again rather than blocked for
    /// good by one torn file. A caller that read the object first never
    /// gets here with such a file; this is for the writes that do not.
    fn existing_or_quarantined(
        &self,
        path: &Path,
        wire_kind: u16,
        d: &str,
    ) -> Result<Option<Event>, StoreError> {
        match self.load_event(path) {
            Ok(existing) => Ok(existing),
            Err(StoreError::Unreadable { what, reason, .. }) => {
                let to = self.paths.quarantine(path, crate::workspace::now_secs())?;
                let problem = crate::problems::WorkspaceProblem::new(
                    crate::problems::ProblemKind::Quarantined,
                    path.display().to_string(),
                    bisa_core::text!(
                        "error-store-problem-quarantined",
                        path = path.display().to_string(),
                        what = format!("{what} of kind {wire_kind} ({d})"),
                        reason = reason,
                        to = to.display().to_string()
                    ),
                    Some(&to),
                );
                match &self.problems {
                    Some(sink) => sink.record(problem),
                    None => tracing::error!(target: "bisa_store", "{}", problem.text),
                }
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    pub(crate) fn state_dir(&self, ns: &str) -> PathBuf {
        self.paths.state_dir(ns)
    }

    fn path_for(&self, ns: &str, wire_kind: u16, d: &str) -> Result<PathBuf, StoreError> {
        Ok(self.state_dir(ns).join(format!(
            "{wire_kind}-{}.json",
            Paths::stem("snapshot d", d)?
        )))
    }

    /// The `revision` tag of a snapshot event (0 when absent). Callers deriving
    /// the next revision for an update use this, never wall-clock.
    pub fn revision_of(event: &Event) -> u64 {
        event
            .tags
            .iter()
            .find_map(|t| {
                let s = t.as_slice();
                (s.len() >= 2 && s[0] == "revision")
                    .then(|| s[1].parse().ok())
                    .flatten()
            })
            .unwrap_or(0)
    }

    /// The current revision stored for `(kind, d)`, or 0.
    pub fn current_revision(&self, ns: &str, wire_kind: u16, d: &str) -> Result<u64, StoreError> {
        Ok(self
            .get_raw(ns, wire_kind, d)?
            .map(|ev| Self::revision_of(&ev))
            .unwrap_or(0))
    }

    /// Sign and store a snapshot. `value` becomes the event content; `d`
    /// addresses it; `revision` is the monotonic snapshot authority — a write
    /// at or below the stored `(revision, created_at)` is refused.
    /// `encrypt_to` NIP-44-encrypts the content between `signer` and that
    /// pubkey (required for ENCRYPT_TO_OWNER kinds — plaintext is refused).
    #[allow(clippy::too_many_arguments)]
    pub fn put<T: Serialize>(
        &self,
        ns: &str,
        wire_kind: u16,
        d: &str,
        value: &T,
        revision: u64,
        signer: &Keys,
        at: u64,
        encrypt_to: Option<&PublicKey>,
        // The object's tags, emitted as NIP-12 `t` tags. A parameter rather
        // than something sniffed out of the content, so every caller's answer
        // is explicit.
        topics: &[String],
    ) -> Result<Event, StoreError> {
        if !kind::is_addressable(wire_kind) {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-kind-not-addressable",
                wire_kind = wire_kind.to_string()
            )));
        }
        if kind::requires_owner_encryption(wire_kind) && encrypt_to.is_none() {
            return Err(StoreError::EncryptionRequired(wire_kind));
        }
        let path = self.path_for(ns, wire_kind, d)?;
        if let Some(existing) = self.existing_or_quarantined(&path, wire_kind, d)? {
            let newer =
                (Self::revision_of(&existing), existing.created_at.as_secs()) >= (revision, at);
            if newer {
                return Err(StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-stale-snapshot-write-stored-rev-new-rev",
                    wire_kind = wire_kind.to_string(),
                    d = d.to_string(),
                    a0 = (Self::revision_of(&existing)).to_string(),
                    a1 = (existing.created_at.as_secs()).to_string(),
                    revision = revision.to_string(),
                    at = at.to_string()
                )));
            }
        }
        self.write_signed(
            &path, wire_kind, d, value, revision, signer, at, encrypt_to, topics,
        )
    }

    /// The compare-and-swap write: the stored snapshot must be at exactly
    /// `expected` (0 when there is none), and the new one is written at
    /// `expected + 1`. Anything else is a [`StoreError::RevisionConflict`]
    /// naming `what` and `d`, and nothing is written. This is how two editors
    /// of one object — or two step completions on one run — cannot lose each
    /// other's work.
    #[allow(clippy::too_many_arguments)]
    pub fn put_expecting<T: Serialize>(
        &self,
        ns: &str,
        wire_kind: u16,
        d: &str,
        what: &'static str,
        value: &T,
        expected: u64,
        signer: &Keys,
        at: u64,
        encrypt_to: Option<&PublicKey>,
        topics: &[String],
    ) -> Result<Event, StoreError> {
        if !kind::is_addressable(wire_kind) {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-kind-not-addressable",
                wire_kind = wire_kind.to_string()
            )));
        }
        if kind::requires_owner_encryption(wire_kind) && encrypt_to.is_none() {
            return Err(StoreError::EncryptionRequired(wire_kind));
        }
        let path = self.path_for(ns, wire_kind, d)?;
        let actual = self
            .load_event(&path)?
            .map(|ev| Self::revision_of(&ev))
            .unwrap_or(0);
        if actual != expected {
            return Err(StoreError::RevisionConflict {
                kind: what,
                id: d.to_string(),
                expected,
                actual,
            });
        }
        self.write_signed(
            &path,
            wire_kind,
            d,
            value,
            expected + 1,
            signer,
            at,
            encrypt_to,
            topics,
        )
    }

    /// Sign `value` as a snapshot event and write it atomically at `path`.
    /// The ordering checks are the callers'; this is the one place the event
    /// is built.
    #[allow(clippy::too_many_arguments)]
    fn write_signed<T: Serialize>(
        &self,
        path: &Path,
        wire_kind: u16,
        d: &str,
        value: &T,
        revision: u64,
        signer: &Keys,
        at: u64,
        encrypt_to: Option<&PublicKey>,
        topics: &[String],
    ) -> Result<Event, StoreError> {
        let d_tag = Tag::parse(["d", d]).map_err(StoreError::nostr)?;
        let rev_tag = Tag::parse(["revision", &revision.to_string()]).map_err(StoreError::nostr)?;
        let plaintext = serde_json::to_string(value)?;
        let (content, extra_tag) = match encrypt_to {
            Some(pk) => {
                let ct = nip44::encrypt(signer.secret_key(), pk, &plaintext, nip44::Version::V2)
                    .map_err(StoreError::nostr)?;
                let p_tag = Tag::parse(["p", &pk.to_hex()]).map_err(StoreError::nostr)?;
                (ct, Some(p_tag))
            }
            None => (plaintext, None),
        };
        let mut builder = EventBuilder::new(Kind::from(wire_kind), content).tags([d_tag, rev_tag]);
        if let Some(t) = extra_tag {
            builder = builder.tag(t);
        }
        for topic in topics {
            builder = builder.tag(Tag::parse(["t", topic]).map_err(StoreError::nostr)?);
        }
        let event = builder
            .custom_created_at(Timestamp::from_secs(at))
            .finalize(signer)
            .map_err(StoreError::nostr)?;
        crate::paths::write_atomic(path, &serde_json::to_vec(&event)?)?;
        Ok(event)
    }

    /// Apply a REMOTE snapshot event verbatim (never re-signed). Latest-wins
    /// across authors on `(revision tag, created_at)` — the higher revision
    /// wins, and the clock only decides between two at the same revision;
    /// returns `Ok(false)` when the stored snapshot is newer or identical.
    /// The caller has already verified the signature and authorized the
    /// author.
    pub fn apply_remote(&self, ns: &str, event: &Event) -> Result<bool, StoreError> {
        let wire_kind = event.kind.as_u16();
        if !kind::is_addressable(wire_kind) {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-kind-not-addressable",
                wire_kind = wire_kind.to_string()
            )));
        }
        let d = event
            .tags
            .iter()
            .find_map(|t| {
                let s = t.as_slice();
                (s.len() >= 2 && s[0] == "d").then(|| s[1].clone())
            })
            .ok_or_else(|| {
                StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-snapshot-event-has-no-d-tag"
                ))
            })?;
        let path = self.path_for(ns, wire_kind, &d)?;
        if let Some(existing) = self.load_event(&path)? {
            if existing.id == event.id {
                return Ok(false);
            }
            let newer = (Self::revision_of(&existing), existing.created_at.as_secs())
                >= (Self::revision_of(event), event.created_at.as_secs());
            if newer {
                return Ok(false);
            }
        }
        crate::paths::write_atomic(&path, &serde_json::to_vec(event)?)?;
        Ok(true)
    }

    pub(crate) fn load_event(&self, path: &Path) -> Result<Option<Event>, StoreError> {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(StoreError::io(path.display().to_string(), e)),
        };
        // A file that is there but is not a signed event is *unreadable*, and
        // said so — never *absent*: a caller that reads one refuses by name,
        // and a rebuild skips it by name (`tolerated`) rather than forgetting
        // a record ever existed.
        let event: Event = serde_json::from_slice(&bytes)
            .map_err(|e| StoreError::unreadable(path, "snapshot event", e))?;
        event
            .verify()
            .map_err(|e| StoreError::unreadable(path, "snapshot event", e))?;
        Ok(Some(event))
    }

    /// Load and deserialize the current snapshot for `(kind, d)`.
    ///
    /// A snapshot whose content this build cannot read as `T` is
    /// [`StoreError::Unreadable`], never `None` and never a bare parse error:
    /// the file is there and it was written by another shape of the code,
    /// and the refusal has to say so and name the file.
    pub fn get<T: DeserializeOwned>(
        &self,
        ns: &str,
        wire_kind: u16,
        d: &str,
    ) -> Result<Option<(T, Event)>, StoreError> {
        let path = self.path_for(ns, wire_kind, d)?;
        match self.load_event(&path)? {
            None => Ok(None),
            Some(event) => {
                let value = serde_json::from_str(&event.content)
                    .map_err(|e| StoreError::unreadable(&path, record_name::<T>(), e))?;
                Ok(Some((value, event)))
            }
        }
    }

    /// Load the raw snapshot event (callers that decrypt or inspect tags).
    pub fn get_raw(&self, ns: &str, wire_kind: u16, d: &str) -> Result<Option<Event>, StoreError> {
        self.load_event(&self.path_for(ns, wire_kind, d)?)
    }

    /// Delete one snapshot file (the caller also removes any index rows).
    pub fn delete_snapshot(&self, ns: &str, wire_kind: u16, d: &str) -> Result<(), StoreError> {
        let path = self.path_for(ns, wire_kind, d)?;
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(StoreError::io(path.display().to_string(), e)),
        }
    }

    /// List every `d` with a stored snapshot of `wire_kind` in a namespace.
    pub fn list_ds(&self, ns: &str, wire_kind: u16) -> Result<Vec<String>, StoreError> {
        let dir = self.state_dir(ns);
        let prefix = format!("{wire_kind}-");
        let mut out = Vec::new();
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(StoreError::io(dir.display().to_string(), e)),
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(rest) = name.strip_prefix(&prefix) {
                if let Some(d) = rest.strip_suffix(".json") {
                    out.push(d.to_string());
                }
            }
        }
        out.sort();
        Ok(out)
    }
}

/// The record a snapshot should have been, as a person would name it: the
/// type's own name, lower-cased and split on its capitals (`WorkflowRun` →
/// "workflow run").
fn record_name<T>() -> &'static str {
    let full = std::any::type_name::<T>();
    let short = full.rsplit("::").next().unwrap_or(full);
    // A handful of names the refusal message reads better with; anything else
    // falls back to the type name, which is still the truth.
    match short {
        "Goal" => "goal",
        "WorkItemSpec" => "work item",
        "Workflow" => "workflow",
        "WorkflowRun" => "workflow run",
        "Channel" => "channel",
        "Agent" => "agent",
        "Team" => "team",
        "Skill" => "skill",
        "Project" => "project",
        _ => "record",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_core::kind::{KIND_GOAL, KIND_WORK_ITEM};

    #[test]
    fn an_undecodable_snapshot_is_unreadable_not_none() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let keys = Keys::generate();
        let ns = "goals/01J";
        store
            .put(
                ns,
                KIND_GOAL,
                "01J",
                &serde_json::json!({"state": "shaping", "statement": "old shape"}),
                1,
                &keys,
                100,
                None,
                &[],
            )
            .unwrap();
        let err = store
            .get::<bisa_core::Goal>(ns, KIND_GOAL, "01J")
            .unwrap_err();
        match err {
            StoreError::Unreadable { what, path, .. } => {
                assert_eq!(what, "goal");
                assert!(path.contains("33400-01J.json"), "{path}");
            }
            other => panic!("expected Unreadable, got {other:?}"),
        }
    }

    fn store(dir: &Path) -> SnapshotStore {
        SnapshotStore::new(Paths::new(dir))
    }

    #[test]
    fn latest_wins_and_stale_write_refused() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let keys = Keys::generate();
        let ns = "goals/01J";
        store
            .put(
                ns,
                KIND_GOAL,
                "01J",
                &serde_json::json!({"v": 1}),
                1,
                &keys,
                100,
                None,
                &[],
            )
            .unwrap();
        store
            .put(
                ns,
                KIND_GOAL,
                "01J",
                &serde_json::json!({"v": 2}),
                2,
                &keys,
                200,
                None,
                &[],
            )
            .unwrap();
        // The same revision at the same instant: nothing newer, refused.
        assert!(store
            .put(
                ns,
                KIND_GOAL,
                "01J",
                &serde_json::json!({"v": 0}),
                2,
                &keys,
                200,
                None,
                &[]
            )
            .is_err());
        // The same revision later on the clock lands: revision first, and the
        // clock decides between two writers at one revision (08 — Persistence).
        store
            .put(
                ns,
                KIND_GOAL,
                "01J",
                &serde_json::json!({"v": 2, "later": true}),
                2,
                &keys,
                300,
                None,
                &[],
            )
            .unwrap();
        // A lower revision with a later clock: refused — revision first.
        assert!(store
            .put(
                ns,
                KIND_GOAL,
                "01J",
                &serde_json::json!({"v": 0}),
                1,
                &keys,
                300,
                None,
                &[]
            )
            .is_err());
        let (value, event) = store
            .get::<serde_json::Value>(ns, KIND_GOAL, "01J")
            .unwrap()
            .unwrap();
        assert_eq!(value["v"], 2);
        assert_eq!(SnapshotStore::revision_of(&event), 2);
        assert_eq!(store.current_revision(ns, KIND_GOAL, "01J").unwrap(), 2);
    }

    #[test]
    fn same_second_writes_do_not_both_succeed() {
        // Two writers read revision 1 and both write within the same second.
        // Under a compare-and-swap the second learns the object moved; the
        // first's content is what is stored.
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let keys = Keys::generate();
        let ns = "goals/01J";
        store
            .put_expecting(
                ns,
                KIND_GOAL,
                "01J",
                "goal",
                &serde_json::json!({"v": 1}),
                0,
                &keys,
                100,
                None,
                &[],
            )
            .unwrap();
        store
            .put_expecting(
                ns,
                KIND_GOAL,
                "01J",
                "goal",
                &serde_json::json!({"v": "first"}),
                1,
                &keys,
                200,
                None,
                &[],
            )
            .unwrap();
        let err = store
            .put_expecting(
                ns,
                KIND_GOAL,
                "01J",
                "goal",
                &serde_json::json!({"v": "second"}),
                1,
                &keys,
                200,
                None,
                &[],
            )
            .unwrap_err();
        match &err {
            StoreError::RevisionConflict {
                kind,
                id,
                expected,
                actual,
            } => {
                assert_eq!(
                    (*kind, id.as_str(), *expected, *actual),
                    ("goal", "01J", 1, 2)
                );
            }
            other => panic!("expected a revision conflict, got {other:?}"),
        }
        assert!(err.is_refusal());
        let (value, event) = store
            .get::<serde_json::Value>(ns, KIND_GOAL, "01J")
            .unwrap()
            .unwrap();
        assert_eq!(value["v"], "first");
        assert_eq!(SnapshotStore::revision_of(&event), 2);
    }

    #[test]
    fn put_expecting_refuses_a_moved_revision() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let keys = Keys::generate();
        let ns = "workflows";
        // Nothing stored: only `expected = 0` may create.
        assert!(matches!(
            store.put_expecting(
                ns,
                33412,
                "w",
                "workflow",
                &serde_json::json!({}),
                1,
                &keys,
                1,
                None,
                &[]
            ),
            Err(StoreError::RevisionConflict {
                expected: 1,
                actual: 0,
                ..
            })
        ));
        store
            .put_expecting(
                ns,
                33412,
                "w",
                "workflow",
                &serde_json::json!({}),
                0,
                &keys,
                1,
                None,
                &[],
            )
            .unwrap();
        store
            .put_expecting(
                ns,
                33412,
                "w",
                "workflow",
                &serde_json::json!({}),
                1,
                &keys,
                2,
                None,
                &[],
            )
            .unwrap();
        // A stale editor (still at 1) and a jumping one (at 5) are both refused.
        assert!(matches!(
            store.put_expecting(
                ns,
                33412,
                "w",
                "workflow",
                &serde_json::json!({}),
                1,
                &keys,
                3,
                None,
                &[]
            ),
            Err(StoreError::RevisionConflict {
                expected: 1,
                actual: 2,
                ..
            })
        ));
        assert!(matches!(
            store.put_expecting(
                ns,
                33412,
                "w",
                "workflow",
                &serde_json::json!({}),
                5,
                &keys,
                3,
                None,
                &[]
            ),
            Err(StoreError::RevisionConflict {
                expected: 5,
                actual: 2,
                ..
            })
        ));
        assert_eq!(store.current_revision(ns, 33412, "w").unwrap(), 2);
        let err = store
            .put_expecting(
                ns,
                33412,
                "w",
                "workflow",
                &serde_json::json!({}),
                1,
                &keys,
                3,
                None,
                &[],
            )
            .unwrap_err()
            .to_string();
        assert!(err.contains("revision 2"), "{err}");
        assert!(err.contains("reload and merge"), "{err}");
    }

    #[test]
    fn remote_apply_prefers_higher_revision_over_later_clock() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let ns = "goals/01J";
        let a = Keys::generate();
        let b = Keys::generate();
        // A peer's events, each built in its own namespace of a scratch store
        // so the peer's own ordering rule never gets in the way.
        let peer_dir = tempfile::tempdir().unwrap();
        let peer = SnapshotStore::new(Paths::new(peer_dir.path()));
        let make = |keys: &Keys, rev: u64, at: u64| {
            peer.put(
                &format!("peer/{rev}-{at}"),
                KIND_GOAL,
                "01J",
                &serde_json::json!({"rev": rev}),
                rev,
                keys,
                at,
                None,
                &[],
            )
            .unwrap()
        };
        assert!(store.apply_remote(ns, &make(&a, 3, 100)).unwrap());
        // A lower revision written later on someone's clock does not win.
        assert!(!store.apply_remote(ns, &make(&b, 2, 200)).unwrap());
        // A higher revision written earlier does.
        assert!(store.apply_remote(ns, &make(&b, 4, 50)).unwrap());
        // Same revision: the later clock breaks the tie.
        assert!(store.apply_remote(ns, &make(&a, 4, 60)).unwrap());
        assert!(!store.apply_remote(ns, &make(&b, 4, 55)).unwrap());
        let (value, event) = store
            .get::<serde_json::Value>(ns, KIND_GOAL, "01J")
            .unwrap()
            .unwrap();
        assert_eq!(value["rev"], 4);
        assert_eq!(event.created_at.as_secs(), 60);
    }

    #[test]
    fn non_addressable_kind_and_unsafe_d_refused() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let keys = Keys::generate();
        assert!(store
            .put(
                "goals/01J",
                3400,
                "x",
                &serde_json::json!({}),
                1,
                &keys,
                1,
                None,
                &[]
            )
            .is_err());
        assert!(store
            .put(
                "goals/01J",
                KIND_GOAL,
                "../x",
                &serde_json::json!({}),
                1,
                &keys,
                1,
                None,
                &[]
            )
            .is_err());
    }

    #[test]
    fn listing_work_items() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let keys = Keys::generate();
        let ns = "goals/01J";
        for d in ["wi-b", "wi-a"] {
            store
                .put(
                    ns,
                    KIND_WORK_ITEM,
                    d,
                    &serde_json::json!({}),
                    1,
                    &keys,
                    1,
                    None,
                    &[],
                )
                .unwrap();
        }
        assert_eq!(
            store.list_ds(ns, KIND_WORK_ITEM).unwrap(),
            vec!["wi-a", "wi-b"]
        );
    }
}
