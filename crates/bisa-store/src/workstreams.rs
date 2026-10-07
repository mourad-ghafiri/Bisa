//! Workstreams: one checkout occupant of a project — a git worktree
//! on its own branch, a copy for a non-git project, or the
//! **primary**: the project's own root checkout, exactly one per project,
//! born with it, never deleted on its own.
//!
//! **Workstreams are local state, never GEP.** A workstream stands for a
//! directory on one machine, so there is no kind, no snapshot and no store
//! event that syncs. What *is* true everywhere — the branch was pushed, PR #12
//! opened — is journaled as a progress fact by the engine.
//!
//! **No path is stored.** [`Workspace::workstream_checkout`] is the one place
//! a record becomes a directory: the primary is the project root, everything
//! else is `projects/<slug>/workstreams/<id>/`. A record therefore cannot
//! point outside its project.
//!
//! Its lifecycle moves only through [`Workspace::transition_workstream`], which
//! calls [`Workstream::apply`].
//!
//! Truth: `projects/<slug>/workstreams/<id>.json`; the `workstreams` index table
//! is a rebuildable locator.

use crate::error::StoreError;
use crate::workspace::Workspace;
use bisa_core::{
    GoalId, Project, ProjectId, RunId, Slug, WorkItemId, Workstream, WorkstreamId, WorkstreamKind,
    WorkstreamState, WorkstreamTransition,
};
use std::path::PathBuf;

/// Which workstreams to list. One column, one filter.
#[derive(Clone, Copy, Debug)]
pub enum WorkstreamFilter {
    All,
    Project(ProjectId),
    Goal(GoalId),
    /// The ones opened for a run's work items — how a run of the
    /// workspace, which no goal holds, finds what it opened.
    Run(RunId),
    WorkItem(WorkItemId),
}

/// The refusal every path that would remove the primary gets — the store's,
/// the engine's and the node's alike (`error-store-invalid-primary-is-the-project`).
pub fn primary_is_the_project() -> bisa_core::Text {
    bisa_core::text!("error-store-invalid-primary-is-the-project")
}

impl Workspace {
    fn workstream_slug(&self, project: ProjectId) -> Result<Slug, StoreError> {
        Ok(self.get_project(project)?.slug)
    }

    fn workstream_record_path(&self, slug: &Slug, id: WorkstreamId) -> PathBuf {
        self.paths.project(slug).workstream_record(id)
    }

    /// Where a workstream's checkout is — **the one place a record becomes a
    /// directory.** The primary is the project's resolved root (outside the
    /// workspace for an adopted project); a worktree or a copy lives beside
    /// its record under the project's `workstreams/`.
    pub fn workstream_checkout(&self, workstream: &Workstream) -> Result<PathBuf, StoreError> {
        let project = self.get_project(workstream.project)?;
        Ok(self.checkout_in(&project, workstream))
    }

    /// [`Self::workstream_checkout`] with the project already in hand.
    pub fn checkout_in(&self, project: &Project, workstream: &Workstream) -> PathBuf {
        match workstream.kind {
            WorkstreamKind::Primary => self.project_root_path(project),
            WorkstreamKind::Worktree { .. } | WorkstreamKind::Copy => self
                .paths
                .project(&project.slug)
                .workstream_dir(workstream.id),
        }
    }

    /// The project's primary workstream — its root checkout as a record.
    pub fn primary_workstream(&self, project: ProjectId) -> Result<Workstream, StoreError> {
        self.get_workstream(WorkstreamId::primary_of(project))
    }

    fn index_workstream(&self, w: &Workstream) -> Result<(), StoreError> {
        self.idx().upsert_workstream(
            &w.id.to_string(),
            &w.project.to_string(),
            w.kind.as_str(),
            w.name.as_deref(),
            w.goal.map(|g| g.to_string()).as_deref(),
            w.work_item.map(|i| i.to_string()).as_deref(),
            w.branch(),
            w.agent.as_deref(),
            w.state.as_str(),
            w.created_at,
        )
    }

