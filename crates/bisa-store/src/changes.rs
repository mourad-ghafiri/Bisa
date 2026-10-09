//! What agents changed in a checkout, per conversation: the ledger
//! ([`bisa_core::ChangeLedger`]) and the blobs it names.
//!
//! Local state, no kind. Truth: `ide/changes/<conversation>/ledger.json`,
//! beside `blobs/<sha256>` — a file as it was before a turn touched it, or
//! as the turn left it. A checkout is this machine's, so none of it syncs,
//! and a conversation's deletion takes the directory with it.
//!
//! A blob is bounded by [`MAX_CHANGE_BLOB`] — a bound of its own, equal to
//! the size above which the editor refuses a file as nobody set it, and not
//! moved when a machine moves that one. A larger file is not tracked at all
//! rather than tracked without the bytes an Undo would need.

use crate::error::StoreError;
use crate::workspace::Workspace;
use bisa_core::{ChangeLedger, ConversationId, Sha256};
use std::collections::HashSet;

/// The largest file a review keeps the bytes of: 20 MiB — the editor's
/// refusal bound where nobody moved it (ide/03, `editor.large_file.refuse_mib`),
/// and fixed here whatever that setting says.
pub const MAX_CHANGE_BLOB: u64 = 20 * 1024 * 1024;

/// Where a ledger that no longer parses is kept, beside the live one.
pub const UNREADABLE_LEDGER: &str = "ledger.unreadable.json";

fn not_found(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::NotFound
}

impl Workspace {
    /// The conversation's ledger; an empty one when nothing was ever
    /// recorded.
    pub fn change_ledger(&self, conversation: ConversationId) -> Result<ChangeLedger, StoreError> {
        let path = self.paths.change_ledger(conversation);
        match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(ledger) => Ok(ledger),
                Err(e) => Ok(self.set_aside_unreadable_ledger(conversation, &path, &e)),
            },
            Err(e) if not_found(&e) => Ok(ChangeLedger::new(conversation)),
            Err(e) => Err(StoreError::io(path.display().to_string(), e)),
        }
    }

    /// A ledger that no longer parses would refuse every read of the
    /// conversation's changes — the pane, the next turn, a Keep. It is moved
    /// aside under [`UNREADABLE_LEDGER`], never deleted, and the conversation
    /// goes on from an empty one: what waited for a word is no longer under
    /// review, and the files in the checkout are as they were.
    fn set_aside_unreadable_ledger(
        &self,
        conversation: ConversationId,
        path: &std::path::Path,
        cause: &serde_json::Error,
    ) -> ChangeLedger {
        let aside = path.with_file_name(UNREADABLE_LEDGER);
        match std::fs::rename(path, &aside) {
            Ok(()) => tracing::error!(
                target: "bisa_store::changes",
                %conversation,
                kept_at = %aside.display(), // LCOV_EXCL_LINE: a field line of the macro; the macro's own line counts
                "the ledger of a conversation's changes does not parse and was set aside: {cause}"
            ),
            Err(e) => tracing::error!(
                target: "bisa_store::changes",
                %conversation,
                "the ledger of a conversation's changes does not parse ({cause}) and could not be set aside: {e}"
            ),
        }
        ChangeLedger::new(conversation)
    }

    /// Write the ledger, then drop the blobs it no longer names.
    pub fn write_change_ledger(&self, ledger: &ChangeLedger) -> Result<(), StoreError> {
        let path = self.paths.change_ledger(ledger.conversation);
        crate::paths::write_atomic(&path, &serde_json::to_vec_pretty(ledger)?)?;
        self.collect_change_blobs(ledger)
    }

    /// Keep `bytes` as a blob of the conversation's changes and answer its
    /// address. Writing the same bytes twice is one file.
    pub fn put_change_blob(
        &self,
        conversation: ConversationId,
        bytes: &[u8],
    ) -> Result<Sha256, StoreError> {
        if bytes.len() as u64 > MAX_CHANGE_BLOB {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-file-bytes-too-large-keep-review",
                a0 = (bytes.len()).to_string()
            )));
        }
        let sha = Sha256::new(crate::recall::sha256_hex(bytes))?;
        let path = self.paths.change_blob(conversation, &sha);
        if !path.exists() {
            crate::paths::write_atomic(&path, bytes)?;
        }
        Ok(sha)
    }

    pub fn change_blob(
        &self,
        conversation: ConversationId,
        sha: &Sha256,
    ) -> Result<Vec<u8>, StoreError> {
        let path = self.paths.change_blob(conversation, sha);
        std::fs::read(&path).map_err(|e| StoreError::io(path.display().to_string(), e))
    }

    fn collect_change_blobs(&self, ledger: &ChangeLedger) -> Result<(), StoreError> {
        // A ledger set aside still names its blobs: they are what makes it
        // worth keeping, so nothing is collected while it lies there.
        if self
            .paths
            .change_ledger(ledger.conversation)
            .with_file_name(UNREADABLE_LEDGER)
            .exists()
        {
            return Ok(());
        }
        let named: HashSet<&str> = ledger.blobs().into_iter().map(Sha256::as_str).collect();
        let dir = self.paths.change_blobs_dir(ledger.conversation);
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if not_found(&e) => return Ok(()),
            Err(e) => return Err(StoreError::io(dir.display().to_string(), e)),
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let held = name.to_str().is_some_and(|n| named.contains(n));
            // Only a name that is a digest is ours to collect.
            if held || !name.to_str().is_some_and(Sha256::is_valid) {
                continue;
            }
            let path = entry.path();
            if let Err(e) = std::fs::remove_file(&path) {
                if !not_found(&e) {
                    return Err(StoreError::io(path.display().to_string(), e));
                } // LCOV_EXCL_LINE: listed a moment ago; gone only under a concurrent collection
            }
        }
        Ok(())
    }

    /// Forget everything recorded about a conversation's changes — what its
    /// deletion calls. The files in the checkout are not touched.
    pub(crate) fn remove_changes_of(&self, conversation: ConversationId) -> Result<(), StoreError> {
        let dir = self.paths.changes_dir(conversation);
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => Ok(()),
            Err(e) if not_found(&e) => Ok(()),
            Err(e) => Err(StoreError::io(dir.display().to_string(), e)),
        }
    }

    /// The private index file a snapshot of the checkout is staged through.
    pub fn change_index_file(&self, conversation: ConversationId) -> std::path::PathBuf {
        self.paths.change_index(conversation)
    }
}

