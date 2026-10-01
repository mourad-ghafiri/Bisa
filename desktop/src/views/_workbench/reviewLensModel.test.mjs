import { strict as assert } from "node:assert";
import { test } from "node:test";
import {
  DEFAULT_LENS_LAYOUT,
  LENS_LAYOUTS,
  UNDO_DIRTY_HINT,
  fileActionWords,
  fileSettleTarget,
  hasNextHunk,
  hasPreviousHunk,
  hunkActionWords,
  hunkPositionWords,
  hunkSettleTarget,
  lensApplies,
  lensLayoutKey,
  lensLayoutWords,
  lensStorageKey,
  nextHunkIndex,
  pendingReviewWords,
  previousHunkIndex,
  reviewOpenDrafts,
  toggleLensLayout,
  undoEnabled,
} from "./reviewLensModel.mjs";
import { docModeKey } from "./fileDocModel.mjs";

function hunk(id, diskStart, diskLen = 2) {
  return { id, base: { start: diskStart, len: diskLen }, disk: { start: diskStart, len: diskLen }, removed: "", added: "" };
}

test("lensStorageKey mirrors docModeKey's shape", () => {
  assert.equal(lensStorageKey("workstream:w1", "src/a.ts"), "workstream:w1|review-lens|src/a.ts");
});

test("previous/next clamp at the ends rather than wrapping", () => {
  const hunks = [hunk("h1", 0), hunk("h2", 5), hunk("h3", 10)];
  assert.equal(hasPreviousHunk(hunks, 0), false);
  assert.equal(hasNextHunk(hunks, 2), false);
  assert.equal(previousHunkIndex(hunks, 0), 0);
  assert.equal(nextHunkIndex(hunks, 2), 2);
  assert.equal(nextHunkIndex(hunks, 0), 1);
  assert.equal(previousHunkIndex(hunks, 2), 1);
});

test("hunkPositionWords counts from one, and says so for no changes", () => {
  const hunks = [hunk("h1", 0), hunk("h2", 5)];
  assert.equal(hunkPositionWords(hunks, 0), "Change 1 of 2");
  assert.equal(hunkPositionWords(hunks, 1), "Change 2 of 2");
  assert.equal(hunkPositionWords([], 0), "No changes");
});

test("a hunk's Undo needs a clean buffer — the server undoes against the disk", () => {
  assert.equal(undoEnabled(false), true);
  assert.equal(undoEnabled(true), false);
  assert.match(UNDO_DIRTY_HINT, /save/i);
});

test("the words are Keep/Undo, never accept/reject", () => {
  assert.deepEqual(hunkActionWords(), { keep: "Keep", undo: "Undo" });
  const file = fileActionWords();
  assert.equal(file.keepFile, "Keep file");
  assert.equal(file.undoFile, "Undo file");
  assert.match(file.nextFile, /Next file/);
});

test("lensApplies is only true for a real file view", () => {
  assert.equal(lensApplies(null), false);
  assert.equal(lensApplies(undefined), false);
  assert.equal(lensApplies({ path: "a.ts" }), true);
});

test("settle targets carry the file's disk hash on a hunk, and just the path on a file", () => {
  assert.deepEqual(hunkSettleTarget("a.ts", { id: "h1" }, "sha:abc"), { grain: "hunk", path: "a.ts", hunk: "h1", disk_hash: "sha:abc" });
  assert.deepEqual(fileSettleTarget("a.ts"), { grain: "file", path: "a.ts" });
});

test("opening a file for review forces Source and the lens on — the document's own draft keys", () => {
  const drafts = reviewOpenDrafts("workstream:01W", "docs/guide.md");
  assert.equal(drafts.modeKey, docModeKey("workstream:01W", "docs/guide.md"));
  assert.equal(drafts.mode, "source");
  assert.equal(drafts.lensKey, lensStorageKey("workstream:01W", "docs/guide.md"));
  assert.equal(drafts.lensOn, true);
  assert.notEqual(drafts.modeKey, drafts.lensKey);
});

test("the lens is inline by default, toggles to side by side and back, and remembers per document", () => {
  assert.equal(DEFAULT_LENS_LAYOUT, "inline");
  assert.deepEqual([...LENS_LAYOUTS], ["inline", "side-by-side"]);
  assert.equal(toggleLensLayout("inline"), "side-by-side");
  assert.equal(toggleLensLayout("side-by-side"), "inline");
  assert.equal(lensLayoutWords("inline"), "Inline");
  assert.equal(lensLayoutWords("side-by-side"), "Side by side");
  assert.notEqual(lensLayoutKey("r", "a.ts"), lensLayoutKey("r", "b.ts"));
  assert.notEqual(lensLayoutKey("r", "a.ts"), lensStorageKey("r", "a.ts"), "the layout and the on/off are two drafts");
});

test("the bar counts the files still waiting, and a rendered document says its changes wait in Source", () => {
  assert.equal(fileActionWords().nextFile, "Next file to review");
  assert.equal(fileActionWords(2).nextFile, "Next file (2 left)");
  assert.equal(pendingReviewWords(1).text, "This file has 1 change to review.");
  assert.equal(pendingReviewWords(3).text, "This file has 3 changes to review.");
  assert.equal(pendingReviewWords(3).show, "Show diff");
});
