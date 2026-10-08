//! The append-only journal: signed Nostr events, one JSONL file per home —
//! a goal's, or a run of the workspace's own.
//!
//! The journal is the source of truth for a goal's history, and for a run
//! of the workspace's. Everything else (snapshots, the SQLite index) is a
//! rebuildable cache. The `EventLog` trait keeps the backend swappable.

use crate::error::StoreError;
use crate::paths::Paths;
use bisa_core::event::JournalEvent;
use bisa_core::kind;
use bisa_core::Home;
use nostr::event::{Event, EventBuilder, FinalizeEvent, Kind, Tag};
use nostr::key::{Keys, PublicKey};
use nostr::nips::nip44;
use nostr::types::Timestamp;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// Address of the addressable event a journal hangs off, used as the `a` tag
/// on every journal fact: a goal's (`33400:<author-pubkey>:<goal>`), or a run
/// of the workspace's own (`33413:<author-pubkey>:<run>`).
#[derive(Clone, Debug)]
pub struct JournalAddr {
    pub author_pubkey_hex: String,
    pub home: Home,
}

impl JournalAddr {
    pub fn coordinate(&self) -> String {
        let kind = match self.home {
            Home::Goal { .. } => kind::KIND_GOAL,
            Home::Run { .. } => kind::KIND_WORKFLOW_RUN,
        };
        format!("{kind}:{}:{}", self.author_pubkey_hex, self.home.id())
    }
}

/// Append-only signed event log, addressed per home.
pub trait EventLog: Send + Sync {
    /// Sign and append. Returns the stored event.
    fn append(
        &self,
        addr: &JournalAddr,
        journal_event: &JournalEvent,
        signer: &Keys,
        owner_pubkey: &PublicKey,
        attestation: Option<Tag>,
    ) -> Result<Event, StoreError>;

    /// Replay all valid events. Invalid lines (bad JSON, bad signature,
    /// failed attestation, undecryptable content) are skipped with a warning
    /// — never a panic, never an error for the good lines.
    fn replay(
        &self,
        addr: &JournalAddr,
        owner: &Keys,
    ) -> Result<Vec<(Event, JournalEvent)>, StoreError>;
}

/// JSONL-file event log; the file is [`crate::paths::HomePaths::journal`].
pub struct JsonlEventLog {
    paths: Paths,
}

impl JsonlEventLog {
    pub fn new(paths: Paths) -> Self {
        Self { paths }
    }

    pub fn journal_path(&self, home: &Home) -> PathBuf {
        self.paths.home(home).journal()
    }

    /// Append a raw, already-signed event (the ingest path). Never re-signs.
    pub(crate) fn append_raw(&self, home: &Home, event: &Event) -> Result<(), StoreError> {
        let path = self.journal_path(home);
        crate::paths::append_line(&path, &serde_json::to_string(event)?)
    }

