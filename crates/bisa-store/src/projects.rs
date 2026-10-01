//! Projects: the folders the workspace works in (kind 33409), and the
//! Goal ⇄ Project relation.
//!
//! A project is a **workspace citizen**. It has no owning goal — not a field,
//! not a column, not a path segment. Attaching a goal is one
//! symmetric row, recorded as a signed fact in the **goal's** journal
//! ([`JournalPayload::Attachment`]) so it syncs with the goal, rebuilds from
//! the journal, and leaves the project's directory naming no goal. Attaching
//! and detaching move no bytes (I14).
//!
//! Truth: `projects/<slug>/project.json`; snapshot events in
//! `projects/state/33409-<id>.json`; the `projects` and `goal_projects` index
//! tables are rebuildable.
//!
//! This module records projects. **Materializing the folder — mkdir, `git
//! init`, `git clone`, the recursive copy behind an import — is the engine's
//! job**, on an explicit user action; nothing here touches a working tree.

use crate::error::StoreError;
use crate::paths::{Paths, ProjectPaths};
use crate::workspace::{mint_ulid, now_secs, EventAudience, StoreEvent, Workspace};
use bisa_core::event::JournalPayload;
use bisa_core::kind::KIND_PROJECT;
use bisa_core::tags::TagEntity;
use bisa_core::{
    Assignee, Attachment, Goal, GoalId, PrincipalId, Project, ProjectId, ProjectOrigin,
    ProjectRoot, PublishPolicy, Slug, Tags, Vcs,
};
use std::path::PathBuf;

/// Fields a caller supplies when creating a project; the id and timestamps
/// are minted here.
#[derive(Clone, Debug)]
pub struct NewProject {
    pub slug: Slug,
    pub name: Option<String>,
    pub root: ProjectRoot,
    pub vcs: Vcs,
    /// Where it is being born. The engine derives this from the session or
    /// the route in hand; no request body carries it (I28a).
    pub origin: ProjectOrigin,
    pub assignees: Vec<Assignee>,
    pub publish: PublishPolicy,
    pub tags: Tags,
}

impl NewProject {
    /// A managed, non-git folder — the simplest project there is.
    pub fn managed(slug: &str) -> Result<Self, StoreError> {
        Ok(Self {
            slug: Slug::new(slug)?,
            name: None,
            root: ProjectRoot::Managed,
            vcs: Vcs::None,
            origin: ProjectOrigin::Workspace,
            assignees: vec![],
            publish: PublishPolicy::default(),
            tags: Tags::default(),
        })
    }
}

impl Workspace {
    /// The paths a project owns — always a workspace directory, whatever its
    /// root.
    pub fn project_paths(&self, project: &Project) -> ProjectPaths {
        self.paths.project(&project.slug)
    }

    /// Where a project's files live. `Managed` is the `tree/` under the
    /// project's directory; `External` is taken verbatim, because the whole
    /// point of adopting a folder is that it stays where it is.
    pub fn project_root_path(&self, project: &Project) -> PathBuf {
        match &project.root {
            ProjectRoot::Managed => self.project_paths(project).tree(),
            ProjectRoot::External { path } => PathBuf::from(path),
        }
    }

    fn write_project(&self, project: &Project) -> Result<(), StoreError> {
        let path = self.project_paths(project).record();
        crate::paths::write_atomic(&path, &serde_json::to_vec_pretty(project)?)?;
        let d = project.id.to_string();
        let existing_rev = self
            .snapshots
            .current_revision(Paths::NS_PROJECTS, KIND_PROJECT, &d)?;
        let event = self.snapshots.put(
            Paths::NS_PROJECTS,
            KIND_PROJECT,
            &d,
            project,
            existing_rev + 1,
            &self.owner,
            now_secs(),
            None,
            project.tags.as_slice(),
        )?;
        self.emit_store_event(StoreEvent::ConversationSnapshot {
            kind: KIND_PROJECT,
            d,
            event,
            audience: EventAudience::Workspace,
        });
        self.index_project(project)
    }

