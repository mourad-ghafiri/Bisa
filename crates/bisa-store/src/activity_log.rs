//! The workspace's **activity log**: the truth file for the engine's own
//! facts — a workstream opened, a setting changed, the node paused — which
//! have no journal and no channel to live in. One JSON line per fact under
//! `activity/<YYYY-MM>.jsonl`, append-only, local: it carries no GEP kind,
//! by the test [09](../../docs/architecture/09-protocol-gep.md) applies to
//! workstreams — what happened on this machine is not a fact a second node
//! could act on, and a peer's node builds its own from what syncs.
//!
//! The index's `activity` table is derived from it (and from the journals
//! and the conversations, whose facts already have truth files), so a
//! rebuild loses nothing.

use crate::error::StoreError;
use bisa_core::ActivityFact;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The month's file a fact at `at` (unix seconds) is appended to.
pub fn file_for(dir: &Path, at: u64) -> PathBuf {
    let days = at / 86_400;
    // Civil-from-days (Howard Hinnant's algorithm), enough for a file name.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    dir.join(format!("{year:04}-{m:02}.jsonl"))
}

/// Append one fact to the month's file, creating the directory on first use.
pub fn append(dir: &Path, fact: &ActivityFact) -> Result<(), StoreError> {
    std::fs::create_dir_all(dir).map_err(|e| StoreError::io(dir.display().to_string(), e))?;
    let path = file_for(dir, fact.at);
    let mut line = serde_json::to_string(fact)?;
    line.push('\n');
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| StoreError::io(path.display().to_string(), e))?;
    file.write_all(line.as_bytes())
        .map_err(|e| StoreError::io(path.display().to_string(), e))?;
    Ok(())
}

/// Every fact in the log, oldest first: the month files in name order, each
/// line in file order. A line that does not parse is skipped with a warning
/// rather than failing the rebuild — the rest of the log is still the truth.
pub fn read_all(dir: &Path) -> Result<Vec<ActivityFact>, StoreError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(StoreError::io(dir.display().to_string(), e)),
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .collect();
    files.sort();
    let mut out = Vec::new();
    for path in files {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| StoreError::io(path.display().to_string(), e))?;
        for line in content.lines().filter(|l| !l.trim().is_empty()) {
            match serde_json::from_str::<ActivityFact>(line) {
                Ok(fact) => out.push(fact),
                Err(e) => tracing::warn!("{}: bad activity line, skipping: {e}", path.display()),
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_core::{ActivityConcept, ActivitySource};

    fn fact(at: u64, kind: &str) -> ActivityFact {
        ActivityFact {
            at,
            concept: ActivityConcept::Node,
            kind: kind.into(),
            source: ActivitySource::node(),
            author: None,
            event: serde_json::json!({ "type": kind }),
        }
    }

    #[test]
    fn a_fact_lands_in_its_month_and_reads_back_in_order() {
        let dir = tempfile::tempdir().unwrap();
        // 2024-01-31T23:59:59Z and 2024-02-01T00:00:00Z: two months, one second apart.
        append(dir.path(), &fact(1_706_745_599, "paused")).unwrap();
        append(dir.path(), &fact(1_706_745_600, "resumed")).unwrap();
        assert!(file_for(dir.path(), 1_706_745_599).ends_with("2024-01.jsonl"));
        assert!(file_for(dir.path(), 1_706_745_600).ends_with("2024-02.jsonl"));
        assert!(file_for(dir.path(), 0).ends_with("1970-01.jsonl"));
        let all = read_all(dir.path()).unwrap();
        assert_eq!(
            all.iter().map(|f| f.kind.as_str()).collect::<Vec<_>>(),
            ["paused", "resumed"]
        );
        assert_eq!(
            all[0].event,
            serde_json::json!({ "type": "paused" }),
            "the payload comes back verbatim"
        );
        assert!(
            read_all(&dir.path().join("nothing")).unwrap().is_empty(),
            "no log yet is no facts, not an error"
        );
    }

    #[test]
    fn a_bad_line_is_skipped_and_the_rest_still_read() {
        let dir = tempfile::tempdir().unwrap();
        append(dir.path(), &fact(10, "paused")).unwrap();
        let path = file_for(dir.path(), 10);
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        f.write_all(b"not json\n").unwrap();
        append(dir.path(), &fact(11, "resumed")).unwrap();
        assert_eq!(read_all(dir.path()).unwrap().len(), 2);
    }
}
