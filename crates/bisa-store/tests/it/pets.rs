//! Pets: Codex's package format, stored exactly as it arrived.
//!
//! Fixtures live in `tempfile` directories released by `Drop`; nothing here
//! deletes anything itself.

use bisa_core::{Pet, PetOrigin};
use bisa_store::{MemoryKeyStore, PetSprite, Workspace, CATALOG};
use std::path::Path;

/// The pets a person installed — the nine built-ins are always listed first.
fn local(ws: &Workspace) -> Vec<Pet> {
    ws.list_pets()
        .unwrap()
        .into_iter()
        .filter(|p| p.origin == PetOrigin::Local)
        .collect()
}

fn is_file(sprite: PetSprite) -> bool {
    matches!(sprite, PetSprite::File(p) if p.is_file())
}

fn ws() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    (dir, ws)
}

fn webp_bytes() -> Vec<u8> {
    let mut v = b"RIFF".to_vec();
    v.extend_from_slice(&[0, 0, 0, 0]);
    v.extend_from_slice(b"WEBPVP8 ");
    v.extend_from_slice(&[0u8; 64]);
    v
}

fn manifest(id: &str, sheet: &str) -> String {
    format!(
        r#"{{"id":"{id}","displayName":"Byte","description":"a pet","spritesheetPath":"{sheet}"}}"#
    )
}

/// A package directory inside `root`, with the sheet at `sheet_at` (or none).
fn package(root: &Path, name: &str, manifest: &str, sheet_at: Option<&str>) -> std::path::PathBuf {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("pet.json"), manifest).unwrap();
    if let Some(rel) = sheet_at {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, webp_bytes()).unwrap();
    }
    dir
}

#[test]
fn a_well_formed_package_installs_and_keeps_its_own_field_names() {
    let (_dir, ws) = ws();
    let src = tempfile::tempdir().unwrap();
    let pkg = package(
        src.path(),
        "byte",
        &manifest("byte", "spritesheet.webp"),
        Some("spritesheet.webp"),
    );
    let pet = ws.install_pet(&pkg).unwrap();
    assert_eq!(pet.id, "byte");
    let installed = ws.paths().pet_dir("byte").unwrap();
    let written = std::fs::read_to_string(installed.join("pet.json")).unwrap();
    assert!(written.contains("displayName") && written.contains("spritesheetPath"));
    assert!(installed.join("spritesheet.webp").is_file());
    assert_eq!(local(&ws), vec![pet.clone()]);
    assert!(is_file(ws.pet_sprite("byte").unwrap()));
    let p: Pet = ws.get_pet("byte").unwrap();
    assert_eq!(p.display_name, "Byte");
    assert_eq!(p.origin, PetOrigin::Local);
}

#[test]
fn the_built_ins_are_listed_first_drawn_from_the_bundle_and_never_removed() {
    let (_dir, ws) = ws();
    let listed = ws.list_pets().unwrap();
    assert_eq!(
        listed.len(),
        CATALOG.pets.len(),
        "nine ship, none installed"
    );
    assert!(listed.iter().all(|p| p.origin == PetOrigin::Catalog));
    let moonrice = "bisa-pets.midnight-shipping.moonrice";
    let pet = ws.get_pet(moonrice).unwrap();
    assert_eq!(pet.display_name, "Moonrice");
    assert!(
        pet.animations.contains_key("idle"),
        "the pack's timing comes along"
    );
    match ws.pet_sprite(moonrice).unwrap() {
        PetSprite::Bundled(bytes) => assert!(bisa_core::is_webp(bytes)),
        PetSprite::File(p) => panic!("a built-in has no file: {}", p.display()),
    }
    let err = ws.remove_pet(moonrice).unwrap_err().to_string();
    assert!(err.contains("ships with the platform"), "{err}");
    assert_eq!(ws.list_pets().unwrap().len(), CATALOG.pets.len());
    // A person's pack may not take a built-in's id.
    let src = tempfile::tempdir().unwrap();
    let pkg = package(
        src.path(),
        "moon",
        &manifest(moonrice, "s.webp"),
        Some("s.webp"),
    );
    let err = ws.install_pet(&pkg).unwrap_err().to_string();
    assert!(err.contains("ships with the platform"), "{err}");
    assert!(local(&ws).is_empty());
    // Their own installs and lists after the nine.
    let mine = package(
        src.path(),
        "mine",
        &manifest("bisa-pets.mine.byte", "s.webp"),
        Some("s.webp"),
    );
    ws.install_pet(&mine).unwrap();
    let all = ws.list_pets().unwrap();
    assert_eq!(all.len(), CATALOG.pets.len() + 1);
    assert_eq!(all.last().unwrap().id, "bisa-pets.mine.byte");
    assert_eq!(all.last().unwrap().origin, PetOrigin::Local);
}

