//! What an agent changed in a checkout during a conversation (ide/20): the
//! tracker that attributes every edit to its turn, the review a person
//! settles, and the restore that goes back to before a message.
//!
//! Only a turn of a conversation about a project or a workstream is tracked
//! — never a worker, a terminal harness or a note's answer. Whatever those
//! write is an **outside write**, met by one rule ([`fold_drift`]): it is
//! folded into the review's base, so the pending difference stays the
//! conversation agent's change alone.
//!
//! The ledger is a file per conversation (`bisa_store::changes`), changed
//! only under the conversation's lock here. Everything in this module is
//! synchronous — files and git — and an async caller steps off its executor
//! through [`off_thread`].

pub mod asks;
pub mod settle;
pub mod tracker;

use crate::events::{EngineEvent, EnginePayload};
use crate::{EngineError, Inner};
use bisa_core::{
    ChangeKind, ChangeLedger, ChangeState, ConversationId, ConversationMode, FileDiff, Hunk,
    RelPath, ReviewFile, Sha256, TurnId, WorkstreamId,
};
use bisa_store::{content_hash, MAX_CHANGE_BLOB};
use dashmap::DashMap;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Per-conversation locks: a ledger is read, changed and written whole.
#[derive(Default)]
pub struct ChangesState {
    locks: DashMap<ConversationId, Arc<Mutex<()>>>,
    pub(crate) asks: asks::AskDesk,
}

impl ChangesState {
    fn lock_of(&self, conversation: ConversationId) -> Arc<Mutex<()>> {
        Arc::clone(&self.locks.entry(conversation).or_default())
    }

    /// The conversation is gone: its lock, its asks and its grants with it.
    pub(crate) fn forget(&self, conversation: ConversationId) {
        self.locks.remove(&conversation);
        self.asks.forget(conversation);
    }
}

/// Where a conversation's turns run.
#[derive(Clone, Debug)]
pub struct Checkout {
    pub conversation: ConversationId,
    pub workstream: WorkstreamId,
    pub root: PathBuf,
}

impl Checkout {
    /// The checkout of a conversation about a project or a workstream;
    /// an error in a sentence for any other origin.
    pub fn of(inner: &Inner, conversation: ConversationId) -> Result<Self, EngineError> {
        let record = inner.ws.get_conversation(conversation)?;
        let (workstream, project, _) =
            crate::conversation::checkout_of_origin(inner, &record.origin).ok_or_else(|| {
                EngineError::Invalid(bisa_core::text!(
                    "error-engine-invalid-conversation-not-about-project-workstream-so-turns"
                ))
            })?;
        Ok(Self {
            conversation,
            workstream: workstream.id,
            root: inner.ws.checkout_in(&project, &workstream),
        })
    }

    /// A path a tool named, as the checkout's own: `None` outside it.
    pub fn relative(&self, named: &str, cwd: &Path) -> Option<RelPath> {
        let named = Path::new(named);
        let absolute = if named.is_absolute() {
            named.to_path_buf()
        } else {
            cwd.join(named)
        };
        let root = self
            .root
            .canonicalize()
            .unwrap_or_else(|_| self.root.clone());
        // The file may not exist yet: canonicalise its folder, keep its name.
        let resolved = match (absolute.parent(), absolute.file_name()) {
            (Some(parent), Some(name)) => parent
                .canonicalize()
                .map(|p| p.join(name))
                .unwrap_or_else(|_| absolute.clone()),
            _ => absolute.clone(),
        };
        let rel = resolved
            .strip_prefix(&root)
            .or_else(|_| absolute.strip_prefix(&self.root))
            .ok()?;
        RelPath::new(rel.to_string_lossy()).ok()
    }

    fn path_of(&self, rel: &RelPath) -> PathBuf {
        self.root.join(rel.as_str())
    }

    /// The file's bytes; `Ok(None)` when it does not stand, `Err` when it is
    /// too large to keep for a review or cannot be read.
    pub(crate) fn read(&self, rel: &RelPath) -> Result<Option<Vec<u8>>, EngineError> {
        let path = self.path_of(rel);
        match std::fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
            Ok(meta) if !meta.is_file() => return Ok(None),
            Ok(meta) if meta.len() > MAX_CHANGE_BLOB => {
                return Err(EngineError::Invalid(bisa_core::text!(
                    "error-engine-invalid-too-large-review",
                    rel = rel.to_string()
                )))
            }
            Ok(_) => {}
        }
        Ok(Some(std::fs::read(&path)?))
    }
}

/// Not text: a NUL in the head, or bytes that are not UTF-8. Sniffed from
/// the bytes, never from the name (ide/03).
pub(crate) fn is_opaque(bytes: &[u8]) -> bool {
    bytes.iter().take(8000).any(|b| *b == 0) || std::str::from_utf8(bytes).is_err()
}

