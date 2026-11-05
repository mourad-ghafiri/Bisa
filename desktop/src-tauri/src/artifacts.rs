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
//!
//! Opening is narrower still: the default application is handed only a file
//! of a kind it can only *show* (`OPENABLE_EXTENSIONS`). An agent names the
//! file, so the name is not trusted — a `.command`, a `.sh`, a `.webloc` or a
//! `.pkg` would *run* under the default application, and a `.html` or `.svg`
//! would open as a `file://` page with the store's folder as its origin. The
//! desktop asks the same question first (`artifactModel.opensWithDefaultApp`)
//! and holds the button; this is the wall that stands when the webview's does
//! not, and a test keeps the two lists equal.

use std::path::{Path, PathBuf};

/// The extensions the default application may be handed: pictures, sound and
/// video, a PDF, a sheet, a document, a deck, Markdown, plain text and data —
/// the desktop's `OPENABLE_KINDS` over its extension table, held equal by
/// `artifactModel.test.mjs`. Lowercase; a name is lowered before it is read.
const OPENABLE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "avif", "bmp", "heic", "ico", "mp4", "webm", "mov", "m4v",
    "mp3", "wav", "ogg", "m4a", "flac", "aac", "pdf", "csv", "tsv", "xlsx", "xls", "ods", "docx",
    "pptx", "md", "mdx", "markdown", "txt", "log", "json", "jsonl", "yaml", "yml", "toml", "xml",
];

/// Whether the default application may be handed `file`, by its name alone.
fn openable(file: &Path) -> Result<(), String> {
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => ext.to_ascii_lowercase(),
        _ => String::new(),
    };
    if OPENABLE_EXTENSIONS.contains(&ext.as_str()) {
        Ok(())
    } else {
        Err(format!(
            "{name} is kept inside Bisa: a file of this kind could run as a program if opened outside"
        ))
    }
}

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

/// Open the named copy with the default application for its kind — a kind
/// that application can only show (`openable`); anything else is refused
/// by name before anything is opened.
#[tauri::command]
pub fn open_artifact(path: String) -> Result<(), String> {
    let file = named_copy(&path)?;
    openable(&file)?;
    // LCOV_EXCL_START opens the file with the system's default app; out by decision
    tauri_plugin_opener::open_path(file, None::<&str>).map_err(|e| e.to_string())
    // LCOV_EXCL_STOP
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

    /// The default application is handed what it can only show — by the
    /// extension, in any case — and a file that could run, a page, a
    /// figure or a name with no extension is refused before anything opens,
    /// named copy or not.
    #[test]
    fn only_a_kind_the_default_app_can_only_show_is_opened() {
        for ok in [
            "report.pdf",
            "Photo.PNG",
            "clip.mp4",
            "song.mp3",
            "q3.csv",
            "book.xlsx",
            "memo.docx",
            "deck.pptx",
            "README.md",
            "notes.txt",
            "data.json",
            "conf.toml",
        ] {
            assert!(openable(Path::new(ok)).is_ok(), "{ok}");
        }
        for held in [
            "page.html",
            "figure.svg",
            "run.sh",
            "setup.py",
            "Open.command",
            "a.terminal",
            "link.webloc",
            "x.inetloc",
            "tool.app",
            "pkg.pkg",
            "d.dmg",
            "x.jar",
            "s.scpt",
            "w.workflow",
            "m.docm",
            "m.xlsm",
            "m.pptm",
            "diagram.mmd",
            "noext",
            ".env",
            ".pdf",
            "thing.xyz",
        ] {
            let refused = openable(Path::new(held)).unwrap_err();
            assert!(refused.contains("kept inside Bisa"), "{held}: {refused}");
        }
        // A named copy of a kind that could run is refused by the command
        // itself, before the opener is asked.
        let dir = tempfile::tempdir().unwrap();
        let named = dir
            .path()
            .join("attachments")
            .join("named")
            .join("c".repeat(64));
        std::fs::create_dir_all(&named).unwrap();
        let script = named.join("run.sh");
        std::fs::write(&script, "#!/bin/sh\necho hi\n").unwrap();
        let refused = open_artifact(script.display().to_string()).unwrap_err();
        assert!(refused.contains("run.sh"), "{refused}");
        assert!(
            open_artifact(named.join("missing.pdf").display().to_string())
                .unwrap_err()
                .contains("cannot read"),
            "the named-copy check still comes first"
        );
    }

    /// A named copy is copied where asked; a place that cannot be written
    /// and a file that is no named copy are each said.
    #[test]
    fn a_named_copy_is_copied_where_asked_or_the_refusal_is_said() {
        let dir = tempfile::tempdir().unwrap();
        let named = dir
            .path()
            .join("attachments")
            .join("named")
            .join("b".repeat(64));
        std::fs::create_dir_all(&named).unwrap();
        let file = named.join("plan.md");
        std::fs::write(&file, "hello").unwrap();
        let to = dir.path().join("out.md");
        copy_artifact(file.display().to_string(), to.display().to_string()).unwrap();
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "hello");
        let nowhere = dir.path().join("no/such/dir/out.md");
        assert!(
            copy_artifact(file.display().to_string(), nowhere.display().to_string())
                .unwrap_err()
                .contains("could not write")
        );
        let loose = dir.path().join("plan.md");
        std::fs::write(&loose, "x").unwrap();
        assert!(
            copy_artifact(loose.display().to_string(), to.display().to_string())
                .unwrap_err()
                .contains("not an artifact's named copy")
        );
    }
}
