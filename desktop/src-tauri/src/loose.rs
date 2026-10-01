//! Loose files (ide/03 §Loose files): a file anywhere on this machine — dropped
//! from the file manager or picked in the open dialog — read and saved back by
//! its absolute path, under no project, goal or group.
//!
//! **A machine capability, so it is the shell's** (ide/01): the node may hold
//! any capability whose blast radius is the workspace, and a file at an
//! arbitrary path is the machine's. The node keeps its three scopes and never
//! learns an absolute path; the webview names one here, as it always has for
//! *Reveal*. The shell writes as the person would with any editor — nothing
//! here is narrower than their account, and nothing is wider.
//!
//! **The same rules the node's editor route keeps**, byte for byte, so one
//! document behaves one way whichever side reads it: a NUL byte or invalid
//! UTF-8 is binary and carries no text; above [`REFUSE_BYTES`] the read is
//! refused with the size; above [`EDITABLE_BYTES`] it opens read-only; the
//! `hash` is sha256 of the bytes served, and a save is **compare-and-swap,
//! never last-write-wins** — no hash creates and refuses an existing path, a
//! hash that no longer matches is a conflict carrying the current text, and
//! the write is atomic (a temp file beside the target, renamed over it).
//!
//! No watcher and no language server follow a loose file: those are a root's.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Editable to here, read-only to the refusal — the node's `ide.rs` numbers.
pub const EDITABLE_BYTES: u64 = 2 * 1024 * 1024;
pub const REFUSE_BYTES: u64 = 20 * 1024 * 1024;

/// One file for the editor, in the shape `GET /ide/file` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LooseFile {
    /// The canonical absolute path — what a save names.
    pub path: String,
    pub name: String,
    pub size: u64,
    pub binary: bool,
    pub truncated: bool,
    /// Within the editable size; above it the editor opens read-only.
    pub editable: bool,
    /// The text, when the file is text and within the cap.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// sha256 of the served bytes — the `base_hash` a save must carry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
}

/// A save that landed, in the shape `PUT /ide/file` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LooseWritten {
    pub path: String,
    pub hash: String,
    pub created: bool,
}

/// Why a read or a write did not happen. Tagged, so the webview raises the
/// same errors the node's routes would: a conflict with the current text
/// (409), a file that is not there (404), a file past the bound with its
/// size and the bound (413 — the editor says both and offers *Reveal*),
/// anything else refused (400).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LooseError {
    Missing {
        message: String,
    },
    Refused {
        message: String,
    },
    TooLarge {
        message: String,
        size: u64,
        limit: u64,
    },
    Conflict {
        message: String,
        current_hash: String,
        current_text: String,
    },
}

impl LooseError {
    fn refused(message: impl Into<String>) -> Self {
        LooseError::Refused {
            message: message.into(),
        }
    }
}

/// sha256 hex of some bytes — the node's `content_hash`, so a hash from one
/// side is a hash on the other.
pub fn content_hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// The absolute path a caller named, or the refusal. Relative paths are
/// refused rather than resolved: the shell has no working directory a person
/// could mean.
fn absolute(path: &str) -> Result<PathBuf, LooseError> {
    let given = Path::new(path);
    if path.trim().is_empty() {
        return Err(LooseError::refused("a path is needed"));
    }
    if !given.is_absolute() {
        return Err(LooseError::refused(format!(
            "{path} is not an absolute path"
        )));
    }
    Ok(given.to_path_buf())
}

