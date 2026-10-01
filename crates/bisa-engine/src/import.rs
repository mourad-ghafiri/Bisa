//! Importing a folder: the tree becomes the goal's own.
//!
//! Adopting points at somebody's directory and writes nothing into it.
//! Importing is the other half of the same question — *"these files are the
//! work, and I want them in here"* — so it **copies** the tree into a
//! `Managed` root and leaves the original exactly as it was. There is no move
//! and no delete anywhere in this module: the source folder is not ours, and a
//! failed import must never be able to cost somebody the only copy.
//!
//! Four rules, each of which is the reason a line here exists.
//!
//! **`.git` comes across.** [`bisa_iso::copy`] deliberately skips it —
//! that copy is a throwaway isolation tree whose diff runs in plain mode, so
//! replicating a repository would only slow it down. An import is the
//! opposite: a repository without its history is not the repository, so this
//! needs its own walk rather than reusing that one. `.git` is an ordinary
//! directory here, and the copy is a repository that `git log` reads.
//!
//! **Symlinks are skipped**, following the same v1 precedent the isolation
//! copy set — and for a second reason that is specific to import. A link
//! pointing out of the source (`node_modules/x -> /Users/you/.ssh`) would be
//! *followed* by a copy that resolved it, turning "import my project folder"
//! into a copy of whatever the link happened to target, now sitting inside the
//! workspace where an agent reads it. Skipping is the only behaviour that
//! cannot exfiltrate. Every skip is counted and reported, so a tree that
//! depended on its links says so rather than silently arriving broken.
//!
//! **The size bound is checked before a single byte is written.** A person can
//! point this at a home directory. Measuring first costs one `stat` per entry
//! and stops at the first entry over the bound, so the refusal is cheap and,
//! more importantly, arrives while the destination is still empty — the
//! alternative is refusing after 20 GB has already been copied.
//!
//! **A failed copy leaves what it wrote.** Nothing here removes a directory.
//! `clone` sets the precedent: it deletes the *record* and leaves the tree,
//! because a recursive delete triggered by an error path is how a bug becomes
//! somebody's lost work. The partial copy stays, and the "destination already
//! exists" refusal below is what makes the retry loud instead of silently
//! merging into it.

use crate::EngineError;
use std::fs;
use std::path::{Path, PathBuf};

/// How much of somebody's disk an import will move without asking again.
///
/// Not a technical limit — the copy would work — but a refusal that arrives
/// before an accidental `~/Downloads` becomes a project. The escape hatch is
/// in the message: linking a folder in place has no bound at all, because it
/// copies nothing.
pub const MAX_IMPORT_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// The other half of the bound: a tree can be small and still be a million
/// files, and it is the entry count that makes the copy take an afternoon.
pub const MAX_IMPORT_ENTRIES: u64 = 200_000;

/// The bound, as a value, so a test can prove the refusal without building a
/// 2 GiB fixture.
#[derive(Clone, Copy, Debug)]
pub struct ImportLimits {
    pub max_bytes: u64,
    pub max_entries: u64,
}

impl Default for ImportLimits {
    fn default() -> Self {
        Self {
            max_bytes: MAX_IMPORT_BYTES,
            max_entries: MAX_IMPORT_ENTRIES,
        }
    }
}

/// What crossed, and what did not.
///
/// `skipped_symlinks` and `skipped_special` are reported rather than logged
/// because they are the one way an import is not a faithful copy, and a person
/// who used symlinks deserves to be told at the moment it happens rather than
/// when a build fails a week later.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct ImportStats {
    /// Regular files copied.
    pub files: u64,
    /// Directories created, the destination root included.
    pub dirs: u64,
    /// Bytes of file content copied.
    pub bytes: u64,
    /// Symlinks left behind. See the module note: following one is how an
    /// import turns into a copy of something outside the folder.
    pub skipped_symlinks: u64,
    /// Sockets, FIFOs and device nodes — nothing a copy could reproduce
    /// meaningfully.
    pub skipped_special: u64,
}

/// Copy `source` into `dest`, or refuse and write nothing.
///
/// `dest` must not exist. Merging an import into a directory that is already
/// there would mean the result is neither the source nor what was in the
/// destination, and no caller could say which files came from where.
pub fn import_tree(
    source: &Path,
    dest: &Path,
    limits: ImportLimits,
) -> Result<ImportStats, EngineError> {
    if dest.exists() {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-already-exists-refusing-import-into-rather-than",
            a0 = (dest.display()).to_string()
        )));
    }
    // Before anything is written. See the module note.
    measure(source, limits)?;
    copy_tree(source, dest)
}

