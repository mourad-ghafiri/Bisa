//! A person's word on what an agent changed: **Keep** moves the review's
//! base forward, **Undo** writes the base back — by hunk, by file, by turn,
//! or all of it — and **restore** goes back to before a message.
//!
//! Every write goes through the IDE's own write path (`ide::files`): atomic,
//! guarded by the hash of the file just read, announced as `FileChanged` so
//! an open buffer reloads and a dirty one gets its three-way affordance. A
//! file the agent made is disposed of the way the explorer disposes of one,
//! never unlinked behind a person's back.
//!
//! An Undo never discards somebody else's work: a turn is taken back out
//! with a three-way merge, and where that would conflict — or where the file
//! is `overlapped` and nobody said `force` — the file is left alone and
//! named in [`Settled::skipped`].

use super::{bytes_of, fold_drift, is_opaque, put, text_of, with_ledger, Checkout};
use crate::events::{EngineEvent, EnginePayload};
use crate::ide::files::{self, Disposal};
use crate::{EngineError, Inner};
use bisa_core::{ChangeLedger, ChangeState, ConversationId, FileDiff, FileScope, RelPath, TurnId};
use bisa_store::content_hash;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Keep,
    Undo,
}

impl Verdict {
    pub fn as_str(&self) -> &'static str {
        match self {
            Verdict::Keep => "keep",
            Verdict::Undo => "undo",
        }
    }
}

/// What a word is said about.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "grain", deny_unknown_fields)]
pub enum Target {
    All,
    Turn {
        turn: TurnId,
    },
    File {
        path: RelPath,
    },
    /// One hunk, cut from the disk whose hash the caller read it with.
    Hunk {
        path: RelPath,
        hunk: String,
        disk_hash: String,
    },
}

/// A file a settle left alone, and why — a sentence a person reads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct Skipped {
    pub path: String,
    pub why: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct Settled {
    /// Files the word reached.
    pub files: usize,
    /// Files still waiting for one.
    pub pending: usize,
    pub skipped: Vec<Skipped>,
}

const OVERLAPPED: &str =
    "somebody else edited the same lines; undoing it would undo their edit too";
const MOVED_ON: &str = "the file changed on the same lines since; it was left as it is";

struct Settler<'a> {
    inner: &'a Arc<Inner>,
    checkout: &'a Checkout,
    disposal: Disposal,
    force: bool,
    files: usize,
    skipped: Vec<Skipped>,
}

