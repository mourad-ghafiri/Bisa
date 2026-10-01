/**
 * Leaving a held document: when it asks, what it says, and the key a hold
 * wears. Run with `node --test desktop/src/shell/leaveGuardModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";
import { holdKey, leaveDecision, leaveWords } from "./leaveGuardModel.mjs";

test("a departure asks only while something dirty is held", () => {
  assert.equal(leaveDecision(null), "go");
  assert.equal(leaveDecision({ dirty: () => false }), "go");
  assert.equal(leaveDecision({ dirty: () => true }), "ask");
});

test("the hold's key names the record once", () => {
  assert.equal(holdKey("note", "01N"), "note:01N");
  assert.equal(holdKey("drawing", "01D"), "drawing:01D");
});

test("the question names the record and says what Don't save costs, by kind", () => {
  const note = leaveWords("note", "Why the cache");
  assert.equal(note.title, "Save changes to “Why the cache”?");
  assert.match(note.description, /since the last save/);
  assert.match(note.note, /typed/);
  const drawing = leaveWords("drawing", "Orders");
  assert.equal(drawing.title, "Save changes to “Orders”?");
  assert.match(drawing.note, /drew/);
  assert.equal(leaveWords("note", "  ").title, "Save changes to “this note”?", "an untitled note is named by its kind");
  assert.equal(leaveWords("drawing", null).title, "Save changes to “this drawing”?");
});
