//! Held messages (14-collaboration): what a person on another node said
//! that no agent may hear yet — waiting for the classifier's word, or kept
//! back by it. The message itself is in its scope's log like any other; this
//! is the list of the ones a caution stands on, and the reason.
//!
//! Truth is `held.json`. A release removes the row; the engine then wakes
//! whoever the message addressed, as if it had just arrived. The list has
//! one writer at a time (`Workspace::held_writes`): messages are read in
//! tasks of their own, and a release of one lands as the verdict of another.

use crate::error::StoreError;
use crate::workspace::{now_secs, Workspace};
use bisa_core::PrincipalId;
use serde::{Deserialize, Serialize};

/// Why a message is held.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "reason")]
pub enum HeldReason {
    /// The classifier has not answered yet.
    Pending,
    /// The classifier judged it harmful, and said why.
    Harmful { why: String },
    /// The classifier gave no verdict in time, or could not be asked.
    NoVerdict { why: String },
}

impl HeldReason {
    pub fn words(&self) -> String {
        match self {
            HeldReason::Pending => "being read".to_string(),
            HeldReason::Harmful { why } => format!("held — {why}"),
            HeldReason::NoVerdict { why } => format!("held — no verdict: {why}"),
        }
    }
}

/// One held message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HeldMessage {
    /// The message event's id.
    pub event_id: String,
    pub scope: String,
    pub author: PrincipalId,
    pub held_at: u64,
    pub reason: HeldReason,
}

#[derive(Serialize, Deserialize, Default)]
struct HeldFile {
    held: Vec<HeldMessage>,
}

