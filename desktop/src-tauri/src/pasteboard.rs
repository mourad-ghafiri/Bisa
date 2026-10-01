//! What the clipboard holds, pasted into a checkout (ide/03 §The
//! explorer): the general pasteboard's file URLs read by the shell — the
//! way `loose.rs` reads the drag board — and copied into a folder of the
//! project by absolute path, in this process; and the picture a screenshot
//! or a copy in another app left there, handed to the webview as PNG bytes
//! for a conversation's attachment, or written under the name the person
//! confirmed. The machine's capability, in the machine's process (ide/01):
//! the node never learns where a file came from; its watcher sees the new
//! entries and announces them like any outside write.
//!
//! The rules are Finder's own where Finder has one: a taken name becomes
//! `name 2`, then `name 3`; a folder is copied whole. Where it has none the
//! answer is to leave the thing out and say so — a symlink is not
//! recreated, a nested `.git` is not copied into somebody's repository, a
//! folder is never pasted into itself. Nothing here decides for the person:
//! every skip is named in the report.

use serde::Serialize;
use std::path::Path;

/// One entry pasted: where it came from and the name it landed under.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Pasted {
    pub from: String,
    pub name: String,
}

/// One entry left out, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Skipped {
    pub path: String,
    pub why: String,
}

/// What a paste did.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct PasteReport {
    pub pasted: Vec<Pasted>,
    pub skipped: Vec<Skipped>,
}

/// What the general pasteboard holds that a paste can take: the absolute
/// paths of the files a copy in the file manager put there, and whether a
/// picture is there (a screenshot, a copy in a picture app). Empty and
/// false where the shell cannot tell: another platform, or a clipboard
/// holding text alone.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Holdings {
    pub paths: Vec<String>,
    pub image: bool,
}

/// One read of the pasteboard, for the desktop's clipboard store.
#[tauri::command]
pub fn pasteboard_holds() -> Holdings {
    general_pasteboard_holdings()
}

/// The picture the general pasteboard holds, as PNG bytes — its own PNG
/// as it is, else its TIFF encoded — answered raw, as the browser's
/// snapshot is. Empty bytes when there is no picture: a PNG is never
/// empty, so the webview needs no second word.
#[tauri::command]
pub fn pasteboard_image() -> Result<tauri::ipc::Response, String> {
    Ok(tauri::ipc::Response::new(
        general_pasteboard_png().unwrap_or_default(),
    ))
}

/// Write the pasteboard's picture into the folder `dest` under `name` —
/// read when the name is confirmed, so the bytes cross no IPC twice — and
/// say what landed. The name is the person's: a taken one is refused, not
/// numbered.
#[tauri::command]
pub async fn paste_image_into(dest: String, name: String) -> Result<Pasted, String> {
    let png = general_pasteboard_png()
        .ok_or_else(|| "the clipboard no longer holds a picture".to_string())?;
    tauri::async_runtime::spawn_blocking(move || write_png(Path::new(&dest), &name, &png))
        .await
        .map_err(|e| e.to_string())?
}

/// Write `png` as the file `name` in the folder `dest`. `dest` must be an
/// existing directory; `name` a plain file name — no slash, not `.` or
/// `..`, no leading dot, no control character, the rule of the store's
/// `sanitise_file_name`; the bytes must start as a PNG does; a taken name
/// is refused. Each refusal is a sentence for the draft row.
pub fn write_png(dest: &Path, name: &str, png: &[u8]) -> Result<Pasted, String> {
    if !dest.is_dir() {
        return Err(format!("{} is not a folder", dest.display()));
    }
    let name = plain_name(name)?;
    if !crate::png::is_png(png) {
        return Err("the clipboard's picture is not a PNG".to_string());
    }
    let target = dest.join(name);
    if std::fs::symlink_metadata(&target).is_ok() {
        return Err(format!("{name} is already here."));
    }
    std::fs::write(&target, png).map_err(|e| format!("{name}: {e}"))?;
    Ok(Pasted {
        from: "the clipboard".to_string(),
        name: name.to_string(),
    })
}

/// `name` trimmed, if it is a plain file name; else why it is not.
fn plain_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("A name is needed.".to_string());
    }
    if name.contains('/') || name.contains('\\') {
        return Err("A name has no slash in it.".to_string());
    }
    if name == "." || name == ".." || name.starts_with('.') {
        return Err("That is not a name.".to_string());
    }
    if name.chars().any(char::is_control) {
        return Err("A name has no control character in it.".to_string());
    }
    Ok(name)
}

