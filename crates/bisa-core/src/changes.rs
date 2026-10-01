//! What an agent changed in a checkout during a conversation, and the text
//! algebra a review runs on.
//!
//! A conversation about a checkout keeps a **ledger**: every turn lists the
//! files it touched, each with the file as it was before the turn's first
//! touch (`base`) and as the turn left it (`image`). Beside the turns stands
//! the **review**: one entry per file still owed a word, whose `base` is what
//! a *Keep* moves forward and an *Undo* goes back to. The bytes themselves
//! are blobs the store addresses by [`Sha256`]; nothing here reads a disk.
//!
//! Three writers share a checkout — the person, the conversation's agent and
//! whoever else runs there (a terminal harness, a worker, another
//! conversation). The review survives them by one rule, [`rebase`]: whatever
//! somebody else changed since the agent's last write is folded into the
//! base, so the pending difference stays *the agent's change only*.
//!
//! The algebra is line-oriented and exact: a line keeps its terminator, so a
//! CRLF file and a file with no final newline round-trip byte for byte.

use crate::conversation::ConversationMode;
use crate::id::{AgentId, ConversationId, Sha256, TurnId};
use crate::path::RelPath;
use serde::{Deserialize, Serialize};
use similar::{DiffOp, MergeResolution, TextDiff, TextMerge};

/// What a turn did to a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Created,
    Modified,
    Removed,
}

impl ChangeKind {
    /// From whether the file stood before and stands after. `None` when it
    /// stood on neither side: nothing happened.
    pub fn of(before: bool, after: bool) -> Option<Self> {
        match (before, after) {
            (false, true) => Some(ChangeKind::Created),
            (true, true) => Some(ChangeKind::Modified),
            (true, false) => Some(ChangeKind::Removed),
            (false, false) => None,
        }
    }
}

/// Where a turn's change to a file stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChangeState {
    /// Owed a word, or waiting for the next message to keep it.
    Pending,
    Kept,
    Undone,
    /// Somebody else took it away — a discard in Git › Changes, a checkout —
    /// before a word was said.
    Gone,
}

impl ChangeState {
    pub fn is_settled(&self) -> bool {
        !matches!(self, ChangeState::Pending)
    }
}

/// One file a turn touched. `base` is the file before the turn's first
/// touch and `image` as the turn left it; `None` is *no file*.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FileChange {
    pub path: RelPath,
    pub kind: ChangeKind,
    pub state: ChangeState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<Sha256>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<Sha256>,
    /// Not text, or too large to diff: kept or undone whole, never by hunk.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub opaque: bool,
}

/// One turn's changes, under the reply that made them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TurnChanges {
    pub turn: TurnId,
    /// The message that woke the turn, and the reply it posted — event ids.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply: Option<String>,
    pub agent: AgentId,
    pub mode: ConversationMode,
    pub started_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<u64>,
    pub files: Vec<FileChange>,
}

impl TurnChanges {
    pub fn file(&self, path: &RelPath) -> Option<&FileChange> {
        self.files.iter().find(|f| &f.path == path)
    }

    pub fn pending(&self) -> impl Iterator<Item = &FileChange> {
        self.files.iter().filter(|f| !f.state.is_settled())
    }
}

/// A file still under review. `base` moves forward with every *Keep* and
/// with every [`rebase`]; `image` is the disk as the ledger last knew it —
/// what tells an outside write from the agent's own.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReviewFile {
    pub path: RelPath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<Sha256>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<Sha256>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub opaque: bool,
    /// Somebody else edited the very lines the agent changed: the pending
    /// difference holds their edit too, and an Undo asks first.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub overlapped: bool,
    /// Whether any part was kept — what the file's turns read as once the
    /// last part is settled.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub kept_any: bool,
}

/// A conversation's changes: the turns, oldest first, and the review.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChangeLedger {
    pub conversation: ConversationId,
    #[serde(default)]
    pub turns: Vec<TurnChanges>,
    #[serde(default)]
    pub review: Vec<ReviewFile>,
}

impl ChangeLedger {
    pub fn new(conversation: ConversationId) -> Self {
        Self {
            conversation,
            turns: Vec::new(),
            review: Vec::new(),
        }
    }

    pub fn turn(&self, id: TurnId) -> Option<&TurnChanges> {
        self.turns.iter().find(|t| t.turn == id)
    }

