//! The platform's disk: how much the data directory holds, by area, and the
//! volume it sits on.
//!
//! One walk of the data directory (symlinks never followed — a link into `/`
//! would sum the whole disk and could loop) attributes every file's size to
//! the directory, to its first-level ancestor and to its second-level one, so
//! `goals/<id>`, `projects/<slug>`, `sessions/<adapter>` and `run/terminals`
//! each read back as a number without a second walk. The index's three
//! files (`index.sqlite`, `-wal`, `-shm`) are one entry, `index`. A walk of a
//! large data dir is not a fast-poll operation, so the caller runs it on the
//! footer's slow cadence.

use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use sysinfo::Disks;

/// One directory (or first-level file) under the data dir and its size.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DirSize {
    /// Relative to the data dir, `/`-joined: `goals/01J…`, `logs`, `index`.
    pub path: String,
    pub bytes: u64,
}

/// The volume the data dir lives on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VolumeUsage {
    pub mount: String,
    pub used: u64,
    pub total: u64,
}

/// What the footer's disk read-out and overlay show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiskUsage {
    /// Every byte under the data dir.
    pub total: u64,
    /// Every directory to depth two, and every first-level file, with its bytes.
    pub dirs: Vec<DirSize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<VolumeUsage>,
    /// How long the walk took.
    pub read_ms: u32,
}

/// How deep an entry may be to earn a row of its own.
const DEPTH: usize = 2;

/// The entry a file at `rel` (relative to the root) is counted under at each
/// depth: `goals/a/state/x.json` → `goals`, `goals/a`; the index's siblings
/// fold into `index`.
fn entries_of(rel: &Path) -> Vec<String> {
    let parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    let Some(first) = parts.first() else {
        return vec![];
    };
    if parts.len() == 1 && first.starts_with("index.sqlite") {
        return vec!["index".to_string()];
    }
    let mut out = Vec::with_capacity(DEPTH);
    // A file at depth n is an entry at every depth up to min(n, DEPTH) — but a
    // file itself is only an entry at depth one (a first-level file), never
    // at depth two (`goals/x.json` is `goals`'s, not a row).
    let dirs = parts.len() - 1;
    for depth in 1..=DEPTH.min(dirs.max(1)) {
        if depth > dirs && depth > 1 {
            break;
        }
        out.push(parts[..depth].join("/"));
    }
    out
}

/// Every file under `root`, summed into the entries `entries_of` names.
fn walk(root: &Path) -> (u64, BTreeMap<String, u64>) {
    let mut total = 0_u64;
    let mut dirs: BTreeMap<String, u64> = BTreeMap::new();
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            // `symlink_metadata` describes the link itself, never its target.
            let Ok(meta) = entry
                .metadata()
                .or_else(|_| std::fs::symlink_metadata(entry.path()))
            else {
                continue;
            };
            let ft = meta.file_type();
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                stack.push(entry.path());
                continue;
            }
            total += meta.len();
            if let Ok(rel) = entry.path().strip_prefix(root) {
                for key in entries_of(rel) {
                    *dirs.entry(key).or_default() += meta.len();
                }
            }
        }
    }
    (total, dirs)
}

/// The volume whose mount point is the longest prefix of `path`.
fn volume_of(path: &Path) -> Option<VolumeUsage> {
    let disks = Disks::new_with_refreshed_list();
    disks
        .iter()
        .filter(|d| path.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| VolumeUsage {
            mount: d.mount_point().display().to_string(),
            used: d.total_space().saturating_sub(d.available_space()),
            total: d.total_space(),
        })
}

fn measure(root: &Path) -> DiskUsage {
    let started = std::time::Instant::now();
    let (total, dirs) = walk(root);
    let volume = volume_of(root);
    DiskUsage {
        total,
        dirs: dirs
            .into_iter()
            .map(|(path, bytes)| DirSize { path, bytes })
            .collect(),
        volume,
        read_ms: started.elapsed().as_millis().min(u32::MAX as u128) as u32,
    }
}

