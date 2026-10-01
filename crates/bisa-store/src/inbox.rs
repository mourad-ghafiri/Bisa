//! What the inbox is not allowed to forget.
//!
//! The inbox used to be a query: a conversation had a row while it had an
//! open gate or an unread message, and lost it the moment either went away.
//! Reading a thread therefore *deleted* it, which is the one thing an inbox
//! may never do — you cannot come back to a row that reading removed.
//!
//! Membership has to come from something that survives both reading and an
//! index rebuild, and this module is where those durable facts are read back:
//!
//! - [`Workspace::decisions`] — every gate or question that was ever decided,
//!   from the `approvals` table, which `rebuild_index` re-derives from the
//!   journal's `Decision` events. The journal is the record of "this needed a
//!   human", so a row founded on it costs nothing to keep and survives a
//!   discarded cache for free.
//! - [`Workspace::scope_activity`] — one row per conversation with the newest
//!   message's timestamp. The inbox needs a `latest_at` for every row it
//!   renders; fetching a page of messages per conversation just to read the
//!   last one's clock is a query per row for a number one grouped query
//!   already has.
//! - [`Workspace::read_markers`] — the local watermarks themselves, which
//!   `unread_counts` can only report *through* a message count. A scope that
//!   has never had a message and a scope you have read both count zero there,
//!   and the difference between them is exactly what "read" means on a row
//!   whose only content is a gate.
//!
//! Read markers are local and lost on rebuild **by design** (see
//! `index.rs`). That is survivable precisely because membership does not
//! depend on them: a rebuild forgets what you had read, never which rows are
//! yours.

use crate::error::StoreError;
use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};

/// How many decisions are read back at once.
///
/// The inbox wants the *latest* decision per conversation, and gates are rare
/// — a handful per goal across its whole life. The cap is here so that a
/// workspace which somehow accumulates a pathological number of them degrades
/// by showing an older decision on a stale row rather than by loading the
/// entire history into every inbox request.
pub const MAX_DECISIONS: usize = 2000;

/// A gate or question that was decided, read back from the durable record.
///
/// `gate` is the label the journal stored (`commit`, `acceptance`,
/// `escalation`, `publish`) rather than a parsed enum: this crate reads what
/// was written, and deciding what an unrecognised label means is the caller's
/// problem, not the store's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DecisionRow {
    /// The goal whose journal holds the decision — `None` when it is a run
    /// of the workspace's ([`Self::run_id`]).
    pub goal_id: Option<String>,
    pub run_id: Option<String>,
    pub gate: String,
    pub subject: String,
    pub actor: String,
    pub approve: bool,
    pub at: u64,
}

/// One conversation's local read watermark.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReadMarker {
    pub scope_id: String,
    pub last_read_at: u64,
    /// Set by `mark_unread`: you put it back deliberately, so no arithmetic
    /// over timestamps may quietly decide it is read again.
    pub forced_unread: bool,
}

impl Workspace {
    /// Every gate decision on record, newest first, capped at
    /// [`MAX_DECISIONS`].
    pub fn decisions(&self) -> Result<Vec<DecisionRow>, StoreError> {
        self.idx().decisions(MAX_DECISIONS)
    }

    /// `(scope, newest message timestamp)` for every conversation that has
    /// one. Scopes with no messages are absent rather than zero — the caller
    /// needs to tell "nothing was ever said here" from "the last thing said
    /// was at the epoch".
    pub fn scope_activity(&self) -> Result<Vec<(String, u64)>, StoreError> {
        self.idx().scope_activity()
    }

    /// Every read watermark this machine holds.
    pub fn read_markers(&self) -> Result<Vec<ReadMarker>, StoreError> {
        self.idx().read_markers()
    }
}
