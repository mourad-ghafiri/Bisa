//! The workbench's layout — which documents are open, which is active — kept
//! per root under `ide/layout/`, so a restart puts a person back where they
//! were. Window furniture: local, never synced, no GEP kind (ide/03).

use crate::{EngineError, Inner};
use bisa_store::FileScope;
use std::path::PathBuf;
use std::sync::Arc;

/// A layout file larger than this is a bug, not a layout.
pub const MAX_LAYOUT_BYTES: usize = 256 * 1024;

fn path_for(inner: &Arc<Inner>, scope: FileScope, id: &str) -> Result<PathBuf, EngineError> {
    // The root has to exist: an id nobody knows is the store's refusal (a
    // 404 through its taxonomy), never a `null` layout that reads as "none saved".
    inner.ws.file_root(scope, id)?;
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-not-id-layout-can-be-filed-under",
            id = format!("{id:?}")
        )));
    }
    Ok(inner.ws.paths().ide_layout(scope, id))
}

/// The stored layout, or `None` when nothing has been saved for this root.
pub fn read(
    inner: &Arc<Inner>,
    scope: FileScope,
    id: &str,
) -> Result<Option<serde_json::Value>, EngineError> {
    let path = path_for(inner, scope, id)?;
    match std::fs::read(&path) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes).map_err(|e| {
            EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-saved-layout-not-layout-file",
                a0 = (scope.as_str()).to_string(),
                id = id.to_string(),
                e = e.to_string()
            ))
        })?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Replace the stored layout. Any JSON the client means; the model on the
/// client decides what it is.
pub fn write(
    inner: &Arc<Inner>,
    scope: FileScope,
    id: &str,
    layout: &serde_json::Value,
) -> Result<(), EngineError> {
    let bytes = serde_json::to_vec(layout).map_err(|e| {
        EngineError::Invalid(bisa_core::text!(
            "error-engine-ide-layout-refused",
            detail = e.to_string()
        ))
    })?;
    if bytes.len() > MAX_LAYOUT_BYTES {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-layout-bytes-cap",
            a0 = (bytes.len()).to_string(),
            max_layout_bytes = (MAX_LAYOUT_BYTES).to_string()
        )));
    }
    let path = path_for(inner, scope, id)?;
    bisa_store::write_atomic(&path, &bytes)?;
    Ok(())
}