/// Read one file for the editor.
pub fn read_loose(path: &str) -> Result<LooseFile, LooseError> {
    let given = absolute(path)?;
    let canonical = given.canonicalize().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            LooseError::Missing {
                message: format!("no such file: {path}"),
            }
        } else {
            LooseError::refused(format!("cannot read {path}: {e}"))
        }
    })?;
    let meta = std::fs::metadata(&canonical)
        .map_err(|e| LooseError::refused(format!("cannot read {path}: {e}")))?;
    if meta.is_dir() {
        return Err(LooseError::refused(format!(
            "{path} is a folder, not a file"
        )));
    }
    let size = meta.len();
    if size > REFUSE_BYTES {
        return Err(LooseError::TooLarge {
            message: format!("{path} is {size} bytes; the editor stops at {REFUSE_BYTES}"),
            size,
            limit: REFUSE_BYTES,
        });
    }
    let name = canonical
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut buf = Vec::new();
    // One byte past the cap is how we learn we hit it — the stat's length lies
    // for anything the kernel synthesizes.
    std::fs::File::open(&canonical)
        .map_err(|e| LooseError::refused(format!("cannot read {path}: {e}")))?
        .take(REFUSE_BYTES + 1)
        .read_to_end(&mut buf)
        .map_err(|e| LooseError::refused(format!("cannot read {path}: {e}")))?;
    let mut truncated = buf.len() as u64 > REFUSE_BYTES;
    buf.truncate(REFUSE_BYTES as usize);
    let size = size.max(buf.len() as u64);
    let shown = canonical.to_string_lossy().into_owned();
    let binary = LooseFile {
        path: shown.clone(),
        name: name.clone(),
        size,
        binary: true,
        truncated: false,
        editable: false,
        text: None,
        hash: None,
    };
    // A NUL byte is the one tell that costs nothing and is never wrong about text.
    if buf.contains(&0) {
        return Ok(binary);
    }
    let text = match std::str::from_utf8(&buf) {
        Ok(s) => s.to_owned(),
        // `error_len: None` means the bytes end mid-character: text, cut short.
        Err(e) if e.error_len().is_none() => {
            truncated = true;
            String::from_utf8_lossy(&buf[..e.valid_up_to()]).into_owned()
        }
        Err(_) => return Ok(binary),
    };
    let hash = if truncated {
        None
    } else {
        Some(content_hash(&buf))
    };
    Ok(LooseFile {
        path: shown,
        name,
        size,
        binary: false,
        truncated,
        editable: !truncated && size <= EDITABLE_BYTES,
        text: Some(text),
        hash,
    })
}

static WRITE_SEQ: AtomicU64 = AtomicU64::new(0);

/// Write bytes beside the target and rename over it, so a reader never sees
/// half a save. The temp file goes with a failure.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let name = path
        .file_name()
        .ok_or_else(|| std::io::Error::other("a path with no file name"))?;
    let mut tmp_name = name.to_os_string();
    tmp_name.push(format!(
        ".tmp.{}.{}",
        std::process::id(),
        WRITE_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let tmp = path.with_file_name(tmp_name);
    let result = (|| {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        // The temp file the failed save left: removed, and said when it
        // could not be — never silently left behind.
        if let Err(e) = std::fs::remove_file(&tmp) {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::debug!("a temp file was left behind at {}: {e}", tmp.display());
            }
        }
    }
    result
}

/// Save `text` at `path`, guarded by the hash of what was read — the node's
/// `write_file` rule, verbatim: `base_hash: None` creates and refuses an
/// existing path; `Some` refuses unless the file's current bytes hash to it.
pub fn write_loose(
    path: &str,
    text: &str,
    base_hash: Option<&str>,
) -> Result<LooseWritten, LooseError> {
    let given = absolute(path)?;
    let Some(name) = given.file_name().map(|n| n.to_os_string()) else {
        return Err(LooseError::refused(format!("{path} names no file")));
    };
    // The folder must be there already: a save dialog only offers such paths,
    // and a drop came from one. Canonical, so a symlinked folder is written
    // where it really is and the answer names that place.
    let parent = given
        .parent()
        .ok_or_else(|| LooseError::refused(format!("{path} has no folder")))?
        .canonicalize()
        .map_err(|e| LooseError::refused(format!("cannot save {path}: its folder: {e}")))?;
    let target = parent.join(&name);
    if target.is_dir() {
        return Err(LooseError::refused(format!("{path} is a folder")));
    }
    let existing = match std::fs::read(&target) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(LooseError::refused(format!("cannot save {path}: {e}"))),
    };
    let shown = target.to_string_lossy().into_owned();
    match (base_hash, &existing) {
        (None, Some(_)) => {
            return Err(LooseError::refused(format!(
                "{shown} already exists; send the base_hash of what you read to overwrite it"
            )))
        }
        (Some(base), Some(bytes)) => {
            let current_hash = content_hash(bytes);
            if current_hash != base {
                return Err(LooseError::Conflict {
                    message: format!("{shown} changed since you read it"),
                    current_hash,
                    current_text: String::from_utf8_lossy(bytes).into_owned(),
                });
            }
        }
        (Some(_), None) => {
            return Err(LooseError::Conflict {
                message: format!("{shown} is no longer there"),
                current_hash: String::new(),
                current_text: String::new(),
            })
        }
        (None, None) => {}
    }
    write_atomic(&target, text.as_bytes())
        .map_err(|e| LooseError::refused(format!("cannot save {shown}: {e}")))?;
    Ok(LooseWritten {
        path: shown,
        hash: content_hash(text.as_bytes()),
        created: existing.is_none(),
    })
}