/// Copy `sources` into the folder `dest`, each under a free name, and say
/// what landed and what was left out. Runs off the main thread: a folder
/// can be large.
#[tauri::command]
pub async fn paste_into(sources: Vec<String>, dest: String) -> Result<PasteReport, String> {
    tauri::async_runtime::spawn_blocking(move || copy_all(&sources, Path::new(&dest)))
        .await
        .map_err(|e| e.to_string())?
}

/// The name an entry lands under beside `existing`: its own, else Finder's
/// `name 2`, `name 3`, … — the number before the extension of a file.
pub fn free_name(name: &str, existing: &[String]) -> String {
    let taken = |candidate: &str| existing.iter().any(|e| e == candidate);
    if !taken(name) {
        return name.to_string();
    }
    let (stem, ext) = split_ext(name);
    for n in 2.. {
        let candidate = format!("{stem} {n}{ext}");
        if !taken(&candidate) {
            return candidate;
        }
    }
    unreachable!("the numbers do not run out")
}

/// A file's stem and its extension (with the dot), the way Finder counts
/// them: a leading dot is not an extension, and neither is a name with none.
fn split_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

/// Copy every source into `dest` and report. `dest` must be an existing
/// directory; a source must be absolute. A source that is missing, a
/// symlink, or a folder that holds `dest` is skipped by name.
pub fn copy_all(sources: &[String], dest: &Path) -> Result<PasteReport, String> {
    if !dest.is_dir() {
        return Err(format!("{} is not a folder", dest.display()));
    }
    let dest = dest
        .canonicalize()
        .map_err(|e| format!("{}: {e}", dest.display()))?;
    let mut report = PasteReport::default();
    let mut existing = names_in(&dest);
    for source in sources {
        let path = Path::new(source);
        if !path.is_absolute() {
            report.skipped.push(skipped(source, "not an absolute path"));
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(path) else {
            report.skipped.push(skipped(source, "not there"));
            continue;
        };
        if meta.file_type().is_symlink() {
            report.skipped.push(skipped(source, "a link, not copied"));
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            report.skipped.push(skipped(source, "no name"));
            continue;
        };
        if meta.is_dir() {
            let own = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
            if dest.starts_with(&own) {
                report.skipped.push(skipped(source, "into itself"));
                continue;
            }
        }
        let landed = free_name(name, &existing);
        let target = dest.join(&landed);
        let outcome = if meta.is_dir() {
            copy_tree(path, &target, &mut report.skipped)
        } else {
            std::fs::copy(path, &target).map(|_| ())
        };
        match outcome {
            Ok(()) => {
                existing.push(landed.clone());
                report.pasted.push(Pasted {
                    from: source.clone(),
                    name: landed,
                });
            }
            Err(e) => report.skipped.push(skipped(source, &e.to_string())),
        }
    }
    Ok(report)
}

/// A folder copied entry by entry: a file copied, a folder recursed, a
/// symlink and a nested `.git` left out and named.
fn copy_tree(from: &Path, to: &Path, skipped_out: &mut Vec<Skipped>) -> std::io::Result<()> {
    std::fs::create_dir(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        let name = entry.file_name();
        let meta = std::fs::symlink_metadata(&source)?;
        if meta.file_type().is_symlink() {
            skipped_out.push(skipped(&source.display().to_string(), "a link, not copied"));
            continue;
        }
        if meta.is_dir() {
            if name == ".git" {
                skipped_out.push(skipped(
                    &source.display().to_string(),
                    "a repository of its own, left out",
                ));
                continue;
            }
            copy_tree(&source, &to.join(&name), skipped_out)?;
        } else {
            std::fs::copy(&source, to.join(&name))?;
        }
    }
    Ok(())
}

fn names_in(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().to_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn skipped(path: &str, why: &str) -> Skipped {
    Skipped {
        path: path.to_string(),
        why: why.to_string(),
    }
}

#[cfg(target_os = "macos")]
mod general {
    //! The general pasteboard — `NSPasteboardNameGeneral`, the one a copy
    //! writes to — read through untyped messages, so no typed binding of
    //! `NSPasteboard` is needed.

    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_foundation::{NSArray, NSData, NSString, NSURL};

    const PNG: &str = "public.png";
    const TIFF: &str = "public.tiff";

    pub fn board() -> Option<Retained<AnyObject>> {
        let cls = AnyClass::get(c"NSPasteboard")?;
        let name = NSString::from_str("Apple CFPasteboard general");
        unsafe { msg_send![cls, pasteboardWithName: &*name] }
    }

    /// The file URLs' paths, one per item that carries one.
    pub fn paths(board: &AnyObject) -> Vec<String> {
        let items: Option<Retained<NSArray<AnyObject>>> =
            unsafe { msg_send![board, pasteboardItems] };
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

    /// Whether the board carries a picture in a form the shell encodes.
    pub fn has_image(board: &AnyObject) -> bool {
        let types =
            NSArray::from_retained_slice(&[NSString::from_str(PNG), NSString::from_str(TIFF)]);
        let found: Option<Retained<NSString>> =
            unsafe { msg_send![board, availableTypeFromArray: &*types] };
        found.is_some()
    }

    /// The picture as PNG bytes: its PNG as it is, else its TIFF encoded.
    pub fn png(board: &AnyObject) -> Option<Vec<u8>> {
        if let Some(png) = data_for(board, PNG) {
            return Some(png.to_vec());
        }
        crate::png::from_data(&*data_for(board, TIFF)?)
    }

    fn data_for(board: &AnyObject, uti: &str) -> Option<Retained<NSData>> {
        let uti = NSString::from_str(uti);
        unsafe { msg_send![board, dataForType: &*uti] }
    }
}

#[cfg(target_os = "macos")]
fn general_pasteboard_holdings() -> Holdings {
    let Some(board) = general::board() else {
        return Holdings::default();
    };
    Holdings {
        paths: general::paths(&board),
        image: general::has_image(&board),
    }
}

#[cfg(target_os = "macos")]
fn general_pasteboard_png() -> Option<Vec<u8>> {
    general::png(&*general::board()?)
}

#[cfg(not(target_os = "macos"))]
fn general_pasteboard_holdings() -> Holdings {
    Holdings::default()
}

#[cfg(not(target_os = "macos"))]
fn general_pasteboard_png() -> Option<Vec<u8>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(dir: &Path) -> Vec<String> {
        let mut v = names_in(dir);
        v.sort();
        v
    }

    #[test]
    fn a_free_name_is_the_name_itself_else_finders_second_and_third() {
        let taken = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(free_name("a.txt", &taken(&["b.txt"])), "a.txt");
        assert_eq!(free_name("a.txt", &taken(&["a.txt"])), "a 2.txt");
        assert_eq!(free_name("a.txt", &taken(&["a.txt", "a 2.txt"])), "a 3.txt");
        assert_eq!(
            free_name("src", &taken(&["src"])),
            "src 2",
            "a folder has no extension to keep"
        );
        assert_eq!(
            free_name(".env", &taken(&[".env"])),
            ".env 2",
            "a leading dot is not an extension"
        );
        assert_eq!(free_name("tar.gz", &taken(&["tar.gz"])), "tar 2.gz");
    }

    #[test]
    fn a_file_and_a_folder_are_copied_whole_and_a_taken_name_is_numbered() {
        let from = tempfile::tempdir().unwrap();
        let to = tempfile::tempdir().unwrap();
        std::fs::write(from.path().join("note.txt"), "hello").unwrap();
        std::fs::create_dir_all(from.path().join("site/css")).unwrap();
        std::fs::write(from.path().join("site/index.html"), "<p>").unwrap();
        std::fs::write(from.path().join("site/css/a.css"), "p{}").unwrap();
        let sources = vec![
            from.path().join("note.txt").display().to_string(),
            from.path().join("site").display().to_string(),
        ];
        let report = copy_all(&sources, to.path()).unwrap();
        assert_eq!(
            report
                .pasted
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["note.txt", "site"]
        );
        assert!(report.skipped.is_empty(), "{report:?}");
        assert_eq!(
            std::fs::read_to_string(to.path().join("note.txt")).unwrap(),
            "hello"
        );
        assert_eq!(
            std::fs::read_to_string(to.path().join("site/css/a.css")).unwrap(),
            "p{}"
        );
        assert_eq!(names(to.path()), ["note.txt", "site"]);

        // Again: nothing is overwritten, the names count up.
        let again = copy_all(&sources, to.path()).unwrap();
        assert_eq!(
            again
                .pasted
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["note 2.txt", "site 2"]
        );
        let third = copy_all(&sources[..1], to.path()).unwrap();
        assert_eq!(third.pasted[0].name, "note 3.txt");
        assert_eq!(
            std::fs::read_to_string(to.path().join("note.txt")).unwrap(),
            "hello",
            "the first is untouched"
        );
    }

    #[test]
    fn a_link_a_nested_repository_a_folder_into_itself_and_a_missing_source_are_left_out_by_name() {
        let from = tempfile::tempdir().unwrap();
        let to = tempfile::tempdir().unwrap();
        std::fs::write(from.path().join("real.txt"), "x").unwrap();
        std::os::unix::fs::symlink(from.path().join("real.txt"), from.path().join("link.txt"))
            .unwrap();
        std::fs::create_dir_all(from.path().join("repo/.git/objects")).unwrap();
        std::fs::write(from.path().join("repo/README.md"), "r").unwrap();
        std::os::unix::fs::symlink(
            from.path().join("real.txt"),
            from.path().join("repo/inner-link"),
        )
        .unwrap();

        let report = copy_all(
            &[
                from.path().join("link.txt").display().to_string(),
                from.path().join("repo").display().to_string(),
                from.path().join("gone.txt").display().to_string(),
                "relative/path.txt".to_string(),
            ],
            to.path(),
        )
        .unwrap();
        assert_eq!(
            report.pasted.len(),
            1,
            "the repository folder alone landed: {report:?}"
        );
        assert_eq!(report.pasted[0].name, "repo");
        assert!(to.path().join("repo/README.md").exists());
        assert!(
            !to.path().join("repo/.git").exists(),
            "a repository of its own is not copied into somebody's"
        );
        assert!(!to.path().join("repo/inner-link").exists());
        let whys: Vec<&str> = report.skipped.iter().map(|s| s.why.as_str()).collect();
        assert!(whys.contains(&"a link, not copied"), "{whys:?}");
        assert!(
            whys.contains(&"a repository of its own, left out"),
            "{whys:?}"
        );
        assert!(whys.contains(&"not there"), "{whys:?}");
        assert!(whys.contains(&"not an absolute path"), "{whys:?}");

        // A folder pasted into itself is refused, not copied without end.
        let inside = copy_all(&[to.path().display().to_string()], &to.path().join("repo")).unwrap();
        assert!(inside.pasted.is_empty());
        assert_eq!(inside.skipped[0].why, "into itself");

        // The destination must be a folder.
        assert!(copy_all(&[], &to.path().join("repo/README.md")).is_err());
    }

    fn png() -> Vec<u8> {
        let mut bytes = crate::png::SIGNATURE.to_vec();
        bytes.extend_from_slice(b"IHDR...");
        bytes
    }

    #[test]
    fn the_clipboards_picture_is_written_under_the_name_given_and_a_taken_name_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let landed = write_png(dir.path(), " login-bug.png ", &png()).unwrap();
        assert_eq!(
            landed.name, "login-bug.png",
            "the name is trimmed, not renumbered"
        );
        assert_eq!(landed.from, "the clipboard");
        assert_eq!(
            std::fs::read(dir.path().join("login-bug.png")).unwrap(),
            png()
        );

        let again = write_png(dir.path(), "login-bug.png", &png()).unwrap_err();
        assert_eq!(again, "login-bug.png is already here.");
        assert_eq!(
            std::fs::read(dir.path().join("login-bug.png")).unwrap(),
            png(),
            "the first is untouched"
        );
    }

    #[test]
    fn a_picture_that_is_not_a_png_a_bad_name_and_a_missing_folder_are_refused_in_words() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            write_png(dir.path(), "shot.png", b"GIF89a").unwrap_err(),
            "the clipboard's picture is not a PNG"
        );
        assert_eq!(
            write_png(dir.path(), "  ", &png()).unwrap_err(),
            "A name is needed."
        );
        assert_eq!(
            write_png(dir.path(), "a/b.png", &png()).unwrap_err(),
            "A name has no slash in it."
        );
        assert_eq!(
            write_png(dir.path(), "..", &png()).unwrap_err(),
            "That is not a name."
        );
        assert_eq!(
            write_png(dir.path(), ".hidden.png", &png()).unwrap_err(),
            "That is not a name.",
            "a dotfile is not a picture's name"
        );
        assert_eq!(
            write_png(dir.path(), "a\u{7}b.png", &png()).unwrap_err(),
            "A name has no control character in it."
        );
        assert!(write_png(&dir.path().join("gone"), "shot.png", &png())
            .unwrap_err()
            .ends_with("is not a folder"));
        assert_eq!(
            names(dir.path()),
            Vec::<String>::new(),
            "nothing was written"
        );
    }
}
