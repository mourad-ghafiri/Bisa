//! Two machine capabilities around an artifact's **named copy** (ide/12): open
//! it with whatever application claims it, and copy it where the person chose
//! in the save dialog.
//!
//! Both take a path the webview learned from the node — the blob under its
//! maker's name, `<data dir>/attachments/named/<sha256>/<name>` — and both
//! refuse anything that is not such a file. The shell does not know the
//! node's data directory, so the check is structural: the file's parent is
//! sixty-four lowercase hex characters, its grandparent is `named`, and that
//! one's parent is `attachments`. A path shaped like that and not under the
//! store is not a path anybody has a reason to hand the shell; a path shaped
//! otherwise is refused by name. No bytes cross the IPC bridge either way.

use std::path::{Path, PathBuf};

/// The named copy a path is, or why it is not one.
fn named_copy(path: &str) -> Result<PathBuf, String> {
    let given = Path::new(path);
    if !given.is_absolute() {
        return Err(format!("{path} is not an absolute path"));
    }
    let canonical = given
        .canonicalize()
        .map_err(|e| format!("cannot read {path}: {e}"))?;
    if !canonical.is_file() {
        return Err(format!("{path} is not a file"));
    }
    let name = |p: Option<&Path>| {
        p.and_then(Path::file_name)
            .map(|n| n.to_string_lossy().to_string())
    };
    let parent = canonical.parent();
    let sha = name(parent).unwrap_or_default();
    let named = name(parent.and_then(Path::parent)).unwrap_or_default();
    let attachments =
        name(parent.and_then(Path::parent).and_then(Path::parent)).unwrap_or_default();
    let is_sha = sha.len() == 64
        && sha
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if !is_sha || named != "named" || attachments != "attachments" {
        return Err(format!("{path} is not an artifact's named copy"));
    }
    Ok(canonical)
}

/// Open the named copy with the default application for its kind.
#[tauri::command]
pub fn open_artifact(path: String) -> Result<(), String> {
    let file = named_copy(&path)?;
    tauri_plugin_opener::open_path(file, None::<&str>).map_err(|e| e.to_string())
}

/// Copy the named copy to where the save dialog pointed.
#[tauri::command]
pub fn copy_artifact(from: String, to: String) -> Result<(), String> {
    let file = named_copy(&from)?;
    let to = PathBuf::from(&to);
    std::fs::copy(&file, &to)
        .map(|_| ())
        .map_err(|e| format!("could not write {}: {e}", to.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_named_copy_inside_the_store_is_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let sha = "a".repeat(64);
        let good = dir
            .path()
            .join("attachments")
            .join("named")
            .join(&sha)
            .join("report.pdf");
        std::fs::create_dir_all(good.parent().unwrap()).unwrap();
        std::fs::write(&good, b"%PDF").unwrap();
        assert_eq!(
            named_copy(good.to_str().unwrap()).unwrap(),
            good.canonicalize().unwrap()
        );

        let blob = dir.path().join("attachments").join("aa").join(&sha[2..]);
        std::fs::create_dir_all(blob.parent().unwrap()).unwrap();
        std::fs::write(&blob, b"%PDF").unwrap();
        assert!(
            named_copy(blob.to_str().unwrap()).is_err(),
            "the blob itself is not a named copy"
        );

        let elsewhere = dir.path().join("named").join(&sha).join("x.txt");
        std::fs::create_dir_all(elsewhere.parent().unwrap()).unwrap();
        std::fs::write(&elsewhere, b"x").unwrap();
        assert!(
            named_copy(elsewhere.to_str().unwrap()).is_err(),
            "no attachments folder above"
        );

        let short = dir
            .path()
            .join("attachments")
            .join("named")
            .join("abc")
            .join("x.txt");
        std::fs::create_dir_all(short.parent().unwrap()).unwrap();
        std::fs::write(&short, b"x").unwrap();
        assert!(
            named_copy(short.to_str().unwrap()).is_err(),
            "the parent is not a digest"
        );

        assert!(named_copy("relative/x.txt").is_err());
        assert!(
            named_copy(good.parent().unwrap().to_str().unwrap()).is_err(),
            "a folder is not a file"
        );
    }
}
