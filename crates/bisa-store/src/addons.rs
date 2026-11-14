//! Addons: the overlay widgets a workspace installed (kind 33407).
//!
//! An addon arrives as a folder — `addon.json` and the files its page
//! needs — and is kept **as a record and a bundle**: the record
//! (`addons/<id>/addon.json`, its snapshot in `addons/state/33407-<id>.json`)
//! is the manifest as installed, where it came from, whether it runs and
//! what the person granted; the bundle (`addons/<id>/files/`) is the copy
//! the node serves. The record is workspace truth and travels like a
//! skill's; the bundle is this machine's, so a record that arrived from a
//! peer lists with `files_present: false` and cannot be enabled until the
//! folder is brought here too.
//!
//! **Everything that can be wrong is checked before anything is written**
//! ([`bisa_core::addon`] spells the rules; this module applies them to a
//! folder): the manifest's problems, then the walk — no symlink, no
//! dotfile, no name that is not UTF-8, no folder deeper than
//! `MAX_BUNDLE_DEPTH` — then the listing's rules, then the copy into a
//! staging folder renamed into place in one move.
//!
//! The built-ins ship in the binary (`catalog::CATALOG.addon_bundles`, from
//! `library/addons/`) and are installed on demand like every catalog entry;
//! a person's own addon may not take a built-in's id.
//!
//! **One writer at a time** (`Workspace::addons_writes`): a switch, a grant,
//! an install, a removal, the refresh at open and a peer's record arriving
//! each read, judge and write under it, so two at once never each write the
//! record as it was before the other, and a snapshot is never refused as
//! stale behind a record file already rewritten. A patch is judged whole
//! before anything is written and written once (`update_addon`). A record
//! this machine holds **wins** over a peer's: its switch and its grants are
//! the person's decisions here.

use crate::catalog::{BuiltinAddon, CATALOG};
use crate::error::StoreError;
use crate::paths::Paths;
use crate::workspace::{now_secs, EventAudience, StoreEvent, Workspace};
use bisa_core::addon::{
    check_bundle_listing, is_bundle_path, AddonError, AddonManifest, AddonPermission, AddonProblem,
    AddonRecord, MANIFEST_FILE_NAME, MAX_BUNDLE_DEPTH, MAX_MANIFEST_BYTES,
};
use bisa_core::kind::KIND_ADDON;
use bisa_core::{AddonId, Localize as _, Origin, Text};
use std::path::{Path, PathBuf};
use std::sync::MutexGuard;

/// What one patch of a record changed, in the order it is said on the bus:
/// the switch first, then the grants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddonChange {
    Enabled,
    Disabled,
    Grants,
}

/// One installed addon as the store knows it: the record, and whether its
/// bundle is on this machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddonEntry {
    pub record: AddonRecord,
    /// The bundle's entry page is here. False for a record a peer wrote.
    pub files_present: bool,
}

impl AddonEntry {
    pub fn id(&self) -> &AddonId {
        &self.record.manifest.id
    }

    /// Enabled, with its files here — the one state in which a window opens
    /// and a file is served.
    pub fn is_active(&self) -> bool {
        self.record.enabled && self.files_present
    }
}

/// Every file of a bundle, read: its `/`-joined path and its bytes.
type BundleFiles = Vec<(String, Vec<u8>)>;

/// One built-in the catalog offers: its slug, its manifest as shipped, and
/// whether it is installed here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddonOffer {
    pub slug: String,
    pub manifest: AddonManifest,
    pub installed: bool,
}

/// The file macOS's Finder writes into a folder it has shown.
const FINDER_FILE_NAME: &str = ".DS_Store";
/// How old a staging folder must be before an open clears it: long past any
/// install another process of this workspace could still be assembling.
const STALE_STAGING: std::time::Duration = std::time::Duration::from_secs(600);

/// The one id no addon may take: `addons/state/` is the snapshots'.
const RESERVED_ADDON_IDS: &[&str] = &["state"];

fn invalid(text: Text) -> StoreError {
    StoreError::Invalid(text)
}

/// Read and parse a folder's manifest. A missing or unreadable manifest is
/// the refusal a folder that is not an addon gets.
fn read_manifest(from: &Path) -> Result<(AddonManifest, Vec<u8>), StoreError> {
    let path = from.join(MANIFEST_FILE_NAME);
    let meta = std::fs::symlink_metadata(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            invalid(bisa_core::text!(
                "error-store-invalid-addon-holds-no-addon-json",
                a0 = from.display().to_string()
            ))
        } else {
            StoreError::io(path.display().to_string(), e)
        }
    })?;
    if !meta.is_file() {
        return Err(invalid(bisa_core::text!(
            "error-store-invalid-addon-holds-no-addon-json",
            a0 = from.display().to_string()
        )));
    }
    if meta.len() > MAX_MANIFEST_BYTES {
        return Err(invalid(bisa_core::text!(
            "error-store-invalid-addon-manifest-too-large",
            a0 = meta.len().to_string(),
            max = MAX_MANIFEST_BYTES.to_string()
        )));
    }
    let bytes = std::fs::read(&path).map_err(|e| StoreError::io(path.display().to_string(), e))?;
    let manifest: AddonManifest = serde_json::from_slice(&bytes).map_err(|e| {
        invalid(bisa_core::text!(
            "error-store-invalid-addon-json-will-not-parse",
            e = e.to_string()
        ))
    })?;
    Ok((manifest, bytes))
}

/// Parse a built-in's manifest; the bundle test holds every one, so a
/// failure here is a build bug named by slug.
pub(crate) fn parse_addon(slug: &str, json: &str) -> Result<AddonManifest, StoreError> {
    let manifest: AddonManifest = serde_json::from_str(json).map_err(|e| {
        invalid(bisa_core::text!(
            "error-store-invalid-catalog-addon",
            slug = slug.to_string(),
            e = e.to_string()
        ))
    })?;
    if manifest.id.as_str() != slug {
        return Err(invalid(bisa_core::text!(
            "error-store-invalid-catalog-addon-id-not-slug",
            slug = slug.to_string(),
            id = manifest.id.to_string()
        )));
    }
    Ok(manifest)
}

