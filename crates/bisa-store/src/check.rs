//! `bisa workspace check`: every file of the workspace read for what the
//! next open would do with it — without opening it, without the engine
//! lock, without the index.
//!
//! The open's rule (`problems.rs`) is that one torn file never stops the
//! node: the owner key alone does, and every other file this build cannot
//! read is quarantined or skipped and named. This is the same reading made
//! ahead of time, as a list a person can act on — after a crash, before a
//! restart, or when the node says the workspace has problems: the owner key
//! parses; `members.json`, `governance.json` and every settings layer
//! parse; every snapshot under a `state/` folder is a signed event; every
//! line log ends in a newline (a tail a crash tore is mended by the next
//! write and skipped by every reader, but it is said); and whatever earlier
//! opens moved under `quarantine/` is listed, since it is the person's to
//! restore by hand or remove. Nothing here writes.

use crate::error::StoreError;
use crate::identity::KeyStore;
use crate::paths::Paths;
use bisa_core::Text;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// What kind of trouble a finding names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "WorkspaceFindingKind")]
pub enum FindingKind {
    /// The one file whose trouble stops an open.
    OwnerKeyUnreadable,
    /// A file this build cannot read; the next open quarantines or skips it.
    Unparseable,
    /// A line log whose last line a crash tore; the next write mends it.
    TornTail,
    /// A file an earlier open moved under `quarantine/`.
    Quarantined,
}

/// One thing the check found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "WorkspaceFinding")]
pub struct Finding {
    pub kind: FindingKind,
    pub path: String,
    /// The sentence a person reads.
    pub text: Text,
}

impl Finding {
    fn new(kind: FindingKind, path: &Path, text: Text) -> Self {
        Finding {
            kind,
            path: path.display().to_string(),
            text,
        }
    }
}

/// Every file of the workspace under `paths`, read as the next open would
/// read it; the findings in path order, none for a sound workspace. A
/// workspace that is not there yet has nothing to check. Only a folder that
/// cannot be listed at all is an error.
pub fn check_files(paths: &Paths) -> Result<Vec<Finding>, StoreError> {
    let mut findings = Vec::new();
    if !paths.root().is_dir() {
        return Ok(findings);
    }
    owner_key(paths, &mut findings);
    parsed_as::<crate::members::MemberFile>(&paths.members_file(), "member file", &mut findings);
    parsed_as::<crate::governance::Governance>(
        &paths.governance_file(),
        "governance document",
        &mut findings,
    );
    for layer in settings_layers(paths)? {
        parsed_as::<bisa_core::settings::Layer>(&layer, "settings file", &mut findings);
    }
    let snapshots = crate::snapshots::SnapshotStore::new(paths.clone());
    let quarantine = paths.quarantine_dir();
    let logs = paths.logs_dir();
    walk(paths.root(), &mut |file| {
        if file.starts_with(&quarantine) || file.starts_with(&logs) {
            return;
        }
        if is_snapshot(file) {
            if let Err(StoreError::Unreadable { reason, .. }) = snapshots.load_event(file) {
                findings.push(Finding::new(
                    FindingKind::Unparseable,
                    file,
                    bisa_core::text!(
                        "error-store-check-unparseable",
                        path = file.display().to_string(),
                        what = "snapshot event".to_string(),
                        reason = reason
                    ),
                ));
            }
        } else if file.extension().is_some_and(|e| e == "jsonl") && torn_tail(file) {
            findings.push(Finding::new(
                FindingKind::TornTail,
                file,
                bisa_core::text!(
                    "error-store-check-torn-tail",
                    path = file.display().to_string()
                ),
            ));
        }
    })?;
    if quarantine.is_dir() {
        walk(&quarantine, &mut |file| {
            findings.push(Finding::new(
                FindingKind::Quarantined,
                file,
                bisa_core::text!(
                    "error-store-check-quarantined",
                    path = file.display().to_string()
                ),
            ));
        })?;
    }
    findings.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(findings)
}

/// The owner key, when it is a file: the one trouble that stops an open.
/// A key in the machine's keyring is not a file and is not read here.
fn owner_key(paths: &Paths, findings: &mut Vec<Finding>) {
    let store = crate::identity::FileKeyStore::new(paths.identity_dir());
    let Some(path) = store
        .path_of(crate::identity::OWNER_KEY_NAME)
        .filter(|p| p.is_file())
    else {
        return;
    };
    // The file's text as the store reads it, parsed as the key the open
    // would parse: a store that cannot read the file, or text that is no key,
    // is the one finding that stops an open.
    let unreadable = match store.get(crate::identity::OWNER_KEY_NAME) {
        Ok(Some(hex)) => nostr::key::Keys::parse(hex.trim())
            .err()
            .map(|e| e.to_string()),
        Ok(None) => None,
        Err(e) => Some(e.to_string()),
    };
    if let Some(e) = unreadable {
        findings.push(Finding::new(
            FindingKind::OwnerKeyUnreadable,
            &path,
            bisa_core::text!(
                "error-store-check-owner-key-unreadable",
                path = path.display().to_string(),
                reason = e
            ),
        ));
    }
}

/// A file that must parse as `T` when it is there; absent is nothing.
fn parsed_as<T: serde::de::DeserializeOwned>(path: &Path, what: &str, findings: &mut Vec<Finding>) {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return,
    };
    if let Err(e) = serde_json::from_slice::<T>(&bytes) {
        findings.push(Finding::new(
            FindingKind::Unparseable,
            path,
            bisa_core::text!(
                "error-store-check-unparseable",
                path = path.display().to_string(),
                what = what.to_string(),
                reason = e.to_string()
            ),
        ));
    }
}

/// The three scopes' files: the machine's, the workspace's, and each
/// project folder's — by the folders on disk, since the index is not read.
fn settings_layers(paths: &Paths) -> Result<Vec<PathBuf>, StoreError> {
    let mut out = vec![paths.machine_settings(), paths.workspace_settings()];
    out.extend(
        paths
            .project_dirs()?
            .iter()
            .map(|project| project.settings())
            .filter(|file| file.is_file()),
    );
    Ok(out)
}

/// A snapshot: a `.json` file in a folder named `state`.
fn is_snapshot(file: &Path) -> bool {
    file.extension().is_some_and(|e| e == "json")
        && file
            .parent()
            .and_then(|p| p.file_name())
            .is_some_and(|n| n == "state")
}

/// Whether a line log's last byte is not a newline. A file that cannot be
/// opened is not torn — it is somebody else's trouble.
fn torn_tail(file: &Path) -> bool {
    std::fs::File::open(file)
        .ok()
        .and_then(|mut f| crate::paths::tail_lacks_newline(&mut f).ok())
        .unwrap_or(false)
}

/// Every file under `dir`, depth first, in name order. A folder that cannot
/// be listed is the error; a file that vanishes between the listing and the
/// read is skipped by the caller's own read.
fn walk(dir: &Path, visit: &mut dyn FnMut(&Path)) -> Result<(), StoreError> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| StoreError::io(dir.display().to_string(), e))?
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            walk(&path, visit)?;
        } else if path.is_file() {
            visit(&path);
        }
    }
    Ok(())
}
