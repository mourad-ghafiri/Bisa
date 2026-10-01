//! The tracker of one conversation session: it watches the session's own
//! tool calls and attributes what they change to the turn that made it.
//!
//! **A file edit** (a Write-tier call that names its file) is bracketed
//! exactly: the file is read when the call is allowed and again when the
//! tool ends. **A command** (an Exec-tier call) names no file, so a git
//! checkout is snapshotted right before it runs and compared right after:
//! whatever moved in between is the command's, and what it was before is
//! read out of the snapshot. Between the conversation's own tools nothing is
//! attributed — a write there is somebody else's ([`super::fold_drift`]).
//!
//! A harness the guard cannot stop raises no call to bracket: its whole
//! turn is one window, snapshotted when it begins and swept when it ends. A
//! root that is not a git repository has no snapshot at all: file edits are
//! still bracketed, and what a command changed is not attributed — a stated
//! bound (ide/20).

use super::{fold_drift, fold_one, is_opaque, now_secs, put, same, with_ledger, Checkout};
use crate::{EngineError, Inner};
use bisa_core::{AgentId, ChangeState, ConversationMode, RelPath, ToolTier, TurnChanges, TurnId};
use bisa_harness::{InputKind, InputRequest};
use bisa_vcs::TreeId;
use std::path::PathBuf;
use std::sync::Mutex;

/// The input fields a file tool names its file in — the guard's own list.
const PATH_FIELDS: [&str; 3] = ["file_path", "path", "notebook_path"];

/// A file edit allowed and not yet ended: the file as it was.
struct OpenWrite {
    tool: String,
    path: RelPath,
    before: Option<Vec<u8>>,
}

#[derive(Default)]
struct Live {
    turn: Option<TurnId>,
    snapshot: Option<TreeId>,
    writes: Vec<OpenWrite>,
    /// Commands allowed and not yet ended, by tool name.
    commands: Vec<String>,
}

pub struct ChangeTracker {
    checkout: Checkout,
    agent: AgentId,
    cwd: PathBuf,
    /// Whether the root is a git repository: only then is there a snapshot.
    git: bool,
    /// Whether the harness stops before a tool runs. Without it the turn is
    /// one window.
    guarded: bool,
    index: PathBuf,
    live: Mutex<Live>,
}

impl ChangeTracker {
    pub fn new(
        inner: &Inner,
        checkout: Checkout,
        agent: AgentId,
        cwd: PathBuf,
        guarded: bool,
    ) -> Self {
        let git = bisa_vcs::git::is_repo(&checkout.root);
        let index = inner.ws.change_index_file(checkout.conversation);
        Self {
            checkout,
            agent,
            cwd,
            git,
            guarded,
            index,
            live: Mutex::new(Live::default()),
        }
    }

    fn live(&self) -> std::sync::MutexGuard<'_, Live> {
        self.live.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn snapshot(&self) -> Option<TreeId> {
        if !self.git {
            return None;
        }
        match bisa_vcs::snapshot::snapshot_tree(&self.checkout.root, &self.index) {
            Ok(tree) => Some(tree),
            Err(e) => {
                tracing::warn!(
                    target: "bisa_engine::changes",
                    conversation = %self.checkout.conversation,
                    workstream = %self.checkout.workstream,
                    agent = %self.agent,
                    root = %self.checkout.root.display(),
                    "no snapshot for a review, so what a command moves in this turn is not attributed: {e}"
                );
                None
            }
        }
    }

    /// A turn begins, woken by `prompt`. What somebody else wrote since the
    /// last turn is folded away first; an `auto` conversation keeps what
    /// earlier turns left pending — the person's next message is their word.
    pub fn begin_turn(
        &self,
        inner: &Inner,
        prompt: Option<String>,
        mode: ConversationMode,
    ) -> Result<TurnId, EngineError> {
        let turn = TurnId::from_ulid(ulid::Ulid::from_datetime(std::time::SystemTime::now()));
        let keep = inner
            .ws
            .settings(None)
            .ok()
            .and_then(|rows| {
                rows.into_iter()
                    .find(|r| r.key == "agents.review.checkpoints")
                    .and_then(|r| r.value.as_u64())
            })
            .unwrap_or(20) as usize;
        with_ledger(inner, &self.checkout, |ledger| {
            fold_drift(inner, &self.checkout, ledger)?;
            if mode.keeps_on_next_message() {
                let paths: Vec<RelPath> = ledger.review.iter().map(|r| r.path.clone()).collect();
                for path in paths {
                    ledger.settled(&path, ChangeState::Kept);
                }
            }
            ledger.turns.push(TurnChanges {
                turn,
                prompt,
                reply: None,
                agent: self.agent.clone(),
                mode,
                started_at: now_secs(),
                ended_at: None,
                files: Vec::new(),
            });
            ledger.prune(keep);
            Ok(((), true))
        })?;
        let snapshot = if self.guarded { None } else { self.snapshot() };
        let mut live = self.live();
        *live = Live {
            turn: Some(turn),
            snapshot,
            ..Live::default()
        };
        Ok(turn)
    }

