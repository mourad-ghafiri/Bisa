//! Drawings (19 — Drawings): a picture on a canvas as a GEP record. The
//! **snapshot is the truth** — a drawing is an addressable event of kind
//! [`KIND_DRAWING`] in the `drawings/` namespace, like an addon's record, so
//! it travels to every peer of the workspace — and the **file is the
//! export**: every write materializes the standard `.excalidraw` file under
//! the drawings repository the engine keeps over `drawings/`
//! (`drawings/workspace/<id>.excalidraw`, `drawings/goals/<id>/…`, the
//! layout notes use), so the person commits pictures that open anywhere and
//! diff as text. A peer's drawing lands the same way: ingested, indexed,
//! drawn into the file.
//!
//! The domain type is [`bisa_core::Drawing`]; its scene is Excalidraw's
//! element JSON, which the core bounds and this module never interprets
//! beyond counting. A listing is an [`OwnerFilter`] and carries **no scene**
//! — a scene can be hundreds of kibibytes — so the index holds every column
//! a list draws.
//!
//! Two writers on one drawing is the ordinary case (the person on the
//! canvas, an agent through the desktop), so a scene change is
//! compare-and-swap on the scene's hash and a mismatch hands back what is
//! there ([`StoreError::EditConflict`]). A title or a pin needs no hash.

use crate::error::StoreError;
use crate::index::DrawingRow;
use crate::owner::OwnerFilter;
use crate::paths::Paths;
use crate::workspace::{mint_ulid, now_secs, EventAudience, StoreEvent, Workspace};
use bisa_core::kind::KIND_DRAWING;
use bisa_core::{Drawing, DrawingId, DrawingSummary, OwnerScope, Scene};
use std::path::{Path, PathBuf};
use std::sync::MutexGuard;

/// The fields a caller supplies. `id`, timestamps and `pinned` are the store's.
#[derive(Clone, Debug)]
pub struct NewDrawing {
    pub scope: OwnerScope,
    pub title: String,
    /// The first scene — a template's elements — or an empty canvas.
    pub scene: Option<Scene>,
}

/// What an update may change.
#[derive(Clone, Debug, Default)]
pub struct DrawingPatch {
    pub title: Option<String>,
    pub pinned: Option<bool>,
    pub scene: Option<Scene>,
}

/// The hash a caller passes back to prove which scene it was editing — the
/// store's alone: the desktop hands back the one it read, never one it made.
pub fn scene_hash(scene: &Scene) -> String {
    let bytes = serde_json::to_vec(scene).unwrap_or_default();
    crate::recall::sha256_hex(&bytes)
}

impl Workspace {
    /// The directory a scope's drawings sit in. Checks the scope exists.
    fn drawings_dir_for(&self, scope: &OwnerScope) -> Result<PathBuf, StoreError> {
        self.owner_dir(&self.paths.drawings_dir(), scope)
    }

    fn drawing_path(&self, scope: &OwnerScope, id: DrawingId) -> Result<PathBuf, StoreError> {
        Ok(Paths::drawing_file_in(&self.drawings_dir_for(scope)?, id))
    }

    /// The record as the snapshot holds it.
    pub fn get_drawing(&self, id: DrawingId) -> Result<Drawing, StoreError> {
        self.snapshots
            .get::<Drawing>(Paths::NS_DRAWINGS, KIND_DRAWING, &id.to_string())?
            .map(|(d, _)| d)
            .ok_or_else(|| StoreError::DefinitionNotFound {
                kind: "drawing",
                id: id.to_string(),
            })
    }