    pub(crate) fn index_project(&self, project: &Project) -> Result<(), StoreError> {
        let idx = self.idx();
        // The goal column says which goal, the three step columns which step
        // made it — under either variant; `origin` says which tab.
        let origin_goal = project.origin.goal().map(|g| g.to_string());
        let by_step = project.origin.step_ref();
        let (origin_run, origin_step, origin_workflow) = (
            by_step.map(|s| s.run.to_string()),
            by_step.map(|s| s.step.to_string()),
            by_step.map(|s| s.workflow.to_string()),
        );
        idx.upsert_project(&crate::index::ProjectRow {
            id: project.id.to_string(),
            slug: project.slug.to_string(),
            name: project.name.clone(),
            root_kind: project.root.kind().to_string(),
            root_path: match &project.root {
                ProjectRoot::External { path } => Some(path.clone()),
                ProjectRoot::Managed => None,
            },
            vcs: project.vcs.kind().to_string(),
            publish: project.publish.as_str().to_string(),
            origin: project.origin.as_str().to_string(),
            origin_goal,
            origin_run,
            origin_step,
            origin_workflow,
            archived_at: project.archived.map(|a| a.at),
            created_at: project.created_at,
        })?;
        idx.set_tags(
            TagEntity::Project,
            &project.id.to_string(),
            project.tags.as_slice(),
        )
    }

