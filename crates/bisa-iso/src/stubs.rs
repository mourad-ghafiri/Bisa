//! v1 stubs for CoW backends. They exist so every [`crate::BackendKind`] is
//! dispatchable on every platform — probe reports unavailable, start rejects
//! with `Unavailable`, and callers fall through the candidate chain with no
//! `#[cfg]` at the call site.

use std::path::Path;

use crate::{BackendKind, IsoError, IsoResult, IsolationBackend, ProbeResult};

const V1_REASON: &str = "not implemented in v1";

macro_rules! stub_backend {
    ($name:ident, $kind:expr) => {
        pub struct $name;

        impl IsolationBackend for $name {
            fn kind(&self) -> BackendKind {
                $kind
            }

            fn probe(&self) -> ProbeResult {
                ProbeResult::unavailable(V1_REASON)
            }

            fn start(&self, _lower: &Path, _merged: &Path) -> IsoResult<()> {
                Err(IsoError::unavailable(V1_REASON))
            }

            fn stop(&self, _merged: &Path) -> IsoResult<()> {
                Err(IsoError::unavailable(V1_REASON))
            }
        }
    };
}

stub_backend!(ApfsStub, BackendKind::Apfs);
stub_backend!(OverlayfsStub, BackendKind::Overlayfs);