    pub fn turn_mut(&mut self, id: TurnId) -> Option<&mut TurnChanges> {
        self.turns.iter_mut().find(|t| t.turn == id)
    }

    pub fn under_review(&self, path: &RelPath) -> Option<&ReviewFile> {
        self.review.iter().find(|f| &f.path == path)
    }

    pub fn under_review_mut(&mut self, path: &RelPath) -> Option<&mut ReviewFile> {
        self.review.iter_mut().find(|f| &f.path == path)
    }

    /// How many files wait for a word.
    pub fn pending(&self) -> usize {
        self.review.len()
    }

    /// A turn touched `path`: the turn's own record — its `base` set once,
    /// at the first touch, its `image` at every one — and the review's,
    /// opened at the first pending touch of any turn.
    pub fn touched(
        &mut self,
        turn: TurnId,
        path: &RelPath,
        before: Option<Sha256>,
        after: Option<Sha256>,
        opaque: bool,
    ) {
        let Some(record) = self.turn_mut(turn) else {
            return;
        };
        match record.files.iter_mut().find(|f| &f.path == path) {
            Some(file) => {
                file.image = after.clone();
                file.opaque |= opaque;
                file.state = ChangeState::Pending;
                if let Some(kind) = ChangeKind::of(file.base.is_some(), after.is_some()) {
                    file.kind = kind;
                }
            }
            None => {
                let Some(kind) = ChangeKind::of(before.is_some(), after.is_some()) else {
                    return;
                };
                record.files.push(FileChange {
                    path: path.clone(),
                    kind,
                    state: ChangeState::Pending,
                    base: before.clone(),
                    image: after.clone(),
                    opaque,
                });
            }
        }
        match self.under_review_mut(path) {
            Some(review) => {
                review.image = after;
                review.opaque |= opaque;
            }
            None => self.review.push(ReviewFile {
                path: path.clone(),
                base: before,
                image: after,
                opaque,
                overlapped: false,
                kept_any: false,
            }),
        }
    }

    /// The file's review is over: every turn's pending change to it reads
    /// `state`, and the entry leaves the review.
    pub fn settled(&mut self, path: &RelPath, state: ChangeState) {
        self.review.retain(|f| &f.path != path);
        for turn in &mut self.turns {
            for file in &mut turn.files {
                if &file.path == path && !file.state.is_settled() {
                    file.state = state;
                }
            }
        }
    }

    /// The state a file's turns read once its last part is settled: kept
    /// when any part was, undone otherwise.
    pub fn verdict(review: &ReviewFile) -> ChangeState {
        if review.kept_any {
            ChangeState::Kept
        } else {
            ChangeState::Undone
        }
    }

    /// Every file touched at or after `turn`, each with the blob it goes
    /// back to (the earliest such turn's `base`) and the blob the agent left
    /// last (the latest such turn's `image`) — what a restore to before the
    /// turn's message reads. Empty when the turn is unknown.
    pub fn restore_plan(&self, turn: TurnId) -> Vec<RestoreTarget> {
        let Some(from) = self.turns.iter().position(|t| t.turn == turn) else {
            return Vec::new();
        };
        let mut plan: Vec<RestoreTarget> = Vec::new();
        for record in &self.turns[from..] {
            for file in &record.files {
                match plan.iter_mut().find(|p| p.path == file.path) {
                    Some(target) => target.left = file.image.clone(),
                    None => plan.push(RestoreTarget {
                        path: file.path.clone(),
                        back_to: file.base.clone(),
                        left: file.image.clone(),
                        opaque: file.opaque,
                    }),
                }
            }
        }
        plan
    }

    /// Drop the oldest turns past `keep`, never one with a change still
    /// pending. The newest `keep` turns always stay — a pending old turn
    /// is kept *in addition*, it never costs a newer one its place. What
    /// the dropped turns pinned is the store's to collect.
    pub fn prune(&mut self, keep: usize) {
        let oldest = self.turns.len().saturating_sub(keep);
        let mut index = 0;
        self.turns.retain(|turn| {
            let old = index < oldest;
            index += 1;
            !old || turn.pending().next().is_some()
        });
    }

    /// Every blob the ledger still names.
    pub fn blobs(&self) -> Vec<&Sha256> {
        let turns = self
            .turns
            .iter()
            .flat_map(|t| &t.files)
            .flat_map(|f| [f.base.as_ref(), f.image.as_ref()]);
        let review = self
            .review
            .iter()
            .flat_map(|f| [f.base.as_ref(), f.image.as_ref()]);
        turns.chain(review).flatten().collect()
    }
}