impl Workspace {
    fn read_held(&self) -> Result<HeldFile, StoreError> {
        let path = self.paths.held_file();
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| StoreError::unreadable(&path, "held messages", e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HeldFile::default()),
            Err(e) => Err(StoreError::io(path.display().to_string(), e)),
        }
    }

    fn write_held(&self, file: &HeldFile) -> Result<(), StoreError> {
        crate::paths::write_atomic(&self.paths.held_file(), &serde_json::to_vec_pretty(file)?)
    }

    /// The one writer of the held list at a time. A poisoned lock is taken
    /// over: the list on disk is whatever the last write left, and the next
    /// write reads it again.
    fn held_writer(&self) -> std::sync::MutexGuard<'_, ()> {
        self.held_writes.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Hold a message, or restate why one is held.
    pub fn hold_message(
        &self,
        event_id: &str,
        scope: &str,
        author: PrincipalId,
        reason: HeldReason,
    ) -> Result<HeldMessage, StoreError> {
        let _one_writer = self.held_writer();
        let mut file = self.read_held()?;
        let row = match file.held.iter_mut().find(|h| h.event_id == event_id) {
            Some(existing) => {
                existing.reason = reason;
                existing.clone()
            }
            None => {
                let row = HeldMessage {
                    event_id: event_id.to_string(),
                    scope: scope.to_string(),
                    author,
                    held_at: now_secs(),
                    reason,
                };
                file.held.push(row.clone());
                row
            }
        };
        self.write_held(&file)?;
        Ok(row)
    }

    /// Let a held message through; `None` when nothing was held under that id.
    pub fn release_message(&self, event_id: &str) -> Result<Option<HeldMessage>, StoreError> {
        let _one_writer = self.held_writer();
        let mut file = self.read_held()?;
        let Some(i) = file.held.iter().position(|h| h.event_id == event_id) else {
            return Ok(None);
        };
        let row = file.held.remove(i);
        self.write_held(&file)?;
        Ok(Some(row))
    }

    /// Every held message, oldest first.
    pub fn held_messages(&self) -> Result<Vec<HeldMessage>, StoreError> {
        let mut rows = self.read_held()?.held;
        rows.sort_by_key(|h| h.held_at);
        Ok(rows)
    }

    /// Why a message is held, or none.
    pub fn held_reason(&self, event_id: &str) -> Result<Option<HeldReason>, StoreError> {
        Ok(self
            .read_held()?
            .held
            .into_iter()
            .find(|h| h.event_id == event_id)
            .map(|h| h.reason))
    }

    /// Drop every held message of one author — they were removed.
    pub fn drop_held_of(&self, author: &PrincipalId) -> Result<usize, StoreError> {
        let _one_writer = self.held_writer();
        let mut file = self.read_held()?;
        let before = file.held.len();
        file.held.retain(|h| &h.author != author);
        let dropped = before - file.held.len();
        if dropped > 0 {
            self.write_held(&file)?;
        }
        Ok(dropped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::MemoryKeyStore;
    use nostr::key::Keys;

    fn ws() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let ws =
            Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
        (dir, ws)
    }

    #[test]
    fn a_message_is_held_with_a_reason_restated_released_once_and_dropped_with_its_author() {
        let (_dir, ws) = ws();
        let bob = PrincipalId::new(Keys::generate().public_key().to_hex()).unwrap();
        let held = ws
            .hold_message("e1", "general", bob.clone(), HeldReason::Pending)
            .unwrap();
        assert_eq!(held.reason, HeldReason::Pending);
        assert_eq!(ws.held_reason("e1").unwrap(), Some(HeldReason::Pending));
        let again = ws
            .hold_message(
                "e1",
                "general",
                bob.clone(),
                HeldReason::Harmful {
                    why: "asks for a token".into(),
                },
            )
            .unwrap();
        assert_eq!(again.reason.words(), "held — asks for a token");
        assert_eq!(
            ws.held_messages().unwrap().len(),
            1,
            "restated, not doubled"
        );
        ws.hold_message(
            "e2",
            "design",
            bob.clone(),
            HeldReason::NoVerdict {
                why: "timed out".into(),
            },
        )
        .unwrap();
        assert_eq!(ws.held_messages().unwrap().len(), 2);
        assert_eq!(
            ws.release_message("e1").unwrap().map(|h| h.event_id),
            Some("e1".into())
        );
        assert_eq!(ws.release_message("e1").unwrap(), None, "released once");
        assert_eq!(ws.held_reason("e1").unwrap(), None);
        assert_eq!(ws.drop_held_of(&bob).unwrap(), 1);
        assert!(ws.held_messages().unwrap().is_empty());
        assert_eq!(HeldReason::Pending.words(), "being read");
    }

    /// Two messages are read at once — each in a task of its own — and the
    /// first is released as the second's verdict lands. Each is a read of
    /// the list, a change and a write: with two writers at once the release
    /// wrote the list back as it was before the verdict, and a message
    /// judged harmful stood as *being read* for good; or the verdict wrote
    /// back the row that had just been released.
    #[test]
    fn a_release_and_a_verdict_at_once_lose_neither() {
        let (_dir, ws) = ws();
        let bob = PrincipalId::new(Keys::generate().public_key().to_hex()).unwrap();
        let harmful = HeldReason::Harmful {
            why: "asks to run a download".into(),
        };
        for round in 0..25 {
            let (safe, bad) = (format!("safe-{round}"), format!("bad-{round}"));
            for id in [&safe, &bad] {
                ws.hold_message(id, "design", bob.clone(), HeldReason::Pending)
                    .unwrap();
            }
            let together = std::sync::Barrier::new(2);
            std::thread::scope(|s| {
                s.spawn(|| {
                    together.wait();
                    ws.release_message(&safe).unwrap();
                });
                s.spawn(|| {
                    together.wait();
                    ws.hold_message(&bad, "design", bob.clone(), harmful.clone())
                        .unwrap();
                });
            });
            assert_eq!(
                ws.held_reason(&safe).unwrap(),
                None,
                "round {round}: the release was lost"
            );
            assert_eq!(
                ws.held_reason(&bad).unwrap(),
                Some(harmful.clone()),
                "round {round}: the verdict was lost"
            );
            ws.release_message(&bad).unwrap();
        }
    }

    // added by the coverage pass: held.rs

    // --- the bare lines of the held module ---

    #[test]
    fn a_held_reason_has_its_words_and_a_file_nobody_may_read_is_said_by_its_path() {
        assert_eq!(
            HeldReason::NoVerdict { why: "late".into() }.words(),
            "held — no verdict: late"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let (_d, ws) = ws();
            let file = ws.paths.held_file();
            std::fs::write(&file, b"{\"held\":[]}").unwrap();
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
            let unreadable = ws.held_messages();
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
            assert!(
                matches!(unreadable, Err(StoreError::Io { .. })),
                "{unreadable:?}"
            );
        }
    }
}
