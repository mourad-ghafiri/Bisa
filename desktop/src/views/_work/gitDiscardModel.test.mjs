/**
 * The words around a discard or a delete in Git › Changes. Run with
 * `node --test desktop/src/views/_work/gitDiscardModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { deleteCopy, deletedWords, discardCopy, discardedWords, recoveryWords, restoreWords, shortRef } from "./gitDiscardModel.mjs";

test("every recovery kind the vcs crate names has a Safety word, and restore says what comes back", () => {
  const src = readFileSync(new URL("../../../../crates/bisa-vcs/src/git.rs", import.meta.url), "utf8");
  const block = src.slice(src.indexOf("pub enum RecoveryKind {"), src.indexOf("}", src.indexOf("pub enum RecoveryKind {")));
  const kinds = [...block.matchAll(/^\s+([A-Z][a-z]+),$/gm)].map((m) => m[1].toLowerCase());
  assert.deepEqual(kinds, ["commit", "tree", "stash"], "the Rust enum is the vocabulary");
  for (const kind of kinds) assert.notEqual(recoveryWords(kind), kind, `${kind} has words of its own`);
  assert.equal(recoveryWords("stash"), "stash entry saved");
  assert.match(restoreWords({ ref_name: "refs/bisa/safety/1-stash_drop.stash", kind: "stash", branch: "main" }), /back on the stash list\. Nothing in the working tree moves/);
  assert.match(restoreWords({ ref_name: "refs/bisa/safety/1-checkout.wip", kind: "tree", branch: "main" }), /HEAD to main, then the saved index and working tree on top/);
  assert.match(restoreWords({ ref_name: "refs/bisa/safety/1-branch_delete", kind: "commit", branch: null }), /HEAD to the commit it was on\./);
});

test("a discard's confirmation names the file and keeps the index out of it in two sentences; the recovery promise is the dialog's SafetyNote, not a third", () => {
  const one = discardCopy(["src/a.rs"]);
  assert.equal(one.title, "Discard changes to src/a.rs?");
  assert.match(one.body, /goes back to what the index holds for this file/);
  assert.match(one.body, /A staged change is kept — unstage it first/);
  assert.ok(!one.body.includes("refs/bisa/safety/"), "said once, by SafetyNote");
  assert.ok(one.body.split(". ").length <= 2, "two sentences");
  assert.equal(one.confirm, "Discard");
  assert.equal(one.danger, true);

  const many = discardCopy(["a", "b", "c", "d", "e"]);
  assert.equal(many.title, "Discard changes to 5 files?");
  assert.match(many.body, /for a, b and 3 more\./);
  assert.equal(many.confirm, "Discard 5 files");
  assert.match(discardCopy(["a", "b", "c"]).body, /for a, b, c\./, "three names are all named");

  const folder = discardCopy(["src/views/a.mjs", "src/views/b.mjs"], { under: "src/views" });
  assert.equal(folder.title, "Discard changes under src/views/?");
  assert.match(folder.body, /for the 2 files under src\/views\/\. A staged change is kept/);
  assert.equal(folder.confirm, "Discard 2 files");
  assert.equal(discardCopy(["src/a.mjs"], { under: "src" }).body.split(". ")[0], "The working tree goes back to what the index holds for the 1 file under src/");
});

test("a delete's confirmation is the file tree's, for a git root, by disposal; a folder's names the folder and its untracked files", () => {
  const trash = deleteCopy(["notes.txt"], "trash");
  assert.equal(trash.title, "Delete notes.txt?");
  assert.match(trash.body, /moves to the Trash/);
  assert.equal(trash.confirm, "Move to Trash");
  assert.equal(trash.danger, false);
  const unlink = deleteCopy(["notes.txt"], "unlink");
  assert.match(unlink.body, /an untracked file has no copy anywhere/);
  assert.equal(unlink.confirm, "Delete");
  assert.equal(unlink.danger, true);
  assert.match(deleteCopy(["notes.txt"], null).body, /Checking how this root disposes/, "unknown yet: the dialog says so rather than guessing");
  const many = deleteCopy(["tmp/a.log", "tmp/b.log", "tmp/c.log"], "trash", { under: "tmp" });
  assert.equal(many.title, "Delete 3 files under tmp/?");
  assert.match(many.body, /a\.log, b\.log, c\.log/, "the files are named, as the tree names them");
  assert.equal(deleteCopy(["tmp/a.log", "tmp/b.log"], "unlink").title, "Delete 2 items?", "without a folder, the tree's own title");
  const every = deleteCopy(["tmp/a.log", "tmp/b.log", "new.txt"], "trash", { all: true });
  assert.equal(every.title, "Delete every untracked file (3)?", "the toolbar's delete says what it reaches");
  assert.match(every.body, /a\.log, b\.log, new\.txt/);
  assert.match(every.body, /moves to the Trash/);
  assert.equal(every.confirm, "Move to Trash");
  assert.equal(deleteCopy(["new.txt"], "unlink", { all: true }).title, "Delete the one untracked file, new.txt?");
  assert.equal(deleteCopy(["new.txt"], "unlink", { all: true }).danger, true);
});

test("the toasts name what happened and, for a discard, where the old text went", () => {
  assert.equal(discardedWords(["src/a.rs"], "refs/bisa/safety/discard-2026"), "Discarded changes to src/a.rs. What was there is saved as discard-2026.");
  assert.equal(discardedWords(["a", "b"], "x"), "Discarded changes to 2 files. What was there is saved as x.");
  assert.equal(shortRef("refs/bisa/safety/abc"), "abc");
  assert.equal(shortRef("other"), "other");
  assert.equal(shortRef(null), "");
  assert.equal(deletedWords(["notes.txt"], "trash"), "notes.txt moved to the Trash.");
  assert.equal(deletedWords(["notes.txt"], "unlink"), "notes.txt deleted.");
  assert.equal(deletedWords(["a", "b", "c"], "trash"), "3 files moved to the Trash.");
  assert.equal(deletedWords(["a", "b"], "unlink"), "2 files deleted.");
});
