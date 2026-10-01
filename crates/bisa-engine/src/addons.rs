//! Addons: what the engine does for an overlay widget beyond the store's
//! record — says when one moved, and fetches for one that was granted the
//! network ([18 — Addons](../../../docs/architecture/18-addons.md)).
//!
//! # The broker
//!
//! An addon's page can reach nothing itself: its frame's policy says
//! `connect-src 'none'`. What it may fetch it asks the desktop for, the
//! desktop asks the node, and the node comes here. Every refusal is read in
//! order before a socket opens: the machine's switch, the record (enabled,
//! files here), the grant, the URL's shape, its scheme (`https` only), the
//! authority (never this machine), the declared hosts, and the person's
//! `security.net.*` lists judged by the same rule a connector's call is —
//! a refusal there is journaled as a guard decision with the tool
//! `addon:<id>`. What comes back is capped at [`MAX_FETCH_BYTES`], read as
//! text, and never redirected: a 3xx is the answer, not a second request.

use crate::{EngineError, EngineEvent, EnginePayload, Inner};
use bisa_core::{AddonError, AddonId};
use bisa_store::AddonEntry;
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;

/// How long one brokered fetch may take, end to end.
pub const ADDON_FETCH_TIMEOUT_SECS: u64 = 10;
/// The most of a body an addon is handed.
pub const MAX_FETCH_BYTES: usize = 1024 * 1024;

/// What a brokered fetch answers: the status, the type the host claimed,
/// the body as text up to the cap, and whether the cap cut it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct AddonFetchResult {
    pub status: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    pub body_text: String,
    pub truncated: bool,
}

/// Which addon moved, and how — the bus frame Settings and the addon layer
/// re-read on. Never a value: only which part moved.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "what")]
pub enum AddonsChange {
    Installed { id: AddonId },
    Removed { id: AddonId },
    Enabled { id: AddonId },
    Disabled { id: AddonId },
    Grants { id: AddonId },
}

impl AddonsChange {
    pub fn as_str(&self) -> &'static str {
        match self {
            AddonsChange::Installed { .. } => "installed",
            AddonsChange::Removed { .. } => "removed",
            AddonsChange::Enabled { .. } => "enabled",
            AddonsChange::Disabled { .. } => "disabled",
            AddonsChange::Grants { .. } => "grants",
        }
    }

    pub fn id(&self) -> &AddonId {
        match self {
            AddonsChange::Installed { id }
            | AddonsChange::Removed { id }
            | AddonsChange::Enabled { id }
            | AddonsChange::Disabled { id }
            | AddonsChange::Grants { id } => id,
        }
    }
}

/// Say on the bus that an addon moved.
pub(crate) fn announce(inner: &Inner, what: AddonsChange) {
    inner.emit(EngineEvent::global(EnginePayload::AddonsChanged { what }));
}

/// Whether this machine runs addons at all (`addons.enabled`, machine scope).
pub fn addons_enabled(inner: &Inner) -> bool {
    inner
        .ws
        .setting("addons.enabled", None)
        .ok()
        .and_then(|r| r.value.as_bool())
        .unwrap_or(true)
}

/// The record an addon must have before anything is done for it: the
/// switch on, the addon known, enabled, its files here.
fn active(inner: &Inner, id: &AddonId) -> Result<AddonEntry, EngineError> {
    if !addons_enabled(inner) {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-addons-switched-off"
        )));
    }
    let addon = inner.ws.get_addon(id)?;
    if !addon.is_active() {
        return Err(EngineError::Store(
            AddonError::NotEnabled(id.to_string()).into(),
        ));
    }
    Ok(addon)
}