/// Walk the source counting entries and bytes, stopping at the first one over
/// the bound. Reads no file content.
fn measure(source: &Path, limits: ImportLimits) -> Result<(), EngineError> {
    let mut entries = 0u64;
    let mut bytes = 0u64;
    // An explicit stack rather than recursion: directory depth is attacker-
    // and accident-controlled, and a stack overflow is not a refusal a caller
    // can render.
    let mut stack = vec![source.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in read_dir(&dir)? {
            let entry = entry.map_err(|e| io(&dir, e))?;
            entries += 1;
            if entries > limits.max_entries {
                return Err(EngineError::Invalid(bisa_core::text!(
                    "error-engine-invalid-refusing-import-holds-more-than-files-folders",
                    a0 = (source.display()).to_string(),
                    a1 = (limits.max_entries).to_string()
                )));
            }
            // `file_type` does not follow the link; `metadata` would, and a
            // symlinked directory is how a walk becomes an infinite one.
            let kind = entry.file_type().map_err(|e| io(&entry.path(), e))?;
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                stack.push(entry.path());
            } else if kind.is_file() {
                bytes += entry.metadata().map_err(|e| io(&entry.path(), e))?.len();
                if bytes > limits.max_bytes {
                    return Err(EngineError::Invalid(bisa_core::text!(
                        "error-engine-invalid-refusing-import-holds-more-than-import-limit",
                        a0 = (source.display()).to_string(),
                        a1 = (human_bytes(limits.max_bytes)).to_string()
                    )));
                }
            }
        }
    }
    Ok(())
}

/// The copy itself. Reads the source, writes the destination, and does
/// nothing else — in particular it never removes, moves or modifies anything
/// under `from`.
fn copy_tree(from: &Path, to: &Path) -> Result<ImportStats, EngineError> {
    let mut stats = ImportStats::default();
    let mut stack: Vec<(PathBuf, PathBuf)> = vec![(from.to_path_buf(), to.to_path_buf())];
    while let Some((src, dst)) = stack.pop() {
        fs::create_dir_all(&dst).map_err(|e| io(&dst, e))?;
        stats.dirs += 1;
        for entry in read_dir(&src)? {
            let entry = entry.map_err(|e| io(&src, e))?;
            let path = entry.path();
            let target = dst.join(entry.file_name());
            let kind = entry.file_type().map_err(|e| io(&path, e))?;
            if kind.is_symlink() {
                stats.skipped_symlinks += 1;
            } else if kind.is_dir() {
                stack.push((path, target));
            } else if kind.is_file() {
                // `fs::copy` carries the permission bits across, so a script
                // that was executable in the source is executable here. An
                // import that silently dropped `+x` would look identical and
                // fail at the first run.
                let n = fs::copy(&path, &target).map_err(|e| io(&path, e))?;
                stats.files += 1;
                stats.bytes += n;
            } else {
                stats.skipped_special += 1;
            }
        }
    }
    Ok(stats)
}

fn read_dir(dir: &Path) -> Result<fs::ReadDir, EngineError> {
    fs::read_dir(dir).map_err(|e| io(dir, e))
}

/// An I/O failure that names the file it happened to. `std::io::Error` does
/// not carry a path, and "permission denied" with no path is a bug report
/// nobody can act on.
fn io(path: &Path, e: std::io::Error) -> EngineError {
    EngineError::Io(std::io::Error::new(
        e.kind(),
        format!("{}: {e}", path.display()),
    ))
}