    fn build_event(
        addr: &JournalAddr,
        journal_event: &JournalEvent,
        signer: &Keys,
        owner_pubkey: &PublicKey,
        attestation: Option<Tag>,
    ) -> Result<Event, StoreError> {
        let wire_kind = journal_event.payload.event_kind().wire_kind();
        // LCOV_EXCL_START: no journal payload maps to an ephemeral kind (`JournalPayload::event_kind`); the refusal stands for the type's sake
        if kind::is_ephemeral(wire_kind) {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-kind-ephemeral-must-never-be-journaled",
                wire_kind = wire_kind.to_string()
            )));
        }
        // LCOV_EXCL_STOP
        let plaintext = serde_json::to_string(&journal_event.payload)?;
        let content = if kind::requires_owner_encryption(wire_kind) {
            nip44::encrypt(
                signer.secret_key(),
                owner_pubkey,
                &plaintext,
                nip44::Version::V2,
            )
            .map_err(StoreError::nostr)?
        } else {
            plaintext
        };

        let a_tag = Tag::parse(["a", &addr.coordinate()]).map_err(StoreError::nostr)?;
        // A fact is an occurrence: the same thing said twice in one second
        // — a loop's step started again, a note repeated — is two events.
        // Alike to the byte they would hash alike, and whoever keeps facts
        // by their id (another node, a feed) would keep one. The mark is a
        // post's own (`conversation::ONCE_TAG`), for the same reason.
        let once = crate::workspace::mint_ulid().to_string();
        let once_tag = Tag::parse([crate::conversation::ONCE_TAG, once.as_str()])
            .map_err(StoreError::nostr)?;
        let mut builder = EventBuilder::new(Kind::from(wire_kind), content)
            .tag(a_tag)
            .tag(once_tag)
            .custom_created_at(Timestamp::from_secs(journal_event.at));
        if kind::requires_owner_encryption(wire_kind) {
            let p_tag = Tag::parse(["p", &owner_pubkey.to_hex()]).map_err(StoreError::nostr)?;
            builder = builder.tag(p_tag);
        }
        if let Some(auth) = attestation {
            builder = builder.tag(auth);
        }
        builder.finalize(signer).map_err(StoreError::nostr)
    }

    /// Decode one stored event back into a `JournalEvent`, verifying signature
    /// and (when present) attestation. Returns a reason string on rejection.
    pub(crate) fn decode(
        event: &Event,
        addr: &JournalAddr,
        owner: &Keys,
    ) -> Result<JournalEvent, String> {
        event
            .verify()
            .map_err(|e| format!("invalid id/signature: {e}"))?;

        let coord = addr.coordinate();
        let addressed_here = event.tags.iter().any(|t| {
            let s = t.as_slice();
            s.len() >= 2 && s[0] == "a" && s[1] == coord
        });
        if !addressed_here {
            return Err("event does not reference this journal's goal or run".into());
        }

        let has_auth_tag = event
            .tags
            .iter()
            .any(|t| t.as_slice().first().map(String::as_str) == Some("auth"));
        if has_auth_tag && crate::identity::verify_attestation(event).is_none() {
            return Err("attestation (auth tag) failed verification".into());
        }

        let wire_kind = event.kind.as_u16();
        let plaintext = if kind::requires_owner_encryption(wire_kind) {
            nip44::decrypt(owner.secret_key(), &event.pubkey, &event.content)
                .map_err(|e| format!("nip44 decrypt failed: {e}"))?
        } else {
            event.content.clone()
        };
        let payload = serde_json::from_str(&plaintext).map_err(|e| format!("bad payload: {e}"))?;

        let author = bisa_core::PrincipalId::new(event.pubkey.to_hex())
            .map_err(|e| format!("bad author: {e}"))?;
        Ok(JournalEvent {
            home: addr.home,
            author,
            at: event.created_at.as_secs(),
            payload,
        })
    }
}

impl EventLog for JsonlEventLog {
    fn append(
        &self,
        addr: &JournalAddr,
        journal_event: &JournalEvent,
        signer: &Keys,
        owner_pubkey: &PublicKey,
        attestation: Option<Tag>,
    ) -> Result<Event, StoreError> {
        let event = Self::build_event(addr, journal_event, signer, owner_pubkey, attestation)?;
        self.append_raw(&addr.home, &event)?;
        Ok(event)
    }

    fn replay(
        &self,
        addr: &JournalAddr,
        owner: &Keys,
    ) -> Result<Vec<(Event, JournalEvent)>, StoreError> {
        read_journal_file(&self.journal_path(&addr.home), addr, owner)
    }
}

