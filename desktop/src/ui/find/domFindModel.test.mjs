/**
 * A match over text nodes. Run with `node --test desktop/src/ui/find/domFindModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { locate, segmentsOf } from "./domFindModel.mjs";
import { matchesOf, emptyFind } from "./findModel.mjs";

test("segments start where the previous text ended, and the whole is their join", () => {
  const s = segmentsOf(["Hello ", "wor", "ld"]);
  assert.deepEqual(s.starts, [0, 6, 9]);
  assert.equal(s.text, "Hello world");
  assert.deepEqual(segmentsOf([]), { starts: [], text: "" });
});

test("a match inside one node is one span; one across two nodes is two, each within its node", () => {
  const texts = ["Hello ", "wor", "ld"];
  const s = segmentsOf(texts);
  const [hello] = matchesOf(s.text, { ...emptyFind(), query: "Hello" });
  assert.deepEqual(locate(s, texts, hello), [{ node: 0, start: 0, end: 5 }]);
  const [world] = matchesOf(s.text, { ...emptyFind(), query: "world" });
  assert.deepEqual(locate(s, texts, world), [
    { node: 1, start: 0, end: 3 },
    { node: 2, start: 0, end: 2 },
  ]);
  assert.deepEqual(locate(s, texts, { start: 3, end: 3 }), [], "an empty match is no span");
});