/// `read_loose`, as the webview calls it.
#[tauri::command]
pub fn read_loose_file(path: String) -> Result<LooseFile, LooseError> {
    read_loose(&path)
}

/// `write_loose`, as the webview calls it.
#[tauri::command]
pub fn write_loose_file(
    path: String,
    text: String,
    base_hash: Option<String>,
) -> Result<LooseWritten, LooseError> {
    write_loose(&path, &text, base_hash.as_deref())
}

/// The files the drag that just ended carried, by absolute path.
///
/// The window keeps HTML5 drops (`dragDropEnabled: false` — the composer's
/// attachments and the workflow palette depend on the DOM seeing them), and
/// the DOM's `File` has bytes and no path. On macOS the drag pasteboard still
/// holds the dropped file URLs when the DOM's `drop` fires, so the webview
/// asks here and pairs the names (`dropModel.pathsForDrop`). Elsewhere the
/// answer is empty and the workbench points at *Open file…*.
#[tauri::command]
pub fn dropped_paths() -> Vec<String> {
    drag_pasteboard_paths()
}

#[cfg(target_os = "macos")]
fn drag_pasteboard_paths() -> Vec<String> {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_foundation::{NSArray, NSString, NSURL};

    let Some(cls) = AnyClass::get(c"NSPasteboard") else {
        return Vec::new();
    };
    // `NSPasteboardNameDrag`: the pasteboard every drag writes to.
    let name = NSString::from_str("Apple CFPasteboard drag");
    let board: Option<Retained<AnyObject>> = unsafe { msg_send![cls, pasteboardWithName: &*name] };
    let Some(board) = board else {
        return Vec::new();
    };
    let items: Option<Retained<NSArray<AnyObject>>> =
        unsafe { msg_send![&*board, pasteboardItems] };
    let Some(items) = items else {
        return Vec::new();
    };
    let file_url = NSString::from_str("public.file-url");
    let mut out = Vec::new();
    for item in items.iter() {
        let url: Option<Retained<NSString>> =
            unsafe { msg_send![&*item, stringForType: &*file_url] };
        let Some(url) = url else { continue };
        // `NSURL` decodes the file URL — percent escapes, `file://localhost/`.
        let Some(parsed) = NSURL::URLWithString(&url) else {
            continue;
        };
        if let Some(path) = parsed.path() {
            out.push(path.to_string());
        }
    }
    out
}