pub(crate) fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub(crate) fn same(a: &Option<Sha256>, bytes: &Option<Vec<u8>>) -> bool {
    match (a, bytes) {
        (None, None) => true,
        (Some(sha), Some(bytes)) => sha.as_str() == content_hash(bytes),
        _ => false,
    }
}

/// Keep bytes as a blob; `None` stays *no file*.
pub(crate) fn put(
    inner: &Inner,
    conversation: ConversationId,
    bytes: Option<&[u8]>,
) -> Result<Option<Sha256>, EngineError> {
    match bytes {
        None => Ok(None),
        Some(bytes) => Ok(Some(inner.ws.put_change_blob(conversation, bytes)?)),
    }
}

pub(crate) fn bytes_of(
    inner: &Inner,
    conversation: ConversationId,
    sha: &Option<Sha256>,
) -> Result<Option<Vec<u8>>, EngineError> {
    match sha {
        None => Ok(None),
        Some(sha) => Ok(Some(inner.ws.change_blob(conversation, sha)?)),
    }
}

/// A blob as text; *no file* is the empty text.
pub(crate) fn text_of(
    inner: &Inner,
    conversation: ConversationId,
    sha: &Option<Sha256>,
) -> Result<String, EngineError> {
    Ok(bytes_of(inner, conversation, sha)?
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default())
}

fn lossy(bytes: &Option<Vec<u8>>) -> String {
    bytes
        .as_ref()
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .unwrap_or_default()
}

/// Run `work` on the ledger under the conversation's lock, write it back
/// when `work` says it changed, and tell the bus how many files now wait.
pub(crate) fn with_ledger<T>(
    inner: &Inner,
    checkout: &Checkout,
    work: impl FnOnce(&mut ChangeLedger) -> Result<(T, bool), EngineError>,
) -> Result<T, EngineError> {
    let lock = inner.changes.lock_of(checkout.conversation);
    let _held = lock.lock().unwrap_or_else(|e| e.into_inner());
    let mut ledger = inner.ws.change_ledger(checkout.conversation)?;
    let before = ledger.pending();
    let (out, changed) = work(&mut ledger)?;
    if changed {
        inner.ws.write_change_ledger(&ledger)?;
    }
    if changed || before != ledger.pending() {
        inner.emit(EngineEvent::global(EnginePayload::ChangesMoved {
            conversation: checkout.conversation.to_string(),
            workstream: checkout.workstream.to_string(),
            pending: ledger.pending(),
        }));
    }
    Ok(out)
}

/// **The conflict rule.** For every file under review whose disk is no
/// longer what the ledger last saw, somebody else wrote it — a person's
/// save, a terminal harness, a worker, another conversation. Their change is
/// folded into the base ([`bisa_core::changes::rebase`]), so what stays
/// pending is the agent's alone; an edit on the agent's own lines marks the
/// file `overlapped`; a file with nothing left pending is `Gone`.
///
/// Called before every read and every settle, and when a turn begins — never
/// while a tool of the conversation's own is writing. Answers whether the
/// ledger changed.
pub(crate) fn fold_drift(
    inner: &Inner,
    checkout: &Checkout,
    ledger: &mut ChangeLedger,
) -> Result<bool, EngineError> {
    let mut changed = false;
    let mut gone: Vec<RelPath> = Vec::new();
    for review in &mut ledger.review {
        let Ok(disk) = checkout.read(&review.path) else {
            continue;
        };
        if same(&review.image, &disk) {
            continue;
        }
        changed = true;
        if fold_one(inner, checkout.conversation, review, &disk)? {
            gone.push(review.path.clone());
        }
    }
    for path in gone {
        ledger.settled(&path, ChangeState::Gone);
    }
    Ok(changed)
}

/// Fold one file's outside write — `disk` is the file as somebody else left
/// it — into its review. Answers whether nothing of the agent's is left.
pub(crate) fn fold_one(
    inner: &Inner,
    conversation: ConversationId,
    review: &mut ReviewFile,
    disk: &Option<Vec<u8>>,
) -> Result<bool, EngineError> {
    if disk.is_none() || same(&review.base, disk) {
        // Removed by somebody else, or put back to where it started.
        return Ok(true);
    }
    if review.opaque || disk.as_deref().is_some_and(is_opaque) {
        review.opaque = true;
        review.overlapped = true;
    } else {
        let base = text_of(inner, conversation, &review.base)?;
        let image = text_of(inner, conversation, &review.image)?;
        let now = lossy(disk);
        let rebased = bisa_core::changes::rebase(&base, &image, &now);
        review.overlapped |= rebased.conflicted;
        if FileDiff::of(&rebased.text, &now).is_empty() {
            return Ok(true);
        }
        // A file the agent made has no base to move.
        if review.base.is_some() {
            review.base = put(inner, conversation, Some(rebased.text.as_bytes()))?;
        }
    }
    review.image = put(inner, conversation, disk.as_deref())?;
    Ok(false)
}

