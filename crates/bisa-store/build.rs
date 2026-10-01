//! Embed the built-in addons.
//!
//! Every folder under `library/addons/<slug>/` is one addon the catalog
//! ships: its `addon.json` and every other file, walked here at compile
//! time and written to `OUT_DIR/builtin_addons.rs` as `include_bytes!`
//! entries, so the binary carries the bundles the way it carries the pets'
//! sheets — and a folder added or dropped is a rebuild, never a hand-kept
//! list. Dotfiles are left out; symlinks are followed by `include_bytes!`
//! only where they point inside the folder, which the bundle test holds.
//!
//! No dependency: `std::fs` walks, `{:?}` escapes the paths.

use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let root = manifest_dir.join("../../library/addons");
    let root = root.canonicalize().unwrap_or(root);
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("builtin_addons.rs");
    println!("cargo:rerun-if-changed={}", root.display());

    let mut slugs: Vec<(String, PathBuf)> = match fs::read_dir(&root) {
        Ok(entries) => entries
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                (!name.starts_with('.')).then(|| (name, e.path()))
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    slugs.sort();

    let mut code = String::new();
    code.push_str(
        "/// The built-in addons, one per folder under `library/addons/`, in slug order.\n",
    );
    code.push_str("pub const BUILTIN_ADDONS: &[BuiltinAddon] = &[\n");
    let mut entries = String::new();
    for (slug, dir) in &slugs {
        println!("cargo:rerun-if-changed={}", dir.display());
        let manifest = dir.join("addon.json");
        println!("cargo:rerun-if-changed={}", manifest.display());
        let mut files = Vec::new();
        walk(dir, dir, &mut files);
        files.sort();
        code.push_str(&format!(
            "    BuiltinAddon {{\n        slug: {slug:?},\n        manifest: include_str!({:?}),\n        files: &[\n",
            manifest.display().to_string()
        ));
        for (rel, abs) in &files {
            println!("cargo:rerun-if-changed={}", abs.display());
            code.push_str(&format!(
                "            ({rel:?}, include_bytes!({:?})),\n",
                abs.display().to_string()
            ));
        }
        code.push_str("        ],\n    },\n");
        entries.push_str(&format!(
            "    ({slug:?}, include_str!({:?})),\n",
            manifest.display().to_string()
        ));
    }
    code.push_str("];\n\n");
    code.push_str("/// The same addons as `(slug, manifest json)`, the pair every other catalog kind is listed by.\n");
    code.push_str(&format!(
        "pub const BUILTIN_ADDON_ENTRIES: &[(&str, &str)] = &[\n{entries}];\n"
    ));
    fs::write(&out, code).expect("write builtin_addons.rs");
}

/// Every regular file under `dir`, as `(path relative to root, absolute path)`,
/// `/`-joined; dotfiles and the manifest itself are left out.
fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            walk(root, &path, out);
            continue;
        }
        if dir == root && name == "addon.json" {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .expect("under the root")
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        out.push((rel, path));
    }
}