/// One file of a restore: where it goes back to and what the agent left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestoreTarget {
    pub path: RelPath,
    pub back_to: Option<Sha256>,
    pub left: Option<Sha256>,
    pub opaque: bool,
}

// ---------------------------------------------------------------------------
// The text algebra
// ---------------------------------------------------------------------------

/// A run of lines: where it starts (0-based) and how many.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LineSpan {
    pub start: usize,
    pub len: usize,
}

impl LineSpan {
    fn range(&self) -> std::ops::Range<usize> {
        self.start..self.start + self.len
    }
}

/// One pending difference between a file's base and its disk: the lines it
/// replaces in each, and their text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Hunk {
    /// Stable while the hunk's own text is: settling another hunk of the
    /// file does not rename this one.
    pub id: String,
    pub base: LineSpan,
    pub disk: LineSpan,
    pub removed: String,
    pub added: String,
}

/// A file's base against its disk, cut into hunks a person keeps or undoes
/// one at a time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDiff<'a> {
    base: Vec<&'a str>,
    disk: Vec<&'a str>,
    hunks: Vec<Hunk>,
}

/// A text as lines that keep their terminators.
fn lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

/// FNV-1a over a hunk's own text: an id with no clock and no counter.
fn fingerprint(removed: &str, added: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in removed.bytes().chain([0]).chain(added.bytes()) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

impl<'a> FileDiff<'a> {
    pub fn of(base: &'a str, disk: &'a str) -> Self {
        let diff = TextDiff::from_lines(base, disk);
        let (base_lines, disk_lines) = (lines(base), lines(disk));
        // A hunk is a maximal run of ops that are not `Equal`.
        let mut spans: Vec<(LineSpan, LineSpan)> = Vec::new();
        let mut open = false;
        for op in diff.ops() {
            if matches!(op, DiffOp::Equal { .. }) {
                open = false;
                continue;
            }
            let (old, new) = (op.old_range(), op.new_range());
            match spans.last_mut().filter(|_| open) {
                Some((b, d)) => {
                    b.len = old.end - b.start;
                    d.len = new.end - d.start;
                }
                None => spans.push((
                    LineSpan {
                        start: old.start,
                        len: old.len(),
                    },
                    LineSpan {
                        start: new.start,
                        len: new.len(),
                    },
                )),
            }
            open = true;
        }
        let mut seen: Vec<u64> = Vec::new();
        let hunks = spans
            .into_iter()
            .map(|(b, d)| {
                let removed = base_lines[b.range()].concat();
                let added = disk_lines[d.range()].concat();
                let print = fingerprint(&removed, &added);
                let nth = seen.iter().filter(|p| **p == print).count();
                seen.push(print);
                Hunk {
                    id: format!("{print:016x}-{nth}"),
                    base: b,
                    disk: d,
                    removed,
                    added,
                }
            })
            .collect();
        Self {
            base: base_lines,
            disk: disk_lines,
            hunks,
        }
    }

    pub fn hunks(&self) -> &[Hunk] {
        &self.hunks
    }

    pub fn is_empty(&self) -> bool {
        self.hunks.is_empty()
    }

    /// Lines added and removed across every hunk.
    pub fn stats(&self) -> (usize, usize) {
        self.hunks
            .iter()
            .fold((0, 0), |(a, r), h| (a + h.disk.len, r + h.base.len))
    }

    fn hunk(&self, id: &str) -> Option<&Hunk> {
        self.hunks.iter().find(|h| h.id == id)
    }

    /// The base with one hunk kept: its lines become the disk's. `None` for
    /// an id this difference does not hold.
    pub fn keep(&self, id: &str) -> Option<String> {
        let hunk = self.hunk(id)?;
        Some(splice(&self.base, hunk.base, &self.disk[hunk.disk.range()]))
    }

    /// The disk with one hunk undone: its lines become the base's.
    pub fn undo(&self, id: &str) -> Option<String> {
        let hunk = self.hunk(id)?;
        Some(splice(&self.disk, hunk.disk, &self.base[hunk.base.range()]))
    }
}

fn splice(text: &[&str], at: LineSpan, with: &[&str]) -> String {
    let mut out = String::new();
    out.extend(text[..at.start].iter().copied());
    out.extend(with.iter().copied());
    out.extend(text[at.start + at.len..].iter().copied());
    out
}

/// A three-way merge's answer. Where the two sides changed the same lines
/// differently, `text` holds *ours* and `conflicted` says so — never a
/// conflict marker in a person's file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Merged {
    pub text: String,
    pub conflicted: bool,
}