// ---------------------------------------------------------------------------
// What a screen reads
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct FileChangeView {
    pub path: String,
    pub kind: ChangeKind,
    pub state: ChangeState,
    pub opaque: bool,
    /// Somebody else edited the agent's own lines: an Undo asks first.
    pub overlapped: bool,
    /// Lines this turn added to and removed from the file.
    pub added: usize,
    pub removed: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct TurnChangesView {
    pub turn: TurnId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply: Option<String>,
    pub agent: String,
    pub mode: ConversationMode,
    pub started_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<u64>,
    pub files: Vec<FileChangeView>,
}

/// A conversation's changes as the pane draws them: the turns that changed
/// something, oldest first, and how many files wait for a word.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct ChangesView {
    pub conversation: String,
    pub workstream: String,
    pub mode: ConversationMode,
    pub pending: usize,
    /// Whether the pending changes are owed a word (`manual`) or will be
    /// kept by the next message (`auto`).
    pub owed: bool,
    pub turns: Vec<TurnChangesView>,
}

/// One file under review, as the editor's lens draws it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct FileReviewView {
    pub path: String,
    pub kind: ChangeKind,
    pub opaque: bool,
    pub overlapped: bool,
    /// The file before the pending changes; empty for one the agent made.
    pub base_text: String,
    /// The hash of the disk these hunks were cut from — what a settle of one
    /// hunk states, so a hunk is never settled against a file that moved.
    pub disk_hash: String,
    pub hunks: Vec<Hunk>,
}

pub fn view(inner: &Inner, conversation: ConversationId) -> Result<ChangesView, EngineError> {
    let checkout = Checkout::of(inner, conversation)?;
    let mode = inner.ws.get_conversation(conversation)?.mode;
    with_ledger(inner, &checkout, |ledger| {
        let changed = fold_drift(inner, &checkout, ledger)?;
        let mut turns = Vec::new();
        for turn in ledger.turns.iter().filter(|t| !t.files.is_empty()) {
            let mut files = Vec::new();
            for file in &turn.files {
                let (added, removed) = if file.opaque {
                    (0, 0)
                } else {
                    let base = text_of(inner, conversation, &file.base)?;
                    let image = text_of(inner, conversation, &file.image)?;
                    FileDiff::of(&base, &image).stats()
                };
                files.push(FileChangeView {
                    path: file.path.to_string(),
                    kind: file.kind,
                    state: file.state,
                    opaque: file.opaque,
                    overlapped: ledger
                        .under_review(&file.path)
                        .is_some_and(|r| r.overlapped),
                    added,
                    removed,
                });
            }
            turns.push(TurnChangesView {
                turn: turn.turn,
                prompt: turn.prompt.clone(),
                reply: turn.reply.clone(),
                agent: turn.agent.to_string(),
                mode: turn.mode,
                started_at: turn.started_at,
                ended_at: turn.ended_at,
                files,
            });
        }
        Ok((
            ChangesView {
                conversation: conversation.to_string(),
                workstream: checkout.workstream.to_string(),
                mode,
                pending: ledger.pending(),
                owed: mode.owes_review() && ledger.pending() > 0,
                turns,
            },
            changed,
        ))
    })
}

/// One file's pending difference. `Ok(None)` when nothing of it is pending.
pub fn file_view(
    inner: &Inner,
    conversation: ConversationId,
    path: &RelPath,
) -> Result<Option<FileReviewView>, EngineError> {
    let checkout = Checkout::of(inner, conversation)?;
    with_ledger(inner, &checkout, |ledger| {
        let changed = fold_drift(inner, &checkout, ledger)?;
        let Some(review) = ledger.under_review(path) else {
            return Ok((None, changed));
        };
        let disk = checkout.read(path)?;
        let base_text = text_of(inner, conversation, &review.base)?;
        let disk_text = lossy(&disk);
        let hunks = if review.opaque {
            Vec::new()
        } else {
            FileDiff::of(&base_text, &disk_text).hunks().to_vec()
        };
        let kind =
            ChangeKind::of(review.base.is_some(), disk.is_some()).unwrap_or(ChangeKind::Modified);
        Ok((
            Some(FileReviewView {
                path: path.to_string(),
                kind,
                opaque: review.opaque,
                overlapped: review.overlapped,
                base_text: if review.opaque {
                    String::new()
                } else {
                    base_text
                },
                disk_hash: disk.as_deref().map(content_hash).unwrap_or_default(),
                hunks,
            }),
            changed,
        ))
    })
}

/// Step off the async executor for work that reads files and runs git.
pub(crate) async fn off_thread<T: Send + 'static>(
    inner: &Arc<Inner>,
    work: impl FnOnce(&Arc<Inner>) -> T + Send + 'static,
) -> Option<T> {
    let inner = Arc::clone(inner);
    tokio::task::spawn_blocking(move || work(&inner)).await.ok()
}
