//! Cross-platform isolation PAL for Bisa work-items.
//!
//! A backend gives the engine a writable "merged" view of a read-only
//! "lower" tree, and knows how to surface the changes the workload made.
//! Design ported from oh-my-pi's `pi-iso` crate:
//!
//! - **Every kind is dispatchable on every platform.** Backends that aren't
//!   implemented for the current target (or in v1 at all) return a stub whose
//!   [`IsolationBackend::probe`] reports `available: false` with a reason —
//!   no `#[cfg]` at call sites.
//! - **Two-phase availability.** `probe()` is a host-level check; `start()`
//!   may still fail with [`IsoError::Unavailable`] for a specific path pair
//!   (e.g. `lower` is not a git repo for [`BackendKind::GitWorktree`]).
//!   Callers walk [`Resolution::candidates`] on `Unavailable`.
//! - [`BackendKind::Copy`] is the universal floor and always terminates the
//!   candidate chain.

use std::fmt;
use std::path::Path;

mod copy;
mod diff;
mod stubs;
mod worktree;

pub use diff::{ChangeKind, Diff, FileChange};

/// Stable identifier for an isolation backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackendKind {
    /// APFS `clonefile(2)` copy-on-write clone (macOS). Stub in v1.
    Apfs,
    /// Kernel `overlay` filesystem (Linux). Stub in v1.
    Overlayfs,
    /// `git worktree add --detach` when `lower` is a git repo.
    GitWorktree,
    /// Plain recursive copy. Always available; the universal fallback.
    Copy,
}

impl BackendKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Apfs => "apfs",
            Self::Overlayfs => "overlayfs",
            Self::GitWorktree => "git-worktree",
            Self::Copy => "copy",
        }
    }

    /// Inverse of [`Self::as_str`]. `None` for unknown strings so callers can
    /// surface a precise error.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "apfs" => Self::Apfs,
            "overlayfs" => Self::Overlayfs,
            "git-worktree" | "worktree" => Self::GitWorktree,
            "copy" | "rcopy" => Self::Copy,
            _ => return None,
        })
    }
}

impl fmt::Display for BackendKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Automatic preference order. CoW backends lead (stubs in v1, so they probe
/// unavailable and fall through), then git-worktree, then the copy floor.
/// Identical on every platform because unavailability is expressed by the
/// probe, not by the list.
pub const AUTO_ORDER: &[BackendKind] = &[
    BackendKind::Apfs,
    BackendKind::Overlayfs,
    BackendKind::GitWorktree,
    BackendKind::Copy,
];

/// Result of a host-level backend probe.
#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub available: bool,
    pub reason: Option<String>,
}

impl ProbeResult {
    pub const fn available() -> Self {
        Self {
            available: true,
            reason: None,
        }
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            available: false,
            reason: Some(reason.into()),
        }
    }
}

/// Error returned by every backend operation.
///
/// `Unavailable` is the only variant callers treat specially: the
/// prerequisite is missing for this host or this specific path pair, and the
/// caller should fall back to the next candidate rather than surface a hard
/// failure.
#[derive(Debug, Clone, thiserror::Error)]
pub enum IsoError {
    #[error("isolation backend unavailable: {0}")]
    Unavailable(String),
    #[error("{0}")]
    Other(String),
}

impl IsoError {
    pub fn unavailable(msg: impl Into<String>) -> Self {
        Self::Unavailable(msg.into())
    }

    pub fn other(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }

    pub const fn is_unavailable(&self) -> bool {
        matches!(self, Self::Unavailable(_))
    }
}

pub type IsoResult<T> = Result<T, IsoError>;

/// `bisa-vcs` speaks a richer error vocabulary than isolation needs. The
/// only distinction that survives the translation is the one callers act on:
/// "this host or this path cannot do it" means walk the fallback chain,
/// everything else is a genuine failure carrying git's own words.
impl From<bisa_vcs::VcsError> for IsoError {
    fn from(err: bisa_vcs::VcsError) -> Self {
        if err.is_unavailable() {
            Self::Unavailable(err.to_string())
        } else {
            Self::Other(err.to_string())
        }
    }
}

/// Backend contract. `lower` is the read-only source tree; `merged` is where
/// the writable view is materialised. Implementations create any auxiliary
/// state in `start` and tear it down in `stop`.
///
/// All methods are synchronous: `start`/`stop` wrap blocking primitives and
/// `diff` does bounded I/O; the engine drives all three from `spawn_blocking`.
pub trait IsolationBackend: Send + Sync {
    fn kind(&self) -> BackendKind;