impl Settler<'_> {
    fn conversation(&self) -> ConversationId {
        self.checkout.conversation
    }

    fn skip(&mut self, path: &RelPath, why: &str) {
        self.skipped.push(Skipped {
            path: path.to_string(),
            why: why.to_string(),
        });
    }

    /// Make the file on disk `bytes` — or make it go when `None` — stating
    /// what was just read of it.
    fn write_back(
        &self,
        path: &RelPath,
        disk: &Option<Vec<u8>>,
        bytes: Option<&[u8]>,
    ) -> Result<(), EngineError> {
        let id = self.checkout.workstream.to_string();
        let scope = FileScope::Workstream;
        match (disk, bytes) {
            (None, None) => Ok(()),
            (Some(_), None) => {
                files::delete_entry(self.inner, scope, &id, path.as_str(), false, self.disposal)
                    .map(|_| ())
            }
            (on_disk, Some(bytes)) => {
                let base_hash = on_disk.as_deref().map(content_hash);
                files::write_bytes(
                    self.inner,
                    scope,
                    &id,
                    path.as_str(),
                    bytes,
                    base_hash.as_deref(),
                )
                .map(|_| ())
            }
        }
    }

    /// The whole file: Keep ends its review where the disk stands, Undo
    /// writes the review's base back.
    fn file(
        &mut self,
        ledger: &mut ChangeLedger,
        path: &RelPath,
        verdict: Verdict,
    ) -> Result<(), EngineError> {
        let Some(review) = ledger.under_review(path).cloned() else {
            return Ok(());
        };
        if verdict == Verdict::Undo {
            if review.overlapped && !self.force {
                self.skip(path, OVERLAPPED);
                return Ok(());
            }
            let disk = self.checkout.read(path)?;
            let base = bytes_of(self.inner, self.conversation(), &review.base)?;
            self.write_back(path, &disk, base.as_deref())?;
        }
        ledger.settled(
            path,
            match verdict {
                Verdict::Keep => ChangeState::Kept,
                Verdict::Undo => ChangeState::Undone,
            },
        );
        self.files += 1;
        Ok(())
    }

    /// One turn's change to one file. Alone in the review it is the whole
    /// file; under other turns' changes it is merged in or taken back out.
    fn turn_file(
        &mut self,
        ledger: &mut ChangeLedger,
        turn: TurnId,
        path: &RelPath,
        verdict: Verdict,
    ) -> Result<(), EngineError> {
        let others = ledger
            .turns
            .iter()
            .filter(|t| t.turn != turn)
            .any(|t| t.file(path).is_some_and(|f| !f.state.is_settled()));
        let Some(change) = ledger.turn(turn).and_then(|t| t.file(path)).cloned() else {
            return Ok(());
        };
        if change.state.is_settled() {
            return Ok(());
        }
        if !others || change.opaque {
            return self.file(ledger, path, verdict);
        }
        let conversation = self.conversation();
        let turn_base = text_of(self.inner, conversation, &change.base)?;
        let turn_image = text_of(self.inner, conversation, &change.image)?;
        let Some(review) = ledger.under_review(path).cloned() else {
            return Ok(());
        };
        let disk = self.checkout.read(path)?;
        let disk_text = disk
            .as_ref()
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .unwrap_or_default();
        let state = match verdict {
            Verdict::Keep => {
                let base = text_of(self.inner, conversation, &review.base)?;
                let folded = bisa_core::changes::fold_in(&base, &turn_base, &turn_image);
                if folded.conflicted {
                    self.skip(path, MOVED_ON);
                    return Ok(());
                }
                let sha = put(self.inner, conversation, Some(folded.text.as_bytes()))?;
                if let Some(review) = ledger.under_review_mut(path) {
                    review.base = sha;
                    review.kept_any = true;
                }
                if FileDiff::of(&folded.text, &disk_text).is_empty() {
                    ledger.settled(path, ChangeState::Kept);
                }
                ChangeState::Kept
            }
            Verdict::Undo => {
                let taken = bisa_core::changes::take_back(&disk_text, &turn_image, &turn_base);
                if taken.conflicted || (review.overlapped && !self.force) {
                    self.skip(
                        path,
                        if taken.conflicted {
                            MOVED_ON
                        } else {
                            OVERLAPPED
                        },
                    );
                    return Ok(());
                }
                self.write_back(path, &disk, Some(taken.text.as_bytes()))?;
                let sha = put(self.inner, conversation, Some(taken.text.as_bytes()))?;
                let base = text_of(self.inner, conversation, &review.base)?;
                if let Some(review) = ledger.under_review_mut(path) {
                    review.image = sha;
                }
                if FileDiff::of(&base, &taken.text).is_empty() {
                    ledger.settled(path, ChangeState::Undone);
                }
                ChangeState::Undone
            }
        };
        if let Some(file) = ledger
            .turn_mut(turn)
            .and_then(|t| t.files.iter_mut().find(|f| &f.path == path))
        {
            file.state = state;
        }
        self.files += 1;
        Ok(())
    }

    fn hunk(
        &mut self,
        ledger: &mut ChangeLedger,
        path: &RelPath,
        hunk: &str,
        disk_hash: &str,
        verdict: Verdict,
    ) -> Result<(), EngineError> {
        let conversation = self.conversation();
        let Some(review) = ledger.under_review(path).cloned() else {
            return Err(EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-nothing-waiting-word",
                path = path.to_string()
            )));
        };
        let disk = self.checkout.read(path)?;
        let current_hash = disk.as_deref().map(content_hash).unwrap_or_default();
        let disk_text = disk
            .as_ref()
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .unwrap_or_default();
        if review.opaque || disk.as_deref().is_some_and(is_opaque) {
            return Err(EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-not-text-keep-undo-whole-file",
                path = path.to_string()
            )));
        }
        if current_hash != disk_hash {
            return Err(EngineError::FileConflict {
                path: path.to_string(),
                current_hash,
                current_text: disk_text,
            });
        }
        let base = text_of(self.inner, conversation, &review.base)?;
        let diff = FileDiff::of(&base, &disk_text);
        let moved = || EngineError::FileConflict {
            path: path.to_string(),
            current_hash: current_hash.clone(),
            current_text: disk_text.clone(),
        };
        let (new_base, new_disk) = match verdict {
            Verdict::Keep => (diff.keep(hunk).ok_or_else(moved)?, disk_text.clone()),
            Verdict::Undo => {
                if review.overlapped && !self.force {
                    self.skip(path, OVERLAPPED);
                    return Ok(());
                }
                (base.clone(), diff.undo(hunk).ok_or_else(moved)?)
            }
        };
        if verdict == Verdict::Undo {
            // Undoing the only hunk of a file the agent made unmakes it.
            let unmade = review.base.is_none() && new_disk.is_empty();
            let bytes = (!unmade).then_some(new_disk.as_bytes());
            self.write_back(path, &disk, bytes)?;
        }
        let base_sha = match (&review.base, verdict) {
            (None, Verdict::Undo) => None,
            _ => put(self.inner, conversation, Some(new_base.as_bytes()))?,
        };
        let image_sha = put(self.inner, conversation, Some(new_disk.as_bytes()))?;
        let done = FileDiff::of(&new_base, &new_disk).is_empty();
        let mut verdict_of_file = None;
        if let Some(review) = ledger.under_review_mut(path) {
            review.base = base_sha;
            review.image = image_sha;
            review.kept_any |= verdict == Verdict::Keep;
            verdict_of_file = done.then(|| ChangeLedger::verdict(review));
        }
        if let Some(state) = verdict_of_file {
            ledger.settled(path, state);
        }
        self.files += 1;
        Ok(())
    }
}