#[cfg(not(target_os = "macos"))]
fn drag_pasteboard_paths() -> Vec<String> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn a_read_carries_the_text_its_hash_and_the_canonical_path() {
        let d = dir();
        let p = d.path().join("notes.txt");
        std::fs::write(&p, "hello\n").unwrap();
        let f = read_loose(p.to_str().unwrap()).unwrap();
        assert_eq!(f.text.as_deref(), Some("hello\n"));
        assert_eq!(f.hash.as_deref(), Some(content_hash(b"hello\n").as_str()));
        assert_eq!(f.name, "notes.txt");
        assert!(f.editable && !f.binary && !f.truncated);
        assert_eq!(f.size, 6);
        assert_eq!(f.path, p.canonicalize().unwrap().to_string_lossy());
    }

    #[test]
    fn a_relative_path_a_folder_and_a_missing_file_are_each_refused_by_name() {
        let d = dir();
        assert!(matches!(
            read_loose("notes.txt"),
            Err(LooseError::Refused { .. })
        ));
        assert!(matches!(read_loose(""), Err(LooseError::Refused { .. })));
        assert!(
            matches!(read_loose(d.path().to_str().unwrap()), Err(LooseError::Refused { message }) if message.contains("folder"))
        );
        let gone = d.path().join("gone.txt");
        assert!(matches!(
            read_loose(gone.to_str().unwrap()),
            Err(LooseError::Missing { .. })
        ));
    }

    #[test]
    fn a_binary_file_carries_no_text_and_no_hash_and_a_large_one_opens_read_only() {
        let d = dir();
        let bin = d.path().join("a.bin");
        std::fs::write(&bin, b"ab\0cd").unwrap();
        let f = read_loose(bin.to_str().unwrap()).unwrap();
        assert!(f.binary && !f.editable && f.text.is_none() && f.hash.is_none());
        // Real text, one byte over the line — a `set_len` hole would be NUL
        // bytes, which the reader rightly calls binary.
        let big = d.path().join("big.txt");
        std::fs::write(&big, vec![b'a'; EDITABLE_BYTES as usize + 1]).unwrap();
        let f = read_loose(big.to_str().unwrap()).unwrap();
        assert!(!f.binary && !f.truncated, "text, and all of it read");
        assert!(
            !f.editable,
            "above the editable size the editor is read-only"
        );
        assert!(f.hash.is_some(), "still a hash: the bytes were all read");
    }

    /// A file past the bound is refused with its size and the bound, tagged,
    /// so the editor says both and offers *Reveal* — as it does for the
    /// node's own 413. A `set_len` past the bound makes a sparse file that
    /// costs no disk.
    #[test]
    fn a_file_past_the_bound_is_refused_with_its_size_and_the_bound() {
        let d = dir();
        let huge = d.path().join("huge.txt");
        let file = std::fs::File::create(&huge).unwrap();
        file.set_len(REFUSE_BYTES + 7).unwrap();
        drop(file);
        let refused = read_loose(huge.to_str().unwrap()).expect_err("past the bound");
        match &refused {
            LooseError::TooLarge {
                message,
                size,
                limit,
            } => {
                assert_eq!(*size, REFUSE_BYTES + 7);
                assert_eq!(*limit, REFUSE_BYTES);
                assert!(message.contains("huge.txt"), "{message}");
            }
            other => panic!("{other:?}"),
        }
        let json = serde_json::to_value(&refused).unwrap();
        assert_eq!(json["kind"], "too_large");
        assert_eq!(json["limit"], REFUSE_BYTES);
    }

    #[test]
    fn a_save_is_compare_and_swap_and_a_conflict_carries_the_current_text() {
        let d = dir();
        let p = d.path().join("a.txt");
        let path = p.to_str().unwrap();
        let w = write_loose(path, "one", None).unwrap();
        assert!(w.created);
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "one");
        // A create over an existing file is refused, never a clobber.
        assert!(
            matches!(write_loose(path, "x", None), Err(LooseError::Refused { message }) if message.contains("already exists"))
        );
        // The right hash writes; the same file read back carries the new hash.
        let w2 = write_loose(path, "two", Some(&w.hash)).unwrap();
        assert!(!w2.created);
        assert_eq!(
            read_loose(path).unwrap().hash.as_deref(),
            Some(w2.hash.as_str())
        );
        // A stale hash is a conflict with what is there now.
        match write_loose(path, "three", Some(&w.hash)) {
            Err(LooseError::Conflict {
                current_hash,
                current_text,
                ..
            }) => {
                assert_eq!(current_text, "two");
                assert_eq!(current_hash, w2.hash);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "two", "nothing moved");
        // A hash for a file that is gone is a conflict too — not a create.
        std::fs::remove_file(&p).unwrap();
        assert!(matches!(
            write_loose(path, "four", Some(&w2.hash)),
            Err(LooseError::Conflict { .. })
        ));
        assert!(!p.exists());
    }

    #[test]
    fn a_save_needs_its_folder_and_leaves_no_temp_file_behind() {
        let d = dir();
        let nowhere = d.path().join("missing").join("a.txt");
        assert!(matches!(
            write_loose(nowhere.to_str().unwrap(), "x", None),
            Err(LooseError::Refused { .. })
        ));
        let p = d.path().join("b.txt");
        write_loose(p.to_str().unwrap(), "x", None).unwrap();
        let names: Vec<String> = std::fs::read_dir(d.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["b.txt".to_string()]);
        assert!(
            matches!(
                write_loose(d.path().to_str().unwrap(), "x", None),
                Err(LooseError::Refused { .. })
            ),
            "a folder is not a file to save"
        );
    }

    #[test]
    fn the_hash_is_the_nodes_sha256_hex() {
        assert_eq!(
            content_hash(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
