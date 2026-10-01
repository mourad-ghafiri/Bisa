//! Backend-agnostic change capture.
//!
//! Two paths, both producing a [`Diff`] = list of [`FileChange`]:
//!
//! - **Git mode.** When `merged/.git` exists (dir or worktree file) we ask
//!   `bisa-vcs` for the patch against HEAD plus the untracked paths,
//!   split on `diff --git` headers, and emit one [`FileChange`] per file.
//!   Binary entries surface as
//!   `diff: None`. Untracked text files are rendered as an added-file unified
//!   diff via `similar` so the whole patch stays `git apply`-consumable.
//! - **Plain mode.** No `.git`: walk both trees (skipping any `.git`
//!   component), short-circuit on `(size, mtime)` equality, then content
//!   compare and emit a unified diff per surviving pair.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use bisa_vcs::git;

use crate::{IsoError, IsoResult};

/// Captured changes between a `lower` baseline and a `merged` view.
#[derive(Debug, Clone, Default)]
pub struct Diff {
    pub files: Vec<FileChange>,
}

impl Diff {
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Concatenated unified-diff text for every text-representable entry.
    /// Binary entries are skipped — enumerate [`Self::files`] and copy them
    /// out-of-band if you need their contents.
    pub fn unified_text(&self) -> String {
        let mut out = String::new();
        for file in &self.files {
            let Some(d) = &file.diff else { continue };
            if d.is_empty() {
                continue;
            }
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(d);
        }
        out
    }
}

/// One entry in a [`Diff`]. `path` is relative to `merged`; `diff: None`
/// means binary/text-unrepresentable.
#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: PathBuf,
    pub op: ChangeKind,
    pub diff: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Modified,
    Removed,
}

/// Default diff: git when `merged` is a git tree, tree walk otherwise.
pub fn default_diff(lower: &Path, merged: &Path) -> IsoResult<Diff> {
    if merged.join(".git").exists() {
        git_diff(merged)
    } else {
        plain_diff(lower, merged)
    }
}

// ---------------------------------------------------------------------------
// Git mode
// ---------------------------------------------------------------------------

fn git_diff(merged: &Path) -> IsoResult<Diff> {
    let mut files = parse_git_diff(&git::diff_head(merged)?);

    for rel in git::list_untracked(merged)? {
        let abs = merged.join(&rel);
        let diff = fs::read(&abs).ok().and_then(|bytes| {
            if is_binary(&bytes) {
                None
            } else {
                let text = String::from_utf8_lossy(&bytes).into_owned();
                Some(added_file_patch(&rel, &text))
            }
        });
        files.push(FileChange {
            path: rel,
            op: ChangeKind::Added,
            diff,
        });
    }
    Ok(Diff { files })
}

/// Split `git diff` output on `diff --git` headers into per-file changes.
fn parse_git_diff(output: &str) -> Vec<FileChange> {
    let mut files = Vec::new();
    let mut current: Option<(PathBuf, ChangeKind, String, bool)> = None;

    let flush = |current: &mut Option<(PathBuf, ChangeKind, String, bool)>,
                 files: &mut Vec<FileChange>| {
        if let Some((path, op, text, binary)) = current.take() {
            files.push(FileChange {
                path,
                op,
                diff: if binary { None } else { Some(text) },
            });
        }
    };

    for line in output.split_inclusive('\n') {
        if line.starts_with("diff --git ") {
            flush(&mut current, &mut files);
            let path = parse_header_path(line.trim_end());
            current = Some((path, ChangeKind::Modified, line.to_string(), false));
            continue;
        }
        if let Some((path0, op, text, binary)) = current.as_mut() {
            if line.starts_with("new file mode") {
                *op = ChangeKind::Added;
            } else if line.starts_with("deleted file mode") {
                *op = ChangeKind::Removed;
            } else if line.starts_with("Binary files") || line.starts_with("GIT binary patch") {
                *binary = true;
            } else if let Some(rest) = line.strip_prefix("+++ b/") {
                *path0 = PathBuf::from(rest.trim_end());
            }
            text.push_str(line);
        }
    }
    flush(&mut current, &mut files);
    files
}