fn announce(inner: &Inner, conversation: ConversationId, act: &str, settled: &Settled) {
    if settled.files == 0 {
        return;
    }
    inner.emit(EngineEvent::global(EnginePayload::ChangesSettled {
        conversation: conversation.to_string(),
        act: act.to_string(),
        files: settled.files,
        skipped: settled.skipped.len(),
    }));
}

/// Say a word about an agent's changes. `force` undoes an `overlapped`
/// file too — the person was asked and said yes.
pub fn settle(
    inner: &Arc<Inner>,
    conversation: ConversationId,
    verdict: Verdict,
    target: Target,
    disposal: Disposal,
    force: bool,
) -> Result<Settled, EngineError> {
    let checkout = Checkout::of(inner, conversation)?;
    let mut settler = Settler {
        inner,
        checkout: &checkout,
        disposal,
        force,
        files: 0,
        skipped: Vec::new(),
    };
    let pending = with_ledger(inner, &checkout, |ledger| {
        fold_drift(inner, &checkout, ledger)?;
        match &target {
            Target::All => {
                let paths: Vec<RelPath> = ledger.review.iter().map(|r| r.path.clone()).collect();
                for path in paths {
                    settler.file(ledger, &path, verdict)?;
                }
            }
            Target::File { path } => settler.file(ledger, path, verdict)?,
            Target::Turn { turn } => {
                let paths: Vec<RelPath> = ledger
                    .turn(*turn)
                    .ok_or_else(|| {
                        EngineError::Invalid(bisa_core::text!(
                            "error-engine-invalid-no-such-turn",
                            turn = turn.to_string()
                        ))
                    })?
                    .pending()
                    .map(|f| f.path.clone())
                    .collect();
                for path in paths {
                    settler.turn_file(ledger, *turn, &path, verdict)?;
                }
            }
            Target::Hunk {
                path,
                hunk,
                disk_hash,
            } => settler.hunk(ledger, path, hunk, disk_hash, verdict)?,
        }
        Ok((ledger.pending(), true))
    })?;
    let settled = Settled {
        files: settler.files,
        pending,
        skipped: settler.skipped,
    };
    announce(inner, conversation, verdict.as_str(), &settled);
    Ok(settled)
}

