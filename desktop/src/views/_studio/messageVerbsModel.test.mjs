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

test("a question about a message quotes its first words, so it is known which one", async () => {
  const { quotedWords } = await import("./messageVerbsModel.mjs");
  assert.equal(quotedWords({ content: "ship it" }), "ship it");
  assert.equal(quotedWords({ content: "\n  first line  \nsecond" }), "first line");
  const long = "the quick brown fox jumps over the lazy dog and then keeps going for a good while longer than fits";
  const quoted = quotedWords({ content: long });
  assert.ok(quoted.endsWith("…") && quoted.length <= 81 && long.startsWith(quoted.slice(0, -1)), quoted);
  assert.ok(!quoted.slice(0, -1).endsWith(" "), "cut at a word");
  assert.equal(quotedWords({ content: "x".repeat(200) }).length, 81, "one long word is cut where the room ends");
  assert.equal(quotedWords({ content: "   " }), null, "a message of files alone is named in general");
  assert.equal(quotedWords(null), null);
});