/// How much disk the platform's data directory uses, by area, and the volume
/// it lives on. `path` is the node's own data dir, learned from `GET /workspace`.
#[tauri::command]
pub async fn data_dir_usage(path: String) -> Result<DiskUsage, String> {
    if path.trim().is_empty() {
        return Err("no data directory to measure".to_string());
    }
    let root = PathBuf::from(path);
    if !root.is_dir() {
        return Err(format!("{} is not a directory", root.display()));
    }
    tauri::async_runtime::spawn_blocking(move || measure(&root))
        .await
        .map_err(|e| format!("the disk walk did not finish: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(root: &Path, rel: &str, len: usize) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, vec![0u8; len]).unwrap();
    }

    fn size(u: &DiskUsage, path: &str) -> Option<u64> {
        u.dirs.iter().find(|d| d.path == path).map(|d| d.bytes)
    }

    #[test]
    fn the_walk_sums_files_and_skips_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, "a.txt", 5);
        put(root, "sub/b.bin", 100);
        let (total, _) = walk(root);
        assert_eq!(total, 105, "every file under the tree, once");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("a.txt"), root.join("link")).unwrap();
            assert_eq!(walk(root).0, 105, "a symlink is skipped, not summed");
        }
    }

    #[test]
    fn an_empty_tree_is_zero() {
        let dir = tempfile::tempdir().unwrap();
        let u = measure(dir.path());
        assert_eq!(u.total, 0);
        assert!(u.dirs.is_empty());
    }

    #[test]
    fn every_area_reads_back_at_depth_one_and_two_and_the_index_is_one_entry() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, "goals/a/state/x.json", 10);
        put(root, "goals/a/journal.jsonl", 5);
        put(root, "goals/b/journal.jsonl", 1);
        put(root, "projects/p/workstreams/w/y.rs", 100);
        put(root, "projects/p/project.json", 2);
        put(root, "notes/goals/a/n.md", 7);
        put(root, "logs/z.jsonl", 20);
        put(root, "index.sqlite", 1000);
        put(root, "index.sqlite-wal", 300);
        put(root, "index.sqlite-shm", 30);
        put(root, "members.json", 3);
        let u = measure(root);
        assert_eq!(u.total, 1478);
        assert_eq!(size(&u, "goals"), Some(16));
        assert_eq!(
            size(&u, "goals/a"),
            Some(15),
            "a depth-three file counts in its depth-two ancestor"
        );
        assert_eq!(size(&u, "goals/b"), Some(1));
        assert_eq!(size(&u, "projects"), Some(102));
        assert_eq!(size(&u, "projects/p"), Some(102));
        assert_eq!(size(&u, "notes"), Some(7));
        assert_eq!(size(&u, "notes/goals"), Some(7));
        assert_eq!(size(&u, "logs"), Some(20));
        assert_eq!(
            size(&u, "index"),
            Some(1330),
            "the index's three files are one entry"
        );
        assert_eq!(
            size(&u, "members.json"),
            Some(3),
            "a first-level file is an entry of its own"
        );
        assert_eq!(
            size(&u, "goals/a/state"),
            None,
            "nothing deeper than two earns a row"
        );
        assert!(u.volume.is_some(), "a temp dir lives on some volume");
    }

    #[test]
    fn entries_stop_at_depth_two_and_a_first_level_file_is_its_own() {
        assert_eq!(
            entries_of(Path::new("goals/a/state/x.json")),
            vec!["goals", "goals/a"]
        );
        assert_eq!(entries_of(Path::new("goals/x.json")), vec!["goals"]);
        assert_eq!(entries_of(Path::new("members.json")), vec!["members.json"]);
        assert_eq!(entries_of(Path::new("index.sqlite-wal")), vec!["index"]);
        assert!(entries_of(Path::new("")).is_empty());
    }
}