#[test]
fn a_packs_animations_are_checked_before_anything_is_written() {
    let (_dir, ws) = ws();
    let src = tempfile::tempdir().unwrap();
    let m = r#"{"id":"timed","displayName":"Timed","spritesheetPath":"s.webp","animations":{"idle":{"row":0,"frames":2,"frameDurationsMs":[100]}}}"#;
    let pkg = package(src.path(), "timed", m, Some("s.webp"));
    let err = ws.install_pet(&pkg).unwrap_err().to_string();
    assert!(err.contains("1 durations for 2 frames"), "{err}");
    assert!(local(&ws).is_empty());
    assert!(!ws.paths().pet_dir("timed").unwrap().exists());
}

#[test]
fn a_refused_package_leaves_nothing_behind() {
    let (_dir, ws) = ws();
    let src = tempfile::tempdir().unwrap();
    let cases = [
        (
            "noheet",
            manifest("nosheet", "spritesheet.webp"),
            None,
            "no sprite sheet",
        ),
        (
            "escape",
            manifest("escape", "../../etc/passwd"),
            None,
            "spritesheetPath",
        ),
        ("badid", manifest("Bad Id", "s.webp"), Some("s.webp"), "pet"),
        (
            "noname",
            r#"{"id":"noname","displayName":"  ","spritesheetPath":"s.webp"}"#.to_string(),
            Some("s.webp"),
            "displayName",
        ),
    ];
    for (name, m, sheet, needle) in cases {
        let pkg = package(src.path(), name, &m, sheet);
        let err = ws.install_pet(&pkg).expect_err(name).to_string();
        assert!(err.contains(needle), "{name}: {err}");
    }
    let not_webp = package(src.path(), "png", &manifest("png", "s.webp"), None);
    std::fs::write(not_webp.join("s.webp"), b"\x89PNG....").unwrap();
    assert!(ws
        .install_pet(&not_webp)
        .unwrap_err()
        .to_string()
        .contains("WebP"));
    assert!(local(&ws).is_empty());
    assert!(std::fs::read_dir(ws.paths().pets_dir())
        .map(|d| d.count() == 0)
        .unwrap_or(true));
}

#[test]
fn a_namespaced_id_installs_and_the_same_pet_twice_is_refused() {
    let (_dir, ws) = ws();
    let src = tempfile::tempdir().unwrap();
    let id = "bisa-pets.my-pack.moon";
    let pkg = package(
        src.path(),
        "moon",
        &manifest(id, "sheet.webp"),
        Some("sheet.webp"),
    );
    ws.install_pet(&pkg).unwrap();
    assert_eq!(ws.get_pet(id).unwrap().id, id);
    let err = ws.install_pet(&pkg).unwrap_err().to_string();
    assert!(err.contains("already installed"), "{err}");
    assert_eq!(local(&ws).len(), 1);
}

#[test]
fn a_package_with_no_manifest_says_what_a_package_is() {
    let (_dir, ws) = ws();
    let src = tempfile::tempdir().unwrap();
    let err = ws.install_pet(src.path()).unwrap_err().to_string();
    assert!(err.contains("pet.json"), "{err}");
}

#[test]
fn a_sheet_in_a_subdirectory_keeps_its_relative_path() {
    let (_dir, ws) = ws();
    let src = tempfile::tempdir().unwrap();
    let pkg = package(
        src.path(),
        "nested",
        &manifest("nested", "art/sheet.webp"),
        Some("art/sheet.webp"),
    );
    ws.install_pet(&pkg).unwrap();
    assert!(ws
        .paths()
        .pet_dir("nested")
        .unwrap()
        .join("art")
        .join("sheet.webp")
        .is_file());
    assert!(is_file(ws.pet_sprite("nested").unwrap()));
}

#[test]
fn an_unreadable_package_costs_that_pet_and_not_the_list() {
    let (_dir, ws) = ws();
    let src = tempfile::tempdir().unwrap();
    let pkg = package(
        src.path(),
        "good",
        &manifest("good", "s.webp"),
        Some("s.webp"),
    );
    ws.install_pet(&pkg).unwrap();
    let bad = ws.paths().pets_dir().join("broken");
    std::fs::create_dir_all(&bad).unwrap();
    std::fs::write(bad.join("pet.json"), b"not json").unwrap();
    let listed = local(&ws);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, "good");
}

#[test]
fn removing_a_pet_takes_its_whole_package_and_a_dangerous_id_is_refused() {
    let (_dir, ws) = ws();
    let src = tempfile::tempdir().unwrap();
    let pkg = package(
        src.path(),
        "gone",
        &manifest("gone", "s.webp"),
        Some("s.webp"),
    );
    ws.install_pet(&pkg).unwrap();
    let installed = ws.paths().pet_dir("gone").unwrap();
    assert!(installed.exists());
    ws.remove_pet("gone").unwrap();
    assert!(!installed.exists());
    assert!(local(&ws).is_empty());
    for bad in ["..", "../x", "a/b", ".hidden", "Upper"] {
        assert!(ws.get_pet(bad).is_err(), "{bad}");
        assert!(ws.remove_pet(bad).is_err(), "{bad}");
    }
}