/// Merge what `theirs` changed since `ancestor` into `ours`.
pub fn merge3(ancestor: &str, ours: &str, theirs: &str) -> Merged {
    let merge = TextMerge::from_lines(ancestor, ours, theirs);
    let mut text = String::new();
    for region in merge.regions() {
        let theirs_only = region.resolution() == MergeResolution::Theirs;
        let range = if theirs_only {
            region.theirs_range()
        } else {
            region.ours_range()
        };
        for index in range {
            let line = if theirs_only {
                merge.theirs_line(index)
            } else {
                merge.ours_line(index)
            };
            text.push_str(line.unwrap_or_default());
        }
    }
    Merged {
        text,
        conflicted: merge.is_conflicted(),
    }
}

/// Somebody else wrote the file since the agent did: fold their change into
/// the review's base so the pending difference stays the agent's alone.
/// `image` is the file as the agent left it, `disk` as it stands now.
pub fn rebase(base: &str, image: &str, disk: &str) -> Merged {
    merge3(image, base, disk)
}

/// Take one turn's change to a file back out of the disk: what the turn
/// made of `base` is `image`, and the disk may have moved on since.
pub fn take_back(disk: &str, image: &str, base: &str) -> Merged {
    merge3(image, disk, base)
}

/// Fold one turn's change into the review's base — a *Keep* of that turn
/// alone, whatever other turns still have pending in the file.
pub fn fold_in(review_base: &str, turn_base: &str, turn_image: &str) -> Merged {
    merge3(turn_base, review_base, turn_image)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha(n: u8) -> Sha256 {
        Sha256::new(format!("{n:02x}").repeat(32)).unwrap()
    }

    fn path(p: &str) -> RelPath {
        RelPath::new(p).unwrap()
    }

    fn turn(n: u64) -> TurnId {
        TurnId::from_ulid(ulid::Ulid::from_parts(n, 1))
    }

    fn ledger_with(turns: &[TurnId]) -> ChangeLedger {
        let mut ledger = ChangeLedger::new(ConversationId::from_ulid(ulid::Ulid::from_parts(1, 1)));
        for (n, id) in turns.iter().enumerate() {
            ledger.turns.push(TurnChanges {
                turn: *id,
                prompt: None,
                reply: None,
                agent: AgentId::new("developer").unwrap(),
                mode: ConversationMode::Manual,
                started_at: n as u64,
                ended_at: None,
                files: Vec::new(),
            });
        }
        ledger
    }

    #[test]
    fn keeping_every_hunk_makes_the_base_the_disk_and_undoing_every_hunk_the_reverse() {
        let base = "a\nb\nc\nd\ne\n";
        let disk = "a\nB\nc\nd\nE\nf\n";
        let diff = FileDiff::of(base, disk);
        assert_eq!(diff.hunks().len(), 2);
        assert_eq!(diff.stats(), (3, 2));

        let mut kept = base.to_string();
        while let Some(id) = FileDiff::of(&kept, disk)
            .hunks()
            .first()
            .map(|h| h.id.clone())
        {
            kept = FileDiff::of(&kept, disk).keep(&id).unwrap();
        }
        assert_eq!(kept, disk);

        let mut undone = disk.to_string();
        while let Some(id) = FileDiff::of(base, &undone)
            .hunks()
            .first()
            .map(|h| h.id.clone())
        {
            undone = FileDiff::of(base, &undone).undo(&id).unwrap();
        }
        assert_eq!(undone, base);
    }

    #[test]
    fn settling_one_hunk_leaves_the_other_with_its_name() {
        let base = "a\nb\nc\nd\ne\n";
        let disk = "a\nB\nc\nd\nE\n";
        let diff = FileDiff::of(base, disk);
        let (first, second) = (diff.hunks()[0].clone(), diff.hunks()[1].clone());
        let kept = diff.keep(&first.id).unwrap();
        let after = FileDiff::of(&kept, disk);
        assert_eq!(after.hunks().len(), 1);
        assert_eq!(after.hunks()[0].id, second.id);
        let undone = diff.undo(&first.id).unwrap();
        assert_eq!(undone, "a\nb\nc\nd\nE\n");
        assert_eq!(FileDiff::of(base, &undone).hunks()[0].id, second.id);
        assert_eq!(diff.keep("nope"), None);
    }

    #[test]
    fn two_hunks_with_the_same_text_wear_different_names() {
        let diff = FileDiff::of("x\n1\ny\n1\nz\n", "x\n2\ny\n2\nz\n");
        let ids: Vec<_> = diff.hunks().iter().map(|h| h.id.as_str()).collect();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1]);
        assert_eq!(diff.undo(ids[1]).unwrap(), "x\n2\ny\n1\nz\n");
    }

    #[test]
    fn line_endings_and_a_missing_final_newline_survive_byte_for_byte() {
        let base = "one\r\ntwo\r\nthree";
        let disk = "one\r\nTWO\r\nthree";
        let diff = FileDiff::of(base, disk);
        let id = diff.hunks()[0].id.clone();
        assert_eq!(diff.undo(&id).unwrap(), base);
        assert_eq!(diff.keep(&id).unwrap(), disk);
        // A final newline the agent added is a hunk of its own kind.
        let diff = FileDiff::of("a\nb", "a\nb\n");
        assert_eq!(diff.hunks().len(), 1);
        assert_eq!(diff.undo(&diff.hunks()[0].id).unwrap(), "a\nb");
    }

    #[test]
    fn a_created_and_a_removed_file_are_one_hunk_each() {
        let created = FileDiff::of("", "fn main() {}\n");
        assert_eq!(created.hunks().len(), 1);
        assert_eq!(created.undo(&created.hunks()[0].id).unwrap(), "");
        let removed = FileDiff::of("gone\n", "");
        assert_eq!(removed.stats(), (0, 1));
        assert!(FileDiff::of("same\n", "same\n").is_empty());
    }

    #[test]
    fn an_outside_edit_elsewhere_in_the_file_is_folded_into_the_base() {
        let base = "a\nb\nc\nd\ne\n";
        let image = "a\nB\nc\nd\ne\n"; // the agent changed line 2
        let disk = "a\nB\nc\nd\nE!\n"; // a person then changed line 5
        let rebased = rebase(base, image, disk);
        assert!(!rebased.conflicted);
        assert_eq!(rebased.text, "a\nb\nc\nd\nE!\n");
        let pending = FileDiff::of(&rebased.text, disk);
        assert_eq!(
            pending.hunks().len(),
            1,
            "only the agent's change is pending"
        );
        assert_eq!(
            pending.undo(&pending.hunks()[0].id).unwrap(),
            "a\nb\nc\nd\nE!\n"
        );
    }

    #[test]
    fn an_outside_edit_on_the_agents_own_lines_is_flagged_and_never_marked_up() {
        let rebased = rebase("a\nb\nc\n", "a\nB\nc\n", "a\nB2\nc\n");
        assert!(rebased.conflicted);
        assert_eq!(rebased.text, "a\nb\nc\n", "the base keeps its own lines");
        assert!(!rebased.text.contains("<<<<"));
    }

    #[test]
    fn an_outside_discard_leaves_nothing_pending() {
        let rebased = rebase("a\nb\n", "a\nB\n", "a\nb\n");
        assert!(FileDiff::of(&rebased.text, "a\nb\n").is_empty());
    }

    #[test]
    fn a_turn_is_taken_back_out_from_under_a_later_edit() {
        // Turn one changed line 2; somebody changed line 4 afterwards.
        let merged = take_back("a\nB\nc\nD\n", "a\nB\nc\nd\n", "a\nb\nc\nd\n");
        assert_eq!(
            merged,
            Merged {
                text: "a\nb\nc\nD\n".into(),
                conflicted: false
            }
        );
        // On the same line it is left alone and says so.
        assert!(take_back("a\nB!\n", "a\nB\n", "a\nb\n").conflicted);
    }

    #[test]
    fn keeping_one_turn_folds_only_its_change_into_the_base() {
        // The review's base is before turn one; turn two changed line 4.
        let folded = fold_in("a\nb\nc\nd\n", "a\nB\nc\nd\n", "a\nB\nc\nD\n");
        assert_eq!(folded.text, "a\nb\nc\nD\n");
        assert!(!folded.conflicted);
        // Two edits on touching lines are one conflict, as git's merge has
        // it: the base keeps its own lines and says so.
        let touching = fold_in("a\nb\nc\n", "a\nB\nc\n", "a\nB\nC\n");
        assert!(touching.conflicted);
        assert_eq!(touching.text, "a\nb\nc\n");
    }

    #[test]
    fn a_touch_opens_the_review_once_and_moves_the_image_every_time() {
        let (one, two) = (turn(1), turn(2));
        let mut ledger = ledger_with(&[one, two]);
        let file = path("src/lib.rs");
        ledger.touched(one, &file, Some(sha(1)), Some(sha(2)), false);
        ledger.touched(one, &file, Some(sha(2)), Some(sha(3)), false);
        ledger.touched(two, &file, Some(sha(3)), Some(sha(4)), false);
        assert_eq!(ledger.pending(), 1);
        let review = ledger.under_review(&file).unwrap();
        assert_eq!(
            (review.base.clone(), review.image.clone()),
            (Some(sha(1)), Some(sha(4)))
        );
        let first = ledger.turn(one).unwrap().file(&file).unwrap();
        assert_eq!(
            (first.base.clone(), first.image.clone()),
            (Some(sha(1)), Some(sha(3)))
        );
        let second = ledger.turn(two).unwrap().file(&file).unwrap();
        assert_eq!(second.base, Some(sha(3)));

        ledger.settled(&file, ChangeState::Kept);
        assert_eq!(ledger.pending(), 0);
        assert!(ledger.turns.iter().all(|t| t.pending().next().is_none()));
    }

    #[test]
    fn a_file_made_then_removed_in_one_turn_reads_as_what_it_ended_as() {
        let one = turn(1);
        let mut ledger = ledger_with(&[one]);
        let file = path("notes.md");
        ledger.touched(one, &file, None, Some(sha(1)), false);
        assert_eq!(ledger.turn(one).unwrap().files[0].kind, ChangeKind::Created);
        ledger.touched(one, &path("old.rs"), Some(sha(5)), None, false);
        assert_eq!(ledger.turn(one).unwrap().files[1].kind, ChangeKind::Removed);
        // A touch of a file that stood on neither side records nothing.
        ledger.touched(one, &path("ghost"), None, None, false);
        assert_eq!(ledger.turn(one).unwrap().files.len(), 2);
    }

    #[test]
    fn a_restore_goes_back_to_the_earliest_base_and_reads_the_latest_image() {
        let (one, two, three) = (turn(1), turn(2), turn(3));
        let mut ledger = ledger_with(&[one, two, three]);
        let (a, b) = (path("a.rs"), path("b.rs"));
        ledger.touched(one, &a, Some(sha(1)), Some(sha(2)), false);
        ledger.touched(two, &a, Some(sha(2)), Some(sha(3)), false);
        ledger.touched(three, &a, Some(sha(3)), Some(sha(4)), false);
        ledger.touched(three, &b, None, Some(sha(9)), false);
        let plan = ledger.restore_plan(two);
        assert_eq!(plan.len(), 2);
        assert_eq!(
            (plan[0].back_to.clone(), plan[0].left.clone()),
            (Some(sha(2)), Some(sha(4)))
        );
        assert_eq!(
            (plan[1].back_to.clone(), plan[1].left.clone()),
            (None, Some(sha(9)))
        );
        assert!(ledger.restore_plan(turn(7)).is_empty());
    }

    #[test]
    fn pruning_never_drops_a_turn_with_a_change_still_pending() {
        let ids = [turn(1), turn(2), turn(3)];
        let mut ledger = ledger_with(&ids);
        ledger.touched(ids[0], &path("kept.rs"), Some(sha(1)), Some(sha(2)), false);
        ledger.touched(ids[1], &path("done.rs"), Some(sha(3)), Some(sha(4)), false);
        ledger.settled(&path("done.rs"), ChangeState::Kept);
        ledger.prune(1);
        let left: Vec<_> = ledger.turns.iter().map(|t| t.turn).collect();
        assert_eq!(left, vec![ids[0], ids[2]]);
        assert!(ledger.blobs().contains(&&sha(1)));
        assert!(!ledger.blobs().contains(&&sha(3)));
    }

    #[test]
    fn the_verdict_of_a_file_is_kept_when_any_part_was() {
        let mut review = ReviewFile {
            path: path("x"),
            base: None,
            image: None,
            opaque: false,
            overlapped: false,
            kept_any: false,
        };
        assert_eq!(ChangeLedger::verdict(&review), ChangeState::Undone);
        review.kept_any = true;
        assert_eq!(ChangeLedger::verdict(&review), ChangeState::Kept);
    }
}
