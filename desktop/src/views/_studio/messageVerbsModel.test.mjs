/**
 * A message's verbs. Run with `node --test desktop/src/views/_studio/messageVerbsModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { VERBS, messageVerbs, verbWords } from "./messageVerbsModel.mjs";

test("every message offers reply, copy and copy link in that order; your own retract, last; a retracted one nothing", () => {
  assert.deepEqual(messageVerbs({ mine: false, retracted: false }), ["reply", "copy", "copy-link"]);
  assert.deepEqual(messageVerbs({ mine: true, retracted: false }), ["reply", "copy", "copy-link", "retract"]);
  assert.deepEqual(messageVerbs({ mine: true, retracted: true }), []);
  assert.deepEqual(messageVerbs({ mine: false, retracted: true }), []);
  assert.deepEqual([...VERBS], ["reply", "copy", "copy-link", "retract"]);
});

test("each verb has its words and glyph; retract alone is a danger set apart", () => {
  assert.deepEqual(verbWords("copy"), { label: "Copy", icon: "copy", danger: false, apart: false });
  assert.deepEqual(verbWords("copy-link"), { label: "Copy link", icon: "link", danger: false, apart: false });
  assert.equal(verbWords("reply").label, "Reply");
  assert.deepEqual(verbWords("retract"), { label: "Retract", icon: "delete", danger: true, apart: true });
  assert.throws(() => verbWords("edit"), /no such verb/);
});
