import assert from "node:assert/strict";
import { test } from "node:test";
import { GUARD_VERBS, guardWords } from "./closeGuardModel.mjs";

test("the answers are the platform's own words for this question", () => {
  assert.deepEqual(GUARD_VERBS, { cancel: "Cancel", discard: "Don't save", save: "Save" });
});

test("one document is named in the question; an untitled one is told it will be asked for a name", () => {
  const file = guardWords([{ label: "notes.md", untitled: false }]);
  assert.equal(file.title, "Save changes to notes.md?");
  assert.equal(file.description, "It has changes that were never saved.");
  assert.equal(file.note, "Not saving drops what you typed since the last save; nothing on disk changes.");

  const fresh = guardWords([{ label: "Untitled-1", untitled: true }]);
  assert.equal(fresh.title, "Save changes to Untitled-1?");
  assert.match(fresh.note, /It has no name yet — saving asks for one\.$/);
});

test("several documents are counted in the question and named under it", () => {
  const many = guardWords([
    { label: "a.ts", untitled: false },
    { label: "Untitled-2", untitled: true },
    { label: "b.ts", untitled: false },
  ]);
  assert.equal(many.title, "Save changes to 3 documents?");
  assert.equal(many.description, "a.ts, Untitled-2, b.ts have changes that were never saved.");
  assert.match(many.note, /^Not saving drops what you typed in them/);
  assert.match(many.note, /One has no name yet — saving asks for it\.$/);
  const two = guardWords([{ label: "Untitled-1", untitled: true }, { label: "Untitled-2", untitled: true }]);
  assert.match(two.note, /2 have no name yet — saving asks for each\.$/);
});