// added by the coverage pass: changes_mod.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::MemoryKeyStore;
    use bisa_core::changes::ChangeLedger;

    fn ws() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let ws =
            Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
        (dir, ws)
    }

    /// A ledger that will not parse is set aside and the conversation goes
    /// on from an empty one; when it cannot even be set aside, the same,
    /// said; a ledger, a blob folder or a stray blob nobody may read or
    /// remove is an I/O error by its path.
    #[cfg(unix)]
    #[test]
    fn a_ledger_that_cannot_be_read_or_set_aside_is_said_by_its_path() {
        use std::os::unix::fs::PermissionsExt;
        let (_d, ws) = ws();
        let conversation = ConversationId::from_ulid(crate::workspace::mint_ulid());
        let ledger = ws.paths.change_ledger(conversation);
        let folder = ws.paths.changes_dir(conversation);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(&ledger, b"{torn").unwrap();
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o500)).unwrap();
        let read = ws.change_ledger(conversation);
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(
            read.unwrap().turns.is_empty(),
            "an empty ledger, the torn one in place"
        );
        assert!(ledger.exists(), "it could not be set aside");
        std::fs::set_permissions(&ledger, std::fs::Permissions::from_mode(0o000)).unwrap();
        let unreadable = ws.change_ledger(conversation);
        std::fs::set_permissions(&ledger, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(
            matches!(unreadable, Err(StoreError::Io { .. })),
            "{unreadable:?}"
        );
        // Now set aside for real: an empty ledger, the torn one kept beside.
        assert!(ws.change_ledger(conversation).unwrap().turns.is_empty());
        assert!(ledger.with_file_name(UNREADABLE_LEDGER).exists());
        std::fs::remove_file(ledger.with_file_name(UNREADABLE_LEDGER)).unwrap();
        // The blobs: a folder nobody may read, then a stray nobody may remove.
        let blobs = ws.paths.change_blobs_dir(conversation);
        std::fs::create_dir_all(&blobs).unwrap();
        std::fs::write(blobs.join("a".repeat(64)), b"stray").unwrap();
        std::fs::set_permissions(&blobs, std::fs::Permissions::from_mode(0o000)).unwrap();
        let unreadable = ws.write_change_ledger(&ChangeLedger::new(conversation));
        std::fs::set_permissions(&blobs, std::fs::Permissions::from_mode(0o500)).unwrap();
        let unremovable = ws.write_change_ledger(&ChangeLedger::new(conversation));
        std::fs::set_permissions(&blobs, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(
            matches!(unreadable, Err(StoreError::Io { .. })),
            "{unreadable:?}"
        );
        assert!(
            matches!(unremovable, Err(StoreError::Io { .. })),
            "{unremovable:?}"
        );
        ws.write_change_ledger(&ChangeLedger::new(conversation))
            .unwrap();
        assert!(
            !blobs.join("a".repeat(64)).exists(),
            "the stray is collected"
        );
        // The whole folder, when its parent refuses the removal.
        let parent = folder.parent().unwrap().to_path_buf();
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o500)).unwrap();
        let unremovable = ws.remove_changes_of(conversation);
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(
            matches!(unremovable, Err(StoreError::Io { .. })),
            "{unremovable:?}"
        );
        ws.remove_changes_of(conversation).unwrap();
        assert!(!folder.exists());
    }
}