/// Go back to before the message that woke `turn`: every file that turn or
/// a later one touched returns to what it was. A file somebody else changed
/// since is merged; one that would conflict is left alone and named. The
/// messages stay — a conversation is a record, not a branch.
pub fn restore(
    inner: &Arc<Inner>,
    conversation: ConversationId,
    turn: TurnId,
    disposal: Disposal,
) -> Result<Settled, EngineError> {
    let checkout = Checkout::of(inner, conversation)?;
    let mut settler = Settler {
        inner,
        checkout: &checkout,
        disposal,
        force: true,
        files: 0,
        skipped: Vec::new(),
    };
    let pending = with_ledger(inner, &checkout, |ledger| {
        fold_drift(inner, &checkout, ledger)?;
        let plan = ledger.restore_plan(turn);
        if plan.is_empty() && ledger.turn(turn).is_none() {
            return Err(EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-no-such-turn-changed-nothing-past-turns",
                turn = turn.to_string()
            )));
        }
        let from = ledger
            .turns
            .iter()
            .position(|t| t.turn == turn)
            .unwrap_or(0);
        for target in plan {
            let disk = checkout.read(&target.path)?;
            let back_to = bytes_of(inner, conversation, &target.back_to)?;
            let untouched = super::same(&target.left, &disk);
            let bytes: Option<Vec<u8>> = if untouched || disk == back_to {
                back_to
            } else if target.opaque || disk.as_deref().is_some_and(is_opaque) {
                settler.skip(&target.path, MOVED_ON);
                continue;
            } else {
                let now = String::from_utf8_lossy(disk.as_deref().unwrap_or_default());
                let left = text_of(inner, conversation, &target.left)?;
                let back = text_of(inner, conversation, &target.back_to)?;
                let taken = bisa_core::changes::take_back(&now, &left, &back);
                if taken.conflicted {
                    settler.skip(&target.path, MOVED_ON);
                    continue;
                }
                Some(taken.text.into_bytes())
            };
            settler.write_back(&target.path, &disk, bytes.as_deref())?;
            settler.files += 1;
            for record in &mut ledger.turns[from..] {
                for file in &mut record.files {
                    if file.path == target.path {
                        file.state = ChangeState::Undone;
                    }
                }
            }
            // What an earlier turn left pending in the file still is.
            let base = ledger.under_review(&target.path).map(|r| r.base.clone());
            if let Some(base) = base {
                let base_text = text_of(inner, conversation, &base)?;
                let now_text = String::from_utf8_lossy(bytes.as_deref().unwrap_or_default());
                if FileDiff::of(&base_text, &now_text).is_empty() {
                    ledger.settled(&target.path, ChangeState::Undone);
                } else if let Some(review) = ledger.under_review_mut(&target.path) {
                    review.image = put(inner, conversation, bytes.as_deref())?;
                }
            }
        }
        Ok((ledger.pending(), true))
    })?;
    let settled = Settled {
        files: settler.files,
        pending,
        skipped: settler.skipped,
    };
    announce(inner, conversation, "restore", &settled);
    Ok(settled)
}