    /// Write (or edit) a workstream record. **Not its state**: an existing
    /// record whose state differs is refused — state moves only through
    /// [`Self::transition_workstream`]. The checkout itself is the engine's.
    ///
    /// A primary must carry its project's own id, and a record carrying that
    /// id must be a primary: the two are one fact, and this is where it is
    /// enforced for every writer.
    pub fn put_workstream(&self, workstream: &Workstream) -> Result<(), StoreError> {
        let slug = self.workstream_slug(workstream.project)?;
        let primary_id = WorkstreamId::primary_of(workstream.project);
        if workstream.is_primary() != (workstream.id == primary_id) {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-workstream-primary-project-s-id-one-fact",
                a0 = (workstream.id).to_string()
            )));
        }
        if let Some(goal) = workstream.goal {
            self.get_goal(goal)?;
        }
        if let Ok(stored) = self.read_workstream(&slug, workstream.id) {
            if stored.state != workstream.state {
                return Err(StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-workstream-state-changes-go-through-transition-workstream",
                    a0 = (workstream.id).to_string()
                )));
            }
            if stored.kind.as_str() != workstream.kind.as_str() {
                return Err(StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-workstream-workstream-s-kind-fixed",
                    a0 = (workstream.id).to_string()
                )));
            }
        }
        self.write_workstream(&slug, workstream)
    }

    /// Edit what a person may change about a workstream — its name, its note,
    /// whether it is pinned, and its place on the Board. Everything else on
    /// the record is the engine's or the state machine's, and a change to it
    /// is refused by name.
    pub fn update_workstream(&self, edited: &Workstream) -> Result<Workstream, StoreError> {
        let current = self.get_workstream(edited.id)?;
        let fixed = Workstream {
            name: current.name.clone(),
            note: current.note.clone(),
            pinned: current.pinned,
            board: current.board.clone(),
            ..edited.clone()
        };
        if fixed != current {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-workstream-only-name-note-pinned-board-can",
                a0 = (edited.id).to_string()
            )));
        }
        if edited.name.as_deref().is_some_and(|n| n.trim().is_empty()) {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-workstream-s-name-must-not-be-blank"
            )));
        }
        let slug = self.workstream_slug(edited.project)?;
        self.write_workstream(&slug, edited)?;
        Ok(edited.clone())
    }

    /// The primary record, born with its project. Called by `create_project`
    /// and nowhere else: a workspace is written by one shape of the code, and
    /// there is no path that patches an older one up.
    pub(crate) fn ensure_primary_workstream(
        &self,
        project: &Project,
    ) -> Result<Workstream, StoreError> {
        let id = WorkstreamId::primary_of(project.id);
        match self.read_workstream(&project.slug, id) {
            Ok(existing) => return Ok(existing),
            Err(StoreError::WorkstreamNotFound(_)) => {}
            Err(e) => return Err(e),
        }
        let primary = Workstream {
            id,
            project: project.id,
            name: None,
            note: None,
            pinned: false,
            kind: WorkstreamKind::Primary,
            goal: None,
            work_item: None,
            agent: None,
            state: WorkstreamState::Open,
            created_at: project.created_at,
            board: Default::default(),
        };
        self.write_workstream(&project.slug, &primary)?;
        Ok(primary)
    }

    fn write_workstream(&self, slug: &Slug, workstream: &Workstream) -> Result<(), StoreError> {
        let path = self.workstream_record_path(slug, workstream.id);
        crate::paths::write_atomic(&path, &serde_json::to_vec_pretty(workstream)?)?;
        self.index_workstream(workstream)
    }

    pub fn get_workstream(&self, id: WorkstreamId) -> Result<Workstream, StoreError> {
        let project = self
            .idx()
            .workstream_project(&id.to_string())?
            .ok_or_else(|| StoreError::WorkstreamNotFound(id.to_string()))?
            .parse::<ProjectId>()
            .map_err(|e| {
                StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-bad-project-id-index",
                    e = e.to_string()
                ))
            })?;
        let slug = self.workstream_slug(project)?;
        self.read_workstream(&slug, id)
    }

    fn read_workstream(&self, slug: &Slug, id: WorkstreamId) -> Result<Workstream, StoreError> {
        let path = self.workstream_record_path(slug, id);
        let bytes = std::fs::read(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StoreError::WorkstreamNotFound(id.to_string())
            } else {
                StoreError::io(path.display().to_string(), e)
            }
        })?;
        serde_json::from_slice(&bytes)
            .map_err(|e| StoreError::unreadable(&path, "workstream record", e))
    }

    /// Workstreams matching a filter: each project's primary first, then
    /// oldest first.
    pub fn list_workstreams(
        &self,
        filter: WorkstreamFilter,
    ) -> Result<Vec<Workstream>, StoreError> {
        let (column, value) = match filter {
            WorkstreamFilter::All => (None, None),
            WorkstreamFilter::Project(p) => (Some("project_id"), Some(p.to_string())),
            WorkstreamFilter::Goal(g) => (Some("goal_id"), Some(g.to_string())),
            WorkstreamFilter::Run(r) => (Some("run_id"), Some(r.to_string())),
            WorkstreamFilter::WorkItem(w) => (Some("work_item"), Some(w.to_string())),
        };
        let locators = self.idx().workstream_locators(column, value.as_deref())?;
        let mut out = Vec::with_capacity(locators.len());
        for (id, project) in locators {
            let id = id.parse::<WorkstreamId>().map_err(|e| {
                StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-bad-workstream-id-index",
                    e = e.to_string()
                ))
            })?;
            let project = project.parse::<ProjectId>().map_err(|e| {
                StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-bad-project-id-index",
                    e = e.to_string()
                ))
            })?;
            let slug = self.workstream_slug(project)?;
            out.push(self.read_workstream(&slug, id)?);
        }
        Ok(out)
    }

    /// Move a workstream. **The only writer of a workstream's state.**
    pub fn transition_workstream(
        &self,
        id: WorkstreamId,
        transition: &WorkstreamTransition,
    ) -> Result<Workstream, StoreError> {
        let mut w = self.get_workstream(id)?;
        w.state = w.apply(transition)?;
        let slug = self.workstream_slug(w.project)?;
        self.write_workstream(&slug, &w)?;
        Ok(w)
    }

    /// Forget a workstream record. The checkout on disk is left alone — an
    /// unpushed branch is the only copy of someone's work. The primary is
    /// refused: it goes only with its project ([`Self::delete_project`]).
    pub fn delete_workstream(&self, id: WorkstreamId) -> Result<(), StoreError> {
        let w = self.get_workstream(id)?;
        if w.is_primary() {
            return Err(StoreError::Invalid(primary_is_the_project()));
        }
        let slug = self.workstream_slug(w.project)?;
        self.forget_workstream_record(&slug, id)
    }

    /// Remove one record file and its row, primary or not. `delete_project`'s
    /// half; every other caller goes through [`Self::delete_workstream`].
    pub(crate) fn forget_workstream_record(
        &self,
        slug: &Slug,
        id: WorkstreamId,
    ) -> Result<(), StoreError> {
        let path = self.workstream_record_path(slug, id);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(StoreError::io(path.display().to_string(), e)),
        }
        // The conversations in the checkout go with its record.
        self.remove_conversations_of("workstream", &id.to_string())?;
        self.idx().delete_workstream(&id.to_string())
    }

    /// Rebuild support: repopulate the `workstreams` table from every project's
    /// `workstreams/*.json`. Read-only on the truth.
    pub(crate) fn reindex_workstreams(&self) -> Result<(), StoreError> {
        for project in self.list_projects()? {
            let dir = self.project_paths(&project).workstreams();
            let entries = match std::fs::read_dir(&dir) {
                Ok(e) => e,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(StoreError::io(dir.display().to_string(), e)),
            };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !name.ends_with(".json") {
                    continue;
                }
                let path = entry.path();
                let bytes = std::fs::read(&path)
                    .map_err(|e| StoreError::io(path.display().to_string(), e))?;
                // A record written by another shape of the code is named and
                // skipped — the rebuild goes on; a single read of it still
                // refuses by name.
                let Some(w) = self.tolerated_record::<Workstream>(
                    "workstream record",
                    &path.display().to_string(),
                    serde_json::from_slice(&bytes)
                        .map_err(|e| StoreError::unreadable(&path, "workstream record", e)),
                )?
                else {
                    continue;
                };
                self.index_workstream(&w)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::MemoryKeyStore;
    use crate::index::SessionRow;
    use crate::projects::NewProject;
    use crate::workspace::now_secs;
    use bisa_core::{ProjectRoot, WorkstreamState};

    fn ws() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let w =
            Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
        (dir, w)
    }

    /// A worktree record the way the engine builds one: the id minted first,
    /// so the checkout can be created under it before the record is written.
    fn workstream(project: ProjectId, goal: Option<GoalId>, branch: &str) -> Workstream {
        Workstream {
            id: WorkstreamId::from_ulid(crate::workspace::mint_ulid()),
            project,
            name: None,
            note: None,
            pinned: false,
            kind: WorkstreamKind::Worktree {
                branch: branch.into(),
                base: "main".into(),
            },
            goal,
            work_item: None,
            agent: Some("developer".into()),
            state: WorkstreamState::Open,
            created_at: now_secs(),
            board: Default::default(),
        }
    }

    #[test]
    fn an_unreadable_workstream_record_costs_its_own_row_and_is_named() {
        let (_dir, ws) = ws();
        let p = ws
            .create_project(NewProject::managed("app").unwrap())
            .unwrap();
        // A record written by another shape of the code: a path and a
        // backing, no kind. Nothing converts it; the rebuild names it and
        // goes on, so an upgrade never turns one torn record into a
        // workspace that will not open.
        let id = WorkstreamId::from_ulid(ulid::Ulid::from_parts(3, 1));
        let path = ws
            .project_paths(&p)
            .workstreams()
            .join(format!("{id}.json"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            format!(
                r#"{{"id":"{id}","project":"{}","path":"/x","backing":{{"type":"copy"}}}}"#,
                p.id
            ),
        )
        .unwrap();
        ws.reindex_workstreams().expect("the rebuild goes on");
        let problems = ws.problems();
        let named = problems
            .iter()
            .find(|p| p.path == path.display().to_string())
            .unwrap_or_else(|| panic!("the record is named: {problems:?}"));
        assert_eq!(named.kind, crate::problems::ProblemKind::RebuildSkipped);
        let text = named.text.to_string();
        assert!(text.contains("not a workstream record"), "{text}");
        // The primary workstream is still there: one record cost its own row.
        assert!(ws.primary_workstream(p.id).is_ok());
    }

    #[test]
    fn crud_list_and_transitions() {
        let (_dir, ws) = ws();
        let goal = ws
            .create_goal(crate::workspace::NewGoal::captured("build it"))
            .unwrap();
        let p = ws
            .create_project(NewProject::managed("app").unwrap())
            .unwrap();
        let q = ws
            .create_project(NewProject::managed("other").unwrap())
            .unwrap();

        let mut a = workstream(p.id, Some(goal.id), "feature/checkout");
        let item = bisa_core::WorkItemSpec {
            id: WorkItemId::from_ulid(ulid::Ulid::from_parts(2, 1)),
            home: bisa_core::Home::Goal { goal: goal.id },
            run: None,
            step: None,
            instructions: "build".into(),
            state: bisa_core::WorkItemState::Open,
            project: Some(p.id),
            harness_candidates: vec!["mock".into()],
            model: None,
            effort: None,
            output_schema: None,
            budget: Default::default(),
            assignees: vec![],
            tier_ceiling: bisa_core::ToolTier::Write,
            agent: None,
            spawn_allowlist: vec![],
            depth_budget: 0,
            result_attempts: 0,
            interruptions: 0,
        };
        ws.put_work_item(&item).unwrap();
        a.work_item = Some(item.id);
        let b = workstream(p.id, Some(goal.id), "fix/cart-total");
        let c = workstream(q.id, None, "ide/branch");
        for w in [&a, &b, &c] {
            ws.put_workstream(w).unwrap();
        }
        assert_eq!(ws.get_workstream(a.id).unwrap(), a);
        // Three worktrees plus each project's primary.
        assert_eq!(ws.list_workstreams(WorkstreamFilter::All).unwrap().len(), 5);
        assert_eq!(
            ws.list_workstreams(WorkstreamFilter::Project(p.id))
                .unwrap()
                .len(),
            3
        );
        assert_eq!(
            ws.list_workstreams(WorkstreamFilter::Goal(goal.id))
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            ws.list_workstreams(WorkstreamFilter::WorkItem(a.work_item.unwrap()))
                .unwrap(),
            vec![a.clone()]
        );
        // A goal-less workstream is a first-class record.
        assert_eq!(ws.get_workstream(c.id).unwrap().goal, None);
        assert!(ws
            .workstream_checkout(&c)
            .unwrap()
            .starts_with(ws.project_paths(&q).workstreams()));

        // State moves only through transitions, and the table decides.
        let mut sneaky = a.clone();
        sneaky.state = WorkstreamState::Merged {
            number: 12,
            url: "https://example.invalid/pr/12".into(),
        };
        assert!(ws.put_workstream(&sneaky).is_err());
        assert!(matches!(
            ws.transition_workstream(a.id, &WorkstreamTransition::Merged),
            Err(StoreError::Workstream(_))
        ));
        ws.transition_workstream(a.id, &WorkstreamTransition::Committed)
            .unwrap();
        ws.transition_workstream(a.id, &WorkstreamTransition::Pushed)
            .unwrap();
        let moved = ws
            .transition_workstream(
                a.id,
                &WorkstreamTransition::PrOpened {
                    number: 12,
                    url: "https://example.invalid/pr/12".into(),
                },
            )
            .unwrap();
        assert!(moved.state.is_published());
        let merged = ws
            .transition_workstream(a.id, &WorkstreamTransition::Merged)
            .unwrap();
        assert!(matches!(
            merged.state,
            WorkstreamState::Merged { number: 12, .. }
        ));

        ws.delete_workstream(b.id).unwrap();
        assert!(ws.get_workstream(b.id).is_err());
        assert_eq!(ws.list_workstreams(WorkstreamFilter::All).unwrap().len(), 4);
    }

    #[test]
    fn creating_a_project_creates_its_primary_and_lists_it_first() {
        let (_dir, ws) = ws();
        let p = ws
            .create_project(NewProject::managed("app").unwrap())
            .unwrap();
        let primary = ws.primary_workstream(p.id).unwrap();
        assert_eq!(primary.id, WorkstreamId::primary_of(p.id));
        assert_eq!(primary.id.to_string(), p.id.to_string());
        assert!(primary.is_primary());
        assert_eq!(primary.state, WorkstreamState::Open);
        assert_eq!(primary.created_at, p.created_at);

        // Worktrees made before and after: the primary still comes first.
        let older = Workstream {
            created_at: primary.created_at.saturating_sub(100),
            board: Default::default(),
            ..workstream(p.id, None, "feature/older")
        };
        ws.put_workstream(&older).unwrap();
        ws.put_workstream(&workstream(p.id, None, "feature/newer"))
            .unwrap();
        let listed = ws
            .list_workstreams(WorkstreamFilter::Project(p.id))
            .unwrap();
        assert_eq!(listed.len(), 3);
        assert!(listed[0].is_primary(), "the primary is hoisted");
        assert_eq!(listed[1].id, older.id);
    }

    #[test]
    fn the_primary_and_the_project_id_are_one_fact() {
        let (_dir, ws) = ws();
        let p = ws
            .create_project(NewProject::managed("app").unwrap())
            .unwrap();
        // A second "primary" under a fresh id is refused …
        let impostor = Workstream {
            kind: WorkstreamKind::Primary,
            ..workstream(p.id, None, "x")
        };
        assert!(ws.put_workstream(&impostor).is_err());
        // … and so is a worktree wearing the project's id.
        let squatter = Workstream {
            id: WorkstreamId::primary_of(p.id),
            ..workstream(p.id, None, "x")
        };
        assert!(ws.put_workstream(&squatter).is_err());
    }

    #[test]
    fn the_primary_is_deleted_only_with_its_project() {
        let (_dir, ws) = ws();
        let p = ws
            .create_project(NewProject::managed("app").unwrap())
            .unwrap();
        let primary = ws.primary_workstream(p.id).unwrap();
        let err = ws.delete_workstream(primary.id).unwrap_err();
        assert!(
            err.to_string().contains("remove the project instead"),
            "{err}"
        );
        assert!(ws.get_workstream(primary.id).is_ok());

        let record = ws.paths().project(&p.slug).workstream_record(primary.id);
        assert!(record.is_file());
        ws.delete_project(p.id).unwrap();
        assert!(!record.exists(), "the record goes with the project");
        assert!(ws.get_workstream(primary.id).is_err());
        assert!(ws
            .list_workstreams(WorkstreamFilter::All)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn checkouts_resolve_from_kind_not_from_a_stored_path() {
        let (dir, ws) = ws();
        let managed = ws
            .create_project(NewProject::managed("app").unwrap())
            .unwrap();
        let primary = ws.primary_workstream(managed.id).unwrap();
        assert_eq!(
            ws.workstream_checkout(&primary).unwrap(),
            ws.project_root_path(&managed),
            "a managed primary is the project's tree"
        );
        let wt = workstream(managed.id, None, "feature/x");
        ws.put_workstream(&wt).unwrap();
        assert_eq!(
            ws.workstream_checkout(&wt).unwrap(),
            ws.paths().project(&managed.slug).workstream_dir(wt.id)
        );

        let elsewhere = dir.path().join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        let adopted = ws
            .create_project(NewProject {
                origin: bisa_core::ProjectOrigin::Workspace,
                root: ProjectRoot::External {
                    path: elsewhere.display().to_string(),
                },
                ..NewProject::managed("ext").unwrap()
            })
            .unwrap();
        let primary = ws.primary_workstream(adopted.id).unwrap();
        assert_eq!(
            ws.workstream_checkout(&primary).unwrap(),
            elsewhere,
            "an adopted primary is the folder it adopted, outside the workspace"
        );
        // Nothing on the record says where it is.
        let json = serde_json::to_value(&primary).unwrap();
        assert!(json.get("path").is_none());
    }

    #[test]
    fn rebuild_leaves_truth_alone() {
        let (_dir, ws) = ws();
        let p = ws
            .create_project(NewProject::managed("app").unwrap())
            .unwrap();
        let id = WorkstreamId::primary_of(p.id);
        ws.forget_workstream_record(&p.slug, id).unwrap();
        assert!(ws.get_workstream(id).is_err());
        ws.rebuild_index().unwrap();
        assert!(
            ws.get_workstream(id).is_err(),
            "a rebuild only reads the truth; nothing invents a record, ever"
        );
    }

    #[test]
    fn sessions_remember_their_workstream() {
        let (_dir, ws) = ws();
        let p = ws
            .create_project(NewProject::managed("app").unwrap())
            .unwrap();
        let primary = WorkstreamId::primary_of(p.id);
        ws.record_session(&SessionRow {
            id: "s1".into(),
            adapter: "mock".into(),
            workstream: Some(primary.to_string()),
            status: crate::SessionStatus::Live,
            ..Default::default()
        })
        .unwrap();
        assert_eq!(
            ws.session_by_id("s1").unwrap().unwrap().workstream,
            Some(primary.to_string())
        );
        ws.rebuild_index().unwrap();
        assert_eq!(
            ws.session_by_id("s1").unwrap().unwrap().workstream,
            Some(primary.to_string()),
            "the attribution survives a rebuild from the session file"
        );
    }

    #[test]
    fn only_name_note_pinned_and_the_board_are_editable() {
        let (_dir, ws) = ws();
        let p = ws
            .create_project(NewProject::managed("app").unwrap())
            .unwrap();
        let w = workstream(p.id, None, "feature/x");
        ws.put_workstream(&w).unwrap();
        let edited = ws
            .update_workstream(&Workstream {
                name: Some("Checkout rewrite".into()),
                note: Some("the v2 flow".into()),
                pinned: true,
                board: bisa_core::WorkstreamBoard {
                    column: Some(bisa_core::BoardColumn::Doing),
                    rank: Some(1024),
                    due: Some(bisa_core::DueDate::parse("2026-09-30").unwrap()),
                },
                ..w.clone()
            })
            .unwrap();
        assert_eq!(ws.get_workstream(w.id).unwrap(), edited);
        assert_eq!(edited.label(None), "Checkout rewrite");
        assert_eq!(
            edited.board.column,
            Some(bisa_core::BoardColumn::Doing),
            "the board slot is the person's too"
        );
        assert!(ws
            .update_workstream(&Workstream {
                name: Some("  ".into()),
                ..w.clone()
            })
            .is_err());
        assert!(ws
            .update_workstream(&Workstream {
                kind: WorkstreamKind::Copy,
                ..w.clone()
            })
            .is_err());
        assert!(ws
            .update_workstream(&Workstream {
                state: WorkstreamState::Committed,
                ..w.clone()
            })
            .is_err());
    }

    #[test]
    fn a_workstream_needs_a_real_project_and_a_real_goal_if_it_names_one() {
        let (_dir, ws) = ws();
        let ghost = ProjectId::from_ulid(ulid::Ulid::from_parts(9, 9));
        assert!(ws.put_workstream(&workstream(ghost, None, "x")).is_err());
        let p = ws
            .create_project(NewProject::managed("app").unwrap())
            .unwrap();
        let ghost_goal = GoalId::from_ulid(ulid::Ulid::from_parts(9, 9));
        assert!(ws
            .put_workstream(&workstream(p.id, Some(ghost_goal), "x"))
            .is_err());
    }

    #[test]
    fn nothing_about_a_workstream_is_published() {
        let (_dir, ws) = ws();
        let p = ws
            .create_project(NewProject::managed("app").unwrap())
            .unwrap();
        let w = workstream(p.id, None, "feature/x");
        let mut rx = ws.subscribe_store_events();
        // Drain what the project's own snapshot said; the workstream must add
        // nothing — not on creation of the primary, not on this write.
        while rx.try_recv().is_ok() {}
        ws.put_workstream(&w).unwrap();
        assert!(
            rx.try_recv().is_err(),
            "a workstream must not emit a store event"
        );
        let needle = w.id.to_string();
        for entry in std::fs::read_dir(ws.paths().state_dir(crate::paths::Paths::NS_PROJECTS))
            .into_iter()
            .flatten()
            .flatten()
        {
            let body = std::fs::read_to_string(entry.path()).unwrap_or_default();
            assert!(
                !body.contains(&needle),
                "{} mentions the workstream",
                entry.path().display()
            );
        }
    }

    #[test]
    fn rebuild_restores_workstreams_and_a_deleted_goal_leaves_them_in_place() {
        let (_dir, ws) = ws();
        let goal = ws
            .create_goal(crate::workspace::NewGoal::captured("build it"))
            .unwrap();
        let p = ws
            .create_project(NewProject::managed("app").unwrap())
            .unwrap();
        let a = workstream(p.id, Some(goal.id), "feature/one");
        ws.put_workstream(&a).unwrap();
        ws.transition_workstream(a.id, &WorkstreamTransition::Committed)
            .unwrap();
        ws.rebuild_index().unwrap();
        assert_eq!(
            ws.get_workstream(a.id).unwrap().state,
            WorkstreamState::Committed
        );

        ws.delete_goal(goal.id).unwrap();
        // The record is under the project, so it survives; the goal it was
        // made for is history.
        assert_eq!(ws.get_workstream(a.id).unwrap().goal, Some(goal.id));
        assert!(ws
            .list_workstreams(WorkstreamFilter::Goal(goal.id))
            .unwrap()
            .is_empty());
        ws.rebuild_index().unwrap();
        // The worktree and the primary.
        assert_eq!(
            ws.list_workstreams(WorkstreamFilter::Project(p.id))
                .unwrap()
                .len(),
            2
        );
    }
}
