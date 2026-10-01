//! Tag queries, at the workspace level.
//!
//! They live on the workspace rather than on the index because the index is
//! `pub(crate)`: every surface that filters — HTTP, CLI, desktop — goes through
//! one set of functions.

use crate::error::StoreError;
use crate::workspace::Workspace;
use bisa_core::tags::{TagEntity, TagMatch};
use serde::{Deserialize, Serialize};

/// One row of a tag facet: how many objects of a kind carry a tag.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagCount {
    pub entity: String,
    pub tag: String,
    pub count: u64,
}

impl Workspace {
    /// Ids of one entity kind carrying `tags`. An empty `tags` returns nothing.
    pub fn ids_with_tags(
        &self,
        entity: TagEntity,
        tags: &[String],
        match_mode: TagMatch,
    ) -> Result<Vec<String>, StoreError> {
        self.idx().ids_with_tags(entity, tags, match_mode)
    }

    pub fn tags_of(&self, entity: TagEntity, id: &str) -> Result<Vec<String>, StoreError> {
        self.idx().tags_of(entity, id)
    }

    /// Facet counts, most-used first. `None` counts across every kind.
    pub fn tag_counts(&self, entity: Option<TagEntity>) -> Result<Vec<TagCount>, StoreError> {
        Ok(self
            .idx()
            .tag_counts(entity)?
            .into_iter()
            .map(|(entity, tag, count)| TagCount { entity, tag, count })
            .collect())
    }
}
