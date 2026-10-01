//! Writing a setting: the one door.
//!
//! A setting's value is kept by the store, held by whichever part of the
//! engine reads it live, and shown by whoever has an editor open. A write
//! therefore does three things, in this order: the store takes the value
//! (and refuses a scope the key does not allow, or a value outside its
//! kind); every part of the engine that holds the key re-reads it
//! ([`refresh`] — one list, so a write and an unset cannot disagree about
//! who hears); and the bus says which keys moved (`settings_changed`), so an
//! open editor, the node's pump and a listener on the command line re-read
//! without a reload.
//!
//! A write made past this door — straight into the store — is a value
//! nobody hears of: the engine goes on with the old one and a screen shows
//! it. Everything that changes a setting comes here: a person's edit
//! ([`crate::Engine::set_setting`]), a project's scripts approved
//! ([`crate::scripts::approve`]), the relays an invitation's code names
//! (the node's pump).

use crate::events::{EngineEvent, EnginePayload};
use crate::{
    cache, listen, logging, lsp, mobile_development, network, security, EngineError, Inner,
};
use bisa_core::{ProjectId, ResolvedSetting, SettingScope};

/// Write one setting at one scope. Answers the value as it now resolves.
pub fn set(
    inner: &Inner,
    scope: SettingScope,
    project: Option<ProjectId>,
    key: &str,
    value: serde_json::Value,
) -> Result<ResolvedSetting, EngineError> {
    let resolved = inner.ws.set_setting(scope, project, key, value)?;
    moved(inner, scope, project, key);
    Ok(resolved)
}

/// Remove one key from one scope; the value falls back to the next layer.
pub fn unset(
    inner: &Inner,
    scope: SettingScope,
    project: Option<ProjectId>,
    key: &str,
) -> Result<ResolvedSetting, EngineError> {
    let resolved = inner.ws.unset_setting(scope, project, key)?;
    moved(inner, scope, project, key);
    Ok(resolved)
}

/// A key moved at a scope: whoever holds it re-reads, then the bus says so.
fn moved(inner: &Inner, scope: SettingScope, project: Option<ProjectId>, key: &str) {
    refresh(inner, key);
    inner.emit(EngineEvent::global(EnginePayload::SettingsChanged {
        scope: scope.as_str().to_string(),
        project,
        keys: vec![key.to_string()],
    }));
}

/// Every part of the engine that holds a setting's value re-reads it. A
/// module that reads a setting live joins this list once.
fn refresh(inner: &Inner, key: &str) {
    cache::refresh_for(inner, key);
    listen::refresh_for(inner, key);
    security::refresh_for(inner, key);
    logging::refresh_for(inner, key);
    network::refresh_for(inner, key);
    mobile_development::refresh_for(inner, key);
    lsp::refresh_for(inner, key);
}