/// Fetch one URL on an addon's behalf, or refuse by name.
pub async fn addon_fetch(
    inner: &Arc<Inner>,
    id: &AddonId,
    url: &str,
    accept: Option<&str>,
) -> Result<AddonFetchResult, EngineError> {
    let addon = active(inner, id)?;
    let Some(declared) = addon.record.network_hosts() else {
        return Err(EngineError::Store(
            AddonError::NotGranted {
                id: id.to_string(),
                word: "network".to_string(),
            }
            .into(),
        ));
    };
    let head = bisa_core::connector::url_head(url).ok_or_else(|| {
        EngineError::Invalid(bisa_core::text!(
            "error-engine-addons-url-malformed",
            url = format!("{url:?}")
        ))
    })?;
    if head.scheme != "https" {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-addons-https-only",
            scheme = head.scheme.to_string()
        )));
    }
    let host = head.authority.to_ascii_lowercase();
    if bisa_core::connector::is_loopback(&host) {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-addons-loopback-refused",
            host = host.clone()
        )));
    }
    if !declared
        .iter()
        .any(|p| bisa_core::connector::host_matches(p, &host))
    {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-addons-host-not-declared",
            host = host.clone(),
            declared = declared.join(", ")
        )));
    }
    let policy = inner.security.policy();
    if let bisa_security::net::HostVerdict::Deny { reason } =
        bisa_security::net::decide_host(&policy.hosts, declared, &host)
    {
        crate::security::record_host_refusal(
            inner,
            None,
            &format!("addon:{id} GET {host}"),
            &reason,
        );
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-addons-host-refused",
            host = host.clone(),
            reason = reason
        )));
    }
    let accept = accept.unwrap_or("*/*").trim();
    if accept.is_empty() || !accept.bytes().all(|b| (0x20..0x7f).contains(&b)) {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-addons-bad-accept"
        )));
    }

    // Every refusal above stands before this line; nothing below is a rule.
    let response = inner
        .http
        .strict()
        .get(url)
        .header(reqwest::header::ACCEPT, accept)
        .timeout(Duration::from_secs(ADDON_FETCH_TIMEOUT_SECS))
        .send()
        .await
        .map_err(|e| {
            EngineError::Invalid(bisa_core::text!(
                "error-engine-addons-fetch-failed",
                e = e.to_string()
            ))
        })?;
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let (bytes, truncated) = read_capped(response, MAX_FETCH_BYTES).await.map_err(|e| {
        EngineError::Invalid(bisa_core::text!(
            "error-engine-addons-fetch-failed",
            e = e.to_string()
        ))
    })?;
    Ok(AddonFetchResult {
        status,
        content_type,
        body_text: String::from_utf8_lossy(&bytes).into_owned(),
        truncated,
    })
}

/// The body up to `cap` bytes, and whether more was coming. The connection
/// is dropped at the cap, so a host that streams forever costs one buffer.
async fn read_capped(
    mut response: reqwest::Response,
    cap: usize,
) -> reqwest::Result<(Vec<u8>, bool)> {
    let mut out = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if out.len() + chunk.len() > cap {
            out.extend_from_slice(&chunk[..cap - out.len()]);
            return Ok((out, true));
        }
        out.extend_from_slice(&chunk);
    }
    Ok((out, false))
}

/// The cap rule on its own, for the test that has no host: `chunks` as they
/// would arrive, `cap` as the broker keeps it.
pub fn cap_chunks(chunks: &[&[u8]], cap: usize) -> (Vec<u8>, bool) {
    let mut out = Vec::new();
    for chunk in chunks {
        if out.len() + chunk.len() > cap {
            out.extend_from_slice(&chunk[..cap - out.len()]);
            return (out, true);
        }
        out.extend_from_slice(chunk);
    }
    (out, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cap_cuts_the_body_and_says_so() {
        assert_eq!(
            cap_chunks(&[b"abc", b"def"], 10),
            (b"abcdef".to_vec(), false)
        );
        assert_eq!(cap_chunks(&[b"abc", b"def"], 4), (b"abcd".to_vec(), true));
        assert_eq!(cap_chunks(&[b"abcdef"], 6), (b"abcdef".to_vec(), false));
        assert_eq!(cap_chunks(&[], 6), (Vec::new(), false));
    }

    #[test]
    fn a_change_names_its_kind_and_its_addon() {
        let id = AddonId::new("clock").unwrap();
        let change = AddonsChange::Grants { id: id.clone() };
        assert_eq!(change.as_str(), "grants");
        assert_eq!(change.id(), &id);
        let json = serde_json::to_string(&change).unwrap();
        assert_eq!(json, r#"{"what":"grants","id":"clock"}"#);
    }
}