    fn check_slug_free(&self, slug: &Slug) -> Result<(), StoreError> {
        if Paths::RESERVED_PROJECT_SLUGS.contains(&slug.as_str()) {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-reserved-name-under-projects",
                slug = slug.to_string()
            )));
        }
        if self.paths.project(slug).record().exists() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-project-named-already-exists",
                slug = slug.to_string()
            )));
        }
        Ok(())
    }

    /// Record a project. Records only: the folder, `git init` and `git clone`
    /// are the engine's, on the user's action.
    pub fn create_project(&self, new: NewProject) -> Result<Project, StoreError> {
        self.check_slug_free(&new.slug)?;
        if new.name.as_deref().is_some_and(|n| n.trim().is_empty()) {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-project-name-must-not-be-blank"
            )));
        }
        if let ProjectRoot::External { path } = &new.root {
            if path.trim().is_empty() {
                return Err(StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-adopted-project-needs-path-folder-adopts"
                )));
            }
            self.check_folder_free(path)?;
        }
        self.vet_assignees(&new.assignees)?;
        let project = Project {
            id: ProjectId::from_ulid(mint_ulid()),
            name: new.name.unwrap_or_else(|| new.slug.to_string()),
            slug: new.slug,
            root: new.root,
            vcs: new.vcs,
            origin: new.origin,
            assignees: new.assignees,
            publish: new.publish,
            tags: new.tags,
            group: None,
            photo: None,
            revision: 1,
            archived: None,
            created_at: now_secs(),
        };
        self.write_project(&project)?;
        // The primary workstream is born with the project. Two
        // files, not one transaction: if the second write fails the first is
        // taken back, so no project ever exists without its primary.
        if let Err(e) = self.ensure_primary_workstream(&project) {
            if let Err(undo) = self.delete_project(project.id) {
                tracing::warn!(project = %project.slug, "could not undo a half-made project: {undo}");
            }
            return Err(e);
        }
        Ok(project)
    }

    /// A folder is one project's: adopting the folder another project —
    /// archived ones included — already adopted, a folder inside it, or one
    /// around it, is refused by that project's name. Two projects over one
    /// tree would each open checkouts, run scripts and settle commits in
    /// the other's files. Compared as written: the node hands a canonical
    /// path, and a path that cannot be compared is not a reason to refuse.
    fn check_folder_free(&self, path: &str) -> Result<(), StoreError> {
        let wanted = std::path::Path::new(path);
        let mut held = self.list_projects()?;
        held.extend(self.list_archived_projects()?);
        for other in held {
            let ProjectRoot::External { path: theirs } = &other.root else {
                continue;
            };
            let theirs = std::path::Path::new(theirs);
            // A key the message selects its words by, never the words.
            let how = if wanted == theirs {
                "same"
            } else if wanted.starts_with(theirs) {
                "inside"
            } else if theirs.starts_with(wanted) {
                "holds"
            } else {
                continue;
            };
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-folder-project-folder-one-project-s",
                a0 = (wanted.display()).to_string(),
                how = how.to_string(),
                a1 = format!("{:?}", other.name),
                a2 = (other.slug).to_string()
            )));
        }
        Ok(())
    }

    fn vet_assignees(&self, assignees: &[Assignee]) -> Result<(), StoreError> {
        for a in assignees {
            match a {
                Assignee::Agent(id) => {
                    self.get_agent(&bisa_core::AgentId::new(id)?)?;
                }
                Assignee::Team(id) => {
                    self.get_team(&bisa_core::TeamId::new(id)?)?;
                }
                Assignee::Human(_) => {}
            }
        }
        Ok(())
    }

    pub fn get_project(&self, id: ProjectId) -> Result<Project, StoreError> {
        let slug = self
            .idx()
            .project_slug(&id.to_string())?
            .ok_or_else(|| StoreError::ProjectNotFound(id.to_string()))?;
        self.read_project(&Slug::new(slug)?)
    }

    pub fn get_project_by_slug(&self, slug: &Slug) -> Result<Project, StoreError> {
        self.read_project(slug)
    }

    fn read_project(&self, slug: &Slug) -> Result<Project, StoreError> {
        let path = self.paths.project(slug).record();
        let bytes = std::fs::read(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StoreError::ProjectNotFound(slug.to_string())
            } else {
                StoreError::io(path.display().to_string(), e)
            }
        })?;
        // A record this build cannot read — origin-less, or carrying fields
        // this shape does not — is refused by name, never defaulted over,
        // exactly as a snapshot is.
        serde_json::from_slice(&bytes).map_err(|e| StoreError::unreadable(&path, "project", e))
    }

    /// The projects born of a goal — from the goal itself, or from a step of
    /// one of its runs. Provenance, not attachment: [`Workspace::projects_for`]
    /// answers who may see it.
    pub fn projects_born_of_goal(&self, goal: GoalId) -> Result<Vec<Project>, StoreError> {
        // The ids first, the guard released, then each record: `get_project`
        // locks the index itself.
        let ids = self.idx().projects_from_goal(&goal.to_string())?;
        ids.into_iter()
            .filter_map(|id| id.parse::<ProjectId>().ok())
            .map(|id| self.get_project(id))
            .collect()
    }

    /// The projects a workflow's steps made.
    pub fn projects_born_of_workflow(
        &self,
        workflow: bisa_core::WorkflowId,
    ) -> Result<Vec<Project>, StoreError> {
        let ids = self.idx().projects_from_workflow(&workflow.to_string())?;
        ids.into_iter()
            .filter_map(|id| id.parse::<ProjectId>().ok())
            .map(|id| self.get_project(id))
            .collect()
    }

    /// Every project in the workspace that is not archived, oldest first.
    pub fn list_projects(&self) -> Result<Vec<Project>, StoreError> {
        let ids = self.idx().list_project_ids()?;
        ids.into_iter()
            .filter_map(|id| id.parse::<ProjectId>().ok())
            .map(|id| self.get_project(id))
            .collect()
    }

    /// The projects put away, newest first — what a list shows only when asked.
    pub fn list_archived_projects(&self) -> Result<Vec<Project>, StoreError> {
        let ids = self.idx().list_archived_project_ids()?;
        ids.into_iter()
            .filter_map(|id| id.parse::<ProjectId>().ok())
            .map(|id| self.get_project(id))
            .collect()
    }

    /// Put a project away, or take it back out: out of the rail and the
    /// pickers, refused for an attachment while it is; the folder and every
    /// record stay. A new revision, like any write.
    pub fn set_project_archived(
        &self,
        id: ProjectId,
        archived: bool,
    ) -> Result<Project, StoreError> {
        let mut project = self.get_project(id)?;
        if project.is_archived() == archived {
            return Ok(project);
        }
        project.archived = archived.then(|| bisa_core::Archived::at(now_secs()));
        project.revision += 1;
        self.write_project(&project)?;
        Ok(project)
    }

    /// The projects attached to a goal that were not born of it — what a
    /// goal's deletion detaches and never touches.
    pub fn projects_attached_only(&self, goal: GoalId) -> Result<Vec<Project>, StoreError> {
        let born: std::collections::HashSet<ProjectId> = self
            .projects_born_of_goal(goal)?
            .into_iter()
            .map(|p| p.id)
            .collect();
        Ok(self
            .projects_for(goal)?
            .into_iter()
            .filter(|p| !born.contains(&p.id))
            .collect())
    }

    /// Persist an edited project, bumping the revision. The slug and the root
    /// are immutable here: both name a directory, and moving a folder is a
    /// filesystem operation the engine owns.
    pub fn update_project(&self, mut project: Project) -> Result<Project, StoreError> {
        let current = self.get_project(project.id)?;
        if current.slug != project.slug {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-cannot-rename-project-here-folder-must-move",
                a0 = (current.slug).to_string(),
                a1 = (project.slug).to_string()
            )));
        }
        if current.root != project.root {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-project-s-root-fixed-create-new-project"
            )));
        }
        if project.name.trim().is_empty() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-project-name-must-not-be-blank"
            )));
        }
        self.vet_assignees(&project.assignees)?;
        project.created_at = current.created_at;
        project.revision = current.revision + 1;
        self.write_project(&project)?;
        Ok(project)
    }

    /// Forget a project: its record, its snapshot, its rows, and every
    /// attachment (each detached with a journal fact on its goal). The files on
    /// disk are left alone — deleting someone's working tree is never an index
    /// operation.
    pub fn delete_project(&self, id: ProjectId) -> Result<(), StoreError> {
        let project = self.get_project(id)?;
        for goal in self.goals_of_project(id)? {
            self.detach(goal, id)?;
        }
        // Every workstream record goes with the project, the primary included —
        // the one path that may forget a primary. Checkouts stay on disk.
        for w in self.list_workstreams(crate::WorkstreamFilter::Project(id))? {
            self.forget_workstream_record(&project.slug, w.id)?;
        }
        let path = self.project_paths(&project).record();
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(StoreError::io(path.display().to_string(), e)),
        }
        // Its notes and drawings live in their repositories: they leave with
        // the record.
        let scope = bisa_core::OwnerScope::Project { id };
        let scoped = |base: std::path::PathBuf| {
            crate::paths::Paths::scoped_dir(&base, "project", Some(project.slug.as_str()))
        };
        self.remove_notes_of(scope.clone(), &scoped(self.paths.notes_dir()))?;
        self.remove_drawings_of(scope, &scoped(self.paths.drawings_dir()))?;
        // The conversations about the project go with it; each workstream's
        // went with its record above.
        self.remove_conversations_of("project", &id.to_string())?;
        self.snapshots
            .delete_snapshot(Paths::NS_PROJECTS, KIND_PROJECT, &id.to_string())?;
        let idx = self.idx();
        idx.delete_project(&id.to_string())?;
        idx.clear_tags(TagEntity::Project, &id.to_string())
    }

    // ------------------------------------------------------------------
    // The relation
    // ------------------------------------------------------------------

    /// Attach a project to a goal. Additive, reversible, idempotent, and it
    /// moves nothing.
    pub fn attach(&self, goal: GoalId, project: ProjectId) -> Result<Attachment, StoreError> {
        self.get_goal(goal)?;
        let p = self.get_project(project)?;
        if p.is_archived() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-project-archived-unarchive-before-attaching",
                a0 = (p.slug).to_string()
            )));
        }
        if let Some(existing) = self
            .attachments_of(goal)?
            .into_iter()
            .find(|a| a.project == project)
        {
            return Ok(existing);
        }
        let owner = self.owner.clone();
        self.append_journal(
            &bisa_core::Home::Goal { goal },
            JournalPayload::Attachment {
                project,
                attached: true,
            },
            &owner,
            None,
        )?;
        let at = now_secs();
        let by = self.owner_principal();
        self.idx()
            .attach(&goal.to_string(), &project.to_string(), at, by.as_hex())?;
        Ok(Attachment {
            goal,
            project,
            attached_at: at,
            attached_by: by,
        })
    }

    /// Detach a project from a goal. Nothing is deleted.
    pub fn detach(&self, goal: GoalId, project: ProjectId) -> Result<(), StoreError> {
        self.get_goal(goal)?;
        if !self
            .idx()
            .is_attached(&goal.to_string(), &project.to_string())?
        {
            return Ok(());
        }
        let owner = self.owner.clone();
        self.append_journal(
            &bisa_core::Home::Goal { goal },
            JournalPayload::Attachment {
                project,
                attached: false,
            },
            &owner,
            None,
        )?;
        self.idx().detach(&goal.to_string(), &project.to_string())
    }

    /// The attachments a goal holds, oldest first.
    pub fn attachments_of(&self, goal: GoalId) -> Result<Vec<Attachment>, StoreError> {
        let rows = self.idx().attachments_of_goal(&goal.to_string())?;
        rows.into_iter()
            .map(|(project, at, by)| {
                Ok(Attachment {
                    goal,
                    project: project.parse::<ProjectId>().map_err(|e| {
                        StoreError::Invalid(bisa_core::text!(
                            "error-store-invalid-bad-project-id-index",
                            e = e.to_string()
                        ))
                    })?,
                    attached_at: at,
                    attached_by: PrincipalId::new(by)?,
                })
            })
            .collect()
    }

    /// The projects attached to a goal. **One join**: a project is a
    /// workspace citizen, so there is nothing above it to inherit from.
    pub fn projects_for(&self, goal: GoalId) -> Result<Vec<Project>, StoreError> {
        let ids = self.idx().projects_of_goal(&goal.to_string())?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            let Ok(pid) = id.parse::<ProjectId>() else {
                continue;
            };
            match self.get_project(pid) {
                Ok(p) => out.push(p),
                Err(e) => tracing::warn!("project {pid} attached but unreadable: {e}"),
            }
        }
        Ok(out)
    }

    /// The goals a project is attached to.
    pub fn goals_of_project(&self, project: ProjectId) -> Result<Vec<GoalId>, StoreError> {
        Ok(self
            .idx()
            .goals_of_project(&project.to_string())?
            .into_iter()
            .filter_map(|id| id.parse().ok())
            .collect())
    }

    /// The goals a project is attached to, loaded.
    pub fn goals_for_project(&self, project: ProjectId) -> Result<Vec<Goal>, StoreError> {
        self.goals_of_project(project)?
            .into_iter()
            .map(|id| self.get_goal(id))
            .collect()
    }

    pub fn is_attached(&self, goal: GoalId, project: ProjectId) -> Result<bool, StoreError> {
        self.idx()
            .is_attached(&goal.to_string(), &project.to_string())
    }

    /// A project that arrived from a peer: write its record so it is a project
    /// here too, then index it. A slug already held by a *different* local
    /// project is a collision the peer's record loses — the one somebody
    /// chose here stays, and the remote one is left as a snapshot only.
    pub(crate) fn adopt_remote_project(&self, project: &Project) -> Result<(), StoreError> {
        let record = self.project_paths(project).record();
        if let Ok(existing) = self.read_project(&project.slug) {
            if existing.id != project.id {
                return Err(StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-remote-project-wants-slug-held-here",
                    a0 = (project.id).to_string(),
                    a1 = (project.slug).to_string(),
                    a2 = (existing.id).to_string()
                )));
            }
            if existing.revision >= project.revision {
                return self.index_project(project);
            }
        }
        crate::paths::write_atomic(&record, &serde_json::to_vec_pretty(project)?)?;
        self.index_project(project)
    }

    /// Rebuild support: repopulate `projects` from every `project.json` under
    /// `projects/`. Attachments come back from the goals' journals.
    pub(crate) fn reindex_projects(&self) -> Result<(), StoreError> {
        let dir = self.paths.projects_dir();
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(StoreError::io(dir.display().to_string(), e)),
        };
        for entry in entries.flatten() {
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(slug) = Slug::new(&name) else {
                continue; // `state/`, or something that is not a project
            };
            match self.read_project(&slug) {
                Ok(p) => self.index_project(&p)?,
                Err(StoreError::ProjectNotFound(_)) => {} // a folder with no record
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::MemoryKeyStore;

    fn ws() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let ws =
            Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
        (dir, ws)
    }

    fn adopting(slug: &str, folder: &std::path::Path) -> NewProject {
        NewProject {
            root: ProjectRoot::External {
                path: folder.to_string_lossy().into_owned(),
            },
            ..NewProject::managed(slug).unwrap()
        }
    }

    #[test]
    fn a_folder_is_one_projects_whether_the_same_inside_or_around() {
        let (_dir, ws) = ws();
        let outside = tempfile::tempdir().unwrap();
        let shop = outside.path().join("shop");
        let inner = shop.join("packages").join("web");
        let sibling = outside.path().join("shop-two");
        for dir in [&inner, &sibling] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let first = ws.create_project(adopting("shop", &shop)).unwrap();

        for (slug, folder, how) in [
            ("again", &shop, "is already"),
            ("web", &inner, "is inside"),
            ("everything", &outside.path().to_path_buf(), "holds"),
        ] {
            let refused = ws
                .create_project(adopting(slug, folder))
                .unwrap_err()
                .to_string();
            assert!(
                refused.contains(how) && refused.contains("\"shop\""),
                "{slug}: {refused}"
            );
            assert!(
                ws.get_project_by_slug(&Slug::new(slug).unwrap()).is_err(),
                "{slug} was not made"
            );
        }
        // A name that merely begins the same is another folder.
        ws.create_project(adopting("shop-two", &sibling)).unwrap();
        // An archived project still holds its folder; a deleted one lets it go.
        ws.set_project_archived(first.id, true).unwrap();
        assert!(ws.create_project(adopting("again", &shop)).is_err());
        ws.delete_project(first.id).unwrap();
        ws.create_project(adopting("again", &shop)).unwrap();
        // Managed projects hold no folder of the person's: any number may exist.
        ws.create_project(NewProject::managed("one").unwrap())
            .unwrap();
        ws.create_project(NewProject::managed("two").unwrap())
            .unwrap();
    }

    #[test]
    fn create_get_list_update_delete_with_no_goal_anywhere() {
        let (_dir, ws) = ws();
        let p = ws
            .create_project(NewProject {
                origin: bisa_core::ProjectOrigin::Workspace,
                slug: Slug::new("storefront").unwrap(),
                name: Some("Storefront".into()),
                root: ProjectRoot::Managed,
                vcs: Vcs::Git {
                    default_branch: "main".into(),
                    remote: None,
                    code_host: None,
                },
                assignees: vec![],
                publish: PublishPolicy::Gated,
                tags: Tags::default(),
            })
            .unwrap();
        assert_eq!(ws.list_goals(None).unwrap().len(), 0, "no goal was needed");
        assert_eq!(ws.get_project(p.id).unwrap(), p);
        assert_eq!(ws.get_project_by_slug(&p.slug).unwrap(), p);
        assert_eq!(ws.list_projects().unwrap(), vec![p.clone()]);
        assert_eq!(
            ws.project_root_path(&p),
            ws.paths().projects_dir().join("storefront").join("tree")
        );
        assert!(!ws.project_root_path(&p).exists(), "records only");

        let mut edited = p.clone();
        edited.name = "Store".into();
        let edited = ws.update_project(edited).unwrap();
        assert_eq!(edited.revision, p.revision + 1);
        let mut moved = p.clone();
        moved.root = ProjectRoot::External { path: "/x".into() };
        assert!(ws.update_project(moved).is_err(), "the root is fixed");

        ws.delete_project(p.id).unwrap();
        assert!(ws.get_project(p.id).is_err());
        assert!(ws.list_projects().unwrap().is_empty());
    }

    #[test]
    fn slugs_are_unique_workspace_wide_and_state_is_reserved() {
        let (_dir, ws) = ws();
        ws.create_project(NewProject::managed("notes").unwrap())
            .unwrap();
        assert!(ws
            .create_project(NewProject::managed("notes").unwrap())
            .is_err());
        assert!(ws
            .create_project(NewProject::managed("state").unwrap())
            .is_err());
        for bad in ["../escape", "", "Upper", "with space", "a/b"] {
            assert!(NewProject::managed(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn external_root_is_taken_verbatim() {
        let (_dir, ws) = ws();
        let p = ws
            .create_project(NewProject {
                origin: bisa_core::ProjectOrigin::Workspace,
                slug: Slug::new("legacy").unwrap(),
                name: None,
                root: ProjectRoot::External {
                    path: "/srv/legacy".into(),
                },
                vcs: Vcs::None,
                assignees: vec![],
                publish: PublishPolicy::Manual,
                tags: Tags::default(),
            })
            .unwrap();
        assert_eq!(ws.project_root_path(&p), PathBuf::from("/srv/legacy"));
        assert_eq!(p.name, "legacy", "name defaults to the slug");
        // Its record still lives in the workspace, never in the adopted folder.
        assert!(ws
            .project_paths(&p)
            .record()
            .starts_with(ws.paths().projects_dir()));
    }

    #[test]
    fn attaching_is_symmetric_idempotent_and_moves_nothing() {
        let (_dir, ws) = ws();
        let a = ws
            .create_goal(crate::workspace::NewGoal::captured("a"))
            .unwrap();
        let b = ws
            .create_goal(crate::workspace::NewGoal::captured("b"))
            .unwrap();
        let p = ws
            .create_project(NewProject::managed("landing").unwrap())
            .unwrap();
        let record_before = std::fs::read(ws.project_paths(&p).record()).unwrap();

        let att = ws.attach(a.id, p.id).unwrap();
        assert_eq!(att.attached_by, ws.owner_principal());
        assert_eq!(ws.attach(a.id, p.id).unwrap(), att, "idempotent");
        ws.attach(b.id, p.id).unwrap();
        assert_eq!(ws.projects_for(a.id).unwrap(), vec![p.clone()]);
        assert_eq!(ws.projects_for(b.id).unwrap(), vec![p.clone()]);
        assert_eq!(ws.goals_of_project(p.id).unwrap(), vec![a.id, b.id]);
        assert!(ws.is_attached(a.id, p.id).unwrap());
        assert_eq!(
            std::fs::read(ws.project_paths(&p).record()).unwrap(),
            record_before,
            "the project's record names no goal and did not change"
        );
        // The fact is in the goal's journal.
        assert!(ws
            .journal(&bisa_core::Home::Goal { goal: a.id })
            .unwrap()
            .iter()
            .any(|je| matches!(
                je.payload,
                JournalPayload::Attachment { project, attached: true } if project == p.id
            )));

        ws.detach(a.id, p.id).unwrap();
        assert!(ws.projects_for(a.id).unwrap().is_empty());
        assert_eq!(ws.goals_of_project(p.id).unwrap(), vec![b.id]);
        ws.detach(a.id, p.id).unwrap(); // idempotent
    }

    #[test]
    fn rebuild_restores_projects_and_attachments_from_truth() {
        let (_dir, ws) = ws();
        let a = ws
            .create_goal(crate::workspace::NewGoal::captured("a"))
            .unwrap();
        let p = ws
            .create_project(NewProject::managed("shared").unwrap())
            .unwrap();
        let q = ws
            .create_project(NewProject::managed("other").unwrap())
            .unwrap();
        ws.attach(a.id, p.id).unwrap();
        ws.attach(a.id, q.id).unwrap();
        ws.detach(a.id, q.id).unwrap();
        ws.rebuild_index().unwrap();
        assert_eq!(ws.get_project(p.id).unwrap(), p);
        assert_eq!(ws.list_projects().unwrap().len(), 2);
        assert_eq!(ws.projects_for(a.id).unwrap(), vec![p]);
    }

    #[test]
    fn deleting_a_project_detaches_it_everywhere_and_leaves_files_alone() {
        let (dir, ws) = ws();
        let a = ws
            .create_goal(crate::workspace::NewGoal::captured("a"))
            .unwrap();
        let p = ws
            .create_project(NewProject::managed("gone").unwrap())
            .unwrap();
        ws.attach(a.id, p.id).unwrap();
        let tree = ws.project_root_path(&p);
        std::fs::create_dir_all(&tree).unwrap();
        std::fs::write(tree.join("README"), b"keep me").unwrap();
        ws.delete_project(p.id).unwrap();
        assert!(ws.get_project(p.id).is_err());
        assert!(ws.projects_for(a.id).unwrap().is_empty());
        assert!(tree.join("README").exists(), "somebody's files stay");
        ws.rebuild_index().unwrap();
        assert!(ws.list_projects().unwrap().is_empty());
        assert!(ws.projects_for(a.id).unwrap().is_empty());
        drop(dir);
    }

    /// The three origins round trip through the record and the index, and a
    /// rebuild finds a project by each.
    #[test]
    fn a_project_records_workspace_goal_or_step_origin_and_the_index_finds_it_by_each() {
        let (_dir, ws) = ws();
        let goal = ws
            .create_goal(crate::workspace::NewGoal::captured("origin home"))
            .unwrap();
        let wf = ws
            .create_workflow(
                crate::workflows::tests::notify_workflow("Maker"),
                bisa_core::WorkflowOrigin::Workspace,
            )
            .unwrap();
        let run = bisa_core::RunId::from_ulid(ulid::Ulid::from_parts(5, 5));
        let step = bisa_core::StepId::new("build").unwrap();

        let mut by_hand = NewProject::managed("by-hand").unwrap();
        by_hand.origin = ProjectOrigin::Workspace;
        let mut from_goal = NewProject::managed("from-goal").unwrap();
        from_goal.origin = ProjectOrigin::from_goal(goal.id);
        let by_step = bisa_core::StepRef {
            run,
            step: step.clone(),
            workflow: wf.id,
        };
        let mut from_step = NewProject::managed("from-step").unwrap();
        from_step.origin = ProjectOrigin::Step {
            goal: Some(goal.id),
            step: by_step.clone(),
        };
        // A step of a run of the workspace: the workflow's, and no goal's.
        let mut from_workspace_run = NewProject::managed("from-workspace-run").unwrap();
        from_workspace_run.origin = ProjectOrigin::Step {
            goal: None,
            step: bisa_core::StepRef {
                run: bisa_core::RunId::from_ulid(ulid::Ulid::from_parts(5, 6)),
                step: step.clone(),
                workflow: wf.id,
            },
        };
        // A step of the goal's own design: the goal's, step named.
        let mut from_design = NewProject::managed("from-design").unwrap();
        from_design.origin = ProjectOrigin::Goal {
            goal: goal.id,
            step: Some(by_step),
        };
        let hand = ws.create_project(by_hand).unwrap();
        let of_goal = ws.create_project(from_goal).unwrap();
        let of_step = ws.create_project(from_step).unwrap();
        let of_design = ws.create_project(from_design).unwrap();
        let of_workspace_run = ws.create_project(from_workspace_run).unwrap();
        assert_eq!(hand.origin, ProjectOrigin::Workspace);
        assert_eq!(of_workspace_run.origin.goal(), None);

        let born_of_goal = ws.projects_born_of_goal(goal.id).unwrap();
        let ids: Vec<_> = born_of_goal.iter().map(|p| p.id).collect();
        assert!(
            ids.contains(&of_goal.id) && ids.contains(&of_step.id) && ids.contains(&of_design.id)
        );
        assert!(!ids.contains(&hand.id));
        assert!(!ids.contains(&of_workspace_run.id), "no goal made it");

        let mut born_of_wf: Vec<_> = ws
            .projects_born_of_workflow(wf.id)
            .unwrap()
            .iter()
            .map(|p| p.id)
            .collect();
        born_of_wf.sort();
        let mut want = vec![of_step.id, of_workspace_run.id];
        want.sort();
        assert_eq!(
            born_of_wf, want,
            "a design's project is the goal's, never filed under a workflow"
        );
        assert_eq!(
            ws.get_project(of_design.id)
                .unwrap()
                .origin
                .step_ref()
                .map(|s| s.step.to_string()),
            Some("build".into()),
            "the step that made it is kept"
        );

        // Origins are history: they survive a rebuild, and deleting the goal
        // deletes no project and blanks nothing.
        ws.rebuild_index().unwrap();
        let mut rebuilt: Vec<_> = ws
            .projects_born_of_workflow(wf.id)
            .unwrap()
            .iter()
            .map(|p| p.id)
            .collect();
        rebuilt.sort();
        assert_eq!(rebuilt, want);
        ws.delete_goal(goal.id).unwrap();
        let survivor = ws.get_project(of_step.id).unwrap();
        assert_eq!(
            survivor.origin.goal(),
            Some(goal.id),
            "history outlives the goal"
        );
    }

    /// A record written before projects carried an origin is refused by name,
    /// never defaulted over.
    #[test]
    fn an_origin_less_project_record_is_refused_as_unreadable() {
        let (_dir, ws) = ws();
        let p = ws
            .create_project(NewProject::managed("aging").unwrap())
            .unwrap();
        let record = ws.project_paths(&p).record();
        let mut json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&record).unwrap()).unwrap();
        json.as_object_mut().unwrap().remove("origin");
        crate::paths::write_atomic(&record, &serde_json::to_vec_pretty(&json).unwrap()).unwrap();
        let err = ws.get_project(p.id).unwrap_err();
        assert!(
            matches!(err, StoreError::Unreadable { .. }),
            "expected Unreadable, got {err}"
        );
    }
}
