//! The **archived** mark — a goal, a workflow or a project put away.
//!
//! A mark on the record, never a status: an archived goal is still a closed
//! goal, an archived workflow still its revisions, an archived project still
//! its folder. What the mark does is hide the record from every list that
//! does not ask for it, and refuse the moves that would start work on it —
//! a run on an archived workflow, an attachment to an archived project.
//! One move back: unarchiving clears it.

use serde::{Deserialize, Serialize};

/// When a record was archived — unix seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Archived {
    pub at: u64,
}

impl Archived {
    pub fn at(at: u64) -> Self {
        Self { at }
    }
}