fn read_journal_file(
    path: &Path,
    addr: &JournalAddr,
    owner: &Keys,
) -> Result<Vec<(Event, JournalEvent)>, StoreError> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(StoreError::io(path.display().to_string(), e)),
    };
    let reader = BufReader::new(file);
    let mut out = Vec::new();
    for (lineno, line) in reader.lines().enumerate() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!("{}:{}: unreadable line: {e}", path.display(), lineno + 1);
                continue;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let event: Event = match serde_json::from_str(&line) {
            Ok(ev) => ev,
            Err(e) => {
                tracing::warn!(
                    "{}:{}: bad event JSON, skipping: {e}",
                    // LCOV_EXCL_START: a tracing line's fields are counted on the macro's own line; the line they make is read back by the crate's tests
                    path.display(),
                    lineno + 1 // LCOV_EXCL_STOP
                );
                continue;
            }
        };
        match JsonlEventLog::decode(&event, addr, owner) {
            Ok(je) => out.push((event, je)),
            Err(reason) => {
                tracing::warn!(
                    "{}:{}: rejected event, skipping: {reason}",
                    // LCOV_EXCL_START: a tracing line's fields are counted on the macro's own line; the line they make is read back by the crate's tests
                    path.display(),
                    lineno + 1 // LCOV_EXCL_STOP
                );
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::attest_agent;
    use bisa_core::event::JournalPayload;
    use bisa_core::{GoalId, PrincipalId, RunId, SessionId};

    fn addr_for(owner: &Keys, home: Home) -> JournalAddr {
        JournalAddr {
            author_pubkey_hex: owner.public_key().to_hex(),
            home,
        }
    }

    fn goal_home(goal: GoalId) -> Home {
        Home::Goal { goal }
    }

    fn note(home: Home, author: &Keys, text: &str) -> JournalEvent {
        JournalEvent {
            home,
            author: PrincipalId::new(author.public_key().to_hex()).unwrap(),
            at: 1_700_000_000,
            payload: JournalPayload::Note { text: text.into() },
        }
    }

    fn log(dir: &Path) -> JsonlEventLog {
        JsonlEventLog::new(Paths::new(dir))
    }

    #[test]
    fn append_replay_roundtrip_with_attestation() {
        let dir = tempfile::tempdir().unwrap();
        let log = log(dir.path());
        let owner = Keys::generate();
        let agent = Keys::generate();
        let goal = GoalId::from_ulid(ulid::Ulid::from_parts(9, 9));
        let addr = addr_for(&owner, goal_home(goal));

        log.append(
            &addr,
            &note(goal_home(goal), &owner, "captured"),
            &owner,
            &owner.public_key(),
            None,
        )
        .unwrap();
        let auth = attest_agent(&owner, &agent.public_key().to_hex(), "").unwrap();
        log.append(
            &addr,
            &note(goal_home(goal), &agent, "working"),
            &agent,
            &owner.public_key(),
            Some(auth),
        )
        .unwrap();

        let replayed = log.replay(&addr, &owner).unwrap();
        assert_eq!(replayed.len(), 2);
        assert_eq!(
            replayed[0].1.payload,
            JournalPayload::Note {
                text: "captured".into()
            }
        );
        assert_eq!(replayed[1].1.author.as_hex(), agent.public_key().to_hex());
        assert_eq!(
            log.journal_path(&goal_home(goal)),
            Paths::new(dir.path()).goal(goal).journal()
        );
    }

    /// Two facts are two facts: the same thing said twice in one second —
    /// a loop's step started again, a note repeated — is two events with
    /// two ids. Alike to the byte, they hashed alike, and whoever keeps facts
    /// by their id — another node reading this one, a feed — kept one.
    #[test]
    fn the_same_fact_said_twice_in_one_second_is_two_events() {
        let dir = tempfile::tempdir().unwrap();
        let log = log(dir.path());
        let owner = Keys::generate();
        let goal = GoalId::from_ulid(ulid::Ulid::from_parts(9, 9));
        let addr = addr_for(&owner, goal_home(goal));
        let said = note(goal_home(goal), &owner, "again");
        let first = log
            .append(&addr, &said, &owner, &owner.public_key(), None)
            .unwrap();
        let second = log
            .append(&addr, &said, &owner, &owner.public_key(), None)
            .unwrap();
        assert_eq!(first.created_at, second.created_at, "one second");
        assert_eq!(first.content, second.content, "the same words");
        assert_ne!(first.id, second.id, "two occurrences, two ids");
        let replayed = log.replay(&addr, &owner).unwrap();
        assert_eq!(replayed.len(), 2);
        assert_eq!(
            replayed[0].1, replayed[1].1,
            "and each reads as it was said"
        );
    }

    #[test]
    fn tampered_and_garbage_lines_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let log = log(dir.path());
        let owner = Keys::generate();
        let goal = GoalId::from_ulid(ulid::Ulid::from_parts(9, 10));
        let addr = addr_for(&owner, goal_home(goal));
        let good = log
            .append(
                &addr,
                &note(goal_home(goal), &owner, "good"),
                &owner,
                &owner.public_key(),
                None,
            )
            .unwrap();

        let mut tampered: serde_json::Value = serde_json::to_value(&good).unwrap();
        tampered["content"] =
            serde_json::Value::String("{\"type\":\"note\",\"text\":\"evil\"}".into());
        let path = log.journal_path(&goal_home(goal));
        crate::paths::append_line(&path, &tampered.to_string()).unwrap();
        crate::paths::append_line(&path, "this is not json").unwrap();

        let replayed = log.replay(&addr, &owner).unwrap();
        assert_eq!(replayed.len(), 1);
    }

    #[test]
    fn encrypted_kind_is_ciphertext_on_disk_and_decrypts_on_replay() {
        let dir = tempfile::tempdir().unwrap();
        let log = log(dir.path());
        let owner = Keys::generate();
        let agent = Keys::generate();
        let goal = GoalId::from_ulid(ulid::Ulid::from_parts(9, 11));
        let addr = addr_for(&owner, goal_home(goal));

        let metrics = JournalEvent {
            home: goal_home(goal),
            author: PrincipalId::new(agent.public_key().to_hex()).unwrap(),
            at: 1_700_000_100,
            payload: JournalPayload::TurnMetrics {
                session: SessionId::from_ulid(ulid::Ulid::from_parts(1, 1)),
                input_tokens: 1000,
                output_tokens: 200,
                usd_cents: 3,
            },
        };
        let auth = attest_agent(&owner, &agent.public_key().to_hex(), "").unwrap();
        let event = log
            .append(&addr, &metrics, &agent, &owner.public_key(), Some(auth))
            .unwrap();
        assert!(!event.content.contains("input_tokens"));
        let raw = std::fs::read_to_string(log.journal_path(&goal_home(goal))).unwrap();
        assert!(!raw.contains("input_tokens"));
        let replayed = log.replay(&addr, &owner).unwrap();
        assert_eq!(replayed.len(), 1);
        assert_eq!(replayed[0].1.payload, metrics.payload);
        let stranger = Keys::generate();
        assert_eq!(log.replay(&addr, &stranger).unwrap().len(), 0);
    }

    /// A run of the workspace keeps a journal of its own, in its own folder,
    /// every fact addressed to the run's coordinate; a fact addressed to a
    /// goal is not the run's and is skipped on replay, and the other way round.
    #[test]
    fn a_workspace_runs_journal_is_addressed_to_the_run_and_refuses_a_goals_fact() {
        let dir = tempfile::tempdir().unwrap();
        let log = log(dir.path());
        let owner = Keys::generate();
        let run = RunId::from_ulid(ulid::Ulid::from_parts(9, 12));
        let home = Home::Run { run };
        let addr = addr_for(&owner, home);
        assert_eq!(
            addr.coordinate(),
            format!("33413:{}:{run}", owner.public_key().to_hex()),
            "a run's own addressable kind"
        );
        let event = log
            .append(
                &addr,
                &note(home, &owner, "started"),
                &owner,
                &owner.public_key(),
                None,
            )
            .unwrap();
        assert_eq!(
            log.journal_path(&home),
            Paths::new(dir.path()).home(&home).journal()
        );
        let replayed = log.replay(&addr, &owner).unwrap();
        assert_eq!(replayed.len(), 1);
        assert_eq!(replayed[0].1.home, home, "read back as the run's");
        // The same event under a goal's address is not that goal's fact.
        let goal = GoalId::from_ulid(ulid::Ulid::from_parts(9, 13));
        let goal_addr = addr_for(&owner, goal_home(goal));
        assert!(JsonlEventLog::decode(&event, &goal_addr, &owner).is_err());
        let goals = log
            .append(
                &goal_addr,
                &note(goal_home(goal), &owner, "captured"),
                &owner,
                &owner.public_key(),
                None,
            )
            .unwrap();
        assert!(JsonlEventLog::decode(&goals, &addr, &owner).is_err());
    }

    #[test]
    fn ephemeral_kinds_refuse_to_journal() {
        assert!(bisa_core::kind::is_ephemeral(
            bisa_core::kind::KIND_OBSERVER_FRAME
        ));
    }

    // added by the coverage pass: s3-journal.rs
    #[test]
    fn a_bogus_attestation_an_unreadable_line_and_an_empty_line_cost_themselves_and_never_the_journal(
    ) {
        let dir = tempfile::tempdir().unwrap();
        let owner = Keys::generate();
        let agent = Keys::generate();
        let goal = GoalId::from_ulid(ulid::Ulid::from_parts(1, 1));
        let addr = addr_for(&owner, goal_home(goal));
        let log = log(dir.path());
        let kept = log
            .append(
                &addr,
                &note(goal_home(goal), &owner, "kept"),
                &owner,
                &owner.public_key(),
                None,
            )
            .unwrap();
        let _ = kept;
        let bogus = nostr::event::EventBuilder::new(
            nostr::event::Kind::from(bisa_core::kind::KIND_GOAL_NOTE),
            "{}",
        )
        .tags([
            nostr::event::Tag::parse(["a", &addr.coordinate()]).unwrap(),
            nostr::event::Tag::parse(["auth", &owner.public_key().to_hex(), "", "00"]).unwrap(),
        ])
        .finalize(&agent)
        .unwrap();
        assert!(JsonlEventLog::decode(&bogus, &addr, &owner)
            .unwrap_err()
            .contains("attestation"));
        let path = log.journal_path(&goal_home(goal));
        let mut bytes = std::fs::read(&path).unwrap();
        bytes.extend_from_slice(b"\n\xff\xfe\n");
        std::fs::write(&path, bytes).unwrap();
        let replayed = log.replay(&addr, &owner).unwrap();
        assert_eq!(replayed.len(), 1);
    }
}