/// Extract the b-side path from a `diff --git a/x b/x` header (fallback when
/// the +++ line is absent, e.g. deletions where the b side is /dev/null).
fn parse_header_path(header: &str) -> PathBuf {
    header
        .rsplit_once(" b/")
        .map(|(_, b)| PathBuf::from(b))
        .unwrap_or_else(|| PathBuf::from("unknown"))
}

/// Render an untracked file as a `git apply`-able added-file patch.
fn added_file_patch(rel: &Path, content: &str) -> String {
    let p = rel.display();
    let mut patch =
        format!("diff --git a/{p} b/{p}\nnew file mode 100644\n--- /dev/null\n+++ b/{p}\n");
    let unified = similar::TextDiff::from_lines("", content)
        .unified_diff()
        .context_radius(3)
        .to_string();
    // Drop similar's own ---/+++ header lines; we already wrote git-style ones.
    for line in unified.lines() {
        if line.starts_with("---") || line.starts_with("+++") {
            continue;
        }
        patch.push_str(line);
        patch.push('\n');
    }
    patch
}

// ---------------------------------------------------------------------------
// Plain mode
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Meta {
    size: u64,
    mtime: Option<SystemTime>,
}

fn plain_diff(lower: &Path, merged: &Path) -> IsoResult<Diff> {
    let lower_files = walk(lower)?;
    let merged_files = walk(merged)?;
    let mut files = Vec::new();

    for rel in lower_files.keys() {
        if !merged_files.contains_key(rel) {
            let old = read_text(&lower.join(rel));
            files.push(FileChange {
                path: rel.clone(),
                op: ChangeKind::Removed,
                diff: old.map(|t| unified(rel, &t, "")),
            });
        }
    }

    for (rel, m_meta) in &merged_files {
        match lower_files.get(rel) {
            None => {
                let new = read_text(&merged.join(rel));
                files.push(FileChange {
                    path: rel.clone(),
                    op: ChangeKind::Added,
                    diff: new.map(|t| unified(rel, "", &t)),
                });
            }
            Some(l_meta) => {
                // Cheap short-circuit before content comparison.
                if l_meta.size == m_meta.size
                    && l_meta.mtime.is_some()
                    && l_meta.mtime == m_meta.mtime
                {
                    continue;
                }
                let old_bytes = fs::read(lower.join(rel)).unwrap_or_default();
                let new_bytes = fs::read(merged.join(rel)).unwrap_or_default();
                if old_bytes == new_bytes {
                    continue;
                }
                let text = if is_binary(&old_bytes) || is_binary(&new_bytes) {
                    None
                } else {
                    Some(unified(
                        rel,
                        &String::from_utf8_lossy(&old_bytes),
                        &String::from_utf8_lossy(&new_bytes),
                    ))
                };
                files.push(FileChange {
                    path: rel.clone(),
                    op: ChangeKind::Modified,
                    diff: text,
                });
            }
        }
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Diff { files })
}

fn unified(rel: &Path, old: &str, new: &str) -> String {
    let p = rel.display();
    similar::TextDiff::from_lines(old, new)
        .unified_diff()
        .context_radius(3)
        .header(&format!("a/{p}"), &format!("b/{p}"))
        .to_string()
}

fn read_text(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    if is_binary(&bytes) {
        None
    } else {
        Some(String::from_utf8_lossy(&bytes).into_owned())
    }
}

/// NUL within the first 8 KiB classifies the file as binary.
fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|b| *b == 0)
}

/// Recursively collect `rel -> meta`, skipping any `.git` component.
fn walk(root: &Path) -> IsoResult<BTreeMap<PathBuf, Meta>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(&dir)
            .map_err(|e| IsoError::other(format!("read_dir {}: {e}", dir.display())))?;
        for entry in entries {
            let entry = entry.map_err(|e| IsoError::other(e.to_string()))?;
            let path = entry.path();
            if path.file_name().is_some_and(|n| n == ".git") {
                continue;
            }
            let meta = entry
                .metadata()
                .map_err(|e| IsoError::other(e.to_string()))?;
            if meta.is_dir() {
                stack.push(path);
            } else if meta.is_file() {
                let rel = path
                    .strip_prefix(root)
                    .expect("walk stays under root")
                    .to_path_buf();
                out.insert(
                    rel,
                    Meta {
                        size: meta.len(),
                        mtime: meta.modified().ok(),
                    },
                );
            }
        }
    }
    Ok(out)
}
