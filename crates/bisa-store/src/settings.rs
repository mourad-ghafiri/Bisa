//! Settings storage: three scopes, three files, one registry.
//!
//! `machine.json` never syncs; `settings.json` and
//! `projects/<slug>/settings.json` do. Resolution and write-scope enforcement
//! are the core's ([`bisa_core::settings`]); this module only reads and
//! writes the layers.

use crate::error::StoreError;
use crate::problems::{ProblemKind, WorkspaceProblem};
use crate::workspace::Workspace;
use bisa_core::settings::{Layer, Resolved, Scope};
use bisa_core::ProjectId;
use std::path::PathBuf;

impl Workspace {
    fn settings_path(
        &self,
        scope: Scope,
        project: Option<ProjectId>,
    ) -> Result<PathBuf, StoreError> {
        Ok(match scope {
            Scope::Machine => self.paths.machine_settings(),
            Scope::Workspace => self.paths.workspace_settings(),
            Scope::Project => {
                let id = project.ok_or_else(|| {
                    StoreError::Invalid(bisa_core::text!(
                        "error-store-invalid-project-scope-settings-need-project"
                    ))
                })?;
                let p = self.get_project(id)?;
                self.paths.project(&p.slug).settings()
            }
        })
    }

    fn read_layer(&self, scope: Scope, project: Option<ProjectId>) -> Result<Layer, StoreError> {
        let path = self.settings_path(scope, project)?;
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| {
                StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-not-settings-file",
                    a0 = (path.display()).to_string(),
                    e = e.to_string()
                ))
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Layer::new()),
            Err(e) => Err(StoreError::io(path.display().to_string(), e)),
        }
    }

    /// Whether a read's refusal is *this file is not a settings file* —
    /// the one trouble a layer can have that the resolution works around.
    fn is_not_a_settings_file(e: &StoreError) -> bool {
        matches!(e, StoreError::Invalid(text) if text.id == "error-store-invalid-not-settings-file")
    }

    /// A layer for the resolution: one this build cannot read costs its
    /// values and is named once, never the whole resolution — an engine, a
    /// pump and a logger all read `settings()` at boot, and a torn
    /// `machine.json` must not be the reason none of them starts. The strict
    /// read stays for [`Self::settings_layer`], whose callers fail closed on
    /// their own terms.
    fn read_layer_tolerant(
        &self,
        scope: Scope,
        project: Option<ProjectId>,
    ) -> Result<Layer, StoreError> {
        match self.read_layer(scope, project) {
            Err(e) if Self::is_not_a_settings_file(&e) => {
                let path = self.settings_path(scope, project)?;
                self.record_problem(WorkspaceProblem::new(
                    ProblemKind::SettingsLayerUnreadable,
                    path.display().to_string(),
                    bisa_core::text!(
                        "error-store-problem-settings-layer",
                        scope = scope.as_str().to_string(),
                        path = path.display().to_string(),
                        reason = e.to_string()
                    ),
                    None,
                ));
                Ok(Layer::new())
            }
            other => other,
        }
    }

    /// A layer for a write: one this build cannot read is moved under
    /// `quarantine/` and the write starts from an empty layer, so a person
    /// can repair a broken scope from Settings instead of a text editor.
    fn read_layer_or_quarantine(
        &self,
        scope: Scope,
        project: Option<ProjectId>,
    ) -> Result<Layer, StoreError> {
        match self.read_layer(scope, project) {
            Err(e) if Self::is_not_a_settings_file(&e) => {
                let path = self.settings_path(scope, project)?;
                let to = self.paths.quarantine(&path, crate::workspace::now_secs())?;
                self.record_problem(WorkspaceProblem::new(
                    ProblemKind::Quarantined,
                    path.display().to_string(),
                    bisa_core::text!(
                        "error-store-problem-quarantined",
                        path = path.display().to_string(),
                        what = "settings file",
                        reason = e.to_string(),
                        to = to.display().to_string()
                    ),
                    Some(&to),
                ));
                Ok(Layer::new())
            }
            other => other,
        }
    }

    fn write_layer(
        &self,
        scope: Scope,
        project: Option<ProjectId>,
        layer: &Layer,
    ) -> Result<(), StoreError> {
        let path = self.settings_path(scope, project)?;
        crate::paths::write_atomic(&path, &serde_json::to_vec_pretty(layer)?)
    }

    /// Every setting, resolved for `project` (or with no project layer). A
    /// layer this build cannot read costs its values and is named in
    /// [`Workspace::problems`]; the resolution stands on the others.
    pub fn settings(&self, project: Option<ProjectId>) -> Result<Vec<Resolved>, StoreError> {
        let machine = self.read_layer_tolerant(Scope::Machine, None)?;
        let workspace = self.read_layer_tolerant(Scope::Workspace, None)?;
        let project_layer = match project {
            Some(id) => self.read_layer_tolerant(Scope::Project, Some(id))?,
            None => Layer::new(),
        };
        Ok(bisa_core::resolve_settings(&[
            (Scope::Project, &project_layer),
            (Scope::Workspace, &workspace),
            (Scope::Machine, &machine),
        ]))
    }

    /// One setting, resolved.
    pub fn setting(&self, key: &str, project: Option<ProjectId>) -> Result<Resolved, StoreError> {
        self.settings(project)?
            .into_iter()
            .find(|r| r.key == key)
            .ok_or_else(|| bisa_core::SettingsError::UnknownKey(key.to_string()).into())
    }

    /// The raw values held at one scope.
    pub fn settings_layer(
        &self,
        scope: Scope,
        project: Option<ProjectId>,
    ) -> Result<Layer, StoreError> {
        self.read_layer(scope, project)
    }

    /// Set `key` at `scope`. Refused — not stored and ignored — when the key
    /// does not allow that scope or the value is the wrong shape.
    pub fn set_setting(
        &self,
        scope: Scope,
        project: Option<ProjectId>,
        key: &str,
        value: serde_json::Value,
    ) -> Result<Resolved, StoreError> {
        bisa_core::check_setting_write(key, scope, &value)?;
        let _one_writer = self
            .settings_writes
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let mut layer = self.read_layer_or_quarantine(scope, project)?;
        layer.insert(key.to_string(), value);
        self.write_layer(scope, project, &layer)?;
        self.setting(key, project)
    }

    /// Remove `key` from one scope, so resolution falls through.
    pub fn unset_setting(
        &self,
        scope: Scope,
        project: Option<ProjectId>,
        key: &str,
    ) -> Result<Resolved, StoreError> {
        let _one_writer = self
            .settings_writes
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let mut layer = self.read_layer_or_quarantine(scope, project)?;
        layer.remove(key);
        self.write_layer(scope, project, &layer)?;
        self.setting(key, project)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::MemoryKeyStore;
    use crate::projects::NewProject;
    use bisa_core::settings::Origin;
    use serde_json::json;

    #[test]
    fn resolution_and_write_scope_enforcement_through_the_files() {
        let dir = tempfile::tempdir().unwrap();
        let ws =
            Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
        let p = ws
            .create_project(NewProject::managed("web").unwrap())
            .unwrap();
        let r = ws.setting("editor.tab_size", Some(p.id)).unwrap();
        assert_eq!(r.origin, Origin::Default);
        assert_eq!(r.value, json!(4));
        ws.set_setting(Scope::Workspace, None, "editor.tab_size", json!(2))
            .unwrap();
        let r = ws
            .set_setting(Scope::Project, Some(p.id), "editor.tab_size", json!(8))
            .unwrap();
        assert_eq!(r.origin, Origin::Project);
        assert_eq!(r.value, json!(8));
        assert_eq!(ws.setting("editor.tab_size", None).unwrap().value, json!(2));
        // A machine key cannot be set on a project.
        assert!(matches!(
            ws.set_setting(Scope::Project, Some(p.id), "editor.font_size", json!(14)),
            Err(StoreError::Settings(_))
        ));
        assert!(ws
            .set_setting(Scope::Machine, None, "nope.key", json!(1))
            .is_err());
        assert!(dir.path().join("settings.json").exists());
        assert!(ws.project_paths(&p).settings().exists());
        assert!(ws
            .set_setting(Scope::Project, None, "editor.tab_size", json!(8))
            .is_err());
        let r = ws
            .unset_setting(Scope::Project, Some(p.id), "editor.tab_size")
            .unwrap();
        assert_eq!(r.origin, Origin::Workspace);
        assert_eq!(ws.settings(None).unwrap().len(), bisa_core::SETTINGS.len());
    }
}