    /// Host-level availability. `start` can still fail `Unavailable` for a
    /// specific path pair.
    fn probe(&self) -> ProbeResult;

    fn start(&self, lower: &Path, merged: &Path) -> IsoResult<()>;

    fn stop(&self, merged: &Path) -> IsoResult<()>;

    /// Capture the changes the workload made in `merged` relative to the
    /// baseline. Uses `git diff` when `merged` is a git working tree (so the
    /// output is what `git apply` consumes), a tree walk otherwise.
    fn diff(&self, lower: &Path, merged: &Path) -> IsoResult<Diff> {
        diff::default_diff(lower, merged)
    }
}

/// Look up a backend by kind. Every kind dispatches in every build; v1 stubs
/// (`Apfs`, `Overlayfs`) probe unavailable and reject `start`.
pub fn backend(kind: BackendKind) -> &'static dyn IsolationBackend {
    match kind {
        BackendKind::Apfs => &stubs::ApfsStub,
        BackendKind::Overlayfs => &stubs::OverlayfsStub,
        BackendKind::GitWorktree => &worktree::GitWorktreeBackend,
        BackendKind::Copy => &copy::CopyBackend,
    }
}

/// Outcome of [`resolve`]: the first host-available backend plus the ordered
/// fallback chain (always ending in [`BackendKind::Copy`]).
#[derive(Debug, Clone)]
pub struct Resolution {
    pub kind: BackendKind,
    pub candidates: Vec<BackendKind>,
    pub fell_back: bool,
    pub reason: Option<String>,
}

/// Pick the best backend whose host-level prerequisites are available.
///
/// 1. A `preferred` kind whose probe passes leads the chain.
/// 2. Otherwise [`AUTO_ORDER`] is walked (skipping `preferred`).
/// 3. [`BackendKind::Copy`] terminates the chain unconditionally.
pub fn resolve(preferred: Option<BackendKind>) -> Resolution {
    let mut reason = None;
    let mut candidates = Vec::new();

    if let Some(p) = preferred {
        let probe = backend(p).probe();
        if probe.available {
            candidates.push(p);
        } else {
            reason = probe.reason;
        }
    }

    for candidate in AUTO_ORDER {
        if Some(*candidate) == preferred || candidates.contains(candidate) {
            continue;
        }
        let probe = backend(*candidate).probe();
        if probe.available {
            candidates.push(*candidate);
        } else if reason.is_none() {
            reason = probe.reason;
        }
    }

    if candidates.is_empty() {
        candidates.push(BackendKind::Copy);
    }
    let kind = candidates[0];
    let fell_back = match preferred {
        Some(p) => kind != p,
        None => kind != AUTO_ORDER[0],
    };

    Resolution {
        kind,
        candidates,
        fell_back,
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_dispatches() {
        for k in [
            BackendKind::Apfs,
            BackendKind::Overlayfs,
            BackendKind::GitWorktree,
            BackendKind::Copy,
        ] {
            assert_eq!(backend(k).kind(), k);
        }
    }

    #[test]
    fn stubs_probe_unavailable() {
        assert!(!backend(BackendKind::Apfs).probe().available);
        assert!(!backend(BackendKind::Overlayfs).probe().available);
        assert!(backend(BackendKind::Copy).probe().available);
    }

    #[test]
    fn resolve_chain_ends_in_copy() {
        let r = resolve(None);
        assert_eq!(r.candidates.last(), Some(&BackendKind::Copy));
        // v1 stubs are unavailable, so auto resolution falls back past Apfs.
        assert!(r.fell_back);
        assert!(r.reason.is_some());
    }

    #[test]
    fn resolve_honors_available_preference() {
        let r = resolve(Some(BackendKind::Copy));
        assert_eq!(r.kind, BackendKind::Copy);
        assert!(!r.fell_back);
    }

    #[test]
    fn resolve_falls_back_from_unavailable_preference() {
        let r = resolve(Some(BackendKind::Apfs));
        assert_ne!(r.kind, BackendKind::Apfs);
        assert!(r.fell_back);
        assert!(r.reason.is_some());
    }

    #[test]
    fn kind_strings_roundtrip() {
        for k in [
            BackendKind::Apfs,
            BackendKind::Overlayfs,
            BackendKind::GitWorktree,
            BackendKind::Copy,
        ] {
            assert_eq!(BackendKind::from_str(k.as_str()), Some(k));
        }
        assert_eq!(BackendKind::from_str("nope"), None);
    }
}