    /// The drawings a filter admits, pinned first and newest first within
    /// that — from the index alone, no scene read.
    pub fn list_drawings(&self, filter: OwnerFilter) -> Result<Vec<DrawingSummary>, StoreError> {
        let (kind, scope_id) = filter.parts();
        let rows = self.idx().drawings_matching(kind, scope_id.as_deref())?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let Ok(id) = row.id.parse::<DrawingId>() else {
                continue;
            };
            let Some(scope) = OwnerScope::from_parts(&row.scope_kind, row.scope_id.as_deref())
            else {
                tracing::warn!(
                    "drawing {id}: bad scope in index: {} {:?}",
                    row.scope_kind,
                    row.scope_id
                );
                continue;
            };
            out.push(DrawingSummary {
                id,
                scope,
                title: row.title,
                pinned: row.pinned,
                hash: row.hash,
                element_count: row.element_count as usize,
                created_at: row.created_at,
                updated_at: row.updated_at,
            });
        }
        Ok(out)
    }

    pub fn create_drawing(&self, new: NewDrawing) -> Result<Drawing, StoreError> {
        let at = now_secs();
        let def = Drawing {
            id: DrawingId::from_ulid(mint_ulid()),
            scope: new.scope,
            title: new.title.trim().to_string(),
            pinned: false,
            created_at: at,
            updated_at: at,
            scene: new.scene.unwrap_or_default().without_deleted(),
        };
        self.write_drawing(&def)?;
        Ok(def)
    }

    /// The one writer of drawings at a time (`drawings_writes`): the canvas's
    /// autosave and the bridge's save each read, compare the hash and write.
    /// A poisoned lock is taken over, as the work items' is.
    fn drawing_writer(&self) -> MutexGuard<'_, ()> {
        self.drawings_writes.lock().unwrap_or_else(|poisoned| {
            tracing::warn!(
                "the drawings write lock was poisoned; continuing with the drawing as it stands"
            );
            poisoned.into_inner()
        })
    }

    /// Edit a drawing, refusing a scene that did not read the latest one.
    /// `base_hash` is [`scene_hash`] of the scene the caller last read, and
    /// it is required whenever `scene` is present. Read, checked and written
    /// under the one writer's lock. **A write that changes nothing writes
    /// nothing**: the same scene at the right hash, the same title, the same
    /// pin answer the record as it stands — no new revision, no new hash —
    /// so a canvas saving what it just adopted, or a bridge saving what the
    /// canvas already saved, moves nothing a reader would have to follow.
    pub fn update_drawing(
        &self,
        id: DrawingId,
        patch: DrawingPatch,
        base_hash: Option<&str>,
    ) -> Result<Drawing, StoreError> {
        let _writer = self.drawing_writer();
        let mut def = self.get_drawing(id)?;
        let mut moved = false;
        if let Some(scene) = patch.scene {
            let current = scene_hash(&def.scene);
            match base_hash {
                Some(h) if h == current => {}
                _ => {
                    return Err(StoreError::EditConflict {
                        what: "drawing",
                        current: serde_json::to_value(&def.scene).unwrap_or_default(),
                        current_hash: current,
                    })
                }
            }
            let next = scene.without_deleted();
            if scene_hash(&next) != current {
                def.scene = next;
                moved = true;
            }
        }
        if let Some(title) = patch.title {
            let title = title.trim().to_string();
            if title != def.title {
                def.title = title;
                moved = true;
            }
        }
        if let Some(pinned) = patch.pinned {
            if pinned != def.pinned {
                def.pinned = pinned;
                moved = true;
            }
        }
        if !moved {
            return Ok(def);
        }
        def.updated_at = now_secs();
        self.write_drawing(&def)?;
        Ok(def)
    }

    /// Remove a drawing: the file, the snapshot and the row, together.
    pub fn delete_drawing(&self, id: DrawingId) -> Result<(), StoreError> {
        let def = self.get_drawing(id)?;
        // Its conversations go with it: the picture they were about is gone.
        self.remove_conversations_of("drawing", &id.to_string())?;
        if let Ok(path) = self.drawing_path(&def.scope, id) {
            remove_file_if_there(&path)?;
        }
        let d = id.to_string();
        self.snapshots
            .delete_snapshot(Paths::NS_DRAWINGS, KIND_DRAWING, &d)?;
        self.idx().delete_drawing(&d)
    }

    /// A scope's drawings directory, snapshots and rows go together — the
    /// goal's, the project's, the workflow's or the channel's delete calls
    /// this, so the repository's next commit records the drawings leaving
    /// with what they were about.
    pub(crate) fn remove_drawings_of(
        &self,
        scope: OwnerScope,
        dir: &Path,
    ) -> Result<(), StoreError> {
        match std::fs::remove_dir_all(dir) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(StoreError::io(dir.display().to_string(), e)),
        }
        let rows = self
            .idx()
            .drawings_matching(Some(scope.kind()), scope.id().as_deref())?;
        for row in &rows {
            self.remove_conversations_of("drawing", &row.id)?;
            self.snapshots
                .delete_snapshot(Paths::NS_DRAWINGS, KIND_DRAWING, &row.id)?;
        }
        let idx = self.idx();
        idx.in_transaction(|| {
            for row in &rows {
                idx.delete_drawing(&row.id)?;
            }
            Ok(())
        })
    }

    /// Rebuild the `drawings` table — and the files — from the snapshots,
    /// which are the truth.
    pub(crate) fn reindex_drawings(&self) -> Result<(), StoreError> {
        for d in self.snapshots.list_ds(Paths::NS_DRAWINGS, KIND_DRAWING)? {
            let Ok(id) = d.parse::<DrawingId>() else {
                continue;
            };
            let Some(def) = self.tolerated_record("drawing", &d, self.get_drawing(id))? else {
                continue;
            };
            self.adopt_drawing(&def);
        }
        Ok(())
    }

    /// A record that arrived from a peer, or one read back at a rebuild:
    /// indexed and drawn into its file where its scope is known here. A
    /// drawing about a goal or a project this workspace does not hold is
    /// kept as its snapshot alone and said in the log — it lists once the
    /// record it is about arrives.
    pub(crate) fn adopt_drawing(&self, def: &Drawing) {
        if let Err(e) = def.validate() {
            tracing::warn!("drawing {}: refused: {e}", def.id);
            return;
        }
        if let Err(e) = self.index_drawing(def) {
            tracing::warn!("drawing {}: not indexed: {e}", def.id);
            return;
        }
        if let Err(e) = self.materialize(def) {
            tracing::warn!("drawing {}: not written to the repository: {e}", def.id);
        }
    }

    /// Every write: the record checked, the snapshot signed at the next
    /// revision and put on the bus, the row and the file made to match.
    fn write_drawing(&self, def: &Drawing) -> Result<(), StoreError> {
        def.validate()?;
        // The scope must be one this workspace holds before anything is signed.
        self.drawings_dir_for(&def.scope)?;
        let d = def.id.to_string();
        let revision = self
            .snapshots
            .current_revision(Paths::NS_DRAWINGS, KIND_DRAWING, &d)?
            + 1;
        let event = self.snapshots.put(
            Paths::NS_DRAWINGS,
            KIND_DRAWING,
            &d,
            def,
            revision,
            &self.owner,
            now_secs(),
            None,
            &[],
        )?;
        self.emit_store_event(StoreEvent::ConversationSnapshot {
            kind: KIND_DRAWING,
            d,
            event,
            audience: EventAudience::Workspace,
        });
        self.index_drawing(def)?;
        self.materialize(def)
    }

    fn index_drawing(&self, def: &Drawing) -> Result<(), StoreError> {
        let home_goal = match &def.scope {
            OwnerScope::Goal { id } => Some(id.to_string()),
            _ => None,
        };
        self.idx().upsert_drawing(&DrawingRow {
            id: def.id.to_string(),
            scope_kind: def.scope.kind().to_string(),
            scope_id: def.scope.id(),
            home_goal,
            title: def.title.clone(),
            pinned: def.pinned,
            hash: scene_hash(&def.scene),
            element_count: def.scene.element_count() as u64,
            created_at: def.created_at,
            updated_at: def.updated_at,
        })
    }

    /// The `.excalidraw` file: the standard envelope around the scene, and a
    /// `bisa` block naming the drawing — Excalidraw's reader ignores a key
    /// it does not know, so the file still opens anywhere, and the title is
    /// in the diff for the person and the commit message.
    fn materialize(&self, def: &Drawing) -> Result<(), StoreError> {
        let path = self.drawing_path(&def.scope, def.id)?;
        let file = serde_json::json!({
            "type": "excalidraw",
            "version": 2,
            "source": "bisa",
            "elements": def.scene.elements,
            "appState": {
                "viewBackgroundColor": def.scene.app_state.view_background_color,
                "gridModeEnabled": def.scene.app_state.grid,
            },
            "files": {},
            "bisa": {
                "id": def.id,
                "title": def.title,
                "scope": def.scope.kind(),
                "scope_id": def.scope.id(),
                "updated_at": def.updated_at,
            },
        });
        crate::paths::write_atomic(&path, &serde_json::to_vec_pretty(&file)?)
    }
}

fn remove_file_if_there(path: &Path) -> Result<(), StoreError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(StoreError::io(path.display().to_string(), e)),
    }
}
