/**
 * A match over text nodes. Run with `node --test desktop/src/ui/find/domFindModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { locate, revealOffset, segmentsOf } from "./domFindModel.mjs";
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

test("a match in view moves nothing; one above or below is centred in its scrollport; one taller than the port is aligned to its top", () => {
  const port = { top: 100, bottom: 500 };
  assert.equal(revealOffset({ top: 200, bottom: 220 }, port, 1000), null, "already seen");
  assert.equal(revealOffset({ top: 100, bottom: 500 }, port, 1000), null, "filling the port exactly is seen");
  // Below: 600 px under the port's top, 20 tall; centred means 600 - (400 - 20) / 2 = 410 further.
  assert.equal(revealOffset({ top: 700, bottom: 720 }, port, 1000), 1410);
  // Above: 300 px over the port's top; centred means -300 - 190 = -490 back.
  assert.equal(revealOffset({ top: -200, bottom: -180 }, port, 1000), 510);
  assert.equal(revealOffset({ top: -200, bottom: -180 }, port, 100), 0, "never past the top");
  assert.equal(revealOffset({ top: 700, bottom: 1300 }, port, 1000), 1600, "taller than the port: its top at the port's top");
  assert.equal(revealOffset({ top: 50, bottom: 520 }, port, 1000), 950, "partly out both ways and taller: aligned to its top");
});
