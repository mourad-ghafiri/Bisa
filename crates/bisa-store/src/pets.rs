//! Pets: an animated companion, in Codex's package format, exactly.
//!
//! The record and its validation are [`bisa_core::pet`]'s; this module
//! copies packages in and out — and answers first for the pets the platform
//! **ships** (`catalog::CATALOG_PETS`): parsed once from the binary, stamped
//! `PetOrigin::Catalog`, listed before a person's own, their sheets served
//! from the bundle. A built-in has no directory: nothing installs it,
//! nothing removes it, and a package a person installs may not take its id.
//! Local packages are stored **exactly as they arrived**. Local, no kind, no
//! index table — the bundle and a directory listing are the whole answer.

use crate::catalog::CATALOG;
use crate::error::StoreError;
use crate::workspace::Workspace;
use bisa_core::{is_webp, validate_pet_id, Pet, PetOrigin, MAX_SPRITESHEET_BYTES};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// The built-in pets, parsed once and stamped as shipped. A manifest that
/// will not parse is a build bug the bundle tests catch; here it is skipped
/// with a warning rather than taking every pet down.
pub fn builtin_pets() -> &'static [Pet] {
    static PETS: LazyLock<Vec<Pet>> = LazyLock::new(|| {
        CATALOG
            .pets
            .iter()
            .filter_map(|b| match serde_json::from_str::<Pet>(b.manifest) {
                Ok(mut pet) => {
                    pet.origin = PetOrigin::Catalog;
                    Some(pet)
                }
                Err(e) => {
                    tracing::warn!("built-in pet {:?} will not parse: {e}", b.slug);
                    None
                }
            })
            .collect()
    });
    &PETS
}

/// A built-in's sheet, by the pet's id.
fn builtin_sheet(id: &str) -> Option<&'static [u8]> {
    builtin_pets()
        .iter()
        .position(|p| p.id == id)
        .and_then(|at| CATALOG.pets.get(at))
        .map(|b| b.sheet)
}

fn builtin(id: &str) -> Option<&'static Pet> {
    builtin_pets().iter().find(|p| p.id == id)
}

/// Where a pet's sprite sheet is: inside the binary for a built-in, on disk
/// inside the package for a local pet — resolved inside it, never outside.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PetSprite {
    Bundled(&'static [u8]),
    File(PathBuf),
}

impl Workspace {
    /// Read one pet's manifest: a built-in's, else the installed package's.
    pub fn get_pet(&self, id: &str) -> Result<Pet, StoreError> {
        if let Some(pet) = builtin(id) {
            return Ok(pet.clone());
        }
        let path = self.paths.pet_manifest(id)?;
        let bytes = std::fs::read(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StoreError::DefinitionNotFound {
                    kind: "pet",
                    id: id.to_string(),
                }
            } else {
                StoreError::io(path.display().to_string(), e)
            }
        })?;
        serde_json::from_slice(&bytes).map_err(|e| StoreError::unreadable(&path, "pet", e))
    }

    /// Every pet: the built-ins in the bundle's order, then the installed
    /// ones by id. A package that will not parse is warned past rather than
    /// fatal.
    pub fn list_pets(&self) -> Result<Vec<Pet>, StoreError> {
        let mut out: Vec<Pet> = builtin_pets().to_vec();
        let dir = self.paths.pets_dir();
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Ok(out);
        };
        let mut local = Vec::new();
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let name = entry.file_name();
            let Some(id) = name.to_str() else { continue };
            if builtin(id).is_some() {
                tracing::warn!(
                    "skipping pet {id:?}: a built-in of that id ships with the platform"
                );
                continue;
            }
            match self.get_pet(id) {
                Ok(def) => local.push(def),
                Err(e) => tracing::warn!("skipping pet {id:?}: {e}"),
            }
        }
        local.sort_by(|a, b| a.id.cmp(&b.id));
        out.extend(local);
        Ok(out)
    }

    /// A pet's sprite sheet: the bundle's bytes for a built-in, else the
    /// package's file, resolved inside the package and never outside.
    pub fn pet_sprite(&self, id: &str) -> Result<PetSprite, StoreError> {
        if let Some(sheet) = builtin_sheet(id) {
            return Ok(PetSprite::Bundled(sheet));
        }
        let def = self.get_pet(id)?;
        let path = crate::paths::resolve_within(&self.paths.pet_dir(id)?, &def.spritesheet_path)?;
        Ok(PetSprite::File(path))
    }

    /// Copy a pet package into the workspace. Everything that can be wrong is
    /// checked **before** anything is written.
    pub fn install_pet(&self, from: &Path) -> Result<Pet, StoreError> {
        let manifest_path = from.join("pet.json");
        let bytes = std::fs::read(&manifest_path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-holds-no-pet-json-pet-package-manifest",
                    a0 = (from.display()).to_string()
                ))
            } else {
                StoreError::io(manifest_path.display().to_string(), e)
            }
        })?;
        let def: Pet = serde_json::from_slice(&bytes).map_err(|e| {
            StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-pet-json-will-not-parse",
                e = e.to_string()
            ))
        })?;
        validate_pet_id(&def.id)?;
        if def.display_name.trim().is_empty() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-pet-needs-displayname"
            )));
        }
        def.validate_animations().map_err(|e| {
            StoreError::Invalid(bisa_core::text!(
                "error-store-pets-refused",
                detail = e.to_string()
            ))
        })?;
        if builtin(&def.id).is_some() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-pet-ships-with-platform-give-your-own",
                a0 = format!("{:?}", def.id)
            )));
        }
        let dest = self.paths.pet_dir(&def.id)?;
        if dest.exists() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-pet-already-installed",
                a0 = format!("{:?}", def.id)
            )));
        }
        let sheet = crate::paths::resolve_within(from, &def.spritesheet_path).map_err(|e| {
            StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-spritesheetpath",
                a0 = format!("{:?}", def.spritesheet_path),
                e = e.to_string()
            ))
        })?;
        let meta = std::fs::metadata(&sheet).map_err(|_| {
            StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-no-sprite-sheet",
                a0 = format!("{:?}", def.spritesheet_path)
            ))
        })?;
        if meta.len() > MAX_SPRITESHEET_BYTES {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-sprite-sheet-bytes-limit",
                a0 = (meta.len()).to_string(),
                max_spritesheet_bytes = (MAX_SPRITESHEET_BYTES).to_string()
            )));
        }
        let sheet_bytes =
            std::fs::read(&sheet).map_err(|e| StoreError::io(sheet.display().to_string(), e))?;
        if !is_webp(&sheet_bytes) {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-not-webp-file-pet-s-sprite-sheet",
                a0 = format!("{:?}", def.spritesheet_path)
            )));
        }
        let dest_sheet = crate::paths::resolve_within_new(&dest, &def.spritesheet_path)?;
        crate::paths::write_atomic(&dest.join("pet.json"), &bytes)?;
        crate::paths::write_atomic(&dest_sheet, &sheet_bytes)?;
        Ok(def)
    }

    /// Remove a pet and everything in its package — a directory this store
    /// created, named by an id it validated. A built-in has no package and
    /// is refused by name.
    pub fn remove_pet(&self, id: &str) -> Result<(), StoreError> {
        if builtin(id).is_some() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-pet-ships-with-platform-not-removed-put",
                id = format!("{id:?}")
            )));
        }
        self.get_pet(id)?;
        let dir = self.paths.pet_dir(id)?;
        std::fs::remove_dir_all(&dir).map_err(|e| StoreError::io(dir.display().to_string(), e))
    }
}
