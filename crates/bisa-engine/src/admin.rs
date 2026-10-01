//! Workspace administration a person performs: who may sign which gate, who
//! is a member, the catalog, pets, addons and uploaded attachments.
//!
//! Thin on purpose — the store owns the files and their refusals; this module
//! is the engine's door for those writes so the node never reaches the store
//! (`docs/architecture/07-layering.md`, rule 2).

use crate::addons::{announce as announce_addons, AddonsChange};
use crate::{EngineError, Inner};
use bisa_core::{AddonId, AddonPermission, AttachmentRef, Gate, MemberRole, Pet, PrincipalId};
use bisa_store::{AddonEntry, CatalogKind, GatePolicy, Governance, Installed};
use std::path::Path;
use std::sync::Arc;

/// Which gates to re-policy; an omitted gate keeps its policy.
#[derive(Debug, Default)]
pub struct GovernancePatch {
    pub approval: Option<GatePolicy>,
    pub escalation: Option<GatePolicy>,
    pub publish: Option<GatePolicy>,
}

/// Apply a governance patch and answer the whole table.
pub fn set_governance(
    inner: &Arc<Inner>,
    patch: GovernancePatch,
) -> Result<Governance, EngineError> {
    for (gate, policy) in [
        (Gate::Approval, patch.approval),
        (Gate::Escalation, patch.escalation),
        (Gate::Publish, patch.publish),
    ] {
        if let Some(p) = policy {
            inner.ws.set_gate_policy(gate, p)?;
        }
    }
    Ok(inner.ws.governance()?)
}

pub fn add_member(
    inner: &Arc<Inner>,
    pubkey: PrincipalId,
    role: MemberRole,
    admission: bisa_store::Admission,
) -> Result<(), EngineError> {
    Ok(inner.ws.add_member(pubkey, role, admission)?)
}

pub fn remove_member(inner: &Arc<Inner>, pubkey: &PrincipalId) -> Result<(), EngineError> {
    Ok(inner.ws.remove_member(pubkey)?)
}

/// Install one catalog entry and everything it needs. Idempotent: what is
/// already present is left alone and answers as nothing installed.
pub fn install_catalog_entry(
    inner: &Arc<Inner>,
    kind: CatalogKind,
    slug: &str,
) -> Result<Installed, EngineError> {
    let installed = inner.ws.install(kind, slug)?;
    // A built-in addon that landed is said like an imported one, so an open
    // desktop draws it without being asked to look again.
    for slug in &installed.addons {
        if let Ok(id) = AddonId::new(slug.as_str()) {
            announce_addons(inner, AddonsChange::Installed { id });
        }
    }
    Ok(installed)
}

/// Copy a person's addon folder in, with the grants and the enabled flag
/// they chose in the review (18 — Addons).
pub fn install_addon(
    inner: &Arc<Inner>,
    from: &Path,
    granted: Vec<AddonPermission>,
    enabled: bool,
) -> Result<AddonEntry, EngineError> {
    let addon = inner.ws.install_addon(from, granted, enabled)?;
    announce_addons(
        inner,
        AddonsChange::Installed {
            id: addon.id().clone(),
        },
    );
    Ok(addon)
}

pub fn set_addon_enabled(
    inner: &Arc<Inner>,
    id: &AddonId,
    enabled: bool,
) -> Result<AddonEntry, EngineError> {
    let addon = inner.ws.set_addon_enabled(id, enabled)?;
    announce_addons(
        inner,
        if enabled {
            AddonsChange::Enabled { id: id.clone() }
        } else {
            AddonsChange::Disabled { id: id.clone() }
        },
    );
    Ok(addon)
}

pub fn set_addon_grants(
    inner: &Arc<Inner>,
    id: &AddonId,
    granted: Vec<AddonPermission>,
) -> Result<AddonEntry, EngineError> {
    let addon = inner.ws.set_addon_grants(id, granted)?;
    announce_addons(inner, AddonsChange::Grants { id: id.clone() });
    Ok(addon)
}

pub fn remove_addon(inner: &Arc<Inner>, id: &AddonId) -> Result<(), EngineError> {
    inner.ws.remove_addon(id)?;
    announce_addons(inner, AddonsChange::Removed { id: id.clone() });
    Ok(())
}

/// Copy a pet package in, exactly as it arrived.
pub fn install_pet(inner: &Arc<Inner>, from: &Path) -> Result<Pet, EngineError> {
    Ok(inner.ws.install_pet(from)?)
}

pub fn remove_pet(inner: &Arc<Inner>, id: &str) -> Result<(), EngineError> {
    Ok(inner.ws.remove_pet(id)?)
}

/// Store uploaded bytes under their hash; answers the descriptor a message
/// names them by.
pub fn put_attachment(
    inner: &Arc<Inner>,
    bytes: &[u8],
    name: &str,
    mime: &str,
) -> Result<AttachmentRef, EngineError> {
    Ok(inner.ws.put_attachment(bytes, name, mime)?)
}

/// A stored blob under the name its maker gave it, made on demand — the file
/// a file manager reveals and the default application opens. The store's
/// one writer of the named copy, reached by the node only through here.
pub fn attachment_named(
    inner: &Arc<Inner>,
    sha256: &str,
    name: &str,
) -> Result<std::path::PathBuf, EngineError> {
    Ok(inner.ws.put_attachment_named(sha256, name)?)
}