/// Bytes as a person reads them, for the refusal message. Exact multiples
/// keep their whole number, so the bound reads as `2 GiB` rather than
/// `2.0 GiB`.
fn human_bytes(n: u64) -> String {
    for (unit, size) in [("GiB", 1u64 << 30), ("MiB", 1 << 20), ("KiB", 1 << 10)] {
        if n >= size {
            return if n % size == 0 {
                format!("{} {unit}", n / size)
            } else {
                format!("{:.1} {unit}", n as f64 / size as f64)
            };
        }
    }
    format!("{n} bytes")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small tree with a `.git` directory, a nested folder and a file that
    /// is executable — the three things an import has to get right.
    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join(".git/objects")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join(".git/HEAD"), b"ref: refs/heads/main\n").unwrap();
        fs::write(root.join(".git/objects/thing"), b"binary-ish").unwrap();
        fs::write(root.join("src/main.rs"), b"fn main() {}\n").unwrap();
        fs::write(root.join("README.md"), b"# hello\n").unwrap();
        dir
    }

    #[test]
    fn an_import_copies_dot_git_because_history_is_the_repository() {
        let src = fixture();
        let dest = tempfile::tempdir().unwrap();
        let dest = dest.path().join("copy");
        let stats = import_tree(src.path(), &dest, ImportLimits::default()).unwrap();

        assert!(dest.join(".git/HEAD").is_file(), "history must come across");
        assert_eq!(
            fs::read(dest.join(".git/HEAD")).unwrap(),
            b"ref: refs/heads/main\n"
        );
        assert_eq!(
            fs::read(dest.join("src/main.rs")).unwrap(),
            b"fn main() {}\n"
        );
        assert_eq!(stats.files, 4);
        assert_eq!(stats.skipped_symlinks, 0);
    }

    #[test]
    #[cfg(unix)]
    fn an_import_skips_symlinks_rather_than_following_them_out_of_the_folder() {
        let src = fixture();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("private"), b"not yours").unwrap();
        std::os::unix::fs::symlink(outside.path().join("private"), src.path().join("escape"))
            .unwrap();
        std::os::unix::fs::symlink(outside.path(), src.path().join("escape-dir")).unwrap();

        let dest = tempfile::tempdir().unwrap();
        let dest = dest.path().join("copy");
        let stats = import_tree(src.path(), &dest, ImportLimits::default()).unwrap();

        assert_eq!(stats.skipped_symlinks, 2, "both links are reported");
        assert!(!dest.join("escape").exists());
        assert!(!dest.join("escape-dir").exists());
        // The thing the skip exists to prevent.
        assert!(!dest.join("escape-dir/private").exists());
    }

    #[test]
    #[cfg(unix)]
    fn an_executable_file_is_still_executable_in_the_copy() {
        use std::os::unix::fs::PermissionsExt as _;
        let src = fixture();
        let script = src.path().join("build.sh");
        fs::write(&script, b"#!/bin/sh\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

        let dest = tempfile::tempdir().unwrap();
        let dest = dest.path().join("copy");
        import_tree(src.path(), &dest, ImportLimits::default()).unwrap();

        let mode = fs::metadata(dest.join("build.sh"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111, "the executable bit survived the copy");
    }

    #[test]
    fn an_existing_destination_is_refused_rather_than_merged() {
        let src = fixture();
        let dest = tempfile::tempdir().unwrap();
        let dest = dest.path().join("copy");
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("theirs.txt"), b"already here").unwrap();

        let err = import_tree(src.path(), &dest, ImportLimits::default()).unwrap_err();
        assert!(err.to_string().contains("already exists"), "{err}");
        // Nothing was merged in.
        assert!(!dest.join("README.md").exists());
        assert_eq!(fs::read(dest.join("theirs.txt")).unwrap(), b"already here");
    }

    #[test]
    fn a_tree_over_the_byte_bound_is_refused_before_anything_is_written() {
        let src = fixture();
        let dest = tempfile::tempdir().unwrap();
        let dest = dest.path().join("copy");
        let limits = ImportLimits {
            max_bytes: 4,
            max_entries: MAX_IMPORT_ENTRIES,
        };
        let err = import_tree(src.path(), &dest, limits)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("4 bytes"),
            "the refusal names the bound: {err}"
        );
        assert!(err.contains("Link it in place"), "{err}");
        assert!(!dest.exists(), "the destination was never created");
    }

    #[test]
    fn a_tree_over_the_entry_bound_is_refused_before_anything_is_written() {
        let src = fixture();
        let dest = tempfile::tempdir().unwrap();
        let dest = dest.path().join("copy");
        let limits = ImportLimits {
            max_bytes: MAX_IMPORT_BYTES,
            max_entries: 2,
        };
        let err = import_tree(src.path(), &dest, limits)
            .unwrap_err()
            .to_string();
        assert!(err.contains("2 files and folders"), "{err}");
        assert!(!dest.exists());
    }

    #[test]
    fn the_shipped_bound_reads_as_a_person_would_say_it() {
        assert_eq!(human_bytes(MAX_IMPORT_BYTES), "2 GiB");
        assert_eq!(human_bytes(MAX_IMPORT_ENTRIES), "195.3 KiB");
        assert_eq!(human_bytes(10), "10 bytes");
        assert_eq!(human_bytes(1 << 20), "1 MiB");
    }
}
