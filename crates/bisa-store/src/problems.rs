//! What the open and the rebuild found wrong and worked around.
//!
//! The rule the whole store keeps: **the owner key alone stops an open; every
//! other file this build cannot read is quarantined or skipped, and named
//! here**, so a person can see what a crash tore and restore it from the
//! quarantine folder by hand, and the node still comes up. A problem is a
//! fact about this process's open — recomputed at every open, never
//! persisted; the quarantine folder is the durable record of what was moved.

use bisa_core::Text;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// What kind of trouble a problem names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "WorkspaceProblemKind")]
pub enum ProblemKind {
    /// A file this build cannot read, left where it is and read as absent
    /// or as its default — governance, read as the owner's alone.
    Unreadable,
    /// A file this build cannot read, moved under `quarantine/` so the
    /// object could be written again.
    Quarantined,
    /// A permanent object recreated after its file was quarantined — the
    /// member file with the owner alone, the `general` channel afresh.
    Recreated,
    /// The index could not take one record the files hold; its row is left
    /// out until the next rebuild.
    IndexDisagrees,
    /// The index rebuild skipped one record it cannot read.
    RebuildSkipped,
    /// A settings layer this build cannot read; its values are not applied
    /// until the file is fixed or a setting is saved there again.
    SettingsLayerUnreadable,
    /// A run that was running when the last node ended and that its goal
    /// never recorded; ended as stopped when the engine next starts, under
    /// its lock (`Workspace::end_orphan_runs`) — never by a plain open.
    OrphanRun,
    /// An index row that said a run was live while its record was finished
    /// or gone; brought back in step with the record.
    StaleRow,
    /// Two runs that began from one signal; one is kept as the signal's
    /// record, the other indexed without it.
    DuplicateDispatch,
}

/// One thing the open or the rebuild found wrong and worked around.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceProblem {
    pub kind: ProblemKind,
    /// The file or record it is about — a path, or an id when no file names it.
    pub path: String,
    /// The sentence a person reads.
    pub text: Text,
    /// Where the file was moved, when it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quarantined: Option<String>,
    /// Unix seconds, when it was found.
    pub at: u64,
}

impl WorkspaceProblem {
    pub fn new(
        kind: ProblemKind,
        path: impl Into<String>,
        text: Text,
        quarantined: Option<&Path>,
    ) -> Self {
        WorkspaceProblem {
            kind,
            path: path.into(),
            text,
            quarantined: quarantined.map(|p| p.display().to_string()),
            at: crate::workspace::now_secs(),
        }
    }

    /// Whether `other` names the same trouble about the same file — what
    /// keeps a read that is made many times a boot from saying it many times.
    pub fn same_as(&self, other: &WorkspaceProblem) -> bool {
        self.kind == other.kind && self.path == other.path
    }
}

/// Where problems are noted: the workspace's list, shared with the snapshot
/// store so a file it moves aside on a write is named like every other.
#[derive(Clone, Default)]
pub struct ProblemSink(Arc<Mutex<Vec<WorkspaceProblem>>>);

impl ProblemSink {
    /// Note one problem, once: a read made many times a boot — a settings
    /// layer's — says its trouble once.
    pub fn record(&self, problem: WorkspaceProblem) {
        let mut problems = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if problems.iter().any(|p| p.same_as(&problem)) {
            return;
        }
        tracing::error!(target: "bisa_store", kind = ?problem.kind, path = %problem.path, "{}", problem.text);
        problems.push(problem);
    }

    /// Every problem noted so far, in the order found.
    pub fn all(&self) -> Vec<WorkspaceProblem> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}
