//! What notes and drawings share below their own modules: the filter a
//! listing takes ([`OwnerFilter`]) and the folder a scope's files sit in
//! under a repository root ([`Workspace::owner_dir`]), which checks the
//! scope names something this workspace has before a file is written for it.

use crate::error::StoreError;
use crate::paths::Paths;
use crate::workspace::Workspace;
use bisa_core::OwnerScope;
use std::path::{Path, PathBuf};

/// Which records a listing is of.
///
/// `Kind` carries one of [`OwnerScope::KINDS`] — *every project's notes* is
/// the Projects tab, not a scope, and the index answers it without the
/// caller enumerating projects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwnerFilter {
    /// Every record on this machine.
    All,
    /// Every record of one kind, whatever it is about.
    Kind(&'static str),
    /// One scope's records.
    Scope(OwnerScope),
}

impl OwnerFilter {
    /// The pair the index matches on: a kind to match or none, an id to match or none.
    pub(crate) fn parts(&self) -> (Option<&str>, Option<String>) {
        match self {
            OwnerFilter::All => (None, None),
            OwnerFilter::Kind(kind) => (Some(kind), None),
            OwnerFilter::Scope(scope) => (Some(scope.kind()), scope.id()),
        }
    }
}

impl Workspace {
    /// The directory a scope's files sit in under `base` — the notes' or the
    /// drawings' root. Checks the scope exists, and names a project by its
    /// slug so the folder reads in a clone.
    pub(crate) fn owner_dir(&self, base: &Path, scope: &OwnerScope) -> Result<PathBuf, StoreError> {
        let id = match scope {
            OwnerScope::Workspace | OwnerScope::Node => None,
            OwnerScope::Goal { id } => {
                self.get_goal(*id)?;
                Some(id.to_string())
            }
            OwnerScope::Project { id } => Some(self.get_project(*id)?.slug.to_string()),
            OwnerScope::Workflow { id } => {
                self.get_workflow(*id)?;
                Some(id.to_string())
            }
            OwnerScope::Channel { id } => {
                self.get_channel(id)?;
                Some(id.to_string())
            }
        };
        Ok(Paths::scoped_dir(base, scope.kind(), id.as_deref()))
    }
}
