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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::MemoryKeyStore;
    use crate::workflows::tests::notify_workflow;
    use bisa_core::{Tags, WorkflowOrigin};

    #[test]
    fn a_workflows_tags_are_read_back_by_its_id_and_counted() {
        let dir = tempfile::tempdir().unwrap();
        let ws =
            Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
        let mut new = notify_workflow("Tagged");
        new.tags = Tags::new(vec!["ops".to_string(), "nightly".to_string()]).unwrap();
        let wf = ws.create_workflow(new, WorkflowOrigin::Workspace).unwrap();
        let id = wf.id.to_string();
        assert_eq!(
            ws.tags_of(TagEntity::Workflow, &id).unwrap(),
            ["nightly", "ops"]
        );
        assert_eq!(
            ws.ids_with_tags(TagEntity::Workflow, &["ops".to_string()], TagMatch::Any)
                .unwrap(),
            std::slice::from_ref(&id)
        );
        assert!(ws
            .ids_with_tags(TagEntity::Workflow, &[], TagMatch::All)
            .unwrap()
            .is_empty());
        let counts = ws.tag_counts(Some(TagEntity::Workflow)).unwrap();
        assert!(
            counts
                .iter()
                .any(|c| c.tag == "ops" && c.count == 1 && c.entity == "workflow"),
            "{counts:?}"
        );
    }
}