    /// A call of the session's was allowed and is about to run.
    pub fn allowed(&self, request: &InputRequest) {
        let InputKind::Permission {
            tool_name,
            tier,
            input,
            ..
        } = &request.kind
        else {
            return;
        };
        match tier {
            ToolTier::Read => {}
            ToolTier::Write => {
                let named = PATH_FIELDS
                    .iter()
                    .find_map(|field| input.get(field).and_then(|v| v.as_str()));
                let Some(path) = named.and_then(|n| self.checkout.relative(n, &self.cwd)) else {
                    return;
                };
                // A file too large to review is not tracked at all.
                let before = match self.checkout.read(&path) {
                    Ok(before) => before,
                    Err(e) => {
                        tracing::info!(target: "bisa_engine::changes", conversation = %self.checkout.conversation, %path, "a write is not tracked for review: {e}");
                        return;
                    }
                };
                self.live().writes.push(OpenWrite {
                    tool: tool_name.clone(),
                    path,
                    before,
                });
            }
            ToolTier::Exec => {
                // The window opens now: what moved before it is not the
                // command's.
                let snapshot = self.snapshot();
                let mut live = self.live();
                if live.commands.is_empty() {
                    live.snapshot = snapshot;
                }
                live.commands.push(tool_name.clone());
            }
        }
    }

    /// A tool of the session's ended: record what it changed.
    pub fn tool_ended(&self, inner: &Inner, tool: &str) -> Result<(), EngineError> {
        let (write, sweep) = {
            let mut live = self.live();
            let write = live
                .writes
                .iter()
                .position(|w| w.tool == tool)
                .map(|at| live.writes.remove(at));
            let mut sweep = None;
            if write.is_none() {
                if let Some(at) = live.commands.iter().position(|c| c == tool) {
                    live.commands.remove(at);
                    if live.commands.is_empty() {
                        sweep = live.snapshot.take();
                    }
                }
            }
            (write, sweep)
        };
        if let Some(write) = write {
            self.record(inner, vec![(write.path, write.before)])?;
        }
        if let Some(from) = sweep {
            self.sweep(inner, &from)?;
        }
        Ok(())
    }

    /// The turn ended, and posted `reply`. Whatever is still open is closed:
    /// a harness may end a turn without ending every tool it began.
    pub fn end_turn(&self, inner: &Inner, reply: Option<String>) -> Result<(), EngineError> {
        let (turn, writes, sweep) = {
            let mut live = self.live();
            let writes: Vec<_> = std::mem::take(&mut live.writes);
            live.commands.clear();
            (live.turn.take(), writes, live.snapshot.take())
        };
        let Some(turn) = turn else {
            return Ok(());
        };
        self.record_in(
            inner,
            turn,
            writes.into_iter().map(|w| (w.path, w.before)).collect(),
        )?;
        if let Some(from) = sweep {
            self.sweep_in(inner, turn, &from)?;
        }
        with_ledger(inner, &self.checkout, |ledger| {
            if let Some(record) = ledger.turn_mut(turn) {
                record.ended_at = Some(now_secs());
                record.reply = reply;
            }
            // A turn that changed nothing leaves no card behind.
            ledger
                .turns
                .retain(|t| t.turn != turn || !t.files.is_empty());
            Ok(((), true))
        })
    }

    fn record(
        &self,
        inner: &Inner,
        touched: Vec<(RelPath, Option<Vec<u8>>)>,
    ) -> Result<(), EngineError> {
        let Some(turn) = self.live().turn else {
            return Ok(());
        };
        self.record_in(inner, turn, touched)
    }

    /// Record files with the bytes they held before: the disk says what they
    /// hold now. A file that did not move is not a change.
    fn record_in(
        &self,
        inner: &Inner,
        turn: TurnId,
        touched: Vec<(RelPath, Option<Vec<u8>>)>,
    ) -> Result<(), EngineError> {
        if touched.is_empty() {
            return Ok(());
        }
        let conversation = self.checkout.conversation;
        with_ledger(inner, &self.checkout, |ledger| {
            let mut changed = false;
            for (path, before) in touched {
                let after = match self.checkout.read(&path) {
                    Ok(after) => after,
                    Err(e) => {
                        tracing::info!(target: "bisa_engine::changes", %conversation, %path, "what a tool left is not kept for review: {e}");
                        continue;
                    }
                };
                if before == after {
                    continue;
                }
                // Somebody else wrote the file between the last look and
                // this tool: that is theirs, folded away before the agent's
                // own write is recorded.
                let mut gone = false;
                if let Some(review) = ledger.under_review_mut(&path) {
                    if !same(&review.image, &before) {
                        gone = fold_one(inner, conversation, review, &before)?;
                    }
                }
                if gone {
                    ledger.settled(&path, ChangeState::Gone);
                }
                let before_sha = put(inner, conversation, before.as_deref())?;
                let opaque = before.as_deref().is_some_and(is_opaque)
                    || after.as_deref().is_some_and(is_opaque);
                let after_sha = put(inner, conversation, after.as_deref())?;
                ledger.touched(turn, &path, before_sha, after_sha, opaque);
                changed = true;
            }
            Ok(((), changed))
        })
    }

    fn sweep(&self, inner: &Inner, from: &TreeId) -> Result<(), EngineError> {
        let Some(turn) = self.live().turn else {
            return Ok(());
        };
        self.sweep_in(inner, turn, from)
    }

    /// Everything that moved since `from` is this turn's: its bytes before
    /// are the snapshot's.
    fn sweep_in(&self, inner: &Inner, turn: TurnId, from: &TreeId) -> Result<(), EngineError> {
        let Some(now) = self.snapshot() else {
            return Ok(());
        };
        let root = &self.checkout.root;
        let mut touched = Vec::new();
        for path in bisa_vcs::snapshot::changed_between(root, from, &now)? {
            let Ok(rel) = RelPath::new(path.to_string_lossy()) else {
                continue;
            };
            let before = bisa_vcs::snapshot::blob_at(root, from, &path)?;
            if before
                .as_ref()
                .is_some_and(|b| b.len() as u64 > bisa_store::MAX_CHANGE_BLOB)
            {
                continue;
            }
            touched.push((rel, before));
        }
        self.record_in(inner, turn, touched)
    }
}