/// Walk a folder the way the copy will: every regular file under it as
/// `(path, bytes)`, the manifest left out, and a refusal — as the sentence
/// a person reads — for a symlink, a dotfile, a name that is not UTF-8,
/// anything that is neither a file nor a folder, or a folder too deep.
fn walk_bundle(root: &Path) -> Result<Vec<(String, u64, PathBuf)>, Text> {
    let mut out = Vec::new();
    walk_into(root, root, 0, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk_into(
    root: &Path,
    dir: &Path,
    depth: usize,
    out: &mut Vec<(String, u64, PathBuf)>,
) -> Result<(), Text> {
    let entries = std::fs::read_dir(dir).map_err(|e| {
        bisa_core::text!(
            "error-store-invalid-addon-folder-unreadable",
            path = dir.display().to_string(),
            e = e.to_string()
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| {
            // LCOV_EXCL_START: a folder entry that fails to read after the folder itself was listed: the disk's fault between the two
            bisa_core::text!(
                "error-store-invalid-addon-folder-unreadable",
                path = dir.display().to_string(),
                e = e.to_string()
            )
            // LCOV_EXCL_STOP
        })?;
        let path = entry.path();
        let shown = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .display()
            .to_string();
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            // LCOV_EXCL_START: a name that is not UTF-8 cannot be made on APFS (EILSEQ); the arm is held on Linux by a_manifest_that_is_a_folder_or_too_large_a_name_that_is_not_text_and_a_folder_too_deep_are_refused
            return Err(bisa_core::text!(
                "error-store-invalid-addon-name-not-utf8",
                path = shown
            ));
            // LCOV_EXCL_STOP
        };
        // What Finder leaves in every folder it showed is no part of a bundle and no reason to
        // refuse one: passed over, as the build passes over it for a built-in.
        if name == FINDER_FILE_NAME {
            continue;
        }
        if name.starts_with('.') {
            return Err(bisa_core::text!(
                "error-store-invalid-addon-dotfile",
                path = shown
            ));
        }
        // LCOV_EXCL_START: an entry the folder just listed has a metadata to read; a removal racing the walk is the disk's
        let meta = std::fs::symlink_metadata(&path).map_err(|e| {
            bisa_core::text!(
                "error-store-invalid-addon-folder-unreadable",
                path = shown.clone(),
                e = e.to_string()
            )
        })?;
        // LCOV_EXCL_STOP
        if meta.file_type().is_symlink() {
            return Err(bisa_core::text!(
                "error-store-invalid-addon-symlink",
                path = shown
            ));
        }
        if meta.is_dir() {
            if depth + 1 > MAX_BUNDLE_DEPTH {
                return Err(bisa_core::text!(
                    "error-store-invalid-addon-too-deep",
                    path = shown,
                    max = MAX_BUNDLE_DEPTH.to_string()
                ));
            }
            walk_into(root, &path, depth + 1, out)?;
            continue;
        }
        // LCOV_EXCL_START: neither a folder, a file nor a link is a device or a pipe, which no bundle a person makes holds
        if !meta.is_file() {
            return Err(bisa_core::text!(
                "error-store-invalid-addon-not-a-file",
                path = shown
            ));
        }
        // LCOV_EXCL_STOP
        if depth == 0 && name == MANIFEST_FILE_NAME {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map_err(|_| {
                // LCOV_EXCL_START: the path was joined under `root` by this walk, so `strip_prefix` cannot fail
                bisa_core::text!("error-store-invalid-addon-not-a-file", path = shown.clone())
            })?
            // LCOV_EXCL_STOP
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        out.push((rel, meta.len(), path));
    }
    Ok(())
}

/// The folder's files, judged by the listing rules and then read.
fn read_bundle(from: &Path, manifest: &AddonManifest) -> Result<BundleFiles, StoreError> {
    let listed = walk_bundle(from).map_err(invalid)?;
    let listing: Vec<(String, u64)> = listed.iter().map(|(p, b, _)| (p.clone(), *b)).collect();
    check_bundle_listing(&listing, &manifest.entry)?;
    let mut files = Vec::with_capacity(listed.len());
    for (rel, listed_bytes, path) in listed {
        let bytes =
            std::fs::read(&path).map_err(|e| StoreError::io(path.display().to_string(), e))?;
        // The caps were judged on the listing: a file that is no longer what was listed — grown,
        // or swapped for a link to something else — is not the file that was judged.
        // LCOV_EXCL_START: a file changed between the walk and the read is a race with another program on the folder
        if bytes.len() as u64 != listed_bytes {
            return Err(invalid(bisa_core::text!(
                "error-store-invalid-addon-folder-unreadable",
                path = path.display().to_string(),
                e = "it changed while it was being read".to_string()
            )));
        }
        // LCOV_EXCL_STOP
        files.push((rel, bytes));
    }
    Ok(files)
}

/// A built-in's bundle as the binary ships it, checked as a folder's would
/// be — the listing's rules and the entry page present.
fn shipped_files(
    bundle: &BuiltinAddon,
    manifest: &AddonManifest,
) -> Result<BundleFiles, StoreError> {
    let files: BundleFiles = bundle
        .files
        .iter()
        .map(|(p, b)| (p.to_string(), b.to_vec()))
        .collect();
    let listing: Vec<(String, u64)> = files
        .iter()
        .map(|(p, b)| (p.clone(), b.len() as u64))
        .collect();
    check_bundle_listing(&listing, &manifest.entry)?;
    Ok(files)
}

/// Clear a staging folder this process made — under a name only this
/// process knows — after a failure part way; its absence is not a failure.
fn clear_staging(staging: &Path) {
    if let Err(clean) = std::fs::remove_dir_all(staging) {
        // LCOV_EXCL_START: a rename or removal inside a folder this process just wrote is refused by the disk alone
        if clean.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(
                target: "bisa_store::addons",
                "the staging folder {} could not be cleared: {clean}",
                staging.display()
            );
        }
        // LCOV_EXCL_STOP
    }
}

fn joined_problems(problems: &[AddonProblem]) -> String {
    problems
        .iter()
        .map(|p| match &p.field {
            Some(f) => format!("{f}: {}", p.text),
            None => p.text.to_string(), // LCOV_EXCL_LINE: every problem the core makes names its field (`AddonProblem::at`)
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Grants as they are kept: in the order the manifest declares them, each
/// once. The order a caller said them in is no fact — the same grants said in
/// another order are the same grants, and no change to write or announce.
fn as_declared(granted: &[AddonPermission], declared: &[AddonPermission]) -> Vec<AddonPermission> {
    declared
        .iter()
        .filter(|d| granted.contains(d))
        .cloned()
        .collect()
}

/// The first grant that was never declared, as its word.
fn undeclared(granted: &[AddonPermission], declared: &[AddonPermission]) -> Option<String> {
    granted
        .iter()
        .find(|g| !declared.contains(g))
        .map(|g| g.word().to_string())
}

impl Workspace {
    /// The built-ins the catalog offers, each with its manifest and whether it
    /// is installed here — what the review reads before an install.
    pub fn list_addon_offers(&self) -> Result<Vec<AddonOffer>, StoreError> {
        CATALOG
            .addon_bundles
            .iter()
            .map(|b| {
                let manifest = parse_addon(b.slug, b.manifest)?;
                let installed = self.get_addon(&manifest.id).is_ok();
                Ok(AddonOffer {
                    slug: b.slug.to_string(),
                    manifest,
                    installed,
                })
            })
            .collect()
    }

    /// What is wrong with a folder as an addon, without writing anything:
    /// the manifest as read, and its problems — the manifest's own rules,
    /// and the bundle's rules as problems under `files`. A folder with no
    /// readable manifest is the error itself.
    pub fn validate_addon_dir(
        &self,
        from: &Path,
    ) -> Result<(AddonManifest, Vec<AddonProblem>), StoreError> {
        let (manifest, _) = read_manifest(from)?;
        let mut problems = manifest.validate();
        match walk_bundle(from) {
            Err(text) => problems.push(AddonProblem::at("files", text)),
            Ok(listed) => {
                let listing: Vec<(String, u64)> =
                    listed.iter().map(|(p, b, _)| (p.clone(), *b)).collect();
                if let Err(e) = check_bundle_listing(&listing, &manifest.entry) {
                    problems.push(AddonProblem::at("files", e.text()));
                }
            }
        }
        if RESERVED_ADDON_IDS.contains(&manifest.id.as_str()) {
            problems.push(AddonProblem::at(
                "id",
                bisa_core::text!(
                    "error-store-invalid-addon-id-reserved",
                    id = manifest.id.to_string()
                ),
            ));
        }
        if CATALOG.addon_bundle(manifest.id.as_str()).is_some() {
            problems.push(AddonProblem::at(
                "id",
                bisa_core::text!(
                    "error-store-invalid-addon-ships-with-platform-give-your-own",
                    a0 = format!("{:?}", manifest.id.as_str())
                ),
            ));
        }
        Ok((manifest, problems))
    }

    /// Copy a person's addon folder into the workspace with the grants and
    /// the enabled flag they chose. Nothing is granted that was not declared.
    pub fn install_addon(
        &self,
        from: &Path,
        granted: Vec<AddonPermission>,
        enabled: bool,
    ) -> Result<AddonEntry, StoreError> {
        let (manifest, _) = read_manifest(from)?;
        let problems = manifest.validate();
        if !problems.is_empty() {
            return Err(invalid(bisa_core::text!(
                "error-store-invalid-addon-refused",
                problems = joined_problems(&problems)
            )));
        }
        if CATALOG.addon_bundle(manifest.id.as_str()).is_some() {
            return Err(invalid(bisa_core::text!(
                "error-store-invalid-addon-ships-with-platform-give-your-own",
                a0 = format!("{:?}", manifest.id.as_str())
            )));
        }
        let _writer = self.addon_writer();
        self.refuse_taken_addon_id(&manifest.id)?;
        if let Some(word) = undeclared(&granted, &manifest.permissions) {
            return Err(AddonError::GrantNotDeclared(word).into());
        }
        let files = read_bundle(from, &manifest)?;
        let granted = as_declared(&granted, &manifest.permissions);
        let record = AddonRecord {
            manifest,
            origin: Origin::Local,
            enabled,
            granted,
            installed_at: now_secs(),
        };
        self.place_addon(record, &files)
    }

    /// Materialise a built-in from the binary: enabled, granted what it
    /// declares — the platform wrote it, and the person read the list before
    /// choosing it.
    /// `None` when it was already here whole: the idempotent nothing.
    pub(crate) fn install_addon_from_catalog(
        &self,
        slug: &str,
    ) -> Result<Option<AddonEntry>, StoreError> {
        let bundle = CATALOG.addon_bundle(slug).ok_or_else(|| {
            invalid(bisa_core::text!(
                "error-store-invalid-catalog-has-no-called",
                kind = "addon".to_string(),
                slug = format!("{slug:?}")
            ))
        })?;
        let manifest = parse_addon(slug, bundle.manifest)?;
        let _writer = self.addon_writer();
        // Already here from the catalog — judged under the lock, so two installs of one slug at
        // once are an install and a nothing, never an install and a refusal. Its files absent
        // (a peer's record, a bundle folder lost): the binary's own are written and its switch
        // and grants kept.
        if let Ok(entry) = self.get_addon(&manifest.id) {
            if matches!(&entry.record.origin, Origin::Catalog { slug: s } if s == slug) {
                if !entry.files_present {
                    self.refresh_builtin(entry, bundle)?;
                    return self.get_addon(&manifest.id).map(Some);
                }
                return Ok(None);
            }
        }
        self.refuse_taken_addon_id(&manifest.id)?;
        let files = shipped_files(bundle, &manifest)?;
        let granted = manifest.permissions.clone();
        let record = AddonRecord {
            manifest,
            origin: Origin::Catalog {
                slug: slug.to_string(),
            },
            enabled: true,
            granted,
            installed_at: now_secs(),
        };
        self.place_addon(record, &files).map(Some)
    }

    fn refuse_taken_addon_id(&self, id: &AddonId) -> Result<(), StoreError> {
        if RESERVED_ADDON_IDS.contains(&id.as_str()) {
            return Err(invalid(bisa_core::text!(
                "error-store-invalid-addon-id-reserved",
                id = id.to_string()
            )));
        }
        if self.paths.addon_dir(id).exists() {
            return Err(invalid(bisa_core::text!(
                "error-store-invalid-addon-already-installed",
                a0 = format!("{:?}", id.as_str())
            )));
        }
        Ok(())
    }

    /// Write the bundle and the record into a staging folder under
    /// `addons/`, whole. A failure part way leaves the staging folder gone.
    fn stage_addon(
        &self,
        record: &AddonRecord,
        files: &BundleFiles,
    ) -> Result<PathBuf, StoreError> {
        let staging = self.paths.addon_staging_dir();
        let staged = (|| -> Result<(), StoreError> {
            let files_dir = staging.join(Paths::ADDON_FILES_DIR);
            std::fs::create_dir_all(&files_dir)
                .map_err(|e| StoreError::io(files_dir.display().to_string(), e))?;
            for (rel, bytes) in files {
                let path = crate::paths::resolve_within_new(&files_dir, rel)?;
                crate::paths::write_atomic(&path, bytes)?;
            }
            crate::paths::write_atomic(
                &staging.join(MANIFEST_FILE_NAME),
                &serde_json::to_vec_pretty(record)?,
            )
        })();
        // LCOV_EXCL_START: a rename or removal inside a folder this process just wrote is refused by the disk alone
        if let Err(e) = staged {
            clear_staging(&staging);
            return Err(e);
        }
        // LCOV_EXCL_STOP
        Ok(staging)
    }

    /// Stage the bundle and the record, then rename the folder into place
    /// in one move, then snapshot the record. A failure part way leaves the
    /// staging folder gone and nothing under the addon's id.
    fn place_addon(
        &self,
        record: AddonRecord,
        files: &BundleFiles,
    ) -> Result<AddonEntry, StoreError> {
        let dest = self.paths.addon_dir(&record.manifest.id);
        let staging = self.stage_addon(&record, files)?;
        // LCOV_EXCL_START: a rename or removal inside a folder this process just wrote is refused by the disk alone
        if let Err(e) = std::fs::rename(&staging, &dest) {
            clear_staging(&staging);
            return Err(StoreError::io(dest.display().to_string(), e));
        }
        // LCOV_EXCL_STOP
        self.snapshot_written(&record);
        Ok(AddonEntry {
            record,
            files_present: true,
        })
    }

    /// Stage the new bundle and record, retire the installed folder to a
    /// staging name, rename the new one into place, then clear the retired
    /// one and snapshot the record. Should the second rename fail, the
    /// retired folder comes back: the addon is never absent.
    fn replace_addon(&self, record: AddonRecord, files: &BundleFiles) -> Result<(), StoreError> {
        let dest = self.paths.addon_dir(&record.manifest.id);
        let staging = self.stage_addon(&record, files)?;
        let retired = self.paths.addon_staging_dir();
        // LCOV_EXCL_START: a rename or removal inside a folder this process just wrote is refused by the disk alone
        if let Err(e) = std::fs::rename(&dest, &retired) {
            clear_staging(&staging);
            return Err(StoreError::io(dest.display().to_string(), e));
        }
        // LCOV_EXCL_STOP
        if let Err(e) = std::fs::rename(&staging, &dest) {
            // LCOV_EXCL_START: the swap's second rename fails only on a disk that refused it after the first; the retired copy is put back or named
            if let Err(back) = std::fs::rename(&retired, &dest) {
                tracing::error!(
                    target: "bisa_store::addons",
                    "the addon {} could not be put back from {}: {back}",
                    record.manifest.id,
                    retired.display()
                );
            }
            clear_staging(&staging);
            return Err(StoreError::io(dest.display().to_string(), e));
            // LCOV_EXCL_STOP
        }
        clear_staging(&retired);
        self.snapshot_written(&record);
        Ok(())
    }

    /// The built-ins this binary outgrew: for every installed record from
    /// the catalog whose bundle this build still ships, when the shipped
    /// manifest's `version` differs from the record's, the bundle is written
    /// again and the record follows it — `enabled` and `installed_at` kept,
    /// `granted` kept but held to what the new manifest declares (a
    /// permission it no longer asks for is dropped; a new one is not
    /// granted: the person reads the list) — and the snapshot rewritten.
    /// Idempotent; runs at every open beside `ensure_core_agents`. One
    /// built-in that will not refresh is warned past, never a stop to the
    /// open. Returns the ids refreshed.
    pub(crate) fn refresh_builtin_addons(&self) -> Result<Vec<AddonId>, StoreError> {
        let _writer = self.addon_writer();
        let mut refreshed = Vec::new();
        for entry in self.list_addons()? {
            let Origin::Catalog { slug } = &entry.record.origin else {
                continue;
            };
            let Some(bundle) = CATALOG.addon_bundle(slug) else {
                continue;
            };
            // A built-in's id is its slug. A record that says it came from the catalog under
            // another id is not that built-in, and refreshing it would write the built-in's own
            // folder with this record's switch and grants.
            if entry.id().as_str() != slug {
                continue;
            }
            let id = entry.id().clone();
            match self.refresh_builtin(entry, bundle) {
                Ok(true) => refreshed.push(id),
                Ok(false) => {}
                // LCOV_EXCL_START: a built-in's refresh writes the files the catalog ships; a failure here is the disk's
                Err(e) => {
                    tracing::warn!(target: "bisa_store::addons", "the built-in {id} could not be refreshed: {e}")
                } // LCOV_EXCL_STOP
            }
        }
        Ok(refreshed)
    }

    fn refresh_builtin(
        &self,
        entry: AddonEntry,
        bundle: &BuiltinAddon,
    ) -> Result<bool, StoreError> {
        let manifest = parse_addon(bundle.slug, bundle.manifest)?;
        // At the shipped version with its files here: nothing to do. With its files absent — a
        // record a peer sent, a bundle folder lost — the binary's own are written, at any version.
        if manifest.version == entry.record.manifest.version && entry.files_present {
            return Ok(false);
        }
        let files = shipped_files(bundle, &manifest)?;
        let AddonRecord {
            origin,
            enabled,
            granted,
            installed_at,
            ..
        } = entry.record;
        let granted = granted
            .into_iter()
            .filter(|g| manifest.permissions.contains(g))
            .collect();
        let record = AddonRecord {
            manifest,
            origin,
            enabled,
            granted,
            installed_at,
        };
        self.replace_addon(record, &files)?;
        Ok(true)
    }

    fn write_record(&self, record: &AddonRecord) -> Result<(), StoreError> {
        crate::paths::write_atomic(
            &self.paths.addon_record(&record.manifest.id),
            &serde_json::to_vec_pretty(record)?,
        )
    }

    /// Snapshot a record that is **already on disk**. The record is the fact
    /// and the answer; a snapshot that could not be written is said in the log
    /// and never turns a write that landed into a failure — the caller would
    /// answer an error for a change the next read shows, and announce nothing.
    /// The next write of the record snapshots it again.
    fn snapshot_written(&self, record: &AddonRecord) {
        // LCOV_EXCL_START: the snapshot is written beside records this process just wrote; a failure here is the disk's
        if let Err(e) = self.write_addon_snapshot(record) {
            tracing::warn!(
                target: "bisa_store::addons",
                "the addon {} is written; its snapshot is not: {e}",
                record.manifest.id
            );
        }
        // LCOV_EXCL_STOP
    }

    fn write_addon_snapshot(&self, record: &AddonRecord) -> Result<(), StoreError> {
        let id = record.manifest.id.as_str();
        let existing_rev = self
            .snapshots
            .revision_for_write(Paths::NS_ADDONS, KIND_ADDON, id)?;
        let event = self.snapshots.put(
            Paths::NS_ADDONS,
            KIND_ADDON,
            id,
            record,
            existing_rev + 1,
            &self.owner,
            now_secs(),
            None,
            &record.manifest.tags,
        )?;
        self.emit_store_event(StoreEvent::ConversationSnapshot {
            kind: KIND_ADDON,
            d: id.to_string(),
            event,
            audience: EventAudience::Workspace,
        });
        Ok(())
    }

    /// One addon's record, and whether its bundle is here.
    pub fn get_addon(&self, id: &AddonId) -> Result<AddonEntry, StoreError> {
        let path = self.paths.addon_record(id);
        let bytes = std::fs::read(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StoreError::DefinitionNotFound {
                    kind: "addon",
                    id: id.to_string(),
                }
            } else {
                StoreError::io(path.display().to_string(), e)
            }
        })?;
        let record: AddonRecord = serde_json::from_slice(&bytes)
            .map_err(|e| StoreError::unreadable(&path, "addon", e))?;
        // A record is its folder's: one that names another id is not this addon's record, and a
        // write of it would land under the other id.
        if &record.manifest.id != id {
            return Err(StoreError::unreadable(
                &path,
                "addon",
                format!("it is the record of {}", record.manifest.id),
            ));
        }
        let files_present = self
            .paths
            .addon_files_dir(id)
            .join(&record.manifest.entry)
            .is_file();
        Ok(AddonEntry {
            record,
            files_present,
        })
    }

    /// Every addon, by id. A folder that is not an addon's — the snapshots'
    /// `state/`, a staging folder, a record that will not parse — is
    /// skipped, and the last two are warned past.
    pub fn list_addons(&self) -> Result<Vec<AddonEntry>, StoreError> {
        let dir = self.paths.addons_dir();
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(StoreError::io(dir.display().to_string(), e)),
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if RESERVED_ADDON_IDS.contains(&name.as_str()) {
                continue;
            }
            let Ok(id) = AddonId::new(name.as_str()) else {
                if !name.starts_with('.') {
                    tracing::warn!(target: "bisa_store::addons", "skipping addons/{name}: not an addon id");
                }
                continue;
            };
            match self.get_addon(&id) {
                Ok(addon) => out.push(addon),
                Err(e) => {
                    tracing::warn!(target: "bisa_store::addons", "skipping addon {id}: {e}")
                }
            }
        }
        out.sort_by(|a, b| a.id().cmp(b.id()));
        Ok(out)
    }

    /// The one writer of addon records at a time. A poisoned lock is taken
    /// over: the record on disk is whatever the last write left, and the
    /// next write reads it again.
    fn addon_writer(&self) -> MutexGuard<'_, ()> {
        self.addons_writes.lock().unwrap_or_else(|poisoned| {
            tracing::warn!(
                "the addon write lock was poisoned; continuing with the records as they stand"
            );
            poisoned.into_inner()
        })
    }

    /// One patch of a record — the switch, the grants, or both — **judged
    /// whole before anything is written** (on needs its files here; a grant
    /// is a subset of what was declared, exactly as declared, else refused by
    /// the first word that was not), written once, and answered with what
    /// changed in the order it is said on the bus. Nothing differs: nothing
    /// is written and the list is empty. The one implementation behind
    /// `set_addon_enabled` and `set_addon_grants`.
    pub fn update_addon(
        &self,
        id: &AddonId,
        enabled: Option<bool>,
        granted: Option<Vec<AddonPermission>>,
    ) -> Result<(AddonEntry, Vec<AddonChange>), StoreError> {
        let _writer = self.addon_writer();
        let mut addon = self.get_addon(id)?;
        if enabled == Some(true) && !addon.files_present {
            return Err(AddonError::FilesAbsent(id.to_string()).into());
        }
        if let Some(granted) = &granted {
            if let Some(word) = undeclared(granted, &addon.record.manifest.permissions) {
                return Err(AddonError::GrantNotDeclared(word).into());
            }
        }
        let mut changes = Vec::new();
        if let Some(enabled) = enabled {
            if addon.record.enabled != enabled {
                addon.record.enabled = enabled;
                changes.push(if enabled {
                    AddonChange::Enabled
                } else {
                    AddonChange::Disabled
                });
            }
        }
        if let Some(granted) = granted {
            let granted = as_declared(&granted, &addon.record.manifest.permissions);
            if addon.record.granted != granted {
                addon.record.granted = granted;
                changes.push(AddonChange::Grants);
            }
        }
        if changes.is_empty() {
            return Ok((addon, changes));
        }
        self.write_record(&addon.record)?;
        self.snapshot_written(&addon.record);
        Ok((addon, changes))
    }

    /// Turn an addon on or off. On needs its files here.
    pub fn set_addon_enabled(&self, id: &AddonId, enabled: bool) -> Result<AddonEntry, StoreError> {
        Ok(self.update_addon(id, Some(enabled), None)?.0)
    }

    /// Replace what the person granted — a subset of what was declared,
    /// exactly as declared, or refused by the first word that was not.
    pub fn set_addon_grants(
        &self,
        id: &AddonId,
        granted: Vec<AddonPermission>,
    ) -> Result<AddonEntry, StoreError> {
        Ok(self.update_addon(id, None, Some(granted))?.0)
    }

    /// Remove an addon: its record, its bundle, its snapshot — and say so on
    /// the store's stream.
    ///
    /// The **folder** is what is removed, whatever its record reads as: one
    /// cut short, or missing, still holds the id against every install, and a
    /// removal that needed a readable record left that id wedged for good.
    /// The folder leaves in one rename — to a staging name, cleared after —
    /// so a removal is never half done; once it has left, the addon is gone
    /// and the answer says so, whatever became of the snapshot file.
    pub fn remove_addon(&self, id: &AddonId) -> Result<(), StoreError> {
        let _writer = self.addon_writer();
        let dir = self.paths.addon_dir(id);
        if RESERVED_ADDON_IDS.contains(&id.as_str()) || !dir.is_dir() {
            return Err(StoreError::DefinitionNotFound {
                kind: "addon",
                id: id.to_string(),
            });
        }
        let retired = self.paths.addon_staging_dir();
        std::fs::rename(&dir, &retired).map_err(|e| StoreError::io(dir.display().to_string(), e))?;
        // LCOV_EXCL_START: the snapshot file is this process's own to remove; a refusal is the disk's
        if let Err(e) = self
            .snapshots
            .delete_snapshot(Paths::NS_ADDONS, KIND_ADDON, id.as_str())
        {
            tracing::warn!(target: "bisa_store::addons", "the addon {id} is removed; its snapshot is not: {e}");
        }
        // LCOV_EXCL_STOP
        clear_staging(&retired);
        self.emit_store_event(StoreEvent::DefinitionRemoved {
            kind: KIND_ADDON,
            d: id.to_string(),
        });
        Ok(())
    }

    /// Clear what a crash left under a staging name — an install part way, a
    /// retired folder never cleared. Only what is old: a fresh one may be
    /// another process of this workspace assembling an install right now.
    pub(crate) fn clear_stale_addon_staging(&self) {
        let _writer = self.addon_writer();
        let Ok(entries) = std::fs::read_dir(self.paths.addons_dir()) else {
            return;
        };
        for entry in entries.flatten() {
            if !entry.file_name().to_string_lossy().starts_with(".staging-") {
                continue;
            }
            let stale = entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|at| at.elapsed().ok())
                .is_some_and(|age| age >= STALE_STAGING);
            if stale {
                clear_staging(&entry.path());
            }
        }
    }

    /// One file of an installed bundle, resolved inside it and never
    /// outside — a path a bundle may not carry is refused before the
    /// filesystem is asked.
    pub fn get_addon_file(&self, id: &AddonId, rel: &str) -> Result<PathBuf, StoreError> {
        if !is_bundle_path(rel) {
            return Err(invalid(bisa_core::text!(
                "error-store-invalid-addon-not-bundle-path",
                rel = format!("{rel:?}")
            )));
        }
        let files = self.paths.addon_files_dir(id);
        crate::paths::resolve_within(&files, rel)
    }

    /// A record that arrived from a peer: written as the record alone, where
    /// none is here. The bundle is that machine's, so the addon lists with
    /// its files absent until the folder is imported under the same id. A
    /// record this machine holds **wins** — its switch and its grants are
    /// the person's decisions here — so a peer's later record lands in the
    /// snapshot store alone (ingest applied it before coming here) and the
    /// record file stands; the next local write republishes past it.
    pub(crate) fn adopt_remote_addon(&self, record: &AddonRecord) -> Result<(), StoreError> {
        let problems = record.validate();
        if !problems.is_empty() {
            return Err(invalid(bisa_core::text!(
                "error-store-invalid-addon-refused",
                problems = joined_problems(&problems)
            )));
        }
        if RESERVED_ADDON_IDS.contains(&record.manifest.id.as_str()) {
            return Err(invalid(bisa_core::text!(
                "error-store-invalid-addon-id-reserved",
                id = record.manifest.id.to_string()
            )));
        }
        // A built-in's id belongs to the catalog's own record of it, under its slug: a record made
        // by hand somewhere under that id, or one that says the catalog under another, would hold
        // the id against the built-in here — or be taken for it at the next refresh.
        let id = record.manifest.id.as_str();
        let fits = match &record.origin {
            Origin::Catalog { slug } => slug == id,
            _ => CATALOG.addon_bundle(id).is_none(),
        };
        if !fits {
            return Err(invalid(bisa_core::text!(
                "error-store-invalid-addon-ships-with-platform-give-your-own",
                a0 = format!("{id:?}")
            )));
        }
        let _writer = self.addon_writer();
        if self.paths.addon_record(&record.manifest.id).exists() {
            tracing::debug!(
                target: "bisa_store::addons",
                "a peer's record for the addon {} arrived; this machine's stands",
                record.manifest.id
            );
            return Ok(());
        }
        self.write_record(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::MemoryKeyStore;

    fn ws() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let ws =
            Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
        (dir, ws)
    }

    fn manifest_json(id: &str) -> String {
        format!(
            r#"{{"id":"{id}","name":"Byte","description":"a widget","version":"1.0.0","license":"MIT",
                "tags":["tool"],"window":{{"width":200,"height":120}},"permissions":["storage"]}}"#
        )
    }

    fn record(id: &str, origin: Origin) -> AddonRecord {
        AddonRecord {
            manifest: serde_json::from_str(&manifest_json(id)).unwrap(),
            origin,
            enabled: false,
            granted: vec![],
            installed_at: 1,
        }
    }

    #[test]
    fn a_built_ins_manifest_is_held_to_its_slug_and_a_peers_record_to_the_rules_and_the_reserved_name(
    ) {
        assert!(matches!(
            parse_addon("x", "{not json"),
            Err(StoreError::Invalid(_))
        ));
        assert!(matches!(
            parse_addon("x", &manifest_json("y")),
            Err(StoreError::Invalid(_))
        ));
        assert_eq!(
            parse_addon("y", &manifest_json("y")).unwrap().id.as_str(),
            "y"
        );
        let (_dir, ws) = ws();
        let mut nameless = record("acme.quiet", Origin::Local);
        nameless.manifest.name = String::new();
        assert!(matches!(
            ws.adopt_remote_addon(&nameless),
            Err(StoreError::Invalid(_))
        ));
        assert!(matches!(
            ws.adopt_remote_addon(&record("state", Origin::Local)),
            Err(StoreError::Invalid(_))
        ));
        assert!(matches!(
            ws.install_addon_from_catalog("nope"),
            Err(StoreError::Invalid(_))
        ));
    }

    #[test]
    fn the_refresh_passes_over_a_local_record_and_a_catalog_record_the_binary_no_longer_ships() {
        let (_dir, ws) = ws();
        ws.adopt_remote_addon(&record("acme.local", Origin::Local))
            .unwrap();
        ws.adopt_remote_addon(&record(
            "gone",
            Origin::Catalog {
                slug: "gone".into(),
            },
        ))
        .unwrap();
        assert!(ws.refresh_builtin_addons().unwrap().is_empty());
        // What is not an addon's folder is passed over by the listing.
        std::fs::write(ws.paths.addons_dir().join("README"), b"x").unwrap();
        std::fs::create_dir_all(ws.paths.addons_dir().join("Not An Id")).unwrap();
        std::fs::create_dir_all(ws.paths.addons_dir().join(".hidden")).unwrap();
        let listed: Vec<String> = ws
            .list_addons()
            .unwrap()
            .iter()
            .map(|a| a.id().to_string())
            .collect();
        assert_eq!(listed, ["acme.local", "gone"]);
        // A switch and a grant already as asked write nothing new.
        let id = AddonId::new("acme.local").unwrap();
        let before = ws.get_addon(&id).unwrap();
        assert_eq!(
            ws.set_addon_enabled(&id, false).unwrap().record,
            before.record
        );
        assert_eq!(
            ws.set_addon_grants(&id, vec![]).unwrap().record,
            before.record
        );
        // A peer's record for an id this machine holds changes nothing here.
        let mut later = record("acme.local", Origin::Local);
        later.enabled = true;
        later.granted = vec![AddonPermission::Storage];
        later.installed_at = 2;
        ws.adopt_remote_addon(&later).unwrap();
        assert_eq!(ws.get_addon(&id).unwrap().record, before.record);
    }

    /// A built-in's id is the catalog's: a peer's record made by hand under
    /// it, or one that says the catalog under another id, is refused — the
    /// first would hold the id against the built-in here, the second would be
    /// taken for the built-in at the next refresh and overwrite its folder.
    #[test]
    fn a_peers_record_cannot_take_a_built_ins_id_or_wear_its_origin_under_another() {
        let (_dir, ws) = ws();
        let by_hand = ws
            .adopt_remote_addon(&record("clock", Origin::Local))
            .unwrap_err()
            .to_string();
        assert!(by_hand.contains("clock"), "{by_hand}");
        let disguised = record(
            "acme.byte",
            Origin::Catalog {
                slug: "clock".into(),
            },
        );
        assert!(ws.adopt_remote_addon(&disguised).is_err());
        assert!(ws.list_addons().unwrap().is_empty());
        // The catalog's own record of it is what a peer's install sends, and lands.
        ws.adopt_remote_addon(&record(
            "clock",
            Origin::Catalog {
                slug: "clock".into(),
            },
        ))
        .unwrap();
        // Written by hand on disk — past every door — a record that wears the clock's origin
        // under another id is passed over by the refresh: the clock's folder is not its to write.
        let odd = record(
            "acme.odd",
            Origin::Catalog {
                slug: "weather".into(),
            },
        );
        ws.write_record(&odd).unwrap();
        assert!(ws.refresh_builtin_addons().unwrap().iter().all(|id| id.as_str() == "clock"));
        assert!(!ws.paths.addon_dir(&AddonId::new("weather").unwrap()).exists());
        assert_eq!(
            ws.get_addon(&AddonId::new("acme.odd").unwrap()).unwrap().record,
            odd
        );
        // And a record written by hand under a built-in's id is not the built-in: the catalog's
        // install refuses the id as taken — removing it is what frees it.
        ws.write_record(&record("weather", Origin::Local)).unwrap();
        let taken = ws.install_addon_from_catalog("weather").unwrap_err().to_string();
        assert!(taken.contains("weather"), "{taken}");
        ws.remove_addon(&AddonId::new("weather").unwrap()).unwrap();
        assert!(ws.install_addon_from_catalog("weather").unwrap().is_some());
    }

    /// A snapshot is filed by its `d`: one whose record names another id is
    /// refused at ingest, never written under an id its snapshot does not hold.
    #[test]
    fn a_peers_snapshot_filed_under_one_id_and_naming_another_is_refused() {
        let (_a, alpha) = ws();
        let keys = MemoryKeyStore::default();
        crate::identity::KeyStore::set(
            &keys,
            "owner",
            &alpha.owner_keys().secret_key().to_secret_hex(),
        )
        .unwrap();
        let b = tempfile::tempdir().unwrap();
        let beta = Workspace::open_with_keystore(b.path(), Box::new(keys)).unwrap();
        let named = record("acme.other", Origin::Local);
        let event = alpha
            .snapshots
            .put(
                Paths::NS_ADDONS,
                KIND_ADDON,
                "acme.filed",
                &named,
                1,
                &alpha.owner,
                now_secs(),
                None,
                &named.manifest.tags,
            )
            .unwrap();
        // The snapshot itself is kept, as every snapshot a peer sends is; no record is made of it.
        beta.ingest_remote_event(&event).unwrap();
        assert!(beta.list_addons().unwrap().is_empty());
        assert!(!beta.paths.addon_dir(&AddonId::new("acme.other").unwrap()).exists());
        assert!(!beta.paths.addon_dir(&AddonId::new("acme.filed").unwrap()).exists());
    }

    /// The addon write lock a panic poisoned is taken over: the next writer
    /// continues with the records as the last write left them.
    #[test]
    fn the_addon_write_lock_recovers_from_poison() {
        let (_dir, ws) = ws();
        let ws = std::sync::Arc::new(ws);
        ws.adopt_remote_addon(&record("acme.local", Origin::Local))
            .unwrap();
        let poisoner = std::sync::Arc::clone(&ws);
        let poisoned = std::thread::spawn(move || {
            let _writer = poisoner.addon_writer();
            panic!("poison the addon lock on purpose");
        })
        .join();
        assert!(poisoned.is_err(), "the thread panicked on purpose");
        let id = AddonId::new("acme.local").unwrap();
        let (entry, changes) = ws
            .update_addon(&id, None, Some(vec![AddonPermission::Storage]))
            .unwrap();
        assert_eq!(changes, vec![AddonChange::Grants]);
        assert_eq!(entry.record.granted, vec![AddonPermission::Storage]);
    }

    #[cfg(unix)]
    #[test]
    fn a_folder_or_a_record_nobody_may_read_is_said_by_its_path() {
        use std::os::unix::fs::PermissionsExt;
        let (_dir, ws) = ws();
        let src = tempfile::tempdir().unwrap();
        let sealed = src.path().join("sealed");
        std::fs::create_dir_all(&sealed).unwrap();
        std::fs::write(sealed.join("addon.json"), manifest_json("acme.sealed")).unwrap();
        let was = std::fs::metadata(&sealed).unwrap().permissions();
        std::fs::set_permissions(&sealed, std::fs::Permissions::from_mode(0o000)).unwrap();
        let read = ws.validate_addon_dir(&sealed);
        std::fs::set_permissions(&sealed, was).unwrap();
        assert!(matches!(read, Err(StoreError::Io { .. })), "{read:?}");
        // A sub-folder nobody may list is a problem under `files`.
        let deep = sealed.join("img");
        std::fs::create_dir_all(&deep).unwrap();
        let was = std::fs::metadata(&deep).unwrap().permissions();
        std::fs::set_permissions(&deep, std::fs::Permissions::from_mode(0o000)).unwrap();
        let judged = ws.validate_addon_dir(&sealed);
        std::fs::set_permissions(&deep, was).unwrap();
        let (_, problems) = judged.unwrap();
        assert!(
            problems.iter().any(|p| p.field.as_deref() == Some("files")),
            "{problems:?}"
        );
        // An installed record nobody may read, and a folder nobody may list.
        ws.adopt_remote_addon(&record("acme.local", Origin::Local))
            .unwrap();
        let id = AddonId::new("acme.local").unwrap();
        let file = ws.paths.addon_record(&id);
        let was = std::fs::metadata(&file).unwrap().permissions();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
        let one = ws.get_addon(&id);
        std::fs::set_permissions(&file, was).unwrap();
        assert!(matches!(one, Err(StoreError::Io { .. })), "{one:?}");
        let dir = ws.paths.addons_dir();
        let was = std::fs::metadata(&dir).unwrap().permissions();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o000)).unwrap();
        let listed = ws.list_addons();
        std::fs::set_permissions(&dir, was).unwrap();
        assert!(matches!(listed, Err(StoreError::Io { .. })), "{listed:?}");
    }
}
