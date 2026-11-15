//! Drawings (19 — Drawings): a picture on a canvas as a GEP record. The
//! **snapshot is the truth** — a drawing is an addressable event of kind
//! [`KIND_DRAWING`] in the `drawings/` namespace, like an addon's record —
//! signed so it can travel, though nothing sends one today — and the **file is the
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
        def.validate_for_write()?;
        self.write_drawing(&def)?;
        Ok(def)
    }

    /// The one writer of drawings at a time (`drawings_writes`): the canvas's
    /// autosave and the bridge's save each read, compare the hash and write.
    /// A deletion — one drawing's, or a whole scope's — is a write like any
    /// other: outside the lock, a save that had already read the drawing
    /// would sign it again after it went. A poisoned lock is taken over, as
    /// the work items' is.
    pub(crate) fn drawing_writer(&self) -> MutexGuard<'_, ()> {
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
    /// *The same scene* is the same elements and state whatever order their
    /// keys came in — the hash is of the bytes, and a canvas that wrote its
    /// keys back in another order has changed nothing.
    ///
    /// What is written is checked, field by field: a new scene as a write
    /// ([`Scene::validate_for_write`]), a new title as a title. A scene
    /// stored before a check existed is not held against a rename or a pin.
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
            if next != def.scene {
                next.validate_for_write()?;
                def.scene = next;
                moved = true;
            }
        }
        if let Some(title) = patch.title {
            let title = title.trim().to_string();
            if title != def.title {
                def.title = title;
                def.validate_title()?;
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

    /// Remove a drawing: the file, the snapshot and the row, together, under
    /// the one writer's lock. Where it is filed is read from its row, and
    /// from its snapshot only when it has none: a drawing whose snapshot
    /// will not read is listed, so it must be removable. Only a drawing with
    /// neither a row nor a snapshot is *not found*.
    ///
    /// Answers the scope it was filed under — `None` for the one nobody can
    /// say it of, a snapshot that will not read and has no row — so the
    /// deletion can be announced without the drawing having been read.
    pub fn delete_drawing(&self, id: DrawingId) -> Result<Option<OwnerScope>, StoreError> {
        let _writer = self.drawing_writer();
        let d = id.to_string();
        let row = self
            .idx()
            .drawing_scope(&d)?
            .and_then(|(kind, scope_id)| OwnerScope::from_parts(&kind, scope_id.as_deref()));
        let scope = match (row, self.get_drawing(id)) {
            (None, Err(e @ StoreError::DefinitionNotFound { .. })) => return Err(e),
            (Some(scope), _) => Some(scope),
            (None, read) => read.ok().map(|def| def.scope),
        };
        // Its conversations go with it: the picture they were about is gone.
        self.remove_conversations_of("drawing", &d)?;
        // A scope whose own record is gone has no folder to look in.
        if let Some(Ok(path)) = scope.as_ref().map(|s| self.drawing_path(s, id)) {
            remove_file_if_there(&path)?;
        }
        self.snapshots
            .delete_snapshot(Paths::NS_DRAWINGS, KIND_DRAWING, &d)?;
        self.idx().delete_drawing(&d)?;
        Ok(scope)
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
        let _writer = self.drawing_writer();
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
    /// record it is about arrives. Answers whether it is listed here now.
    pub(crate) fn adopt_drawing(&self, def: &Drawing) -> bool {
        if let Err(e) = def.validate() {
            tracing::warn!("drawing {}: refused: {e}", def.id);
            return false;
        }
        if let Err(e) = self.index_drawing(def) {
            // A drawing about a goal this workspace does not hold: its row
            // has no goal to belong to.
            tracing::warn!("drawing {}: not indexed: {e}", def.id);
            return false;
        }
        if let Err(e) = self.materialize(def) {
            tracing::warn!("drawing {}: not written to the repository: {e}", def.id);
        }
        true
    }

    /// Every write of a record its caller has checked: the snapshot signed
    /// at the next revision, the row made to match, the change put on the
    /// bus, the file drawn. In that order, so nothing is announced that a
    /// reader could not then find. The snapshot is the truth and the file
    /// its export: a file that cannot be written is said in the log and
    /// drawn again by the next write or rebuild — the save has happened, and
    /// answering it as failed would leave its caller holding a stale hash.
    fn write_drawing(&self, def: &Drawing) -> Result<(), StoreError> {
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
        self.index_drawing(def)?;
        self.emit_store_event(StoreEvent::ConversationSnapshot {
            kind: KIND_DRAWING,
            d,
            event,
            audience: EventAudience::Workspace,
        });
        if let Err(e) = self.materialize(def) {
            tracing::warn!("drawing {}: not written to the repository: {e}", def.id);
        }
        Ok(())
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

// added by the coverage pass: drawings_mod.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::MemoryKeyStore;
    use crate::owner::OwnerFilter;
    use bisa_core::OwnerScope;

    fn ws() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let ws =
            Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
        (dir, ws)
    }

    fn drawing(ws: &Workspace, scope: OwnerScope, title: &str) -> Drawing {
        ws.create_drawing(NewDrawing {
            scope,
            title: title.into(),
            scene: None,
        })
        .unwrap()
    }

    /// A drawing row that names no drawing, or a scope that is none, costs
    /// the list that row and is said; the rebuild walks past a snapshot
    /// whose name is no drawing's; the one writer recovers from poison.
    #[test]
    fn a_list_tolerates_bad_rows_and_a_rebuild_walks_past_a_stranger() {
        let (_d, ws) = ws();
        let kept = drawing(&ws, OwnerScope::Workspace, "Kept");
        let odd = drawing(&ws, OwnerScope::Workspace, "Odd");
        ws.idx()
            .execute_for_test(&format!(
                "UPDATE drawings SET id = 'not-a-drawing' WHERE id = '{}'",
                odd.id
            ))
            .unwrap();
        let listed = ws.list_drawings(OwnerFilter::All).unwrap();
        assert_eq!(
            listed.iter().map(|d| d.id).collect::<Vec<_>>(),
            vec![kept.id]
        );
        ws.rebuild_index().unwrap();
        ws.idx()
            .execute_for_test(&format!(
                "UPDATE drawings SET scope_kind = 'goal', scope_id = 'nope' WHERE id = '{}'",
                odd.id
            ))
            .unwrap();
        let listed = ws.list_drawings(OwnerFilter::All).unwrap();
        assert_eq!(
            listed.iter().map(|d| d.id).collect::<Vec<_>>(),
            vec![kept.id]
        );
        ws.snapshots
            .put(
                Paths::NS_DRAWINGS,
                KIND_DRAWING,
                "not-a-drawing",
                &serde_json::json!({}),
                0,
                ws.owner_keys(),
                1,
                None,
                &[],
            )
            .unwrap();
        ws.rebuild_index().unwrap();
        assert_eq!(ws.list_drawings(OwnerFilter::All).unwrap().len(), 2);
        let ws = std::sync::Arc::new(ws);
        let poisoner = std::sync::Arc::clone(&ws);
        let poisoned = std::thread::spawn(move || {
            let _writer = poisoner.drawing_writer();
            panic!("poison the drawings writer on purpose");
        })
        .join();
        assert!(poisoned.is_err());
        assert_eq!(drawing(&ws, OwnerScope::Workspace, "After").title, "After");
    }

    /// A drawing about a goal whose folder is gone is still deleted whole;
    /// what is adopted from a peer is refused, indexed or drawn as far as
    /// it can be, each said; a file that is not there is nothing to remove.
    #[test]
    fn a_drawing_of_a_goal_that_is_gone_is_deleted_and_a_peers_is_adopted_as_far_as_it_can_be() {
        let (_d, ws) = ws();
        let goal = ws
            .create_goal(crate::workspace::NewGoal::captured("drawn"))
            .unwrap();
        let of_goal = drawing(&ws, OwnerScope::Goal { id: goal.id }, "Of the goal");
        let goal_dir = ws.paths.goal(goal.id).dir().to_path_buf();
        let aside = goal_dir.with_file_name("aside");
        std::fs::rename(&goal_dir, &aside).unwrap();
        ws.delete_drawing(of_goal.id).unwrap();
        std::fs::rename(&aside, &goal_dir).unwrap();
        assert!(ws.get_drawing(of_goal.id).is_err());
        let mut too_big = drawing(&ws, OwnerScope::Workspace, "Big");
        too_big.scene.elements = vec![
            serde_json::json!({"id": "a", "type": "rectangle"});
            bisa_core::MAX_DRAWING_ELEMENTS + 1
        ];
        ws.adopt_drawing(&too_big);
        assert_eq!(
            ws.list_drawings(OwnerFilter::All).unwrap()[0].element_count,
            0,
            "the refused scene was not indexed"
        );
        assert!(remove_file_if_there(&goal_dir.join("nothing.excalidraw")).is_ok());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let file = ws.drawing_path(&OwnerScope::Workspace, too_big.id).unwrap();
            let drawings = file.parent().unwrap().to_path_buf();
            std::fs::set_permissions(&drawings, std::fs::Permissions::from_mode(0o500)).unwrap();
            let fresh = Drawing {
                id: DrawingId::from_ulid(mint_ulid()),
                scope: OwnerScope::Workspace,
                title: "From a peer".into(),
                pinned: false,
                created_at: 1,
                updated_at: 1,
                scene: Default::default(),
            };
            ws.adopt_drawing(&fresh);
            let unremovable = remove_file_if_there(&file);
            std::fs::set_permissions(&drawings, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert!(matches!(unremovable, Err(StoreError::Io { .. })));
            assert!(
                ws.list_drawings(OwnerFilter::All)
                    .unwrap()
                    .iter()
                    .any(|d| d.id == fresh.id),
                "indexed although not drawn"
            );
            std::fs::create_dir_all(goal_dir.join("drawings")).unwrap();
            std::fs::set_permissions(&goal_dir, std::fs::Permissions::from_mode(0o500)).unwrap();
            let unremovable =
                ws.remove_drawings_of(OwnerScope::Goal { id: goal.id }, &goal_dir.join("drawings"));
            std::fs::set_permissions(&goal_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert!(
                matches!(&unremovable, Err(StoreError::Io { .. })),
                "{unremovable:?}"
            );
        }
    }

    // --- the hardening pass ---

    fn scene_of(elements: Vec<serde_json::Value>) -> Scene {
        Scene {
            elements,
            ..Default::default()
        }
    }

    fn rect(id: &str) -> serde_json::Value {
        serde_json::json!({"id": id, "type": "rectangle", "x": 1, "y": 2, "width": 3, "height": 4})
    }

    fn snapshot_file(ws: &Workspace, id: DrawingId) -> PathBuf {
        ws.paths
            .state_dir(Paths::NS_DRAWINGS)
            .join(format!("{KIND_DRAWING}-{id}.json"))
    }

    /// A workspace of the same owner — the person's other machine.
    fn twin_of(alpha: &Workspace) -> (tempfile::TempDir, Workspace) {
        let keys = MemoryKeyStore::default();
        crate::KeyStore::set(
            &keys,
            "owner",
            &alpha.owner_keys().secret_key().to_secret_hex(),
        )
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let beta = Workspace::open_with_keystore(dir.path(), Box::new(keys)).unwrap();
        (dir, beta)
    }

    /// A save reads the drawing, then writes it back: a delete that landed
    /// between the two used to be undone by the write — the snapshot, the
    /// row and the file came back. Under the one writer's lock a delete
    /// waits its turn, and the save after it finds no drawing.
    #[test]
    fn a_delete_is_never_undone_by_a_save_that_was_in_flight() {
        let (_d, ws) = ws();
        for round in 0..40 {
            let d = drawing(&ws, OwnerScope::Workspace, "Racing");
            std::thread::scope(|s| {
                let ws = &ws;
                s.spawn(move || {
                    for i in 0..40 {
                        let renamed = ws.update_drawing(
                            d.id,
                            DrawingPatch {
                                title: Some(format!("Racing {i}")),
                                ..Default::default()
                            },
                            None,
                        );
                        if renamed.is_err() {
                            break;
                        }
                    }
                });
                s.spawn(move || {
                    std::thread::yield_now();
                    ws.delete_drawing(d.id).unwrap();
                });
            });
            assert!(
                ws.get_drawing(d.id).is_err(),
                "round {round}: the snapshot came back"
            );
            assert!(
                ws.list_drawings(OwnerFilter::All).unwrap().is_empty(),
                "round {round}: the row came back"
            );
            assert!(
                !ws.drawing_path(&OwnerScope::Workspace, d.id)
                    .unwrap()
                    .exists(),
                "round {round}: the file came back"
            );
        }
    }

    /// A drawing whose snapshot nobody can read is listed — its row stands —
    /// and so must be removable: the delete goes by where the row says the
    /// drawing is, never by reading it. One with no row and no snapshot is
    /// not found.
    #[test]
    fn a_drawing_whose_snapshot_will_not_read_can_still_be_deleted() {
        let (_d, ws) = ws();
        let torn = drawing(&ws, OwnerScope::Workspace, "Torn");
        let file = ws.drawing_path(&OwnerScope::Workspace, torn.id).unwrap();
        std::fs::write(snapshot_file(&ws, torn.id), b"{torn").unwrap();
        assert!(ws.get_drawing(torn.id).is_err());
        assert_eq!(ws.list_drawings(OwnerFilter::All).unwrap().len(), 1);
        ws.delete_drawing(torn.id).unwrap();
        assert!(ws.list_drawings(OwnerFilter::All).unwrap().is_empty());
        assert!(!file.exists() && !snapshot_file(&ws, torn.id).exists());
        assert!(matches!(
            ws.delete_drawing(torn.id),
            Err(StoreError::DefinitionNotFound {
                kind: "drawing",
                ..
            })
        ));
        // A snapshot with no row — a drawing about a goal this workspace
        // does not hold — is removed by what the snapshot says.
        let rowless = drawing(&ws, OwnerScope::Workspace, "Rowless");
        ws.idx().delete_drawing(&rowless.id.to_string()).unwrap();
        ws.delete_drawing(rowless.id).unwrap();
        assert!(!snapshot_file(&ws, rowless.id).exists());
    }

    /// The snapshot is the truth and the file its export: a save whose file
    /// cannot be written has still saved — answered as saved, listed at its
    /// new hash, said on the bus once — and the file is drawn again by the
    /// next write that can.
    #[cfg(unix)]
    #[test]
    fn a_save_whose_file_cannot_be_written_has_still_saved() {
        use std::os::unix::fs::PermissionsExt;
        let (_d, ws) = ws();
        let d = drawing(&ws, OwnerScope::Workspace, "Kept");
        let file = ws.drawing_path(&OwnerScope::Workspace, d.id).unwrap();
        let folder = file.parent().unwrap().to_path_buf();
        let mut rx = ws.subscribe_store_events();
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o500)).unwrap();
        let saved = ws.update_drawing(
            d.id,
            DrawingPatch {
                scene: Some(scene_of(vec![rect("a")])),
                ..Default::default()
            },
            Some(&scene_hash(&d.scene)),
        );
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
        let saved = saved.expect("the record was saved");
        assert_eq!(ws.get_drawing(d.id).unwrap(), saved);
        assert_eq!(
            ws.list_drawings(OwnerFilter::All).unwrap()[0].hash,
            scene_hash(&saved.scene)
        );
        assert_eq!(
            std::iter::from_fn(|| rx.try_recv().ok()).count(),
            1,
            "said once"
        );
        // The next write draws the file again.
        ws.update_drawing(
            d.id,
            DrawingPatch {
                pinned: Some(true),
                ..Default::default()
            },
            None,
        )
        .unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("\"a\""), "{text}");
    }

    /// A write is checked as a write: two elements under one id, a place no
    /// canvas can draw and a `javascript:` link are each refused with the
    /// element named, and nothing is written. A scene stored before the
    /// check that holds one still reads, lists after a rebuild, can be
    /// renamed and pinned, and is deleted.
    #[test]
    fn a_write_is_checked_as_a_write_and_what_was_stored_before_still_reads_and_deletes() {
        let (_d, ws) = ws();
        let d = drawing(&ws, OwnerScope::Workspace, "Checked");
        let h = scene_hash(&d.scene);
        let mut linked = rect("b");
        linked["link"] = serde_json::json!("javascript:alert(1)");
        let mut far = rect("c");
        far["y"] = serde_json::json!(4.0e15);
        for (elements, said) in [
            (vec![rect("a"), rect("a")], "share the id"),
            (vec![rect("a"), linked.clone()], "`javascript:` link"),
            (vec![far], "`y` of element"),
        ] {
            let refused = ws
                .update_drawing(
                    d.id,
                    DrawingPatch {
                        scene: Some(scene_of(elements)),
                        ..Default::default()
                    },
                    Some(&h),
                )
                .unwrap_err();
            assert!(refused.to_string().contains(said), "{refused}");
        }
        // A title is checked as a title: none, and one past the cap.
        for title in [
            "  ".to_string(),
            "t".repeat(bisa_core::draw::MAX_DRAWING_TITLE_CHARS + 1),
        ] {
            let refused = ws.update_drawing(
                d.id,
                DrawingPatch {
                    title: Some(title),
                    ..Default::default()
                },
                None,
            );
            assert!(refused.is_err(), "{refused:?}");
        }
        assert_eq!(ws.get_drawing(d.id).unwrap(), d, "nothing was written");
        assert!(ws
            .create_drawing(NewDrawing {
                scope: OwnerScope::Workspace,
                title: "Born bad".into(),
                scene: Some(scene_of(vec![linked.clone()])),
            })
            .is_err());

        // Stored before the check: put straight into the snapshot.
        let mut old = d.clone();
        old.scene = scene_of(vec![rect("a"), linked]);
        ws.snapshots
            .put(
                Paths::NS_DRAWINGS,
                KIND_DRAWING,
                &old.id.to_string(),
                &old,
                9,
                ws.owner_keys(),
                now_secs() + 5,
                None,
                &[],
            )
            .unwrap();
        ws.rebuild_index().unwrap();
        assert_eq!(ws.get_drawing(old.id).unwrap().scene, old.scene);
        let listed = ws.list_drawings(OwnerFilter::All).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].element_count, 2, "it still lists");
        let renamed = ws
            .update_drawing(
                old.id,
                DrawingPatch {
                    title: Some("Renamed".into()),
                    pinned: Some(true),
                    ..Default::default()
                },
                None,
            )
            .expect("a title and a pin are not a write of the scene");
        assert_eq!(renamed.scene, old.scene);
        ws.delete_drawing(old.id).unwrap();
        assert!(ws.list_drawings(OwnerFilter::All).unwrap().is_empty());
    }

    /// The same scene with its keys in another order is the same scene: a
    /// canvas that wrote its elements back otherwise moves nothing.
    #[test]
    fn a_save_of_the_same_scene_with_its_keys_in_another_order_writes_nothing() {
        let (_d, ws) = ws();
        let d = ws
            .create_drawing(NewDrawing {
                scope: OwnerScope::Workspace,
                title: "Ordered".into(),
                scene: Some(scene_of(vec![rect("a")])),
            })
            .unwrap();
        let reordered = serde_json::json!({"height": 4, "width": 3, "y": 2, "x": 1, "type": "rectangle", "id": "a"});
        assert_ne!(
            scene_hash(&scene_of(vec![reordered.clone()])),
            scene_hash(&d.scene),
            "the bytes differ"
        );
        let mut rx = ws.subscribe_store_events();
        let same = ws
            .update_drawing(
                d.id,
                DrawingPatch {
                    scene: Some(scene_of(vec![reordered])),
                    ..Default::default()
                },
                Some(&scene_hash(&d.scene)),
            )
            .unwrap();
        // The title it already has is no change either.
        let renamed_to_itself = ws
            .update_drawing(
                d.id,
                DrawingPatch {
                    title: Some(" Ordered ".into()),
                    ..Default::default()
                },
                None,
            )
            .unwrap();
        assert_eq!(renamed_to_itself, d);
        assert_eq!(same.updated_at, d.updated_at);
        assert_eq!(
            scene_hash(&same.scene),
            scene_hash(&d.scene),
            "the hash a reader holds still stands"
        );
        assert!(rx.try_recv().is_err(), "nothing was said");
    }

    /// What another machine sends is checked before it is stored: a record
    /// filed under an id that is not its own, and one whose scene a write
    /// here would refuse, are turned away with nothing kept and nothing
    /// said. A drawing about a goal this workspace does not hold is kept as
    /// its snapshot alone — not listed, and not announced as arrived.
    #[test]
    fn a_peers_drawing_is_checked_before_it_is_stored_and_said_only_when_it_landed() {
        let (_a, alpha) = ws();
        let (_b, beta) = twin_of(&alpha);
        let sent = |d: &str, def: &Drawing| {
            alpha
                .snapshots
                .put(
                    Paths::NS_DRAWINGS,
                    KIND_DRAWING,
                    d,
                    def,
                    1,
                    alpha.owner_keys(),
                    now_secs(),
                    None,
                    &[],
                )
                .unwrap()
        };
        let arrived = |rx: &mut tokio::sync::broadcast::Receiver<StoreEvent>| {
            std::iter::from_fn(|| rx.try_recv().ok())
                .filter(|e| matches!(e, StoreEvent::RemoteDrawingArrived { .. }))
                .count()
        };
        let base = Drawing {
            id: DrawingId::from_ulid(mint_ulid()),
            scope: OwnerScope::Workspace,
            title: "From the other machine".into(),
            pinned: false,
            created_at: 1,
            updated_at: 1,
            scene: scene_of(vec![rect("a")]),
        };
        let mut rx = beta.subscribe_store_events();

        // Filed under another id.
        let other = DrawingId::from_ulid(mint_ulid());
        let outcome = beta
            .ingest_remote_event(&sent(&other.to_string(), &base))
            .unwrap();
        assert!(
            format!("{outcome:?}").to_lowercase().contains("reject"),
            "{outcome:?}"
        );
        assert!(!snapshot_file(&beta, other).exists() && !snapshot_file(&beta, base.id).exists());

        // A scene a write here would refuse.
        let mut hostile = Drawing {
            id: DrawingId::from_ulid(mint_ulid()),
            ..base.clone()
        };
        hostile.scene.elements[0]["link"] = serde_json::json!("javascript:alert(1)");
        let outcome = beta
            .ingest_remote_event(&sent(&hostile.id.to_string(), &hostile))
            .unwrap();
        assert!(
            format!("{outcome:?}").to_lowercase().contains("reject"),
            "{outcome:?}"
        );
        assert!(
            !snapshot_file(&beta, hostile.id).exists(),
            "nothing was kept"
        );

        // Not a drawing at all.
        let junk = DrawingId::from_ulid(mint_ulid());
        let event = alpha
            .snapshots
            .put(
                Paths::NS_DRAWINGS,
                KIND_DRAWING,
                &junk.to_string(),
                &serde_json::json!({"not": "a drawing"}),
                1,
                alpha.owner_keys(),
                now_secs(),
                None,
                &[],
            )
            .unwrap();
        let outcome = beta.ingest_remote_event(&event).unwrap();
        assert!(
            format!("{outcome:?}").to_lowercase().contains("reject"),
            "{outcome:?}"
        );
        assert!(!snapshot_file(&beta, junk).exists());
        assert!(beta.list_drawings(OwnerFilter::All).unwrap().is_empty());
        assert_eq!(arrived(&mut rx), 0, "nothing landed, nothing said");

        // About a goal this workspace does not hold: kept, not listed, not said.
        let ghost = bisa_core::GoalId::from_ulid(ulid::Ulid::from_parts(9, 9));
        let of_goal = Drawing {
            id: DrawingId::from_ulid(mint_ulid()),
            scope: OwnerScope::Goal { id: ghost },
            ..base.clone()
        };
        beta.ingest_remote_event(&sent(&of_goal.id.to_string(), &of_goal))
            .unwrap();
        assert!(
            snapshot_file(&beta, of_goal.id).exists(),
            "kept for when the goal arrives"
        );
        assert!(beta.list_drawings(OwnerFilter::All).unwrap().is_empty());
        assert_eq!(arrived(&mut rx), 0);

        // And one that is sound lands and is said, once.
        beta.ingest_remote_event(&sent(&base.id.to_string(), &base))
            .unwrap();
        assert_eq!(beta.get_drawing(base.id).unwrap(), base);
        assert_eq!(beta.list_drawings(OwnerFilter::All).unwrap().len(), 1);
        assert_eq!(arrived(&mut rx), 1);
    }

    // added by the coverage pass: drawings.rs

    #[test]
    fn the_drawings_writer_recovers_from_poison() {
        let (_d, ws) = ws();
        let kept = drawing(&ws, OwnerScope::Workspace, "Kept");
        let ws = std::sync::Arc::new(ws);
        let poisoner = std::sync::Arc::clone(&ws);
        let poisoned = std::thread::spawn(move || {
            let _writer = poisoner.drawing_writer();
            panic!("poison the drawings writer on purpose");
        })
        .join();
        assert!(poisoned.is_err());
        let renamed = ws
            .update_drawing(
                kept.id,
                DrawingPatch {
                    title: Some("Kept still".into()),
                    ..Default::default()
                },
                None,
            )
            .unwrap();
        assert_eq!(renamed.title, "Kept still");
    }
}
